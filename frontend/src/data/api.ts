import type {
  AiWorldCreateRequest,
  AiWorldCreateResponse,
  // Shared types from types.ts
  WorldResponse, WorldOpeningMessage, WorldUpsertRequest,
  CharacterResponse, CharacterTemplateResponse, CharacterCreateFromTemplateRequest,
  ChatMessage, ChatMessageResponse,
  SessionSnapshot, SessionSnapshotResponse, SessionMapNode, SessionMapEdge,
  InventoryItem, SceneRuntime, AssetSelection, CharacterVisualState, SessionState,
  SessionCreateRequest, PlayerActionMode, PlayerActionRequest, RetryFailedLlmStepRequest,
  SwitchCharacterProposalRequest, SwitchPlayerCharacterRequest,
  SaveResponse, ModelConfigResponse,
  ImageModelTestRequest, EmbeddingModelFileStatus, EmbeddingModelStatus,
  SettingsResponse, SettingsUpdateRequest, PluginResponse,
  McpToolResponse, McpToolUpsertRequest,
  AttributeSchemaResponse, AttributeSchemaUpsertRequest,
  AttributeValueResponse, AttributeValueUpsertRequest,
  RuntimeAttributeItem, RuntimeAttributeGroup, SessionRuntimeAttributesResponse,
  WorldOpeningPromptPreviewResponse,
  VerifyWorldPackageUiCompatibilityRequest,
  WorldUiBundleValidationRequest,
  WorldUiBundleValidationResult,
  WorldUiCompileRequest,
  WorldUiCompileResult,
  WorldUiCompatibilityReport,
  WorldUiDocumentRequest,
  WorldUiDocumentValidationResult,
} from "./types";
import type { SessionDebugResponse } from "./tauriApi";

export type {
  WorldResponse, WorldOpeningMessage, WorldUpsertRequest,
  CharacterResponse, CharacterTemplateResponse, CharacterCreateFromTemplateRequest,
  ChatMessage, ChatMessageResponse,
  SessionSnapshot, SessionSnapshotResponse, SessionMapNode, SessionMapEdge,
  InventoryItem, SceneRuntime, AssetSelection, CharacterVisualState, SessionState,
  SessionCreateRequest, PlayerActionMode, PlayerActionRequest, RetryFailedLlmStepRequest,
  SwitchCharacterProposalRequest, SwitchPlayerCharacterRequest,
  SaveResponse, ModelConfigResponse,
  ImageModelTestRequest, EmbeddingModelFileStatus, EmbeddingModelStatus,
  SettingsResponse, SettingsUpdateRequest, PluginResponse,
  McpToolResponse, McpToolUpsertRequest,
  AttributeSchemaResponse, AttributeSchemaUpsertRequest,
  AttributeValueResponse, AttributeValueUpsertRequest,
  RuntimeAttributeItem, RuntimeAttributeGroup, SessionRuntimeAttributesResponse,
  WorldOpeningPromptPreviewResponse,
  WorldUiDocumentRequest, WorldUiDocumentValidationResult,
  WorldUiBundleValidationRequest, WorldUiBundleValidationResult,
  WorldUiCompileRequest, WorldUiCompileResult,
  VerifyWorldPackageUiCompatibilityRequest, WorldUiCompatibilityReport,
} from "./types";

export type { SessionDebugResponse } from "./tauriApi";

export type SessionStateResponse = SessionState;
export type SceneRuntimeResponse = SceneRuntime;
export type SessionMapNodeResponse = SessionMapNode;
export type SessionMapEdgeResponse = SessionMapEdge;
export type CharacterVisualStateResponse = CharacterVisualState;
export type AssetSelectionResponse = AssetSelection;
export type InventoryItemResponse = InventoryItem;
export type RuntimeAttributeItemResponse = RuntimeAttributeItem;
export type RuntimeAttributeGroupResponse = RuntimeAttributeGroup;
export type CharacterUpsertRequest = {
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
  custom_tabs: Record<string, string>;
  system_prompt_template: string;
  response_contract_prompt: string;
  narration_prompt: string;
  runtime_system_prompt: string;
};

