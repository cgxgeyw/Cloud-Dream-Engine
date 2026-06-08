import { useEffect, useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import {
  assetUrl,
  createCharacterInWorldFromCharacter,
  deleteWorldCharacter,
  exportWorldCharacterTemplate,
  fetchModels,
  fetchSettings,
  fetchWorld,
  fetchWorldCharacters,
  fetchWorlds,
  type CharacterResponse,
  type ModelConfigResponse,
  type SettingsResponse,
  type WorldResponse,
} from "../data/apiAdapter";
import { ConfirmDialog, ModalDialog } from "../components/ModalDialog";
import { ScreenLayout, SurfacePanel } from "../components/ScreenLayout";
import { X, Users } from "lucide-react";

type CreateMode = "same_world" | "other_world";

export function WorldCharactersPage() {
  const navigate = useNavigate();
  const { worldId } = useParams();
  const [world, setWorld] = useState<WorldResponse | null>(null);
  const [worldOptions, setWorldOptions] = useState<WorldResponse[]>([]);
  const [characters, setCharacters] = useState<CharacterResponse[]>([]);
  const [textModels, setTextModels] = useState<ModelConfigResponse[]>([]);
  const [settings, setSettings] = useState<SettingsResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null);
  const [characterPendingDelete, setCharacterPendingDelete] = useState<CharacterResponse | null>(null);
  const [creatingFromCharacter, setCreatingFromCharacter] = useState<CharacterResponse | null>(null);
  const [createMode, setCreateMode] = useState<CreateMode>("same_world");
  const [targetWorldId, setTargetWorldId] = useState("");
  const [newCharacterName, setNewCharacterName] = useState("");
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    if (!worldId) {
      setLoading(false);
      setError("缺少世界 ID");
      return;
    }

    const stableWorldId = worldId;
    let cancelled = false;

    async function loadData() {
      try {
        setLoading(true);
        setError(null);
        const [worldData, characterData, availableWorlds, textModelData, settingsData] = await Promise.all([
          fetchWorld(stableWorldId),
          fetchWorldCharacters(stableWorldId),
          fetchWorlds(),
          fetchModels("text"),
          fetchSettings(),
        ]);
        if (!cancelled) {
          setWorld(worldData);
          setCharacters(characterData);
          setWorldOptions(availableWorlds);
          setTextModels(textModelData);
          setSettings(settingsData);
        }
      } catch (loadError) {
        if (!cancelled) {
          setError(loadError instanceof Error ? loadError.message : "加载角色池失败");
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
  }, [worldId]);

  const sortedCharacters = useMemo(
    () => [...characters].sort((left, right) => left.name.localeCompare(right.name, "zh-CN")),
    [characters],
  );

  const otherWorldOptions = useMemo(
    () => worldOptions.filter((item) => item.id !== worldId),
    [worldId, worldOptions],
  );
  const textModelMap = useMemo(
    () =>
      new Map(
        textModels.flatMap((model) => [
          [model.id, model],
          [model.model_id, model],
          [model.name, model],
        ]),
      ),
    [textModels],
  );

  function resolveCharacterModelLabel(character: CharacterResponse): string {
    const modelRef = character.model.trim();
    if (!modelRef) {
      return "模型：跟随默认文本模型";
    }
    const matched = textModelMap.get(modelRef);
    if (matched) {
      return `模型：${matched.name || matched.model_id}`;
    }
    return `模型：${modelRef}`;
  }

  const fallbackTextModel = useMemo(
    () => textModels.find((model) => model.is_default) ?? textModels[0] ?? null,
    [textModels],
  );

  function resolveEffectiveModelConfig(modelRef: string | null | undefined): ModelConfigResponse | null {
    const normalizedRef = modelRef?.trim() ?? "";
    if (!normalizedRef) {
      return null;
    }
    return textModelMap.get(normalizedRef) ?? null;
  }

  function resolveEffectiveModelLabel(character: CharacterResponse): string {
    const overrideModel = resolveEffectiveModelConfig(character.model);
    const defaultModel = resolveEffectiveModelConfig(settings?.default_text_model);
    const effectiveModel = overrideModel ?? defaultModel ?? fallbackTextModel;
    const effectiveLabel = effectiveModel
      ? effectiveModel.name.trim() || effectiveModel.model_id.trim() || effectiveModel.id.trim()
      : "";

    if (!effectiveLabel) {
      return "模型：未配置文本模型";
    }
    if (overrideModel) {
      return `模型：${effectiveLabel}`;
    }
    if (defaultModel) {
      return `模型：${effectiveLabel}（默认）`;
    }
    return `模型：${effectiveLabel}（回退）`;
  }

  async function handleDelete(character: CharacterResponse) {
    if (!worldId) {
      setError("缺少世界 ID");
      return;
    }

    try {
      setDeleting(character.id);
      setError(null);
      await deleteWorldCharacter(worldId, character.id);
      setCharacters((current) => current.filter((item) => item.id !== character.id));
      if (world?.player_character_id === character.id) {
        setWorld((current) => (current ? { ...current, player_character_id: null } : current));
      }
      setCharacterPendingDelete((current) => (current?.id === character.id ? null : current));
    } catch (deleteError) {
      setError(deleteError instanceof Error ? deleteError.message : "删除角色失败");
    } finally {
      setDeleting(null);
    }
  }

  function requestDelete(character: CharacterResponse) {
    setCharacterPendingDelete(character);
    setError(null);
  }

  function closeDeleteDialog() {
    if (deleting) {
      return;
    }
    setCharacterPendingDelete(null);
  }

  function handleExportTemplate(character: CharacterResponse) {
    if (!worldId) {
      setError("缺少世界 ID");
      return;
    }
    void (async () => {
      try {
        setError(null);
        const payload = await exportWorldCharacterTemplate(worldId, character.id);
        const blob = new Blob([JSON.stringify(payload, null, 2)], { type: "application/json" });
        const url = URL.createObjectURL(blob);
        const anchor = document.createElement("a");
        anchor.href = url;
        anchor.download = `character-template-${character.name}.json`;
        anchor.click();
        URL.revokeObjectURL(url);
      } catch (exportError) {
        setError(exportError instanceof Error ? exportError.message : "导出模板失败");
      }
    })();
  }

  function openCreateDialog(character: CharacterResponse) {
    setCreatingFromCharacter(character);
    setCreateMode("same_world");
    setTargetWorldId(otherWorldOptions[0]?.id ?? "");
    setNewCharacterName("");
    setSubmitting(false);
    setError(null);
  }

  function closeCreateDialog() {
    if (submitting) {
      return;
    }
    setCreatingFromCharacter(null);
    setCreateMode("same_world");
    setTargetWorldId("");
    setNewCharacterName("");
    setSubmitting(false);
  }

  async function handleCreateFromTemplate() {
    if (!creatingFromCharacter || !worldId) {
      return;
    }

    const nextName = newCharacterName.trim();
    if (!nextName) {
      setError("基于此角色创建新角色时必须填写新的角色名称");
      return;
    }

    const nextTargetWorldId = createMode === "same_world" ? worldId : targetWorldId;
    if (!nextTargetWorldId) {
      setError("创建到其他世界时必须选择目标世界");
      return;
    }

    try {
      setSubmitting(true);
      setError(null);
      const created = await createCharacterInWorldFromCharacter(worldId, creatingFromCharacter.id, {
        target_world_id: nextTargetWorldId,
        name: nextName,
      });
      if (nextTargetWorldId === worldId) {
        setCharacters((current) => [...current, created]);
      }
      closeCreateDialog();
    } catch (createError) {
      setError(createError instanceof Error ? createError.message : "创建角色失败");
      setSubmitting(false);
    }
  }

  return (
    <ScreenLayout
      title={world ? `${world.name} / 角色池` : "角色池"}
      subtitle="管理该世界中的角色实例。跨世界复用基于角色模板创建，不带入原有记忆和运行状态。"
      toolbar={
        <>
          <button type="button" onClick={() => navigate("/worlds")} className="action-btn">
            返回世界列表
          </button>
          {worldId ? (
            <>
              <button type="button" onClick={() => navigate(`/worlds/${worldId}/edit`)} className="action-btn">
                编辑世界
              </button>
              <button
                type="button"
                onClick={() => navigate(`/characters/new?worldId=${worldId}`)}
                className="action-btn action-btn--accent"
              >
                + 新建角色
              </button>
            </>
          ) : null}
        </>
      }
      maxWidth={980}
    >
      {worldId ? (
        <div className="character-pool-mobile-header show-mobile">
          <div className="character-pool-mobile-copy">
            <strong>{world ? `${world.name} / \u89d2\u8272\u6c60` : "\u89d2\u8272\u6c60"}</strong>
          </div>
          <button
            type="button"
            onClick={() => navigate(`/characters/new?worldId=${worldId}`)}
            className="action-btn action-btn--accent"
          >
            {"\u002b \u65b0\u5efa\u89d2\u8272"}
          </button>
        </div>
      ) : null}

      {loading ? <SurfacePanel className="surface-panel--pad-lg">正在加载角色池...</SurfacePanel> : null}
      {error ? <SurfacePanel className="surface-panel--pad-lg text-error">错误：{error}</SurfacePanel> : null}

      {!loading && !error && world && sortedCharacters.length === 0 ? (
        <SurfacePanel className="surface-panel--pad-lg">
          <div className="empty-text">当前世界还没有角色。先创建角色，再继续做导出模板和跨世界创建。</div>
        </SurfacePanel>
      ) : null}

      {!loading && !error && world && sortedCharacters.length > 0 ? (
        <div className="card-grid character-pool-grid">
          {sortedCharacters.map((character) => {
            const isDeleting = deleting === character.id;
            return (
              <div
                key={character.id}
                className="card-item"
                onClick={() => navigate(`/characters/${character.id}/edit?worldId=${world.id}`)}
              >
                <button
                  type="button"
                  aria-label={`删除角色 ${character.name}`}
                  title="删除角色"
                  disabled={isDeleting}
                  onClick={(event) => {
                    event.stopPropagation();
                    requestDelete(character);
                  }}
                  className="card-delete-btn"
                >
                  <X size={14} />
                </button>
                {character.portrait_assets[0] ? (
                  <div className="card-item-icon card-item-icon--image">
                    <img src={assetUrl(character.portrait_assets[0])} alt={character.name} className="card-item-icon-image" />
                  </div>
                ) : (
                  <div className="card-item-icon"><Users size={18} /></div>
                )}
                <div className="card-item-title">{character.name}</div>
                <div className="card-item-desc">{character.role || "未设定身份"}</div>
                <div className="card-item-meta">
                  <span>{resolveEffectiveModelLabel(character)}</span>
                  <span>立绘：{character.portrait_assets.length}</span>
                  {world.player_character_id === character.id ? <span>玩家操控</span> : null}
                </div>
                <div className="card-item-actions">
                  <button
                    type="button"
                    className="card-action-btn"
                    onClick={(event) => {
                      event.stopPropagation();
                      navigate(`/characters/${character.id}/edit?worldId=${world.id}`);
                    }}
                  >
                    编辑角色
                  </button>
                  <button
                    type="button"
                    className="card-action-btn card-action-btn--secondary"
                    onClick={(event) => {
                      event.stopPropagation();
                      openCreateDialog(character);
                    }}
                  >
                    基于此角色创建新角色
                  </button>
                  <button
                    type="button"
                    className="card-action-btn card-action-btn--secondary"
                    onClick={(event) => {
                      event.stopPropagation();
                      handleExportTemplate(character);
                    }}
                  >
                    导出模板
                  </button>
                  <button
                    type="button"
                    disabled={isDeleting}
                    className="card-action-btn card-action-btn--secondary"
                    onClick={(event) => {
                      event.stopPropagation();
                      requestDelete(character);
                    }}
                  >
                    {isDeleting ? "删除中..." : "删除"}
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      ) : null}

      <ModalDialog
        open={Boolean(creatingFromCharacter)}
        title="基于角色模板创建新角色"
        onClose={closeCreateDialog}
        maxWidth={640}
        footer={
          <>
            <button type="button" className="action-btn" disabled={submitting} onClick={closeCreateDialog}>
              取消
            </button>
            <button
              type="button"
              className="action-btn action-btn--accent"
              disabled={submitting}
              onClick={() => void handleCreateFromTemplate()}
            >
              {submitting ? "创建中..." : "创建新角色"}
            </button>
          </>
        }
      >
        {creatingFromCharacter ? (
          <div className="editor-content">
            <div>
              <strong style={{ fontSize: 15 }}>{creatingFromCharacter.name}</strong>
              <p className="text-muted" style={{ marginTop: 4, marginBottom: 0 }}>
                这里只会带入基础配置，不会带入记忆、关系、物品、事件参与和其他运行态数据。
              </p>
            </div>

            <div style={{ display: "flex", gap: 12, flexWrap: "wrap" }}>
              <button
                type="button"
                className={`action-btn${createMode === "same_world" ? " action-btn--accent" : ""}`}
                onClick={() => setCreateMode("same_world")}
                disabled={submitting}
              >
                在本世界创建新角色
              </button>
              <button
                type="button"
                className={`action-btn${createMode === "other_world" ? " action-btn--accent" : ""}`}
                onClick={() => setCreateMode("other_world")}
                disabled={submitting}
              >
                创建到其他世界
              </button>
            </div>

            {createMode === "other_world" ? (
              <label className="editor-field">
                <span className="editor-field-label">目标世界</span>
                <select
                  value={targetWorldId}
                  onChange={(event) => setTargetWorldId(event.target.value)}
                  className="editor-field-input editor-field-select"
                >
                  <option value="">请选择目标世界</option>
                  {otherWorldOptions.map((item) => (
                    <option key={item.id} value={item.id}>
                      {item.name}
                    </option>
                  ))}
                </select>
              </label>
            ) : null}

            <label className="editor-field">
              <span className="editor-field-label">新角色名称</span>
              <input
                value={newCharacterName}
                onChange={(event) => setNewCharacterName(event.target.value)}
                className="editor-field-input"
                placeholder={creatingFromCharacter.name}
              />
            </label>
          </div>
        ) : null}
      </ModalDialog>

      <ConfirmDialog
        open={Boolean(characterPendingDelete)}
        title="删除角色"
        description={
          characterPendingDelete ? `确定删除角色「${characterPendingDelete.name}」吗？此操作不可撤销。` : ""
        }
        confirmLabel={deleting ? "删除中..." : "删除"}
        confirmVariant="danger"
        confirmDisabled={!characterPendingDelete || Boolean(deleting)}
        onClose={closeDeleteDialog}
        onConfirm={() => {
          if (!characterPendingDelete) {
            return;
          }
          void handleDelete(characterPendingDelete);
        }}
      />
    </ScreenLayout>
  );
}
