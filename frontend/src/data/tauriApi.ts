import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppSettings,
  AssetSelection,
  AiWorldCreateRequest,
  AiWorldCreateResponse,
  AttributeSchemaResponse,
  AttributeSchemaUpsertRequest,
  AttributeValueResponse,
  AttributeValueUpsertRequest,
  CharacterCreateFromTemplateRequest,
  CharacterCreateRequest,
  CharacterResponse,
  CharacterTemplateResponse,
  CharacterUpsertRequest,
  CharacterVisualState,
  ChatMessage,
  ChatMessageResponse,
  ConnectionTestResult,
  EmbeddingModelFileStatus,
  EmbeddingModelStatus,
  ImageModelTestRequest,
  ImageModelTestResult,
  InventoryItem,
  McpToolCreateRequest,
  McpToolExposurePolicy,
  McpToolResponse,
  McpToolUpsertRequest,
  MemoryEntry,
  ModelConfig,
  ModelConfigResponse,
  ModelDiscoverResponse,
  PlayerActionMode,
  PlayerActionRequest,
  PluginResponse,
  RetryFailedLlmStepRequest,
  RuntimeAttributeGroup,
  RuntimeAttributeItem,
  SaveResponse,
  SceneRuntime,
  SessionCreateRequest,
  SessionMapEdge,
  SessionMapNode,
  SessionRuntimeAttributesResponse,
  SessionSnapshot,
  SessionSnapshotResponse,
  SessionState,
  SettingsResponse,
  SettingsUpdateRequest,
  SwitchCharacterProposalRequest,
  SwitchPlayerCharacterRequest,
  UploadResponse,
  VerifyWorldPackageUiCompatibilityRequest,
  WorldCreateRequest,
  WorldOpeningMessage,
  WorldOpeningPromptPreviewResponse,
  WorldResponse,
  WorldUiBundleValidationRequest,
  WorldUiBundleValidationResult,
  WorldUiCompileRequest,
  WorldUiCompileResult,
  WorldUiCompatibilityReport,
  WorldUiDocumentRequest,
  WorldUiDocumentValidationResult,
  WorldUpsertRequest,
  WorldPermissionStatus,
} from "./types";

// 类型定义
export type {
  WorldResponse, WorldOpeningMessage, WorldCreateRequest, WorldUpsertRequest, AiWorldCreateRequest, AiWorldCreateResponse,
  CharacterResponse, CharacterCreateRequest, CharacterUpsertRequest, CharacterTemplateResponse, CharacterCreateFromTemplateRequest,
  ChatMessage, ChatMessageResponse, SessionSnapshot, SessionSnapshotResponse,
  SessionMapNode, SessionMapEdge, InventoryItem, SceneRuntime, AssetSelection, CharacterVisualState, SessionState,
  SessionCreateRequest, PlayerActionMode, PlayerActionRequest, RetryFailedLlmStepRequest,
  SwitchCharacterProposalRequest, SwitchPlayerCharacterRequest, SaveResponse,
  ModelConfig, ModelConfigResponse, ConnectionTestResult, ImageModelTestRequest, ImageModelTestResult, ModelDiscoverResponse,
  EmbeddingModelFileStatus, EmbeddingModelStatus, AppSettings, SettingsResponse, SettingsUpdateRequest,
  PluginResponse, McpToolExposurePolicy, McpToolResponse, McpToolCreateRequest, McpToolUpsertRequest,
  AttributeSchemaResponse, AttributeSchemaUpsertRequest, AttributeValueResponse, AttributeValueUpsertRequest,
  MemoryEntry, RuntimeAttributeItem, RuntimeAttributeGroup, SessionRuntimeAttributesResponse,
  WorldOpeningPromptPreviewResponse, UploadResponse,
  WorldUiDocumentRequest, WorldUiDocumentValidationResult,
  WorldUiBundleValidationRequest, WorldUiBundleValidationResult,
  WorldUiCompileRequest, WorldUiCompileResult,
  VerifyWorldPackageUiCompatibilityRequest, WorldUiCompatibilityReport, WorldPermissionStatus,
} from "./types";









export type CharacterMemoryGroup = {
  character_id: string;
  character_name: string;
  memories: MemoryEntry[];
};

export type SpeakerSelectionPreview = {
  speaker?: string;
  debug_lines: string[];
};

export type PromptCallTrace = {
  turn_index?: number;
  step?: string;
  recipient_type?: string;
  recipient_name?: string;
  created_at?: string;
  stage?: unknown;
  prompt_call?: Record<string, unknown>;
  prompt_result?: Record<string, unknown>;
  tool_loop_messages?: unknown[];
};