export type CharacterMemoryGroupResponse = { character_id: string; character_name: string; memories: MemoryEntryResponse[]; };
export type ImageModelTestResponse = { ok: boolean; detail: string; debug_lines: string[]; asset_path?: string | null; image_url?: string | null; seed?: number | null; };
export type MemoryEntryResponse = { id: string; world_id: string; session_id: string; conversation_id?: string | null; character_id: string; event_id?: string | null; item_id?: string | null; scene_id?: string | null; layer: string; content: string; source: string; importance: number; created_at: string; memory_type: string; speaker?: string | null; role?: string | null; location?: string | null; participants: string[]; keywords: string[]; };
export type ModelConfigUpsertRequest = { name: string; model_type: string; provider: string; model_id: string; base_url: string; api_key: string; max_tokens: number; streaming_enabled: boolean; is_default: boolean; };
export type ModelEndpointDiscoveryRequest = { provider: string; base_url: string; api_key: string; };
export type ModelEndpointDiscoveryResponse = { ok: boolean; detail: string; model_ids: string[]; debug_lines: string[]; };
export type ModelTestResponse = { ok: boolean; detail: string; debug_lines: string[]; };
export type PromptTracePreviewResponse = { speaker?: string | null; prompt_trace: Record<string, unknown>; };
export type SpeakerSelectionPreviewResponse = { speaker: string; debug_lines: string[]; };
export type WorldTemplateResponse = { name: string; genre: string; background_prompt: string; opening_scene: string; summary: string; time_system: string; map_nodes: import("./types").WorldMapTopology; triggers: string[]; custom_tabs: Record<string, string>; time_config: Record<string, unknown>; director_config: Record<string, unknown>; ui_theme_config: Record<string, unknown>; opening_messages: WorldOpeningMessage[]; opening_character_names: string[]; player_character_name?: string | null; characters: CharacterTemplateResponse[]; };
const DEFAULT_API_BASE_URL =
  typeof window !== "undefined" && /^https?:$/i.test(window.location.protocol)
    ? window.location.origin
    : "http://127.0.0.1:8010";

export let API_BASE_URL = (import.meta.env.VITE_API_BASE_URL as string | undefined) ?? DEFAULT_API_BASE_URL;

export function setApiBaseUrl(apiBaseUrl: string) {
  const trimmed = apiBaseUrl.trim().replace(/\/+$/, "");
  if (trimmed) {
    API_BASE_URL = trimmed;
  }
}

function toApiUrl(path: string) {
  return `${API_BASE_URL}${path}`;
}

async function readErrorMessage(response: Response): Promise<string> {
  const fallback = `Request failed: ${response.status} ${response.statusText}`.trim();

  try {
    const contentType = response.headers.get("Content-Type") ?? "";
    if (contentType.includes("application/json")) {
      const payload = (await response.json()) as { detail?: unknown };
      if (typeof payload.detail === "string" && payload.detail.trim()) {
        return payload.detail.trim();
      }
    }

    const text = (await response.text()).trim();
    if (text) {
      return text;
    }
  } catch {
    return fallback;
  }

  return fallback;
}

export function toSessionWebSocketUrl(sessionId: string) {
  const url = new URL(API_BASE_URL);
  const protocol = url.protocol === "https:" ? "wss:" : "ws:";
  return `${protocol}//${url.host}/ws/sessions/${sessionId}`;
}

async function fetchJson<T>(path: string): Promise<T> {
  const response = await fetch(toApiUrl(path));
  if (!response.ok) {
    throw new Error(await readErrorMessage(response));
  }

  return (await response.json()) as T;
}

async function requestJson<TResponse, TPayload>(
  method: "POST" | "PUT",
  path: string,
  payload: TPayload,
): Promise<TResponse> {
  const response = await fetch(toApiUrl(path), {
    method,
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify(payload),
  });
  if (!response.ok) {
    throw new Error(await readErrorMessage(response));
  }

  return (await response.json()) as TResponse;
}

export function fetchWorlds() {
  return fetchJson<WorldResponse[]>("/api/worlds");
}

export function fetchAttributeSchemas(scope?: string) {
  const suffix = scope ? `?scope=${encodeURIComponent(scope)}` : "";
  return fetchJson<AttributeSchemaResponse[]>(`/api/attributes/schemas${suffix}`);
}

export function createAttributeSchema(payload: AttributeSchemaUpsertRequest) {
  return requestJson<AttributeSchemaResponse, AttributeSchemaUpsertRequest>("POST", "/api/attributes/schemas", payload);
}

export function updateAttributeSchema(schemaId: string, payload: AttributeSchemaUpsertRequest) {
  return requestJson<AttributeSchemaResponse, AttributeSchemaUpsertRequest>("PUT", `/api/attributes/schemas/${schemaId}`, payload);
}

