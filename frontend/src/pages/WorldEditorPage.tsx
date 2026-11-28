import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { useNavigate, useParams } from "react-router-dom";
import {
  assetUrl,
  compileWorldUiDocument,
  createWorld,
  deleteWorld,
  downloadWorldPackage,
  fetchDefaultDirectorResponseContract,
  fetchDefaultMemorySummaryPrompts,
  fetchModels,
  fetchMcpTools,
  fetchWorld,
  fetchWorldCharacters,
  isTauriEnvironment,
  listWorldKnowledgeCandidates,
  uploadFile,
  updateWorld,
  validateWorldUiBundle,
  verifyWorldPackageUiCompatibility,
  type CharacterResponse,
  type KnowledgeBase,
  type ModelConfigResponse,
  type McpToolResponse,
  type MemorySummaryPromptDefaults,
  type WorldCreateRequest,
  type WorldOpeningMessage,
  type WorldResponse,
} from "../data/apiAdapter";
import { AttributePanel } from "../components/AttributePanel";
import { HelpTip } from "../components/HelpTip";
import { ConfirmDialog } from "../components/ModalDialog";
import { useIsMobile } from "../components/ResponsiveLayout";
import { useSectionParam } from "../hooks/useSectionParam";
import { useWorldPromptPreview } from "../hooks/useWorldPromptPreview";
import { GameUiSandboxPreview } from "../components/GameUiSandboxPreview";
import { PromptSendPreviewCard } from "../components/PromptTraceView";
import { PromptSectionList } from "../components/PromptSectionList";
import { buildDirectorAttributePreview } from "../components/promptAttributePreview";
import { ScreenLayout, SurfacePanel } from "../components/ScreenLayout";
import { showToast } from "../components/Toast";
import {
  ArrowDown,
  ArrowLeft,
  ArrowUp,
  Check,
  ChevronRight,
  GripVertical,
  MoreHorizontal,
  Pencil,
  Plus,
  RotateCcw,
  Save,
  Search,
  Trash2,
} from "lucide-react";
import {
  defaultGameUiFile,
  parseGameUiDocument,
  type GameUiPlatform,
  type ParsedGameUiDocument,
} from "../data/gameUi";
import {
  DirectorConfig,
  FixedTabId,
  FoldableEditorSection,
  InteractionKind,
  OpeningComposerRole,
  OpeningSpeakerOption,
  ReturnProcessingRule,
  DIRECTOR_ONLY_MCP_TOOL_IDS,
  MCP_TOOL_CHANGE_SCENE_ID,
  MCP_TOOL_LIST_CHARACTERS_ID,
  MCP_TOOL_LIST_SCENES_ID,
  MCP_TOOL_SWITCH_PLAYER_CHARACTER_ID,
  applyDirectorImageGenerationEnabled,
  resolveDirectorImageGenerationEnabled,
  isDirectorOnlyMcpToolId,
  characterPromptSections,
  directorPromptSections,
  UiGovernanceSnapshot,
  UiThemeConfig,
  appendUniqueAsset,
  buildStatusTabOptions,
  buildThemePreviewStyle,
  buildUiThemeEnvelope,
  createNewWorldDraft,
  fixedTabs,
  formatMapTopologyJson,
  getAssetDisplayName,
  interactionKindOptions,
  moveAssetToFront,
  normalizeDirectorConfig,
  normalizeOpeningCharacterIds,
  normalizeOpeningMessages,
  normalizeTimeConfig,
  normalizeUiThemeConfig,
  parseMapTopologyJson,
  removeAsset,
  resolveExposurePolicyMode,
  resolveOpeningSceneCharacters,
  resolveOpeningSpeakerLabel,
  resolveSceneNames,
} from "./worldEditorShared";

type DirectorPaneId =
  | "mode"
  | "permissions"
  | "context"
  | "summaries"
  | "prompt"
  | "tools";

type StylePaneId = "backgrounds" | "scenes" | "ui" | "preview";

const DIRECTOR_NAV_ITEMS: Array<{ id: DirectorPaneId; label: string }> = [
  { id: "mode", label: "运行方式" },
  { id: "permissions", label: "权限与判定" },
  { id: "context", label: "上下文轮数" },
  { id: "summaries", label: "总结设置" },
  { id: "prompt", label: "模型提示词设置" },
  { id: "tools", label: "MCP 工具授权" },
];

const DIRECTOR_PANE_META: Record<DirectorPaneId, { help?: string }> = {
  mode: { help: "世界模拟由主控调度多个角色；单智能体对话只回复默认角色。记忆模式逐级增加长期上下文。" },
  permissions: { help: "主控专用工具不会下发给普通角色。知识库检索统一在「MCP 工具授权」里开启。" },
  context: {},
  summaries: { help: "统一设置长期记忆召回、自动总结频率，以及 NPC 和世界主控的总结提示词。「召回片段上限」不是总结频率；总结频率由「总结间隔」决定。" },
  prompt: {},
  tools: { help: "授权后工具会进入主控提示词；实际执行还取决于工具的全局状态和服务器配置。" },
};

const MEMORY_MODE_OPTIONS: Array<{ value: DirectorConfig["memory_mode"]; label: string; help: string }> = [
  { value: "history", label: "历史对话", help: "只注入最近的可见对话，不自动召回长期记忆或 OAG 信息。" },
  { value: "embedding", label: "Embedding 记忆", help: "在最近对话之外，按当前输入召回相关的历史对话、事件和总结。" },
  { value: "oag", label: "OAG", help: "在语义召回之外，注入当前角色可见的事实、关系、事件和记忆，帮助理解世界格局。" },
];

const SERVICE_MODE_OPTIONS: Array<{ value: DirectorConfig["service_mode"]; label: string }> = [
  { value: "world_sim", label: "世界模拟" },
  { value: "agent_chat", label: "单智能体对话" },
];

const MEMORY_MODE_TRACK_POSITIONS = MEMORY_MODE_OPTIONS.map(
  (_, index) => 3 + (index / (MEMORY_MODE_OPTIONS.length - 1)) * 94,
);

const SERVICE_MODE_TRACK_POSITIONS = [15, 85];

