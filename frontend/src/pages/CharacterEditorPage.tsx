import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router-dom";
import {
  assetUrl,
  createWorldCharacter,
  deleteWorldCharacter,
  fetchCharacter,
  fetchAttributeSchemas,
  fetchModels,
  fetchWorlds,
  listWorldKnowledgeScope,
  updateWorld,
  updateWorldCharacter,
  uploadFile,
  type CharacterResponse,
  type AttributeSchemaResponse,
  type ModelConfigResponse,
  type KnowledgeBase,
  type WorldResponse,
} from "../data/apiAdapter";
import { AttributePanel } from "../components/AttributePanel";
import { ConfirmDialog } from "../components/ModalDialog";
import { HelpTip } from "../components/HelpTip";
import { ScreenLayout, SurfacePanel } from "../components/ScreenLayout";
import { useIsMobile } from "../components/ResponsiveLayout";
import { showToast } from "../components/Toast";
import { ArrowLeft, ChevronRight, GripVertical, MoreHorizontal } from "lucide-react";
import { characterPromptSections } from "./worldEditorShared";

const fixedTabs = [
  { id: "basic", label: "基础信息" },
  { id: "prompt", label: "提示词" },
  { id: "promptPlan", label: "NPC 提示词编排" },
  { id: "runtimeContext", label: "运行时上下文" },
  { id: "avatar", label: "头像" },
  { id: "portrait", label: "立绘" },
  { id: "memory", label: "记忆" },
  { id: "attribute", label: "属性" },
  { id: "preview", label: "预览" },
] as const;

type FixedTabId = (typeof fixedTabs)[number]["id"];

const fixedTabMeta: Record<FixedTabId, { group: string; desc: string }> = {
  basic: { group: "角色内容", desc: "名称、身份、所属世界与调用模型" },
  prompt: { group: "角色内容", desc: "长期背景、系统提示词与返回格式" },
  promptPlan: { group: "角色内容", desc: "拖动调整最终提示词片段顺序" },
  runtimeContext: { group: "角色内容", desc: "每回合注入的实时上下文" },
  avatar: { group: "素材", desc: "角色列表与对话头像" },
  portrait: { group: "素材", desc: "场景与角色面板立绘" },
  memory: { group: "记忆与属性", desc: "记忆偏好与最近对话轮数" },
  attribute: { group: "记忆与属性", desc: "角色标签与结构化属性" },
  preview: { group: "其他", desc: "查看当前配置的完整 JSON" },
};

