import type {
  GameUiDiagnostic,
  GameUiDocument,
  GameUiPlatform,
  ParsedGameUiDocument,
  UiAssetConfig,
  WorldLogicConfig,
  WorldStorageCollectionConfig,
  WorldStorageConfig,
  WorldUiEnvelope,
  WorldUiEnvelopeV3,
} from "./types";
import { DEFAULT_UI_ASSET_CONFIG } from "./types";
import {
  DEFAULT_DESKTOP_UI_SOURCE,
  DEFAULT_MOBILE_UI_SOURCE,
} from "./defaults";

const DEFAULT_STORAGE_CONFIG: WorldStorageConfig = { kv_namespaces: [], collections: {} };

export const DEFAULT_DESKTOP_UI_FILE = DEFAULT_DESKTOP_UI_SOURCE;
export const DEFAULT_MOBILE_UI_FILE = DEFAULT_MOBILE_UI_SOURCE;

export function stringifyGameUiDocument(document: GameUiDocument): string {
  return document.html;
}

export function defaultGameUiDocument(platform: GameUiPlatform): GameUiDocument {
  return { html: defaultGameUiFile(platform) };
}

export function defaultGameUiFile(platform: GameUiPlatform): string {
  return platform === "desktop" ? DEFAULT_DESKTOP_UI_FILE : DEFAULT_MOBILE_UI_FILE;
}

export function normalizeAssetConfig(raw: unknown): UiAssetConfig {
  const value = isRecord(raw) ? raw : {};
  return {
    background_source_mode: nonEmptyString(value.background_source_mode) ?? DEFAULT_UI_ASSET_CONFIG.background_source_mode,
    portrait_source_mode: nonEmptyString(value.portrait_source_mode) ?? DEFAULT_UI_ASSET_CONFIG.portrait_source_mode,
    runtime_image_generation_enabled: typeof value.runtime_image_generation_enabled === "boolean"
      ? value.runtime_image_generation_enabled
      : DEFAULT_UI_ASSET_CONFIG.runtime_image_generation_enabled,
    local_background_assets: normalizeStringArray(value.local_background_assets),
    local_scene_backgrounds: normalizeAssetGroups(value.local_scene_backgrounds),
  };
}

function normalizeAssetGroups(raw: unknown): Record<string, string[]> {
  if (!isRecord(raw)) return {};
  return Object.fromEntries(Object.entries(raw)
    .map(([key, value]) => [key.trim(), normalizeStringArray(value)])
    .filter(([key, items]) => Boolean(key) && (items as string[]).length > 0));
}

function normalizeStorageConfig(raw: unknown): WorldStorageConfig {
  if (!isRecord(raw)) return structuredClone(DEFAULT_STORAGE_CONFIG);
  const collections: Record<string, WorldStorageCollectionConfig> = {};
  if (isRecord(raw.collections)) {
    for (const [name, config] of Object.entries(raw.collections)) {
      if (!/^[a-z0-9._-]{1,64}$/i.test(name.trim()) || !isRecord(config)) continue;
      const normalized: WorldStorageCollectionConfig = {};
      if (isRecord(config.schema)) normalized.schema = structuredClone(config.schema) as WorldStorageCollectionConfig["schema"];
      if (Array.isArray(config.indexes)) normalized.indexes = normalizeStringArray(config.indexes);
      collections[name.trim().toLowerCase()] = normalized;
    }
  }
  return {
    kv_namespaces: normalizeStringArray(raw.kv_namespaces).map((name) => name.toLowerCase())
      .filter((name, index, names) => /^[a-z0-9._-]{1,64}$/i.test(name) && names.indexOf(name) === index),
    collections,
  };
}

