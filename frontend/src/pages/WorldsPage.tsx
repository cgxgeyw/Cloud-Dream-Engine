import { useEffect, useRef, useState, type ChangeEvent } from "react";
import { useNavigate } from "react-router-dom";
import {
  assetUrl,
  deleteAllWorlds,
  deleteWorld,
  downloadWorldPackage,
  duplicateWorld,
  fetchWorldCharacters,
  fetchWorlds,
  importWorldPackage,
  type CharacterResponse,
  type WorldResponse,
} from "../data/apiAdapter";
import { ConfirmDialog } from "../components/ModalDialog";
import { useIsMobile } from "../components/ResponsiveLayout";
import { ScreenLayout } from "../components/ScreenLayout";
import { showToast } from "../components/Toast";
import { X } from "lucide-react";

function normalizeAssetList(value: unknown): string[] {
  if (!Array.isArray(value)) {
    return [];
  }
  return value.map((item) => String(item).trim()).filter(Boolean);
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
  const [worlds, setWorlds] = useState<WorldResponse[]>([]);
  const [worldCharacters, setWorldCharacters] = useState<Record<string, CharacterResponse[]>>({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null);
  const [deletingAll, setDeletingAll] = useState(false);
  const [duplicating, setDuplicating] = useState<string | null>(null);
  const [exporting, setExporting] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);
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
          setError(loadError instanceof Error ? loadError.message : "加载世界列表失败");
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
      setError(deleteError instanceof Error ? deleteError.message : "删除世界失败");
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
      showToast("世界已复制");
    } catch (duplicateError) {
      setError(duplicateError instanceof Error ? duplicateError.message : "复制世界失败");
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
      setError(deleteError instanceof Error ? deleteError.message : "删除全部世界失败");
    } finally {
      setDeletingAll(false);
    }
  }

  async function handleExport(world: WorldResponse) {
    try {
      setExporting(world.id);
      setError(null);
      const { blob, filename } = await downloadWorldPackage(world.id);
      if (!blob) {
        throw new Error("世界包导出失败：未收到文件内容。");
      }
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = filename;
      document.body.append(anchor);
      anchor.click();
      anchor.remove();
      URL.revokeObjectURL(url);
      showToast("世界包已导出");
    } catch (exportError) {
      setError(exportError instanceof Error ? exportError.message : "导出世界包失败");
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
      showToast("世界包已导入");
    } catch (importError) {
      setError(importError instanceof Error ? importError.message : "导入世界包失败");
    } finally {
      setImporting(false);
      event.target.value = "";
    }
  }

  return (
    <ScreenLayout
      title={isMobile ? "" : "\u4e16\u754c\u5de5\u574a"}
      subtitle={isMobile ? undefined : "\u521b\u5efa\u3001\u7f16\u8f91\u3001\u5bfc\u5165\u548c\u5bfc\u51fa\u4e16\u754c\u5305\u3002"}
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
          <button type="button" onClick={() => navigate(-1)} className="action-btn">
            返回首页
          </button>
          <button
            type="button"
            onClick={() => importInputRef.current?.click()}
            disabled={importing || deletingAll || Boolean(deleting) || Boolean(duplicating) || Boolean(exporting)}
            className="action-btn"
          >
            {importing ? "导入中..." : "导入"}
          </button>
          {worlds.length > 0 ? (
            <button
              type="button"
              onClick={() => setShowDeleteAllDialog(true)}
              disabled={deletingAll || importing || Boolean(deleting) || Boolean(duplicating) || Boolean(exporting)}
              className="action-btn action-btn--danger"
            >
              {deletingAll ? "删除中..." : "清空全部"}
            </button>
          ) : null}
          <button
            type="button"
            onClick={() => navigate("/worlds/new")}
            disabled={deletingAll || importing || Boolean(exporting)}
            className="action-btn action-btn--accent"
          >
            + 新建世界
          </button>
        </div>
      }
    >
      {isMobile ? (
        <div className="worlds-mobile-header">
          <h1 className="worlds-mobile-title">{"\u4e16\u754c\u5217\u8868"}</h1>
          <div className="worlds-mobile-actions">
            <button
              type="button"
              onClick={() => navigate("/worlds/new")}
              disabled={deletingAll || importing || Boolean(exporting)}
              className="action-btn"
            >
              {"\u65b0\u589e\u4e16\u754c"}
            </button>
            <button
              type="button"
              onClick={() => setShowDeleteAllDialog(true)}
              disabled={worlds.length === 0 || deletingAll || importing || Boolean(deleting) || Boolean(duplicating) || Boolean(exporting)}
              className="action-btn action-btn--danger"
            >
              {deletingAll ? "\u5220\u9664\u4e2d..." : "\u5220\u9664\u5168\u90e8\u4e16\u754c"}
            </button>
          </div>
        </div>
      ) : null}

      {loading ? <div className="empty-text">正在加载世界列表...</div> : null}
      {error ? <div className="error-text">加载失败：{error}</div> : null}

      {!loading && !error && worlds.length === 0 ? (
        <div className="empty-text">暂无世界。</div>
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
                  aria-label={`删除世界 ${world.name}`}
                  title="删除世界"
                  disabled={isDeleting || deletingAll || isExporting}
                  onClick={() => setPendingDelete(world)}
                  className="card-delete-btn"
                >
                  <X size={14} />
                </button>

                <div className="card-item-title">{world.name}</div>
                <div className="card-item-meta">
                  <span>{world.genre || "未分类"}</span>
                  <span>地图 {countMapNodes(world.map_nodes)}</span>
                  <span>角色 {getCharacterCount(world.id)}</span>
                </div>

                <div className="card-item-actions world-card-actions">
                  <button
                    type="button"
                    className="card-action-btn"
                    disabled={deletingAll || isExporting}
                    onClick={() => navigate(`/new-game/setup/${world.id}`)}
                  >
                    进入
                  </button>
                  <button
                    type="button"
                    className="card-action-btn"
                    disabled={deletingAll || isExporting}
                    onClick={() => navigate(`/worlds/${world.id}/edit`)}
                  >
                    编辑
                  </button>
                  <button
                    type="button"
                    className="card-action-btn"
                    disabled={deletingAll || isExporting}
                    onClick={() => navigate(`/worlds/${world.id}/characters`)}
                  >
                    角色池
                  </button>
                  <button
                    type="button"
                    className="card-action-btn card-action-btn--secondary"
                    disabled={deletingAll || importing || isDeleting || isDuplicating || isExporting}
                    onClick={() => void handleExport(world)}
                  >
                    {isExporting ? "导出中..." : "导出"}
                  </button>
                  <button
                    type="button"
                    className="card-action-btn card-action-btn--secondary"
                    disabled={isDuplicating || deletingAll || isExporting}
                    onClick={() => void handleDuplicate(world.id)}
                  >
                    {isDuplicating ? "复制中..." : "复制"}
                  </button>
                  <button
                    type="button"
                    disabled={isDeleting || deletingAll || isExporting}
                    className="card-action-btn card-action-btn--secondary"
                    onClick={() => setPendingDelete(world)}
                  >
                    {isDeleting ? "删除中..." : "删除"}
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      ) : null}

      <ConfirmDialog
        open={Boolean(pendingDelete)}
        title="删除世界"
        description={pendingDelete ? `确定要删除「${pendingDelete.name}」吗？此操作不可撤销。` : ""}
        confirmLabel={deleting ? "删除中..." : "删除世界"}
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
        title="删除全部世界"
        description="确定删除当前全部世界吗？此操作不可撤销。"
        confirmLabel={deletingAll ? "删除中..." : "删除全部世界"}
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
