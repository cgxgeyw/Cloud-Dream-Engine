use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex as StdMutex},
};
use tokio::sync::Mutex;

use crate::db::Database;
use crate::services::backend::BackendServices;

pub const SESSION_MUTATION_BUSY_ERROR_CODE: &str = "SESSION_MUTATION_IN_PROGRESS";

#[derive(Clone, Default)]
pub struct SessionMutationCoordinator {
    active_sessions: Arc<StdMutex<HashSet<String>>>,
}

impl SessionMutationCoordinator {
    pub fn try_acquire(&self, session_id: &str) -> Result<SessionMutationPermit, String> {
        let mut active_sessions = self
            .active_sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !active_sessions.insert(session_id.to_string()) {
            return Err(format!(
                "{SESSION_MUTATION_BUSY_ERROR_CODE}: session_id={session_id}"
            ));
        }

        Ok(SessionMutationPermit {
            active_sessions: Arc::clone(&self.active_sessions),
            session_id: session_id.to_string(),
        })
    }
}

#[must_use]
pub struct SessionMutationPermit {
    active_sessions: Arc<StdMutex<HashSet<String>>>,
    session_id: String,
}

impl Drop for SessionMutationPermit {
    fn drop(&mut self) {
        let mut active_sessions = self
            .active_sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        active_sessions.remove(&self.session_id);
    }
}

pub struct AppState {
    pub db: Mutex<Database>,
    pub services: BackendServices,
    pub data_dir: PathBuf,
    pub session_mutations: SessionMutationCoordinator,
    pub knowledge_rebuild: KnowledgeRebuildTracker,
}

/// 知识库向量索引重建任务的进度快照。任务在后台线程跑，前端通过事件 + 轮询
/// 拿到这里的快照来画进度条 / 任务栏进度，页面切走也不会中断。
#[derive(Clone, Debug, serde::Serialize)]
pub struct KnowledgeRebuildSnapshot {
    pub job_id: String,
    pub base_id: String,
    pub base_name: String,
    pub total: usize,
    pub done: usize,
    pub failed: usize,
    /// "running" | "completed" | "failed"
    pub status: String,
    pub error: Option<String>,
    pub started_at_ms: i64,
}

/// 进程内所有知识库重建任务的注册表（桌面/安卓共用）。
#[derive(Clone, Default)]
pub struct KnowledgeRebuildTracker {
    jobs: Arc<StdMutex<HashMap<String, KnowledgeRebuildSnapshot>>>,
}

impl KnowledgeRebuildTracker {
    fn lock_jobs(&self) -> std::sync::MutexGuard<'_, HashMap<String, KnowledgeRebuildSnapshot>> {
        self.jobs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// 为某个知识库启动新任务；若该库已有 running 任务则复用并返回 `(job_id, false)`。
    pub fn start(&self, job_id: &str, base_id: &str, base_name: &str) -> (String, bool) {
        let mut jobs = self.lock_jobs();
        // 清理 10 分钟前已结束的旧任务，避免长时间运行后快照无限堆积。
        let cutoff = chrono::Utc::now().timestamp_millis() - 10 * 60 * 1000;
        jobs.retain(|_, job| job.status == "running" || job.started_at_ms >= cutoff);
        if let Some(existing) = jobs
            .values()
            .find(|job| job.base_id == base_id && job.status == "running")
        {
            return (existing.job_id.clone(), false);
        }
        jobs.insert(
            job_id.to_string(),
            KnowledgeRebuildSnapshot {
                job_id: job_id.to_string(),
                base_id: base_id.to_string(),
                base_name: base_name.to_string(),
                total: 0,
                done: 0,
                failed: 0,
                status: "running".to_string(),
                error: None,
                started_at_ms: chrono::Utc::now().timestamp_millis(),
            },
        );
        (job_id.to_string(), true)
    }

    pub fn set_total(&self, job_id: &str, total: usize) -> Option<KnowledgeRebuildSnapshot> {
        let mut jobs = self.lock_jobs();
        let job = jobs.get_mut(job_id)?;
        job.total = total;
        Some(job.clone())
    }

    pub fn advance(
        &self,
        job_id: &str,
        processed: usize,
        failed: usize,
    ) -> Option<KnowledgeRebuildSnapshot> {
        let mut jobs = self.lock_jobs();
        let job = jobs.get_mut(job_id)?;
        job.done = job.done.saturating_add(processed);
        job.failed += failed;
        Some(job.clone())
    }

    pub fn finish(&self, job_id: &str, error: Option<String>) -> Option<KnowledgeRebuildSnapshot> {
        let mut jobs = self.lock_jobs();
        let job = jobs.get_mut(job_id)?;
        job.status = if error.is_some() {
            "failed".to_string()
        } else {
            "completed".to_string()
        };
        job.error = error;
        Some(job.clone())
    }

    pub fn snapshot(&self, job_id: &str) -> Option<KnowledgeRebuildSnapshot> {
        self.lock_jobs().get(job_id).cloned()
    }

    /// 所有任务快照（含已完成），用于页面重挂载 / 应用重启后恢复展示。
    pub fn list(&self) -> Vec<KnowledgeRebuildSnapshot> {
        let mut jobs = self.lock_jobs().values().cloned().collect::<Vec<_>>();
        jobs.sort_by_key(|job| job.started_at_ms);
        jobs
    }

    /// 某知识库最近一次任务（含已完成的），用于页面重新挂载时恢复进度显示。
    pub fn latest_for_base(&self, base_id: &str) -> Option<KnowledgeRebuildSnapshot> {
        self.lock_jobs()
            .values()
            .filter(|job| job.base_id == base_id)
            .max_by_key(|job| job.started_at_ms)
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::{SessionMutationCoordinator, SESSION_MUTATION_BUSY_ERROR_CODE};

    #[test]
    fn same_session_is_rejected_until_permit_is_dropped() {
        let coordinator = SessionMutationCoordinator::default();
        let first_permit = coordinator
            .try_acquire("session-a")
            .expect("first mutation should acquire the session");

        let error = match coordinator.try_acquire("session-a") {
            Ok(_) => panic!("second mutation for the same session must be rejected"),
            Err(error) => error,
        };
        assert_eq!(
            error,
            format!("{SESSION_MUTATION_BUSY_ERROR_CODE}: session_id=session-a")
        );

        drop(first_permit);
        assert!(coordinator.try_acquire("session-a").is_ok());
    }

    #[test]
    fn different_sessions_can_mutate_concurrently() {
        let coordinator = SessionMutationCoordinator::default();
        let _first_permit = coordinator
            .try_acquire("session-a")
            .expect("first session should acquire");
        let _second_permit = coordinator
            .try_acquire("session-b")
            .expect("different session should acquire independently");
    }
}