export function fetchAttributeValues(params: {
  ownerType?: string;
  ownerId?: string;
  schemaId?: string;
}) {
  const search = new URLSearchParams();
  if (params.ownerType) search.set("owner_type", params.ownerType);
  if (params.ownerId) search.set("owner_id", params.ownerId);
  if (params.schemaId) search.set("schema_id", params.schemaId);
  const suffix = search.size > 0 ? `?${search.toString()}` : "";
  return fetchJson<AttributeValueResponse[]>(`/api/attributes/values${suffix}`);
}

export function upsertAttributeValue(payload: AttributeValueUpsertRequest) {
  return requestJson<AttributeValueResponse, AttributeValueUpsertRequest>("PUT", "/api/attributes/values", payload);
}

export function fetchWorld(worldId: string) {
  return fetchJson<WorldResponse>(`/api/worlds/${worldId}`);
}

export function fetchWorldCharacters(worldId: string) {
  return fetchJson<CharacterResponse[]>(`/api/worlds/${worldId}/characters`);
}

export function fetchWorldOpeningPromptPreview(worldId: string, params?: { playerCharacterId?: string | null; playerInput?: string }) {
  const search = new URLSearchParams();
  if (params?.playerCharacterId) search.set("player_character_id", params.playerCharacterId);
  if (params?.playerInput) search.set("player_input", params.playerInput);
  const suffix = search.size > 0 ? `?${search.toString()}` : "";
  return fetchJson<WorldOpeningPromptPreviewResponse>(`/api/worlds/${worldId}/opening-prompt-preview${suffix}`);
}

function unsupportedInHttpMode(name: string): never {
  throw new Error(`${name} is currently available only in the Tauri runtime.`);
}

export function validateWorldUiDocument(_payload: WorldUiDocumentRequest): Promise<WorldUiDocumentValidationResult> {
  unsupportedInHttpMode("validateWorldUiDocument");
}

export function validateWorldUiBundle(_payload: WorldUiBundleValidationRequest): Promise<WorldUiBundleValidationResult> {
  unsupportedInHttpMode("validateWorldUiBundle");
}

export function compileWorldUiDocument(_payload: WorldUiCompileRequest): Promise<WorldUiCompileResult> {
  unsupportedInHttpMode("compileWorldUiDocument");
}

export function verifyWorldPackageUiCompatibility(
  _payload: VerifyWorldPackageUiCompatibilityRequest,
): Promise<WorldUiCompatibilityReport> {
  unsupportedInHttpMode("verifyWorldPackageUiCompatibility");
}

export function createWorld(payload: WorldUpsertRequest) {
  return requestJson<WorldResponse, WorldUpsertRequest>("POST", "/api/worlds", payload);
}

export function createWorldWithAi(_payload: AiWorldCreateRequest): Promise<AiWorldCreateResponse> {
  unsupportedInHttpMode("createWorldWithAi");
}

export function updateWorld(worldId: string, payload: WorldUpsertRequest) {
  return requestJson<WorldResponse, WorldUpsertRequest>("PUT", `/api/worlds/${worldId}`, payload);
}

export function duplicateWorld(worldId: string) {
  return requestJson<WorldResponse, Record<string, never>>("POST", `/api/worlds/${worldId}/duplicate`, {});
}

export function exportWorldTemplate(worldId: string) {
  return requestJson<WorldTemplateResponse, Record<string, never>>("POST", `/api/worlds/${worldId}/export`, {});
}

