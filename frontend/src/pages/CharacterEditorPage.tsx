import { useEffect, useMemo, useState, type KeyboardEvent } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router-dom";
import {
  assetUrl,
  createWorldCharacter,
  deleteWorldCharacter,
  fetchCharacter,
  fetchModels,
  fetchWorlds,
  uploadFile,
  updateWorldCharacter,
  type CharacterResponse,
  type CustomAttributeDefinition,
  type ModelConfigResponse,
  type WorldResponse,
} from "../data/apiAdapter";
import { ConfirmDialog } from "../components/ModalDialog";
import { ScreenLayout, SurfacePanel } from "../components/ScreenLayout";
import { showToast } from "../components/Toast";
import { X, Check } from "lucide-react";

const CUSTOM_TAB_PREFIX = "custom:";
const LEGACY_CUSTOM_TAB_PREFIX = "legacy-custom:";

const fixedTabs = [
  { id: "basic", label: "客观角色资料" },
  { id: "prompt", label: "角色提示词" },
  { id: "portrait", label: "立绘图床" },
  { id: "memory", label: "记忆策略" },
  { id: "attribute", label: "角色标签" },
  { id: "preview", label: "发送预览" },
] as const;

type FixedTabId = (typeof fixedTabs)[number]["id"];
type EditorTabId = FixedTabId | `custom:${string}` | `legacy-custom:${string}`;

function toCustomTabId(tabName: string): `custom:${string}` {
  return `${CUSTOM_TAB_PREFIX}${tabName}` as `custom:${string}`;
}

function toLegacyCustomTabId(tabName: string): `legacy-custom:${string}` {
  return `${LEGACY_CUSTOM_TAB_PREFIX}${tabName}` as `legacy-custom:${string}`;
}

function parseCustomTabName(tabId: EditorTabId): string | null {
  return tabId.startsWith(CUSTOM_TAB_PREFIX) ? tabId.slice(CUSTOM_TAB_PREFIX.length) : null;
}

function parseLegacyCustomTabName(tabId: EditorTabId): string | null {
  return tabId.startsWith(LEGACY_CUSTOM_TAB_PREFIX) ? tabId.slice(LEGACY_CUSTOM_TAB_PREFIX.length) : null;
}

function createFallbackCustomAttributeDefinition(name: string, order: number): CustomAttributeDefinition {
  return {
    id: name
      .trim()
      .toLowerCase()
      .replace(/[^\w-]+/g, "_")
      .replace(/^_+|_+$/g, "") || `custom_${order}`,
    name: name.trim(),
    value_type: "longText",
    order,
    enabled: true,
    required: false,
    placeholder: "",
    default_value: "",
  };
}

const defaultSystemPromptTemplate = `你是{{speaker}}。

角色身份 / 职责：{{role}}

{{background_prompt}}

你必须始终站在该角色视角回应，不要代替玩家决定行动。

如果需要输出对白或行动，只输出该角色本轮会表达的内容。`;

const defaultResponseContractPrompt = `只返回一个 JSON 对象，包含字符串字段：speaker、content、intent、emotion、narration。不要输出 markdown。`;

