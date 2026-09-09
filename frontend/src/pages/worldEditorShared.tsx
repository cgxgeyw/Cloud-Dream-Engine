// Extracted from WorldEditorPage.tsx — types, defaults, pure helpers, FoldableEditorSection
import { useEffect, useMemo, useState, type CSSProperties, type ReactNode } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { useId } from "react";
import { useLayoutEffect, useRef } from "react";
import {
  assetUrl,
  compileWorldUiDocument,
  createWorld,
  deleteWorld,
  downloadWorldPackage,
  fetchModels,
  fetchMcpTools,
  fetchWorld,
  fetchWorldCharacters,
  isTauriEnvironment,
  uploadFile,
  updateWorld,
  validateWorldUiBundle,
  verifyWorldPackageUiCompatibility,
  type CharacterResponse,
  type ModelConfigResponse,
  type McpToolResponse,
  type WorldUiBundleValidationResult,
  type WorldUiCompileResult,
  type WorldUiCompatibilityReport,
  type WorldCreateRequest,
  type WorldMapTopology,
  type WorldOpeningMessage,
  type WorldResponse,
} from "../data/apiAdapter";
import { AttributePanel } from "../components/AttributePanel";
import { GenerationParamsEditor } from "../components/GenerationParamsEditor";
import { ConfirmDialog } from "../components/ModalDialog";
import type { GenerationParams } from "../data/types";
import { useIsMobile } from "../components/ResponsiveLayout";
import { useSectionParam } from "../hooks/useSectionParam";
import { useWorldPromptPreview } from "../hooks/useWorldPromptPreview";
import { GameUiSandboxPreview } from "../components/GameUiSandboxPreview";
import { GameUiStructureEditor } from "../components/game-ui-editor/GameUiStructureEditor";
import { PromptSendPreviewCard } from "../components/PromptTraceView";
import { ScreenLayout, SurfacePanel } from "../components/ScreenLayout";
import { showToast } from "../components/Toast";
import { ArrowLeft, Save, X } from "lucide-react";
import {
  buildGameUiStylesheet,
  createGameUiScopeSelector,
  defaultGameUiFile,
  normalizeGameUiScopeId,
  normalizeAssetConfig,
  normalizeWorldUiEnvelope,
  parseGameUiDocument,
  stringifyGameUiDocument,
  type GameUiDocumentV2,
  type GameUiPlatform,
  type ParsedGameUiDocument,
} from "../data/gameUi";


export const fixedTabs = [
  { id: "basic", label: "基础信息" },
  { id: "background", label: "世界背景" },
  { id: "opening", label: "开场配置" },
  { id: "time", label: "时间系统" },
  { id: "map", label: "地图" },
  { id: "customAttributes", label: "自定义属性" },
  { id: "runtimeContext", label: "运行时上下文" },
  { id: "director", label: "世界主控" },
  { id: "promptPreview", label: "Prompt 预览" },
  { id: "style", label: "界面风格" },
  { id: "configPreview", label: "配置预览" },
] as const;

export type FixedTabId = (typeof fixedTabs)[number]["id"];
export type OpeningComposerRole = "system" | "agent";

export type OpeningSpeakerOption = {
  value: string;
  label: string;
};

export type StatusTabOption = {
  key: string;
  label: string;
  content: string;
  owners: string[];
};

export type FoldableEditorSectionProps = {
  title: string;
  description?: string;
  badge?: string | null;
  defaultOpen?: boolean;
  children: ReactNode;
};

