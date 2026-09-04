// 共享类型定义 — api.ts 和 tauriApi.ts 的共同类型
// 从 tauriApi.ts 提取为 canonical 定义，消除双份维护

// ==============================
// World
// ==============================

export type WorldMapTopologyNode = {
  id?: string;
  label: string;
  children?: WorldMapTopologyNode[];
};

export type WorldMapTopology = {
  version?: number;
  root?: WorldMapTopologyNode;
  tree?: WorldMapTopologyNode;
  nodes?: WorldMapTopologyNode[];
  edges?: Array<{ source?: string; target?: string; from?: string; to?: string }>;
};

export type WorldResponse = {
  id: string;
  name: string;
  genre: string;
  background_prompt: string;
  opening_scene: string;
  summary: string;
  time_system: string;
  map_nodes: WorldMapTopology;
  triggers: string[];
  time_config: Record<string, unknown>;
  director_config: Record<string, unknown>;
  ui_theme_config: Record<string, unknown>;
  director_system_prompt_base: string;
  director_runtime_system_prompt: string;
  opening_messages: WorldOpeningMessage[];
  opening_character_ids: string[];
  player_character_id?: string | null;
};

export type WorldOpeningMessage = {
  role: string;
  content: string;
  speaker?: string | null;
};

export type WorldUpsertRequest = {
  name: string;
  genre: string;
  background_prompt: string;
  opening_scene: string;
  summary: string;
  time_system: string;
  map_nodes: WorldMapTopology;
  triggers: string[];
  time_config: Record<string, unknown>;
  director_config: Record<string, unknown>;
  ui_theme_config: Record<string, unknown>;
  opening_messages: WorldOpeningMessage[];
  opening_character_ids: string[];
  player_character_id?: string | null;
};

export type WorldCreateRequest = WorldUpsertRequest;

export type WorldRecord = {
  id: string;
  world_id: string;
  collection: string;
  data: Record<string, unknown>;
  created_at: string;
  updated_at: string;
};

export type WorldRecordWriteRequest = {
  collection: string;
  data: Record<string, unknown>;
};

export type WorldKvEntry = {
  owner_type: string;
  owner_id: string;
  namespace: string;
  key: string;
  value: unknown;
  updated_at: string;
};

/** 消息中的交互（第 5 项）：选项/多选/表单/确认/滑条 */
export type MessageInteraction = {
  interaction_id: string;
  kind: "choice" | "multi_choice" | "form" | "confirm" | "slider";
  prompt: string;
  config: Record<string, unknown>;
  status: "pending" | "answered" | string;
  answer?: unknown;
  answered_at?: string;
};

export type AnswerInteractionResponse = {
  session: SessionSnapshot;
  answer: unknown;
  newly_answered: boolean;
};

/** 变量作用域：world（跨存档共享）/ session（世界存档）/ character（存档内角色） */
export type KvScope = {
  scope?: "world" | "session" | "character";
  session_id?: string;
  character_id?: string;
};

export type AiWorldCreateMode = "single_agent" | "multi_agent";

export type AiWorldCreateRequest = {
  mode: AiWorldCreateMode;
  concept: string;
};

export type AiWorldCreateResponse = {
  world: WorldResponse;
  characters: CharacterResponse[];
  notes: string[];
};

export type WorldOpeningPromptPreviewResponse = {
  opening_calls_llm: boolean;
  opening_messages: ChatMessage[];
  sample_player_input: string;
  planned_speakers: string[];
  world_director_prompt_trace: Record<string, unknown>;
  character_prompt_traces: Array<{
    speaker?: string | null;
    prompt_trace: Record<string, unknown>;
  }>;
  notes: string[];
};

export type WorldUiDocumentRequest = {
  source: string;
  platform?: string | null;
};

export type WorldUiBundleValidationRequest = {
  desktop_file: string;
  mobile_file: string;
  runtime_version?: number | null;
  desktop_stylesheet?: string;
  mobile_stylesheet?: string;
  capabilities?: string[];
  storage?: Record<string, unknown>;
  logic?: Record<string, unknown>;
};

export type WorldUiCompileRequest = {
  source: string;
  platform?: string | null;
};

