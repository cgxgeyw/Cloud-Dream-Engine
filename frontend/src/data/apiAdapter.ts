let isTauri = false;
let api: typeof import("./api") | null = null;
let tauriApi: typeof import("./tauriApi") | null = null;
let runtimePlatform: "web" | "desktop" | "android" = "web";
let initPromise: Promise<void> | null = null;

import type {
  AppSettings,
  AiWorldCreateRequest,
  AiWorldCreateResponse,
  AiWorldCreateMode,
  AttributeSchemaUpsertRequest,
  AttributeValueType,
  AttributeValueUpsertRequest,
  CharacterCreateFromTemplateRequest,
  CharacterCreateRequest,
  ImageModelTestRequest,
  McpToolCreateRequest,
  ModelConfig,
  PlayerActionRequest,
  RetryFailedLlmStepRequest,
  SessionCreateRequest,
  SessionSnapshot,
  SwitchPlayerCharacterRequest,
  VerifyWorldPackageUiCompatibilityRequest,
  WorldMapTopology,
  WorldUiBundleValidationResult,
  WorldUiBundleValidationRequest,
  WorldUiCompileRequest,
  WorldUiCompileResult,
  WorldUiCompatibilityReport,
  WorldUiDocumentRequest,
  WorldUiDocumentValidationResult,
  WorldPermissionStatus,
  WorldUpsertRequest,
} from "./types";