export type DirectorConfig = {
  service_mode: "world_sim" | "agent_chat";
  default_agent_id: string;
  runtime_policy: {
    memory_write_mode: "session" | "character" | "world_and_character";
  };
  allow_scene_transition: boolean;
  allow_npc_spawn: boolean;
  allow_player_character_switch: boolean;
  director_interaction_kinds: InteractionKind[];
  history_dialogue_rounds: number;
  director_tool_loop_limit: number;
  director_model: string;
  character_memory_hit_turns: number;
  character_memory_event_window_rounds: number;
  character_memory_dialogue_window_rounds: number;
  character_memory_retrieval_mode: "lexical_only" | "hybrid" | "semantic_only";
  character_memory_candidate_limit: number;
  character_memory_semantic_weight: number;
  runtime_context_prompt: string;
  world_director_prompt: string;
  prompt_presets: PromptPreset[];
  return_processing_rules: ReturnProcessingRule[];
  allowed_mcp_tool_ids: string[];
  /** 世界级生成参数（第 8 项）。留空的项交给应用默认，玩家还能在存档里再覆盖。 */
  generation_params: GenerationParams;
};

export type InteractionKind = "choice" | "multi_choice" | "form" | "confirm" | "slider";

export const interactionKindOptions: Array<{ value: InteractionKind; label: string }> = [
  { value: "choice", label: "单选" },
  { value: "multi_choice", label: "多选" },
  { value: "form", label: "表单" },
  { value: "confirm", label: "确认" },
  { value: "slider", label: "滑条" },
];

export type PromptPreset = {
  id: string;
  name: string;
  content: string;
  scope: "director" | "character" | "both";
  enabled: boolean;
  order: number;
};

export type ReturnProcessingRule = {
  id: string;
  name: string;
  scope: "director" | "character" | "both";
  pattern: string;
  replacement: string;
  enabled: boolean;
  order: number;
};

export type TimeSlot = {
  label: string;
  clock: string;
};

export type TimeConfig = {
  mode: "labels" | "24h";
  slots: TimeSlot[];
  start_label: string;
  start_time: string;
};

export type UiThemeConfig = {
  runtime_version: 2 | 3;
  capabilities: string[];
  storage: Record<string, unknown>;
  logic: Record<string, unknown>;
  background_source_mode: string;
  portrait_source_mode: string;
  runtime_image_generation_enabled: boolean;
  local_background_assets: string[];
  local_scene_backgrounds: Record<string, string[]>;
  desktop_file: string;
  mobile_file: string;
  desktop_stylesheet: string;
  mobile_stylesheet: string;
};

export const defaultWorldDirectorPrompt = "";

export const defaultDirectorConfig: DirectorConfig = {
  service_mode: "world_sim",
  default_agent_id: "",
  runtime_policy: {
    memory_write_mode: "session",
  },
  allow_scene_transition: true,
  allow_npc_spawn: true,
  allow_player_character_switch: true,
  director_interaction_kinds: [],
  history_dialogue_rounds: 6,
  director_tool_loop_limit: 4,
  director_model: "",
  character_memory_hit_turns: 2,
  character_memory_event_window_rounds: 10,
  character_memory_dialogue_window_rounds: 2,
  character_memory_retrieval_mode: "hybrid",
  character_memory_candidate_limit: 200,
  character_memory_semantic_weight: 0.65,
  runtime_context_prompt: "",
  world_director_prompt: defaultWorldDirectorPrompt,
  prompt_presets: [],
  return_processing_rules: [],
  allowed_mcp_tool_ids: [],
  generation_params: {},
};

export const defaultTimeConfig: TimeConfig = {
  mode: "labels",
  slots: [
    { label: "清晨", clock: "06:00" },
    { label: "正午", clock: "12:00" },
    { label: "夜晚", clock: "20:00" },
  ],
  start_label: "清晨",
  start_time: "08:00",
};

export const defaultUiThemeConfig: UiThemeConfig = {
  runtime_version: 3,
  capabilities: ["supports_file_picker", "supports_mic"],
  storage: { kv_namespaces: [], collections: {} },
  logic: { runtime: "disabled", source: "", timeout_ms: 1000 },
  background_source_mode: "local-first",
  portrait_source_mode: "local-first",
  runtime_image_generation_enabled: false,
  local_background_assets: [],
  local_scene_backgrounds: {},
  desktop_file: defaultGameUiFile("desktop"),
  mobile_file: defaultGameUiFile("mobile"),
  desktop_stylesheet: "",
  mobile_stylesheet: "",
};

