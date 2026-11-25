import { useEffect, useMemo, useState } from 'react';
import {
  listKnowledgeRebuildJobs,
  onKnowledgeRebuildProgress,
  isTauriEnvironment,
  isDesktopRuntime,
} from '../data/apiAdapter';
import type { KnowledgeRebuildSnapshot } from '../data/types';
import './KnowledgeRebuildProgress.css';

const AUTO_DISMISS_MS = 4000;
const RECENT_WINDOW_MS = 120_000;

/**
 * 知识库向量索引重建的全局进度浮层
 *
 * - 重建在 Rust 后台任务里跑，本组件只订阅 `knowledge-rebuild:progress` 事件 +
 *   挂载时拉一次任务列表；页面切走再回来、甚至前端刷新后都能恢复显示。
 * - 桌面端把总进度写进系统任务栏（`setProgressBar`）；安卓/网页只显示浮层。
 * - 完成后短暂显示"完成"再自动收起，失败标红保留直到手动关闭。
 */
export function KnowledgeRebuildProgress() {
  const [jobs, setJobs] = useState<Record<string, KnowledgeRebuildSnapshot>>({});
  const [dismissed, setDismissed] = useState<Set<string>>(() => new Set());

  useEffect(() => {
    if (!isTauriEnvironment()) return;
    let active = true;
    void listKnowledgeRebuildJobs()
      .then((list) => {
        if (!active) return;
        setJobs((prev) => {
          const next = { ...prev };
          for (const job of list) {
            if (!next[job.job_id]) next[job.job_id] = job;
          }
          return next;
        });
      })
      .catch(() => {});
    const unsubscribePromise = onKnowledgeRebuildProgress((snapshot) => {
      setJobs((prev) => ({ ...prev, [snapshot.job_id]: snapshot }));
    });
    let unsubscribe: (() => void) | null = null;
    void unsubscribePromise.then((fn) => {
      unsubscribe = fn;
    });
    return () => {
      active = false;
      if (unsubscribe) unsubscribe();
      else void unsubscribePromise.then((fn) => fn());
    };
  }, []);

  const running = useMemo(
    () => Object.values(jobs).filter((job) => job.status === 'running'),
    [jobs],
  );
  const finished = useMemo(
    () =>
      Object.values(jobs).filter(
        (job) =>
          job.status !== 'running' &&
          !dismissed.has(job.job_id) &&
          Date.now() - job.started_at_ms < RECENT_WINDOW_MS,
      ),
    [jobs, dismissed],
  );

  // 任务栏进度（仅桌面）：把所有运行中任务汇总成一个百分比。
  const runningSignature = running
    .map((job) => `${job.job_id}:${job.done}:${job.total}`)
    .join('|');
  useEffect(() => {
    if (!isDesktopRuntime()) return;
    let cancelled = false;
    void (async () => {
      try {
        const { getCurrentWindow, ProgressBarStatus } = await import('@tauri-apps/api/window');
        if (cancelled) return;
        const win = getCurrentWindow();
        if (running.length > 0) {
          const total = running.reduce((sum, job) => sum + Math.max(job.total, 1), 0);
          const done = running.reduce(
            (sum, job) => sum + Math.min(job.done, Math.max(job.total, 1)),
            0,
          );
          await win.setProgressBar({
            status: ProgressBarStatus.Normal,
            progress: Math.max(0, Math.min(100, Math.round((done / total) * 100))),
          });
        } else {
          await win.setProgressBar({ status: ProgressBarStatus.None });
        }
      } catch {
        // 非桌面环境或权限未开时静默忽略。
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [runningSignature]);

  // 完成的任务 4 秒后自动收起。
  const finishedSignature = finished.map((job) => job.job_id).join('|');
  useEffect(() => {
    const completed = finished.filter((job) => job.status === 'completed');
    if (completed.length === 0) return;
    const timer = window.setTimeout(() => {
      setDismissed((prev) => {
        const next = new Set(prev);
        for (const job of completed) next.add(job.job_id);
        return next;
      });
    }, AUTO_DISMISS_MS);
    return () => window.clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [finishedSignature]);

  const visible = [...running, ...finished];
  if (visible.length === 0) return null;

  return (
    <div className="knowledge-rebuild-stack" aria-live="polite">
      {visible.map((job) => {
        const preparing = job.total === 0 && job.status === 'running';
        const total = Math.max(job.total, 1);
        const percent = preparing ? 0 : Math.min(100, Math.round((job.done / total) * 100));
        const done = job.status !== 'running';
        const failed = job.status === 'failed';
        return (
          <div
            key={job.job_id}
            className={`knowledge-rebuild-card${done ? ' knowledge-rebuild-card--done' : ''}${failed ? ' knowledge-rebuild-card--failed' : ''}`}
          >
            <div className="knowledge-rebuild-head">
              <span className="knowledge-rebuild-title">
                {failed ? '索引重建失败' : done ? '索引重建完成' : '正在重建索引'}
              </span>
              <span className="knowledge-rebuild-percent">
                {preparing ? '准备中' : `${percent}%`}
              </span>
            </div>
            <div className="knowledge-rebuild-sub">{job.base_name}</div>
            <div className="knowledge-rebuild-track">
              <div
                className={`knowledge-rebuild-fill${preparing ? ' knowledge-rebuild-fill--preparing' : ''}`}
                style={{ width: preparing ? '100%' : `${percent}%` }}
              />
            </div>
            <div className="knowledge-rebuild-meta">
              {failed
                ? (job.error ?? '向量模型调用失败')
                : preparing
                  ? '正在读取资料并切片…'
                  : done
                    ? `已处理 ${job.done} / ${job.total} 片段${job.failed > 0 ? `，失败 ${job.failed}` : ''}`
                    : `已处理 ${job.done} / ${job.total} 片段 · 可切到其他页面，任务在后台继续`}
            </div>
            {done ? (
              <button
                type="button"
                className="knowledge-rebuild-close"
                onClick={() => setDismissed((prev) => new Set(prev).add(job.job_id))}
                aria-label="关闭"
              >
                ×
              </button>
            ) : null}
          </div>
        );
      })}
    </div>
  );
}
