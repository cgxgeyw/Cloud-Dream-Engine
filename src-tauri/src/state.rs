use std::{
    collections::HashSet,
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