export function resolveExposurePolicyMode(policy: string | Record<string, unknown> | undefined): string {
  if (typeof policy === "string") {
    const normalized = policy.trim();
    return normalized || "on-demand";
  }
  const mode = typeof policy?.mode === "string" ? policy.mode.trim() : "";
  return mode || "on-demand";
}

export function normalizeAssetGroupMap(raw: unknown): Record<string, string[]> {
  if (!raw || typeof raw !== "object") {
    return {};
  }

  return Object.fromEntries(
    Object.entries(raw as Record<string, unknown>)
      .map(([key, value]) => {
        const assetKey = key.trim();
        if (!assetKey) {
          return [assetKey, []] as const;
        }

        if (!Array.isArray(value)) {
          return [assetKey, []] as const;
        }

        return [assetKey, value.map((item) => String(item).trim()).filter(Boolean)] as const;
      })
      .filter(([key, value]) => key && value.length > 0),
  );
}

export function appendUniqueAsset(items: string[], nextItem: string): string[] {
  const value = nextItem.trim();
  if (!value) {
    return items;
  }
  return items.includes(value) ? items : [...items, value];
}

export function removeAsset(items: string[], target: string): string[] {
  return items.filter((item) => item !== target);
}

export function moveAssetToFront(items: string[], target: string): string[] {
  const next = removeAsset(items, target);
  return items.includes(target) ? [target, ...next] : next;
}

export function getAssetDisplayName(assetPath: string): string {
  const parts = assetPath.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? assetPath;
}

export function defaultMapTopology(): WorldMapTopology {
  return {
    version: 1,
    root: {
      id: "main-scene",
      label: "主场景",
      children: [
        { id: "secondary-scene", label: "次级场景" },
      ],
    },
  };
}

export function extractMapSceneNames(topology: WorldMapTopology | null | undefined): string[] {
  const names: string[] = [];
  function visit(node: unknown) {
    if (!node || typeof node !== "object" || Array.isArray(node)) {
      return;
    }
    const value = node as { label?: unknown; name?: unknown; children?: unknown };
    const label = typeof value.label === "string"
      ? value.label.trim()
      : typeof value.name === "string"
        ? value.name.trim()
        : "";
    if (label) {
      names.push(label);
    }
    if (Array.isArray(value.children)) {
      value.children.forEach(visit);
    }
  }

  if (topology && typeof topology === "object") {
    const value = topology as { root?: unknown; tree?: unknown; nodes?: unknown };
    if (value.root) {
      visit(value.root);
    } else if (value.tree) {
      visit(value.tree);
    } else if (Array.isArray(value.nodes)) {
      value.nodes.forEach(visit);
    }
  }

  return names;
}

export function formatMapTopologyJson(topology: WorldMapTopology | null | undefined): string {
  return JSON.stringify(topology ?? defaultMapTopology(), null, 2);
}

export function parseMapTopologyJson(source: string): WorldMapTopology {
  const parsed = JSON.parse(source) as unknown;
  if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
    return parsed as WorldMapTopology;
  }
  throw new Error("地图拓扑必须是 JSON 对象，不能使用旧数组格式。请填写包含 root、tree 或 nodes 的对象。");
}

export function resolveSceneNames(world: WorldResponse | null): string[] {
  if (!world) {
    return ["未命名场景"];
  }

  const names = [world.opening_scene, ...extractMapSceneNames(world.map_nodes)]
    .map((item) => item.trim())
    .filter(Boolean);
  return names.length > 0 ? Array.from(new Set(names)) : ["未命名场景"];
}

export function resolvePreviewBackgroundAsset(config: UiThemeConfig, sceneName: string | null): string {
  const sceneAssets = sceneName ? config.local_scene_backgrounds[sceneName] ?? [] : [];
  if (sceneAssets.length > 0) {
    return sceneAssets[0];
  }
  return config.local_background_assets[0] ?? "";
}

export function countGroupedAssets(groups: Record<string, string[]>): number {
  return Object.values(groups).reduce((total, items) => total + items.length, 0);
}

