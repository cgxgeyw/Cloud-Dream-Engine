/**
 * Tauri IPC transport.
 *
 * Application code must import from `./apiAdapter` only — this module is a
 * transport implementation detail paired with `api.ts` (HTTP).
 */
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppSettings,
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
  ConnectionTestResult,
  EmbeddingModelStatus,
  GenerationParams,
  ImageModelTestRequest,
  ImageModelTestResult,
  McpToolCreateRequest,
  McpToolImportSummary,
  McpToolResponse,
  MemoryEntry,
  MemoryEntity,
  MemoryRelation,
  MemorySummaryPromptDefaults,
  OagActor,
  OagQueryRequest,
  OagQueryResponse,
  ModelConfig,
  ModelDiscoverResponse,
  PlayerActionRequest,
  PluginResponse,
  RetryFailedLlmStepRequest,
  RuntimeAttributeGroup,
  RuntimeAttributeItem,
  SaveResponse,
  SessionCreateRequest,
  SessionGenerationParamsResponse,
  SessionRuntimeAttributesResponse,
  SessionSnapshot,
  SwitchPlayerCharacterRequest,
  UploadResponse,
  StickerItem,
  StickerPack,
  VerifyWorldPackageUiCompatibilityRequest,
  WorldCreateRequest,
  WorldOpeningPromptPreviewResponse,
  WorldResponse,
  WorldRecord,
  WorldRecordWriteRequest,
  WorldKvEntry,
  WorldUiBundleValidationRequest,
  WorldUiBundleValidationResult,
  WorldUiCompileRequest,
  WorldUiCompileResult,
  WorldUiCompatibilityReport,
  WorldUiDocumentRequest,
  WorldUiDocumentValidationResult,
  WorldPackageInspectionResult,
  WorldEventResponse,
  WorldPermissionStatus,
  WorldFeatureGrantStatus,
  KvScope,
  AnswerInteractionResponse,
  McpServerConfig, McpServerUpsertRequest, McpServerProbeResult,
} from "./types";

