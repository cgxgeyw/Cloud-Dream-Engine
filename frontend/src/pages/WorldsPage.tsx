import { useEffect, useRef, useState, type ChangeEvent } from "react";
import { useNavigate } from "react-router-dom";
import {
  assetUrl,
  createWorldWithAi,
  onAiWorldCreateProgress,
  deleteAllWorlds,
  deleteWorld,
  downloadWorldPackage,
  duplicateWorld,
  fetchWorldCharacters,
  fetchWorlds,
  importWorldPackage,
  type CharacterResponse,
  type AiWorldCreateMode,
  type WorldResponse,
} from "../data/apiAdapter";
import { ConfirmDialog, ModalDialog } from "../components/ModalDialog";
import { useIsMobile } from "../components/ResponsiveLayout";
import { ScreenLayout } from "../components/ScreenLayout";
import { showToast } from "../components/Toast";
import { useT } from "../data/i18n/context";
import { X } from "lucide-react";

function normalizeAssetList(value: unknown): string[] {
  if (!Array.isArray(value)) {
    return [];
  }
  return value.map((item) => String(item).trim()).filter(Boolean);
}

// Tauri rejects commands with a plain string (the Rust Err(String)), not an
// Error instance, so `instanceof Error` misses it. Extract a usable message
// from whatever was thrown.
function errorMessage(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === "string") return err;
  if (err == null) return "";
  try {
    return JSON.stringify(err);
  } catch {
    return String(err);
  }
}

function resolveWorldDefaultBackgroundAsset(world: WorldResponse): string {
  const config =
    world.ui_theme_config && typeof world.ui_theme_config === "object"
      ? (world.ui_theme_config as Record<string, unknown>)
      : {};
  const openingScene = world.opening_scene.trim();
  const sceneBackgrounds = config.local_scene_backgrounds;
  if (openingScene && sceneBackgrounds && typeof sceneBackgrounds === "object") {
    const sceneAssets = normalizeAssetList((sceneBackgrounds as Record<string, unknown>)[openingScene]);
    if (sceneAssets.length > 0) {
      return sceneAssets[0];
    }
  }
  return normalizeAssetList(config.local_background_assets)[0] ?? "";
}

function countMapNodes(value: WorldResponse["map_nodes"]): number {
  let count = 0;
  function visit(node: unknown) {
    if (!node || typeof node !== "object" || Array.isArray(node)) {
      return;
    }
    count += 1;
    const children = (node as { children?: unknown }).children;
    if (Array.isArray(children)) {
      children.forEach(visit);
    }
  }

  const root = value.root ?? value.tree;
  if (root) {
    visit(root);
  } else if (Array.isArray(value.nodes)) {
    value.nodes.forEach(visit);
  }
  return count;
}