function normalizeLogicConfig(raw: unknown): WorldLogicConfig {
  const value = isRecord(raw) ? raw : {};
  const source = typeof value.source === "string" ? value.source : "";
  const events = isRecord(value.events) ? Object.fromEntries(Object.entries(value.events)
    .filter(([key, handler]) => ["session_start", "turn_completed", "interaction_answered"].includes(key)
      && typeof handler === "string" && /^[a-zA-Z0-9._-]{1,128}$/.test(handler.trim()))) as WorldLogicConfig["events"] : undefined;
  const rawAgentTurn = isRecord(value.agent_turn) ? value.agent_turn : undefined;
  const rawTurnProtocol = isRecord(value.turn_protocol) ? value.turn_protocol : undefined;
  const rawCommandHandlers = rawTurnProtocol && isRecord(rawTurnProtocol.command_handlers)
    ? Object.fromEntries(Object.entries(rawTurnProtocol.command_handlers)
      .filter(([handler, actor]) => /^[a-zA-Z0-9._-]{1,128}$/.test(handler)
        && (actor === "player" || actor === "agent" || actor === "system"))) as Record<string, "player" | "agent" | "system">
    : undefined;
  const turnProtocol = rawTurnProtocol
    && typeof rawTurnProtocol.state_handler === "string"
    && /^[a-zA-Z0-9._-]{1,128}$/.test(rawTurnProtocol.state_handler)
    ? {
      state_handler: rawTurnProtocol.state_handler,
      phase_field: typeof rawTurnProtocol.phase_field === "string" ? rawTurnProtocol.phase_field : undefined,
      active_actor_field: typeof rawTurnProtocol.active_actor_field === "string" ? rawTurnProtocol.active_actor_field : undefined,
      actor_values: isRecord(rawTurnProtocol.actor_values)
        ? Object.fromEntries(Object.entries(rawTurnProtocol.actor_values).filter(([, raw]) => typeof raw === "string"))
        : undefined,
      terminal_phase_values: Array.isArray(rawTurnProtocol.terminal_phase_values)
        ? rawTurnProtocol.terminal_phase_values.filter((item): item is string => typeof item === "string")
        : undefined,
      initialize_handlers: Array.isArray(rawTurnProtocol.initialize_handlers)
        ? rawTurnProtocol.initialize_handlers.filter((item): item is string => typeof item === "string" && /^[a-zA-Z0-9._-]{1,128}$/.test(item))
        : undefined,
      command_handlers: rawCommandHandlers,
    }
    : undefined;
  return {
    runtime: value.runtime === "sandbox-js-v1" && source.trim() ? "sandbox-js-v1" : "disabled",
    source,
    entry: nonEmptyString(value.entry),
    timeout_ms: Number.isFinite(Number(value.timeout_ms))
      ? Math.min(5_000, Math.max(100, Math.round(Number(value.timeout_ms))))
      : 1_000,
    events,
    execution: value.execution === "host" ? "host" : undefined,
    entry_result_state: nonEmptyString(value.entry_result_state),
    agent_turn: rawAgentTurn
      && typeof rawAgentTurn.when_field === "string"
      && Array.isArray(rawAgentTurn.after_handlers)
      && rawAgentTurn.after_handlers.every((item) => typeof item === "string")
      && typeof rawAgentTurn.message === "string"
      && typeof rawAgentTurn.view_handler === "string"
      ? rawAgentTurn as WorldLogicConfig["agent_turn"]
      : undefined,
    turn_protocol: turnProtocol,
  };
}

export function normalizeWorldUiEnvelope(raw: unknown): WorldUiEnvelope {
  const value = isRecord(raw) ? raw : {};
  const entries = isRecord(value.entries) ? value.entries : {};
  const desktop = normalizeEntry(entries.desktop, value.desktop_file, value.desktop_stylesheet, DEFAULT_DESKTOP_UI_FILE);
  const mobile = normalizeEntry(entries.mobile, value.mobile_file, value.mobile_stylesheet, DEFAULT_MOBILE_UI_FILE);
  return {
    runtime_version: 3,
    capabilities: normalizeStringArray(value.capabilities),
    assets: normalizeAssetConfig(value.assets),
    storage: normalizeStorageConfig(value.storage),
    logic: normalizeLogicConfig(value.logic),
    entries: { desktop, mobile },
    desktop_file: desktop.document,
    mobile_file: mobile.document,
  };
}

function normalizeEntry(raw: unknown, legacyDocument: unknown, legacyStylesheet: unknown, fallback: string) {
  const value = isRecord(raw) ? raw : {};
  return {
    document: firstString(value.document, legacyDocument) ?? fallback,
    stylesheet: firstString(value.stylesheet, legacyStylesheet) ?? "",
  };
}

export function resolveUiFile(envelope: WorldUiEnvelope, platform: GameUiPlatform): string {
  return envelope.entries[platform].document;
}

export function resolveUiStylesheet(envelope: WorldUiEnvelope, platform: GameUiPlatform): string {
  return envelope.entries[platform].stylesheet;
}

