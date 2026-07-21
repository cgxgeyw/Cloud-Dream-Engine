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
  role: string;
  content: string | ContentPart[];
  speaker?: string | null;
  metadata?: Record<string, unknown> | null;
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
};

export type SessionSnapshotResponse = SessionSnapshot;

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
};

export type ModelConfigResponse = ModelConfig;

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
};

export type McpToolUpsertRequest = McpToolCreateRequest;

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
