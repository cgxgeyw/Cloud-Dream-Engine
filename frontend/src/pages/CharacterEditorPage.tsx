import { useEffect, useMemo, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router-dom";
import {
  assetUrl,
  createWorldCharacter,
  deleteWorldCharacter,
  fetchCharacter,
  fetchModels,
  fetchWorlds,
  updateWorldCharacter,
  uploadFile,
  type CharacterResponse,
  type ModelConfigResponse,
  type WorldResponse,
} from "../data/apiAdapter";
import { AttributePanel } from "../components/AttributePanel";
import { ConfirmDialog } from "../components/ModalDialog";
import { ScreenLayout, SurfacePanel } from "../components/ScreenLayout";
import { showToast } from "../components/Toast";

const fixedTabs = [
  { id: "basic", label: "基础信息" },
  { id: "prompt", label: "提示词" },
  { id: "runtimeContext", label: "运行时上下文" },
  { id: "avatar", label: "头像" },
  { id: "portrait", label: "立绘" },
  { id: "memory", label: "记忆" },
  { id: "attribute", label: "属性" },
  { id: "preview", label: "预览" },
] as const;

type FixedTabId = (typeof fixedTabs)[number]["id"];

const newCharacterDraft: CharacterResponse = {
  id: "new",
  name: "新角色",
  world_id: "",
  role: "",
  background_prompt: "",
  model: "",
  memory_strategy: "",
  recent_dialogue_rounds: 2,
  attributes: [],
  portrait_assets: [],
  avatar_asset: "",
  system_prompt_template: "",
  response_contract_prompt: "",
  narration_prompt: "",
  runtime_system_prompt: "",
};

function appendUniqueAsset(items: string[], nextItem: string): string[] {
  const value = nextItem.trim();
  return value && !items.includes(value) ? [...items, value] : items;
}

function removeAsset(items: string[], target: string): string[] {
  return items.filter((item) => item !== target);
}

function moveAssetToFront(items: string[], target: string): string[] {
  return items.includes(target) ? [target, ...removeAsset(items, target)] : items;
}

function getAssetDisplayName(assetPath: string): string {
  const parts = assetPath.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? assetPath;
}

export function CharacterEditorPage() {
  const navigate = useNavigate();
  const { id } = useParams();
  const [searchParams] = useSearchParams();
  const isNew = id === "new" || !id;
  const preselectedWorldId = searchParams.get("worldId") ?? "";
  const sourceCharacterId = searchParams.get("fromCharacterId") ?? "";

  const [activeTab, setActiveTab] = useState<FixedTabId>("basic");
  const [character, setCharacter] = useState<CharacterResponse | null>(isNew ? newCharacterDraft : null);
  const [worlds, setWorlds] = useState<WorldResponse[]>([]);
  const [textModels, setTextModels] = useState<ModelConfigResponse[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [showDeleteDialog, setShowDeleteDialog] = useState(false);

  const backTarget = character?.world_id
    ? `/worlds/${character.world_id}/characters`
    : preselectedWorldId
      ? `/worlds/${preselectedWorldId}/characters`
      : "/worlds";

  useEffect(() => {
    let cancelled = false;

    async function loadData() {
      try {
        setLoading(true);
        setError(null);

        const [worldData, modelData] = await Promise.all([fetchWorlds(), fetchModels("text")]);
        if (cancelled) {
          return;
        }
        setWorlds(worldData);
        setTextModels(modelData);

        if (isNew) {
          const preferredWorldId = worldData.some((world) => world.id === preselectedWorldId)
            ? preselectedWorldId
            : worldData[0]?.id ?? "";

          if (sourceCharacterId) {
            const sourceCharacter = await fetchCharacter(sourceCharacterId);
            if (cancelled) {
              return;
            }
            setCharacter({
              ...sourceCharacter,
              id: "new",
              name: sourceCharacter.name ? `${sourceCharacter.name} 副本` : "",
              world_id: preferredWorldId || sourceCharacter.world_id,
            });
          } else {
            setCharacter((current) => ({
              ...(current ?? newCharacterDraft),
              world_id: current?.world_id || preferredWorldId,
            }));
          }
          return;
        }

        const characterData = await fetchCharacter(id as string);
        if (!cancelled) {
          setCharacter(characterData);
        }
      } catch (loadError) {
        if (!cancelled) {
          setError(loadError instanceof Error ? loadError.message : "加载角色失败");
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
  }, [id, isNew, preselectedWorldId, sourceCharacterId]);

  const selectedWorld = worlds.find((world) => world.id === character?.world_id) ?? null;
  const attributesText = useMemo(() => (character?.attributes ?? []).join("\n"), [character]);
  const savedCharacterId = !isNew && character?.id ? character.id : undefined;

  const previewJson = useMemo(
    () =>
      JSON.stringify(
        {
          character: character
            ? {
                id: character.id,
                name: character.name,
                role: character.role,
                world_id: character.world_id,
                model: character.model,
                memory_strategy: character.memory_strategy,
                recent_dialogue_rounds: character.recent_dialogue_rounds,
                attributes: character.attributes,
                portrait_assets: character.portrait_assets,
                avatar_asset: character.avatar_asset,
                system_prompt_template: character.system_prompt_template,
                response_contract_prompt: character.response_contract_prompt,
                narration_prompt: character.narration_prompt,
                runtime_system_prompt: character.runtime_system_prompt,
              }
            : null,
          world: selectedWorld
            ? {
                id: selectedWorld.id,
                name: selectedWorld.name,
                genre: selectedWorld.genre,
                summary: selectedWorld.summary,
                background_prompt: selectedWorld.background_prompt,
              }
            : null,
        },
        null,
        2,
      ),
    [character, selectedWorld],
  );

  function updateDraft(patch: Partial<CharacterResponse>) {
    setCharacter((current) => (current ? { ...current, ...patch } : current));
  }

  async function handleUploadPortrait(file: File | null) {
    if (!file || !character) {
      return;
    }

    try {
      const uploaded = await uploadFile(file);
      updateDraft({
        portrait_assets: appendUniqueAsset(character.portrait_assets, uploaded.url),
      });
    } catch (uploadError) {
      setError(uploadError instanceof Error ? uploadError.message : "上传立绘失败");
    }
  }

  async function handleUploadAvatar(file: File | null) {
    if (!file || !character) {
      return;
    }

    try {
      const uploaded = await uploadFile(file);
      updateDraft({ avatar_asset: uploaded.url });
    } catch (uploadError) {
      setError(uploadError instanceof Error ? uploadError.message : "上传头像失败");
    }
  }

  async function handleSave() {
    if (!character || !character.name.trim()) {
      setError("角色名称不能为空");
      return;
    }
    if (!character.world_id.trim()) {
      setError("所属世界不能为空");
      return;
    }

    try {
      setSaving(true);
      setError(null);

      const payload = {
        name: character.name.trim(),
        role: character.role.trim(),
        background_prompt: character.background_prompt,
        model: character.model.trim(),
        memory_strategy: character.memory_strategy.trim(),
        recent_dialogue_rounds: Math.max(0, Number(character.recent_dialogue_rounds) || 0),
        attributes: character.attributes.filter(Boolean),
        portrait_assets: character.portrait_assets.filter(Boolean),
        avatar_asset: character.avatar_asset.trim(),
        system_prompt_template: character.system_prompt_template,
        response_contract_prompt: character.response_contract_prompt,
        narration_prompt: character.narration_prompt,
        runtime_system_prompt: character.runtime_system_prompt,
      };

      const saved = isNew
        ? await createWorldCharacter(character.world_id, payload)
        : await updateWorldCharacter(character.world_id, character.id, payload);

      setCharacter(saved);
      showToast("角色已保存");

      if (isNew) {
        navigate(`/characters/${saved.id}/edit?worldId=${encodeURIComponent(saved.world_id)}`, {
          replace: true,
        });
      }
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : "保存角色失败");
    } finally {
      setSaving(false);
    }
  }

  async function handleDelete() {
    if (!character || isNew) {
      return;
    }

    try {
      setDeleting(true);
      setError(null);
      await deleteWorldCharacter(character.world_id, character.id);
      navigate(backTarget);
    } catch (deleteError) {
      setError(deleteError instanceof Error ? deleteError.message : "删除角色失败");
    } finally {
      setDeleting(false);
    }
  }

  return (
    <ScreenLayout
      title={character?.name ?? "角色编辑"}
      subtitle=""
      toolbar={(
        <>
          <button type="button" onClick={() => navigate(-1)} className="action-btn">返回</button>
          {!isNew ? (
            <button
              type="button"
              onClick={() => setShowDeleteDialog(true)}
              disabled={deleting || saving}
              className="action-btn action-btn--danger"
            >
              删除
            </button>
          ) : null}
          <button
            type="button"
            onClick={() => void handleSave()}
            disabled={saving || deleting}
            className="action-btn action-btn--accent"
          >
            {saving ? "保存中..." : "保存"}
          </button>
        </>
      )}
    >
      <div className="character-editor-mobile-actions show-mobile">
        <button type="button" onClick={() => navigate(-1)} className="action-btn">
          返回
        </button>
        {!isNew ? (
          <button
            type="button"
            onClick={() => setShowDeleteDialog(true)}
            disabled={deleting || saving}
            className="action-btn action-btn--danger"
          >
            删除
          </button>
        ) : null}
        <button
          type="button"
          onClick={() => void handleSave()}
          disabled={saving || deleting}
          className="action-btn action-btn--accent"
        >
          {saving ? "保存中..." : "保存角色"}
        </button>
      </div>

      {loading ? <SurfacePanel className="surface-panel--pad-lg">正在加载角色详情...</SurfacePanel> : null}
      {error ? <SurfacePanel className="surface-panel--pad-lg text-error">{error}</SurfacePanel> : null}

      {!loading && character ? (
        <div className="editor-content">
          <div className="editor-tabs character-editor-tabs">
            {fixedTabs.map((tab) => (
              <button
                key={tab.id}
                type="button"
                onClick={() => setActiveTab(tab.id)}
                className={`editor-tab${activeTab === tab.id ? " editor-tab--active" : ""}`}
              >
                {tab.label}
              </button>
            ))}
          </div>

          <SurfacePanel className="surface-panel--pad-lg">
            {activeTab === "basic" ? (
              <div className="editor-content">
                <label className="editor-field">
                  <span className="editor-field-label">角色名称</span>
                  <input
                    value={character.name}
                    onChange={(event) => updateDraft({ name: event.target.value })}
                    className="editor-field-input"
                  />
                </label>
                <label className="editor-field">
                  <span className="editor-field-label">身份 / 职责</span>
                  <input
                    value={character.role}
                    onChange={(event) => updateDraft({ role: event.target.value })}
                    className="editor-field-input"
                  />
                </label>
                <label className="editor-field">
                  <span className="editor-field-label">所属世界</span>
                  {isNew ? (
                    <select
                      value={character.world_id}
                      onChange={(event) => updateDraft({ world_id: event.target.value })}
                      className="editor-field-input editor-field-select"
                    >
                      {worlds.map((world) => (
                        <option key={world.id} value={world.id}>
                          {world.name}
                        </option>
                      ))}
                    </select>
                  ) : (
                    <input
                      value={selectedWorld?.name ?? character.world_id}
                      className="editor-field-input"
                      readOnly
                    />
                  )}
                </label>
                <label className="editor-field">
                  <span className="editor-field-label">调用模型</span>
                  <select
                    value={character.model}
                    onChange={(event) => updateDraft({ model: event.target.value })}
                    className="editor-field-input editor-field-select"
                  >
                    <option value="">使用默认文本模型</option>
                    {textModels.map((model) => (
                      <option key={model.id} value={model.id}>
                        {model.name || model.model_id}
                      </option>
                    ))}
                  </select>
                </label>
              </div>
            ) : null}

            {activeTab === "prompt" ? (
              <div className="editor-content">
                <div className="text-muted" style={{ fontSize: 12 }}>
                  当前支持的变量：{"{{current_time}}"}、{"{{当前时间}}"}
                </div>
                <label className="editor-field">
                  <span className="editor-field-label">长期背景提示词</span>
                  <textarea
                    value={character.background_prompt}
                    onChange={(event) => updateDraft({ background_prompt: event.target.value })}
                    className="editor-field-input editor-field-textarea"
                    style={{ minHeight: 220 }}
                  />
                </label>
                <label className="editor-field">
                  <span className="editor-field-label">系统提示词模板</span>
                  <textarea
                    value={character.system_prompt_template}
                    onChange={(event) => updateDraft({ system_prompt_template: event.target.value })}
                    className="editor-field-input editor-field-textarea"
                    style={{ minHeight: 220 }}
                  />
                </label>
                <label className="editor-field">
                  <span className="editor-field-label">返回格式提示词</span>
                  <textarea
                    value={character.response_contract_prompt}
                    onChange={(event) => updateDraft({ response_contract_prompt: event.target.value })}
                    className="editor-field-input editor-field-textarea"
                    style={{ minHeight: 120 }}
                  />
                </label>
                <label className="editor-field">
                  <span className="editor-field-label">旁白提示词</span>
                  <textarea
                    value={character.narration_prompt}
                    onChange={(event) => updateDraft({ narration_prompt: event.target.value })}
                    className="editor-field-input editor-field-textarea"
                    style={{ minHeight: 160 }}
                  />
                </label>
              </div>
            ) : null}

            {activeTab === "runtimeContext" ? (
              <div className="editor-content">
                <label className="editor-field">
                  <span className="editor-field-label">运行时上下文</span>
                  <div className="text-muted" style={{ fontSize: 12 }}>
                    当前支持的变量：{"{{current_time}}"}、{"{{当前时间}}"}
                  </div>
                  <textarea
                    value={character.runtime_system_prompt}
                    onChange={(event) => updateDraft({ runtime_system_prompt: event.target.value })}
                    className="editor-field-input editor-field-textarea"
                    style={{ minHeight: 300 }}
                  />
                </label>
              </div>
            ) : null}

            {activeTab === "avatar" ? (
              <div className="editor-content">
                <label className="action-btn" style={{ cursor: "pointer", width: "fit-content" }}>
                  上传头像
                  <input
                    type="file"
                    accept="image/*"
                    style={{ display: "none" }}
                    onChange={(event) => void handleUploadAvatar(event.target.files?.[0] ?? null)}
                  />
                </label>
                {character.avatar_asset.trim() ? (
                  <div className="asset-gallery">
                    <div className="asset-gallery-card">
                      <div className="asset-gallery-thumb-wrap">
                        <img
                          src={assetUrl(character.avatar_asset)}
                          alt={`${character.name}-头像`}
                          className="asset-gallery-thumb"
                        />
                        <span className="asset-gallery-badge">头像</span>
                        <button
                          type="button"
                          className="asset-gallery-delete"
                          onClick={() => updateDraft({ avatar_asset: "" })}
                        >
                          清除
                        </button>
                      </div>
                      <div className="asset-gallery-path" title={character.avatar_asset}>
                        {getAssetDisplayName(character.avatar_asset)}
                      </div>
                    </div>
                  </div>
                ) : (
                  <div className="text-muted">当前还没有头像。</div>
                )}
              </div>
            ) : null}

            {activeTab === "portrait" ? (
              <div className="editor-content">
                <label className="action-btn" style={{ cursor: "pointer", width: "fit-content" }}>
                  上传立绘
                  <input
                    type="file"
                    accept="image/*"
                    style={{ display: "none" }}
                    onChange={(event) => void handleUploadPortrait(event.target.files?.[0] ?? null)}
                  />
                </label>
                {character.portrait_assets.length === 0 ? (
                  <div className="text-muted">当前还没有立绘。</div>
                ) : null}
                <div className="asset-gallery">
                  {character.portrait_assets.map((assetPath, index) => (
                    <div key={assetPath} className="asset-gallery-card">
                      <div className="asset-gallery-thumb-wrap">
                        <img
                          src={assetUrl(assetPath)}
                          alt={`${character.name}-${index + 1}`}
                          className="asset-gallery-thumb"
                        />
                        {index === 0 ? <span className="asset-gallery-badge">首图</span> : null}
                        <button
                          type="button"
                          className="asset-gallery-delete"
                          onClick={() => updateDraft({ portrait_assets: removeAsset(character.portrait_assets, assetPath) })}
                        >
                          删除
                        </button>
                      </div>
                      <button
                        type="button"
                        className="action-btn asset-gallery-primary-btn"
                        onClick={() => updateDraft({ portrait_assets: moveAssetToFront(character.portrait_assets, assetPath) })}
                        disabled={index === 0}
                      >
                        {index === 0 ? "当前首图" : "设为首图"}
                      </button>
                      <div className="asset-gallery-path" title={assetPath}>
                        {getAssetDisplayName(assetPath)}
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            ) : null}

            {activeTab === "memory" ? (
              <div className="editor-content">
                <label className="editor-field">
                  <span className="editor-field-label">记忆策略</span>
                  <textarea
                    value={character.memory_strategy}
                    onChange={(event) => updateDraft({ memory_strategy: event.target.value })}
                    className="editor-field-input editor-field-textarea"
                  />
                </label>
                <label className="editor-field">
                  <span className="editor-field-label">带入历史对话轮数</span>
                  <input
                    type="number"
                    min={0}
                    value={character.recent_dialogue_rounds}
                    onChange={(event) =>
                      updateDraft({ recent_dialogue_rounds: Math.max(0, Number(event.target.value) || 0) })
                    }
                    className="editor-field-input"
                  />
                </label>
              </div>
            ) : null}

            {activeTab === "attribute" ? (
              <div className="editor-content">
                <label className="editor-field">
                  <span className="editor-field-label">角色标签（每行一个）</span>
                  <textarea
                    value={attributesText}
                    onChange={(event) =>
                      updateDraft({
                        attributes: event.target.value.split("\n").map((item) => item.trim()).filter(Boolean),
                      })
                    }
                    className="editor-field-input editor-field-textarea"
                  />
                </label>
                <SurfacePanel className="surface-panel--pad-lg">
                  <div className="editor-content">
                    <div className="editor-field-label">结构化属性</div>
                    <AttributePanel
                      scope="character"
                      ownerType="character"
                      ownerId={savedCharacterId}
                    />
                  </div>
                </SurfacePanel>
              </div>
            ) : null}

            {activeTab === "preview" ? (
              <div className="editor-content">
                <label className="editor-field">
                  <span className="editor-field-label">当前配置预览</span>
                  <textarea
                    readOnly
                    value={previewJson}
                    className="editor-field-input editor-field-textarea"
                    style={{ minHeight: 520, fontFamily: "Consolas, 'Courier New', monospace" }}
                  />
                </label>
              </div>
            ) : null}
          </SurfacePanel>
        </div>
      ) : null}

      <ConfirmDialog
        open={showDeleteDialog && !isNew && Boolean(character)}
        title="删除角色"
        description={character ? `确定删除角色“${character.name}”吗？此操作不可撤销。` : ""}
        confirmLabel={deleting ? "删除中..." : "删除角色"}
        confirmVariant="danger"
        confirmDisabled={deleting || saving}
        onClose={() => !deleting && setShowDeleteDialog(false)}
        onConfirm={() => void handleDelete()}
      />
    </ScreenLayout>
  );
}