export type WorldUiCompatibilityTarget = {
  name: string;
  supported_schema_versions: number[];
  supported_components: string[];
  supported_actions: string[];
  supported_capabilities: string[];
};

export type VerifyWorldPackageUiCompatibilityRequest = {
  desktop_file: string;
  mobile_file: string;
  target?: WorldUiCompatibilityTarget | null;
};

export type WorldUiDiagnostic = {
  severity: string;
  code: string;
  message: string;
  path?: string | null;
};

export type WorldUiDocumentValidationResult = {
  ok: boolean;
  platform?: string | null;
  schema_version?: number | null;
  components: string[];
  actions: string[];
  capabilities: string[];
  errors: WorldUiDiagnostic[];
  warnings: WorldUiDiagnostic[];
  normalized_document?: unknown | null;
};

export type WorldUiBundleValidationResult = {
  ok: boolean;
  desktop: WorldUiDocumentValidationResult;
  mobile: WorldUiDocumentValidationResult;
  errors: WorldUiDiagnostic[];
  warnings: WorldUiDiagnostic[];
};

export type WorldUiCompileResult = {
  ok: boolean;
  platform?: string | null;
  schema_version?: number | null;
  normalized_ast?: unknown | null;
  component_dependencies: string[];
  action_dependencies: string[];
  capability_requirements: string[];
  diagnostics: WorldUiDiagnostic[];
};

export type WorldUiCompatibilityDocumentReport = {
  platform: string;
  ok: boolean;
  schema_version?: number | null;
  component_dependencies: string[];
  action_dependencies: string[];
  capability_requirements: string[];
  unsupported_schema_versions: number[];
  unsupported_components: string[];
  unsupported_actions: string[];
  unsupported_capabilities: string[];
  diagnostics: WorldUiDiagnostic[];
};

export type WorldUiCompatibilityReport = {
  ok: boolean;
  target: WorldUiCompatibilityTarget;
  documents: WorldUiCompatibilityDocumentReport[];
  diagnostics: WorldUiDiagnostic[];
};

// ==============================
// Character
// ==============================

export type CharacterResponse = {
  id: string;
  name: string;
  world_id: string;
  role: string;
  background_prompt: string;
  model: string;
  memory_strategy: string;
  recent_dialogue_rounds: number;
  attributes: string[];
  portrait_assets: string[];
  avatar_asset: string;
  system_prompt_template: string;
  response_contract_prompt: string;
  narration_prompt: string;
  runtime_system_prompt: string;
};

export type CharacterCreateRequest = {
  name: string;
  role: string;
  background_prompt: string;
  model: string;
  memory_strategy: string;
  recent_dialogue_rounds: number;
  attributes: string[];
  portrait_assets: string[];
  avatar_asset: string;
  system_prompt_template: string;
  response_contract_prompt: string;
  narration_prompt: string;
  runtime_system_prompt: string;
};

export type CharacterUpsertRequest = CharacterCreateRequest;

export type CharacterTemplateResponse = {
  name: string;
  role: string;
  background_prompt: string;
  model: string;
  memory_strategy: string;
  recent_dialogue_rounds: number;
  attributes: string[];
  portrait_assets: string[];
  avatar_asset: string;
  system_prompt_template: string;
  response_contract_prompt: string;
  narration_prompt: string;
  runtime_system_prompt: string;
};

export type CharacterCreateFromTemplateRequest = {
  target_world_id: string;
  name: string;
};

// ==============================
// Session
// ==============================

export type ImageContentPart = {
  type: "image_url";
  image_url: { url: string };
};

export type AudioContentPart = {
  type: "input_audio";
  input_audio: { data: string; format: string; duration_secs?: number };
};

export type TextContentPart = {
  type: "text";
  text: string;
};

export type ContentPart = TextContentPart | ImageContentPart | AudioContentPart;

export type ChatMessage = {
  /** 稳定消息 ID（后端写入时生成；旧数据反序列化时补发） */
  message_id?: string;
  role: string;
  content: string | ContentPart[];
  speaker?: string | null;
  metadata?: Record<string, unknown> | null;
  /** RFC3339 写入时间；旧数据可能为空串 */
  created_at?: string;
  /** 分支/重生成场景下来源消息的 ID */
  parent_message_id?: string | null;
};