function resolveDownloadFilename(response: Response, fallback: string): string {
  const header = response.headers.get("Content-Disposition") ?? response.headers.get("content-disposition") ?? "";
  const utf8Match = header.match(/filename\*=UTF-8''([^;]+)/i);
  if (utf8Match?.[1]) {
    try {
      return decodeURIComponent(utf8Match[1]);
    } catch {
      return utf8Match[1];
    }
  }

  const quotedMatch = header.match(/filename=\"([^\"]+)\"/i);
  if (quotedMatch?.[1]) {
    return quotedMatch[1];
  }

  const plainMatch = header.match(/filename=([^;]+)/i);
  if (plainMatch?.[1]) {
    return plainMatch[1].trim();
  }

  return fallback;
}

export async function downloadWorldPackage(worldId: string): Promise<{ blob?: Blob; filename: string; savedPath?: string }> {
  const response = await fetch(toApiUrl(`/api/worlds/${worldId}/export-package`), {
    method: "POST",
  });
  if (!response.ok) {
    throw new Error(await readErrorMessage(response));
  }

  return {
    blob: await response.blob(),
    filename: resolveDownloadFilename(response, `world-package-${worldId}.zip`),
  };
}

export async function importWorldPackage(file: File): Promise<WorldResponse> {
  const formData = new FormData();
  formData.append("file", file);
  const response = await fetch(toApiUrl("/api/worlds/import-package"), {
    method: "POST",
    body: formData,
  });
  if (!response.ok) {
    throw new Error(await readErrorMessage(response));
  }

  return (await response.json()) as WorldResponse;
}

export function createWorldCharacter(worldId: string, payload: CharacterUpsertRequest) {
  return requestJson<CharacterResponse, CharacterUpsertRequest>("POST", `/api/worlds/${worldId}/characters`, payload);
}

export function updateWorldCharacter(worldId: string, characterId: string, payload: CharacterUpsertRequest) {
  return requestJson<CharacterResponse, CharacterUpsertRequest>(
    "PUT",
    `/api/worlds/${worldId}/characters/${characterId}`,
    payload,
  );
}

export function deleteWorldCharacter(worldId: string, characterId: string) {
  return fetch(toApiUrl(`/api/worlds/${worldId}/characters/${characterId}`), { method: "DELETE" }).then((response) => {
    if (!response.ok) throw new Error(`Request failed: ${response.status}`);
  });
}

export function exportWorldCharacterTemplate(worldId: string, characterId: string) {
  return requestJson<CharacterTemplateResponse, Record<string, never>>(
    "POST",
    `/api/worlds/${worldId}/characters/${characterId}/export-template`,
    {},
  );
}

export function createCharacterInWorldFromCharacter(
  worldId: string,
  characterId: string,
  payload: CharacterCreateFromTemplateRequest,
) {
  return requestJson<CharacterResponse, CharacterCreateFromTemplateRequest>(
    "POST",
    `/api/worlds/${worldId}/characters/${characterId}/create-in-world`,
    payload,
  );
}

export function deleteWorld(worldId: string) {
  return fetch(toApiUrl(`/api/worlds/${worldId}`), { method: "DELETE" }).then((response) => {
    if (!response.ok) throw new Error(`Request failed: ${response.status}`);
  });
}

export async function deleteAllWorlds(): Promise<{ ok: boolean; deleted_count: number }> {
  const response = await fetch(toApiUrl("/api/worlds"), { method: "DELETE" });
  if (!response.ok) {
    throw new Error(`Request failed: ${response.status}`);
  }
  return (await response.json()) as { ok: boolean; deleted_count: number };
}

export function fetchCharacter(characterId: string) {
  return fetchJson<CharacterResponse>(`/api/characters/${characterId}`);
}

export function fetchSaves() {
  return fetchJson<SaveResponse[]>("/api/saves");
}

export function deleteSave(saveId: string) {
  return fetch(toApiUrl(`/api/saves/${saveId}`), { method: "DELETE" }).then((response) => {
    if (!response.ok) throw new Error(`Request failed: ${response.status}`);
  });
}

export async function deleteAllSaves(): Promise<{ ok: boolean; deleted_count: number }> {
  const response = await fetch(toApiUrl("/api/saves"), { method: "DELETE" });
  if (!response.ok) {
    throw new Error(`Request failed: ${response.status}`);
  }
  return (await response.json()) as { ok: boolean; deleted_count: number };
}

export function branchSave(saveId: string) {
  return requestJson<SaveResponse, Record<string, never>>("POST", `/api/saves/${saveId}/branch`, {});
}

export function fetchSession(sessionId: string) {
  return fetchJson<SessionSnapshotResponse>(`/api/sessions/${sessionId}`);
}

export function createSession(payload: SessionCreateRequest) {
  return requestJson<SessionSnapshotResponse, SessionCreateRequest>("POST", "/api/sessions", payload);
}

export function submitPlayerAction(sessionId: string, payload: PlayerActionRequest) {
  return requestJson<SessionSnapshotResponse, PlayerActionRequest>("POST", `/api/sessions/${sessionId}/actions`, payload);
}

type StreamPlayerActionHandlers = {
  onSnapshot?: (snapshot: SessionSnapshotResponse) => void;
  onError?: (detail: string) => void;
};

export async function streamPlayerAction(
  sessionId: string,
  payload: PlayerActionRequest,
  handlers: StreamPlayerActionHandlers = {},
): Promise<SessionSnapshotResponse | null> {
  const response = await fetch(toApiUrl(`/api/sessions/${sessionId}/actions/stream`), {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Accept: "text/event-stream",
    },
    body: JSON.stringify(payload),
  });
  if (!response.ok) {
    throw new Error(await readErrorMessage(response));
  }
  if (!response.body) {
    throw new Error("Stream unavailable");
  }

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  let currentEvent = "message";
  let dataLines: string[] = [];
  let latestSnapshot: SessionSnapshotResponse | null = null;
  let streamError: string | null = null;

  const flushEvent = () => {
    if (dataLines.length === 0) {
      currentEvent = "message";
      return;
    }
    const data = dataLines.join("\n");
    dataLines = [];
    const eventName = currentEvent || "message";
    currentEvent = "message";

    let parsed: { type?: string; payload?: SessionSnapshotResponse; detail?: string } | null = null;
    try {
      parsed = JSON.parse(data) as { type?: string; payload?: SessionSnapshotResponse; detail?: string };
    } catch {
      return;
    }
    if (!parsed) {
      return;
    }

    if (eventName === "session.snapshot" && parsed.payload) {
      latestSnapshot = parsed.payload;
      handlers.onSnapshot?.(parsed.payload);
      return;
    }
    if (eventName === "error") {
      streamError = parsed.detail?.trim() || "Request failed";
      handlers.onError?.(streamError);
    }
  };

  while (true) {
    const { value, done } = await reader.read();
    buffer += decoder.decode(value ?? new Uint8Array(), { stream: !done });

    let newlineIndex = buffer.indexOf("\n");
    while (newlineIndex !== -1) {
      const rawLine = buffer.slice(0, newlineIndex);
      buffer = buffer.slice(newlineIndex + 1);
      const line = rawLine.endsWith("\r") ? rawLine.slice(0, -1) : rawLine;

      if (!line) {
        flushEvent();
      } else if (line.startsWith("event:")) {
        currentEvent = line.slice(6).trim() || "message";
      } else if (line.startsWith("data:")) {
        dataLines.push(line.slice(5).trim());
      }

      newlineIndex = buffer.indexOf("\n");
    }

    if (done) {
      break;
    }
  }

  if (buffer.trim() || dataLines.length > 0) {
    flushEvent();
  }

  if (streamError) {
    throw new Error(streamError);
  }
  return latestSnapshot;
}