export type LlmCallTrace = {
  turn_index: number;
  step: string;
  speaker: string;
  created_at?: string;
  recipient_type?: string;
  stage?: unknown;
  provider?: string;
  model?: string;
  model_id?: string;
  status?: string;
  latency_ms?: number;
  request?: unknown;
  response?: unknown;
  parsed?: unknown;
  written_result?: unknown;
  raw_model_return?: unknown;
  error?: unknown;
  tool_calls?: unknown[];
  tool_results?: unknown[];
  tool_loop_messages?: unknown[];
  input_payload?: unknown;
  output_payload?: unknown;
  raw_input_payload?: unknown;
  raw_output_payload?: unknown;
};

export type TraceTimelineEvent = {
  created_at?: string;
  step?: string;
  event_type?: string;
  domain?: string;
  [key: string]: unknown;
};

export type TraceTimelineTurn = {
  turn_index: number;
  steps: Array<Record<string, unknown>>;
  events: TraceTimelineEvent[];
};

export type SessionDebugResponse = {
  status?: string;
  session: SessionSnapshot;
  runtime_session_attributes: RuntimeAttributeItem[];
  runtime_character_attributes: RuntimeAttributeGroup[];
  speaker_selection_preview: SpeakerSelectionPreview;
  memory_groups: CharacterMemoryGroup[];
  agent_sessions?: Array<Record<string, unknown>>;
  latest_checkpoints?: Array<Record<string, unknown>>;
  turn_journal?: Array<Record<string, unknown>>;
  recovery_state?: Record<string, unknown>;
  director_prompt_traces: Array<Record<string, unknown>>;
  director_tool_loops?: Array<Record<string, unknown>>;
  system_prompt_coverage?: Record<string, unknown>;
  character_prompt_traces: Array<Record<string, unknown>>;
  llm_calls: LlmCallTrace[];
  prompt_calls: PromptCallTrace[];
  turn_journal_timeline?: Array<Record<string, unknown>>;
  trace_timeline?: TraceTimelineTurn[];
  latest_turn_trace?: Record<string, unknown> | null;
  latest_writeback_events?: Array<Record<string, unknown>>;
  latest_tool_loop_summary?: Array<Record<string, unknown>>;
  memory_commit_trace?: Record<string, unknown>;
  latest_runtime_decision?: Record<string, unknown>;
  state_writeback_trace?: Record<string, unknown>;
  tool_chain?: Array<Record<string, unknown>>;
  event_chain: string[];
  available_modules: string[];
};

export type RuleDefinition = {
  id: string;
  scope: string;
  name: string;
  enabled: boolean;
  priority: number;
  description: string;
  condition: Record<string, unknown>;
  effects: Record<string, unknown>[];
};

export type RuleCreateRequest = Omit<RuleDefinition, "id">;

type BinaryFilePayload = {
  filename: string;
  bytes: number[];
};

type SavedFilePayload = {
  filename: string;
  saved_path: string;
};

let assetBaseDir = "";

// 通用 Tauri 命令调用封装
async function tauriCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(command, args);
}

export async function initTauriRuntime(): Promise<void> {
  assetBaseDir = await tauriCommand<string>("get_asset_base_dir");
}

// API 函数
export function fetchWorlds(): Promise<WorldResponse[]> {
  return tauriCommand("list_worlds");
}

export function fetchWorld(worldId: string): Promise<WorldResponse> {
  return tauriCommand("get_world", { id: worldId });
}

export function createWorld(payload: WorldCreateRequest): Promise<WorldResponse> {
  return tauriCommand("create_world", { request: payload });
}

export function createWorldWithAi(payload: AiWorldCreateRequest): Promise<AiWorldCreateResponse> {
  return tauriCommand("create_world_with_ai", { request: payload });
}

export function onAiWorldCreateProgress(callback: (receivedChars: number) => void): () => void {
  const unlistenPromise = listen<{ received_chars: number }>("ai_world_create:progress", (event) => {
    callback(event.payload?.received_chars ?? 0);
  });
  let resolvedUnlisten: (() => void) | null = null;
  unlistenPromise.then((fn) => { resolvedUnlisten = fn; });
  let cancelled = false;
  return () => {
    if (cancelled) return;
    cancelled = true;
    if (resolvedUnlisten) {
      resolvedUnlisten();
    } else {
      unlistenPromise.then((fn) => fn());
    }
  };
}

export function updateWorld(worldId: string, payload: WorldCreateRequest): Promise<WorldResponse> {
  return tauriCommand("update_world", { id: worldId, request: payload });
}