async function detectTauri(): Promise<boolean> {
  try {
    const mod = await import("@tauri-apps/api/core");
    return typeof mod.invoke === "function" && !!(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  } catch {
    return false;
  }
}

export async function initApiAdapter() {
  const p = (async () => {
    isTauri = await detectTauri();
    runtimePlatform = isTauri
      ? (/android/i.test(navigator.userAgent || "") ? "android" : "desktop")
      : "web";
    if (isTauri) {
      tauriApi = await import("./tauriApi");
      await tauriApi.initTauriRuntime();
    } else {
      api = await import("./api");
    }
    console.log(`API 适配层已初始化：${isTauri ? "Tauri IPC" : "HTTP"}`);
  })();
  initPromise = p;
  await p;
}

/** 等待 API 适配层初始化完成 */
export async function waitForApiReady(): Promise<void> {
  if (initPromise) {
    await initPromise;
  }
}

export function isTauriEnvironment(): boolean {
  return isTauri;
}

export function getRuntimePlatform(): "web" | "desktop" | "android" {
  return runtimePlatform;
}

export function isAndroidRuntime(): boolean {
  return runtimePlatform === "android";
}

export function isDesktopRuntime(): boolean {
  // L3: web 不应被当作 desktop,否则桌面专属功能可能被误调。仅真正的桌面运行时返回 true。
  return runtimePlatform === "desktop";
}

async function getTauri() {
  if (initPromise) await initPromise;
  if (!tauriApi) throw new Error("Tauri API 尚未初始化，请先调用 initApiAdapter().");
  return tauriApi;
}

async function getHttp() {
  if (initPromise) await initPromise;
  if (!api) throw new Error("HTTP API 尚未初始化，请先调用 initApiAdapter().");
  return api;
}

// 同步版本，仅在确认已初始化后使用
function getTauriSync() {
  if (!tauriApi) throw new Error("Tauri API 尚未初始化，请先调用 initApiAdapter().");
  return tauriApi;
}

function getHttpSync() {
  if (!api) throw new Error("HTTP API 尚未初始化，请先调用 initApiAdapter().");
  return api;
}

// 类型：统一从 types.ts 复用，避免双份定义漂移
export type {
  WorldResponse,
  AiWorldCreateRequest,
  AiWorldCreateResponse,
  AiWorldCreateMode,
  WorldMapTopology,
  WorldOpeningMessage,
  WorldCreateRequest,
  WorldUpsertRequest,
  CharacterResponse,
  CharacterCreateRequest,
  CharacterUpsertRequest,
  CharacterTemplateResponse,
  CharacterCreateFromTemplateRequest,
  SessionSnapshot,
  ChatMessage,
  ChatMessageResponse,
  SessionSnapshotResponse,
  SessionMapNode,
  SessionMapEdge,
  InventoryItem,
  SceneRuntime,
  AssetSelection,
  CharacterVisualState,
  SessionState,
  SessionCreateRequest,
  PlayerActionMode,
  PlayerActionRequest,
  RetryFailedLlmStepRequest,
  SwitchCharacterProposalRequest,
  SwitchPlayerCharacterRequest,
  SaveResponse,
  ModelConfig,
  ModelConfigResponse,
  ConnectionTestResult,
  ImageModelTestRequest,
  ImageModelTestResult,
  ModelDiscoverResponse,
  EmbeddingModelFileStatus,
  EmbeddingModelStatus,
  AppSettings,
  SettingsResponse,
  SettingsUpdateRequest,
  PluginResponse,
  McpToolResponse,
  McpToolCreateRequest,
  McpToolUpsertRequest,
  AttributeValueType,
  AttributeSchemaResponse,
  AttributeSchemaUpsertRequest,
  AttributeValueResponse,
  AttributeValueUpsertRequest,
  MemoryEntry,
  RuntimeAttributeItem,
  RuntimeAttributeGroup,
  SessionRuntimeAttributesResponse,
  WorldOpeningPromptPreviewResponse,
  UploadResponse,
  WorldUiBundleValidationResult,
  WorldUiCompileResult,
  WorldUiCompatibilityReport,
  WorldUiDocumentValidationResult,
} from "./types";

// 仅 SessionDebugResponse 从 tauriApi 导入（tauriApi 特有）
export type { SessionDebugResponse } from "./tauriApi";

// API 适配函数
export async function fetchWorlds() {
  return isTauri ? (await getTauri()).fetchWorlds() : (await getHttp()).fetchWorlds();
}

export async function fetchWorld(worldId: string) {
  return isTauri ? (await getTauri()).fetchWorld(worldId) : (await getHttp()).fetchWorld(worldId);
}

export async function createWorld(payload: WorldUpsertRequest) {
  return isTauri ? (await getTauri()).createWorld(payload) : (await getHttp()).createWorld(payload);
}

export async function createWorldWithAi(payload: AiWorldCreateRequest): Promise<AiWorldCreateResponse> {
  return isTauri ? (await getTauri()).createWorldWithAi(payload) : (await getHttp()).createWorldWithAi(payload);
}

export async function onAiWorldCreateProgress(callback: (receivedChars: number) => void): Promise<() => void> {
  if (isTauri) {
    return (await getTauri()).onAiWorldCreateProgress(callback);
  }
  // HTTP mode has no streaming progress channel; return a no-op unsubscribe.
  return () => {};
}

export async function updateWorld(worldId: string, payload: WorldUpsertRequest) {
  return isTauri ? (await getTauri()).updateWorld(worldId, payload) : (await getHttp()).updateWorld(worldId, payload);
}

export async function deleteWorld(worldId: string) {
  return isTauri ? (await getTauri()).deleteWorld(worldId) : (await getHttp()).deleteWorld(worldId);
}

export async function deleteAllWorlds() {
  return isTauri ? (await getTauri()).deleteAllWorlds() : (await getHttp()).deleteAllWorlds();
}

export async function duplicateWorld(worldId: string) {
  return isTauri ? (await getTauri()).duplicateWorld(worldId) : (await getHttp()).duplicateWorld(worldId);
}

export async function fetchWorldCharacters(worldId: string) {
  return isTauri ? (await getTauri()).fetchWorldCharacters(worldId) : (await getHttp()).fetchWorldCharacters(worldId);
}

export async function fetchAllCharacters() {
  return isTauri ? (await getTauri()).fetchAllCharacters() : (await getHttp()).fetchAllCharacters();
}

export async function fetchCharacter(characterId: string) {
  return isTauri ? (await getTauri()).fetchCharacter(characterId) : (await getHttp()).fetchCharacter(characterId);
}

export async function createWorldCharacter(worldId: string, payload: CharacterCreateRequest) {
  if (isTauri) return (await getTauri()).createWorldCharacter(worldId, payload);
  // H10: HTTP 后端要求 CharacterUpsertRequest(含 world_id / custom_tabs),
  // 此前用 `as any` 强转会丢这两个必填字段导致后端静默丢数据或 422。显式补全。
  return (await getHttp()).createWorldCharacter(worldId, toUpsertRequest(worldId, payload));
}

export async function updateWorldCharacter(worldId: string, characterId: string, payload: CharacterCreateRequest) {
  if (isTauri) return (await getTauri()).updateWorldCharacter(worldId, characterId, payload);
  return (await getHttp()).updateWorldCharacter(worldId, characterId, toUpsertRequest(worldId, payload));
}

/// H10: 把通用的 CharacterCreateRequest 适配为 HTTP 后端要求的 CharacterUpsertRequest
/// (api.ts 版本,含 world_id / custom_tabs)。
function toUpsertRequest(
  worldId: string,
  payload: CharacterCreateRequest,
): import("./api").CharacterUpsertRequest {
  return {
    ...payload,
    world_id: worldId,
    custom_tabs:
      (payload as Partial<import("./api").CharacterUpsertRequest>).custom_tabs ?? {},
  };
}

export async function deleteWorldCharacter(worldId: string, characterId: string) {
  return isTauri ? (await getTauri()).deleteWorldCharacter(worldId, characterId) : (await getHttp()).deleteWorldCharacter(worldId, characterId);
}

export async function exportWorldCharacterTemplate(worldId: string, characterId: string) {
  return isTauri ? (await getTauri()).exportWorldCharacterTemplate(worldId, characterId) : (await getHttp()).exportWorldCharacterTemplate(worldId, characterId);
}

export async function createCharacterInWorldFromCharacter(worldId: string, characterId: string, payload: CharacterCreateFromTemplateRequest) {
  return isTauri ? (await getTauri()).createCharacterInWorldFromCharacter(worldId, characterId, payload) : (await getHttp()).createCharacterInWorldFromCharacter(worldId, characterId, payload);
}

export async function fetchWorldOpeningPromptPreview(worldId: string, params?: { playerCharacterId?: string | null; playerInput?: string }) {
  return isTauri ? (await getTauri()).fetchWorldOpeningPromptPreview(worldId, params) : (await getHttp()).fetchWorldOpeningPromptPreview(worldId, params);
}

export async function validateWorldUiDocument(payload: WorldUiDocumentRequest): Promise<WorldUiDocumentValidationResult> {
  return isTauri ? (await getTauri()).validateWorldUiDocument(payload) : (await getHttp()).validateWorldUiDocument(payload);
}

export async function validateWorldUiBundle(payload: WorldUiBundleValidationRequest): Promise<WorldUiBundleValidationResult> {
  return isTauri ? (await getTauri()).validateWorldUiBundle(payload) : (await getHttp()).validateWorldUiBundle(payload);
}

export async function compileWorldUiDocument(payload: WorldUiCompileRequest): Promise<WorldUiCompileResult> {
  return isTauri ? (await getTauri()).compileWorldUiDocument(payload) : (await getHttp()).compileWorldUiDocument(payload);
}

export async function verifyWorldPackageUiCompatibility(
  payload: VerifyWorldPackageUiCompatibilityRequest,
): Promise<WorldUiCompatibilityReport> {
  return isTauri
    ? (await getTauri()).verifyWorldPackageUiCompatibility(payload)
    : (await getHttp()).verifyWorldPackageUiCompatibility(payload);
}

export async function downloadWorldPackage(worldId: string) {
  return isTauri ? (await getTauri()).downloadWorldPackage(worldId) : (await getHttp()).downloadWorldPackage(worldId);
}

export async function importWorldPackage(file: File) {
  return isTauri ? (await getTauri()).importWorldPackage(file) : (await getHttp()).importWorldPackage(file);
}

export async function importWorldPackageFromPath(path: string) {
  if (!isTauri) {
    throw new Error("Importing a world package from a local path requires the Tauri runtime.");
  }
  return (await getTauri()).importWorldPackageFromPath(path);
}

export async function uploadFile(file: File) {
  return isTauri ? (await getTauri()).uploadFile(file) : (await getHttp()).uploadFile(file);
}

export async function fetchSession(sessionId: string) {
  return isTauri ? (await getTauri()).fetchSession(sessionId) : (await getHttp()).fetchSession(sessionId);
}

export async function createSession(payload: SessionCreateRequest) {
  return isTauri ? (await getTauri()).createSession(payload) : (await getHttp()).createSession(payload);
}

export async function submitPlayerAction(sessionId: string, payload: PlayerActionRequest) {
  return isTauri ? (await getTauri()).submitPlayerAction(sessionId, payload) : (await getHttp()).submitPlayerAction(sessionId, payload);
}

export async function streamPlayerAction(
  sessionId: string,
  payload: PlayerActionRequest,
  handlers: { onSnapshot?: (snapshot: SessionSnapshot) => void; onError?: (detail: string) => void } = {},
) {
  return isTauri
    ? (await getTauri()).streamPlayerAction(sessionId, payload, handlers)
    : (await getHttp()).streamPlayerAction(sessionId, payload, handlers);
}

export async function retryFailedLlmStep(
  sessionId: string,
  payload: RetryFailedLlmStepRequest,
) {
  return isTauri
    ? (await getTauri()).retryFailedLlmStep(sessionId, payload)
    : (await getHttp()).retryFailedLlmStep(sessionId, payload);
}

export async function switchPlayerCharacter(sessionId: string, payload: SwitchPlayerCharacterRequest) {
  return isTauri ? (await getTauri()).switchPlayerCharacter(sessionId, payload) : (await getHttp()).switchPlayerCharacter(sessionId, payload);
}

export async function fetchSaves() {
  return isTauri ? (await getTauri()).fetchSaves() : (await getHttp()).fetchSaves();
}

export async function branchSave(saveId: string) {
  return isTauri ? (await getTauri()).branchSave(saveId) : (await getHttp()).branchSave(saveId);
}

export async function deleteSave(saveId: string) {
  return isTauri ? (await getTauri()).deleteSave(saveId) : (await getHttp()).deleteSave(saveId);
}

export async function deleteAllSaves() {
  return isTauri ? (await getTauri()).deleteAllSaves() : (await getHttp()).deleteAllSaves();
}

export async function fetchModels(modelType?: string) {
  return isTauri ? (await getTauri()).fetchModels(modelType) : (await getHttp()).fetchModels(modelType);
}

export async function fetchModel(modelId: string) {
  return isTauri ? (await getTauri()).fetchModel(modelId) : (await getHttp()).fetchModel(modelId);
}

export async function createModel(payload: Omit<ModelConfig, "id">) {
  return isTauri ? (await getTauri()).createModel(payload) : (await getHttp()).createModel(payload);
}

export async function updateModel(modelId: string, payload: Partial<Omit<ModelConfig, "id">>) {
  return isTauri ? (await getTauri()).updateModel(modelId, payload) : (await getHttp()).updateModel(modelId, payload);
}

export async function deleteModel(modelId: string) {
  return isTauri ? (await getTauri()).deleteModel(modelId) : (await getHttp()).deleteModel(modelId);
}

export async function setDefaultModel(modelId: string) {
  return isTauri ? (await getTauri()).setDefaultModel(modelId) : (await getHttp()).setDefaultModel(modelId);
}

export async function testModel(modelId: string) {
  return isTauri ? (await getTauri()).testModel(modelId) : (await getHttp()).testModel(modelId);
}

export async function testImageModel(modelId: string, payload: ImageModelTestRequest) {
  return isTauri ? (await getTauri()).testImageModel(modelId, payload) : (await getHttp()).testImageModel(modelId, payload);
}

type DiscoverModelsParams = {
  provider: string;
  base_url: string;
  api_key: string;
};

export async function discoverModels(
  providerOrParams: string | DiscoverModelsParams,
  baseUrl?: string,
  apiKey?: string,
) {
  const params =
    typeof providerOrParams === "string"
      ? {
          provider: providerOrParams,
          base_url: baseUrl ?? "",
          api_key: apiKey ?? "",
        }
      : providerOrParams;
  if (isTauri) {
    return (await getTauri()).discoverModels(
      params.provider,
      params.base_url,
      params.api_key,
    );
  }
  return (await getHttp()).discoverModels(params);
}

export async function fetchSettings() {
  return isTauri ? (await getTauri()).fetchSettings() : (await getHttp()).fetchSettings();
}

export async function fetchBuiltinEmbeddingModelStatus(modelId?: string) {
  return isTauri
    ? (await getTauri()).fetchBuiltinEmbeddingModelStatus(modelId)
    : (await getHttp()).fetchBuiltinEmbeddingModelStatus(modelId);
}

export async function downloadBuiltinEmbeddingModel(modelId?: string) {
  return isTauri
    ? (await getTauri()).downloadBuiltinEmbeddingModel(modelId)
    : (await getHttp()).downloadBuiltinEmbeddingModel(modelId);
}

export async function updateSettings(payload: AppSettings) {
  return isTauri ? (await getTauri()).updateSettings(payload) : (await getHttp()).updateSettings(payload);
}

export async function getExportDirectorySuggestion() {
  if (!isTauri) {
    throw new Error("导出目录建议仅在 Tauri 模式下可用");
  }
  return (await getTauri()).getExportDirectorySuggestion();
}

export async function fetchPlugins() {
  return isTauri ? (await getTauri()).fetchPlugins() : (await getHttp()).fetchPlugins();
}

export async function fetchMcpTools() {
  return isTauri ? (await getTauri()).fetchMcpTools() : (await getHttp()).fetchMcpTools();
}

export async function createMcpTool(payload: McpToolCreateRequest) {
  return isTauri ? (await getTauri()).createMcpTool(payload) : (await getHttp()).createMcpTool(payload);
}

export async function updateMcpTool(toolId: string, payload: McpToolCreateRequest) {
  return isTauri ? (await getTauri()).updateMcpTool(toolId, payload) : (await getHttp()).updateMcpTool(toolId, payload);
}

export async function deleteMcpTool(toolId: string) {
  return isTauri ? (await getTauri()).deleteMcpTool(toolId) : (await getHttp()).deleteMcpTool(toolId);
}

export async function fetchAttributeSchemas(scope?: string) {
  return isTauri ? (await getTauri()).fetchAttributeSchemas(scope) : (await getHttp()).fetchAttributeSchemas(scope);
}

export async function createAttributeSchema(payload: AttributeSchemaUpsertRequest) {
  return isTauri ? (await getTauri()).createAttributeSchema(payload) : (await getHttp()).createAttributeSchema(payload);
}

export async function updateAttributeSchema(schemaId: string, payload: AttributeSchemaUpsertRequest) {
  return isTauri ? (await getTauri()).updateAttributeSchema(schemaId, payload) : (await getHttp()).updateAttributeSchema(schemaId, payload);
}

export async function deleteAttributeSchema(schemaId: string) {
  return isTauri ? (await getTauri()).deleteAttributeSchema(schemaId) : (await getHttp()).deleteAttributeSchema(schemaId);
}

export async function fetchAttributeValues(params: { ownerType?: string; ownerId?: string; schemaId?: string }) {
  return isTauri ? (await getTauri()).fetchAttributeValues(params) : (await getHttp()).fetchAttributeValues(params);
}

export async function upsertAttributeValue(payload: AttributeValueUpsertRequest) {
  return isTauri ? (await getTauri()).upsertAttributeValue(payload) : (await getHttp()).upsertAttributeValue(payload);
}

export async function requestWorldPermissions(permissions: string[]): Promise<WorldPermissionStatus[]> {
  if (!isTauri) {
    return permissions.map((permission) => ({
      permission,
      requested: false,
      granted: null,
      error: null,
    }));
  }
  return (await getTauri()).requestWorldPermissions(permissions);
}

export async function fetchMemories(worldId?: string, sessionId?: string, characterId?: string, layer?: string, limit?: number) {
  return isTauri ? (await getTauri()).fetchMemories(worldId, sessionId, characterId, layer, limit) : (await getHttp()).fetchMemories(worldId, sessionId, characterId, layer, limit);
}

export async function fetchSessionDebug(sessionId: string) {
  return isTauri ? (await getTauri()).fetchSessionDebug(sessionId) : (await getHttp()).fetchSessionDebug(sessionId);
}

export async function fetchSessionRuntimeAttributes(sessionId: string) {
  return isTauri
    ? (await getTauri()).fetchSessionRuntimeAttributes(sessionId)
    : (await getHttp()).fetchSessionRuntimeAttributes(sessionId);
}

export async function onSessionSnapshot(sessionId: string, callback: (snapshot: SessionSnapshot) => void): Promise<() => void> {
  if (isTauri) {
    return (await getTauri()).onSessionSnapshot(sessionId, callback);
  }
  // HTTP 模式下使用 WebSocket 接收实时更新
  const wsUrl = (await getHttp()).toSessionWebSocketUrl(sessionId);
  const ws = new WebSocket(wsUrl);
  ws.onmessage = (event) => {
    try {
      const data = JSON.parse(event.data);
      if ((data.type === "snapshot" || data.type === "session.snapshot") && data.payload) {
        callback(data.payload);
      }
    } catch {
      // 忽略单条消息解析失败，继续保持连接
    }
  };
  return () => ws.close();
}

export function toSessionWebSocketUrl(sessionId: string): string {
  if (isTauri) {
    throw new Error("WebSocket URL is unavailable in Tauri mode; use Tauri events instead.");
  }
  return getHttpSync().toSessionWebSocketUrl(sessionId);
}

export function assetUrl(path: string | null | undefined): string {
  if (!tauriApi && !api) {
    // 如果还没初始化，返回空串，避免崩溃
    return "";
  }
  return isTauri ? getTauriSync().assetUrl(path) : getHttpSync().assetUrl(path ?? "");
}