export type ChatMessageResponse = ChatMessage;

export type SessionMapNode = {
  node_id: string;
  label: string;
  discovered: boolean;
  current: boolean;
};

export type SessionMapEdge = {
  edge_id: string;
  source_node_id: string;
  target_node_id: string;
};

export type InventoryItem = {
  item_id: string;
  name: string;
  category: string;
  quantity: number;
  description: string;
  tags: string[];
  owner_type: string;
  owner_id: string;
  visibility: string;
  disclosed_to: string[];
};

export type SceneRuntime = {
  scene_id: string;
  name: string;
  background_hint: string;
  temporary_tags: string[];
  present_characters: string[];
};

export type CharacterVisualState = {
  character_name: string;
  portrait_hint: string;
  portrait_asset_path?: string | null;
  generation_prompt?: string;
};

export type AssetSelection = {
  background_hint: string;
  active_speaker_portrait: string;
  background_asset_path?: string | null;
  active_speaker_portrait_path?: string | null;
  background_generation_prompt: string;
  active_speaker_generation_prompt: string;
  visible_character_portraits: CharacterVisualState[];
};

export type SessionState = {
  metrics: Record<string, number>;
  tags: string[];
  phase: string;
};

export type SessionSnapshot = {
  id: string;
  world_name: string;
  location: string;
  time_label: string;
  current_speaker: string;
  current_line: string;
  player_character_id: string;
  player_character_name: string;
  visible_characters: string[];
  messages: ChatMessage[];
  player_stats: string[];
  map_graph_nodes: SessionMapNode[];
  map_graph_edges: SessionMapEdge[];
  inventory_items: InventoryItem[];
  system_log: string[];
  scene: SceneRuntime;
  assets: AssetSelection;
  state: SessionState;
  /** 本存档的生成参数覆盖（三级覆盖里优先级最高的一层）。空对象表示不覆盖。 */
  generation_params?: GenerationParams;
};

export type SessionSnapshotResponse = SessionSnapshot;

// ==============================
// 生成参数（采样参数）
// ==============================

/**
 * 生成参数。每个字段可缺省，缺省表示「这一层不覆盖」，交给下一层或内置默认决定。
 * 覆盖顺序：内置默认（导演 0.7 / 角色 0.8）→ 应用设置 → 世界 → 存档。
 */
export type GenerationParams = {
  temperature?: number;
  top_p?: number;
  top_k?: number;
  max_tokens?: number;
  stop?: string[];
  presence_penalty?: number;
  frequency_penalty?: number;
  seed?: number;
};

/** 某个生成参数被 provider 过滤掉的记录。 */
export type DroppedGenerationParam = {
  name: string;
  reason: string;
};

/** 本存档的三级生成参数与最终生效值。 */
export type SessionGenerationParamsResponse = {
  app: GenerationParams;
  world: GenerationParams;
  session: GenerationParams;
  effective_director: GenerationParams;
  effective_character: GenerationParams;
};

export type SessionCreateRequest = {
  world_id: string;
  player_character_id?: string | null;
};

// ==============================
// Player Action
// ==============================

export type PlayerActionMode = "submit" | "resend" | "edit";

export type PlayerActionRequest = {
  content: string | ContentPart[];
  action_mode: PlayerActionMode;
  resend_from_turn_index?: number;
};

export type RetryFailedLlmStepRequest = {
  retry_token: string;
};

export type SwitchCharacterProposalRequest = {
  target_character_name?: string | null;
  reason?: string | null;
  location?: string | null;
  scene_name?: string | null;
  scene_background_hint?: string | null;
  scene_tags: string[];
  visible_characters: string[];
};

export type SwitchPlayerCharacterRequest = {
  player_character_id: string;
  proposal?: SwitchCharacterProposalRequest | null;
};

// ==============================
// Save
// ==============================