export function deleteWorld(worldId: string): Promise<void> {
  return tauriCommand("delete_world", { id: worldId });
}

export function deleteAllWorlds(): Promise<{ ok: boolean; deleted_count: number }> {
  return tauriCommand("delete_all_worlds");
}

export function duplicateWorld(worldId: string): Promise<WorldResponse> {
  return tauriCommand("duplicate_world", { id: worldId });
}

export function fetchWorldCharacters(worldId: string): Promise<CharacterResponse[]> {
  return tauriCommand("list_world_characters", { worldId });
}

export function fetchAllCharacters(): Promise<CharacterResponse[]> {
  return tauriCommand("list_all_characters");
}

export function fetchCharacter(characterId: string): Promise<CharacterResponse> {
  return tauriCommand("get_character", { id: characterId });
}

export function createWorldCharacter(worldId: string, payload: CharacterCreateRequest): Promise<CharacterResponse> {
  return tauriCommand("create_world_character", { worldId, request: payload });
}

export function updateWorldCharacter(worldId: string, characterId: string, payload: CharacterCreateRequest): Promise<CharacterResponse> {
  return tauriCommand("update_world_character", { worldId, id: characterId, request: payload });
}

export function deleteWorldCharacter(worldId: string, characterId: string): Promise<void> {
  return tauriCommand("delete_world_character", { worldId, id: characterId });
}

export function exportWorldCharacterTemplate(worldId: string, characterId: string): Promise<CharacterTemplateResponse> {
  return tauriCommand("export_character_template", { worldId, characterId });
}

export function createCharacterInWorldFromCharacter(worldId: string, characterId: string, payload: CharacterCreateFromTemplateRequest): Promise<CharacterResponse> {
  return tauriCommand("create_character_in_world", { worldId, characterId, request: payload });
}

export function fetchWorldOpeningPromptPreview(worldId: string, params?: { playerCharacterId?: string | null; playerInput?: string }): Promise<WorldOpeningPromptPreviewResponse> {
  return tauriCommand("preview_opening_prompt", { worldId, params });
}

export function validateWorldUiDocument(
  payload: WorldUiDocumentRequest,
): Promise<WorldUiDocumentValidationResult> {
  return tauriCommand("validate_world_ui_document", { request: payload });
}

export function validateWorldUiBundle(
  payload: WorldUiBundleValidationRequest,
): Promise<WorldUiBundleValidationResult> {
  return tauriCommand("validate_world_ui_bundle", { request: payload });
}

export function compileWorldUiDocument(
  payload: WorldUiCompileRequest,
): Promise<WorldUiCompileResult> {
  return tauriCommand("compile_world_ui_document", { request: payload });
}

export function verifyWorldPackageUiCompatibility(
  payload: VerifyWorldPackageUiCompatibilityRequest,
): Promise<WorldUiCompatibilityReport> {
  return tauriCommand("verify_world_package_ui_compatibility", { request: payload });
}

export async function downloadWorldPackage(worldId: string): Promise<{ blob?: Blob; filename: string; savedPath?: string }> {
  const payload = await tauriCommand<SavedFilePayload>("export_world_package_to_downloads", { worldId });
  return {
    filename: payload.filename,
    savedPath: payload.saved_path,
  };
}

export async function importWorldPackage(file: File): Promise<WorldResponse> {
  const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
  return tauriCommand("import_world_package", { filename: file.name, data: bytes });
}

export function importWorldPackageFromPath(path: string): Promise<WorldResponse> {
  return tauriCommand("import_world_package_from_path", { path });
}

export async function uploadFile(file: File): Promise<UploadResponse> {
  const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
  return tauriCommand("upload_file", { filename: file.name, data: bytes });
}

export function fetchSession(sessionId: string): Promise<SessionSnapshot> {
  return tauriCommand("get_session", { id: sessionId });
}

export function createSession(payload: SessionCreateRequest): Promise<SessionSnapshot> {
  return tauriCommand("create_session", { request: payload });
}

export function submitPlayerAction(sessionId: string, payload: PlayerActionRequest): Promise<SessionSnapshot> {
  return tauriCommand("submit_player_action", { sessionId, request: payload });
}

export async function streamPlayerAction(
  sessionId: string,
  payload: PlayerActionRequest,
  handlers: { onSnapshot?: (snapshot: SessionSnapshot) => void; onError?: (detail: string) => void } = {},
): Promise<SessionSnapshot | null> {
  const unlisten = await waitForSessionSnapshotListener(sessionId, (snapshot) => {
    handlers.onSnapshot?.(snapshot);
  });

  try {
    const result = await submitPlayerAction(sessionId, payload);
    return result;
  } catch (e) {
    handlers.onError?.(String(e));
    throw e;
  } finally {
    unlisten();
  }
}