// 类型定义
export type {
  WorldResponse, WorldOpeningMessage, WorldCreateRequest, WorldUpsertRequest, AiWorldCreateRequest, AiWorldCreateResponse,
  CharacterResponse, CharacterCreateRequest, CharacterUpsertRequest, CharacterTemplateResponse, CharacterCreateFromTemplateRequest,
  ChatMessage, ChatMessageResponse, SessionSnapshot, SessionSnapshotResponse,
  SessionMapNode, SessionMapEdge, SceneRuntime, AssetSelection, CharacterVisualState, SessionState,
  SessionCreateRequest, PlayerActionMode, PlayerActionRequest, RetryFailedLlmStepRequest,
  SwitchCharacterProposalRequest, SwitchPlayerCharacterRequest, SaveResponse,
  ModelConfig, ModelConfigResponse, ConnectionTestResult, ImageModelTestRequest, ImageModelTestResult, ModelDiscoverResponse,
  EmbeddingModelFileStatus, EmbeddingModelStatus, AppSettings, SettingsResponse, SettingsUpdateRequest,
  PluginResponse, McpToolExposurePolicy, McpToolResponse, McpToolCreateRequest, McpToolUpsertRequest, McpToolImportSummary,
  AttributeSchemaResponse, AttributeSchemaUpsertRequest, AttributeValueResponse, AttributeValueUpsertRequest,
  MemoryEntry, RuntimeAttributeItem, RuntimeAttributeGroup, SessionRuntimeAttributesResponse,
  WorldOpeningPromptPreviewResponse, UploadResponse,
  WorldUiDocumentRequest, WorldUiDocumentValidationResult,
  WorldUiBundleValidationRequest, WorldUiBundleValidationResult,
  WorldUiCompileRequest, WorldUiCompileResult,
  VerifyWorldPackageUiCompatibilityRequest, WorldUiCompatibilityReport, WorldPermissionStatus,
  WorldRecord, WorldRecordWriteRequest, WorldKvEntry,
  WorldPackageInspectionResult,
  WorldEventResponse,
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

export function listWorldRecords(worldId: string, collection: string): Promise<WorldRecord[]> {
  return tauriCommand("list_world_records", { worldId, collection });
}

export function createWorldRecord(
  worldId: string,
  payload: WorldRecordWriteRequest,
): Promise<WorldRecord> {
  return tauriCommand("create_world_record", { worldId, request: payload });
}

export function updateWorldRecord(
  worldId: string,
  recordId: string,
  payload: WorldRecordWriteRequest,
): Promise<WorldRecord> {
  return tauriCommand("update_world_record", { worldId, id: recordId, request: payload });
}

export function deleteWorldRecord(
  worldId: string,
  collection: string,
  recordId: string,
): Promise<void> {
  return tauriCommand("delete_world_record", { worldId, collection, id: recordId });
}

export function listWorldKv(worldId: string, namespace: string, scope?: KvScope): Promise<WorldKvEntry[]> {
  return tauriCommand("list_world_kv", { worldId, namespace, scope });
}

export function getWorldKv(
  worldId: string,
  namespace: string,
  key: string,
  scope?: KvScope,
): Promise<WorldKvEntry | null> {
  return tauriCommand("get_world_kv", { worldId, namespace, key, scope });
}

export function setWorldKv(
  worldId: string,
  namespace: string,
  key: string,
  value: unknown,
  scope?: KvScope,
): Promise<WorldKvEntry> {
  return tauriCommand("set_world_kv", { worldId, namespace, key, value, scope });
}

export function deleteWorldKv(worldId: string, namespace: string, key: string, scope?: KvScope): Promise<void> {
  return tauriCommand("delete_world_kv", { worldId, namespace, key, scope });
}

// ---- 第 12 项：世界包平台能力 ----

export function invokeWorldPlatformFeature(
  worldId: string,
  feature: string,
  params: unknown,
): Promise<unknown> {
  return tauriCommand("invoke_world_platform_feature", {
    request: { world_id: worldId, feature, params },
  });
}

export function listWorldFeatureGrants(worldId: string): Promise<WorldFeatureGrantStatus[]> {
  return tauriCommand("list_world_feature_grants", { worldId });
}

export function setWorldFeatureGrant(worldId: string, feature: string, granted: boolean): Promise<void> {
  return tauriCommand("set_world_feature_grant", { worldId, feature, granted });
}

export function deleteWorld(worldId: string): Promise<void> {
  return tauriCommand("delete_world", { id: worldId });
}

export function answerInteraction(
  sessionId: string,
  messageId: string,
  interactionId: string,
  answer: unknown,
): Promise<AnswerInteractionResponse> {
  return tauriCommand("answer_interaction", {
    request: { session_id: sessionId, message_id: messageId, interaction_id: interactionId, answer },
  });
}

export function fetchSessionGenerationParams(
  sessionId: string,
): Promise<SessionGenerationParamsResponse> {
  return tauriCommand("get_session_generation_params", { sessionId });
}

export function updateSessionGenerationParams(
  sessionId: string,
  params: GenerationParams,
): Promise<SessionGenerationParamsResponse> {
  return tauriCommand("update_session_generation_params", { sessionId, params });
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

export function fetchWorldOpeningPromptPreview(worldId: string, params?: { playerCharacterId?: string | null; playerInput?: string; directorConfig?: Record<string, unknown> }): Promise<WorldOpeningPromptPreviewResponse> {
  return tauriCommand("preview_opening_prompt", { worldId, params });
}

export function fetchDefaultDirectorResponseContract(): Promise<string> {
  return tauriCommand("get_default_director_response_contract");
}

export function fetchDefaultMemorySummaryPrompts(): Promise<MemorySummaryPromptDefaults> {
  return tauriCommand("get_default_memory_summary_prompts");
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

export function dispatchWorldEvent(worldId: string, sessionId: string, event: import("./types").WorldEvent): Promise<WorldEventResponse> {
  return tauriCommand("dispatch_world_event", { worldId, sessionId, event });
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

export async function inspectWorldPackage(file: File): Promise<WorldPackageInspectionResult> {
  const data = Array.from(new Uint8Array(await file.arrayBuffer()));
  return tauriCommand("inspect_world_package", { request: { data } });
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

export function fetchMcpServers() {
  return tauriCommand<McpServerConfig[]>("list_mcp_servers");
}

export function createMcpServer(payload: McpServerUpsertRequest) {
  return tauriCommand<McpServerConfig>("create_mcp_server", { request: payload });
}

export function updateMcpServer(serverId: string, payload: McpServerUpsertRequest) {
  return tauriCommand<McpServerConfig>("update_mcp_server", { id: serverId, request: payload });
}

export function deleteMcpServer(serverId: string) {
  return tauriCommand<void>("delete_mcp_server", { id: serverId });
}

export function probeMcpServer(serverId: string) {
  return tauriCommand<McpServerProbeResult>("probe_mcp_server", { id: serverId });
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

export function exportMcpTools(ids: string[]): Promise<string> {
  return tauriCommand("export_mcp_tools", { ids });
}

export function importMcpTools(json: string): Promise<McpToolImportSummary> {
  return tauriCommand("import_mcp_tools", { json });
}

export function fetchAttributeSchemas(scope?: string, worldId?: string): Promise<AttributeSchemaResponse[]> {
  return tauriCommand("list_attribute_schemas", { scope, worldId });
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

export function fetchMemoryEntities(sessionId: string): Promise<MemoryEntity[]> {
  return tauriCommand("list_memory_entities", { sessionId });
}

export function fetchMemoryRelations(sessionId: string, activeOnly = false): Promise<MemoryRelation[]> {
  return tauriCommand("list_memory_relations", { sessionId, activeOnly });
}

export function fetchSessionDebug(sessionId: string): Promise<SessionDebugResponse> {
  return tauriCommand("get_session_debug", { sessionId });
}

export function queryOagDebug(
  sessionId: string,
  actor: OagActor,
  request: OagQueryRequest,
  asOfTurn?: number,
  maxNodes = 200,
): Promise<OagQueryResponse> {
  return tauriCommand("debug_oag_query", {
    sessionId,
    actor,
    request,
    asOfTurn,
    maxNodes,
  });
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

export function toSessionWebSocketUrl(_sessionId: string): string {
  throw new Error("Tauri 模式下无法使用 WebSocket URL，请改用 Tauri 事件。");
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
export function fetchKnowledgeBases(worldId?: string | null): Promise<import('./types').KnowledgeBase[]> { return tauriCommand('list_knowledge_bases', { worldId: worldId ?? null }); }
export function createKnowledgeBase(name: string, description: string, worldId: string | null): Promise<import('./types').KnowledgeBase> { return tauriCommand('create_knowledge_base', { name, description, worldId }); }
export function deleteKnowledgeBase(id: string): Promise<void> { return tauriCommand('delete_knowledge_base', { id }); }
export function updateKnowledgeChunking(id: string, strategy: string, maxChars: number, overlapChars: number, delimiter: string): Promise<import('./types').KnowledgeBase> { return tauriCommand('update_knowledge_chunking', { id, strategy, maxChars, overlapChars, delimiter }); }
export function rebuildKnowledgeEmbeddings(knowledgeBaseId: string): Promise<import('./types').KnowledgeRebuildStart> { return tauriCommand('rebuild_knowledge_embeddings', { knowledgeBaseId }); }
export function getKnowledgeRebuildStatus(jobId: string): Promise<import('./types').KnowledgeRebuildSnapshot | null> { return tauriCommand('get_knowledge_rebuild_status', { jobId }); }
export function getKnowledgeRebuildForBase(knowledgeBaseId: string): Promise<import('./types').KnowledgeRebuildSnapshot | null> { return tauriCommand('get_knowledge_rebuild_for_base', { knowledgeBaseId }); }
export function listKnowledgeRebuildJobs(): Promise<import('./types').KnowledgeRebuildSnapshot[]> { return tauriCommand('list_knowledge_rebuild_jobs'); }
export function onKnowledgeRebuildProgress(callback: (snapshot: import('./types').KnowledgeRebuildSnapshot) => void): () => void {
  const unlistenPromise = listen<import('./types').KnowledgeRebuildSnapshot>('knowledge-rebuild:progress', (event) => {
    if (event.payload) callback(event.payload);
  });
  let resolvedUnlisten: (() => void) | null = null;
  unlistenPromise.then((fn) => { resolvedUnlisten = fn; });
  let cancelled = false;
  return () => {
    if (cancelled) return;
    cancelled = true;
    if (resolvedUnlisten) resolvedUnlisten();
    else unlistenPromise.then((fn) => fn());
  };
}
export function fetchKnowledgeDocuments(knowledgeBaseId: string): Promise<import('./types').KnowledgeDocument[]> { return tauriCommand('list_knowledge_documents', { knowledgeBaseId }); }
export async function addKnowledgeDocument(knowledgeBaseId: string, file: File, audience: import('./types').KnowledgeDocumentAudience = 'public', audienceCharacterId?: string | null): Promise<import('./types').KnowledgeDocument> { return tauriCommand('add_knowledge_document', { knowledgeBaseId, filename: file.name, data: Array.from(new Uint8Array(await file.arrayBuffer())), audience, audienceCharacterId: audienceCharacterId ?? null }); }
export function deleteKnowledgeDocument(id: string): Promise<void> { return tauriCommand('delete_knowledge_document', { id }); }
export function updateKnowledgeDocument(id: string, title: string, content: string, audience?: import('./types').KnowledgeDocumentAudience, audienceCharacterId?: string | null): Promise<void> { return tauriCommand('update_knowledge_document', { id, title, content, audience: audience ?? null, audienceCharacterId: audienceCharacterId ?? null }); }
export function listWorldKnowledgeCandidates(worldId?: string | null): Promise<import('./types').KnowledgeBase[]> { return tauriCommand('list_world_knowledge_candidates', { worldId: worldId ?? null }); }
export function listWorldKnowledgeScope(worldId: string): Promise<import('./types').KnowledgeBase[]> { return tauriCommand('list_world_knowledge_scope', { worldId }); }
export function fetchStickerPacks(): Promise<StickerPack[]> { return tauriCommand('list_sticker_packs'); }
export function createStickerPack(name: string, description: string, worldId: string | null): Promise<StickerPack> { return tauriCommand('create_sticker_pack', { request: { name, description, worldId } }); }
export function updateStickerPack(id: string, name: string, description: string, enabled: boolean): Promise<StickerPack> { return tauriCommand('update_sticker_pack', { id, request: { name, description, enabled } }); }
export function deleteStickerPack(id: string): Promise<void> { return tauriCommand('delete_sticker_pack', { id }); }
export function fetchStickers(packId: string): Promise<StickerItem[]> { return tauriCommand('list_stickers', { packId }); }
export function createSticker(packId: string, key: string, label: string, description: string, assetPath: string): Promise<StickerItem> { return tauriCommand('create_sticker', { packId, key, label, description, assetPath }); }
export function updateSticker(id: string, key: string, label: string, description: string, assetPath: string, enabled: boolean): Promise<StickerItem> { return tauriCommand('update_sticker', { id, key, label, description, assetPath, enabled }); }
export function deleteSticker(id: string): Promise<void> { return tauriCommand('delete_sticker', { id }); }
export function reorderStickers(packId: string, stickerIds: string[]): Promise<void> { return tauriCommand('reorder_stickers', { packId, stickerIds }); }
export function getStickerWorldBindings(packId: string): Promise<string[]> { return tauriCommand('get_sticker_world_bindings', { packId }); }
export function setStickerWorldBinding(packId: string, worldId: string, enabled: boolean): Promise<void> { return tauriCommand('set_sticker_world_binding', { packId, worldId, enabled }); }
