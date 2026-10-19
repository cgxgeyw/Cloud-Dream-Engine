export type GameUiPlatform = "desktop" | "mobile";

export type GameUiDocument = { html: string };

export type GameUiDiagnostic = {
  severity: "error" | "warning" | "info";
  code: string;
  message: string;
  path?: string;
  line?: number;
  column?: number;
};

export type UiAssetConfig = {
  background_source_mode: string;
  portrait_source_mode: string;
  runtime_image_generation_enabled: boolean;
  local_background_assets: string[];
  local_scene_backgrounds: Record<string, string[]>;
};

export type WorldStorageFieldSchema = {
  type?: "string" | "number" | "integer" | "boolean" | "object" | "array" | "null";
  enum?: Array<string | number | boolean | null>;
  minLength?: number;
  maxLength?: number;
  minimum?: number;
  maximum?: number;
};

export type WorldStorageObjectSchema = {
  type?: "object";
  required?: string[];
  properties?: Record<string, WorldStorageFieldSchema>;
  additionalProperties?: boolean;
};

export type WorldStorageCollectionConfig = {
  schema?: WorldStorageObjectSchema;
  indexes?: string[];
};

export type WorldStorageConfig = {
  kv_namespaces: string[];
  collections: Record<string, WorldStorageCollectionConfig>;
};

export type WorldLogicEventName = "session_start" | "turn_completed" | "interaction_answered";

export type WorldLogicConfig = {
  execution?: "host";
  entry_result_state?: string;
  agent_turn?: { when_field: string; after_handlers: string[]; message: string; view_handler: string };
  runtime: "disabled" | "sandbox-js-v1";
  source: string;
  entry?: string;
  timeout_ms: number;
  events?: Partial<Record<WorldLogicEventName, string>>;
  turn_protocol?: {
    state_handler: string;
    phase_field?: string;
    active_actor_field?: string;
    actor_values?: Record<string, string>;
    terminal_phase_values?: string[];
    initialize_handlers?: string[];
    command_handlers?: Record<string, "player" | "agent" | "system">;
  };
};

export type WorldUiEnvelope = {
  runtime_version: 2 | 3;
  capabilities: string[];
  assets: UiAssetConfig;
  storage: WorldStorageConfig;
  logic: WorldLogicConfig;
  entries: Record<GameUiPlatform, WorldUiEntry>;
  desktop_file: string;
  mobile_file: string;
};

export type WorldUiEntry = {
  document: string;
  stylesheet: string;
};

export type WorldUiEnvelopeV3 = Omit<WorldUiEnvelope, "runtime_version" | "desktop_file" | "mobile_file"> & {
  runtime_version: 3;
};

export type ParsedGameUiDocument = {
  document: GameUiDocument;
  source: string;
  diagnostics: GameUiDiagnostic[];
  error: string | null;
  usedFallback: boolean;
};

export const DEFAULT_UI_ASSET_CONFIG: UiAssetConfig = {
  background_source_mode: "local-first",
  portrait_source_mode: "local-first",
  runtime_image_generation_enabled: false,
  local_background_assets: [],
  local_scene_backgrounds: {},
};