export function retryFailedLlmStep(
  sessionId: string,
  payload: RetryFailedLlmStepRequest,
): Promise<SessionSnapshot> {
  return tauriCommand("retry_failed_llm_step", { sessionId, request: payload });
}

export function switchPlayerCharacter(sessionId: string, payload: SwitchPlayerCharacterRequest): Promise<SessionSnapshot> {
  return tauriCommand("switch_player_character", { sessionId, request: payload });
}

export function fetchSaves(): Promise<SaveResponse[]> {
  return tauriCommand("list_saves");
}

export function branchSave(saveId: string): Promise<SaveResponse> {
  return tauriCommand("branch_save", { id: saveId });
}

export function deleteSave(saveId: string): Promise<void> {
  return tauriCommand("delete_save", { id: saveId });
}

export function deleteAllSaves(): Promise<{ ok: boolean; deleted_count: number }> {
  return tauriCommand("delete_all_saves");
}

export function fetchModels(modelType?: string): Promise<ModelConfig[]> {
  return tauriCommand("list_models", { modelType });
}

export function fetchModel(modelId: string): Promise<ModelConfig> {
  return tauriCommand("get_model", { id: modelId });
}

export function createModel(payload: Omit<ModelConfig, "id">): Promise<ModelConfig> {
  return tauriCommand("create_model", { request: payload });
}

export function updateModel(modelId: string, payload: Partial<Omit<ModelConfig, "id">>): Promise<ModelConfig> {
  return tauriCommand("update_model", { id: modelId, request: payload });
}

export function deleteModel(modelId: string): Promise<void> {
  return tauriCommand("delete_model", { id: modelId });
}

export function setDefaultModel(modelId: string): Promise<void> {
  return tauriCommand("set_default_model", { id: modelId });
}

export function testModel(modelId: string): Promise<ConnectionTestResult> {
  return tauriCommand("test_model", { id: modelId });
}

export function testImageModel(modelId: string, payload: ImageModelTestRequest): Promise<ImageModelTestResult> {
  return tauriCommand("test_image_model", { id: modelId, request: payload });
}

export function discoverModels(provider: string, baseUrl: string, apiKey: string): Promise<ModelDiscoverResponse> {
  return tauriCommand("discover_models", { provider, baseUrl, apiKey });
}

export function fetchBuiltinEmbeddingModelStatus(modelId?: string): Promise<EmbeddingModelStatus> {
  return tauriCommand("get_builtin_embedding_model_status", { modelId });
}

export function downloadBuiltinEmbeddingModel(modelId?: string): Promise<EmbeddingModelStatus> {
  return tauriCommand("download_builtin_embedding_model", { modelId });
}

export function fetchSettings(): Promise<AppSettings> {
  return tauriCommand("get_settings");
}

export function updateSettings(payload: AppSettings): Promise<AppSettings> {
  return tauriCommand("update_settings", { request: payload });
}

export function getExportDirectorySuggestion(): Promise<string> {
  return tauriCommand("get_export_directory_suggestion");
}

export function fetchPlugins(): Promise<PluginResponse[]> {
  return tauriCommand("list_plugins");
}

export function fetchMcpTools(): Promise<McpToolResponse[]> {
  return tauriCommand("list_mcp_tools");
}

export function createMcpTool(payload: McpToolCreateRequest): Promise<McpToolResponse> {
  return tauriCommand("create_mcp_tool", { request: payload });
}

export function updateMcpTool(toolId: string, payload: McpToolCreateRequest): Promise<McpToolResponse> {
  return tauriCommand("update_mcp_tool", { id: toolId, request: payload });
}

export function deleteMcpTool(toolId: string): Promise<void> {
  return tauriCommand("delete_mcp_tool", { id: toolId });
}

export function fetchAttributeSchemas(scope?: string): Promise<AttributeSchemaResponse[]> {
  return tauriCommand("list_attribute_schemas", { scope });
}

export function createAttributeSchema(payload: AttributeSchemaUpsertRequest): Promise<AttributeSchemaResponse> {
  return tauriCommand("create_attribute_schema", { request: payload });
}

export function updateAttributeSchema(schemaId: string, payload: AttributeSchemaUpsertRequest): Promise<AttributeSchemaResponse> {
  return tauriCommand("update_attribute_schema", { id: schemaId, request: payload });
}

export function deleteAttributeSchema(schemaId: string): Promise<void> {
  return tauriCommand("delete_attribute_schema", { id: schemaId });
}