const newCharacterDraft: CharacterResponse = {
  id: "new",
  name: "新角色",
  world_id: "",
  role: "",
  background_prompt: "",
  model: "",
  memory_strategy: "",
  recent_dialogue_rounds: 30,
  attributes: [],
  portrait_assets: [],
  avatar_asset: "",
  system_prompt_template: "",
  response_contract_prompt: "",
  narration_prompt: "",
  runtime_system_prompt: "",
  knowledge_base_ids: [],
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

function normalizeMemoryStrategyForEditor(strategy: string): string {
  return strategy.trim().toLowerCase() === "recent" ? "优先回忆最近发生的事" : strategy;
}

export function CharacterEditorPage() {
  const navigate = useNavigate();
  const { id } = useParams();
  const [searchParams] = useSearchParams();
  const isNew = id === "new" || !id;
  const preselectedWorldId = searchParams.get("worldId") ?? "";
  const sourceCharacterId = searchParams.get("fromCharacterId") ?? "";

  const [activeTab, setActiveTab] = useState<FixedTabId>("basic");
  const [mobileListOpen, setMobileListOpen] = useState(true);
  const [character, setCharacter] = useState<CharacterResponse | null>(isNew ? newCharacterDraft : null);
  const [attributeSchemas, setAttributeSchemas] = useState<AttributeSchemaResponse[]>([]);
  const [worlds, setWorlds] = useState<WorldResponse[]>([]);
  const [knowledgeScope, setKnowledgeScope] = useState<KnowledgeBase[]>([]);
  const [textModels, setTextModels] = useState<ModelConfigResponse[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [showDeleteDialog, setShowDeleteDialog] = useState(false);
  const [showLeaveDialog, setShowLeaveDialog] = useState(false);
  const [draggedSectionId, setDraggedSectionId] = useState<string | null>(null);
  const isMobile = useIsMobile();
  const dirtyBaselineRef = useRef<string | null>(null);

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
        dirtyBaselineRef.current = null;

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
              memory_strategy: normalizeMemoryStrategyForEditor(sourceCharacter.memory_strategy),
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
          setCharacter({
            ...characterData,
            memory_strategy: normalizeMemoryStrategyForEditor(characterData.memory_strategy),
          });
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
  const hasUnsavedChanges = Boolean(
    character
    && dirtyBaselineRef.current !== null
    && JSON.stringify(character) !== dirtyBaselineRef.current,
  );
  useEffect(() => {
    if (!loading && character && dirtyBaselineRef.current === null) {
      dirtyBaselineRef.current = JSON.stringify(character);
    }
  }, [loading, character]);
  function requestLeave() {
    if (hasUnsavedChanges) {
      setShowLeaveDialog(true);
      return;
    }
    navigate(backTarget);
  }
  function handleMobileBack() {
    if (!mobileListOpen) {
      setMobileListOpen(true);
      return;
    }
    requestLeave();
  }
  function openMobileSection(tabId: FixedTabId) {
    setActiveTab(tabId);
    setMobileListOpen(false);
  }
  useEffect(() => {
    let cancelled = false;
    if (!character?.world_id) {
      setAttributeSchemas([]);
      setKnowledgeScope([]);
      return;
    }
    void fetchAttributeSchemas("character", character.world_id)
      .then((schemas) => { if (!cancelled) setAttributeSchemas(schemas); })
      .catch(() => { if (!cancelled) setAttributeSchemas([]); });
    void listWorldKnowledgeScope(character.world_id)
      .then((bases) => { if (!cancelled) setKnowledgeScope(bases); })
      .catch(() => { if (!cancelled) setKnowledgeScope([]); });
    return () => { cancelled = true; };
  }, [character?.world_id]);
  const attributesText = useMemo(() => (character?.attributes ?? []).join("\n"), [character]);
  const savedCharacterId = !isNew && character?.id ? character.id : undefined;
  const availablePromptSections = useMemo(
    () => [
      ...characterPromptSections.filter(([id]) => id !== "character_attributes"),
      ["character_attributes", "角色基础属性"] as const,
      ...attributeSchemas.map((schema) => [`character_attribute:${schema.key}`, schema.label] as const),
    ],
    [attributeSchemas],
  );
  const promptOrder = useMemo(() => {
    const configured = selectedWorld?.director_config.character_prompt_order;
    const valid = Array.isArray(configured)
      ? configured.filter((id): id is string => typeof id === "string" && availablePromptSections.some(([sectionId]) => sectionId === id))
      : [];
    const movable = Array.from(new Set(valid));
    for (const [sectionId] of availablePromptSections) {
      if (!movable.includes(sectionId)) movable.push(sectionId);
    }
    return movable;
  }, [availablePromptSections, selectedWorld]);

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
                knowledge_base_ids: character.knowledge_base_ids,
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

  function reorderPromptSections(draggedId: string, targetId: string) {
    if (!selectedWorld || draggedId === targetId) return;
    const movable = [...promptOrder];
    const sourceIndex = movable.indexOf(draggedId);
    if (sourceIndex < 0) return;
    const [moved] = movable.splice(sourceIndex, 1);
    const targetIndex = movable.indexOf(targetId);
    if (!moved || targetIndex < 0) return;
    movable.splice(targetIndex, 0, moved);
    const nextOrder = movable;
    setWorlds((current) => current.map((world) => world.id === selectedWorld.id
      ? { ...world, director_config: { ...world.director_config, character_prompt_order: nextOrder } }
      : world));
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
        knowledge_base_ids: character.knowledge_base_ids,
      };

      const saved = isNew
        ? await createWorldCharacter(character.world_id, payload)
        : await updateWorldCharacter(character.world_id, character.id, payload);

      if (selectedWorld) {
        const worldDraft = await updateWorld(selectedWorld.id, {
          name: selectedWorld.name,
          genre: selectedWorld.genre,
          background_prompt: selectedWorld.background_prompt,
          opening_scene: selectedWorld.opening_scene,
          summary: selectedWorld.summary,
          time_system: selectedWorld.time_system,
          map_nodes: selectedWorld.map_nodes,
          time_config: selectedWorld.time_config,
          director_config: selectedWorld.director_config,
          ui_theme_config: selectedWorld.ui_theme_config,
          opening_messages: selectedWorld.opening_messages,
          opening_character_ids: selectedWorld.opening_character_ids,
          player_character_id: selectedWorld.player_character_id ?? null,
        });
        setWorlds((current) => current.map((world) => world.id === worldDraft.id ? worldDraft : world));
      }

      const normalizedSaved = {
        ...saved,
        memory_strategy: normalizeMemoryStrategyForEditor(saved.memory_strategy),
      };
      setCharacter(normalizedSaved);
      dirtyBaselineRef.current = JSON.stringify(normalizedSaved);
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

  function renderMobileSectionList() {
    if (!character) {
      return null;
    }
    const groups: Array<{ label: string; ids: FixedTabId[] }> = [];
    for (const tab of fixedTabs) {
      const group = fixedTabMeta[tab.id].group;
      const current = groups.find((item) => item.label === group);
      if (current) {
        current.ids.push(tab.id);
      } else {
        groups.push({ label: group, ids: [tab.id] });
      }
    }
    const summary = (tabId: FixedTabId) => {
      switch (tabId) {
        case "basic":
          return [character.name.trim() || "未命名角色", character.role.trim() || "未设置身份"].join(" · ");
        case "prompt":
          return character.background_prompt.trim() ? `背景 ${character.background_prompt.length} 字` : "尚未填写背景";
        case "promptPlan":
          return `${promptOrder.length} 个提示词片段`;
        case "runtimeContext":
          return character.runtime_system_prompt.trim() ? `${character.runtime_system_prompt.length} 字` : "使用内置默认";
        case "avatar":
          return character.avatar_asset.trim() ? "已上传头像" : "未上传头像";
        case "portrait":
          return character.portrait_assets.length > 0 ? `${character.portrait_assets.length} 张立绘` : "未上传立绘";
        case "memory":
          return `历史 ${character.recent_dialogue_rounds} 轮`;
        case "attribute":
          return `${(character.attributes ?? []).length} 个角色标签`;
        default:
          return "查看完整 JSON";
      }
    };
    return (
      <div className="wed-m-list">
        {groups.map((group) => (
          <section key={group.label} className="wed-m-group">
            <div className="wed-m-group-title">{group.label}</div>
            <div className="wed-m-cards">
              {group.ids.map((tabId) => {
                const tab = fixedTabs.find((item) => item.id === tabId)!;
                return (
                  <button key={tabId} type="button" className="wed-m-card" onClick={() => openMobileSection(tabId)}>
                    <span className="wed-m-card-copy">
                      <span className="t">{tab.label}</span>
                      <span className="s">{summary(tabId)}</span>
                    </span>
                    <ChevronRight size={16} aria-hidden="true" />
                  </button>
                );
              })}
            </div>
          </section>
        ))}
      </div>
    );
  }

  return (
    <ScreenLayout
      title={isMobile ? "" : (character?.name ?? "角色编辑")}
      subtitle=""
      toolbar={isMobile ? undefined : (
        <>
          <button type="button" onClick={requestLeave} className="action-btn">返回</button>
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
      {isMobile ? (
        <div className="mobile-editor-topbar">
          <button
            type="button"
            className="mobile-editor-back"
            onClick={handleMobileBack}
            aria-label={mobileListOpen ? "返回角色池" : "返回角色分区列表"}
          >
            <ArrowLeft size={18} />
          </button>
          <div className="mobile-editor-heading">
            <strong className="mobile-editor-title">
              {mobileListOpen
                ? character?.name?.trim() || (isNew ? "未命名角色" : "角色编辑")
                : fixedTabs.find((tab) => tab.id === activeTab)?.label ?? "角色编辑"}
              {hasUnsavedChanges ? <i className="mobile-editor-dirty-dot" aria-label="有未保存修改" /> : null}
            </strong>
            <span className="mobile-editor-kicker">{isNew ? "新建角色" : "角色编辑"}</span>
          </div>
          {!isNew ? (
            <button
              type="button"
              className="mobile-editor-more"
              onClick={() => setShowDeleteDialog(true)}
              disabled={deleting || saving}
              aria-label="更多操作"
            >
              <MoreHorizontal size={18} />
            </button>
          ) : (
            <span className="mobile-editor-more-spacer" aria-hidden="true" />
          )}
        </div>
      ) : null}

      {loading ? <SurfacePanel className="surface-panel--pad-lg">正在加载角色详情...</SurfacePanel> : null}
      {error ? <SurfacePanel className="surface-panel--pad-lg text-error">{error}</SurfacePanel> : null}

      {!loading && character ? (
        isMobile && mobileListOpen ? (
          <div className="editor-content">{renderMobileSectionList()}</div>
        ) : (
        <div className="editor-content">
          {isMobile ? (
            <div className="wed-pane-head">
              <div>
                <h2 className="wed-pane-title">{fixedTabs.find((tab) => tab.id === activeTab)?.label}</h2>
                <div className="wed-pane-sub">{fixedTabMeta[activeTab].desc}</div>
              </div>
            </div>
          ) : (
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
          )}

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
                  <div className="text-muted" style={{ fontSize: 12 }}>
                    留空使用内置返回格式；填写后<b>完全替代</b>内置契约，需自行包含 speaker/content/narration 等引擎必需字段的格式说明。
                  </div>
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

            {activeTab === "promptPlan" ? (
              <div className="editor-content">
                {promptOrder.map((sectionId) => {
                  const [_, label] = availablePromptSections.find(([id]) => id === sectionId)!;
                  const preview = (() => {
                    switch (sectionId) {
                      case "world_background": return selectedWorld?.background_prompt || "";
                      case "oag_context": return "由当前会话的记忆检索结果生成，并遵循角色可见范围。";
                      case "character_background": return character.background_prompt;
                      case "character_rules": return character.system_prompt_template;
                      case "character_attributes": return JSON.stringify({ role: character.role, attributes: character.attributes }, null, 2);
                      case "world_runtime_context": return String(selectedWorld?.director_config.runtime_context_prompt ?? "");
                      case "character_runtime_context": return character.runtime_system_prompt;
                      case "visible_characters": return "由当前场景角色、地点、场景状态和可见性规则生成。";
                      case "chat_history": return `最近 ${character.recent_dialogue_rounds} 轮对话与命中的记忆。`;
                      case "narration_prompt": return character.narration_prompt;
                      case "player_input": return "当前玩家输入与附件。";
                      case "response_contract": return character.response_contract_prompt;
                      case "fact_memory_rules": return "框架返回字段与记忆提取规则。";
                      default: {
                        const schema = attributeSchemas.find((item) => `character_attribute:${item.key}` === sectionId);
                        return schema ? `${schema.label}（${schema.key}）\n默认值：${JSON.stringify(schema.default_value)}` : "由当前角色可用工具生成。";
                      }
                    }
                  })();
  return (
                    <details key={sectionId} draggable
                      onDragStart={(event) => { setDraggedSectionId(sectionId); event.dataTransfer.setData("text/plain", sectionId); event.dataTransfer.effectAllowed = "move"; }}
                      onDragOver={(event) => { event.preventDefault(); }}
                      onDrop={(event) => { event.preventDefault(); const dragged = event.dataTransfer.getData("text/plain") || draggedSectionId; if (dragged) reorderPromptSections(dragged, sectionId); setDraggedSectionId(null); }}
                      onDragEnd={() => setDraggedSectionId(null)}
                      style={{ border: "1px solid var(--color-border)", borderRadius: 6, opacity: draggedSectionId === sectionId ? 0.5 : 1 }}>
                      <summary style={{ display: "flex", alignItems: "center", gap: 8, padding: "9px 10px", cursor: "grab", listStyle: "none" }}>
                        <GripVertical size={15} aria-label="拖动排序" />
                        <span style={{ flex: 1 }}>{label}</span>
                      </summary>
                      <pre style={{ margin: "0 12px 12px 34px", whiteSpace: "pre-wrap", overflowWrap: "anywhere", fontSize: 12, lineHeight: 1.5 }}>{preview || "当前内容为空。"}</pre>
                    </details>
                  );
                })}
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
                <div className="editor-field">
                  <div className="field-label-head">
                    <span className="editor-field-label">记忆偏好</span>
                    <HelpTip text="recent 表示优先回忆最近发生的事，不是只保留最近记忆。可填写：优先回忆最近发生的事、重要的事要牢记、不记。留空使用默认；“不记”会停止该角色的长期记忆召回和自动总结，但不会删除已有记忆。" />
                  </div>
                  <textarea
                    aria-label="记忆偏好"
                    value={character.memory_strategy}
                    onChange={(event) => updateDraft({ memory_strategy: event.target.value })}
                    className="editor-field-input editor-field-textarea"
                    placeholder="例如：优先回忆最近发生的事；重要的事要牢记"
                  />
                </div>
                <div className="editor-field">
                  <div className="field-label-head">
                    <span className="editor-field-label">最近对话轮数</span>
                    <HelpTip text="每次回复时直接带入该角色可见的最近几轮对话。0 表示不直接携带对话原文，长期记忆仍按世界的记忆模式召回。为避免与总结重复，实际轮数会按总结间隔对齐，可能略有增减。" />
                  </div>
                  <input
                    type="number"
                    aria-label="最近对话轮数"
                    min={0}
                    value={character.recent_dialogue_rounds}
                    onChange={(event) =>
                      updateDraft({ recent_dialogue_rounds: Math.max(0, Number(event.target.value) || 0) })
                    }
                    className="editor-field-input"
                  />
                </div>
                <div className="editor-field">
                  <span className="editor-field-label">可查询的知识库</span>
                  <div className="text-muted" style={{ fontSize: 12 }}>
                    可选范围来自所属世界：世界自带的知识库自动可用，公共知识库需先在世界主控权限里勾选。这里全不选＝沿用世界主控范围。
                  </div>
                  {knowledgeScope.length === 0 ? (
                    <div className="text-muted">该世界当前没有可查询的知识库。</div>
                  ) : (
                    <div className="editor-interaction-options">
                      {knowledgeScope.map((item) => (
                        <label key={item.id} className="editor-interaction-option">
                          <input
                            type="checkbox"
                            checked={character.knowledge_base_ids.includes(item.id)}
                            onChange={() => {
                              const current = new Set(character.knowledge_base_ids);
                              if (current.has(item.id)) {
                                current.delete(item.id);
                              } else {
                                current.add(item.id);
                              }
                              updateDraft({ knowledge_base_ids: Array.from(current) });
                            }}
                          />
                          <span>
                            {item.name}
                            {item.world_id ? "（世界自带）" : "（公共）"}
                          </span>
                        </label>
                      ))}
                    </div>
                  )}
                </div>
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
                      worldId={character.world_id}
                      onSchemaChange={() => {
                        void fetchAttributeSchemas("character", character.world_id).then(setAttributeSchemas);
                      }}
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
        )
      ) : null}

      {isMobile ? (
        <div className="mobile-editor-actionbar">
          <button
            type="button"
            onClick={() => void handleSave()}
            disabled={saving || deleting}
            className="action-btn action-btn--accent"
          >
            {saving ? "保存中..." : "保存角色"}
          </button>
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

      <ConfirmDialog
        open={showLeaveDialog}
        title="放弃未保存的修改？"
        description="当前角色的修改尚未保存，离开后这些修改会丢失。"
        confirmLabel="放弃修改"
        cancelLabel="继续编辑"
        confirmVariant="danger"
        onClose={() => setShowLeaveDialog(false)}
        onConfirm={() => {
          setShowLeaveDialog(false);
          navigate(backTarget);
        }}
      />
    </ScreenLayout>
  );
}