export function composeDirectorRuntimeSystemPrompt(basePrompt: string, extraPrompt: string): string {
  const base = basePrompt.trim();
  const extra = extraPrompt.trim();
  if (!extra) {
    return base;
  }
  return [base, "以下为世界包追加的运行时提示词：", extra].filter(Boolean).join("\n\n");
}

export function FoldableEditorSection({
  title,
  description,
  badge,
  defaultOpen = false,
  children,
}: FoldableEditorSectionProps) {
  const [open, setOpen] = useState(defaultOpen);

  return (
    <section className={open ? "editor-foldable editor-foldable--open" : "editor-foldable"}>
      <button type="button" className="editor-foldable-toggle" onClick={() => setOpen((current) => !current)}>
        <span className="editor-foldable-copy">
          <span className="editor-foldable-title">{title}</span>
          {description ? null : null}
        </span>
        <span className="editor-foldable-meta">
          {badge ? <span className="editor-foldable-badge">{badge}</span> : null}
          <span className="editor-foldable-chevron" aria-hidden="true">▾</span>
        </span>
      </button>
      {open ? <div className="editor-foldable-body">{children}</div> : null}
    </section>
  );
}

export function normalizeDirectorConfig(raw: Record<string, unknown> | undefined): DirectorConfig {
  const normalizeScope = (value: unknown): "director" | "character" | "both" =>
    value === "director" || value === "character" || value === "both" ? value : "both";
  const rawRuntimePolicy = raw?.runtime_policy as Record<string, unknown> | undefined;
  const serviceMode = raw?.service_mode === "agent_chat" ? "agent_chat" : "world_sim";
  const normalizeInteractionKinds = (value: unknown): InteractionKind[] =>
    Array.isArray(value)
      ? Array.from(new Set(value.filter((item): item is InteractionKind =>
        typeof item === "string" && interactionKindOptions.some((option) => option.value === item),
      )))
      : [];
  return {
    service_mode: serviceMode,
    default_agent_id:
      typeof raw?.default_agent_id === "string" ? raw.default_agent_id.trim() : defaultDirectorConfig.default_agent_id,
    runtime_policy: {
      memory_write_mode:
        rawRuntimePolicy?.memory_write_mode === "character" ||
        rawRuntimePolicy?.memory_write_mode === "world_and_character"
          ? rawRuntimePolicy.memory_write_mode
          : raw?.memory_write_mode === "character" || raw?.memory_write_mode === "world_and_character"
            ? raw.memory_write_mode
          : defaultDirectorConfig.runtime_policy.memory_write_mode,
    },
    allow_scene_transition:
      typeof raw?.allow_scene_transition === "boolean"
        ? raw.allow_scene_transition
        : defaultDirectorConfig.allow_scene_transition,
    allow_npc_spawn:
      typeof raw?.allow_npc_spawn === "boolean" ? raw.allow_npc_spawn : defaultDirectorConfig.allow_npc_spawn,
    allow_player_character_switch:
      typeof raw?.allow_player_character_switch === "boolean"
        ? raw.allow_player_character_switch
        : defaultDirectorConfig.allow_player_character_switch,
    director_interaction_kinds: normalizeInteractionKinds(raw?.director_interaction_kinds),
    history_dialogue_rounds:
      typeof raw?.history_dialogue_rounds === "number"
        ? Math.max(0, Math.min(20, Math.round(raw.history_dialogue_rounds)))
        : defaultDirectorConfig.history_dialogue_rounds,
    director_tool_loop_limit:
      typeof raw?.director_tool_loop_limit === "number"
        ? Math.max(1, Math.min(12, Math.round(raw.director_tool_loop_limit)))
        : defaultDirectorConfig.director_tool_loop_limit,
    director_model:
      typeof raw?.director_model === "string"
        ? raw.director_model.trim()
        : defaultDirectorConfig.director_model,
    character_memory_hit_turns:
      typeof raw?.character_memory_hit_turns === "number"
        ? Math.max(1, Math.min(6, Math.round(raw.character_memory_hit_turns)))
        : defaultDirectorConfig.character_memory_hit_turns,
    character_memory_event_window_rounds:
      typeof raw?.character_memory_event_window_rounds === "number"
        ? Math.max(0, Math.min(20, Math.round(raw.character_memory_event_window_rounds)))
        : defaultDirectorConfig.character_memory_event_window_rounds,
    character_memory_dialogue_window_rounds:
      typeof raw?.character_memory_dialogue_window_rounds === "number"
        ? Math.max(0, Math.min(6, Math.round(raw.character_memory_dialogue_window_rounds)))
        : defaultDirectorConfig.character_memory_dialogue_window_rounds,
    character_memory_retrieval_mode:
      raw?.character_memory_retrieval_mode === "lexical_only" ||
      raw?.character_memory_retrieval_mode === "semantic_only" ||
      raw?.character_memory_retrieval_mode === "hybrid"
        ? raw.character_memory_retrieval_mode
        : defaultDirectorConfig.character_memory_retrieval_mode,
    character_memory_candidate_limit:
      typeof raw?.character_memory_candidate_limit === "number"
        ? Math.max(20, Math.min(600, Math.round(raw.character_memory_candidate_limit)))
        : defaultDirectorConfig.character_memory_candidate_limit,
    character_memory_semantic_weight:
      typeof raw?.character_memory_semantic_weight === "number"
        ? Math.max(0, Math.min(1, Number(raw.character_memory_semantic_weight)))
        : defaultDirectorConfig.character_memory_semantic_weight,
    world_director_prompt:
      typeof raw?.world_director_prompt === "string" && raw.world_director_prompt.trim()
        ? raw.world_director_prompt
        : defaultDirectorConfig.world_director_prompt,
    runtime_context_prompt:
      typeof raw?.runtime_context_prompt === "string"
        ? raw.runtime_context_prompt
        : defaultDirectorConfig.runtime_context_prompt,
    prompt_presets: Array.isArray(raw?.prompt_presets)
      ? raw.prompt_presets.map((item, index) => {
          const row = item as Record<string, unknown>;
          return {
            id: typeof row.id === "string" && row.id.trim() ? row.id : `preset-${index + 1}`,
            name: typeof row.name === "string" && row.name.trim() ? row.name : "未命名预设",
            content: typeof row.content === "string" ? row.content : "",
            scope: normalizeScope(row.scope),
            enabled: typeof row.enabled === "boolean" ? row.enabled : true,
            order: typeof row.order === "number" ? row.order : index + 1,
          };
        })
      : defaultDirectorConfig.prompt_presets,
    return_processing_rules: Array.isArray(raw?.return_processing_rules)
      ? raw.return_processing_rules.map((item, index) => {
          const row = item as Record<string, unknown>;
          return {
            id: typeof row.id === "string" && row.id.trim() ? row.id : `rule-${index + 1}`,
            name: typeof row.name === "string" && row.name.trim() ? row.name : "未命名规则",
            pattern: typeof row.pattern === "string" ? row.pattern : "",
            replacement: typeof row.replacement === "string" ? row.replacement : "",
            scope: normalizeScope(row.scope),
            enabled: typeof row.enabled === "boolean" ? row.enabled : true,
            order: typeof row.order === "number" ? row.order : index + 1,
          };
        })
      : defaultDirectorConfig.return_processing_rules,
    allowed_mcp_tool_ids: Array.isArray(raw?.allowed_mcp_tool_ids)
      ? Array.from(new Set(raw.allowed_mcp_tool_ids.map((item) => String(item).trim()).filter(Boolean)))
      : defaultDirectorConfig.allowed_mcp_tool_ids,
    generation_params: normalizeGenerationParams(raw?.generation_params),
  };
}

