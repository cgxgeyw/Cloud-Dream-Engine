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
  fetchWorldOpeningPromptPreview,
  isTauriEnvironment,
  uploadFile,
  updateWorld,
  validateWorldUiBundle,
  verifyWorldPackageUiCompatibility,
  type CharacterResponse,
  type CustomAttributeDefinition,
  type ModelConfigResponse,
  type McpToolResponse,
  type WorldUiBundleValidationResult,
  type WorldUiCompileResult,
  type WorldUiCompatibilityReport,
  type WorldCreateRequest,
  type WorldMapTopology,
  type WorldOpeningPromptPreviewResponse,
  type WorldOpeningMessage,
  type WorldResponse,
} from "../data/apiAdapter";
import { ConfirmDialog } from "../components/ModalDialog";
import { GameUiPreview } from "../components/GameUiPreview";
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

const fixedTabs = [
  { id: "basic", label: "基础信息", summary: "维护世界名称、类型、摘要和默认玩家身份。" },
  { id: "background", label: "世界背景", summary: "维护世界客观设定和共享背景资料。" },
  { id: "opening", label: "开场配置", summary: "配置新游戏开局消息和初始在场角色。" },
  { id: "time", label: "时间系统", summary: "配置时间推进模式、起始时间和时段列表。" },
  { id: "map", label: "地图", summary: "使用 JSON 描述世界地图层级拓扑。" },
  { id: "customAttributes", label: "自定义属性", summary: "配置世界自定义属性项和角色共享属性项。" },
  { id: "director", label: "世界主控", summary: "配置世界主控权限、记忆和提示词。" },
  { id: "promptPreview", label: "Prompt 预览", summary: "查看实际发送给模型的 prompt 内容。" },
  { id: "style", label: "界面风格", summary: "配置背景素材和世界包 UI 文档。" },
  { id: "configPreview", label: "配置预览", summary: "查看当前世界完整 JSON 配置。" },
] as const;

function createLegacyGameUiFile(platform: GameUiPlatform): string {
  const legacyDocument = platform === "desktop"
    ? {
        schema_version: 1 as const,
        meta: {
          name: "Desktop Gameplay UI (Legacy)",
          platform: "desktop",
        },
        layout: {
          root: {
            type: "grid",
            columns: ["minmax(280px, 1fr)", "minmax(360px, 1.24fr)", "minmax(240px, 0.82fr)"],
            rows: ["auto", "minmax(0, 1fr)"],
            areas: [
              ["header", "header", "header"],
              ["scene", "chat", "side"],
            ],
            gap: "16px",
            padding: "18px",
            style: {
              height: "100%",
              min_height: "0",
            },
            children: [
              { type: "mount", mount: "header", area: "header" },
              {
                type: "stack",
                area: "scene",
                gap: "12px",
                style: {
                  height: "100%",
                  min_height: "0",
                },
                children: [
                  { type: "mount", mount: "scene_focus" },
                  { type: "mount", mount: "character_bar" },
                ],
              },
              {
                type: "stack",
                area: "chat",
                gap: "12px",
                style: {
                  height: "100%",
                  min_height: "0",
                },
                children: [
                  { type: "mount", mount: "narration" },
                  { type: "mount", mount: "message_list" },
                  { type: "mount", mount: "input_area" },
                ],
              },
              { type: "mount", mount: "side_panel", area: "side" },
              {
                type: "absolute",
                children: [
                  {
                    type: "mount",
                    mount: "floating_actions",
                    anchor: {
                      top: "18px",
                      right: "20px",
                    },
                  },
                ],
              },
            ],
          },
        },
        mounts: {
          side_panel: {
            tab_order: ["map"],
          },
        },
        tokens: {},
        components: {},
        effects: {},
        custom_css: "",
      }
    : {
        schema_version: 1 as const,
        meta: {
          name: "Mobile Gameplay UI (Legacy)",
          platform: "mobile",
        },
        layout: {
          root: {
            type: "stack",
            direction: "vertical",
            gap: "10px",
            padding: "12px",
            style: {
              height: "100%",
              min_height: "0",
            },
            children: [
              { type: "mount", mount: "header" },
              { type: "mount", mount: "scene_focus" },
              { type: "mount", mount: "character_bar" },
              { type: "mount", mount: "narration" },
              {
                type: "mount",
                mount: "message_list",
                style: {
                  flex: "1 1 0",
                  min_height: "0",
                },
              },
              { type: "mount", mount: "side_panel" },
              { type: "mount", mount: "input_area" },
              {
                type: "absolute",
                children: [
                  {
                    type: "mount",
                    mount: "floating_actions",
                    anchor: {
                      top: "12px",
                      right: "12px",
                    },
                  },
                ],
              },
            ],
          },
        },
        mounts: {
          side_panel: {
            tab_order: ["map"],
          },
        },
        tokens: {},
        components: {},
        effects: {},
        custom_css: "",
      };

  return `${JSON.stringify(legacyDocument, null, 2)}\n`;
}

type FixedTabId = (typeof fixedTabs)[number]["id"];
type OpeningComposerRole = "system" | "agent";

type OpeningSpeakerOption = {
  value: string;
  label: string;
};

type StatusTabOption = {
  key: string;
  label: string;
  content: string;
  owners: string[];
};

type FoldableEditorSectionProps = {
  title: string;
  description?: string;
  badge?: string | null;
  defaultOpen?: boolean;
  children: ReactNode;
};

type DirectorConfig = {
  allow_scene_transition: boolean;
  allow_npc_spawn: boolean;
  history_dialogue_rounds: number;
  director_tool_loop_limit: number;
  director_model: string;
  character_memory_hit_turns: number;
  character_memory_event_window_rounds: number;
  character_memory_dialogue_window_rounds: number;
  character_memory_retrieval_mode: "lexical_only" | "hybrid" | "semantic_only";
  character_memory_candidate_limit: number;
  character_memory_semantic_weight: number;
  world_director_prompt: string;
  prompt_presets: PromptPreset[];
  return_processing_rules: ReturnProcessingRule[];
  allowed_mcp_tool_ids: string[];
};

type PromptPreset = {
  id: string;
  name: string;
  content: string;
  scope: "director" | "character" | "both";
  enabled: boolean;
  order: number;
};

type ReturnProcessingRule = {
  id: string;
  name: string;
  scope: "director" | "character" | "both";
  pattern: string;
  replacement: string;
  enabled: boolean;
  order: number;
};

type TimeSlot = {
  label: string;
  clock: string;
};

type TimeConfig = {
  mode: "labels" | "24h";
  slots: TimeSlot[];
  start_label: string;
  start_time: string;
};

type UiThemeConfig = {
  background_source_mode: string;
  portrait_source_mode: string;
  runtime_image_generation_enabled: boolean;
  local_background_assets: string[];
  local_scene_backgrounds: Record<string, string[]>;
  desktop_file: string;
  mobile_file: string;
};

type GameUiSchemaVersion = 1 | 2;

const defaultWorldDirectorPrompt = `你是世界主控。你只根据玩家可编辑提示词、世界资料、当前状态、聊天记录和工具资料做状态决策。

你需要返回一个 JSON 对象，用于描述下一步世界状态。常用字段包括：
- world_phase
- next_scene_name
- next_location
- next_time_label
- scene_visible_characters
- planned_speakers
- current_speaker
- current_line
- next_scene_background_hint
- character_visual_directives

要求：
1. 只返回 JSON，不要输出额外解释。
2. 你负责推进世界状态，不直接替角色说对白。
3. current_scene_character_roster 和 scene_visible_characters 表示当前场景在场角色，不是世界全体角色。
4. planned_speakers 表示本回合应当依次发言的 NPC 列表，通常不应为空，也不应长期只包含同一个人。
5. 如果当前场景有多名在场 NPC，且玩家输入不是明确只点名某一人、也不是剧情上只需要单人回应，则 planned_speakers 默认安排 2 到 3 人，让多人依次发言。
6. 只有在玩家明确只对一人说话、当前场景实际只有一名 NPC 在场，或剧情推进确实应由单人回应时，才让一名 NPC 单独发言。
7. 如果场景不该切换，就保持当前场景和地点。
8. 只让当前场景里合理存在的角色进入 visible / planned 列表。`;

const defaultDirectorConfig: DirectorConfig = {
  allow_scene_transition: true,
  allow_npc_spawn: true,
  history_dialogue_rounds: 6,
  director_tool_loop_limit: 4,
  director_model: "",
  character_memory_hit_turns: 2,
  character_memory_event_window_rounds: 10,
  character_memory_dialogue_window_rounds: 2,
  character_memory_retrieval_mode: "hybrid",
  character_memory_candidate_limit: 200,
  character_memory_semantic_weight: 0.65,
  world_director_prompt: defaultWorldDirectorPrompt,
  prompt_presets: [],
  return_processing_rules: [],
  allowed_mcp_tool_ids: [],
};