export function WorldEditorPage() {
  const isMobile = useIsMobile();
  const navigate = useNavigate();
  const { id } = useParams();
  const isNew = id === "new" || !id;
  const [activeTab, setActiveTab] = useState<FixedTabId>("basic");
  // 移动端「打开某项设置」由 URL 的 ?section= 驱动（见 useSectionParam）：
  // 点返回按钮与侧滑/系统返回都只是历史回退，落点一致（都回到编辑页、关闭子面板）。
  const { activeSection, openSection: openSectionParam, closeSection } = useSectionParam<FixedTabId>(
    fixedTabs.map((tab) => tab.id),
  );
  // 预览始终跟随当前设备平台：电脑端只预览桌面界面，手机端只预览移动界面。
  const previewPlatform: GameUiPlatform = isMobile ? "mobile" : "desktop";
  const [world, setWorld] = useState<WorldResponse | null>(isNew ? createNewWorldDraft() : null);
  const [characters, setCharacters] = useState<CharacterResponse[]>([]);
  const [textModels, setTextModels] = useState<ModelConfigResponse[]>([]);
  const [mcpTools, setMcpTools] = useState<McpToolResponse[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [uploadingBackground, setUploadingBackground] = useState(false);
  const [showDeleteDialog, setShowDeleteDialog] = useState(false);
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false);
  const mobileListScrollRef = useRef(0);
  const [showLeaveDialog, setShowLeaveDialog] = useState(false);
  const [openingComposerRole, setOpeningComposerRole] = useState<OpeningComposerRole>("system");
  const [openingComposerSpeaker, setOpeningComposerSpeaker] = useState("");
  const [openingComposerContent, setOpeningComposerContent] = useState("");
  const [editingOpeningIndex, setEditingOpeningIndex] = useState<number | null>(null);
  const [openingEditRole, setOpeningEditRole] = useState<OpeningComposerRole>("system");
  const [openingEditSpeaker, setOpeningEditSpeaker] = useState("");
  const [openingEditContent, setOpeningEditContent] = useState("");
  const [mcpToolSearch, setMcpToolSearch] = useState("");
  const [knowledgeCandidates, setKnowledgeCandidates] = useState<KnowledgeBase[]>([]);
  const [directorPane, setDirectorPane] = useState<DirectorPaneId>("mode");
  const [stylePane, setStylePane] = useState<StylePaneId>("backgrounds");
  const [styleScene, setStyleScene] = useState("");
  const [openingMode, setOpeningMode] = useState<"organize" | "preview">("organize");
  const [openingNpcSearch, setOpeningNpcSearch] = useState("");
  const [openingComposerOpen, setOpeningComposerOpen] = useState(false);
  const [dragMessageIndex, setDragMessageIndex] = useState<number | null>(null);
  const [directorPromptDraft, setDirectorPromptDraft] = useState<string | null>(null);
  const [summaryPromptDefaults, setSummaryPromptDefaults] = useState<MemorySummaryPromptDefaults | null>(null);
  const [savedSignature, setSavedSignature] = useState("");
  const [reverting, setReverting] = useState(false);
  const {
    preview: promptPreview,
    attributeSchemas: promptAttributeSchemas,
    loading: promptPreviewLoading,
    error: promptPreviewError,
    attributeSchemaError: promptAttributeSchemaError,
    reload: reloadPromptPreview,
  } = useWorldPromptPreview({
    active: (isMobile ? activeSection : activeTab) === "promptPreview",
    isNew,
    worldId: world?.id,
    playerCharacterId: world?.player_character_id,
    directorConfig: (world?.director_config ?? {}) as Record<string, unknown>,
  });
  const [mapTopologySource, setMapTopologySource] = useState(formatMapTopologyJson(createNewWorldDraft().map_nodes));
  const [uiGovernance, setUiGovernance] = useState<UiGovernanceSnapshot>({
    loading: false,
    error: null,
    bundle: null,
    desktopCompile: null,
    mobileCompile: null,
    compatibility: null,
  });

  useEffect(() => {
    let cancelled = false;
    fetchDefaultMemorySummaryPrompts()
      .then((defaults) => {
        if (!cancelled) setSummaryPromptDefaults(defaults);
      })
      .catch(() => {
        // 读取失败时保持输入框空白，仍可手写模板。
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    async function loadData() {
      try {
        setLoading(true);
        setError(null);
        if (isNew) {
          const [toolData, modelData, knowledgeData] = await Promise.all([
            fetchMcpTools(),
            fetchModels("text"),
            // 新世界还没有 id：只能选公共库，世界自带的库保存后再回来勾选。
            listWorldKnowledgeCandidates().catch(() => []),
          ]);
          if (cancelled) {
            return;
          }
          const draft = createNewWorldDraft();
          setWorld(draft);
          setMapTopologySource(formatMapTopologyJson(draft.map_nodes));
          setSavedSignature(`${JSON.stringify(draft)}|${formatMapTopologyJson(draft.map_nodes)}`);
          setCharacters([]);
          setMcpTools(toolData);
          setTextModels(modelData);
          setKnowledgeCandidates(knowledgeData);
          return;
        }
        const [worldData, characterData, toolData, modelData] = await Promise.all([
          fetchWorld(id as string),
          fetchWorldCharacters(id as string),
          fetchMcpTools(),
          fetchModels("text"),
        ]);
        const knowledgeData = await listWorldKnowledgeCandidates(id as string).catch(() => []);
        if (!cancelled) {
          setKnowledgeCandidates(knowledgeData);
          const normalizedWorld = {
            ...worldData,
            time_config: normalizeTimeConfig(worldData.time_config as Record<string, unknown>),
            director_config: normalizeDirectorConfig(worldData.director_config as Record<string, unknown>),
            ui_theme_config: buildUiThemeEnvelope(normalizeUiThemeConfig(worldData.ui_theme_config as Record<string, unknown>)),
            opening_messages: normalizeOpeningMessages(worldData.opening_messages),
            opening_character_ids: normalizeOpeningCharacterIds(worldData.opening_character_ids, characterData),
          };
          setWorld(normalizedWorld);
          setMapTopologySource(formatMapTopologyJson(normalizedWorld.map_nodes));
          setSavedSignature(`${JSON.stringify(normalizedWorld)}|${formatMapTopologyJson(normalizedWorld.map_nodes)}`);
          setCharacters(characterData);
          setMcpTools(toolData);
          setTextModels(modelData);
        }
      } catch (loadError) {
        if (!cancelled) {
          setError(loadError instanceof Error ? loadError.message : "加载世界失败");
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
  }, [id, isNew]);

  // normalize* 每次 render 新建对象会击穿下游 useMemo；按源字段引用 memo。
  const timeConfig = useMemo(
    () => normalizeTimeConfig((world?.time_config ?? {}) as Record<string, unknown>),
    [world?.time_config],
  );
  const directorConfig = useMemo(
    () => normalizeDirectorConfig((world?.director_config ?? {}) as Record<string, unknown>),
    [world?.director_config],
  );
  const memoryModeIndex = MEMORY_MODE_OPTIONS.findIndex((option) => option.value === directorConfig.memory_mode);
  const serviceModeIndex = SERVICE_MODE_OPTIONS.findIndex((option) => option.value === directorConfig.service_mode);
  const uiThemeConfig = useMemo(
    () => normalizeUiThemeConfig((world?.ui_theme_config ?? {}) as Record<string, unknown>),
    [world?.ui_theme_config],
  );
  const directorImageGenerationEnabled = resolveDirectorImageGenerationEnabled(
    directorConfig,
    uiThemeConfig,
  );
  const openingMessages = useMemo(
    () => normalizeOpeningMessages(world?.opening_messages ?? []),
    [world?.opening_messages],
  );
  const openingCharacterIds = useMemo(
    () => normalizeOpeningCharacterIds(world?.opening_character_ids ?? [], characters),
    [characters, world?.opening_character_ids],
  );
  const openingSceneCharacters = useMemo(
    () => resolveOpeningSceneCharacters(characters, openingCharacterIds, openingMessages),
    [characters, openingCharacterIds, openingMessages],
  );
  const sceneNames = useMemo(() => resolveSceneNames(world), [world]);
  const mapNodesText = mapTopologySource;
  const selectedPlayerCharacter = characters.find((character) => character.id === world?.player_character_id) ?? null;
  const availableStatusTabs = useMemo(() => buildStatusTabOptions(), []);
  const stylePreview = useMemo(
    () => buildThemePreviewStyle(uiThemeConfig, world?.opening_scene?.trim() || sceneNames[0] || "未命名场景"),
    [sceneNames, uiThemeConfig, world?.opening_scene],
  );
  const parsedDesktopGameUi = useMemo(
    () => parseGameUiDocument(uiThemeConfig.desktop_file, "desktop"),
    [uiThemeConfig.desktop_file],
  );
  const parsedMobileGameUi = useMemo(
    () => parseGameUiDocument(uiThemeConfig.mobile_file, "mobile"),
    [uiThemeConfig.mobile_file],
  );
  const desktopPreviewStylesheet = useMemo(
    () => uiThemeConfig.desktop_stylesheet,
    [uiThemeConfig.desktop_stylesheet],
  );
  const mobilePreviewStylesheet = useMemo(
    () => uiThemeConfig.mobile_stylesheet,
    [uiThemeConfig.mobile_stylesheet],
  );
  const openingSpeakerOptions = useMemo<OpeningSpeakerOption[]>(
    () => characters.map((character) => ({ value: character.name, label: character.name })),
    [characters],
  );
  const previewMessages = useMemo(
    () =>
      openingMessages.length > 0
        ? openingMessages
        : ([{ role: "system", content: "暂无开场消息。" }] as WorldOpeningMessage[]),
    [openingMessages],
  );
  const previewWorldName = world?.name?.trim() || "未命名世界";
  const previewLocation = world?.opening_scene?.trim() || sceneNames[0] || "未命名场景";
  const previewPlayerName = selectedPlayerCharacter?.name?.trim() || "玩家";
  const previewVisibleCharacters = useMemo(
    () =>
      (openingSceneCharacters.length > 0 ? openingSceneCharacters : characters)
        .map((character) => character.name.trim())
        .filter(Boolean),
    [characters, openingSceneCharacters],
  );
  const previewFocusMessage = useMemo(
    () => previewMessages.find((message) => message.role === "agent" && message.content.trim()) ?? null,
    [previewMessages],
  );
  const previewCharacter = useMemo(() => {
    const speakerName = previewFocusMessage?.speaker?.trim();
    if (speakerName) {
      return characters.find((character) => character.name.trim() === speakerName) ?? null;
    }
    return openingSceneCharacters[0] ?? characters[0] ?? null;
  }, [characters, openingSceneCharacters, previewFocusMessage]);
  const previewStatusTabs = useMemo(
    () =>
      availableStatusTabs.map((tab) => ({
        key: tab.key,
        label: tab.label,
        content: tab.content,
      })),
    [availableStatusTabs],
  );
  const previewNarration = useMemo(
    () =>
      previewMessages.find((message) => message.role === "system" && message.content.trim())?.content.trim()
      || world?.summary?.trim()
      || "暂无场景说明。",
    [previewMessages, world?.summary],
  );
  const previewTimeLabel = useMemo(() => {
    const startLabel = timeConfig.start_label.trim();
    const startTime = timeConfig.start_time.trim();
    if (timeConfig.mode === "24h") {
      return startTime || "08:00";
    }
    return startLabel || startTime || "未设定";
  }, [timeConfig.mode, timeConfig.start_label, timeConfig.start_time]);
  const previewPortraitPath = useMemo(
    () => previewCharacter?.portrait_assets?.[0] ?? "",
    [previewCharacter?.portrait_assets],
  );
  const previewPortraitSrc = useMemo(
    () => (previewPortraitPath ? assetUrl(previewPortraitPath) : undefined),
    [previewPortraitPath],
  );
  const selectedMcpTools = useMemo(
    () => mcpTools.filter((tool) =>
      directorConfig.allowed_mcp_tool_ids.includes(tool.id) && !isDirectorOnlyMcpToolId(tool.id),
    ),
    [directorConfig.allowed_mcp_tool_ids, mcpTools],
  );
  const directorOnlyToolCount = DIRECTOR_ONLY_MCP_TOOL_IDS.filter((toolId) =>
    directorConfig.allowed_mcp_tool_ids.includes(toolId),
  ).length;
  const authorizedToolCount = directorConfig.allowed_mcp_tool_ids.length;
  const filteredMcpTools = useMemo(() => {
    const selectedToolIds = new Set(directorConfig.allowed_mcp_tool_ids);
    const availableTools = mcpTools.filter((tool) =>
      !selectedToolIds.has(tool.id) && !isDirectorOnlyMcpToolId(tool.id),
    );
    const keyword = mcpToolSearch.trim().toLowerCase();
    if (!keyword) {
      return availableTools;
    }
    return availableTools.filter((tool) =>
      [
        tool.name,
        tool.description,
        tool.server_name,
        tool.tool_name,
        resolveExposurePolicyMode(tool.exposure_policy),
        tool.risk_level,
        ...tool.trigger_keywords,
      ]
        .join(" ")
        .toLowerCase()
        .includes(keyword),
      );
  }, [directorConfig.allowed_mcp_tool_ids, mcpToolSearch, mcpTools]);
  const resolvedDirectorModelOption = useMemo(() => {
    const modelRef = directorConfig.director_model.trim();
    if (!modelRef) {
      return null;
    }
    return (
      textModels.find((model) =>
        model.id === modelRef || model.model_id === modelRef || model.name === modelRef,
      ) ?? null
    );
  }, [directorConfig.director_model, textModels]);
  const filteredOpeningCharacters = useMemo(() => {
    const keyword = openingNpcSearch.trim().toLowerCase();
    if (!keyword) {
      return characters;
    }
    return characters.filter((character) =>
      [character.name, character.role]
        .join(" ")
        .toLowerCase()
        .includes(keyword),
    );
  }, [characters, openingNpcSearch]);
  const activeStyleScene = styleScene && sceneNames.includes(styleScene) ? styleScene : (sceneNames[0] ?? "");
  const dirtySignature = world ? `${JSON.stringify(world)}|${mapTopologySource}` : "";
  const isDirty = Boolean(world) && savedSignature !== "" && dirtySignature !== savedSignature;
  const directorModelSelectValue = resolvedDirectorModelOption?.id ?? directorConfig.director_model.trim();
  const previewJson = useMemo(
    () =>
      JSON.stringify(
        {
          ...(world ?? {}),
          director_config: directorConfig,
          ui_theme_config: buildUiThemeEnvelope(uiThemeConfig),
        },
        null,
        2,
      ),
    [world, directorConfig, uiThemeConfig],
  );
  const tauriUiGovernanceEnabled = isTauriEnvironment();

  useEffect(() => {
    if (!tauriUiGovernanceEnabled) {
      setUiGovernance({
        loading: false,
        error: "Backend UI governance is available only in the Tauri runtime.",
        bundle: null,
        desktopCompile: null,
        mobileCompile: null,
        compatibility: null,
      });
      return;
    }

    let cancelled = false;
    const timer = window.setTimeout(() => {
      void (async () => {
        try {
          setUiGovernance((current) => ({ ...current, loading: true, error: null }));
          const [bundle, desktopCompile, mobileCompile, compatibility] = await Promise.all([
            validateWorldUiBundle({
              desktop_file: uiThemeConfig.desktop_file,
              mobile_file: uiThemeConfig.mobile_file,
              runtime_version: uiThemeConfig.runtime_version,
              desktop_stylesheet: uiThemeConfig.desktop_stylesheet,
              mobile_stylesheet: uiThemeConfig.mobile_stylesheet,
              capabilities: uiThemeConfig.capabilities,
              storage: uiThemeConfig.storage,
              logic: uiThemeConfig.logic,
            }),
            compileWorldUiDocument({
              source: uiThemeConfig.desktop_file,
              platform: "desktop",
            }),
            compileWorldUiDocument({
              source: uiThemeConfig.mobile_file,
              platform: "mobile",
            }),
            verifyWorldPackageUiCompatibility({
              desktop_file: uiThemeConfig.desktop_file,
              mobile_file: uiThemeConfig.mobile_file,
            }),
          ]);
          if (cancelled) {
            return;
          }
          setUiGovernance({
            loading: false,
            error: null,
            bundle,
            desktopCompile,
            mobileCompile,
            compatibility,
          });
        } catch (governanceError) {
          if (cancelled) {
            return;
          }
          setUiGovernance({
            loading: false,
            error: governanceError instanceof Error ? governanceError.message : "UI governance failed.",
            bundle: null,
            desktopCompile: null,
            mobileCompile: null,
            compatibility: null,
          });
        }
      })();
    }, 250);

    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [tauriUiGovernanceEnabled, uiThemeConfig.desktop_file, uiThemeConfig.mobile_file]);

  function updateDraft(patch: Partial<WorldResponse>) {
    setWorld((current) => (current ? { ...current, ...patch } : current));
  }

  function updateMapTopologySource(source: string) {
    setMapTopologySource(source);
    try {
      updateDraft({ map_nodes: parseMapTopologyJson(source) });
      setError(null);
    } catch {
      // Allow partially typed JSON; save performs the hard validation.
    }
  }

  function updateOpeningMessages(nextMessages: WorldOpeningMessage[]) {
    updateDraft({ opening_messages: nextMessages });
  }

  function updateOpeningCharacterIds(nextCharacterIds: string[]) {
    updateDraft({ opening_character_ids: nextCharacterIds });
  }

  function toggleOpeningCharacter(characterId: string) {
    const nextIds = openingCharacterIds.includes(characterId)
      ? openingCharacterIds.filter((item) => item !== characterId)
      : [...openingCharacterIds, characterId];
    updateOpeningCharacterIds(nextIds);
  }

  function addOpeningMessage() {
    const content = openingComposerContent.trim();
    if (!content) {
      setError("开场内容不能为空");
      return;
    }

    const speaker = openingComposerRole === "agent" ? (openingComposerSpeaker.trim() || characters[0]?.name || null) : null;

    updateOpeningMessages([
      ...openingMessages,
      {
        role: openingComposerRole,
        speaker,
        content,
      },
    ]);
    setError(null);
    setOpeningComposerContent("");
    setOpeningComposerOpen(false);
  }

  function removeOpeningMessage(index: number) {
    if (editingOpeningIndex === index) {
      cancelEditOpeningMessage();
    } else if (editingOpeningIndex !== null && editingOpeningIndex > index) {
      setEditingOpeningIndex(editingOpeningIndex - 1);
    }
    updateOpeningMessages(openingMessages.filter((_, itemIndex) => itemIndex !== index));
  }

  function moveOpeningMessage(index: number, direction: -1 | 1) {
    const nextIndex = index + direction;
    if (nextIndex < 0 || nextIndex >= openingMessages.length) {
      return;
    }
    const nextMessages = [...openingMessages];
    const [target] = nextMessages.splice(index, 1);
    nextMessages.splice(nextIndex, 0, target);
    if (editingOpeningIndex === index) {
      setEditingOpeningIndex(nextIndex);
    } else if (editingOpeningIndex === nextIndex) {
      setEditingOpeningIndex(index);
    }
    updateOpeningMessages(nextMessages);
  }

  function reorderOpeningMessages(from: number, to: number) {
    if (from === to || from < 0 || to < 0 || from >= openingMessages.length || to >= openingMessages.length) {
      return;
    }
    const nextMessages = [...openingMessages];
    const [moved] = nextMessages.splice(from, 1);
    nextMessages.splice(to, 0, moved);
    if (editingOpeningIndex === from) {
      setEditingOpeningIndex(to);
    } else if (editingOpeningIndex !== null) {
      if (from < editingOpeningIndex && to >= editingOpeningIndex) {
        setEditingOpeningIndex(editingOpeningIndex - 1);
      } else if (from > editingOpeningIndex && to <= editingOpeningIndex) {
        setEditingOpeningIndex(editingOpeningIndex + 1);
      }
    }
    updateOpeningMessages(nextMessages);
  }

  function startEditOpeningMessage(index: number) {
    const message = openingMessages[index];
    if (!message) {
      return;
    }
    setEditingOpeningIndex(index);
    setOpeningEditRole(message.role === "agent" ? "agent" : "system");
    setOpeningEditSpeaker(message.speaker?.trim() || "");
    setOpeningEditContent(message.content);
    setOpeningComposerOpen(false);
    setError(null);
  }

  function cancelEditOpeningMessage() {
    setEditingOpeningIndex(null);
    setOpeningEditRole("system");
    setOpeningEditSpeaker("");
    setOpeningEditContent("");
    setError(null);
  }

  function saveOpeningMessage() {
    if (editingOpeningIndex === null) {
      return;
    }
    const content = openingEditContent.trim();
    if (!content) {
      setError("开场内容不能为空");
      return;
    }
    const speaker = openingEditRole === "agent"
      ? (openingEditSpeaker.trim() || characters[0]?.name || null)
      : null;
    const nextMessages = openingMessages.map((message, itemIndex) => (
      itemIndex === editingOpeningIndex
        ? { role: openingEditRole, speaker, content }
        : message
    ));
    updateOpeningMessages(nextMessages);
    cancelEditOpeningMessage();
  }

  function updateGameUiFile(platform: GameUiPlatform, source: string) {
    updateThemePatch(platform === "desktop" ? { desktop_file: source } : { mobile_file: source });
  }

  function updateGameUiStylesheet(platform: GameUiPlatform, source: string) {
    updateThemePatch(platform === "desktop"
      ? { desktop_stylesheet: source, runtime_version: 3 }
      : { mobile_stylesheet: source, runtime_version: 3 });
  }

  function loadDefaultGameUiTemplate(platform: GameUiPlatform) {
    updateGameUiFile(platform, defaultGameUiFile(platform));
    setError(null);
  }

  function renderGameUiDocumentEditor(
    platform: GameUiPlatform,
    parsed: ParsedGameUiDocument,
  ) {
    const label = platform === "desktop" ? "桌面界面" : "移动界面";
    const source = platform === "desktop" ? uiThemeConfig.desktop_file : uiThemeConfig.mobile_file;
    const stylesheet = platform === "desktop"
      ? uiThemeConfig.desktop_stylesheet
      : uiThemeConfig.mobile_stylesheet;
    const compileResult = platform === "desktop" ? uiGovernance.desktopCompile : uiGovernance.mobileCompile;
    const compatibilityDocument = uiGovernance.compatibility?.documents.find((entry) => entry.platform === platform) ?? null;

    return (
      <div
        className="editor-content"
        style={{ padding: 12, border: "1px solid var(--color-border)", borderRadius: 16 }}
      >
        <div className="flex flex--items-center flex--justify-between world-editor-section-head" style={{ gap: 12 }}>
          <div style={{ display: "grid", gap: 6 }}>
            <div className="editor-field-label">{label}</div>
            <div className="text-muted" style={{ fontSize: 12 }}>
              HTML / CSS / JavaScript，隔离页面 + world SDK
            </div>
          </div>
          <div className="flex flex--gap-sm" style={{ flexWrap: "wrap" }}>
            <button
              type="button"
              className="action-btn"
              onClick={() => loadDefaultGameUiTemplate(platform)}
            >
              载入默认模板
            </button>
          </div>
        </div>

        {parsed.diagnostics.length > 0 ? (
          <div className="game-input-bubble" role="status" style={{ display: "grid", gap: 5 }}>
            {parsed.diagnostics.map((diagnostic, index) => (
              <div key={`${diagnostic.code}-${index}`}>
                [{diagnostic.severity}] {diagnostic.code}: {diagnostic.message}
              </div>
            ))}
          </div>
        ) : null}

        <label className="editor-field">
          <span className="editor-field-label">HTML（可包含 style 和 script）</span>
          <textarea
            value={source}
            onChange={(event) => updateGameUiFile(platform, event.target.value)}
            className="editor-field-input editor-field-textarea"
            spellCheck={false}
            style={{ minHeight: 320, fontFamily: "Consolas, 'Courier New', monospace", fontSize: 12 }}
          />
        </label>

        <label className="editor-field">
          <span className="editor-field-label">{"v3 \u539f\u59cb\u6837\u5f0f\u8868\uff08iframe \u5185\uff09"}</span>
          <textarea
            value={stylesheet}
            onChange={(event) => updateGameUiStylesheet(platform, event.target.value)}
            className="editor-field-input editor-field-textarea"
            spellCheck={false}
            style={{ minHeight: 220, fontFamily: "Consolas, 'Courier New', monospace", fontSize: 12 }}
          />
        </label>

        {uiGovernance.loading ? (
          <div className="text-muted" style={{ fontSize: 12 }}>
            Backend governance is validating the current UI source...
          </div>
        ) : null}
        {uiGovernance.error ? (
          <div className="game-input-bubble">{uiGovernance.error}</div>
        ) : null}
        {compileResult ? (
          <div
            style={{
              marginTop: 12,
              padding: 12,
              border: "1px solid var(--color-border-light)",
              borderRadius: 12,
              display: "grid",
              gap: 8,
              background: "var(--color-surface-2)",
            }}
          >
            <div className="editor-field-label">后端编译快照</div>
            <div className="text-muted" style={{ fontSize: 12 }}>
              {compileResult.ok ? "编译通过" : "编译存在错误"}
              {compatibilityDocument ? ` | 兼容性：${compatibilityDocument.ok ? "通过" : "存在不支持的依赖"}` : ""}
            </div>
            <div className="text-muted" style={{ fontSize: 12 }}>
              组件依赖：{compileResult.component_dependencies.join(", ") || "无"}
            </div>
            <div className="text-muted" style={{ fontSize: 12 }}>
              动作依赖：{compileResult.action_dependencies.join(", ") || "无"}
            </div>
            <div className="text-muted" style={{ fontSize: 12 }}>
              能力要求：{compileResult.capability_requirements.join(", ") || "无"}
            </div>
            {compileResult.diagnostics.length > 0 ? (
              <div style={{ display: "grid", gap: 6 }}>
                {compileResult.diagnostics.map((diagnostic, index) => (
                  <div key={`${diagnostic.code}-${index}`} className="text-muted" style={{ fontSize: 12 }}>
                    [{diagnostic.severity}] {diagnostic.code}
                    {diagnostic.path ? ` @ ${diagnostic.path}` : ""}: {diagnostic.message}
                  </div>
                ))}
              </div>
            ) : null}
            {compatibilityDocument && compatibilityDocument.unsupported_components.length > 0 ? (
              <div className="text-muted" style={{ fontSize: 12 }}>
                Unsupported components: {compatibilityDocument.unsupported_components.join(", ")}
              </div>
            ) : null}
            {compatibilityDocument && compatibilityDocument.unsupported_actions.length > 0 ? (
              <div className="text-muted" style={{ fontSize: 12 }}>
                Unsupported actions: {compatibilityDocument.unsupported_actions.join(", ")}
              </div>
            ) : null}
          </div>
        ) : null}

      </div>
    );
  }

  function renderGameUiPreviewSection() {
    const previewLabel = previewPlatform === "mobile" ? "手机端预览" : "电脑端预览";
    const parsedPreview = previewPlatform === "mobile" ? parsedMobileGameUi : parsedDesktopGameUi;
    const previewStylesheet = previewPlatform === "mobile" ? mobilePreviewStylesheet : desktopPreviewStylesheet;
    return (
      <section className="wed-panel">
        <div className="wed-panel-head">
          <div>
            <div className="wed-panel-title">界面预览</div>
            <div className="wed-panel-sub">{previewLabel}</div>
          </div>
        </div>
        <div className="wed-panel-body">
          <div className={`world-style-preview-shell world-style-preview-shell--${previewPlatform}`}>
            <div className={`world-style-preview-frame world-style-preview-frame--${previewPlatform}`}>
              <GameUiSandboxPreview
                platform={previewPlatform}
                document={parsedPreview.document}
                stylesheet={previewStylesheet}
                rootStyle={stylePreview}
                worldName={previewWorldName}
                location={previewLocation}
                timeLabel={previewTimeLabel}
                playerName={previewPlayerName}
                visibleCharacters={previewVisibleCharacters}
                focusSpeaker={previewFocusMessage?.speaker?.trim() || previewCharacter?.name?.trim() || undefined}
                focusContent={previewFocusMessage?.content?.trim() || undefined}
                portraitSrc={previewPortraitSrc}
                narration={previewNarration}
                messages={previewMessages}
                statusTabs={previewStatusTabs}
                parseError={parsedPreview.error}
                usedFallback={parsedPreview.usedFallback}
                showStyleDebug
              />
            </div>
          </div>
        </div>
      </section>
    );
  }

  function renderGameUiGovernanceSection() {
    const compatibilityDocuments = uiGovernance.compatibility?.documents ?? [];
    return (
      <section className="wed-panel">
        <div className="wed-panel-head">
          <div>
            <div className="wed-panel-title">界面治理</div>
            <div className="wed-panel-sub">后端校验、编译与兼容性检查</div>
          </div>
        </div>
        <div className="wed-panel-body">
        <div
          style={{ display: "grid", gap: 10 }}
        >
          <div className="editor-field-label">后端治理快照</div>
          <div className="text-muted" style={{ fontSize: 12 }}>
            {uiGovernance.loading
              ? "正在刷新后端校验、编译与兼容性检查..."
              : uiGovernance.error
                ? uiGovernance.error
                : uiGovernance.bundle
                  ? `打包校验：${uiGovernance.bundle.ok ? "通过" : "存在问题"} | 兼容性：${uiGovernance.compatibility?.ok ? "通过" : "存在不支持的依赖"}`
                  : "暂无治理快照。"}
          </div>
          {uiGovernance.bundle ? (
            <>
              <div className="text-muted" style={{ fontSize: 12 }}>
                桌面端错误：{uiGovernance.bundle.desktop.errors.length} | 移动端错误：{uiGovernance.bundle.mobile.errors.length}
              </div>
              <div className="text-muted" style={{ fontSize: 12 }}>
                桌面端警告：{uiGovernance.bundle.desktop.warnings.length} | 移动端警告：{uiGovernance.bundle.mobile.warnings.length}
              </div>
              {[...uiGovernance.bundle.errors, ...uiGovernance.bundle.warnings].length > 0 ? (
                <div style={{ display: "grid", gap: 5 }}>
                  {[...uiGovernance.bundle.errors, ...uiGovernance.bundle.warnings].map((diagnostic, index) => (
                    <div key={`${diagnostic.code}-${index}`} className="text-muted" style={{ fontSize: 12 }}>
                      [{diagnostic.severity}] {diagnostic.code}{diagnostic.path ? ` @ ${diagnostic.path}` : ""}: {diagnostic.message}
                    </div>
                  ))}
                </div>
              ) : null}
            </>
          ) : null}
          {compatibilityDocuments.length > 0 ? (
            <div style={{ display: "grid", gap: 6 }}>
              {compatibilityDocuments.map((report) => (
                <div key={report.platform} className="text-muted" style={{ fontSize: 12 }}>
                  {report.platform}: {report.ok ? "compatible" : "incompatible"}
                  {report.unsupported_components.length > 0 ? ` | components: ${report.unsupported_components.join(", ")}` : ""}
                  {report.unsupported_actions.length > 0 ? ` | actions: ${report.unsupported_actions.join(", ")}` : ""}
                  {report.unsupported_capabilities.length > 0 ? ` | capabilities: ${report.unsupported_capabilities.join(", ")}` : ""}
                </div>
              ))}
            </div>
          ) : null}
          {uiGovernance.compatibility?.diagnostics.length ? (
            <div style={{ display: "grid", gap: 5 }}>
              {uiGovernance.compatibility.diagnostics.map((diagnostic, index) => (
                <div key={`${diagnostic.code}-${index}`} className="text-muted" style={{ fontSize: 12 }}>
                  [{diagnostic.severity}] {diagnostic.code}{diagnostic.path ? ` @ ${diagnostic.path}` : ""}: {diagnostic.message}
                </div>
              ))}
            </div>
          ) : null}
        </div>
        </div>
      </section>
    );
  }

  function updateDirectorPatch(patch: Partial<DirectorConfig>) {
    updateDraft({ director_config: { ...directorConfig, ...patch } });
  }

  // "填入内置契约"：取后端单一事实源生成的默认契约文本，作者从真实默认值起步改写。
  async function fillDefaultDirectorResponseContract() {
    try {
      const text = await fetchDefaultDirectorResponseContract();
      if (
        directorConfig.response_contract_prompt.trim() &&
        !window.confirm("输入框已有内容，填入内置契约会覆盖现有文本，确定继续？")
      ) {
        return;
      }
      updateDirectorPatch({ response_contract_prompt: text });
    } catch (error) {
      window.alert(error instanceof Error ? error.message : "读取内置契约失败");
    }
  }

  function updateServiceMode(nextMode: DirectorConfig["service_mode"]) {
    const fallbackAgent =
      characters.find((character) => character.id !== world?.player_character_id)
      ?? characters[0]
      ?? null;
    updateDirectorPatch({
      service_mode: nextMode,
      default_agent_id:
        nextMode === "agent_chat"
          ? directorConfig.default_agent_id || fallbackAgent?.id || ""
          : directorConfig.default_agent_id,
      runtime_policy: {
        ...directorConfig.runtime_policy,
      },
    });
  }

  function toggleAllowedMcpTool(toolId: string) {
    const current = new Set(directorConfig.allowed_mcp_tool_ids);
    if (current.has(toolId)) {
      current.delete(toolId);
    } else {
      current.add(toolId);
    }
    updateDirectorPatch({ allowed_mcp_tool_ids: Array.from(current) });
  }

  function toggleInteractionKind(
    configKey: "director_interaction_kinds",
    kind: InteractionKind,
  ) {
    const selected = new Set(directorConfig[configKey]);
    if (selected.has(kind)) {
      selected.delete(kind);
    } else {
      selected.add(kind);
    }
    updateDirectorPatch({ [configKey]: Array.from(selected) } as Pick<DirectorConfig, typeof configKey>);
  }

  function updateMergedDirectorPrompt(value: string) {
    updateDirectorPatch({
      world_director_prompt: value,
    });
  }

  function reorderPromptSections(scope: "director" | "character", visibleIds: string[]) {
    const configKey = scope === "director" ? "director_prompt_order" : "character_prompt_order";
    const ordered = Array.from(new Set([...directorConfig[configKey], ...visibleIds]));
    const visible = new Set(visibleIds);
    let next = 0;
    updateDirectorPatch({ [configKey]: ordered.map((sectionId) => visible.has(sectionId) ? visibleIds[next++] : sectionId) });
  }

  function updateReturnRule(index: number, patch: Partial<ReturnProcessingRule>) {
    updateDirectorPatch({
      return_processing_rules: directorConfig.return_processing_rules.map((item, itemIndex) =>
        itemIndex === index ? { ...item, ...patch } : item,
      ),
    });
  }

  function addReturnRule() {
    updateDirectorPatch({
      return_processing_rules: [
        ...directorConfig.return_processing_rules,
        {
          id: `rule-${Date.now()}`,
          name: "新返回处理规则",
          scope: "both",
          pattern: "",
          replacement: "",
          enabled: true,
          order: directorConfig.return_processing_rules.length + 1,
        },
      ],
    });
  }

  function removeReturnRule(index: number) {
    updateDirectorPatch({
      return_processing_rules: directorConfig.return_processing_rules.filter((_, itemIndex) => itemIndex !== index),
    });
  }

  function updateThemePatch(patch: Partial<UiThemeConfig>) {
    updateDraft({ ui_theme_config: buildUiThemeEnvelope({ ...uiThemeConfig, ...patch }) });
  }

  function toggleDirectorImageGeneration(enabled: boolean) {
    const next = applyDirectorImageGenerationEnabled(directorConfig, uiThemeConfig, enabled);
    updateDraft({
      director_config: next.directorConfig,
      ui_theme_config: buildUiThemeEnvelope(next.uiThemeConfig),
    });
  }

  async function handleUploadBackground(file: File | null, sceneName?: string) {
    if (!file) {
      return;
    }
    try {
      setUploadingBackground(true);
      setError(null);
      const uploaded = await uploadFile(file);
      const assetRef =
        uploaded.url?.trim()
        || uploaded.asset_path?.trim()
        || uploaded.relative_path?.trim()
        || "";
      if (!assetRef) {
        throw new Error("上传成功，但未返回可用的资源路径");
      }
      if (sceneName) {
        updateThemePatch({
          local_scene_backgrounds: {
            ...uiThemeConfig.local_scene_backgrounds,
            [sceneName]: appendUniqueAsset(
              uiThemeConfig.local_scene_backgrounds[sceneName] ?? [],
              assetRef,
            ),
          },
        });
        showToast(`已上传场景背景：${sceneName}`);
        return;
      }
      updateThemePatch({
        local_background_assets: appendUniqueAsset(
          uiThemeConfig.local_background_assets,
          assetRef,
        ),
      });
      showToast("已上传通用背景");
    } catch (uploadError) {
      setError(uploadError instanceof Error ? uploadError.message : "上传背景失败");
    } finally {
      setUploadingBackground(false);
    }
  }

  function removeBackgroundAsset(target: string, sceneName?: string) {
    if (sceneName) {
      const nextSceneAssets = removeAsset(uiThemeConfig.local_scene_backgrounds[sceneName] ?? [], target);
      const nextGroups = { ...uiThemeConfig.local_scene_backgrounds };
      if (nextSceneAssets.length > 0) {
        nextGroups[sceneName] = nextSceneAssets;
      } else {
        delete nextGroups[sceneName];
      }
      updateThemePatch({
        local_scene_backgrounds: nextGroups,
      });
      return;
    }

    updateThemePatch({
      local_background_assets: removeAsset(uiThemeConfig.local_background_assets, target),
    });
  }

  function setPrimaryBackgroundAsset(target: string, sceneName?: string) {
    if (sceneName) {
      updateThemePatch({
        local_scene_backgrounds: {
          ...uiThemeConfig.local_scene_backgrounds,
          [sceneName]: moveAssetToFront(uiThemeConfig.local_scene_backgrounds[sceneName] ?? [], target),
        },
      });
      return;
    }

    updateThemePatch({
      local_background_assets: moveAssetToFront(uiThemeConfig.local_background_assets, target),
    });
  }

  function renderBackgroundTiles(assets: string[], sceneName?: string) {
    if (assets.length === 0) {
      return <div className="wed-empty">暂无背景素材。</div>;
    }
    return (
      <div className="wed-tiles">
        {assets.map((asset, index) => {
          const isVideo = /\.(mp4|webm|mov|m4v)$/i.test(asset);
          return (
            <article key={asset} className="wed-tile">
              <div className="wed-tile-media">
                {isVideo ? (
                  <video src={assetUrl(asset)} muted loop playsInline preload="metadata" />
                ) : (
                  <div className="wed-tile-image" style={{ backgroundImage: `url("${assetUrl(asset)}")` }} />
                )}
                <span className="wed-tile-badge">{index === 0 ? "主背景" : `#${index + 1}`}</span>
              </div>
              <div className="wed-tile-body">
                <div className="wed-tile-copy">
                  <div className="wed-tile-name">{getAssetDisplayName(asset)}</div>
                  <div className="wed-tile-path">{asset}</div>
                </div>
                <div className="wed-tile-actions">
                  {index > 0 ? (
                    <button
                      type="button"
                      className="wed-icon-btn"
                      title="设为主背景"
                      aria-label="设为主背景"
                      onClick={() => setPrimaryBackgroundAsset(asset, sceneName)}
                    >
                      <Check size={14} />
                    </button>
                  ) : null}
                  <button
                    type="button"
                    className="wed-icon-btn wed-icon-btn--danger"
                    title="删除"
                    aria-label="删除背景"
                    onClick={() => removeBackgroundAsset(asset, sceneName)}
                  >
                    <Trash2 size={14} />
                  </button>
                </div>
              </div>
            </article>
          );
        })}
      </div>
    );
  }

  async function handleSave() {
    if (!world || !world.name.trim()) {
      setError("世界名称不能为空");
      return;
    }
    try {
      setSaving(true);
      setError(null);
      const mapTopology = parseMapTopologyJson(mapTopologySource);
      const payload: WorldCreateRequest = {
        name: world.name.trim(),
        genre: world.genre.trim(),
        background_prompt: world.background_prompt.trim(),
        opening_scene: world.opening_scene.trim(),
        summary: world.summary.trim(),
        time_system: "",
        map_nodes: mapTopology,
        time_config: {},
        director_config: directorConfig,
        ui_theme_config: buildUiThemeEnvelope(uiThemeConfig),
        opening_messages: openingMessages,
        opening_character_ids: openingCharacterIds,
        player_character_id: world.player_character_id ?? null,
      };
      const saved = isNew ? await createWorld(payload) : await updateWorld(world.id, payload);
      const normalizedSaved = {
        ...saved,
        time_config: normalizeTimeConfig(saved.time_config as Record<string, unknown>),
        director_config: normalizeDirectorConfig(saved.director_config as Record<string, unknown>),
        ui_theme_config: buildUiThemeEnvelope(normalizeUiThemeConfig(saved.ui_theme_config as Record<string, unknown>)),
        opening_messages: normalizeOpeningMessages(saved.opening_messages),
        opening_character_ids: normalizeOpeningCharacterIds(saved.opening_character_ids, characters),
      };
      setWorld(normalizedSaved);
      setMapTopologySource(formatMapTopologyJson(normalizedSaved.map_nodes));
      setSavedSignature(`${JSON.stringify(normalizedSaved)}|${formatMapTopologyJson(normalizedSaved.map_nodes)}`);
      showToast("世界已保存");
      if (isNew) {
        navigate(`/worlds/${saved.id}/edit`, { replace: true });
      }
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : "保存世界失败");
    } finally {
      setSaving(false);
    }
  }

  async function handleRevert() {
    if (!world || isNew) {
      return;
    }
    try {
      setReverting(true);
      setError(null);
      const [worldData, characterData] = await Promise.all([
        fetchWorld(world.id),
        fetchWorldCharacters(world.id),
      ]);
      const normalizedWorld = {
        ...worldData,
        time_config: normalizeTimeConfig(worldData.time_config as Record<string, unknown>),
        director_config: normalizeDirectorConfig(worldData.director_config as Record<string, unknown>),
        ui_theme_config: buildUiThemeEnvelope(normalizeUiThemeConfig(worldData.ui_theme_config as Record<string, unknown>)),
        opening_messages: normalizeOpeningMessages(worldData.opening_messages),
        opening_character_ids: normalizeOpeningCharacterIds(worldData.opening_character_ids, characterData),
      };
      setWorld(normalizedWorld);
      setMapTopologySource(formatMapTopologyJson(normalizedWorld.map_nodes));
      setSavedSignature(`${JSON.stringify(normalizedWorld)}|${formatMapTopologyJson(normalizedWorld.map_nodes)}`);
      cancelEditOpeningMessage();
      setOpeningComposerOpen(false);
      showToast("已还原为上次保存的内容");
    } catch (revertError) {
      setError(revertError instanceof Error ? revertError.message : "还原世界失败");
    } finally {
      setReverting(false);
    }
  }

  useEffect(() => {
    function handleShortcut(event: KeyboardEvent) {
      if (!(event.metaKey || event.ctrlKey) || event.key.toLowerCase() !== "s") {
        return;
      }
      event.preventDefault();
      if (!saving && !deleting && !exporting && world) {
        void handleSave();
      }
    }
    window.addEventListener("keydown", handleShortcut);
    return () => window.removeEventListener("keydown", handleShortcut);
  }, [deleting, exporting, saving, world]);

  useEffect(() => {
    if (!isDirty) {
      return;
    }
    function handleBeforeUnload(event: BeforeUnloadEvent) {
      event.preventDefault();
      event.returnValue = "";
    }
    window.addEventListener("beforeunload", handleBeforeUnload);
    return () => window.removeEventListener("beforeunload", handleBeforeUnload);
  }, [isDirty]);

  useEffect(() => {
    const sheetOpen = isMobile && activeSection === "opening" && (editingOpeningIndex !== null || openingComposerOpen);
    if (!sheetOpen) {
      return;
    }
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = previousOverflow;
    };
  }, [activeSection, editingOpeningIndex, isMobile, openingComposerOpen]);

  useEffect(() => {
    if (!isMobile || !activeSection) {
      return;
    }
    document
      .querySelectorAll<HTMLElement>(".mobile-content, .layout-inner")
      .forEach((element) => {
        element.scrollTop = 0;
      });
  }, [activeSection, isMobile]);

  useEffect(() => {
    if (directorPromptDraft === null) {
      return;
    }
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = previousOverflow;
    };
  }, [directorPromptDraft]);

  async function handleDelete() {
    if (!world || isNew) {
      return;
    }
    try {
      setDeleting(true);
      setError(null);
      await deleteWorld(world.id);
      setShowDeleteDialog(false);
      navigate("/worlds");
    } catch (deleteError) {
      setError(deleteError instanceof Error ? deleteError.message : "删除世界失败");
    } finally {
      setDeleting(false);
    }
  }

  async function handleExport() {
    if (!world || isNew) {
      return;
    }
    try {
      setExporting(true);
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
        showToast("世界包已导出");
      } else {
        showToast(`世界包已保存到：${savedPath ?? filename}`);
      }
    } catch (exportError) {
      setError(exportError instanceof Error ? exportError.message : "导出世界包失败");
    } finally {
      setExporting(false);
    }
  }

  // Mobile-specific navigation
  function mobileScrollers(): HTMLElement[] {
    return Array.from(
      document.querySelectorAll<HTMLElement>(".mobile-content, .layout-inner"),
    );
  }

  function openSection(sectionId: FixedTabId) {
    mobileListScrollRef.current = mobileScrollers().reduce(
      (max, element) => Math.max(max, element.scrollTop),
      0,
    );
    cancelEditOpeningMessage();
    setOpeningComposerOpen(false);
    openSectionParam(sectionId);
    setError(null);
  }

  function handleDetailBack() {
    cancelEditOpeningMessage();
    setOpeningComposerOpen(false);
    // activeSection 由 ?section= 驱动：有应用内历史时走历史回退（与侧滑/系统返回
    // 一致），无历史（首屏深链/刷新进入）时清除 ?section= 回到列表，避免离开应用。
    closeSection();
    const restoreListScroll = () => {
      mobileScrollers().forEach((element) => {
        element.scrollTop = mobileListScrollRef.current;
      });
    };
    window.requestAnimationFrame(() => window.requestAnimationFrame(restoreListScroll));
  }

  const mobileSectionLabel = activeSection
    ? fixedTabs.find((tab) => tab.id === activeSection)?.label ?? "世界编辑"
    : "";
  function performMobileBack() {
    setMobileMenuOpen(false);
    if (activeSection) {
      handleDetailBack();
      return;
    }
    navigate("/worlds");
  }
  function requestMobileBack() {
    setMobileMenuOpen(false);
    // 分区详情返回列表不会丢草稿（仍保留未保存标记），只有离开编辑页才需要确认。
    if (isDirty && !activeSection) {
      setShowLeaveDialog(true);
      return;
    }
    performMobileBack();
  }

  // ==================== Mobile-specific rendering ====================
  function renderMobileSectionList() {
    if (!world) {
      return null;
    }
    const groups: Array<{ label: string; ids: FixedTabId[] }> = [
      { label: "世界内容", ids: ["basic", "background", "opening", "map"] },
      { label: "运行与提示词", ids: ["director", "runtimeContext", "promptPreview"] },
      { label: "界面与规则", ids: ["style", "returnRules", "configPreview"] },
      { label: "高级", ids: ["customAttributes"] },
    ];
    const summary = (id: FixedTabId) => {
      switch (id) {
        case "basic":
          return world.genre.trim() || "名称、类型与概述";
        case "background":
          return world.background_prompt.trim() ? `${world.background_prompt.length} 字` : "尚未填写";
        case "opening":
          return `${openingMessages.length} 条消息 · ${openingCharacterIds.length} 位 NPC`;
        case "map":
          return `${Array.isArray(world.map_nodes) ? world.map_nodes.length : 0} 个节点`;
        case "runtimeContext":
          return directorConfig.runtime_context_prompt.trim() ? `${directorConfig.runtime_context_prompt.length} 字` : "使用内置默认";
        case "director":
          return `${directorConfig.service_mode === "agent_chat" ? "单智能体对话" : "世界模拟"} · ${directorConfig.world_director_prompt.length} 字提示词`;
        case "promptPreview":
          return "查看实际发送的 Prompt";
        case "style":
          return `${uiThemeConfig.local_background_assets.length} 张背景 · ${sceneNames.length} 个场景`;
        case "returnRules":
          return `${directorConfig.return_processing_rules.length} 条规则`;
        case "configPreview":
          return "查看完整配置 JSON";
        case "customAttributes":
          return "世界属性扩展";
        default:
          return "";
      }
    };
    return (
      <SurfacePanel className="surface-panel--pad-lg">
        <div className="wed-m-list">
          <div className="wed-m-hero">
            <div>
              <div className="wed-m-hero-name">{world.name.trim() || "未命名世界"}</div>
              <div className="wed-m-hero-meta">
                {world.genre.trim() || "未设置类型"} · {characters.length} 个角色 · {sceneNames.length} 个场景
              </div>
            </div>
            <span className={isDirty ? "wed-dirty" : "wed-m-saved"}>{isDirty ? "有未保存修改" : "已保存"}</span>
            {isDirty ? (
              <button
                type="button"
                className="wed-m-hero-save"
                onClick={() => void handleSave()}
                disabled={saving || deleting || exporting}
              >
                <Save size={14} /> {saving ? "保存中..." : "保存修改"}
              </button>
            ) : null}
          </div>
          {groups.map((group) => (
            <section key={group.label} className="wed-m-group">
              <div className="wed-m-group-title">{group.label}</div>
              <div className="wed-m-cards">
                {group.ids.map((id) => {
                  const tab = fixedTabs.find((item) => item.id === id);
                  if (!tab) {
                    return null;
                  }
                  return (
                    <button key={id} type="button" className="wed-m-card" onClick={() => openSection(id)}>
                      <span className="wed-m-card-copy">
                        <span className="t">{tab.label}</span>
                        <span className="s">{summary(id)}</span>
                      </span>
                      <ChevronRight size={16} aria-hidden="true" />
                    </button>
                  );
                })}
              </div>
            </section>
          ))}
        </div>
      </SurfacePanel>
    );
  }

  function renderMobileSectionContent(sectionOverride?: FixedTabId, embedded = false) {
    const sectionId = sectionOverride ?? activeSection;
    if (!sectionId || !world) return null;

    const mobileSaveBar = !embedded ? (
      <div className="mobile-editor-actionbar">
        <button
          type="button"
          onClick={() => void handleRevert()}
          disabled={reverting || saving || deleting || exporting || !isDirty || isNew}
          className="action-btn"
        >
          <RotateCcw size={15} />
          取消
        </button>
        <button
          type="button"
          onClick={() => void handleSave()}
          disabled={saving || deleting || exporting}
          className="action-btn action-btn--accent"
        >
          <Save size={15} />
          {saving ? "保存中..." : "保存修改"}
        </button>
      </div>
    ) : null;
    return (
      <div className={`editor-content${sectionId === "director" && !isMobile ? " editor-content--fill" : ""}`}>

        {sectionId === "basic" ? (
          <SurfacePanel className="surface-panel--pad-lg">
            <div className="editor-content">
              <label className="editor-field"><span className="editor-field-label">世界名称</span><input value={world.name} onChange={(e) => updateDraft({ name: e.target.value })} className="editor-field-input" /></label>
              <label className="editor-field"><span className="editor-field-label">世界类型</span><input value={world.genre} onChange={(e) => updateDraft({ genre: e.target.value })} className="editor-field-input" /></label>
              <label className="editor-field"><span className="editor-field-label">世界概述</span><textarea value={world.summary} onChange={(e) => updateDraft({ summary: e.target.value })} className="editor-field-input editor-field-textarea" /></label>
              {!isNew ? (
                <>
                  <label className="editor-field">
                    <span className="editor-field-label">默认玩家操控角色</span>
                    <select value={world.player_character_id ?? ""} onChange={(e) => updateDraft({ player_character_id: e.target.value || null })} className="editor-field-input editor-field-select">
                      <option value="">不指定，进入世界后由玩家直接行动</option>
                      {characters.map((character) => <option key={character.id} value={character.id}>{character.name}</option>)}
                    </select>
                  </label>
                  <div className="text-muted" style={{ marginTop: -8 }}>{selectedPlayerCharacter ? `当前默认身份：${selectedPlayerCharacter.name}` : "当前默认身份：未指定"}</div>
                </>
              ) : null}
            </div>
          </SurfacePanel>
        ) : null}

        {sectionId === "background" ? (
          <SurfacePanel className="surface-panel--pad-lg">
            <div className="editor-content">
              <label className="editor-field">
                <span className="editor-field-label">世界背景设定</span>
                <textarea value={world.background_prompt} onChange={(e) => updateDraft({ background_prompt: e.target.value })} className="editor-field-input editor-field-textarea" style={{ minHeight: 220 }} />
              </label>
            </div>
          </SurfacePanel>
        ) : null}

        {sectionId === "opening" ? (
          <SurfacePanel className="surface-panel--pad-lg">
            <div className="wed-shell">
              <div className="wed-toolbar">
                <div className="wed-seg" role="tablist" aria-label="开场配置视图">
                  <button type="button" role="tab" aria-selected={openingMode === "organize"} onClick={() => setOpeningMode("organize")}>消息编排</button>
                  <button type="button" role="tab" aria-selected={openingMode === "preview"} onClick={() => setOpeningMode("preview")}>开场预览</button>
                </div>
                <span className="wed-meta">{openingMessages.length} 条消息 · {openingCharacterIds.length} 位在场 NPC</span>
              </div>

              {openingMode === "organize" ? (
                <>
                  <div className="wed-panel">
                    <div className="wed-panel-head">
                      <div>
                        <div className="wed-panel-title">开场在场 NPC</div>
                        <div className="wed-panel-sub">决定开局同时出现的角色，可搜索后点选</div>
                      </div>
                      <span className="wed-count">{openingCharacterIds.length} 位</span>
                    </div>
                    <div className="wed-panel-body">
                      {characters.length === 0 ? (
                        <div className="text-muted">当前世界还没有角色，先去角色池创建角色。</div>
                      ) : (
                        <>
                          <div className="wed-search">
                            <Search size={14} aria-hidden="true" />
                            <input
                              type="search"
                              value={openingNpcSearch}
                              onChange={(event) => setOpeningNpcSearch(event.target.value)}
                              placeholder="搜索角色名或身份"
                              className="editor-field-input"
                              aria-label="搜索角色"
                            />
                          </div>
                          <div className="wed-chips" role="group" aria-label="可选角色">
                            {filteredOpeningCharacters.map((character) => {
                              const checked = openingCharacterIds.includes(character.id);
                              return (
                                <button
                                  key={character.id}
                                  type="button"
                                  className="wed-chip"
                                  aria-pressed={checked}
                                  title={character.role || "未设置身份"}
                                  onClick={() => toggleOpeningCharacter(character.id)}
                                >
                                  {checked ? <Check size={13} aria-hidden="true" /> : null}
                                  {character.name}
                                </button>
                              );
                            })}
                          </div>
                          {filteredOpeningCharacters.length === 0 ? (
                            <div className="text-muted">没有匹配的角色，清空搜索词可查看全部。</div>
                          ) : null}
                        </>
                      )}
                    </div>
                  </div>

                  <div className="wed-open-grid">
                    <div className="wed-panel">
                      <div className="wed-panel-head">
                        <div>
                          <div className="wed-panel-title">开场消息</div>
                          <div className="wed-panel-sub">拖动或用箭头调整顺序，点击任意一条在右侧编辑</div>
                        </div>
                        <button
                          type="button"
                          className="action-btn action-btn--accent"
                          onClick={() => {
                            setEditingOpeningIndex(null);
                            setOpeningComposerOpen(true);
                          }}
                        >
                          <Plus size={14} aria-hidden="true" /> 添加消息
                        </button>
                      </div>
                      <div className="wed-panel-body">
                        {openingMessages.length === 0 ? (
                          <div className="wed-empty">
                            <div className="wed-empty-title">还没有开场消息</div>
                            <div>添加第一条旁白或角色台词，玩家进入世界时会按顺序播放。</div>
                          </div>
                        ) : (
                          <ol className="wed-timeline" aria-label="开场消息列表">
                            {openingMessages.map((message, index) => {
                              const selected = editingOpeningIndex === index;
                              return (
                                <li
                                  key={`opening-msg-${index}`}
                                  className={`wed-msg${selected ? " is-selected" : ""}${dragMessageIndex === index ? " is-dragging" : ""}`}
                                  draggable
                                  aria-selected={selected}
                                  onClick={() => startEditOpeningMessage(index)}
                                  onDragStart={() => setDragMessageIndex(index)}
                                  onDragOver={(event) => event.preventDefault()}
                                  onDrop={(event) => {
                                    event.preventDefault();
                                    if (dragMessageIndex !== null) {
                                      reorderOpeningMessages(dragMessageIndex, index);
                                    }
                                    setDragMessageIndex(null);
                                  }}
                                  onDragEnd={() => setDragMessageIndex(null)}
                                >
                                  <span className="wed-msg-grip" aria-hidden="true"><GripVertical size={14} /></span>
                                  <span className="wed-msg-index">{index + 1}</span>
                                  <span className="wed-msg-body">
                                    <span className="wed-msg-role">
                                      {message.role === "agent" ? resolveOpeningSpeakerLabel(message) : "旁白"}
                                    </span>
                                    <span className="wed-msg-text">{message.content}</span>
                                  </span>
                                  <span className="wed-msg-actions">
                                    <button
                                      type="button"
                                      className="wed-icon-btn"
                                      aria-label="上移这条消息"
                                      disabled={index === 0}
                                      onClick={(event) => { event.stopPropagation(); moveOpeningMessage(index, -1); }}
                                    >
                                      <ArrowUp size={14} />
                                    </button>
                                    <button
                                      type="button"
                                      className="wed-icon-btn"
                                      aria-label="下移这条消息"
                                      disabled={index === openingMessages.length - 1}
                                      onClick={(event) => { event.stopPropagation(); moveOpeningMessage(index, 1); }}
                                    >
                                      <ArrowDown size={14} />
                                    </button>
                                    <button
                                      type="button"
                                      className="wed-icon-btn"
                                      aria-label="编辑这条消息"
                                      onClick={(event) => { event.stopPropagation(); startEditOpeningMessage(index); }}
                                    >
                                      <Pencil size={14} />
                                    </button>
                                    <button
                                      type="button"
                                      className="wed-icon-btn wed-icon-btn--danger"
                                      aria-label="删除这条消息"
                                      onClick={(event) => { event.stopPropagation(); removeOpeningMessage(index); }}
                                    >
                                      <Trash2 size={14} />
                                    </button>
                                  </span>
                                </li>
                              );
                            })}
                          </ol>
                        )}
                      </div>
                    </div>

                    {(editingOpeningIndex !== null || openingComposerOpen) ? (
                      <div
                        className="wed-sheet-backdrop"
                        aria-hidden="true"
                        onClick={() => {
                          cancelEditOpeningMessage();
                          setOpeningComposerOpen(false);
                        }}
                      />
                    ) : null}
                    <aside
                      className={`wed-panel wed-inspector${editingOpeningIndex !== null || openingComposerOpen ? " wed-inspector--sheet" : " wed-inspector--idle"}`}
                      aria-label="消息编辑"
                    >
                      <div className="wed-sheet-handle" aria-hidden="true" />
                      {editingOpeningIndex !== null ? (
                        <>
                          <div className="wed-panel-head">
                            <div>
                              <div className="wed-panel-title">编辑第 {editingOpeningIndex + 1} 条消息</div>
                              <div className="wed-panel-sub">修改后保存，不会影响已生成的存档</div>
                            </div>
                            <button type="button" className="wed-icon-btn" aria-label="关闭检查器" onClick={cancelEditOpeningMessage}>
                              <ArrowLeft size={14} />
                            </button>
                          </div>
                          <div className="wed-panel-body">
                            <div className="wed-field-grid">
                              <label className="editor-field">
                                <span className="editor-field-label">发言身份</span>
                                <select
                                  value={openingEditRole}
                                  onChange={(e) => {
                                    const nextRole = e.target.value as OpeningComposerRole;
                                    setOpeningEditRole(nextRole);
                                    if (nextRole !== "agent") {
                                      setOpeningEditSpeaker("");
                                    }
                                  }}
                                  className="editor-field-input editor-field-select"
                                >
                                  <option value="system">系统旁白</option>
                                  <option value="agent">角色</option>
                                </select>
                              </label>
                              {openingEditRole === "agent" ? (
                                <label className="editor-field">
                                  <span className="editor-field-label">角色</span>
                                  <select
                                    value={openingEditSpeaker}
                                    onChange={(e) => setOpeningEditSpeaker(e.target.value)}
                                    className="editor-field-input editor-field-select"
                                  >
                                    <option value="">{openingSpeakerOptions[0]?.label ?? "选择角色"}</option>
                                    {openingSpeakerOptions.map((option) => (
                                      <option key={option.value} value={option.value}>{option.label}</option>
                                    ))}
                                  </select>
                                </label>
                              ) : null}
                            </div>
                            <label className="editor-field">
                              <span className="editor-field-label">开场内容</span>
                              <textarea
                                value={openingEditContent}
                                onChange={(e) => setOpeningEditContent(e.target.value)}
                                className="editor-field-input editor-field-textarea"
                                style={{ minHeight: 180 }}
                                placeholder="修改这条开场消息的内容。"
                              />
                            </label>
                            <div className="wed-inspector-actions">
                              <button type="button" className="action-btn action-btn--accent" onClick={saveOpeningMessage}>保存修改</button>
                              <button type="button" className="action-btn" onClick={cancelEditOpeningMessage}>取消</button>
                            </div>
                          </div>
                        </>
                      ) : openingComposerOpen ? (
                        <>
                          <div className="wed-panel-head">
                            <div>
                              <div className="wed-panel-title">添加开场消息</div>
                              <div className="wed-panel-sub">新消息会追加到当前顺序末尾</div>
                            </div>
                          </div>
                          <div className="wed-panel-body">
                            <div className="wed-field-grid">
                              <label className="editor-field">
                                <span className="editor-field-label">发言身份</span>
                                <select
                                  value={openingComposerRole}
                                  onChange={(e) => {
                                    const nextRole = e.target.value as OpeningComposerRole;
                                    setOpeningComposerRole(nextRole);
                                    if (nextRole !== "agent") {
                                      setOpeningComposerSpeaker("");
                                    }
                                  }}
                                  className="editor-field-input editor-field-select"
                                >
                                  <option value="system">系统旁白</option>
                                  <option value="agent">角色</option>
                                </select>
                              </label>
                              {openingComposerRole === "agent" ? (
                                <label className="editor-field">
                                  <span className="editor-field-label">角色</span>
                                  <select
                                    value={openingComposerSpeaker}
                                    onChange={(e) => setOpeningComposerSpeaker(e.target.value)}
                                    className="editor-field-input editor-field-select"
                                  >
                                    <option value="">{openingSpeakerOptions[0]?.label ?? "选择角色"}</option>
                                    {openingSpeakerOptions.map((option) => (
                                      <option key={option.value} value={option.value}>{option.label}</option>
                                    ))}
                                  </select>
                                </label>
                              ) : null}
                            </div>
                            <label className="editor-field">
                              <span className="editor-field-label">开场内容</span>
                              <textarea
                                value={openingComposerContent}
                                onChange={(e) => setOpeningComposerContent(e.target.value)}
                                className="editor-field-input editor-field-textarea"
                                style={{ minHeight: 180 }}
                                placeholder="写一段开场旁白，或让指定角色说出第一句话。"
                              />
                            </label>
                            <div className="wed-inspector-actions">
                              <button type="button" className="action-btn action-btn--accent" onClick={addOpeningMessage}>加入开场聊天</button>
                              <button type="button" className="action-btn" onClick={() => setOpeningComposerOpen(false)}>取消</button>
                            </div>
                          </div>
                        </>
                      ) : (
                        <div className="wed-empty wed-empty--inspector">
                          <div className="wed-empty-title">选择一条消息开始编辑</div>
                          <div>左侧点选消息，或新建一条旁白 / 台词。草稿会随世界一起保存。</div>
                          <button
                            type="button"
                            className="action-btn"
                            onClick={() => {
                              setEditingOpeningIndex(null);
                              setOpeningComposerOpen(true);
                            }}
                          >
                            <Plus size={14} aria-hidden="true" /> 添加消息
                          </button>
                        </div>
                      )}
                    </aside>
                  </div>
                </>
              ) : (
                <div className="wed-open-grid">
                  <div className="wed-panel">
                    <div className="wed-panel-head">
                      <div>
                        <div className="wed-panel-title">开场预览</div>
                        <div className="wed-panel-sub">按当前消息顺序模拟玩家进入世界的第一屏</div>
                      </div>
                    </div>
                    <div className="wed-panel-body">
                      <div className="wed-preview-stage">
                        <div className="wed-preview-scene">
                          <span className="wed-preview-where">{world.name || "未命名世界"} · {world.opening_scene || sceneNames[0] || "未命名场景"}</span>
                          <strong>{world.opening_scene || sceneNames[0] || "未命名场景"}</strong>
                        </div>
                        <div className="wed-preview-messages">
                          {openingMessages.length === 0 ? (
                            <div className="text-muted">当前还没有开场消息。</div>
                          ) : (
                            openingMessages.map((message, index) => (
                              <div
                                key={`opening-preview-${index}`}
                                className={`wed-bubble wed-bubble--${message.role === "agent" ? "agent" : "system"}`}
                              >
                                <span className="wed-bubble-who">
                                  {message.role === "agent" ? resolveOpeningSpeakerLabel(message) : "旁白"}
                                </span>
                                <span className="wed-bubble-text">{message.content}</span>
                              </div>
                            ))
                          )}
                        </div>
                      </div>
                    </div>
                  </div>
                  <aside className="wed-panel">
                    <div className="wed-panel-head">
                      <div className="wed-panel-title">配置摘要</div>
                    </div>
                    <div className="wed-panel-body">
                      <ul className="wed-side-list">
                        <li><span>消息条数</span><strong>{openingMessages.length} 条</strong></li>
                        <li><span>在场角色</span><strong>{openingSceneCharacters.length ? openingSceneCharacters.map((character) => character.name).join("、") : "无"}</strong></li>
                        <li><span>开场场景</span><strong>{world.opening_scene || sceneNames[0] || "未设定"}</strong></li>
                      </ul>
                    </div>
                  </aside>
                </div>
              )}
            </div>
          </SurfacePanel>
        ) : null}

        {sectionId === "map" ? (
          <SurfacePanel className="surface-panel--pad-lg">
            <div className="editor-content">
              <label className="editor-field"><span className="editor-field-label">开场地点</span><input value={world.opening_scene} onChange={(e) => updateDraft({ opening_scene: e.target.value })} className="editor-field-input" /></label>
              <label className="editor-field">
                <span className="editor-field-label">地图拓扑 JSON</span>
                <textarea value={mapNodesText} onChange={(e) => updateMapTopologySource(e.target.value)} className="editor-field-input editor-field-textarea" style={{ minHeight: 180 }} />
              </label>
            </div>
          </SurfacePanel>
        ) : null}

        {sectionId === "customAttributes" ? (
          <SurfacePanel className="surface-panel--pad-lg">
            <div className="editor-content">
              <AttributePanel
                scope="world"
                ownerType="world"
                ownerId={isNew ? undefined : world.id}
                worldId={isNew ? undefined : world.id}
                onSchemaChange={() => void reloadPromptPreview()}
              />
            </div>
          </SurfacePanel>
        ) : null}

        {sectionId === "runtimeContext" ? (
          <SurfacePanel className="surface-panel--pad-lg">
            <div className="editor-content">
              <label className="editor-field">
                <span className="editor-field-label">运行时上下文</span>
                <div className="text-muted" style={{ fontSize: 12 }}>
                  支持 {"{{变量}}"} 插值：内置变量与下方插值管理中的自定义变量。
                </div>
                <textarea
                  value={directorConfig.runtime_context_prompt}
                  onChange={(e) => updateDirectorPatch({ runtime_context_prompt: e.target.value })}
                  className="editor-field-input editor-field-textarea"
                  style={{ minHeight: 300 }}
                />
              </label>
              <label className="editor-field">
                <span className="editor-field-label" style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  主控返回格式
                  <button
                    type="button"
                    className="action-btn"
                    onClick={() => void fillDefaultDirectorResponseContract()}
                  >
                    填入内置契约
                  </button>
                </span>
                <div className="text-muted" style={{ fontSize: 12 }}>
                  留空使用框架内置返回格式；填写后<b>完全替代</b>内置契约，作者自行控制发给模型的全部格式说明。
                  点"填入内置契约"可把当前默认契约原文填进来再改。需自行包含 planned_speakers 等引擎必需字段的格式，否则对应功能会退化。
                </div>
                <textarea
                  value={directorConfig.response_contract_prompt}
                  onChange={(e) => updateDirectorPatch({ response_contract_prompt: e.target.value })}
                  className="editor-field-input editor-field-textarea"
                  style={{ minHeight: 200 }}
                />
              </label>
              <div className="editor-field">
                <div className="editor-field-label">插值管理（自定义提示词变量）</div>
                <div className="text-muted" style={{ fontSize: 12, marginBottom: 8 }}>
                  在运行时上下文、返回格式、模型提示词设置、系统提示词、背景、旁白等输入框里用 {"{{变量名}}"} 引用。自定义变量优先于内置变量，变量值里还可以再引用内置变量。内置变量：
                  {"{{time}}"}（当前时间）、{"{{date}}"}、{"{{user}}"} / {"{{player}}"}（玩家角色名）、
                  {"{{world_name}}"}、{"{{location}}"}、{"{{scene}}"}、{"{{random:选项1,选项2}}"}（随机选一）、
                  {"{{var:key}}"}（会话运行时变量）。
                </div>
                <div style={{ display: "grid", gap: 8 }}>
                  {Object.entries(directorConfig.prompt_variables)
                    .sort(([a], [b]) => a.localeCompare(b))
                    .map(([key, value]) => (
                      <div key={key} style={{ display: "grid", gridTemplateColumns: "220px 1fr auto", gap: 8, alignItems: "center" }}>
                        <input
                          type="text"
                          className="editor-field-input"
                          value={key}
                          placeholder="变量名，如 nickname"
                          onChange={(event) => {
                            const newKey = event.target.value.trim();
                            const next: Record<string, string> = {};
                            for (const [existingKey, existingValue] of Object.entries(directorConfig.prompt_variables)) {
                              next[existingKey === key ? (newKey || existingKey) : existingKey] = existingValue;
                            }
                            updateDirectorPatch({ prompt_variables: next });
                          }}
                        />
                        <input
                          type="text"
                          className="editor-field-input"
                          value={value}
                          placeholder='替换成的文本，可再引用 {{user}} 等内置变量'
                          onChange={(event) =>
                            updateDirectorPatch({
                              prompt_variables: { ...directorConfig.prompt_variables, [key]: event.target.value },
                            })
                          }
                        />
                        <button
                          type="button"
                          className="action-btn"
                          onClick={() => {
                            const next = { ...directorConfig.prompt_variables };
                            delete next[key];
                            updateDirectorPatch({ prompt_variables: next });
                          }}
                        >
                          删除
                        </button>
                      </div>
                    ))}
                  <div>
                    <button
                      type="button"
                      className="action-btn"
                      onClick={() => {
                        let key = "var1";
                        let index = 1;
                        while (directorConfig.prompt_variables[key] !== undefined) {
                          index += 1;
                          key = `var${index}`;
                        }
                        updateDirectorPatch({
                          prompt_variables: { ...directorConfig.prompt_variables, [key]: "" },
                        });
                      }}
                    >
                      添加变量
                    </button>
                  </div>
                </div>
              </div>
            </div>
          </SurfacePanel>
        ) : null}

        {sectionId === "director" ? (
          <SurfacePanel className={`surface-panel--pad-lg${isMobile ? "" : " surface-panel--fill"}`}>
            <div className="wed-shell">
              <div className="wed-summary" role="list" aria-label="主控配置摘要">
                {[
                  { section: "mode" as DirectorPaneId, k: "运行模式", v: directorConfig.service_mode === "agent_chat" ? "单智能体对话" : "世界模拟" },
                  { section: "mode" as DirectorPaneId, k: "记忆模式", v: directorConfig.memory_mode === "oag" ? "OAG" : directorConfig.memory_mode === "embedding" ? "Embedding 记忆" : "历史对话" },
                  { section: "context" as DirectorPaneId, k: "历史对话", v: `${directorConfig.history_dialogue_rounds} 轮 · 循环 ${directorConfig.director_tool_loop_limit}` },
                  { section: "prompt" as DirectorPaneId, k: "模型提示词", v: `${directorConfig.world_director_prompt.length} 字` },
                  { section: "tools" as DirectorPaneId, k: "工具授权", v: `${authorizedToolCount} 个` },
                ].map((item) => (
                  <button key={item.k} type="button" role="listitem" className="wed-summary-item" onClick={() => setDirectorPane(item.section)}>
                    <span className="k">{item.k}</span>
                    <span className="v">{item.v}</span>
                  </button>
                ))}
              </div>
              <div className="wed-grid">
                <nav className="wed-nav" aria-label="主控配置分区">
                  {DIRECTOR_NAV_ITEMS.map((item) => {
                    const hint =
                      item.id === "mode" ? (directorConfig.service_mode === "agent_chat" ? "单智能体对话" : "世界模拟")
                        : item.id === "permissions" ? `${directorOnlyToolCount}/4 项工具开启`
                          : item.id === "context" ? `历史 ${directorConfig.history_dialogue_rounds} 轮`
                            : item.id === "summaries" ? `每 ${directorConfig.memory_summary_interval_rounds} 回合`
                              : item.id === "prompt" ? `${directorConfig.world_director_prompt.length} 字`
                                : `${authorizedToolCount} 个已授权`;
                    return (
                      <button
                        key={item.id}
                        type="button"
                        className="wed-nav-item"
                        aria-current={directorPane === item.id ? "true" : "false"}
                        onClick={() => setDirectorPane(item.id)}
                      >
                        <span className="t">{item.label}</span>
                        <span className="s">{hint}</span>
                      </button>
                    );
                  })}
                </nav>
                <div className="wed-pane">
                  <div className="wed-pane-head">
                    <div className="field-label-head">
                      <h2 className="wed-pane-title">世界主控</h2>
                      {DIRECTOR_PANE_META[directorPane].help ? (
                        <HelpTip text={DIRECTOR_PANE_META[directorPane].help} />
                      ) : null}
                    </div>
                    {directorPane === "prompt" ? (
                      <div className="wed-pane-actions">
                        <span className="wed-meta">{directorConfig.world_director_prompt.length} 字</span>
                        <button type="button" className="action-btn" onClick={() => setDirectorPromptDraft(directorConfig.world_director_prompt)}>全屏编辑</button>
                      </div>
                    ) : null}
                  </div>
                  {(directorPane === "permissions" || directorPane === "context" || directorPane === "prompt") && directorConfig.service_mode !== "world_sim" ? (
                    <div className="wed-empty">当前为「单智能体对话」模式，本分区仅在世界模拟模式下可用。</div>
                  ) : null}

                  {directorPane === "mode" ? (
                    <div className="wed-block">
                      <div className="wed-service-group">
                        <div className="editor-field">
                          <span className="editor-field-label">服务模式</span>
                          <div
                            className="wed-depth wed-depth--service"
                            style={{
                              "--depth-progress": `${SERVICE_MODE_TRACK_POSITIONS[serviceModeIndex]}%`,
                            } as CSSProperties}
                          >
                            <div className="wed-depth-control">
                              <button
                                type="button"
                                role="switch"
                                className="wed-depth-range wed-depth-range--toggle"
                                aria-label="服务模式"
                                aria-checked={directorConfig.service_mode === "agent_chat"}
                                onClick={() => updateServiceMode(
                                  directorConfig.service_mode === "agent_chat" ? "world_sim" : "agent_chat",
                                )}
                              />
                              <div className="wed-depth-visual" aria-hidden="true">
                                <span className="wed-depth-line" />
                                {SERVICE_MODE_OPTIONS.map((option, index) => (
                                  <span
                                    key={option.value}
                                    className="wed-depth-tick"
                                    style={{ left: `${SERVICE_MODE_TRACK_POSITIONS[index]}%` }}
                                  />
                                ))}
                                {SERVICE_MODE_OPTIONS.map((option, index) => (
                                  <span
                                    key={option.value}
                                    className={[
                                      "wed-depth-stop",
                                      index <= serviceModeIndex ? "is-reached" : "",
                                      index === serviceModeIndex ? "is-current" : "",
                                    ].filter(Boolean).join(" ")}
                                    style={{ left: `${SERVICE_MODE_TRACK_POSITIONS[index]}%` }}
                                  />
                                ))}
                              </div>
                            </div>
                            <div className="wed-depth-labels" aria-hidden="true">
                              {SERVICE_MODE_OPTIONS.map((option, index) => (
                                <span
                                  key={option.value}
                                  className={index === serviceModeIndex ? "is-current" : index < serviceModeIndex ? "is-passed" : ""}
                                  style={{ left: `${SERVICE_MODE_TRACK_POSITIONS[index]}%` }}
                                >
                                  {option.label}
                                </span>
                              ))}
                            </div>
                          </div>
                        </div>
                        <div className="wed-service-agent">
                          <label className="editor-field">
                            <span className="editor-field-label">默认回复角色</span>
                            <select
                              value={directorConfig.default_agent_id}
                              onChange={(e) => updateDirectorPatch({ default_agent_id: e.target.value })}
                              className="editor-field-input editor-field-select"
                              disabled={characters.length === 0 || directorConfig.service_mode !== "agent_chat"}
                            >
                              <option value="">自动选择</option>
                              {characters.map((character) => (
                                <option key={character.id} value={character.id}>
                                  {character.name}
                                </option>
                              ))}
                            </select>
                          </label>
                        </div>
                      </div>
                      <div className="editor-field wed-memory-field">
                      <div className="field-label-head">
                        <span className="editor-field-label">记忆模式</span>
                        <HelpTip text={MEMORY_MODE_OPTIONS[memoryModeIndex].help} />
                      </div>
                        <div
                          className="wed-depth wed-depth--memory"
                          style={{
                            "--depth-progress": `${MEMORY_MODE_TRACK_POSITIONS[memoryModeIndex]}%`,
                          } as CSSProperties}
                        >
                          <div className="wed-depth-hints" aria-hidden="true">
                            <span>Token 消耗低</span>
                            <span>记忆强度高</span>
                          </div>
                          <div className="wed-depth-control">
                            <input
                              className="wed-depth-range"
                              type="range"
                              min={0}
                              max={MEMORY_MODE_OPTIONS.length - 1}
                              step={1}
                              value={memoryModeIndex}
                              aria-label="记忆模式"
                              aria-valuetext={MEMORY_MODE_OPTIONS[memoryModeIndex].label}
                              onChange={(event) => {
                                const option = MEMORY_MODE_OPTIONS[Number(event.currentTarget.value)];
                                if (option) updateDirectorPatch({ memory_mode: option.value });
                              }}
                            />
                            <div className="wed-depth-visual" aria-hidden="true">
                              <span className="wed-depth-line" />
                              {MEMORY_MODE_OPTIONS.map((option, index) => (
                                <span
                                  key={option.value}
                                  className="wed-depth-tick"
                                  style={{ left: `${MEMORY_MODE_TRACK_POSITIONS[index]}%` }}
                                />
                              ))}
                              {MEMORY_MODE_OPTIONS.map((option, index) => (
                                <span
                                  key={option.value}
                                  className={[
                                    "wed-depth-stop",
                                    index <= memoryModeIndex ? "is-reached" : "",
                                    index === memoryModeIndex ? "is-current" : "",
                                  ].filter(Boolean).join(" ")}
                                  style={{ left: `${MEMORY_MODE_TRACK_POSITIONS[index]}%` }}
                                />
                              ))}
                            </div>
                          </div>
                          <div className="wed-depth-labels" aria-hidden="true">
                            {MEMORY_MODE_OPTIONS.map((option, index) => (
                              <span
                                key={option.value}
                                className={index === memoryModeIndex ? "is-current" : index < memoryModeIndex ? "is-passed" : ""}
                                style={{ left: `${MEMORY_MODE_TRACK_POSITIONS[index]}%` }}
                              >
                                {option.label}
                              </span>
                            ))}
                          </div>
                        </div>
                      </div>
                    </div>
                  ) : null}

                  {directorPane === "permissions" ? (
                    <>
                      {directorConfig.service_mode === "world_sim" ? (
                        <div className="wed-block">
                          <div className="wed-block-title">主控专用工具</div>
                          <div className="settings-form-grid">
                            {[
                              { id: MCP_TOOL_LIST_SCENES_ID, label: "列出场景" },
                              { id: MCP_TOOL_LIST_CHARACTERS_ID, label: "列出角色" },
                              { id: MCP_TOOL_CHANGE_SCENE_ID, label: "切换场景" },
                              { id: MCP_TOOL_SWITCH_PLAYER_CHARACTER_ID, label: "切换玩家角色" },
                            ].map((tool) => (
                              <label key={tool.id} className="editor-field">
                                <span className="editor-field-label">{tool.label}</span>
                                <div className="settings-inline-toggle">
                                  <input
                                    type="checkbox"
                                    checked={directorConfig.allowed_mcp_tool_ids.includes(tool.id)}
                                    onChange={() => toggleAllowedMcpTool(tool.id)}
                                  />
                                </div>
                              </label>
                            ))}
                          </div>
                          <div className="editor-field" style={{ marginTop: 16 }}>
                            <div className="field-label-head">
                              <span className="editor-field-label">允许世界主控 AI 生图</span>
                              <HelpTip text="一个开关同时控制运行时生图和两个主控专用工具。开启后，世界主控可按剧情生成场景背景或为指定角色生成立绘；关闭后不会调用图像模型，已上传的本地素材仍可正常使用。" />
                            </div>
                            <div className="settings-inline-toggle">
                              <input
                                type="checkbox"
                                aria-label="允许世界主控 AI 生图"
                                checked={directorImageGenerationEnabled}
                                onChange={(event) => toggleDirectorImageGeneration(event.target.checked)}
                              />
                            </div>
                          </div>
                          <div className="settings-form-grid">
                            <label className="editor-field">
                              <span className="editor-field-label">允许生成新 NPC</span>
                              <div className="settings-inline-toggle">
                                <input type="checkbox" checked={directorConfig.allow_npc_spawn} onChange={(e) => updateDirectorPatch({ allow_npc_spawn: e.target.checked })} />
                              </div>
                            </label>
                          </div>
                        </div>
              ) : null}

              {directorConfig.service_mode === "world_sim" && directorPane === "permissions" ? (
                <div className="wed-block">
                  <div className="wed-block-title field-label-head">
                    <span>可查询的知识库</span>
                    <HelpTip text="勾选主控可查的知识库范围。实际检索还需在「MCP 工具授权」中授予知识库检索工具；角色可在「角色编辑 → 记忆」里进一步收窄。" />
                  </div>
                  {knowledgeCandidates.length === 0 ? (
                    <div className="text-muted">还没有可用知识库。可在「图库和知识库」新建后回到这里勾选。</div>
                  ) : (
                    <div className="editor-interaction-options">
                      {knowledgeCandidates.map((item) => (
                        <label key={item.id} className="editor-interaction-option">
                          <input
                            type="checkbox"
                            checked={directorConfig.knowledge_base_ids.includes(item.id)}
                            onChange={() => {
                              const current = new Set(directorConfig.knowledge_base_ids);
                              if (current.has(item.id)) {
                                current.delete(item.id);
                              } else {
                                current.add(item.id);
                              }
                              updateDirectorPatch({ knowledge_base_ids: Array.from(current) });
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
              ) : null}

              {directorConfig.service_mode === "world_sim" ? (
                <div className="wed-block">
                  <div className="wed-block-title">行动选项</div>
                  <fieldset className="editor-interaction-source">
                    <legend>世界主控提供</legend>
                    <div className="editor-interaction-options">
                      {interactionKindOptions.map((option) => (
                        <label key={option.value} className="editor-interaction-option">
                          <input
                            type="checkbox"
                            checked={directorConfig.director_interaction_kinds.includes(option.value)}
                            onChange={() => toggleInteractionKind("director_interaction_kinds", option.value)}
                          />
                          <span>{option.label}</span>
                        </label>
                      ))}
                    </div>
                  </fieldset>
                </div>
              ) : null}
                    </>
                  ) : null}

                  {directorPane === "context" ? (
                    <>
                      {directorConfig.service_mode === "world_sim" ? (
                        <div className="wed-block">
                  <div className="settings-form-grid">
                    <label className="editor-field">
                      <span className="editor-field-label">历史对话轮数</span>
                      <input type="number" min="0" value={directorConfig.history_dialogue_rounds} onChange={(e) => updateDirectorPatch({ history_dialogue_rounds: Number(e.target.value) })} className="editor-field-input" />
                    </label>
                    <label className="editor-field">
                      <span className="editor-field-label">工具循环上限</span>
                      <input type="number" min="1" max="12" value={directorConfig.director_tool_loop_limit} onChange={(e) => updateDirectorPatch({ director_tool_loop_limit: Number(e.target.value) })} className="editor-field-input" />
                    </label>
                  </div>
                        </div>
              ) : null}
                    </>
                  ) : null}

                  {directorPane === "summaries" ? (
                    <>
                      <div className="wed-block">
                        <div className="wed-block-title">长期记忆召回</div>
                        <div className="settings-form-grid">
                          <label className="editor-field">
                            <span className="editor-field-label">
                              <span className="field-label-head">
                                <span>召回片段上限</span>
                                <HelpTip text="每次最多注入几个不同回合的长期记忆。数值越大，NPC 能联系到的旧情节越多，但提示词也更长；这不是总结间隔。推荐 2-3。" />
                              </span>
                            </span>
                            <input type="number" min="1" max="6" value={directorConfig.character_memory_hit_turns} onChange={(e) => updateDirectorPatch({ character_memory_hit_turns: Number(e.target.value) })} className="editor-field-input" />
                          </label>
                          <label className="editor-field">
                            <span className="editor-field-label">
                              <span className="field-label-head">
                                <span>关联事件范围（回合）</span>
                                <HelpTip text="在召回的记忆回合前后各 N 回合补充相关事件。例如 N=10 会覆盖命中回合前后最多 21 回合；0 表示不补充事件。" />
                              </span>
                            </span>
                            <input type="number" min="0" max="20" value={directorConfig.character_memory_event_window_rounds} onChange={(e) => updateDirectorPatch({ character_memory_event_window_rounds: Number(e.target.value) })} className="editor-field-input" />
                          </label>
                          <label className="editor-field">
                            <span className="editor-field-label">
                              <span className="field-label-head">
                                <span>关联对话范围（回合）</span>
                                <HelpTip text="在召回的记忆回合前后各 N 回合补充 NPC 当时可见的对话。只注入它有机会知道的内容；0 表示不补充对话。" />
                              </span>
                            </span>
                            <input type="number" min="0" max="6" value={directorConfig.character_memory_dialogue_window_rounds} onChange={(e) => updateDirectorPatch({ character_memory_dialogue_window_rounds: Number(e.target.value) })} className="editor-field-input" />
                          </label>
                          <label className="editor-field">
                            <span className="editor-field-label">
                              <span className="field-label-head">
                                <span>语义匹配权重</span>
                                <HelpTip text="混合检索中语义匹配所占的权重。0 表示完全按关键词排序，1 表示完全按意思相似度排序；推荐 0.6-0.7。未配置可用向量模型时会自动退回关键词匹配。" />
                              </span>
                            </span>
                            <input type="number" min="0" max="1" step="0.05" value={directorConfig.character_memory_semantic_weight} onChange={(e) => updateDirectorPatch({ character_memory_semantic_weight: Number(e.target.value) })} className="editor-field-input" />
                          </label>
                        </div>
                      </div>
                      <div className="wed-block">
                        <div className="wed-block-title">自动总结</div>
                        <div className="settings-form-grid">
                          <label className="editor-field">
                            <span className="editor-field-label">
                              <span className="field-label-head">
                                <span>总结间隔（回合）</span>
                                <HelpTip text="每 N 回合结束后异步生成一次 NPC 和世界主控总结，默认 10 回合。数值越大，总结越少、单次内容越多，原始对话保留越久；推荐 10-20 回合。" />
                              </span>
                            </span>
                            <input type="number" min="1" max="100" value={directorConfig.memory_summary_interval_rounds} onChange={(e) => updateDirectorPatch({ memory_summary_interval_rounds: Number(e.target.value) })} className="editor-field-input" />
                          </label>
                          <label className="editor-field">
                            <span className="editor-field-label">
                              <span className="field-label-head">
                                <span>NPC 总结提示词</span>
                                <HelpTip text="留空恢复内置默认。按每个 NPC 可见的消息生成；即使模板未写 {{transcript}}，对话材料也会自动附在末尾。支持 {{character_name}}、{{scene_name}}、{{bucket_start}}、{{bucket_end}}、{{turn_start}}、{{turn_end}}、{{transcript}}。" />
                              </span>
                            </span>
                            <textarea
                              value={directorConfig.character_memory_summary_prompt || summaryPromptDefaults?.character || ""}
                              onChange={(e) => updateDirectorPatch({ character_memory_summary_prompt: e.target.value })}
                              className="editor-field-input editor-field-textarea"
                              style={{ minHeight: 160 }}
                              placeholder="留空使用内置默认。支持 {{character_name}}、{{scene_name}}、{{bucket_start}}、{{bucket_end}}、{{turn_start}}、{{turn_end}}、{{transcript}}。"
                            />
                          </label>
                        </div>
                      </div>
                      <div className="wed-block">
                        <label className="editor-field">
                          <span className="editor-field-label">
                            <span className="field-label-head">
                              <span>主控总结提示词</span>
                              <HelpTip text="留空恢复内置默认。按固定桶汇总全部剧情并注入主控提示词，不按角色隔离。支持 {{bucket_start}}、{{bucket_end}}、{{turn_start}}、{{turn_end}}、{{transcript}}。" />
                            </span>
                          </span>
                          <textarea
                            value={directorConfig.director_memory_summary_prompt || summaryPromptDefaults?.director || ""}
                            onChange={(e) => updateDirectorPatch({ director_memory_summary_prompt: e.target.value })}
                            className="editor-field-input editor-field-textarea"
                            style={{ minHeight: 240 }}
                            placeholder="留空使用内置默认。支持 {{bucket_start}}、{{bucket_end}}、{{turn_start}}、{{turn_end}}、{{transcript}}。"
                          />
                        </label>
                      </div>
                    </>
                  ) : null}

                  {directorPane === "prompt" ? (
                    <>
                      {directorConfig.service_mode === "world_sim" ? (
                        <>
                          <div className="wed-block">
                            <label className="editor-field">
                              <span className="editor-field-label">
                                <span className="field-label-head">
                                  <span>世界主控模型</span>
                                  <HelpTip text="选择世界主控调度时使用的文本模型；留空使用应用默认文本模型。采样参数请在模型配置页面设置。" />
                                </span>
                              </span>
                              <select
                                value={directorModelSelectValue}
                                onChange={(e) => updateDirectorPatch({ director_model: e.target.value })}
                                className="editor-field-input editor-field-select"
                              >
                                <option value="">使用默认文本模型</option>
                                {textModels.map((model) => (
                                  <option key={model.id} value={model.id}>
                                    {model.name || model.model_id}
                                  </option>
                                ))}
                                {!resolvedDirectorModelOption && directorConfig.director_model.trim() ? (
                                  <option value={directorConfig.director_model.trim()}>
                                    {directorConfig.director_model.trim()}（当前未匹配）
                                  </option>
                                ) : null}
                              </select>
                            </label>
                          </div>
                          <div className="wed-block">
                            <label className="editor-field">
                              <span className="editor-field-label">
                                <span className="field-label-head">
                                  <span>世界主控提示词</span>
                                  <HelpTip text="留空使用框架默认。支持 {{current_time}} 和 {{当前时间}}，运行时替换为当前时间。" />
                                </span>
                              </span>
                              <textarea value={directorConfig.world_director_prompt} onChange={(e) => updateMergedDirectorPrompt(e.target.value)} className="editor-field-input editor-field-textarea" style={{ minHeight: 340 }} />
                            </label>
                            <div className="wed-prompt-bar">
                              <div className="wed-chips">
                                <button type="button" className="wed-chip" onClick={() => updateMergedDirectorPrompt(`${directorConfig.world_director_prompt}{{current_time}}`)}>+ {"{{current_time}}"}</button>
                                <button type="button" className="wed-chip" onClick={() => updateMergedDirectorPrompt(`${directorConfig.world_director_prompt}{{当前时间}}`)}>+ {"{{当前时间}}"}</button>
                              </div>
                              <span className="wed-meta">{directorConfig.world_director_prompt.length} 字 · 留空使用框架默认</span>
                            </div>
                          </div>
                        </>
                      ) : null}
                    </>
                  ) : null}

                  {directorPane === "tools" ? (
                    <div className="wed-block">
                <div className="editor-field-label">已选工具</div>
                {selectedMcpTools.length === 0 ? <div className="text-muted">当前未选择任何 MCP 工具。</div> : null}
                {selectedMcpTools.map((tool) => (
                  <div key={tool.id} className="settings-form-grid" style={{ padding: 12, border: "1px solid var(--color-border)", borderRadius: 12 }}>
                    <div>
                      <div className="field-label-head">
                        <div>{tool.name}</div>
                        <HelpTip text={tool.description} />
                      </div>
                    </div>
                    <div className="editor-field" style={{ alignSelf: "center" }}>
                      <button type="button" className="action-btn" onClick={() => toggleAllowedMcpTool(tool.id)}>移除</button>
                    </div>
                  </div>
                ))}
                <div className="editor-field-label">可用工具</div>
                <input type="text" value={mcpToolSearch} onChange={(e) => setMcpToolSearch(e.target.value)} className="editor-field-input" placeholder="搜索工具..." />
                <div className="settings-form-grid">
                  {filteredMcpTools.map((tool) => (
                    <div key={tool.id} className="settings-form-grid" style={{ padding: 12, border: "1px solid var(--color-border)", borderRadius: 12 }}>
                      <div>
                        <div className="field-label-head">
                          <div>{tool.name}</div>
                          <HelpTip text={tool.description} />
                        </div>
                      </div>
                      <div className="editor-field" style={{ alignSelf: "center" }}>
                        <button type="button" className="action-btn" onClick={() => toggleAllowedMcpTool(tool.id)}>添加</button>
                      </div>
                    </div>
                  ))}
                </div>
                    </div>
                  ) : null}
                </div>
              </div>
            </div>

            {directorPromptDraft !== null ? (
              <div className="wed-modal" role="dialog" aria-modal="true" aria-label="模型提示词设置全屏编辑">
                <div className="wed-modal-card">
                  <div className="wed-modal-head">
                    <div>
                      <div className="wed-panel-title">模型提示词设置 · 全屏编辑</div>
                      <div className="wed-panel-sub">{directorPromptDraft.length} 字 · 留空使用框架默认</div>
                    </div>
                  </div>
                  <textarea
                    className="editor-field-input editor-field-textarea wed-modal-textarea"
                    value={directorPromptDraft}
                    onChange={(event) => setDirectorPromptDraft(event.target.value)}
                    spellCheck={false}
                    autoFocus
                  />
                  <div className="wed-modal-actions">
                    <button type="button" className="action-btn" onClick={() => setDirectorPromptDraft(null)}>取消</button>
                    <button
                      type="button"
                      className="action-btn action-btn--accent"
                      onClick={() => {
                        updateMergedDirectorPrompt(directorPromptDraft);
                        setDirectorPromptDraft(null);
                      }}
                    >
                      保存到配置
                    </button>
                  </div>
                </div>
              </div>
            ) : null}
          </SurfacePanel>
        ) : null}

        {sectionId === "promptPreview" ? (
          <SurfacePanel className="surface-panel--pad-lg">
            <div className="editor-content prompt-preview-surface">
              {promptPreview ? ([
                { scope: "director" as const, title: "世界主控 Prompt 编排", sections: directorPromptSections, order: directorConfig.director_prompt_order, trace: promptPreview.world_director_prompt_trace },
                { scope: "character" as const, title: "NPC Prompt 编排", sections: characterPromptSections, order: directorConfig.character_prompt_order, trace: promptPreview.character_prompt_traces[0]?.prompt_trace },
              ]).map(({ scope, title, sections, order, trace }) => {
                const modules = Array.isArray(trace?.modules) ? trace.modules as Array<Record<string, unknown>> : [];
                const { modulesById, labels: attributeLabels } = scope === "director"
                  ? buildDirectorAttributePreview(modules, promptAttributeSchemas)
                  : { modulesById: new Map(modules.map((module) => [String(module.id ?? ""), module])), labels: new Map<string, string>() };
                const rawDebug = trace?.raw_debug && typeof trace.raw_debug === "object"
                  ? trace.raw_debug as Record<string, unknown>
                  : null;
                const requestTools = (rawDebug?.request && typeof rawDebug.request === "object"
                  ? (rawDebug.request as Record<string, unknown>).tools
                  : undefined)
                  ?? rawDebug?.tools;
                const dynamicSections = Array.from(modulesById.values())
                  .filter((module) => String(module.id ?? "").startsWith("world_attribute:") || String(module.id ?? "").startsWith("character_attribute:"))
                  .map((module) => {
                    const id = String(module.id);
                    const scopeLabel = id.startsWith("world_attribute:") ? "世界属性" : "角色属性";
                    const schemaLabel = attributeLabels.get(id);
                    return [id, schemaLabel ? `${scopeLabel}：${schemaLabel}` : "自定义属性"] as [string, string];
                  });
                const allSections = [...sections, ...dynamicSections];
                const ordered = order
                  .map((id) => allSections.find(([sectionId]) => sectionId === id))
                  .filter((item): item is [string, string] => Boolean(item));
                for (const item of dynamicSections) if (!ordered.some(([id]) => id === item[0])) ordered.push(item);
                return (
                  <FoldableEditorSection key={scope} title={title} defaultOpen={scope === "director"}>
                    <PromptSectionList sections={ordered} modules={modulesById}
                      requestTools={requestTools}
                      onReorder={(ids) => reorderPromptSections(scope, ids)} />
                  </FoldableEditorSection>
                );
              }) : null}
              {promptPreviewLoading ? <div className="empty-text">正在加载 prompt 预览...</div> : null}
              {promptPreviewError ? <div className="text-error">{promptPreviewError}</div> : null}
              {promptAttributeSchemaError ? <div className="text-error">{promptAttributeSchemaError}</div> : null}
              {promptPreview ? (
                <div style={{ display: "grid", gap: 12 }}>
                  <PromptSendPreviewCard
                    item={{
                      recipient_type: "director",
                      recipient_name: "世界主控",
                      prompt_call: promptPreview.world_director_prompt_trace,
                    }}
                    defaultOpen
                  />
                  {promptPreview.character_prompt_traces.map((item, index) => (
                    <PromptSendPreviewCard
                      key={`${item.speaker ?? "character"}-${index}`}
                      item={{
                        recipient_type: "character",
                        recipient_name: item.speaker ?? `角色 ${index + 1}`,
                        prompt_call: item.prompt_trace,
                      }}
                    />
                  ))}
                </div>
              ) : null}
              <div className="flex flex--justify-end">
                <button type="button" className="action-btn" onClick={() => void reloadPromptPreview()}>重新加载</button>
              </div>
            </div>
          </SurfacePanel>
        ) : null}

        {sectionId === "style" ? (
          <SurfacePanel className="surface-panel--pad-lg">
            <div className="wed-shell">
              <div className="wed-toolbar">
                <div className="wed-seg" role="tablist" aria-label="界面风格分区">
                  {([
                    { id: "backgrounds", label: "背景素材" },
                    { id: "scenes", label: "场景背景" },
                    { id: "ui", label: "UI 文档" },
                    { id: "preview", label: "预览与校验" },
                  ] as Array<{ id: StylePaneId; label: string }>).map((tab) => (
                    <button key={tab.id} type="button" role="tab" aria-selected={stylePane === tab.id} onClick={() => setStylePane(tab.id)}>
                      {tab.label}
                    </button>
                  ))}
                </div>
                <span className="wed-meta">{uiThemeConfig.local_background_assets.length} 张通用背景 · {sceneNames.length} 个场景</span>
              </div>

              {stylePane === "backgrounds" ? (
                <div className="wed-panel">
                  <div className="wed-panel-head">
                    <div>
                      <div className="wed-panel-title">通用背景</div>
                      <div className="wed-panel-sub">不绑定场景的全局背景，第一张作为默认主背景</div>
                    </div>
                    <label className="action-btn action-btn--accent" style={{ cursor: uploadingBackground ? "wait" : "pointer" }}>
                      {uploadingBackground ? "上传中..." : "上传背景"}
                      <input type="file" accept="image/*,video/*" onChange={(e) => void handleUploadBackground(e.target.files?.[0] ?? null)} disabled={uploadingBackground} style={{ display: "none" }} />
                    </label>
                  </div>
                  <div className="wed-panel-body">
                    {renderBackgroundTiles(uiThemeConfig.local_background_assets)}
                    <p className="wed-help">上传的素材会随世界包一起导出；视频素材在游戏内按循环播放处理。</p>
                  </div>
                </div>
              ) : null}

              {stylePane === "scenes" ? (
                <div className="wed-scene-grid">
                  <div className="wed-panel">
                    <div className="wed-panel-head">
                      <div>
                        <div className="wed-panel-title">场景列表</div>
                        <div className="wed-panel-sub">先选场景，再管理它的背景</div>
                      </div>
                    </div>
                    <div className="wed-panel-body">
                      {sceneNames.length === 0 ? (
                        <div className="wed-empty">世界还没有场景。</div>
                      ) : (
                        <div className="wed-scene-items">
                          {sceneNames.map((scene) => {
                            const count = (uiThemeConfig.local_scene_backgrounds[scene] ?? []).length;
                            return (
                              <button
                                key={scene}
                                type="button"
                                className="wed-scene-item"
                                aria-current={activeStyleScene === scene ? "true" : "false"}
                                onClick={() => setStyleScene(scene)}
                              >
                                <span className="t">{scene}</span>
                                <span className="s">{count} 张背景</span>
                              </button>
                            );
                          })}
                        </div>
                      )}
                    </div>
                  </div>
                  <div className="wed-panel">
                    <div className="wed-panel-head">
                      <div>
                        <div className="wed-panel-title">{activeStyleScene || "场景背景"}</div>
                        <div className="wed-panel-sub">该场景优先使用自己的背景，缺失时回退通用背景</div>
                      </div>
                      {activeStyleScene ? (
                        <label className="action-btn action-btn--accent" style={{ cursor: uploadingBackground ? "wait" : "pointer" }}>
                          {uploadingBackground ? "上传中..." : "上传背景"}
                          <input type="file" accept="image/*,video/*" onChange={(e) => void handleUploadBackground(e.target.files?.[0] ?? null, activeStyleScene)} disabled={uploadingBackground} style={{ display: "none" }} />
                        </label>
                      ) : null}
                    </div>
                    <div className="wed-panel-body">
                      {activeStyleScene ? (
                        renderBackgroundTiles(uiThemeConfig.local_scene_backgrounds[activeStyleScene] ?? [], activeStyleScene)
                      ) : (
                        <div className="wed-empty">先在左侧选择一个场景。</div>
                      )}
                    </div>
                  </div>
                </div>
              ) : null}

              {stylePane === "ui" ? (
                <div className="wed-doc-grid">
                  <div className="wed-panel">
                    <div className="wed-panel-head">
                      <div>
                        <div className="wed-panel-title">桌面端 UI 文档</div>
                        <div className="wed-panel-sub">HTML / CSS / JavaScript，隔离页面 + world SDK</div>
                      </div>
                    </div>
                    <div className="wed-panel-body">{renderGameUiDocumentEditor("desktop", parsedDesktopGameUi)}</div>
                  </div>
                  <div className="wed-panel">
                    <div className="wed-panel-head">
                      <div>
                        <div className="wed-panel-title">移动端 UI 文档</div>
                        <div className="wed-panel-sub">移动端会独立编译与校验，不共用桌面端文档</div>
                      </div>
                    </div>
                    <div className="wed-panel-body">{renderGameUiDocumentEditor("mobile", parsedMobileGameUi)}</div>
                  </div>
                </div>
              ) : null}

              {stylePane === "preview" ? (
                <div className="wed-preview-grid">
                  {renderGameUiPreviewSection()}
                  {renderGameUiGovernanceSection()}
                </div>
              ) : null}
            </div>
          </SurfacePanel>
        ) : null}

        {sectionId === "returnRules" ? (
          <SurfacePanel className="surface-panel--pad-lg">
            <div className="editor-content">
              <div className="text-muted" style={{ marginBottom: 12 }}>
                按顺序对模型返回内容执行文本替换。规则作用范围选“世界主控”“角色/NPC”或“两者”时，分别应用到对应回复流程。
              </div>
              {directorConfig.return_processing_rules.map((rule, index) => (
                <div
                  key={rule.id}
                  className="settings-form-grid"
                  style={{ padding: 12, marginBottom: 12, border: "1px solid var(--color-border)", borderRadius: 12 }}
                >
                  <label className="editor-field">
                    <span className="editor-field-label">规则名称</span>
                    <input
                      type="text"
                      value={rule.name}
                      onChange={(event) => updateReturnRule(index, { name: event.target.value })}
                      className="editor-field-input"
                    />
                  </label>
                  <label className="editor-field">
                    <span className="editor-field-label">作用范围</span>
                    <select
                      value={rule.scope}
                      onChange={(event) => updateReturnRule(index, { scope: event.target.value as ReturnProcessingRule["scope"] })}
                      className="editor-field-input editor-field-select"
                    >
                      <option value="director">世界主控</option>
                      <option value="character">角色/NPC</option>
                      <option value="both">两者</option>
                    </select>
                  </label>
                  <label className="editor-field">
                    <span className="editor-field-label">执行顺序</span>
                    <input
                      type="number"
                      value={rule.order}
                      onChange={(event) => updateReturnRule(index, { order: Number(event.target.value) })}
                      className="editor-field-input"
                    />
                  </label>
                  <label className="editor-field">
                    <span className="editor-field-label">启用规则</span>
                    <div className="settings-inline-toggle">
                      <input
                        type="checkbox"
                        checked={rule.enabled}
                        onChange={(event) => updateReturnRule(index, { enabled: event.target.checked })}
                      />
                    </div>
                  </label>
                  <label className="editor-field">
                    <span className="editor-field-label">匹配内容（正则表达式）</span>
                    <textarea
                      value={rule.pattern}
                      onChange={(event) => updateReturnRule(index, { pattern: event.target.value })}
                      className="editor-field-input editor-field-textarea"
                      rows={3}
                    />
                  </label>
                  <label className="editor-field">
                    <span className="editor-field-label">替换为</span>
                    <textarea
                      value={rule.replacement}
                      onChange={(event) => updateReturnRule(index, { replacement: event.target.value })}
                      className="editor-field-input editor-field-textarea"
                      rows={3}
                    />
                  </label>
                  <div className="editor-field">
                    <button type="button" className="action-btn" onClick={() => removeReturnRule(index)}>删除规则</button>
                  </div>
                </div>
              ))}
              <button type="button" className="action-btn" onClick={addReturnRule}>添加返回处理规则</button>
            </div>
          </SurfacePanel>
        ) : null}

        {sectionId === "configPreview" ? (
          <SurfacePanel className="surface-panel--pad-lg">
            <div className="editor-content">
              <pre style={{ whiteSpace: "pre-wrap", wordBreak: "break-all", fontSize: 12, padding: 16, background: "var(--color-surface-2)", borderRadius: 12 }}>{previewJson}</pre>
            </div>
          </SurfacePanel>
        ) : null}
        {mobileSaveBar}
      </div>
    );
  }

  // ==================== Desktop rendering ====================
  function renderDesktopContent() {
    if (!world) return null;

    return (
      <div className={`editor-content${activeTab === "director" ? " editor-content--fill" : ""}`}>
        <div className="editor-tabs">
          {fixedTabs.map((tab) => (
            <button key={tab.id} type="button" onClick={() => setActiveTab(tab.id)} className={`editor-tab${activeTab === tab.id ? " editor-tab--active" : ""}`}>
              {tab.label}
            </button>
          ))}
        </div>
        {renderMobileSectionContent(activeTab, true)}
      </div>
    );
  }

  // ==================== Main render ====================
  return (
    <ScreenLayout
      title={isMobile ? "" : world?.name ?? "世界编辑"}
      subtitle=""
      toolbar={isMobile ? undefined : (
        <div className="world-editor-toolbar">
          <button type="button" onClick={() => navigate("/worlds")} className="action-btn">
            <ArrowLeft size={14} /> 返回
          </button>
          {isDirty ? <span className="wed-dirty">有未保存修改</span> : null}
          {isDirty && !isNew ? (
            <button
              type="button"
              onClick={() => void handleRevert()}
              disabled={reverting || saving || deleting || exporting}
              className="action-btn"
            >
              <RotateCcw size={14} /> {reverting ? "还原中..." : "还原"}
            </button>
          ) : null}
          {!isNew ? (
            <button
              type="button"
              onClick={() => void handleExport()}
              disabled={exporting || deleting || saving}
              className="action-btn"
            >
              {exporting ? "导出中..." : "导出世界包"}
            </button>
          ) : null}
          {!isNew ? (
            <button type="button" onClick={() => setShowDeleteDialog(true)} disabled={deleting || saving || exporting} className="action-btn">
              {deleting ? "删除中..." : "删除世界"}
            </button>
          ) : null}
          <button type="button" onClick={() => void handleSave()} disabled={saving || deleting || exporting} className="action-btn action-btn--accent">
            {saving ? "保存中..." : "保存世界"}
          </button>
        </div>
      )}
    >
      {isMobile ? (
        <div className="mobile-editor-topbar">
          <button
            type="button"
            className="mobile-editor-back"
            onClick={requestMobileBack}
            aria-label={activeSection ? "返回分区列表" : "返回世界工坊"}
          >
            <ArrowLeft size={18} />
          </button>
          <div className="mobile-editor-heading">
            <strong className="mobile-editor-title">
              {activeSection
                ? mobileSectionLabel
                : world?.name?.trim() || (isNew ? "未命名世界" : "世界编辑")}
              {isDirty ? <i className="mobile-editor-dirty-dot" aria-label="有未保存修改" /> : null}
            </strong>
            <span className="mobile-editor-kicker">
              {activeSection ? "世界编辑" : isNew ? "新建世界" : "世界工坊"}
            </span>
          </div>
          <button
            type="button"
            className="mobile-editor-more"
            onClick={() => setMobileMenuOpen((open) => !open)}
            aria-label="更多操作"
            aria-expanded={mobileMenuOpen}
          >
            <MoreHorizontal size={18} />
          </button>
          {mobileMenuOpen ? (
            <div className="mobile-editor-menu" role="menu">
              <button
                type="button"
                role="menuitem"
                className="mobile-editor-menu-item"
                disabled={saving || deleting || exporting}
                onClick={() => {
                  setMobileMenuOpen(false);
                  void handleSave();
                }}
              >
                <Save size={15} /> 保存修改
              </button>
              {!isNew ? (
                <button
                  type="button"
                  role="menuitem"
                  className="mobile-editor-menu-item"
                  disabled={exporting || deleting || saving}
                  onClick={() => {
                    setMobileMenuOpen(false);
                    void handleExport();
                  }}
                >
                  导出世界包
                </button>
              ) : null}
              {!isNew ? (
                <button
                  type="button"
                  role="menuitem"
                  className="mobile-editor-menu-item mobile-editor-menu-item--danger"
                  disabled={deleting || saving || exporting}
                  onClick={() => {
                    setMobileMenuOpen(false);
                    setShowDeleteDialog(true);
                  }}
                >
                  删除世界
                </button>
              ) : null}
              <button
                type="button"
                role="menuitem"
                className="mobile-editor-menu-item"
                onClick={() => {
                  setMobileMenuOpen(false);
                  window.dispatchEvent(new Event("od-mobile-nav-open"));
                }}
              >
                主菜单
              </button>
            </div>
          ) : null}
        </div>
      ) : null}
      {loading ? <SurfacePanel className="surface-panel--pad-lg">正在加载世界详情...</SurfacePanel> : null}
      {error ? <SurfacePanel className="surface-panel--pad-lg text-error">错误：{error}</SurfacePanel> : null}
      {!loading && world ? (
        <>
          {isMobile ? (
            activeSection ? renderMobileSectionContent() : renderMobileSectionList()
          ) : (
            renderDesktopContent()
          )}
        </>
      ) : null}

      <ConfirmDialog
        open={showDeleteDialog}
        title="删除世界"
        description={world ? `确定要删除“${world.name}”吗？此操作不可恢复。` : ""}
        confirmLabel={deleting ? "删除中..." : "删除世界"}
        confirmVariant="danger"
        confirmDisabled={deleting || saving || exporting}
        onClose={() => {
          if (!deleting) {
            setShowDeleteDialog(false);
          }
        }}
        onConfirm={() => {
          void handleDelete();
        }}
      />

      <ConfirmDialog
        open={showLeaveDialog}
        title="放弃未保存的修改？"
        description="当前世界的修改尚未保存，离开后这些修改会丢失。"
        confirmLabel="放弃修改"
        cancelLabel="继续编辑"
        confirmVariant="danger"
        onClose={() => setShowLeaveDialog(false)}
        onConfirm={() => {
          setShowLeaveDialog(false);
          performMobileBack();
        }}
      />
    </ScreenLayout>
  );
}

/* TODO: WorldEditorPage 移动端与桌面端 UI 结构差异较大(section-list vs tabs),
 * 当前通过 isMobile 条件渲染两套布局,后续可考虑进一步重构为统一组件。*/