export function migrateWorldUiEnvelopeToV3(raw: unknown): WorldUiEnvelopeV3 {
  const { desktop_file: _desktopFile, mobile_file: _mobileFile, ...envelope } = normalizeWorldUiEnvelope(raw);
  return { ...envelope, runtime_version: 3 };
}

export function parseGameUiDocument(source: string, platform: GameUiPlatform): ParsedGameUiDocument {
  const fallbackSource = defaultGameUiFile(platform);
  const diagnostics = diagnoseGameUiDocument(source, platform);
  const errors = diagnostics.filter((item) => item.severity === "error");
  const usedFallback = errors.length > 0 || !source.trim();
  if (!source.trim()) {
    diagnostics.unshift({ severity: "error", code: "ui.empty", message: "HTML 页面不能为空。", path: platform });
  }
  return {
    document: { html: usedFallback ? fallbackSource : source },
    source: usedFallback ? fallbackSource : source,
    diagnostics,
    error: errors[0]?.message ?? (usedFallback ? "HTML 页面为空。" : null),
    usedFallback,
  };
}

export function diagnoseGameUiDocument(source: string, platform: GameUiPlatform): GameUiDiagnostic[] {
  const diagnostics: GameUiDiagnostic[] = [];
  const add = (severity: GameUiDiagnostic["severity"], code: string, message: string) => {
    diagnostics.push({ severity, code, message, path: platform });
  };
  const html = source.trim();
  if (!html) return [{ severity: "error", code: "ui.empty", message: "HTML 页面不能为空。", path: platform }];
  if (html.startsWith("{") || html.startsWith("[")) {
    add("error", "ui.legacy_dsl", "旧 JSONC UI DSL 已停用；世界界面必须是 HTML 文档。");
  }
  if (new Blob([source]).size > 2_000_000) add("error", "ui.document_too_large", "HTML 超过 2 MB 限制。建议将图片等资源作为世界包文件引用。");
  if (!/<html(?:\s|>)/i.test(html) || !/<body(?:\s|>)/i.test(html)) {
    add("error", "ui.document_structure", "页面需要包含完整的 <html> 和 <body> 结构。");
  }
  if (/<script\b[^>]*\bsrc\s*=/i.test(html) || /<link\b[^>]*\brel\s*=\s*["']?stylesheet/i.test(html)) {
    add("error", "ui.external_code", "运行时禁止加载外部脚本和样式表；请将 CSS/JS 写入 HTML 或世界包内联资源。");
  }
  if (/<(?:iframe|object|embed)\b/i.test(html)) add("error", "ui.nested_frame", "世界界面不能创建嵌套 frame、object 或 embed。");
  if (!/<script\b/i.test(html)) add("warning", "ui.no_script", "页面没有脚本；需要动态显示游戏状态时请订阅 world SDK。");
  if (platform === "mobile" && !/<meta\b[^>]*name\s*=\s*["']viewport["']/i.test(html)) {
    add("warning", "ui.mobile_viewport", "移动端页面建议声明 viewport meta，避免窄屏按桌面宽度缩放。");
  }
  if (/<form\b/i.test(html)) add("warning", "ui.form_submit", "原生表单提交会被宿主拦截；请使用按钮和 world SDK 提交动作。");
  if (/<(?:script|style)\b[^>]*>\s*<\/(?:script|style)\s*>/i.test(html)) {
    add("warning", "ui.empty_code_block", "页面包含空的 script 或 style 区块。");
  }
  return diagnostics;
}

export function normalizeGameUiScopeId(scopeId: string): string {
  return scopeId.trim().replace(/[^a-zA-Z0-9_-]+/g, "-") || "game-ui";
}

export function createGameUiScopeSelector(scopeId: string): string {
  return `[data-game-ui-scope="${normalizeGameUiScopeId(scopeId)}"]`;
}

export function normalizeAssetList(value: unknown): string[] {
  return normalizeStringArray(value);
}

export function resolvePreviewBackgroundAsset(config: UiAssetConfig, sceneName: string): string {
  return config.local_scene_backgrounds[sceneName]?.[0] ?? config.local_background_assets[0] ?? "";
}

function normalizeStringArray(value: unknown): string[] {
  return Array.isArray(value)
    ? value.map((item) => String(item).trim()).filter((item, index, items) => item && items.indexOf(item) === index)
    : [];
}

function firstString(...values: unknown[]): string | undefined {
  return values.find((value): value is string => typeof value === "string" && Boolean(value.trim()));
}

function nonEmptyString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function isRecord(value: unknown): value is Record<string, any> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}