export function retryFailedLlmStep(sessionId: string, payload: RetryFailedLlmStepRequest) {
  return requestJson<SessionSnapshotResponse, RetryFailedLlmStepRequest>(
    "POST",
    `/api/sessions/${sessionId}/retry-failed-llm-step`,
    payload,
  );
}

export function switchPlayerCharacter(sessionId: string, payload: SwitchPlayerCharacterRequest) {
  return requestJson<SessionSnapshotResponse, SwitchPlayerCharacterRequest>("POST", `/api/sessions/${sessionId}/switch-character`, payload);
}

export function fetchSettings() {
  return fetchJson<SettingsResponse>("/api/settings");
}

export function updateSettings(payload: SettingsUpdateRequest) {
  return requestJson<SettingsResponse, SettingsUpdateRequest>("PUT", "/api/settings", payload);
}

export function fetchModels(modelType?: string) {
  const suffix = modelType ? `?model_type=${encodeURIComponent(modelType)}` : "";
  return fetchJson<ModelConfigResponse[]>(`/api/models${suffix}`);
}

export function fetchModel(modelId: string) {
  return fetchJson<ModelConfigResponse>(`/api/models/${modelId}`);
}

export function createModel(payload: ModelConfigUpsertRequest) {
  return requestJson<ModelConfigResponse, ModelConfigUpsertRequest>("POST", "/api/models", payload);
}

export function updateModel(modelId: string, payload: Partial<ModelConfigUpsertRequest>) {
  return requestJson<ModelConfigResponse, Partial<ModelConfigUpsertRequest>>("PUT", `/api/models/${modelId}`, payload);
}

export function deleteModel(modelId: string) {
  return fetch(toApiUrl(`/api/models/${modelId}`), { method: "DELETE" }).then((response) => {
    if (!response.ok) throw new Error(`Request failed: ${response.status}`);
  });
}

export function setDefaultModel(modelId: string) {
  return fetch(toApiUrl(`/api/models/${modelId}/set-default`), { method: "POST" }).then((response) => {
    if (!response.ok) throw new Error(`Request failed: ${response.status}`);
  });
}

export function testModel(modelId: string) {
  return requestJson<ModelTestResponse, Record<string, never>>("POST", `/api/models/${modelId}/test`, {});
}