export type SaveResponse = {
  id: string;
  session_id: string;
  title: string;
  world_name: string;
  updated_at: string;
  progress: string;
  summary: string;
  player_character_name?: string | null;
  parent_save_id?: string | null;
  branch_root_save_id?: string | null;
  branch_label?: string | null;
  turn_index: number;
};

// ==============================
// Model
// ==============================

export type ModelConfig = {
  id: string;
  name: string;
  model_type: string;
  provider: string;
  model_id: string;
  base_url: string;
  api_key: string;
  max_tokens: number;
  streaming_enabled: boolean;
  is_default: boolean;
  /** 声明支持的输入模态（"image" / "audio"），空 = 仅文本。 */
  input_modalities: string[];
};

export type ModelConfigResponse = ModelConfig;

/** 第 12 项：世界包平台能力的声明与授权状态（设置页「世界权限」区）。 */
export type WorldFeatureGrantStatus = {
  feature: string;
  declared: boolean;
  granted: boolean;
};

export type ConnectionTestResult = {
  ok: boolean;
  detail: string;
  debug_lines: string[];
};

export type ImageModelTestRequest = {
  prompt: string;
};

export type ImageModelTestResult = {
  ok: boolean;
  detail: string;
  debug_lines: string[];
  asset_path?: string | null;
  image_url?: string | null;
  seed?: number | null;
};

export type ModelDiscoverResponse = {
  ok: boolean;
  detail: string;
  model_ids: string[];
  debug_lines: string[];
};

export type EmbeddingModelFileStatus = {
  name: string;
  relative_path: string;
  exists: boolean;
  size_bytes: number;
};

export type EmbeddingModelStatus = {
  model_id: string;
  display_name: string;
  installed: boolean;
  detail: string;
  local_dir: string;
  total_size_bytes: number;
  files: EmbeddingModelFileStatus[];
};

// ==============================
// Settings
// ==============================

export type AppSettings = {
  text_model_provider: string;
  default_text_model: string;
  image_model_provider: string;
  default_image_workflow: string;
  embedding_enabled: boolean;
  default_embedding_model: string;
  home_background_strategy: string;
  export_directory: string;
  /** 应用级生成参数（三级覆盖的第一层）。缺省字段表示不覆盖内置默认。 */
  generation_params?: GenerationParams;
};

export type SettingsResponse = AppSettings;

export type SettingsUpdateRequest = AppSettings;

// ==============================
// Plugin / MCP
// ==============================

export type PluginResponse = {
  id: string;
  name: string;
  enabled: boolean;
  description: string;
  hooks: string[];
};

export type McpToolExposurePolicy = string | { mode?: string; [key: string]: unknown };

/** MCP server 传输方式。stdio 仅桌面端可用，http 全平台可用。 */
export type McpTransport = "stdio" | "http";

export type McpServerConfig = {
  id: string;
  name: string;
  transport: McpTransport;
  command: string;
  args: string[];
  env: Record<string, string>;
  url: string;
  headers: Record<string, string>;
  auth_token: string;
  enabled: boolean;
  timeout_ms: number;
  max_result_bytes: number;
};

export type McpServerUpsertRequest = Omit<McpServerConfig, "id">;

export type McpServerProbeResult = {
  ok: boolean;
  error: string | null;
  tools: Array<Record<string, unknown>>;
  platform_supported: boolean;
};

export type McpToolResponse = {
  id: string;
  name: string;
  description: string;
  server_name: string;
  tool_name: string;
  enabled: boolean;
  exposure_policy: McpToolExposurePolicy;
  risk_level: string;
  trigger_keywords: string[];
  input_schema: Record<string, unknown>;
  /** 绑定的 MCP server（mcp_servers.id）；为空表示未绑定，不会下发给模型 */
  server_id: string;
  /** 实现方式："mcp"（默认，走外部 server）| "builtin_http"（本地执行，无需 server） */
  impl_kind: string;
  /** builtin_http 的执行配置（generic / template 等模式） */
  impl_config: Record<string, unknown>;
};

