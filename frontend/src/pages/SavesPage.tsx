import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { branchSave, deleteAllSaves, deleteSave, fetchSaves, type SaveResponse } from "../data/apiAdapter";
import { ConfirmDialog } from "../components/ModalDialog";
import { useIsMobile } from "../components/ResponsiveLayout";
import { ScreenLayout, SurfacePanel } from "../components/ScreenLayout";
import { useT } from "../data/i18n/context";

export function SavesPage() {
  const isMobile = useIsMobile();
  const navigate = useNavigate();
  const t = useT();

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
          setError(loadError instanceof Error ? loadError.message : t("saves.loadFailed"));
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
      setError(deleteError instanceof Error ? deleteError.message : t("saves.deleteFailed"));
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
      setError(deleteError instanceof Error ? deleteError.message : t("saves.deleteAllFailed"));
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
      setError(branchError instanceof Error ? branchError.message : t("saves.branchFailed"));
    } finally {
      setBranching(null);
    }
  }

  return (
    <ScreenLayout
      title={t("saves.title")}
      subtitle={t("saves.subtitle")}
      compactHeader={isMobile}
      toolbar={
        <>
          {!isMobile ? (
            <button type="button" onClick={() => navigate("/")} className="action-btn">
              {t("saves.backHome")}
            </button>
          ) : null}
          {saves.length > 0 ? (
            <button
              type="button"
              onClick={() => setShowDeleteAllDialog(true)}
              disabled={deletingAll || Boolean(deleting)}
              className="action-btn action-btn--danger"
            >
              {deletingAll ? t("saves.deletingAll") : t("saves.deleteAll")}
            </button>
          ) : null}
        </>
      }
      maxWidth={980}
    >
      <div className="grid grid--gap-md">
        {isMobile ? (
          <div className="saves-mobile-header">
            <h1 className="saves-mobile-title">{t("saves.mobileTitle")}</h1>
            <button
              type="button"
              onClick={() => setShowDeleteAllDialog(true)}
              disabled={saves.length === 0 || deletingAll || Boolean(deleting)}
              className="action-btn action-btn--danger"
            >
              {deletingAll ? t("saves.deleting") : t("saves.deleteAll")}
            </button>
          </div>
        ) : null}

        {loading ? <SurfacePanel className="surface-panel--pad-lg">{t("saves.loading")}</SurfacePanel> : null}
        {error ? <SurfacePanel className="surface-panel--pad-lg error-text">{t("saves.loadError").replace("{error}", error)}</SurfacePanel> : null}

        {!loading && !error && saves.length === 0 ? (
          <SurfacePanel className="surface-panel--pad-lg empty-text">{t("saves.empty")}</SurfacePanel>
        ) : null}

        {!loading && !error
          ? saves.map((save) => (
              <SurfacePanel key={save.id} className="surface-panel--pad-lg save-row">
                  <div className="save-info">
                    <strong className="save-title">{save.title}</strong>
                    <div className="save-meta">
                      {save.world_name} / {save.progress} / {t("saves.lastSaved")} {formatTime(save.updated_at)}
                    </div>
                    {save.player_character_name ? (
                      <div className="save-meta">{t("saves.playerCharacter").replace("{name}", save.player_character_name)}</div>
                    ) : null}
                    <div className="save-meta">{t("saves.turnIndex").replace("{n}", String(save.turn_index))}</div>
                    <div className="save-summary">{save.summary}</div>
                  </div>

                  <div className="save-actions">
                    <button
                      type="button"
                      onClick={() => navigate(`/game/${save.session_id}`)}
                      className="action-btn action-btn--accent"
                    >
                      {t("saves.continue")}
                    </button>
                    <button
                      type="button"
                      disabled={branching === save.id || deletingAll}
                      onClick={() => void handleBranch(save)}
                      className="action-btn"
                    >
                      {branching === save.id ? t("saves.branching") : t("saves.branch")}
                    </button>
                    <button
                      type="button"
                      disabled={deleting === save.id || deletingAll}
                      onClick={() => setPendingDelete(save)}
                      className="action-btn action-btn--danger"
                      style={{ opacity: deleting === save.id || deletingAll ? 0.5 : 1 }}
                    >
                      {deleting === save.id ? t("saves.deleting") : t("saves.delete")}
                    </button>
                  </div>
              </SurfacePanel>
            ))
          : null}
      </div>

      <ConfirmDialog
        open={Boolean(pendingDelete)}
        title={t("saves.deleteTitle")}
        description={pendingDelete ? t("saves.deleteConfirm").replace("{title}", pendingDelete.title) : ""}
        confirmLabel={deleting ? t("saves.deleting") : t("saves.deleteAction")}
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
        title={t("saves.deleteAllTitle")}
        description={t("saves.deleteAllConfirm")}
        confirmLabel={deletingAll ? t("saves.deletingAll") : t("saves.deleteAll")}
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