export function fetchAttributeValues(params: { ownerType?: string; ownerId?: string; schemaId?: string }): Promise<AttributeValueResponse[]> {
  return tauriCommand("list_attribute_values", params);
}

export function upsertAttributeValue(payload: AttributeValueUpsertRequest): Promise<AttributeValueResponse> {
  return tauriCommand("upsert_attribute_value", { request: payload });
}

export function requestWorldPermissions(permissions: string[], wait = false): Promise<WorldPermissionStatus[]> {
  return tauriCommand("request_world_permissions", { request: { permissions, wait } });
}

export function fetchMemories(worldId?: string, sessionId?: string, characterId?: string, layer?: string, limit?: number): Promise<MemoryEntry[]> {
  return tauriCommand("list_memories", { worldId, sessionId, characterId, layer, limit });
}

export function fetchSessionDebug(sessionId: string): Promise<SessionDebugResponse> {
  return tauriCommand("get_session_debug", { sessionId });
}

export function fetchSessionRuntimeAttributes(
  sessionId: string,
): Promise<SessionRuntimeAttributesResponse> {
  return tauriCommand("get_session_runtime_attributes", { sessionId });
}

export function resumeLastIncompleteTurn(sessionId: string): Promise<SessionSnapshot> {
  return tauriCommand("resume_last_incomplete_turn", { sessionId });
}

export function importCharacterTemplate(worldId: string, payload: CharacterTemplateResponse): Promise<CharacterResponse> {
  return tauriCommand("import_character_template", { worldId, request: payload });
}

export function deleteAsset(filename: string): Promise<void> {
  return tauriCommand("delete_asset", { filename });
}

export function fetchRules(scope?: string): Promise<RuleDefinition[]> {
  return tauriCommand("list_rules", { scope });
}

export function fetchRule(ruleId: string): Promise<RuleDefinition> {
  return tauriCommand("get_rule", { id: ruleId });
}

export function createRule(payload: RuleCreateRequest): Promise<RuleDefinition> {
  return tauriCommand("create_rule", { request: payload });
}

export function updateRule(ruleId: string, payload: RuleCreateRequest): Promise<RuleDefinition> {
  return tauriCommand("update_rule", { id: ruleId, request: payload });
}

export function deleteRule(ruleId: string): Promise<void> {
  return tauriCommand("delete_rule", { id: ruleId });
}

export function onSessionSnapshot(sessionId: string, callback: (snapshot: SessionSnapshot) => void): () => void {
  const unlistenPromise = listen<SessionSnapshot>(`session:${sessionId}:snapshot`, (event) => {
    callback(event.payload);
  });

  let resolvedUnlisten: (() => void) | null = null;
  unlistenPromise.then(fn => { resolvedUnlisten = fn; });

  let cancelled = false;
  return () => {
    if (cancelled) return;
    cancelled = true;
    if (resolvedUnlisten) {
      resolvedUnlisten();
    } else {
      unlistenPromise.then(fn => fn());
    }
  };
}

async function waitForSessionSnapshotListener(
  sessionId: string,
  callback: (snapshot: SessionSnapshot) => void,
): Promise<UnlistenFn> {
  return listen<SessionSnapshot>(`session:${sessionId}:snapshot`, (event) => {
    callback(event.payload);
  });
}

export function toSessionWebSocketUrl(sessionId: string): string {
  throw new Error("WebSocket URL is unavailable in Tauri mode; use Tauri events instead.");
}

export function assetUrl(path: string | null | undefined): string {
  const normalized = normalizeAssetReference(path);
  if (!normalized) return "";
  if (normalized.startsWith("http://") || normalized.startsWith("https://")) return normalized;
  if (normalized.startsWith("data:")) return normalized;
  if (!assetBaseDir) return normalized;
  const relative = normalized.replace(/^\/?assets\//, "");
  const joined = [assetBaseDir.replace(/[\\\/]+$/, ""), relative]
    .filter(Boolean)
    .join("/");
  return convertFileSrc(joined);
}

function normalizeAssetReference(path: string | null | undefined): string {
  if (!path) return "";
  const trimmed = path.trim();
  if (!trimmed || trimmed === "static") return "";
  if (trimmed.startsWith("http://") || trimmed.startsWith("https://") || trimmed.startsWith("data:")) {
    return trimmed;
  }
  const unix = trimmed.split("\\").join("/");
  if (unix.startsWith("/assets/") || unix.startsWith("assets/")) {
    return unix;
  }
  return `assets/${unix.replace(/^\/+/, "")}`;
}