export function testImageModel(modelId: string, payload: ImageModelTestRequest) {
  return requestJson<ImageModelTestResponse, ImageModelTestRequest>("POST", `/api/models/${modelId}/test-image`, payload);
}

export function discoverModels(payload: ModelEndpointDiscoveryRequest) {
  return requestJson<ModelEndpointDiscoveryResponse, ModelEndpointDiscoveryRequest>("POST", "/api/models/discover", payload);
}

export function fetchBuiltinEmbeddingModelStatus(modelId?: string) {
  const query = modelId ? `?modelId=${encodeURIComponent(modelId)}` : "";
  return fetchJson<EmbeddingModelStatus>(`/api/models/builtin-embedding-status${query}`);
}

export function downloadBuiltinEmbeddingModel(modelId?: string) {
  return requestJson<EmbeddingModelStatus, { model_id?: string }>(
    "POST",
    "/api/models/builtin-embedding-download",
    modelId ? { model_id: modelId } : {},
  );
}

export type UploadResponse = {
  filename: string;
  relative_path?: string;
  asset_path?: string;
  url: string;   // 例如 "/assets/abc123.png"
};

export async function uploadFile(file: File): Promise<UploadResponse> {
  const formData = new FormData();
  formData.append("file", file);
  const response = await fetch(toApiUrl("/api/uploads"), {
    method: "POST",
    body: formData,
  });
  if (!response.ok) {
    throw new Error(`Upload failed: ${response.status}`);
  }
  return (await response.json()) as UploadResponse;
}

export async function deleteAsset(filename: string): Promise<void> {
  const response = await fetch(toApiUrl(`/api/uploads/${filename}`), { method: "DELETE" });
  if (!response.ok) throw new Error(`Delete failed: ${response.status}`);
}

/**
 * 把后端保存的资源路径转换成前端可直接使用的 URL。
 * 后端返回的资源路径通常形如 "/assets/xxx.png"。
 * 开发环境下直接补上 API_BASE_URL，避免依赖代理配置。
 * 生产环境则继续使用同源路径。
 */
export function assetUrl(path: string): string {
  const normalized = normalizeAssetReference(path);
  if (!normalized) return "";
  if (normalized.startsWith("http")) return normalized;
  // 开发环境下补上 API_BASE_URL，直接命中后端静态资源
  if (normalized.startsWith("/")) return `${API_BASE_URL}${normalized}`;
  return `${API_BASE_URL}/${normalized}`;
}

function normalizeAssetReference(path: string | null | undefined): string {
  if (!path) return "";
  const trimmed = path.trim();
  if (!trimmed || trimmed === "static") return "";
  if (trimmed.startsWith("http://") || trimmed.startsWith("https://") || trimmed.startsWith("data:")) {
    return trimmed;
  }
  const unix = trimmed.split("\\").join("/");
  if (unix.startsWith("/assets/")) return unix;
  if (unix.startsWith("assets/")) return `/${unix}`;
  return `/assets/${unix.replace(/^\/+/, "")}`;
}

export function fetchPlugins() {
  return fetchJson<PluginResponse[]>("/api/plugins");
}

export function fetchMcpTools() {
  return fetchJson<McpToolResponse[]>("/api/mcp/tools");
}

export function createMcpTool(payload: McpToolUpsertRequest) {
  return requestJson<McpToolResponse, McpToolUpsertRequest>("POST", "/api/mcp/tools", payload);
}

export function updateMcpTool(toolId: string, payload: McpToolUpsertRequest) {
  return requestJson<McpToolResponse, McpToolUpsertRequest>("PUT", `/api/mcp/tools/${toolId}`, payload);
}

export function deleteMcpTool(toolId: string) {
  return fetch(toApiUrl(`/api/mcp/tools/${toolId}`), { method: "DELETE" }).then((response) => {
    if (!response.ok) throw new Error(`Request failed: ${response.status}`);
  });
}

export function fetchSessionRuntimeAttributes(sessionId: string) {
  return fetchJson<SessionRuntimeAttributesResponse>(`/api/sessions/${sessionId}/runtime-attributes`);
}

export function fetchSessionDebug(sessionId: string) {
  return fetchJson<SessionDebugResponse>(`/api/debug/sessions/${sessionId}`);
}

export function fetchAllCharacters() {
  return fetchJson<CharacterResponse[]>("/api/characters");
}

export function fetchMemories(_worldId?: string, _sessionId?: string, _characterId?: string, _layer?: string, _limit?: number) {
  return fetchJson<MemoryEntryResponse[]>("/api/memories");
}