const defaultNarrationPrompt = `除扮演当前角色说话外，你还需要同时输出 narration 字段，用一两句简洁旁白补充这一轮发言后场景里真实发生的环境变化、动作结果和气氛变化。

要求：
1. narration 不能复述 content 里的对白。
2. narration 只描述当前角色视角下可以确定的外部变化，不代替其他角色发言，不补写玩家未做出的行动。
3. 如果这一轮没有新的环境变化，narration 返回空字符串。`;

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
  custom_tabs: {},
  system_prompt_template: defaultSystemPromptTemplate,
  response_contract_prompt: defaultResponseContractPrompt,
  narration_prompt: defaultNarrationPrompt,
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

  const [activeTab, setActiveTab] = useState<EditorTabId>("basic");
  const [character, setCharacter] = useState<CharacterResponse | null>(isNew ? newCharacterDraft : null);
  const [worlds, setWorlds] = useState<WorldResponse[]>([]);
  const [textModels, setTextModels] = useState<ModelConfigResponse[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [showDeleteDialog, setShowDeleteDialog] = useState(false);
  const [addingCustomTab, setAddingCustomTab] = useState(false);
  const [newCustomTabName, setNewCustomTabName] = useState("");

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
        if (cancelled) return;
        setWorlds(worldData);
        setTextModels(modelData);

        if (isNew) {
          const preferredWorldId = worldData.some((world) => world.id === preselectedWorldId) ? preselectedWorldId : "";
          const preferredWorld = worldData.find((world) => world.id === preferredWorldId) ?? worldData[0] ?? null;
          if (sourceCharacterId) {
            const sourceCharacter = await fetchCharacter(sourceCharacterId);
            if (cancelled) return;
            setCharacter({
              ...sourceCharacter,
              id: "new",
              name: "",
              world_id: preferredWorldId || sourceCharacter.world_id || worldData[0]?.id || "",
              custom_tabs: buildSharedCharacterCustomTabs(preferredWorld, sourceCharacter.custom_tabs),
            });
          } else {
            setCharacter((current) => ({
              ...(current ?? newCharacterDraft),
              world_id: current?.world_id || preferredWorldId || worldData[0]?.id || "",
              custom_tabs: buildSharedCharacterCustomTabs(preferredWorld, current?.custom_tabs ?? {}),
            }));
          }
          return;
        }

        const characterData = await fetchCharacter(id as string);
        if (!cancelled) setCharacter(characterData);
      } catch (loadError) {
        if (!cancelled) setError(loadError instanceof Error ? loadError.message : "角色加载失败");
      } finally {
        if (!cancelled) setLoading(false);
      }
    }
    void loadData();
    return () => {
      cancelled = true;
    };
  }, [id, isNew, preselectedWorldId, sourceCharacterId]);

  const selectedWorld = worlds.find((world) => world.id === character?.world_id) ?? null;
  const attributesText = useMemo(() => (character?.attributes ?? []).join("\n"), [character]);
  const characterAttributeDefinitions = useMemo(
    () => {
      const definitions = selectedWorld?.character_custom_attribute_definitions ?? [];
      const fallbackDefinitions = definitions.length > 0
        ? definitions
        : Object.keys(character?.custom_tabs ?? {}).map((name, index) =>
            createFallbackCustomAttributeDefinition(name, index),
          );
      return fallbackDefinitions
        .filter((definition: CustomAttributeDefinition) => definition.enabled !== false && definition.name.trim())
        .sort((a: CustomAttributeDefinition, b: CustomAttributeDefinition) => a.order - b.order);
    },
    [character?.custom_tabs, selectedWorld?.character_custom_attribute_definitions],
  );
  const customTabEntries = useMemo(
    () =>
      characterAttributeDefinitions.map(
        (definition) =>
          [definition.name, character?.custom_tabs?.[definition.name] ?? definition.default_value ?? "", definition] as const,
      ),
    [character?.custom_tabs, characterAttributeDefinitions],
  );
  const legacyCustomTabEntries = useMemo(() => {
    const templateNames = new Set(characterAttributeDefinitions.map((definition) => definition.name));
    return Object.entries(character?.custom_tabs ?? {})
      .filter(([tabName]) => tabName.trim() && !templateNames.has(tabName))
      .map(([tabName, content]) => [tabName, content ?? ""] as const);
  }, [character?.custom_tabs, characterAttributeDefinitions]);
  const activeCustomTabName = parseCustomTabName(activeTab);
  const activeLegacyCustomTabName = parseLegacyCustomTabName(activeTab);

  function buildSharedCharacterCustomTabs(
    world: WorldResponse | null,
    currentValues: Record<string, string> = {},
  ): Record<string, string> {
    const next = { ...currentValues };
    for (const definition of world?.character_custom_attribute_definitions ?? []) {
      if (definition.enabled === false || !definition.name.trim()) {
        continue;
      }
      if (!Object.prototype.hasOwnProperty.call(next, definition.name)) {
        next[definition.name] = definition.default_value ?? "";
      }
    }
    return next;
  }

  useEffect(() => {
    if (!activeCustomTabName) {
      return;
    }
    if (!customTabEntries.some(([tabName]) => tabName === activeCustomTabName)) {
      setActiveTab("basic");
    }
  }, [activeCustomTabName, customTabEntries]);

  useEffect(() => {
    if (!activeLegacyCustomTabName) {
      return;
    }
    if (!legacyCustomTabEntries.some(([tabName]) => tabName === activeLegacyCustomTabName)) {
      setActiveTab("basic");
    }
  }, [activeLegacyCustomTabName, legacyCustomTabEntries]);

  const objectivePreview = useMemo(
    () =>
      JSON.stringify(
        {
          "发给谁": character?.name || "当前角色",
          "玩家可编辑提示词": {
            "世界提示词预设": selectedWorld?.director_config?.prompt_presets ?? [],
            "角色长期提示词": character?.background_prompt ?? "",
            "角色系统提示词模板": character?.system_prompt_template ?? "",
            "角色返回契约提示词": character?.response_contract_prompt ?? "",
            "角色旁白提示词": character?.narration_prompt ?? "",
          },
          "客观世界资料": selectedWorld
            ? {
                name: selectedWorld.name,
                genre: selectedWorld.genre,
                background_prompt: selectedWorld.background_prompt,
                summary: selectedWorld.summary,
                opening_scene: selectedWorld.opening_scene,
                time_system: selectedWorld.time_system,
                map_nodes: selectedWorld.map_nodes,
                custom_tabs: selectedWorld.custom_tabs,
              }
            : {},
          "客观角色资料": character
            ? {
                name: character.name,
                role: character.role,
                model: character.model,
                attributes: character.attributes,
                portrait_assets: character.portrait_assets,
                custom_tabs: character.custom_tabs,
              }
            : {},
        },
        null,
        2,
      ),
    [character, selectedWorld],
  );

  function updateDraft(patch: Partial<CharacterResponse>) {
    setCharacter((current) => (current ? { ...current, ...patch } : current));
  }

  function cancelAddCustomTab() {
    setAddingCustomTab(false);
    setNewCustomTabName("");
  }

  function confirmAddCustomTab() {
    if (!character) return;
    const tabName = newCustomTabName.trim();
    if (!tabName) {
      cancelAddCustomTab();
      return;
    }
    if (Object.prototype.hasOwnProperty.call(character.custom_tabs, tabName)) {
      setError(`Tab "${tabName}" already exists`);
      return;
    }
    updateDraft({
      custom_tabs: {
        ...character.custom_tabs,
        [tabName]: "",
      },
    });
    setError(null);
    setActiveTab(toCustomTabId(tabName));
    cancelAddCustomTab();
  }

  function removeCustomTab(tabName: string) {
    if (!character) return;
    if (!Object.prototype.hasOwnProperty.call(character.custom_tabs, tabName)) {
      return;
    }
    const nextCustomTabs = { ...character.custom_tabs };
    delete nextCustomTabs[tabName];
    updateDraft({ custom_tabs: nextCustomTabs });
    if (activeCustomTabName === tabName) {
      const nextTabName = Object.keys(nextCustomTabs)[0] ?? null;
      setActiveTab(nextTabName ? toCustomTabId(nextTabName) : "basic");
    }
  }

  function updateCustomTabContent(tabName: string, content: string) {
    if (!character) return;
    updateDraft({
      custom_tabs: {
        ...character.custom_tabs,
        [tabName]: content,
      },
    });
  }

  function handleNewCustomTabKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "Enter") {
      event.preventDefault();
      confirmAddCustomTab();
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      cancelAddCustomTab();
    }
  }

  async function handleUploadPortrait(file: File | null) {
    if (!file || !character) return;
    try {
      const uploaded = await uploadFile(file);
      updateDraft({ portrait_assets: appendUniqueAsset(character.portrait_assets, uploaded.url) });
    } catch (uploadError) {
      setError(uploadError instanceof Error ? uploadError.message : "上传立绘失败");
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
        world_id: character.world_id.trim(),
        role: character.role.trim(),
        background_prompt: character.background_prompt,
        model: character.model.trim(),
        memory_strategy: character.memory_strategy.trim(),
        recent_dialogue_rounds: Math.max(0, Number(character.recent_dialogue_rounds) || 0),
        attributes: character.attributes.filter(Boolean),
        portrait_assets: character.portrait_assets.filter(Boolean),
        custom_tabs: Object.fromEntries(
          Object.entries(buildSharedCharacterCustomTabs(selectedWorld, character.custom_tabs)).filter(([key]) => key.trim()),
        ),
        system_prompt_template: character.system_prompt_template,
        response_contract_prompt: character.response_contract_prompt,
        narration_prompt: character.narration_prompt,
      };
      const saved = isNew
        ? await createWorldCharacter(payload.world_id, payload)
        : await updateWorldCharacter(character.world_id, character.id, payload);
      setCharacter(saved);
      showToast("角色已保存");
      if (isNew) {
        navigate(`/characters/${saved.id}/edit?worldId=${encodeURIComponent(saved.world_id)}`, { replace: true });
      }
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : "角色保存失败");
    } finally {
      setSaving(false);
    }
  }

  async function handleDelete() {
    if (!character || isNew) return;
    try {
      setDeleting(true);
      setError(null);
      await deleteWorldCharacter(character.world_id, character.id);
      navigate(backTarget);
    } catch (deleteError) {
      setError(deleteError instanceof Error ? deleteError.message : "角色删除失败");
    } finally {
      setDeleting(false);
    }
  }

  return (
    <ScreenLayout
      title={character?.name ?? "角色编辑"}
      subtitle="身份、标签、立绘等是客观资料；角色主观扮演与返回约束都在这里配置。"
      toolbar={(
        <>
          <button type="button" onClick={() => navigate(-1)} className="action-btn">返回</button>
          {!isNew ? <button type="button" onClick={() => setShowDeleteDialog(true)} disabled={deleting || saving} className="action-btn action-btn--danger">删除</button> : null}
          <button type="button" onClick={() => void handleSave()} disabled={saving || deleting} className="action-btn action-btn--accent">{saving ? "保存中..." : "保存"}</button>
        </>
      )}
    >
      {loading ? <SurfacePanel className="surface-panel--pad-lg">正在加载角色详情...</SurfacePanel> : null}
      {error ? <SurfacePanel className="surface-panel--pad-lg text-error">错误：{error}</SurfacePanel> : null}

      {!loading && character ? (
        <div className="editor-content">
          <div className="editor-tabs">
            {fixedTabs.map((tab) => (
              <button key={tab.id} type="button" onClick={() => setActiveTab(tab.id)} className={`editor-tab${activeTab === tab.id ? " editor-tab--active" : ""}`}>
                {tab.label}
              </button>
            ))}
            {customTabEntries.map(([tabName]) => {
              const tabId = toCustomTabId(tabName);
              return (
                <button
                  key={tabId}
                  type="button"
                  onClick={() => setActiveTab(tabId)}
                  className={`editor-tab${activeTab === tabId ? " editor-tab--active" : ""}`}
                >
                  {tabName}
                </button>
              );
            })}
            {legacyCustomTabEntries.map(([tabName]) => {
              const tabId = toLegacyCustomTabId(tabName);
              return (
                <button
                  key={tabId}
                  type="button"
                  onClick={() => setActiveTab(tabId)}
                  className={`editor-tab${activeTab === tabId ? " editor-tab--active" : ""}`}
                >
                  旧：{tabName}
                </button>
              );
            })}
          </div>

          <SurfacePanel className="surface-panel--pad-lg">
            {activeTab === "basic" ? (
              <div className="editor-content">
                <label className="editor-field"><span className="editor-field-label">角色名称</span><input value={character.name} onChange={(event) => updateDraft({ name: event.target.value })} className="editor-field-input" /></label>
                <label className="editor-field"><span className="editor-field-label">角色身份 / 职责（客观资料，不自动写成"你是"）</span><input value={character.role} onChange={(event) => updateDraft({ role: event.target.value })} className="editor-field-input" /></label>
                <label className="editor-field"><span className="editor-field-label">所属世界</span>{isNew ? <select value={character.world_id} onChange={(event) => {
                  const nextWorld = worlds.find((world) => world.id === event.target.value) ?? null;
                  updateDraft({ world_id: event.target.value, custom_tabs: buildSharedCharacterCustomTabs(nextWorld, {}) });
                }} className="editor-field-input editor-field-select">{worlds.map((world) => <option key={world.id} value={world.id}>{world.name}</option>)}</select> : <input value={selectedWorld?.name ?? character.world_id} className="editor-field-input" readOnly />}</label>
                <label className="editor-field"><span className="editor-field-label">调用模型</span><select value={character.model} onChange={(event) => updateDraft({ model: event.target.value })} className="editor-field-input editor-field-select"><option value="">使用默认文本模型</option>{textModels.map((model) => <option key={model.id} value={model.id}>{model.name || model.model_id}</option>)}</select></label>
              </div>
            ) : null}

            {activeTab === "prompt" ? (
              <div className="editor-content">
                <div className="text-muted">角色长期提示词负责人物背景、关系、立场与说话风格；系统模板、返回契约和旁白约束负责运行时包裹层。</div>
                <label className="editor-field"><span className="editor-field-label">角色长期提示词</span><textarea value={character.background_prompt} onChange={(event) => updateDraft({ background_prompt: event.target.value })} className="editor-field-input editor-field-textarea" style={{ minHeight: 220 }} /></label>
                <label className="editor-field"><span className="editor-field-label">角色系统提示词模板</span><textarea value={character.system_prompt_template} onChange={(event) => updateDraft({ system_prompt_template: event.target.value })} className="editor-field-input editor-field-textarea" style={{ minHeight: 220 }} /></label>
                <label className="editor-field"><span className="editor-field-label">角色返回契约提示词</span><textarea value={character.response_contract_prompt} onChange={(event) => updateDraft({ response_contract_prompt: event.target.value })} className="editor-field-input editor-field-textarea" style={{ minHeight: 120 }} /></label>
                <label className="editor-field"><span className="editor-field-label">角色旁白提示词</span><textarea value={character.narration_prompt} onChange={(event) => updateDraft({ narration_prompt: event.target.value })} className="editor-field-input editor-field-textarea" style={{ minHeight: 160 }} /></label>
              </div>
            ) : null}

            {activeTab === "portrait" ? (
              <div className="editor-content">
                <label className="action-btn" style={{ cursor: "pointer", width: "fit-content" }}>上传立绘<input type="file" accept="image/*" style={{ display: "none" }} onChange={(event) => void handleUploadPortrait(event.target.files?.[0] ?? null)} /></label>
                {character.portrait_assets.length === 0 ? <div className="text-muted">当前还没有立绘。</div> : null}
                <div className="asset-gallery">
                  {character.portrait_assets.map((assetPath, index) => (
                    <div key={assetPath} className="asset-gallery-card">
                      <div className="asset-gallery-thumb-wrap">
                        <img src={assetUrl(assetPath)} alt={`${character.name}-${index + 1}`} className="asset-gallery-thumb" />
                        {index === 0 ? <span className="asset-gallery-badge">首图</span> : null}
                        <button type="button" className="asset-gallery-delete" onClick={() => updateDraft({ portrait_assets: removeAsset(character.portrait_assets, assetPath) })}>删除</button>
                      </div>
                      <button type="button" className="action-btn asset-gallery-primary-btn" onClick={() => updateDraft({ portrait_assets: moveAssetToFront(character.portrait_assets, assetPath) })} disabled={index === 0}>{index === 0 ? "当前首图" : "设为首图"}</button>
                      <div className="asset-gallery-path" title={assetPath}>{getAssetDisplayName(assetPath)}</div>
                    </div>
                  ))}
                </div>
              </div>
            ) : null}

            {activeTab === "memory" ? (
              <div className="editor-content">
                <label className="editor-field"><span className="editor-field-label">记忆设置备注</span><textarea value={character.memory_strategy} onChange={(event) => updateDraft({ memory_strategy: event.target.value })} className="editor-field-input editor-field-textarea" /></label>
                <label className="editor-field"><span className="editor-field-label">带入历史对话轮数</span><input type="number" min={0} value={character.recent_dialogue_rounds} onChange={(event) => updateDraft({ recent_dialogue_rounds: Math.max(0, Number(event.target.value) || 0) })} className="editor-field-input" /></label>
              </div>
            ) : null}

            {activeTab === "attribute" ? (
              <div className="editor-content">
                <label className="editor-field"><span className="editor-field-label">角色标签（每行一个）</span><textarea value={attributesText} onChange={(event) => updateDraft({ attributes: event.target.value.split("\n").map((item) => item.trim()).filter(Boolean) })} className="editor-field-input editor-field-textarea" /></label>
              </div>
            ) : null}

            {activeTab === "preview" ? (
              <div className="editor-content">
                <label className="editor-field"><span className="editor-field-label">角色发送预览</span><textarea readOnly value={objectivePreview} className="editor-field-input editor-field-textarea" style={{ minHeight: 520, fontFamily: "Consolas, 'Courier New', monospace" }} /></label>
              </div>
            ) : null}
            {activeCustomTabName ? (
              <div className="editor-content">
                <div className="text-muted">角色自定义属性项由世界统一配置；这里仅填写当前角色的内容。</div>
                <label className="editor-field">
                  <span className="editor-field-label">{activeCustomTabName}</span>
                  <textarea
                    value={character.custom_tabs[activeCustomTabName] ?? ""}
                    onChange={(event) => updateCustomTabContent(activeCustomTabName, event.target.value)}
                    placeholder={characterAttributeDefinitions.find((definition) => definition.name === activeCustomTabName)?.placeholder ?? ""}
                    className="editor-field-input editor-field-textarea"
                    style={{ minHeight: 260 }}
                  />
                </label>
              </div>
            ) : null}
            {activeLegacyCustomTabName ? (
              <div className="editor-content">
                <div className="text-muted">这是旧角色自定义 tab，尚未纳入当前世界的“角色自定义属性项”。可以在世界编辑里添加同名属性项来统一管理。</div>
                <label className="editor-field">
                  <span className="editor-field-label">{activeLegacyCustomTabName}</span>
                  <textarea
                    value={character.custom_tabs[activeLegacyCustomTabName] ?? ""}
                    onChange={(event) => updateCustomTabContent(activeLegacyCustomTabName, event.target.value)}
                    className="editor-field-input editor-field-textarea"
                    style={{ minHeight: 260 }}
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
        description={character ? `确定删除角色"${character.name}"吗？此操作不可撤销。` : ""}
        confirmLabel={deleting ? "删除中..." : "删除角色"}
        confirmVariant="danger"
        confirmDisabled={deleting || saving}
        onClose={() => !deleting && setShowDeleteDialog(false)}
        onConfirm={() => void handleDelete()}
      />
    </ScreenLayout>
  );
}