/** 只收数字/字符串数组类型的参数项，其余（手写坏了的世界包）按「没配」处理。 */
export function normalizeGenerationParams(raw: unknown): GenerationParams {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) {
    return {};
  }
  const row = raw as Record<string, unknown>;
  const result: GenerationParams = {};
  const numericKeys = [
    "temperature",
    "top_p",
    "top_k",
    "max_tokens",
    "presence_penalty",
    "frequency_penalty",
    "seed",
  ] as const;
  for (const key of numericKeys) {
    const value = row[key];
    if (typeof value === "number" && Number.isFinite(value)) {
      result[key] = value;
    }
  }
  if (Array.isArray(row.stop)) {
    const stop = row.stop.map((item) => String(item).trim()).filter(Boolean);
    if (stop.length > 0) {
      result.stop = stop;
    }
  }
  return result;
}

export function normalizeTimeConfig(raw: Record<string, unknown> | undefined): TimeConfig {
  const rawSlots = Array.isArray(raw?.slots) ? raw.slots : defaultTimeConfig.slots;
  const slots = rawSlots
    .map((item) => {
      const row = item as Record<string, unknown>;
      return {
        label: typeof row?.label === "string" ? row.label : "",
        clock: typeof row?.clock === "string" ? row.clock : "",
      };
    })
    .filter((item) => item.label || item.clock);
  return {
    mode: raw?.mode === "24h" ? "24h" : "labels",
    slots: slots.length > 0 ? slots : defaultTimeConfig.slots,
    start_label:
      typeof raw?.start_label === "string" && raw.start_label.trim()
        ? raw.start_label.trim()
        : defaultTimeConfig.start_label,
    start_time:
      typeof raw?.start_time === "string" && raw.start_time.trim()
        ? raw.start_time.trim()
        : defaultTimeConfig.start_time,
  };
}