export type McpToolCreateRequest = {
  name: string;
  description: string;
  server_name: string;
  tool_name: string;
  enabled: boolean;
  exposure_policy: McpToolExposurePolicy;
  risk_level: string;
  trigger_keywords: string[];
  input_schema: Record<string, unknown>;
  /** 绑定的 MCP server（mcp_servers.id）；为空表示未绑定，不会下发给模型 */
  server_id: string;
  /** 实现方式："mcp"（默认，走外部 server）| "builtin_http"（本地执行，无需 server） */
  impl_kind: string;
  /** builtin_http 的执行配置（generic / template 等模式） */
  impl_config: Record<string, unknown>;
};

export type McpToolUpsertRequest = McpToolCreateRequest;

/** import_mcp_tools 的导入结果统计。 */
export type McpToolImportSummary = {
  imported: number;
  updated: number;
  skipped: number;
};

// ==============================
// Attribute
// ==============================

export type AttributeScope =
  | "world"
  | "character"
  | "session"
  | "session_character";

export type AttributeValueType =
  | "text"
  | "number"
  | "boolean"
  | "list"
  | "json";

export type AttributeSchemaResponse = {
  id: string;
  scope: AttributeScope;
  key: string;
  label: string;
  value_type: AttributeValueType;
  description: string;
  default_value: unknown;
  enum_options: string[];
  display_policy: Record<string, unknown>;
  access_policy: Record<string, unknown>;
  mutation_policy: Record<string, unknown>;
  influence_policy: Record<string, unknown>;
  projection_policy: Record<string, unknown>;
};

export type AttributeSchemaUpsertRequest = {
  scope: AttributeScope;
  key: string;
  label: string;
  value_type: AttributeValueType;
  description: string;
  default_value: unknown;
  enum_options: string[];
  display_policy: Record<string, unknown>;
  access_policy: Record<string, unknown>;
  mutation_policy: Record<string, unknown>;
  influence_policy: Record<string, unknown>;
  projection_policy: Record<string, unknown>;
};

export type AttributeValueResponse = {
  id: string;
  schema_id: string;
  owner_type: string;
  owner_id: string;
  value: unknown;
  source: string;
};

export type AttributeValueUpsertRequest = {
  schema_id: string;
  owner_type: string;
  owner_id: string;
  value: unknown;
  source: string;
};

export type WorldPermissionStatus = {
  permission: string;
  requested: boolean;
  granted: boolean | null;
  error: string | null;
};

// ==============================
// Memory / Runtime Attributes
// ==============================

export type MemoryEntry = {
  id: string;
  world_id: string;
  session_id: string;
  character_id: string;
  layer: string;
  content: string;
  source: string;
  importance: number;
  created_at: string;
  turn_index: number;
  conversation_id?: string | null;
  event_id?: string | null;
  item_id?: string | null;
  scene_id?: string | null;
  memory_type: string;
  speaker?: string | null;
  role?: string | null;
  location?: string | null;
  participants: string[];
  keywords: string[];
};

export type MemoryEntity = {
  id: string;
  world_id: string;
  session_id: string;
  name: string;
  name_normalized: string;
  entity_type: string;
  aliases: string[];
  mention_count: number;
  first_seen_turn: number;
  last_seen_turn: number;
  created_at: string;
};

export type MemoryRelation = {
  id: string;
  world_id: string;
  session_id: string;
  subject_entity_id: string;
  predicate: string;
  object_entity_id?: string | null;
  object_text: string;
  valid_from_turn: number;
  invalid_at_turn?: number | null;
  source: string;
  confidence: number;
  created_at: string;
};

export type RuntimeAttributeItem = {
  schema_id: string;
  key: string;
  label: string;
  value_type: AttributeValueType;
  value: unknown;
  source: string;
  display_policy: Record<string, unknown>;
  influence_policy: Record<string, unknown>;
};

export type RuntimeAttributeGroup = {
  owner_type: string;
  owner_id: string;
  owner_label: string;
  items: RuntimeAttributeItem[];
};

export type SessionRuntimeAttributesResponse = {
  session_attributes: RuntimeAttributeGroup[];
  character_attributes: RuntimeAttributeGroup[];
};

// ==============================
// Upload
// ==============================

export type UploadResponse = {
  filename: string;
  relative_path?: string;
  asset_path?: string;
  url: string;
};