export function WorldsPage() {
  const isMobile = useIsMobile();
  const navigate = useNavigate();
  const t = useT();
  const [worlds, setWorlds] = useState<WorldResponse[]>([]);
  const [worldCharacters, setWorldCharacters] = useState<Record<string, CharacterResponse[]>>({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null);
  const [deletingAll, setDeletingAll] = useState(false);
  const [duplicating, setDuplicating] = useState<string | null>(null);
  const [exporting, setExporting] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);
  const [showAiCreateDialog, setShowAiCreateDialog] = useState(false);
  const [aiCreateMode, setAiCreateMode] = useState<AiWorldCreateMode>("single_agent");
  const [aiConcept, setAiConcept] = useState("");
  const [aiCreating, setAiCreating] = useState(false);
  const [aiError, setAiError] = useState<string | null>(null);
  const [aiProgressChars, setAiProgressChars] = useState(0);
  const [pendingDelete, setPendingDelete] = useState<WorldResponse | null>(null);
  const [showDeleteAllDialog, setShowDeleteAllDialog] = useState(false);
  const importInputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    let cancelled = false;

    async function loadData() {
      try {
        setLoading(true);
        setError(null);
        const worldData = await fetchWorlds();
        const characterEntries = await Promise.all(
          worldData.map(async (world) => [world.id, await fetchWorldCharacters(world.id)] as const),
        );
        if (!cancelled) {
          setWorlds(worldData);
          setWorldCharacters(Object.fromEntries(characterEntries));
        }
      } catch (loadError) {
        if (!cancelled) {
          setError(loadError instanceof Error ? loadError.message : t("worlds.loadFailed"));
        }
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    }

    void loadData();
    return () => {
      cancelled = true;
    };
  }, []);

  function getCharacterCount(worldId: string) {
    return worldCharacters[worldId]?.length ?? 0;
  }

  async function handleDelete(worldId: string) {
    try {
      setDeleting(worldId);
      setError(null);
      await deleteWorld(worldId);
      setWorlds((prev) => prev.filter((world) => world.id !== worldId));
      setWorldCharacters((prev) => {
        const next = { ...prev };
        delete next[worldId];
        return next;
      });
      setPendingDelete((current) => (current?.id === worldId ? null : current));
    } catch (deleteError) {
      setError(deleteError instanceof Error ? deleteError.message : t("worlds.deleteWorldFailed"));
    } finally {
      setDeleting(null);
    }
  }

  async function handleDuplicate(worldId: string) {
    try {
      setDuplicating(worldId);
      setError(null);
      const duplicated = await duplicateWorld(worldId);
      setWorlds((prev) => [duplicated, ...prev]);
      const duplicatedCharacters = await fetchWorldCharacters(duplicated.id);
      setWorldCharacters((prev) => ({
        ...prev,
        [duplicated.id]: duplicatedCharacters,
      }));
      showToast(t("worlds.duplicated"));
    } catch (duplicateError) {
      setError(duplicateError instanceof Error ? duplicateError.message : t("worlds.duplicateFailed"));
    } finally {
      setDuplicating(null);
    }
  }

  async function handleDeleteAll() {
    try {
      setDeletingAll(true);
      setError(null);
      await deleteAllWorlds();
      setWorlds([]);
      setWorldCharacters({});
      setPendingDelete(null);
      setShowDeleteAllDialog(false);
    } catch (deleteError) {
      setError(deleteError instanceof Error ? deleteError.message : t("worlds.deleteAllFailed"));
    } finally {
      setDeletingAll(false);
    }
  }

  async function handleExport(world: WorldResponse) {
    try {
      setExporting(world.id);
      setError(null);
      const { blob, filename, savedPath } = await downloadWorldPackage(world.id);
      if (blob) {
        const url = URL.createObjectURL(blob);
        const anchor = document.createElement("a");
        anchor.href = url;
        anchor.download = filename;
        document.body.append(anchor);
        anchor.click();
        anchor.remove();
        URL.revokeObjectURL(url);
        showToast(t("worlds.exported"));
      } else {
        showToast(t("worlds.exportedTo").replace("{path}", savedPath ?? filename));
      }
    } catch (exportError) {
      setError(exportError instanceof Error ? exportError.message : t("worlds.exportFailed"));
    } finally {
      setExporting(null);
    }
  }

  async function handleImport(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    if (!file) {
      return;
    }

    try {
      setImporting(true);
      setError(null);
      const importedWorld = await importWorldPackage(file);
      const importedCharacters = await fetchWorldCharacters(importedWorld.id);
      setWorlds((prev) => [importedWorld, ...prev]);
      setWorldCharacters((prev) => ({
        ...prev,
        [importedWorld.id]: importedCharacters,
      }));
      showToast(t("worlds.imported"));
    } catch (importError) {
      setError(importError instanceof Error ? importError.message : t("worlds.importFailed"));
    } finally {
      setImporting(false);
      event.target.value = "";
    }
  }

  async function handleAiCreate() {
    const concept = aiConcept.trim();
    if (!concept) {
      setAiError(t("worlds.conceptRequired"));
      return;
    }

    try {
      setAiCreating(true);
      setAiError(null);
      setAiProgressChars(0);
      const stopProgress = await onAiWorldCreateProgress((received) => {
        setAiProgressChars(received);
      });
      let created;
      try {
        created = await createWorldWithAi({
          mode: aiCreateMode,
          concept,
        });
      } finally {
        stopProgress();
      }
      setWorlds((prev) => [created.world, ...prev]);
      setWorldCharacters((prev) => ({
        ...prev,
        [created.world.id]: created.characters,
      }));
      setShowAiCreateDialog(false);
      setAiConcept("");
      showToast(t("worlds.aiCreated"));
    } catch (createError) {
      // Show the real backend message (e.g. token-truncation / provider 400),
      // not just the generic fallback.
      const detail = errorMessage(createError).trim();
      setAiError(detail ? `${t("worlds.aiCreateFailed")}：${detail}` : t("worlds.aiCreateFailed"));
    } finally {
      setAiCreating(false);
    }
  }

  return (
    <ScreenLayout
      title={isMobile ? "" : t("worlds.title")}
      subtitle={isMobile ? undefined : t("worlds.subtitle")}
      compactHeader
      maxWidth={980}
      toolbar={
        <div className="worlds-toolbar">
          <input
            ref={importInputRef}
            type="file"
            accept=".zip,application/zip"
            style={{ display: "none" }}
            onChange={(event) => void handleImport(event)}
          />
          <button type="button" onClick={() => navigate("/")} className="action-btn">
            {t("common.back")}
          </button>
          <button
            type="button"
            onClick={() => importInputRef.current?.click()}
            disabled={importing || aiCreating || deletingAll || Boolean(deleting) || Boolean(duplicating) || Boolean(exporting)}
            className="action-btn"
          >
            {importing ? t("worlds.importing") : t("worlds.import")}
          </button>
          <button
            type="button"
            onClick={() => setShowAiCreateDialog(true)}
            disabled={importing || aiCreating || deletingAll || Boolean(deleting) || Boolean(duplicating) || Boolean(exporting)}
            className="action-btn"
          >
            {aiCreating ? t("worlds.aiCreating") : t("worlds.aiCreate")}
          </button>
          {worlds.length > 0 ? (
            <button
              type="button"
              onClick={() => setShowDeleteAllDialog(true)}
              disabled={deletingAll || importing || aiCreating || Boolean(deleting) || Boolean(duplicating) || Boolean(exporting)}
              className="action-btn action-btn--danger"
            >
              {deletingAll ? t("worlds.deleting") : t("worlds.clearAll")}
            </button>
          ) : null}
          <button
            type="button"
            onClick={() => navigate("/worlds/new")}
            disabled={deletingAll || importing || aiCreating || Boolean(exporting)}
            className="action-btn action-btn--accent"
          >
            {t("worlds.newWorld")}
          </button>
        </div>
      }
    >
      {isMobile ? (
        <div className="worlds-mobile-header">
          <h1 className="worlds-mobile-title">{t("worlds.mobileTitle")}</h1>
          <div className="worlds-mobile-actions">
            <button
              type="button"
              onClick={() => navigate("/worlds/new")}
              disabled={deletingAll || importing || aiCreating || Boolean(exporting)}
              className="action-btn"
            >
              {t("worlds.mobileNewWorld")}
            </button>
            <button
              type="button"
              onClick={() => setShowAiCreateDialog(true)}
              disabled={deletingAll || importing || aiCreating || Boolean(deleting) || Boolean(duplicating) || Boolean(exporting)}
              className="action-btn"
            >
              {aiCreating ? t("worlds.aiCreating") : t("worlds.aiCreate")}
            </button>
            <button
              type="button"
              onClick={() => setShowDeleteAllDialog(true)}
              disabled={worlds.length === 0 || deletingAll || importing || aiCreating || Boolean(deleting) || Boolean(duplicating) || Boolean(exporting)}
              className="action-btn action-btn--danger"
            >
              {deletingAll ? t("worlds.deleting") : t("worlds.mobileClearAll")}
            </button>
          </div>
        </div>
      ) : null}

      {loading ? <div className="empty-text">{t("worlds.loading")}</div> : null}
      {error ? <div className="error-text">{t("worlds.loadError").replace("{error}", error)}</div> : null}

      {!loading && !error && worlds.length === 0 ? (
        <div className="empty-text">{t("worlds.empty")}</div>
      ) : null}

      {!loading && !error ? (
        <div className="card-grid">
          {worlds.map((world) => {
            const isDeleting = deleting === world.id;
            const isDuplicating = duplicating === world.id;
            const isExporting = exporting === world.id;
            const defaultBackgroundAsset = resolveWorldDefaultBackgroundAsset(world);
            const hasDefaultBackground = Boolean(defaultBackgroundAsset);

            return (
              <div
                key={world.id}
                className={`card-item${hasDefaultBackground ? " card-item--world-bg" : ""}`}
                style={
                  hasDefaultBackground
                    ? {
                        backgroundImage: `linear-gradient(180deg, rgba(10, 18, 30, 0.22) 0%, rgba(10, 18, 30, 0.78) 100%), url("${assetUrl(defaultBackgroundAsset)}")`,
                        backgroundPosition: "center",
                        backgroundSize: "cover",
                        backgroundRepeat: "no-repeat",
                      }
                    : undefined
                }
              >
                <button
                  type="button"
                  aria-label={t("worlds.deleteWorldAria").replace("{name}", world.name)}
                  title={t("worlds.deleteWorld")}
                  disabled={isDeleting || deletingAll || isExporting}
                  onClick={() => setPendingDelete(world)}
                  className="card-delete-btn"
                >
                  <X size={14} />
                </button>

                <div className="card-item-title">{world.name}</div>
                <div className="card-item-meta">
                  <span>{world.genre || t("worlds.uncategorized")}</span>
                  <span>{t("worlds.mapCount")} {countMapNodes(world.map_nodes)}</span>
                  <span>{t("worlds.characterCount")} {getCharacterCount(world.id)}</span>
                </div>

                <div className="card-item-actions world-card-actions">
                  <button
                    type="button"
                    className="card-action-btn"
                    disabled={deletingAll || isExporting}
                    onClick={() => navigate(`/new-game/setup/${world.id}`)}
                  >
                    {t("worlds.cardEnter")}
                  </button>
                  <button
                    type="button"
                    className="card-action-btn"
                    disabled={deletingAll || isExporting}
                    onClick={() => navigate(`/worlds/${world.id}/edit`)}
                  >
                    {t("worlds.cardEdit")}
                  </button>
                  <button
                    type="button"
                    className="card-action-btn"
                    disabled={deletingAll || isExporting}
                    onClick={() => navigate(`/worlds/${world.id}/characters`)}
                  >
                    {t("worlds.cardCharacters")}
                  </button>
                  <button
                    type="button"
                    className="card-action-btn card-action-btn--secondary"
                    disabled={deletingAll || importing || isDeleting || isDuplicating || isExporting}
                    onClick={() => void handleExport(world)}
                  >
                    {isExporting ? t("worlds.cardExporting") : t("worlds.cardExport")}
                  </button>
                  <button
                    type="button"
                    className="card-action-btn card-action-btn--secondary"
                    disabled={isDuplicating || deletingAll || isExporting}
                    onClick={() => void handleDuplicate(world.id)}
                  >
                    {isDuplicating ? t("worlds.cardDuplicating") : t("worlds.cardDuplicate")}
                  </button>
                  <button
                    type="button"
                    disabled={isDeleting || deletingAll || isExporting}
                    className="card-action-btn card-action-btn--secondary"
                    onClick={() => setPendingDelete(world)}
                  >
                    {isDeleting ? t("worlds.cardDeleting") : t("worlds.cardDelete")}
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      ) : null}

      <ModalDialog
        open={showAiCreateDialog}
        title={t("worlds.aiDialogTitle")}
        maxWidth={640}
        onClose={() => {
          if (!aiCreating) {
            setShowAiCreateDialog(false);
            setAiError(null);
          }
        }}
        footer={
          <>
            <button
              type="button"
              className="action-btn"
              disabled={aiCreating}
              onClick={() => { setShowAiCreateDialog(false); setAiError(null); }}
            >
              {t("common.cancel")}
            </button>
            <button
              type="button"
              className="action-btn action-btn--accent"
              disabled={aiCreating || !aiConcept.trim()}
              onClick={() => void handleAiCreate()}
            >
              {aiCreating ? t("worlds.creating") : t("worlds.startCreate")}
            </button>
          </>
        }
      >
        <div className="ai-world-create">
          <div className="ai-world-mode-group" role="group" aria-label={t("worlds.worldType")}>
            <button
              type="button"
              className={`ai-world-mode-btn${aiCreateMode === "single_agent" ? " is-active" : ""}`}
              onClick={() => setAiCreateMode("single_agent")}
              disabled={aiCreating}
            >
              {t("worlds.singleAgent")}
            </button>
            <button
              type="button"
              className={`ai-world-mode-btn${aiCreateMode === "multi_agent" ? " is-active" : ""}`}
              onClick={() => setAiCreateMode("multi_agent")}
              disabled={aiCreating}
            >
              {t("worlds.multiAgent")}
            </button>
          </div>
          <label className="ai-world-field">
            <span>{t("worlds.concept")}</span>
            <textarea
              value={aiConcept}
              onChange={(event) => setAiConcept(event.target.value)}
              disabled={aiCreating}
              rows={8}
              placeholder={t("worlds.conceptPlaceholder")}
            />
          </label>
          <p className="ai-world-hint">
            {t("worlds.aiHint")}
          </p>
          {aiCreating ? (
            <p className="ai-world-hint">
              {t("worlds.aiGenerating").replace("{n}", String(aiProgressChars))}
            </p>
          ) : null}
          {aiError ? (
            <p className="error-text" style={{ whiteSpace: "pre-wrap" }}>{aiError}</p>
          ) : null}
        </div>
      </ModalDialog>

      <ConfirmDialog
        open={Boolean(pendingDelete)}
        title={t("worlds.deleteWorld")}
        description={pendingDelete ? t("worlds.deleteConfirm").replace("{name}", pendingDelete.name) : ""}
        confirmLabel={deleting ? t("worlds.cardDeleting") : t("worlds.deleteWorldAction")}
        confirmVariant="danger"
        confirmDisabled={!pendingDelete || Boolean(deleting) || deletingAll || importing || Boolean(exporting)}
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
        title={t("worlds.deleteAllTitle")}
        description={t("worlds.deleteAllConfirm")}
        confirmLabel={deletingAll ? t("worlds.cardDeleting") : t("worlds.deleteAllAction")}
        confirmVariant="danger"
        confirmDisabled={deletingAll || importing || Boolean(deleting) || Boolean(duplicating) || Boolean(exporting)}
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