export function normalizeUiThemeConfig(raw: Record<string, unknown> | undefined): UiThemeConfig {
  const envelope = normalizeWorldUiEnvelope(raw);
  const assets = normalizeAssetConfig(envelope.assets);
  return {
    runtime_version: envelope.runtime_version,
    capabilities: envelope.capabilities,
    storage: structuredClone(envelope.storage) as unknown as Record<string, unknown>,
    logic: structuredClone(envelope.logic) as unknown as Record<string, unknown>,
    background_source_mode: assets.background_source_mode,
    portrait_source_mode: assets.portrait_source_mode,
    runtime_image_generation_enabled: assets.runtime_image_generation_enabled,
    local_background_assets: assets.local_background_assets,
    local_scene_backgrounds: assets.local_scene_backgrounds,
    desktop_file: envelope.desktop_file,
    mobile_file: envelope.mobile_file,
    desktop_stylesheet: envelope.entries.desktop.stylesheet,
    mobile_stylesheet: envelope.entries.mobile.stylesheet,
  };
}

export function buildUiThemeEnvelope(config: UiThemeConfig): Record<string, unknown> {
  return {
    runtime_version: config.runtime_version,
    capabilities: config.capabilities,
    storage: config.storage,
    logic: config.logic,
    assets: {
      background_source_mode: config.background_source_mode,
      portrait_source_mode: config.portrait_source_mode,
      runtime_image_generation_enabled: config.runtime_image_generation_enabled,
      local_background_assets: config.local_background_assets,
      local_scene_backgrounds: config.local_scene_backgrounds,
    },
    desktop_file: config.desktop_file,
    mobile_file: config.mobile_file,
    entries: {
      desktop: {
        document: config.desktop_file,
        stylesheet: config.desktop_stylesheet,
      },
      mobile: {
        document: config.mobile_file,
        stylesheet: config.mobile_stylesheet,
      },
    },
  };
}

export function normalizeOpeningMessages(raw: unknown): WorldOpeningMessage[] {
  if (!Array.isArray(raw)) {
    return [];
  }
  return raw.reduce<WorldOpeningMessage[]>((messages, item) => {
    const row = item as Record<string, unknown>;
    const content = typeof row?.content === "string" ? row.content.trim() : "";
    if (!content) {
      return messages;
    }
    const role = row?.role === "system" ? "system" : "agent";
    const speaker = typeof row?.speaker === "string" && row.speaker.trim() ? row.speaker.trim() : null;
    messages.push({ role, content, speaker });
    return messages;
  }, []);
}

