import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { branchSave, deleteAllSaves, deleteSave, fetchSaves, type SaveResponse } from "../data/apiAdapter";
import { ConfirmDialog } from "../components/ModalDialog";
import { useIsMobile } from "../components/ResponsiveLayout";
import { ScreenLayout, SurfacePanel } from "../components/ScreenLayout";

export function SavesPage() {
  const isMobile = useIsMobile();
  const navigate = useNavigate();

  function formatTime(iso: string): string {
    try {
      const d = new Date(iso);
      if (isNaN(d.getTime())) return iso;
      const pad = (n: number) => String(n).padStart(2, "0");
      return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
    } catch {
      return iso;
    }
  }
  const [saves, setSaves] = useState<SaveResponse[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null);
  const [deletingAll, setDeletingAll] = useState(false);
  const [branching, setBranching] = useState<string | null>(null);
  const [pendingDelete, setPendingDelete] = useState<SaveResponse | null>(null);
  const [showDeleteAllDialog, setShowDeleteAllDialog] = useState(false);

  useEffect(() => {
    let cancelled = false;

    async function loadSaves() {
      try {
        setLoading(true);
        setError(null);
        const data = await fetchSaves();
        if (!cancelled) {
          setSaves(data);
        }
      } catch (loadError) {
        if (!cancelled) {
          setError(loadError instanceof Error ? loadError.message : "加载存档失败");
        }
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    }

    void loadSaves();

    return () => {
      cancelled = true;
    };
  }, []);

  async function handleDelete(saveId: string) {
    try {
      setError(null);
      setDeleting(saveId);
      await deleteSave(saveId);
      setSaves((prev) => prev.filter((s) => s.id !== saveId));
      setPendingDelete((current) => (current?.id === saveId ? null : current));
    } catch (deleteError) {
      setError(deleteError instanceof Error ? deleteError.message : "删除存档失败");
    } finally {
      setDeleting(null);
    }
  }

  async function handleDeleteAll() {
    try {
      setError(null);
      setDeletingAll(true);
      await deleteAllSaves();
      setSaves([]);
      setPendingDelete(null);
      setShowDeleteAllDialog(false);
    } catch (deleteError) {
      setError(deleteError instanceof Error ? deleteError.message : "删除全部存档失败");
    } finally {
      setDeletingAll(false);
    }
  }

  async function handleBranch(save: SaveResponse) {
    try {
      setBranching(save.id);
      setError(null);
      const branched = await branchSave(save.id);
      setSaves((prev) => [branched, ...prev]);
    } catch (branchError) {
      setError(branchError instanceof Error ? branchError.message : "创建分支失败");
    } finally {
      setBranching(null);
    }
  }

  return (
    <ScreenLayout
      title="载入游戏"
      subtitle="选择一个存档继续游戏，或从现有存档创建分支。"
      compactHeader={isMobile}
      toolbar={
        <>
          {!isMobile ? (
            <button type="button" onClick={() => navigate("/")} className="action-btn">
              返回首页
            </button>
          ) : null}
          {saves.length > 0 ? (
            <button
              type="button"
              onClick={() => setShowDeleteAllDialog(true)}
              disabled={deletingAll || Boolean(deleting)}
              className="action-btn action-btn--danger"
            >
              {deletingAll ? "删除全部中..." : "删除全部存档"}
            </button>
          ) : null}
        </>
      }
      maxWidth={980}
    >
      <div className="grid grid--gap-md">
        {isMobile ? (
          <div className="saves-mobile-header">
            <h1 className="saves-mobile-title">{"\u5b58\u6863\u5217\u8868"}</h1>
            <button
              type="button"
              onClick={() => setShowDeleteAllDialog(true)}
              disabled={saves.length === 0 || deletingAll || Boolean(deleting)}
              className="action-btn action-btn--danger"
            >
              {deletingAll ? "\u5220\u9664\u4e2d..." : "\u5220\u9664\u5168\u90e8\u5b58\u6863"}
            </button>
          </div>
        ) : null}

        {loading ? <SurfacePanel className="surface-panel--pad-lg">正在加载存档列表...</SurfacePanel> : null}
        {error ? <SurfacePanel className="surface-panel--pad-lg error-text">加载失败：{error}</SurfacePanel> : null}

        {!loading && !error && saves.length === 0 ? (
          <SurfacePanel className="surface-panel--pad-lg empty-text">暂无存档。</SurfacePanel>
        ) : null}

        {!loading && !error
          ? saves.map((save) => (
              <SurfacePanel key={save.id} className="surface-panel--pad-lg save-row">
                  <div className="save-info">
                    <strong className="save-title">{save.title}</strong>
                    <div className="save-meta">
                      {save.world_name} / {save.progress} / 最后保存 {formatTime(save.updated_at)}
                    </div>
                    {save.player_character_name ? (
                      <div className="save-meta">玩家角色：{save.player_character_name}</div>
                    ) : null}
                    <div className="save-meta">第 {save.turn_index} 回合</div>
                    <div className="save-summary">{save.summary}</div>
                  </div>

                  <div className="save-actions">
                    <button
                      type="button"
                      onClick={() => navigate(`/game/${save.session_id}`)}
                      className="action-btn action-btn--accent"
                    >
                      继续游戏
                    </button>
                    <button
                      type="button"
                      disabled={branching === save.id || deletingAll}
                      onClick={() => void handleBranch(save)}
                      className="action-btn"
                    >
                      {branching === save.id ? "分支中..." : "分支"}
                    </button>
                    <button
                      type="button"
                      disabled={deleting === save.id || deletingAll}
                      onClick={() => setPendingDelete(save)}
                      className="action-btn action-btn--danger"
                      style={{ opacity: deleting === save.id || deletingAll ? 0.5 : 1 }}
                    >
                      {deleting === save.id ? "删除中..." : "删除"}
                    </button>
                  </div>
              </SurfacePanel>
            ))
          : null}
      </div>

      <ConfirmDialog
        open={Boolean(pendingDelete)}
        title="删除存档"
        description={pendingDelete ? `确定要删除存档「${pendingDelete.title}」吗？此操作不可撤销。` : ""}
        confirmLabel={deleting ? "删除中..." : "删除存档"}
        confirmVariant="danger"
        confirmDisabled={!pendingDelete || Boolean(deleting) || deletingAll}
        onClose={() => {
          if (!deleting && !deletingAll) {
            setPendingDelete(null);
          }
        }}
        onConfirm={() => {
          if (!pendingDelete) {
            return;
          }
          void handleDelete(pendingDelete.id);
        }}
      />

      <ConfirmDialog
        open={showDeleteAllDialog}
        title="删除全部存档"
        description="确定要删除当前所有存档吗？此操作会清空全部会话与对应记忆，且不可撤销。"
        confirmLabel={deletingAll ? "删除全部中..." : "删除全部存档"}
        confirmVariant="danger"
        confirmDisabled={deletingAll || Boolean(deleting)}
        onClose={() => {
          if (!deletingAll) {
            setShowDeleteAllDialog(false);
          }
        }}
        onConfirm={() => {
          void handleDeleteAll();
        }}
      />
    </ScreenLayout>
  );
}