const defaultTimeConfig: TimeConfig = {
  mode: "labels",
  slots: [
    { label: "清晨", clock: "06:00" },
    { label: "正午", clock: "12:00" },
    { label: "夜晚", clock: "20:00" },
  ],
  start_label: "清晨",
  start_time: "08:00",
};

const defaultUiThemeConfig: UiThemeConfig = {
  background_source_mode: "local-first",
  portrait_source_mode: "local-first",
  runtime_image_generation_enabled: false,
  local_background_assets: [],
  local_scene_backgrounds: {},
  desktop_file: defaultGameUiFile("desktop"),
  mobile_file: defaultGameUiFile("mobile"),
};

function resolveExposurePolicyMode(policy: string | Record<string, unknown> | undefined): string {
  if (typeof policy === "string") {
    const normalized = policy.trim();
    return normalized || "on-demand";
  }
  const mode = typeof policy?.mode === "string" ? policy.mode.trim() : "";
  return mode || "on-demand";
}

function normalizeAssetGroupMap(raw: unknown): Record<string, string[]> {
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

function appendUniqueAsset(items: string[], nextItem: string): string[] {
  const value = nextItem.trim();
  if (!value) {
    return items;
  }
  return items.includes(value) ? items : [...items, value];
}

function removeAsset(items: string[], target: string): string[] {
  return items.filter((item) => item !== target);
}

function moveAssetToFront(items: string[], target: string): string[] {
  const next = removeAsset(items, target);
  return items.includes(target) ? [target, ...next] : next;
}

function getAssetDisplayName(assetPath: string): string {
  const parts = assetPath.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? assetPath;
}

function defaultMapTopology(): WorldMapTopology {
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

function extractMapSceneNames(topology: WorldMapTopology | null | undefined): string[] {
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

function formatMapTopologyJson(topology: WorldMapTopology | null | undefined): string {
  return JSON.stringify(topology ?? defaultMapTopology(), null, 2);
}

function parseMapTopologyJson(source: string): WorldMapTopology {
  const parsed = JSON.parse(source) as unknown;
  if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
    return parsed as WorldMapTopology;
  }
  throw new Error("地图拓扑必须是 JSON 对象，不能使用旧数组格式。请填写包含 root、tree 或 nodes 的对象。");
}

function resolveSceneNames(world: WorldResponse | null): string[] {
  if (!world) {
    return ["未命名场景"];
  }

  const names = [world.opening_scene, ...extractMapSceneNames(world.map_nodes)]
    .map((item) => item.trim())
    .filter(Boolean);
  return names.length > 0 ? Array.from(new Set(names)) : ["未命名场景"];
}

function resolvePreviewBackgroundAsset(config: UiThemeConfig, sceneName: string | null): string {
  const sceneAssets = sceneName ? config.local_scene_backgrounds[sceneName] ?? [] : [];
  if (sceneAssets.length > 0) {
    return sceneAssets[0];
  }
  return config.local_background_assets[0] ?? "";
}

function countGroupedAssets(groups: Record<string, string[]>): number {
  return Object.values(groups).reduce((total, items) => total + items.length, 0);
}

function composeDirectorRuntimeSystemPrompt(basePrompt: string, extraPrompt: string): string {
  const base = basePrompt.trim();
  const extra = extraPrompt.trim();
  if (!extra) {
    return base;
  }
  return [base, "以下为世界包追加的运行时提示词：", extra].filter(Boolean).join("\n\n");
}

function FoldableEditorSection({
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

function normalizeDirectorConfig(raw: Record<string, unknown> | undefined): DirectorConfig {
  const normalizeScope = (value: unknown): "director" | "character" | "both" =>
    value === "director" || value === "character" || value === "both" ? value : "both";
  return {
    allow_scene_transition:
      typeof raw?.allow_scene_transition === "boolean"
        ? raw.allow_scene_transition
        : defaultDirectorConfig.allow_scene_transition,
    allow_npc_spawn:
      typeof raw?.allow_npc_spawn === "boolean" ? raw.allow_npc_spawn : defaultDirectorConfig.allow_npc_spawn,
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
  };
}

function normalizeTimeConfig(raw: Record<string, unknown> | undefined): TimeConfig {
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

function normalizeUiThemeConfig(raw: Record<string, unknown> | undefined): UiThemeConfig {
  const envelope = normalizeWorldUiEnvelope(raw);
  const assets = normalizeAssetConfig(envelope.assets);
  return {
    background_source_mode: assets.background_source_mode,
    portrait_source_mode: assets.portrait_source_mode,
    runtime_image_generation_enabled: assets.runtime_image_generation_enabled,
    local_background_assets: assets.local_background_assets,
    local_scene_backgrounds: assets.local_scene_backgrounds,
    desktop_file: envelope.desktop_file,
    mobile_file: envelope.mobile_file,
  };
}

function buildUiThemeEnvelope(config: UiThemeConfig): Record<string, unknown> {
  return {
    assets: {
      background_source_mode: config.background_source_mode,
      portrait_source_mode: config.portrait_source_mode,
      runtime_image_generation_enabled: config.runtime_image_generation_enabled,
      local_background_assets: config.local_background_assets,
      local_scene_backgrounds: config.local_scene_backgrounds,
    },
    desktop_file: config.desktop_file,
    mobile_file: config.mobile_file,
  };
}

function normalizeOpeningMessages(raw: unknown): WorldOpeningMessage[] {
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

function normalizeOpeningCharacterIds(raw: unknown, characters: CharacterResponse[]): string[] {
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

function resolveOpeningSceneCharacters(
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

function resolveOpeningSpeakerLabel(message: WorldOpeningMessage): string {
  if (message.role === "system") {
    return "系统";
  }
  return message.speaker?.trim() || "未指定角色";
}

function createNewWorldDraft(): WorldResponse {
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
    custom_tabs: {},
    world_custom_attribute_definitions: [],
    character_custom_attribute_definitions: [],
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

function createCustomAttributeDefinition(name: string, order: number): CustomAttributeDefinition {
  const trimmedName = name.trim();
  const fallbackId = `custom_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 7)}`;
  const id = trimmedName
    .toLowerCase()
    .replace(/[^\w-]+/g, "_")
    .replace(/^_+|_+$/g, "") || fallbackId;

  return {
    id,
    name: trimmedName || "新属性项",
    value_type: "longText",
    order,
    enabled: true,
    required: false,
    placeholder: "",
    default_value: "",
  };
}

function normalizeCustomAttributeDefinitions(
  definitions: CustomAttributeDefinition[] | undefined,
): CustomAttributeDefinition[] {
  return (definitions ?? [])
    .filter((definition) => definition.name.trim())
    .map((definition, index) => ({
      ...definition,
      id: definition.id.trim() || createCustomAttributeDefinition(definition.name, index).id,
      name: definition.name.trim(),
      value_type: definition.value_type === "text" ? "text" : "longText",
      order: index,
      enabled: definition.enabled !== false,
      required: Boolean(definition.required),
      placeholder: definition.placeholder ?? "",
      default_value: definition.default_value ?? "",
    }));
}

function buildCustomAttributeDefinitionsFromNames(names: string[]): CustomAttributeDefinition[] {
  const seen = new Set<string>();
  return names
    .map((name) => name.trim())
    .filter((name) => name && !seen.has(name) && seen.add(name))
    .map((name, index) => createCustomAttributeDefinition(name, index));
}

function syncWorldCustomTabsWithDefinitions(
  values: Record<string, string>,
  definitions: CustomAttributeDefinition[],
): Record<string, string> {
  const next = { ...values };
  for (const definition of definitions) {
    if (!Object.prototype.hasOwnProperty.call(next, definition.name)) {
      next[definition.name] = definition.default_value ?? "";
    }
  }
  return next;
}

function buildTimeSystemSummary(config: TimeConfig): string {
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

function buildThemePreviewStyle(config: UiThemeConfig, openingScene: string): CSSProperties & Record<string, string> {
  const assetRef =
    (config.background_source_mode === "local-first" || config.background_source_mode === "local-only")
      ? resolvePreviewBackgroundAsset(config, openingScene)
      : "";
  return assetRef ? { "--game-runtime-bg-image": `url("${assetUrl(assetRef)}")` } : {};
}

function buildStatusTabOptions(customTabs: Record<string, string>): StatusTabOption[] {
  return [
    {
      key: "map",
      label: "地图",
      content: "地图拓扑由世界包 JSON 提供。",
      owners: ["系统"],
    },
    ...Object.entries(customTabs)
      .filter(([label]) => label.trim())
      .map(([label, content]) => ({
        key: `custom:${label}`,
        label,
        content: typeof content === "string" ? content : "",
        owners: ["示例角色"],
      })),
  ];
}

function buildStatusTabOptionsFromCharacters(
  characters: CharacterResponse[],
  definitions: CustomAttributeDefinition[] = [],
): StatusTabOption[] {
  const customTabs = new Map<string, StatusTabOption>();

  for (const definition of normalizeCustomAttributeDefinitions(definitions).filter((item) => item.enabled)) {
    customTabs.set(`custom:${definition.name}`, {
      key: `custom:${definition.name}`,
      label: definition.name,
      content: definition.default_value,
      owners: ["角色模板"],
    });
  }

  for (const character of characters) {
    const owner = character.name.trim() || "未命名角色";
    for (const [rawLabel, rawContent] of Object.entries(character.custom_tabs ?? {})) {
      const label = rawLabel.trim();
      if (!label || (definitions.length > 0 && !customTabs.has(`custom:${label}`))) {
        continue;
      }
      const key = `custom:${label}`;
      const content = typeof rawContent === "string" ? rawContent.trim() : "";
      const existing = customTabs.get(key);
      if (existing) {
        if (!existing.owners.includes(owner)) {
          existing.owners.push(owner);
        }
        if (!existing.content && content) {
          existing.content = content;
        }
        continue;
      }

      customTabs.set(key, {
        key,
        label,
        content,
        owners: [owner],
      });
    }
  }

  return [
    {
      key: "map",
      label: "地图",
      content: "地图拓扑由世界包 JSON 提供。",
      owners: ["系统"],
    },
    ...Array.from(customTabs.values()),
  ];
}

function resolveStatusTabPreviewContent(
  tab: StatusTabOption | null,
  previewCharacter: CharacterResponse | null,
): string {
  if (!tab) {
    return "";
  }

  if (tab.key === "map") {
    return tab.content;
  }

  const tabName = tab.key.startsWith("custom:") ? tab.key.slice("custom:".length) : tab.label;
  const previewContent = previewCharacter?.custom_tabs?.[tabName];
  if (typeof previewContent === "string" && previewContent.trim()) {
    return previewContent.trim();
  }
  if (typeof previewContent === "string") {
    return "该角色尚未填写这个自定义页签。";
  }

  const owners = tab.owners.join(" / ");
  if (previewCharacter) {
    return owners
      ? `当前角色没有“${tab.label}”页签。拥有该页签的角色：${owners}。`
      : "当前角色没有这个自定义页签。";
  }
  return owners ? `拥有该页签的角色：${owners}。` : "暂无预览内容。";
}

interface WorldEditorPageProps {
  isMobile?: boolean;
}

type UiGovernanceSnapshot = {
  loading: boolean;
  error: string | null;
  bundle: WorldUiBundleValidationResult | null;
  desktopCompile: WorldUiCompileResult | null;
  mobileCompile: WorldUiCompileResult | null;
  compatibility: WorldUiCompatibilityReport | null;
};

export function WorldEditorPage({ isMobile = false }: WorldEditorPageProps) {
  const navigate = useNavigate();
  const { id } = useParams();
  const isNew = id === "new" || !id;
  const [activeTab, setActiveTab] = useState<FixedTabId>("basic");
  const [activeSection, setActiveSection] = useState<FixedTabId | null>(null);
  const [previewPlatform, setPreviewPlatform] = useState<GameUiPlatform>(isMobile ? "mobile" : "desktop");
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
  const [openingComposerRole, setOpeningComposerRole] = useState<OpeningComposerRole>("system");
  const [openingComposerSpeaker, setOpeningComposerSpeaker] = useState("");
  const [openingComposerContent, setOpeningComposerContent] = useState("");
  const [mcpToolSearch, setMcpToolSearch] = useState("");
  const [promptPreview, setPromptPreview] = useState<WorldOpeningPromptPreviewResponse | null>(null);
  const [promptPreviewLoading, setPromptPreviewLoading] = useState(false);
  const [promptPreviewError, setPromptPreviewError] = useState<string | null>(null);
  const [mapTopologySource, setMapTopologySource] = useState(formatMapTopologyJson(createNewWorldDraft().map_nodes));
  const [uiGovernance, setUiGovernance] = useState<UiGovernanceSnapshot>({
    loading: false,
    error: null,
    bundle: null,
    desktopCompile: null,
    mobileCompile: null,
    compatibility: null,
  });
  const pendingTimeSlotFocusRef = useRef(false);
  const timeSlotsContainerRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    setPreviewPlatform(isMobile ? "mobile" : "desktop");
  }, [isMobile]);

  useEffect(() => {
    let cancelled = false;
    async function loadData() {
      try {
        setLoading(true);
        setError(null);
        if (isNew) {
          const [toolData, modelData] = await Promise.all([fetchMcpTools(), fetchModels("text")]);
          if (cancelled) {
            return;
          }
          const draft = createNewWorldDraft();
          setWorld(draft);
          setMapTopologySource(formatMapTopologyJson(draft.map_nodes));
          setCharacters([]);
          setMcpTools(toolData);
          setTextModels(modelData);
          return;
        }
        const [worldData, characterData, toolData, modelData] = await Promise.all([
          fetchWorld(id as string),
          fetchWorldCharacters(id as string),
          fetchMcpTools(),
          fetchModels("text"),
        ]);
        if (!cancelled) {
          const worldAttributeDefinitions = normalizeCustomAttributeDefinitions(
            worldData.world_custom_attribute_definitions,
          );
          const characterAttributeDefinitions = normalizeCustomAttributeDefinitions(
            worldData.character_custom_attribute_definitions,
          );
          const normalizedWorld = {
            ...worldData,
            time_config: normalizeTimeConfig(worldData.time_config as Record<string, unknown>),
            director_config: normalizeDirectorConfig(worldData.director_config as Record<string, unknown>),
            ui_theme_config: buildUiThemeEnvelope(normalizeUiThemeConfig(worldData.ui_theme_config as Record<string, unknown>)),
            opening_messages: normalizeOpeningMessages(worldData.opening_messages),
            opening_character_ids: normalizeOpeningCharacterIds(worldData.opening_character_ids, characterData),
            world_custom_attribute_definitions: worldAttributeDefinitions.length > 0
              ? worldAttributeDefinitions
              : buildCustomAttributeDefinitionsFromNames(Object.keys(worldData.custom_tabs ?? {})),
            character_custom_attribute_definitions: characterAttributeDefinitions.length > 0
              ? characterAttributeDefinitions
              : buildCustomAttributeDefinitionsFromNames(
                  characterData.flatMap((character) => Object.keys(character.custom_tabs ?? {})),
                ),
          };
          setWorld(normalizedWorld);
          setMapTopologySource(formatMapTopologyJson(normalizedWorld.map_nodes));
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

  const timeConfig = normalizeTimeConfig((world?.time_config ?? {}) as Record<string, unknown>);
  const directorConfig = normalizeDirectorConfig((world?.director_config ?? {}) as Record<string, unknown>);
  useLayoutEffect(() => {
    if (!pendingTimeSlotFocusRef.current) {
      return;
    }
    pendingTimeSlotFocusRef.current = false;
    const container = timeSlotsContainerRef.current;
    const focusTarget = container?.querySelector<HTMLInputElement>('[data-time-slot-label="true"]:last-of-type');
    focusTarget?.scrollIntoView({ block: "center", behavior: "smooth" });
    focusTarget?.focus();
  }, [timeConfig.slots.length]);
  const uiThemeConfig = normalizeUiThemeConfig((world?.ui_theme_config ?? {}) as Record<string, unknown>);
  const openingMessages = normalizeOpeningMessages(world?.opening_messages ?? []);
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
  const timeSystemSummary = useMemo(() => buildTimeSystemSummary(timeConfig), [timeConfig]);
  const directorSystemPromptPreview = useMemo(
    () => directorConfig.world_director_prompt,
    [directorConfig.world_director_prompt],
  );
  const availableStatusTabs = useMemo(
    () => buildStatusTabOptionsFromCharacters(characters, world?.character_custom_attribute_definitions ?? []),
    [characters, world?.character_custom_attribute_definitions],
  );
  const customStatusTabs = useMemo(
    () => availableStatusTabs.filter((item) => item.key.startsWith("custom:")),
    [availableStatusTabs],
  );
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
  const previewBackgroundAsset = useMemo(
    () => resolvePreviewBackgroundAsset(uiThemeConfig, world?.opening_scene?.trim() || sceneNames[0] || "未命名场景"),
    [sceneNames, uiThemeConfig, world?.opening_scene],
  );
  const desktopPreviewScopeId = normalizeGameUiScopeId(useId());
  const desktopPreviewScopeSelector = useMemo(() => createGameUiScopeSelector(desktopPreviewScopeId), [desktopPreviewScopeId]);
  const desktopPreviewStylesheet = useMemo(
    () => buildGameUiStylesheet(parsedDesktopGameUi.document, previewBackgroundAsset ? assetUrl(previewBackgroundAsset) : undefined, desktopPreviewScopeSelector),
    [desktopPreviewScopeSelector, parsedDesktopGameUi.document, previewBackgroundAsset],
  );
  const mobilePreviewScopeId = normalizeGameUiScopeId(useId());
  const mobilePreviewScopeSelector = useMemo(() => createGameUiScopeSelector(mobilePreviewScopeId), [mobilePreviewScopeId]);
  const mobilePreviewStylesheet = useMemo(
    () => buildGameUiStylesheet(parsedMobileGameUi.document, previewBackgroundAsset ? assetUrl(previewBackgroundAsset) : undefined, mobilePreviewScopeSelector),
    [mobilePreviewScopeSelector, parsedMobileGameUi.document, previewBackgroundAsset],
  );
  const totalSceneBackgroundCount = useMemo(
    () => countGroupedAssets(uiThemeConfig.local_scene_backgrounds),
    [uiThemeConfig.local_scene_backgrounds],
  );
  const openingSpeakerOptions = useMemo<OpeningSpeakerOption[]>(
    () => characters.map((character) => ({ value: character.name, label: character.name })),
    [characters],
  );
  const previewMessages = useMemo(
    () =>
      openingMessages.length > 0
        ? openingMessages
        : ([{ role: "system", content: "开场消息会显示在这里。请在开场配置中添加旁白或角色消息。" }] as WorldOpeningMessage[]),
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
        content: resolveStatusTabPreviewContent(tab, previewCharacter),
      })),
    [availableStatusTabs, previewCharacter],
  );
  const previewNarration = useMemo(
    () =>
      previewMessages.find((message) => message.role === "system" && message.content.trim())?.content.trim()
      || world?.summary?.trim()
      || "这里会显示世界旁白或当前场景说明。",
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
    () => mcpTools.filter((tool) => directorConfig.allowed_mcp_tool_ids.includes(tool.id)),
    [directorConfig.allowed_mcp_tool_ids, mcpTools],
  );
  const filteredMcpTools = useMemo(() => {
    const keyword = mcpToolSearch.trim().toLowerCase();
    if (!keyword) {
      return mcpTools;
    }
    return mcpTools.filter((tool) =>
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
  }, [mcpToolSearch, mcpTools]);
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
  const directorModelSelectValue = resolvedDirectorModelOption?.id ?? directorConfig.director_model.trim();
  const previewJson = useMemo(
    () =>
      JSON.stringify(
        {
          ...(world ?? {}),
          time_system: timeSystemSummary,
          time_config: timeConfig,
          director_config: directorConfig,
          ui_theme_config: buildUiThemeEnvelope(uiThemeConfig),
        },
        null,
        2,
      ),
    [world, timeConfig, directorConfig, uiThemeConfig, timeSystemSummary],
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

  useEffect(() => {
    if (isNew || activeTab !== "promptPreview" || !world?.id) {
      return;
    }

    const stableWorldId = world.id;
    const stablePlayerCharacterId = world.player_character_id;
    let cancelled = false;
    async function loadPromptPreview() {
      try {
        setPromptPreviewLoading(true);
        setPromptPreviewError(null);
        const data = await fetchWorldOpeningPromptPreview(stableWorldId, {
          playerCharacterId: stablePlayerCharacterId,
          playerInput: "缁х画",
        });
        if (!cancelled) setPromptPreview(data);
      } catch (previewError) {
        if (!cancelled) setPromptPreviewError(previewError instanceof Error ? previewError.message : "加载 prompt 预览失败");
      } finally {
        if (!cancelled) setPromptPreviewLoading(false);
      }
    }

    void loadPromptPreview();
    return () => {
      cancelled = true;
    };
  }, [activeTab, isNew, world?.id, world?.player_character_id]);

  function updateDraft(patch: Partial<WorldResponse>) {
    setWorld((current) => (current ? { ...current, ...patch } : current));
  }

  function updateAttributeDefinitions(
    field: "world_custom_attribute_definitions" | "character_custom_attribute_definitions",
    updater: (definitions: CustomAttributeDefinition[]) => CustomAttributeDefinition[],
  ) {
    setWorld((current) => {
      if (!current) return current;
      const definitions = normalizeCustomAttributeDefinitions(updater(current[field] ?? []));
      const patch: Partial<WorldResponse> = { [field]: definitions };
      if (field === "world_custom_attribute_definitions") {
        patch.custom_tabs = syncWorldCustomTabsWithDefinitions(current.custom_tabs, definitions);
      }
      return { ...current, ...patch };
    });
  }

  function addAttributeDefinition(field: "world_custom_attribute_definitions" | "character_custom_attribute_definitions") {
    updateAttributeDefinitions(field, (definitions) => [
      ...definitions,
      createCustomAttributeDefinition("新属性项", definitions.length),
    ]);
  }

  function patchAttributeDefinition(
    field: "world_custom_attribute_definitions" | "character_custom_attribute_definitions",
    index: number,
    patch: Partial<CustomAttributeDefinition>,
  ) {
    updateAttributeDefinitions(field, (definitions) =>
      definitions.map((definition, definitionIndex) =>
        definitionIndex === index ? { ...definition, ...patch } : definition,
      ),
    );
  }

  function removeAttributeDefinition(
    field: "world_custom_attribute_definitions" | "character_custom_attribute_definitions",
    index: number,
  ) {
    updateAttributeDefinitions(field, (definitions) =>
      definitions.filter((_, definitionIndex) => definitionIndex !== index),
    );
  }

  function moveAttributeDefinition(
    field: "world_custom_attribute_definitions" | "character_custom_attribute_definitions",
    index: number,
    direction: -1 | 1,
  ) {
    updateAttributeDefinitions(field, (definitions) => {
      const targetIndex = index + direction;
      if (targetIndex < 0 || targetIndex >= definitions.length) {
        return definitions;
      }
      const next = [...definitions];
      [next[index], next[targetIndex]] = [next[targetIndex], next[index]];
      return next;
    });
  }

  function updateWorldCustomAttributeValue(name: string, value: string) {
    if (!world) return;
    updateDraft({
      custom_tabs: {
        ...world.custom_tabs,
        [name]: value,
      },
    });
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

  async function reloadPromptPreview() {
    if (isNew || !world?.id) {
      setPromptPreviewError("新世界保存后才能生成 prompt 预览。");
      return;
    }
    try {
      setPromptPreviewLoading(true);
      setPromptPreviewError(null);
      const data = await fetchWorldOpeningPromptPreview(world.id, {
        playerCharacterId: world.player_character_id,
        playerInput: "缁х画",
      });
      setPromptPreview(data);
    } catch (previewError) {
      setPromptPreviewError(previewError instanceof Error ? previewError.message : "加载 prompt 预览失败");
    } finally {
      setPromptPreviewLoading(false);
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
  }

  function removeOpeningMessage(index: number) {
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
    updateOpeningMessages(nextMessages);
  }

  function updateGameUiFile(platform: GameUiPlatform, source: string) {
    updateThemePatch(platform === "desktop" ? { desktop_file: source } : { mobile_file: source });
  }

  function updateStructuredGameUiDocument(platform: GameUiPlatform, nextDocument: GameUiDocumentV2) {
    updateGameUiFile(platform, stringifyGameUiDocument(nextDocument));
  }

  function replaceGameUiSchema(platform: GameUiPlatform, schemaVersion: GameUiSchemaVersion) {
    updateGameUiFile(
      platform,
      schemaVersion === 2 ? defaultGameUiFile(platform) : createLegacyGameUiFile(platform),
    );
    setPreviewPlatform(platform);
    setError(null);
  }

  function renderGameUiDocumentEditor(
    platform: GameUiPlatform,
    parsed: ParsedGameUiDocument,
  ) {
    const label = platform === "desktop" ? "Desktop UI" : "Mobile UI";
    const source = platform === "desktop" ? uiThemeConfig.desktop_file : uiThemeConfig.mobile_file;
    const isActivePreview = previewPlatform === platform;
    const isLegacy = !parsed.error && parsed.document.schema_version === 1;
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
              {parsed.error
                ? "Current source is invalid. Preview is using fallback."
                : `schema_version: ${parsed.document.schema_version}${isLegacy ? " (legacy compatibility)" : " (component tree)"}`}
            </div>
          </div>
          <div className="flex flex--gap-sm" style={{ flexWrap: "wrap" }}>
            <button
              type="button"
              className={`action-btn${isActivePreview ? " action-btn--accent" : ""}`}
              onClick={() => setPreviewPlatform(platform)}
            >
              {isActivePreview ? "Previewing" : "Use for Preview"}
            </button>
            <button
              type="button"
              className="action-btn"
              onClick={() => replaceGameUiSchema(platform, 1)}
            >
              Load v1 Template
            </button>
            <button
              type="button"
              className="action-btn"
              onClick={() => replaceGameUiSchema(platform, 2)}
            >
              Load v2 Template
            </button>
          </div>
        </div>

        {parsed.error ? <div className="game-input-bubble">{parsed.error}</div> : null}
        {isLegacy ? (
          <div className="text-muted" style={{ fontSize: 12 }}>
            v1 is frozen for compatibility. New UI capabilities should be authored in v2.
          </div>
        ) : null}

        <label className="editor-field">
          <span className="editor-field-label">Raw JSONC</span>
          <textarea
            value={source}
            onChange={(event) => updateGameUiFile(platform, event.target.value)}
            className="editor-field-input editor-field-textarea"
            spellCheck={false}
            style={{ minHeight: 320, fontFamily: "Consolas, 'Courier New', monospace", fontSize: 12 }}
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
            <div className="editor-field-label">Backend Compile Snapshot</div>
            <div className="text-muted" style={{ fontSize: 12 }}>
              {compileResult.ok ? "Compile OK" : "Compile has errors"}
              {compatibilityDocument ? ` | Compatibility: ${compatibilityDocument.ok ? "OK" : "unsupported dependencies"}` : ""}
            </div>
            <div className="text-muted" style={{ fontSize: 12 }}>
              Components: {compileResult.component_dependencies.join(", ") || "none"}
            </div>
            <div className="text-muted" style={{ fontSize: 12 }}>
              Actions: {compileResult.action_dependencies.join(", ") || "none"}
            </div>
            <div className="text-muted" style={{ fontSize: 12 }}>
              Capabilities: {compileResult.capability_requirements.join(", ") || "none"}
            </div>
            {compileResult.diagnostics.length > 0 ? (
              <div style={{ display: "grid", gap: 6 }}>
                {compileResult.diagnostics.slice(0, 6).map((diagnostic, index) => (
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

        {!parsed.error && parsed.document.schema_version === 2 ? (
          <GameUiStructureEditor
            platform={platform}
            document={parsed.document as GameUiDocumentV2}
            onChangeDocument={(nextDocument) => updateStructuredGameUiDocument(platform, nextDocument)}
          />
        ) : null}
      </div>
    );
  }

  function renderGameUiPreviewSection() {
    const previewLabel = previewPlatform === "mobile" ? "手机端预览" : "电脑端预览";
    const parsedPreview = previewPlatform === "mobile" ? parsedMobileGameUi : parsedDesktopGameUi;
    const previewScopeId = previewPlatform === "mobile" ? mobilePreviewScopeId : desktopPreviewScopeId;
    const previewStylesheet = previewPlatform === "mobile" ? mobilePreviewStylesheet : desktopPreviewStylesheet;
    return (
      <FoldableEditorSection title="鐣岄潰棰勮">
        <div className="flex flex--items-center flex--justify-between world-editor-section-head" style={{ gap: 12 }}>
          <div className="editor-field-label">{previewLabel}</div>
          <div className="world-editor-section-head-action" style={{ display: "flex", gap: 8 }}>
            <button
              type="button"
              className="action-btn"
              onClick={() => setPreviewPlatform("desktop")}
              disabled={previewPlatform === "desktop"}
            >
              Desktop
            </button>
            <button
              type="button"
              className="action-btn"
              onClick={() => setPreviewPlatform("mobile")}
              disabled={previewPlatform === "mobile"}
            >
              Mobile
            </button>
          </div>
        </div>
        <div className={`world-style-preview-shell world-style-preview-shell--${previewPlatform}`}>
          <div className={`world-style-preview-frame world-style-preview-frame--${previewPlatform}`}>
            <GameUiPreview
              platform={previewPlatform}
              document={parsedPreview.document}
              stylesheet={previewStylesheet}
              scopeId={previewScopeId}
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
            />
          </div>
        </div>
      </FoldableEditorSection>
    );
  }

  function renderGameUiGovernanceSection() {
    const compatibilityDocuments = uiGovernance.compatibility?.documents ?? [];
    return (
      <FoldableEditorSection title="UI Governance">
        <div
          className="editor-content"
          style={{ padding: 12, border: "1px solid var(--color-border)", borderRadius: 16, display: "grid", gap: 10 }}
        >
          <div className="editor-field-label">Backend governance snapshot</div>
          <div className="text-muted" style={{ fontSize: 12 }}>
            {uiGovernance.loading
              ? "Refreshing backend validation, compile, and compatibility checks..."
              : uiGovernance.error
                ? uiGovernance.error
                : uiGovernance.bundle
                  ? `Bundle validation: ${uiGovernance.bundle.ok ? "OK" : "has issues"} | Compatibility: ${uiGovernance.compatibility?.ok ? "OK" : "has unsupported dependencies"}`
                  : "No governance snapshot available."}
          </div>
          {uiGovernance.bundle ? (
            <>
              <div className="text-muted" style={{ fontSize: 12 }}>
                Desktop errors: {uiGovernance.bundle.desktop.errors.length} | Mobile errors: {uiGovernance.bundle.mobile.errors.length}
              </div>
              <div className="text-muted" style={{ fontSize: 12 }}>
                Desktop warnings: {uiGovernance.bundle.desktop.warnings.length} | Mobile warnings: {uiGovernance.bundle.mobile.warnings.length}
              </div>
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
        </div>
      </FoldableEditorSection>
    );
  }

  function updateTimePatch(patch: Partial<TimeConfig>) {
    updateDraft({ time_config: { ...timeConfig, ...patch } });
  }

  function updateTimeSlot(index: number, patch: Partial<TimeSlot>) {
    updateTimePatch({
      slots: timeConfig.slots.map((slot, slotIndex) => (slotIndex === index ? { ...slot, ...patch } : slot)),
    });
  }

  function addTimeSlot() {
    pendingTimeSlotFocusRef.current = true;
    updateTimePatch({
      slots: [...timeConfig.slots, { label: `时段 ${timeConfig.slots.length + 1}`, clock: "" }],
    });
  }

  function removeTimeSlot(index: number) {
    const nextSlots = timeConfig.slots.filter((_, slotIndex) => slotIndex !== index);
    updateTimePatch({
      slots: nextSlots.length > 0 ? nextSlots : [{ label: "", clock: "" }],
    });
  }

  function updateDirectorPatch(patch: Partial<DirectorConfig>) {
    updateDraft({ director_config: { ...directorConfig, ...patch } });
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

  function updateMergedDirectorPrompt(value: string) {
    updateDirectorPatch({
      world_director_prompt: value,
    });
  }

  function updatePromptPreset(index: number, patch: Partial<PromptPreset>) {
    updateDirectorPatch({
      prompt_presets: directorConfig.prompt_presets.map((item, itemIndex) =>
        itemIndex === index ? { ...item, ...patch } : item,
      ),
    });
  }

  function addPromptPreset() {
    updateDirectorPatch({
      prompt_presets: [
        ...directorConfig.prompt_presets,
        {
          id: `preset-${Date.now()}`,
          name: "新提示词预设",
          content: "",
          scope: "both",
          enabled: true,
          order: directorConfig.prompt_presets.length + 1,
        },
      ],
    });
  }

  function removePromptPreset(index: number) {
    updateDirectorPatch({
      prompt_presets: directorConfig.prompt_presets.filter((_, itemIndex) => itemIndex !== index),
    });
  }

  function duplicatePromptPreset(index: number) {
    const source = directorConfig.prompt_presets[index];
    if (!source) return;
    updateDirectorPatch({
      prompt_presets: [
        ...directorConfig.prompt_presets,
        { ...source, id: `preset-${Date.now()}`, name: `${source.name} 副本`, order: directorConfig.prompt_presets.length + 1 },
      ],
    });
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
        time_system: timeSystemSummary,
        map_nodes: mapTopology,
        triggers: world.triggers,
        custom_tabs: syncWorldCustomTabsWithDefinitions(
          world.custom_tabs,
          normalizeCustomAttributeDefinitions(world.world_custom_attribute_definitions),
        ),
        world_custom_attribute_definitions: normalizeCustomAttributeDefinitions(
          world.world_custom_attribute_definitions,
        ),
        character_custom_attribute_definitions: normalizeCustomAttributeDefinitions(
          world.character_custom_attribute_definitions,
        ),
        time_config: timeConfig,
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
        world_custom_attribute_definitions: normalizeCustomAttributeDefinitions(
          saved.world_custom_attribute_definitions,
        ),
        character_custom_attribute_definitions: normalizeCustomAttributeDefinitions(
          saved.character_custom_attribute_definitions,
        ),
      };
      setWorld(normalizedSaved);
      setMapTopologySource(formatMapTopologyJson(normalizedSaved.map_nodes));
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
  function openSection(sectionId: FixedTabId) {
    setActiveSection(sectionId);
    setError(null);
  }

  function handleDetailBack() {
    if (activeSection) {
      setActiveSection(null);
    } else {
      navigate("/worlds");
    }
  }

  // ==================== Mobile-specific rendering ====================
  function renderMobileSectionList() {
    return (
      <SurfacePanel className="surface-panel--pad-lg">
        <div className="editor-content">
          {fixedTabs.map((tab) => (
            <button
              key={tab.id}
              type="button"
              onClick={() => openSection(tab.id)}
              className="action-btn"
              style={{
                width: "100%",
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
                padding: "16px 18px",
                textAlign: "left",
                borderRadius: 18,
              }}
            >
              <span style={{ display: "grid", gap: 4 }}>
                <span style={{ fontSize: 16, fontWeight: 700 }}>{tab.label}</span>
                <span className="text-muted" style={{ fontSize: 13 }}>{tab.summary}</span>
              </span>
              <span aria-hidden="true" style={{ fontSize: 18, opacity: 0.72 }}>›</span>
            </button>
          ))}
        </div>
      </SurfacePanel>
    );
  }

  function renderMobileSectionContent(sectionOverride?: FixedTabId, embedded = false) {
    const sectionId = sectionOverride ?? activeSection;
    if (!sectionId || !world) return null;

    const currentSectionMeta = fixedTabs.find((tab) => tab.id === sectionId);
    const mobileSaveBar = !embedded ? (
      <div className="world-editor-mobile-savebar">
        <button
          type="button"
          onClick={() => void handleSave()}
          disabled={saving || deleting || exporting}
          className="action-btn action-btn--accent world-editor-mobile-savebar-btn"
        >
          <Save size={16} />
          {saving ? "保存中..." : `保存${currentSectionMeta?.label ?? "当前修改"}`}
        </button>
      </div>
    ) : null;
    const renderCustomAttributeDefinitionEditor = (
      field: "world_custom_attribute_definitions" | "character_custom_attribute_definitions",
      title: string,
      description: string,
      showWorldValues = false,
    ) => {
      const definitions = normalizeCustomAttributeDefinitions(world[field]);
      return (
        <FoldableEditorSection title={title} defaultOpen>
          <div className="text-muted">{description}</div>
          {definitions.length === 0 ? <div className="empty-text">还没有属性项。</div> : null}
          {definitions.map((definition, index) => (
            <div key={definition.id} className="editor-content" style={{ padding: 12, border: "1px solid var(--color-border)", borderRadius: 12 }}>
              <div className="settings-form-grid">
                <label className="editor-field">
                  <span className="editor-field-label">属性名称</span>
                  <input
                    value={definition.name}
                    onChange={(event) => patchAttributeDefinition(field, index, { name: event.target.value })}
                    className="editor-field-input"
                  />
                </label>
                <label className="editor-field">
                  <span className="editor-field-label">输入类型</span>
                  <select
                    value={definition.value_type}
                    onChange={(event) => patchAttributeDefinition(field, index, { value_type: event.target.value === "text" ? "text" : "longText" })}
                    className="editor-field-input editor-field-select"
                  >
                    <option value="longText">长文本</option>
                    <option value="text">短文本</option>
                  </select>
                </label>
              </div>
              <div className="settings-form-grid">
                <label className="editor-field">
                  <span className="editor-field-label">占位提示</span>
                  <input
                    value={definition.placeholder}
                    onChange={(event) => patchAttributeDefinition(field, index, { placeholder: event.target.value })}
                    className="editor-field-input"
                  />
                </label>
                <label className="editor-field">
                  <span className="editor-field-label">默认内容</span>
                  <input
                    value={definition.default_value}
                    onChange={(event) => patchAttributeDefinition(field, index, { default_value: event.target.value })}
                    className="editor-field-input"
                  />
                </label>
              </div>
              <div className="flex flex--wrap" style={{ gap: 8 }}>
                <label className="settings-inline-toggle">
                  <input
                    type="checkbox"
                    checked={definition.enabled}
                    onChange={(event) => patchAttributeDefinition(field, index, { enabled: event.target.checked })}
                  />
                  启用
                </label>
                <label className="settings-inline-toggle">
                  <input
                    type="checkbox"
                    checked={definition.required}
                    onChange={(event) => patchAttributeDefinition(field, index, { required: event.target.checked })}
                  />
                  必填
                </label>
                <button type="button" className="action-btn" onClick={() => moveAttributeDefinition(field, index, -1)} disabled={index === 0}>上移</button>
                <button type="button" className="action-btn" onClick={() => moveAttributeDefinition(field, index, 1)} disabled={index === definitions.length - 1}>下移</button>
                <button type="button" className="action-btn action-btn--danger" onClick={() => removeAttributeDefinition(field, index)}>删除属性项</button>
              </div>
              {showWorldValues ? (
                <label className="editor-field">
                  <span className="editor-field-label">世界属性内容</span>
                  <textarea
                    value={world.custom_tabs[definition.name] ?? definition.default_value}
                    onChange={(event) => updateWorldCustomAttributeValue(definition.name, event.target.value)}
                    placeholder={definition.placeholder}
                    className="editor-field-input editor-field-textarea"
                    style={{ minHeight: definition.value_type === "text" ? 80 : 160 }}
                  />
                </label>
              ) : null}
            </div>
          ))}
          <button type="button" className="action-btn action-btn--accent" onClick={() => addAttributeDefinition(field)}>
            新增属性项
          </button>
        </FoldableEditorSection>
      );
    };

    return (
      <div className="editor-content">
        {!embedded ? <div className="settings-detail-head">
          <button type="button" onClick={handleDetailBack} className="action-btn settings-detail-back">
            <ArrowLeft size={14} /> 返回
          </button>
          <div className="settings-detail-head-copy">
            <strong>{currentSectionMeta?.label}</strong>
          </div>
        </div> : null}

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
              <div className="text-muted">
                这里填写世界的客观背景设定。这部分会作为角色和世界主控共享的世界常识读取，所以不要写“系统会怎样”“世界主控负责怎样”这类元信息。
              </div>
              <label className="editor-field">
                <span className="editor-field-label">世界背景设定</span>
                <textarea value={world.background_prompt} onChange={(e) => updateDraft({ background_prompt: e.target.value })} className="editor-field-input editor-field-textarea" style={{ minHeight: 220 }} />
              </label>
            </div>
          </SurfacePanel>
        ) : null}

        {sectionId === "opening" ? (
          <SurfacePanel className="surface-panel--pad-lg">
            <div className="editor-content">
              <div className="text-muted">这里配置新游戏开场时直接出现在聊天区里的内容，不会再让 NPC 自动代替世界做说明。</div>
              <div className="editor-content" style={{ padding: 12, border: "1px solid var(--color-border)", borderRadius: 16 }}>
                <div className="editor-field-label">开局在场 NPC</div>
                <div className="text-muted">这里决定新游戏开局时已经出现在场景里的角色。未勾选的世界角色不会自动出现在开局场景；如果它在开场消息里直接发言，也会自动视为在场。</div>
                {characters.length === 0 ? (
                  <div className="text-muted">当前世界还没有角色，先去角色池创建角色。</div>
                ) : (
                  <div className="settings-form-grid">
                    {characters.map((character) => {
                      const checked = openingCharacterIds.includes(character.id);
                      return (
                        <label
                          key={character.id}
                          className="editor-field"
                          style={{ padding: 12, border: "1px solid var(--color-border)", borderRadius: 14, gap: 8 }}
                        >
                          <span className="flex flex--items-center" style={{ gap: 10 }}>
                            <input
                              type="checkbox"
                              checked={checked}
                              onChange={() => toggleOpeningCharacter(character.id)}
                            />
                            <span>{character.name}</span>
                          </span>
                          <span className="text-muted">{character.role || "未设置身份"}</span>
                        </label>
                      );
                    })}
                  </div>
                )}
              </div>
              <div className="editor-content" style={{ padding: 12, border: "1px solid rgba(255,255,255,0.08)", borderRadius: 16 }}>
                <div className="settings-form-grid">
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
                    style={{ minHeight: 140 }}
                    placeholder="写一段开场旁白，或让指定角色说出第一句话。"
                  />
                </label>
                <div className="flex flex--justify-end">
                  <button type="button" className="action-btn action-btn--accent" onClick={addOpeningMessage}>加入开场聊天</button>
                </div>
              </div>
              <div className="editor-content" style={{ padding: 12, border: "1px solid var(--color-border)", borderRadius: 16 }}>
                <div className="editor-field-label">开场预览</div>
                <div className="text-muted">保存后，新游戏会直接以这里的消息列表作为聊天开局。</div>
                <div className="editor-content" style={{ gap: 8, padding: 0 }}>
                  <div className="editor-field-label">开场场景内角色</div>
                  {openingSceneCharacters.length > 0 ? (
                    <div className="game-scene-characters" style={{ justifyContent: "flex-start" }}>
                      {openingSceneCharacters.map((character) => (
                        <span key={character.id} className="game-scene-char">{character.name}</span>
                      ))}
                    </div>
                  ) : (
                    <div className="text-muted">当前没有配置开局就在场的 NPC。</div>
                  )}
                </div>
                <div className="opening-preview-messages">
                  {openingMessages.length === 0 ? <div className="text-muted">当前还没有开场消息。</div> : null}
                  {openingMessages.map((message, index) => (
                    <div key={`${message.role}-${index}-${message.content}`} className={`opening-preview-message opening-preview-message--${message.role}`}>
                      {message.role === "agent" ? <div className="opening-preview-speaker">{resolveOpeningSpeakerLabel(message)}</div> : null}
                      <div className={`opening-preview-content ${message.role === "system" ? "opening-preview-content--system" : "opening-preview-content--default"}`}>{message.content}</div>
                      <div className="flex flex--gap-sm" style={{ marginTop: 8 }}>
                        <button type="button" className="action-btn" onClick={() => moveOpeningMessage(index, -1)} disabled={index === 0}>上移</button>
                        <button type="button" className="action-btn" onClick={() => moveOpeningMessage(index, 1)} disabled={index === openingMessages.length - 1}>下移</button>
                        <button type="button" className="action-btn" onClick={() => removeOpeningMessage(index)}>删除</button>
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          </SurfacePanel>
        ) : null}

        {sectionId === "time" ? (
          <SurfacePanel className="surface-panel--pad-lg">
            <div className="editor-content">
              <label className="editor-field"><span className="editor-field-label">时间推进模式</span><select value={timeConfig.mode} onChange={(e) => updateTimePatch({ mode: e.target.value === "24h" ? "24h" : "labels" })} className="editor-field-input editor-field-select"><option value="labels">标签序列</option><option value="24h">24 小时制</option></select></label>
              <label className="editor-field"><span className="editor-field-label">起始时间</span><input value={timeConfig.start_time} onChange={(e) => updateTimePatch({ start_time: e.target.value })} className="editor-field-input" /></label>
              {timeConfig.mode === "labels" ? (
                <>
                  <label className="editor-field">
                    <span className="editor-field-label">起始时段</span>
                    <select value={timeConfig.start_label} onChange={(e) => updateTimePatch({ start_label: e.target.value })} className="editor-field-input editor-field-select">
                      {timeConfig.slots.map((slot, index) => {
                        const fallbackLabel = `时段 ${index + 1}`;
                        const optionLabel = slot.label.trim() || fallbackLabel;
                        return <option key={`${index}-${optionLabel}`} value={optionLabel}>{optionLabel}</option>;
                      })}
                    </select>
                  </label>
                  <div className="editor-content" ref={timeSlotsContainerRef}>
                    <div className="flex flex--items-center flex--justify-between world-editor-section-head" style={{ gap: 12 }}>
                      <div>
                        <div className="editor-field-label">时段列表</div>
                        <div className="text-muted">标签序列模式下按列表推进，每个时段都可以单独填写名称和时刻。</div>
                      </div>
                      <div className="world-editor-section-head-action">
                        <button type="button" className="action-btn" onClick={() => addTimeSlot()}>新增时段</button>
                      </div>
                    </div>
                    {timeConfig.slots.map((slot, index) => (
                      <div key={`time-slot-${index}`} className="settings-form-grid">
                        <label className="editor-field">
                          <span className="editor-field-label">时段名称</span>
                          <input
                            value={slot.label}
                            onChange={(e) => updateTimeSlot(index, { label: e.target.value })}
                            className="editor-field-input"
                            data-time-slot-label="true"
                            placeholder={`时段 ${index + 1}`}
                          />
                        </label>
                        <label className="editor-field">
                          <span className="editor-field-label">对应时刻</span>
                          <input
                            value={slot.clock}
                            onChange={(e) => updateTimeSlot(index, { clock: e.target.value })}
                            className="editor-field-input"
                            placeholder="06:00"
                          />
                        </label>
                        <div className="editor-field" style={{ alignSelf: "end" }}>
                          <button type="button" className="action-btn" onClick={() => removeTimeSlot(index)}>删除时段</button>
                        </div>
                      </div>
                    ))}
                  </div>
                </>
              ) : null}
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
              {renderCustomAttributeDefinitionEditor(
                "world_custom_attribute_definitions",
                "世界自定义属性项",
                "这些属性项用于记录世界观、规则、势力、历史等世界级资料。",
                true,
              )}
              {renderCustomAttributeDefinitionEditor(
                "character_custom_attribute_definitions",
                "角色自定义属性项",
                "这些属性项会作为所有角色共享的资料模板；角色编辑页只填写内容，不再单独增删 tab。",
              )}
            </div>
          </SurfacePanel>
        ) : null}

        {sectionId === "director" ? (
          <SurfacePanel className="surface-panel--pad-lg">
            <div className="editor-content">
              <FoldableEditorSection title="基础权限" defaultOpen>
                <div className="settings-form-grid">
                  <label className="editor-field">
                    <span className="editor-field-label">允许切换场景</span>
                    <div className="settings-inline-toggle">
                      <input type="checkbox" checked={directorConfig.allow_scene_transition} onChange={(e) => updateDirectorPatch({ allow_scene_transition: e.target.checked })} />
                    </div>
                  </label>
                  <label className="editor-field">
                    <span className="editor-field-label">允许生成新 NPC</span>
                    <div className="settings-inline-toggle">
                      <input type="checkbox" checked={directorConfig.allow_npc_spawn} onChange={(e) => updateDirectorPatch({ allow_npc_spawn: e.target.checked })} />
                    </div>
                  </label>
                </div>
              </FoldableEditorSection>

              <FoldableEditorSection title="运行上下文">
                <div className="settings-form-grid">
                  <label className="editor-field">
                    <span className="editor-field-label">历史对话轮数</span>
                    <input type="number" min="0" max="20" value={directorConfig.history_dialogue_rounds} onChange={(e) => updateDirectorPatch({ history_dialogue_rounds: Number(e.target.value) })} className="editor-field-input" />
                  </label>
                  <label className="editor-field">
                    <span className="editor-field-label">工具循环上限</span>
                    <input type="number" min="1" max="12" value={directorConfig.director_tool_loop_limit} onChange={(e) => updateDirectorPatch({ director_tool_loop_limit: Number(e.target.value) })} className="editor-field-input" />
                  </label>
                </div>
              </FoldableEditorSection>

              <FoldableEditorSection title="角色记忆">
                <div className="settings-form-grid">
                  <label className="editor-field">
                    <span className="editor-field-label">记忆触发轮数</span>
                    <input type="number" min="1" max="6" value={directorConfig.character_memory_hit_turns} onChange={(e) => updateDirectorPatch({ character_memory_hit_turns: Number(e.target.value) })} className="editor-field-input" />
                  </label>
                  <label className="editor-field">
                    <span className="editor-field-label">事件窗口轮数</span>
                    <input type="number" min="0" max="20" value={directorConfig.character_memory_event_window_rounds} onChange={(e) => updateDirectorPatch({ character_memory_event_window_rounds: Number(e.target.value) })} className="editor-field-input" />
                  </label>
                  <label className="editor-field">
                    <span className="editor-field-label">对话窗口轮数</span>
                    <input type="number" min="0" max="6" value={directorConfig.character_memory_dialogue_window_rounds} onChange={(e) => updateDirectorPatch({ character_memory_dialogue_window_rounds: Number(e.target.value) })} className="editor-field-input" />
                  </label>
                  <label className="editor-field">
                    <span className="editor-field-label">检索模式</span>
                    <select value={directorConfig.character_memory_retrieval_mode} onChange={(e) => updateDirectorPatch({ character_memory_retrieval_mode: e.target.value as any })} className="editor-field-input editor-field-select">
                      <option value="lexical_only">仅关键词</option>
                      <option value="hybrid">混合</option>
                      <option value="semantic_only">仅语义</option>
                    </select>
                  </label>
                  <label className="editor-field">
                    <span className="editor-field-label">候选数量</span>
                    <input type="number" min="20" max="600" value={directorConfig.character_memory_candidate_limit} onChange={(e) => updateDirectorPatch({ character_memory_candidate_limit: Number(e.target.value) })} className="editor-field-input" />
                  </label>
                  <label className="editor-field">
                    <span className="editor-field-label">语义权重</span>
                    <input type="number" min="0" max="1" step="0.05" value={directorConfig.character_memory_semantic_weight} onChange={(e) => updateDirectorPatch({ character_memory_semantic_weight: Number(e.target.value) })} className="editor-field-input" />
                  </label>
                </div>
              </FoldableEditorSection>

              <FoldableEditorSection title="主控提示词" defaultOpen>
                <label className="editor-field">
                  <span className="editor-field-label">世界主控提示词</span>
                  <select
                    value={directorModelSelectValue}
                    onChange={(e) => updateDirectorPatch({ director_model: e.target.value })}
                    className="editor-field-input editor-field-select"
                    style={{ marginBottom: 12 }}
                  >
                    <option value="">Use default text model</option>
                    {textModels.map((model) => (
                      <option key={model.id} value={model.id}>
                        {model.name || model.model_id}
                      </option>
                    ))}
                    {!resolvedDirectorModelOption && directorConfig.director_model.trim() ? (
                      <option value={directorConfig.director_model.trim()}>
                        {directorConfig.director_model.trim()} (current unmatched)
                      </option>
                    ) : null}
                  </select>
                  <textarea value={directorConfig.world_director_prompt} onChange={(e) => updateMergedDirectorPrompt(e.target.value)} className="editor-field-input editor-field-textarea" style={{ minHeight: 300 }} />
                </label>
                <div className="text-muted">角色系统模板、角色返回契约和角色旁白提示词已经移到角色池，每个角色单独维护。</div>
              </FoldableEditorSection>

              <FoldableEditorSection title="MCP 工具权限">
                <div className="editor-field-label">已选工具</div>
                {selectedMcpTools.length === 0 ? <div className="text-muted">当前未选择任何 MCP 工具。</div> : null}
                {selectedMcpTools.map((tool) => (
                  <div key={tool.id} className="settings-form-grid" style={{ padding: 12, border: "1px solid var(--color-border)", borderRadius: 12 }}>
                    <div>
                      <div>{tool.name}</div>
                      <div className="text-muted">{tool.description}</div>
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
                        <div>{tool.name}</div>
                        <div className="text-muted">{tool.description}</div>
                      </div>
                      <div className="editor-field" style={{ alignSelf: "center" }}>
                        <button type="button" className="action-btn" onClick={() => toggleAllowedMcpTool(tool.id)} disabled={directorConfig.allowed_mcp_tool_ids.includes(tool.id)}>添加</button>
                      </div>
                    </div>
                  ))}
                </div>
              </FoldableEditorSection>
            </div>
          </SurfacePanel>
        ) : null}

        {sectionId === "promptPreview" ? (
          <SurfacePanel className="surface-panel--pad-lg">
            <div className="editor-content">
              <div className="text-muted">这里展示实际发送给模型的 prompt 预览。</div>
              {promptPreviewLoading ? <div className="empty-text">正在加载 prompt 预览...</div> : null}
              {promptPreviewError ? <div className="text-error">{promptPreviewError}</div> : null}
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
            <div className="editor-content">
              <FoldableEditorSection title="背景素材" defaultOpen>
                <div className="editor-field-label">通用背景</div>
                {uiThemeConfig.local_background_assets.length === 0 ? <div className="text-muted">暂无通用背景。</div> : null}
                {uiThemeConfig.local_background_assets.map((asset, index) => (
                  <div key={asset} className="settings-form-grid" style={{ padding: 12, border: "1px solid var(--color-border)", borderRadius: 12 }}>
                    <div>
                      <div>{getAssetDisplayName(asset)}</div>
                      <div className="text-muted">{asset}</div>
                    </div>
                    <div className="editor-field" style={{ alignSelf: "center", gap: 8 }}>
                      {index > 0 ? <button type="button" className="action-btn" onClick={() => setPrimaryBackgroundAsset(asset)}>设为主背景</button> : null}
                      <button type="button" className="action-btn" onClick={() => removeBackgroundAsset(asset)}>删除</button>
                    </div>
                  </div>
                ))}
                <div className="editor-field-label">上传通用背景</div>
                <label className="action-btn action-btn--accent" style={{ cursor: uploadingBackground ? "wait" : "pointer" }}>
                  {uploadingBackground ? "上传中..." : "选择文件"}
                  <input type="file" accept="image/*,video/*" onChange={(e) => void handleUploadBackground(e.target.files?.[0] ?? null)} disabled={uploadingBackground} style={{ display: "none" }} />
                </label>
              </FoldableEditorSection>

              <FoldableEditorSection title="场景背景">
                {sceneNames.map((scene) => (
                  <div key={scene} className="editor-content" style={{ padding: 12, border: "1px solid var(--color-border)", borderRadius: 12, marginBottom: 12 }}>
                    <div className="editor-field-label">{scene}</div>
                    {(uiThemeConfig.local_scene_backgrounds[scene] ?? []).length === 0 ? <div className="text-muted">暂无场景背景。</div> : null}
                    {(uiThemeConfig.local_scene_backgrounds[scene] ?? []).map((asset, index) => (
                      <div key={asset} className="settings-form-grid" style={{ padding: 8, border: "1px solid var(--color-border-light)", borderRadius: 8, marginTop: 8 }}>
                        <div>
                          <div>{getAssetDisplayName(asset)}</div>
                          <div className="text-muted">{asset}</div>
                        </div>
                        <div className="editor-field" style={{ alignSelf: "center", gap: 8 }}>
                          {index > 0 ? <button type="button" className="action-btn" onClick={() => setPrimaryBackgroundAsset(asset, scene)}>设为主背景</button> : null}
                          <button type="button" className="action-btn" onClick={() => removeBackgroundAsset(asset, scene)}>删除</button>
                        </div>
                      </div>
                    ))}
                    <div className="editor-field-label">上传场景背景</div>
                    <label className="action-btn action-btn--accent" style={{ cursor: uploadingBackground ? "wait" : "pointer" }}>
                      {uploadingBackground ? "上传中..." : "选择文件"}
                      <input type="file" accept="image/*,video/*" onChange={(e) => void handleUploadBackground(e.target.files?.[0] ?? null, scene)} disabled={uploadingBackground} style={{ display: "none" }} />
                    </label>
                  </div>
                ))}
              </FoldableEditorSection>

              <FoldableEditorSection title="UI 文档">
                {renderGameUiDocumentEditor("desktop", parsedDesktopGameUi)}
                {renderGameUiDocumentEditor("mobile", parsedMobileGameUi)}
              </FoldableEditorSection>

              {renderGameUiPreviewSection()}
              {renderGameUiGovernanceSection()}
            </div>
          </SurfacePanel>
        ) : null}

        {sectionId === "configPreview" ? (
          <SurfacePanel className="surface-panel--pad-lg">
            <div className="editor-content">
              <div className="text-muted">当前世界完整 JSON 配置。</div>
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
      <div className="editor-content">
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
      title={isMobile && activeSection ? fixedTabs.find((tab) => tab.id === activeSection)?.label ?? "世界编辑" : world?.name ?? "世界编辑"}
      subtitle={
        isMobile && activeSection
          ? fixedTabs.find((tab) => tab.id === activeSection)?.summary ??
            "这里维护世界级信息、时间系统、地图、世界主控，以及跟世界绑定的游戏界面风格。"
          : "这里维护世界级信息、时间系统、地图、世界主控，以及跟世界绑定的游戏界面风格。"
      }
      toolbar={
        <div className="world-editor-toolbar">
          {isMobile ? (
            <button type="button" onClick={handleDetailBack} className="action-btn">
              <ArrowLeft size={14} /> 返回
            </button>
          ) : (
            <button type="button" onClick={() => navigate(-1)} className="action-btn">
              <ArrowLeft size={14} /> 返回
            </button>
          )}
          {isMobile ? (
            <button type="button" onClick={() => void handleSave()} disabled={saving || deleting || exporting} className="action-btn action-btn--accent">
              <Save size={14} /> {saving ? "保存中..." : "保存"}
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
          {!isMobile ? (
            <button type="button" onClick={() => void handleSave()} disabled={saving || deleting || exporting} className="action-btn action-btn--accent">
              {saving ? "保存中..." : "保存世界"}
            </button>
          ) : null}
        </div>
      }
    >
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
    </ScreenLayout>
  );
}

/* TODO: WorldEditorPage 绉诲姩绔笌妗岄潰绔?UI 鏋舵瀯宸紓杈冨ぇ锛坰ection-list vs tabs锛夛紝
 * 褰撳墠閫氳繃 isMobile 鏉′欢娓叉煋涓ゅ甯冨眬锛屽悗缁彲鑰冭檻杩涗竴姝ラ噸鏋勪负缁熶竴缁勪欢銆?*/