export function normalizeOpeningCharacterIds(raw: unknown, characters: CharacterResponse[]): string[] {
  if (!Array.isArray(raw)) {
    return [];
  }

  const knownIds = new Set(characters.map((character) => character.id));
  const normalized: string[] = [];
  for (const item of raw) {
    const value = String(item ?? "").trim();
    if (!value || !knownIds.has(value) || normalized.includes(value)) {
      continue;
    }
    normalized.push(value);
  }
  return normalized;
}

export function resolveOpeningSceneCharacters(
  characters: CharacterResponse[],
  openingCharacterIds: string[],
  openingMessages: WorldOpeningMessage[],
): CharacterResponse[] {
  const byId = new Map(characters.map((character) => [character.id, character] as const));
  const byName = new Map(
    characters
      .map((character) => [character.name.trim(), character] as const)
      .filter(([name]) => name),
  );
  const resolved: CharacterResponse[] = [];

  for (const characterId of openingCharacterIds) {
    const character = byId.get(characterId);
    if (character && !resolved.some((item) => item.id === character.id)) {
      resolved.push(character);
    }
  }

  for (const message of openingMessages) {
    if (message.role !== "agent") {
      continue;
    }
    const speaker = message.speaker?.trim() ?? "";
    const character = byName.get(speaker);
    if (character && !resolved.some((item) => item.id === character.id)) {
      resolved.push(character);
    }
  }

  return resolved;
}

export function resolveOpeningSpeakerLabel(message: WorldOpeningMessage): string {
  if (message.role === "system") {
    return "系统";
  }
  return message.speaker?.trim() || "未指定角色";
}

export function createNewWorldDraft(): WorldResponse {
  return {
    id: "new",
    name: "新世界",
    genre: "",
    background_prompt: "",
    opening_scene: "",
    summary: "",
    time_system: "",
    map_nodes: defaultMapTopology(),
    triggers: [],
    time_config: { ...defaultTimeConfig },
    director_config: { ...defaultDirectorConfig },
    ui_theme_config: buildUiThemeEnvelope(defaultUiThemeConfig),
    director_system_prompt_base: "",
    director_runtime_system_prompt: "",
    opening_messages: [],
    opening_character_ids: [],
    player_character_id: null,
  };
}

export function buildTimeSystemSummary(config: TimeConfig): string {
  if (config.mode === "24h") {
    return `24 小时制，从 ${config.start_time} 开始，由世界主控按剧情推进时间。`;
  }

  const slotSummary = config.slots
    .filter((slot) => slot.label.trim() || slot.clock.trim())
    .map((slot) => (slot.clock.trim() ? `${slot.label.trim()}（${slot.clock.trim()}）` : slot.label.trim()))
    .filter(Boolean)
    .join("、");
  const startSummary = config.start_label.trim() ? `起始时段 ${config.start_label.trim()}` : `起始时间 ${config.start_time}`;
  return `标签序列时间系统，${startSummary}，时段按列表顺序推进。可用时段：${slotSummary || "未配置"}`;
}

export function buildThemePreviewStyle(config: UiThemeConfig, openingScene: string): CSSProperties & Record<string, string> {
  const assetRef =
    (config.background_source_mode === "local-first" || config.background_source_mode === "local-only")
      ? resolvePreviewBackgroundAsset(config, openingScene)
      : "";
  return assetRef ? { "--game-runtime-bg-image": `url("${assetUrl(assetRef)}")` } : {};
}

export function buildStatusTabOptions(): StatusTabOption[] {
  return [
    {
      key: "map",
      label: "地图",
      content: "地图拓扑由世界包 JSON 提供。",
      owners: ["系统"],
    },
  ];
}

export type UiGovernanceSnapshot = {
  loading: boolean;
  error: string | null;
  bundle: WorldUiBundleValidationResult | null;
  desktopCompile: WorldUiCompileResult | null;
  mobileCompile: WorldUiCompileResult | null;
  compatibility: WorldUiCompatibilityReport | null;
};

