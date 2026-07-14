import { parse, printParseErrorCode, type ParseError } from "jsonc-parser";
import type { CSSProperties } from "react";
import type {
  GameUiActionReference,
  GameUiAnchor,
  GameUiComponentStyleDefinition,
  GameUiDocument,
  GameUiDocumentV2,
  GameUiLayoutNode,
  GameUiLayoutNodeV2,
  GameUiMountId,
  GameUiMountOptions,
  GameUiNodeBase,
  GameUiPlatform,
  GameUiPropValue,
  GameUiStyleRecord,
  ParsedGameUiDocument,
  UiAssetConfig,
  WorldUiEnvelope,
} from "./types";
import {
  DEFAULT_UI_ASSET_CONFIG,
  GAME_UI_MOUNT_IDS,
  SUPPORTED_GAME_UI_SCHEMA_VERSIONS,
} from "./types";
import {
  DEFAULT_DESKTOP_DOCUMENT,
  DEFAULT_DESKTOP_UI_SOURCE,
  DEFAULT_MOBILE_DOCUMENT,
  DEFAULT_MOBILE_UI_SOURCE,
} from "./defaults";

const EMPTY_V2_NODE: GameUiLayoutNodeV2 = {
  type: "stack",
  children: [],
};

export const DEFAULT_DESKTOP_UI_FILE = DEFAULT_DESKTOP_UI_SOURCE;
export const DEFAULT_MOBILE_UI_FILE = DEFAULT_MOBILE_UI_SOURCE;

export function stringifyGameUiDocument(document: GameUiDocument): string {
  return `${JSON.stringify(document, null, 2)}\n`;
}

export function defaultGameUiDocument(platform: GameUiPlatform): GameUiDocument {
  return structuredClone(platform === "desktop" ? DEFAULT_DESKTOP_DOCUMENT : DEFAULT_MOBILE_DOCUMENT);
}

export function defaultGameUiFile(platform: GameUiPlatform): string {
  return platform === "desktop" ? DEFAULT_DESKTOP_UI_FILE : DEFAULT_MOBILE_UI_FILE;
}

export function normalizeAssetConfig(raw: unknown): UiAssetConfig {
  const value = isPlainObject(raw) ? raw : {};
  return {
    background_source_mode:
      typeof value.background_source_mode === "string" && value.background_source_mode.trim()
        ? value.background_source_mode.trim()
        : DEFAULT_UI_ASSET_CONFIG.background_source_mode,
    portrait_source_mode:
      typeof value.portrait_source_mode === "string" && value.portrait_source_mode.trim()
        ? value.portrait_source_mode.trim()
        : DEFAULT_UI_ASSET_CONFIG.portrait_source_mode,
    runtime_image_generation_enabled:
      typeof value.runtime_image_generation_enabled === "boolean"
        ? value.runtime_image_generation_enabled
        : DEFAULT_UI_ASSET_CONFIG.runtime_image_generation_enabled,
    local_background_assets: Array.isArray(value.local_background_assets)
      ? value.local_background_assets.map((item) => String(item).trim()).filter(Boolean)
      : [...DEFAULT_UI_ASSET_CONFIG.local_background_assets],
    local_scene_backgrounds: normalizeAssetGroupMap(value.local_scene_backgrounds),
  };
}

function normalizeAssetGroupMap(raw: unknown): Record<string, string[]> {
  if (!isPlainObject(raw)) {
    return {};
  }

  return Object.fromEntries(
    Object.entries(raw)
      .map(([key, value]) => [
        key.trim(),
        Array.isArray(value) ? value.map((item) => String(item).trim()).filter(Boolean) : [],
      ])
      .filter(([key, items]) => key && items.length > 0),
  );
}

export function normalizeWorldUiEnvelope(raw: unknown): WorldUiEnvelope {
  const value = isPlainObject(raw) ? raw : {};
  return {
    assets: normalizeAssetConfig(value.assets),
    desktop_file:
      typeof value.desktop_file === "string" && value.desktop_file.trim()
        ? value.desktop_file
        : DEFAULT_DESKTOP_UI_FILE,
    mobile_file:
      typeof value.mobile_file === "string" && value.mobile_file.trim()
        ? value.mobile_file
        : DEFAULT_MOBILE_UI_FILE,
  };
}

export function resolveUiFile(envelope: WorldUiEnvelope, platform: GameUiPlatform): string {
  return platform === "desktop" ? envelope.desktop_file : envelope.mobile_file;
}

export function parseGameUiDocument(
  source: string,
  platform: GameUiPlatform,
): ParsedGameUiDocument {
  const fallbackDocument = defaultGameUiDocument(platform);
  const fallbackSource = defaultGameUiFile(platform);
  const trimmed = source.trim();

  if (!trimmed) {
    return {
      document: fallbackDocument,
      source: fallbackSource,
      error: null,
      usedFallback: true,
    };
  }

  try {
    // M14: jsonc-parser 的 parse 对非法 JSON 不抛异常而返回 undefined,使外层 try/catch 形同虚设,
    // 截断/损坏的 JSON 可能被恢复成残缺对象并恰好通过校验、静默使用错误内容。这里收集解析错误,
    // 有错误就明确按解析失败处理。
    const parseErrors: ParseError[] = [];
    const parsed = parse(trimmed, parseErrors, {
      allowTrailingComma: true,
      disallowComments: false,
    }) as unknown;
    if (parseErrors.length > 0) {
      const first = parseErrors[0];
      return {
        document: fallbackDocument,
        source: fallbackSource,
        error: `Invalid UI document JSON: ${printParseErrorCode(first.error)} at offset ${first.offset}.`,
        usedFallback: true,
      };
    }
    if (parsed === undefined) {
      return {
        document: fallbackDocument,
        source: fallbackSource,
        error: "Failed to parse UI document: empty or invalid JSON.",
        usedFallback: true,
      };
    }
    const schemaVersion = readRequestedSchemaVersion(parsed);
    if (schemaVersion === null) {
      const hasSchemaVersion = isPlainObject(parsed) && "schema_version" in parsed;
      return {
        document: fallbackDocument,
        source: fallbackSource,
        error: hasSchemaVersion
          ? "schema_version must be a number."
          : "schema_version is missing. UI documents must declare schema_version 2.",
        usedFallback: true,
      };
    }
    if (!isSupportedSchemaVersion(schemaVersion)) {
      return {
        document: fallbackDocument,
        source: fallbackSource,
        error: `Unsupported schema_version: ${String(schemaVersion)}.`,
        usedFallback: true,
      };
    }

    const validationError = validateGameUiDocumentV2(parsed);

    if (validationError) {
      return {
        document: fallbackDocument,
        source: fallbackSource,
        error: validationError,
        usedFallback: true,
      };
    }

    const document = normalizeGameUiDocumentV2(parsed);

    return {
      document,
      source,
      error: null,
      usedFallback: false,
    };
  } catch (error) {
    return {
      document: fallbackDocument,
      source: fallbackSource,
      error: error instanceof Error ? error.message : "Failed to parse UI document.",
      usedFallback: true,
    };
  }
}

function normalizeGameUiDocumentV2(raw: unknown): GameUiDocumentV2 {
  const value = isPlainObject(raw) ? raw : {};
  const layoutValue = isPlainObject(value.layout) ? value.layout : {};

  return {
    schema_version: 2,
    meta: normalizeMeta(value.meta),
    state: normalizePropRecord(value.state),
    layout: {
      root: normalizeLayoutNodeV2(layoutValue.root, EMPTY_V2_NODE),
    },
    mounts: normalizeMountOptions(value.mounts),
    components: normalizeComponents(value.components),
    tokens: normalizeTokens(value.tokens, {}),
    effects: normalizeMeta(value.effects),
    custom_css: typeof value.custom_css === "string" ? value.custom_css : "",
  };
}

function normalizeMeta(
  raw: unknown,
  fallback: Record<string, unknown> = {},
): Record<string, unknown> {
  if (!isPlainObject(raw)) {
    return structuredClone(fallback);
  }
  // L7: 深拷贝返回,避免与 fallback 分支(structuredClone)行为不一致;直接返回 raw 会让
  // 后续就地修改污染解析得到的源对象。
  return structuredClone(raw);
}

function normalizeLayoutNodeV2(raw: unknown, fallback: GameUiLayoutNodeV2): GameUiLayoutNodeV2 {
  const value = isPlainObject(raw) ? raw : {};
  const type = readOptionalString(value.type) ?? fallback.type;

  if (type === "component") {
    return {
      ...normalizeNodeBase(value),
      type: "component",
      component:
        readOptionalString(value.component)
        ?? (fallback.type === "component" ? fallback.component : "unknown_component"),
      variant: readOptionalString(value.variant),
      props: normalizePropRecord(value.props),
      slots: normalizeComponentSlots(value.slots),
      anchor: normalizeAnchor(value.anchor),
    };
  }

  if (type === "slot") {
    return {
      ...normalizeNodeBase(value),
      type: "slot",
      name: readOptionalString(value.name) ?? (fallback.type === "slot" ? fallback.name : "default"),
    };
  }

  if (type === "when") {
    return {
      ...normalizeNodeBase(value),
      type: "when",
      expr: readOptionalString(value.expr) ?? (fallback.type === "when" ? fallback.expr : "true"),
      child: normalizeLayoutNodeV2(
        value.child,
        fallback.type === "when" ? fallback.child : EMPTY_V2_NODE,
      ),
    };
  }

  if (type === "for_each") {
    return {
      ...normalizeNodeBase(value),
      type: "for_each",
      source:
        readOptionalString(value.source)
        ?? (fallback.type === "for_each" ? fallback.source : "$items"),
      item_as:
        readOptionalString(value.item_as)
        ?? (fallback.type === "for_each" ? fallback.item_as : "item"),
      index_as:
        readOptionalString(value.index_as)
        ?? (fallback.type === "for_each" ? fallback.index_as : undefined),
      empty:
        value.empty !== undefined
          ? normalizeLayoutNodeV2(
              value.empty,
              fallback.type === "for_each" && fallback.empty ? fallback.empty : EMPTY_V2_NODE,
            )
          : undefined,
      child: normalizeLayoutNodeV2(
        value.child,
        fallback.type === "for_each" ? fallback.child : EMPTY_V2_NODE,
      ),
    };
  }

  if (type === "text") {
    return {
      ...normalizeNodeBase(value),
      type: "text",
      text: readOptionalString(value.text) ?? (fallback.type === "text" ? fallback.text : ""),
      variant: readOptionalString(value.variant),
    };
  }

  if (type === "image") {
    const fit = readOptionalString(value.fit);
    return {
      ...normalizeNodeBase(value),
      type: "image",
      src: readOptionalString(value.src) ?? (fallback.type === "image" ? fallback.src : ""),
      alt: readOptionalString(value.alt),
      fit: fit === "cover" || fit === "contain" || fit === "fill" || fit === "none" || fit === "scale-down"
        ? fit
        : undefined,
    };
  }

  if (type === "badge") {
    return {
      ...normalizeNodeBase(value),
      type: "badge",
      text: readOptionalString(value.text) ?? (fallback.type === "badge" ? fallback.text : ""),
      variant: readOptionalString(value.variant),
    };
  }

  if (type === "button") {
    return {
      ...normalizeNodeBase(value),
      type: "button",
      label: readOptionalString(value.label) ?? (fallback.type === "button" ? fallback.label : ""),
      variant: readOptionalString(value.variant),
      disabled_when_empty_state: readOptionalString(value.disabled_when_empty_state),
      action: normalizeActionReference(value.action),
    };
  }

  if (type === "checkbox") {
    return {
      ...normalizeNodeBase(value),
      type: "checkbox",
      label: readOptionalString(value.label) ?? (fallback.type === "checkbox" ? fallback.label : ""),
      value: readOptionalString(value.value) ?? (fallback.type === "checkbox" ? fallback.value : ""),
      bind_checked_list:
        readOptionalString(value.bind_checked_list)
        ?? (fallback.type === "checkbox" ? fallback.bind_checked_list : ""),
      checked: typeof value.checked === "boolean" ? value.checked : undefined,
      disabled: typeof value.disabled === "boolean" ? value.disabled : undefined,
      variant: readOptionalString(value.variant),
    };
  }

  if (type === "absolute") {
    return {
      ...normalizeNodeBase(value),
      type: "absolute",
      children: normalizeChildrenV2(value.children),
    };
  }

  if (type === "stack") {
    return {
      ...normalizeNodeBase(value),
      type: "stack",
      direction: value.direction === "horizontal" ? "horizontal" : "vertical",
      wrap: typeof value.wrap === "boolean" ? value.wrap : false,
      gap: readOptionalString(value.gap),
      children: normalizeChildrenV2(value.children),
    };
  }

  return {
    ...normalizeNodeBase(value),
    type: "grid",
    columns: normalizeStringList(value.columns),
    rows: normalizeStringList(value.rows),
    areas: normalizeStringMatrix(value.areas),
    gap: readOptionalString(value.gap),
    children: normalizeChildrenV2(value.children),
  };
}

function normalizeChildrenV2(raw: unknown): GameUiLayoutNodeV2[] {
  if (!Array.isArray(raw)) {
    return [];
  }
  return raw.map((item) => normalizeLayoutNodeV2(item, EMPTY_V2_NODE));
}

function normalizeComponentSlots(
  raw: unknown,
): Record<string, GameUiLayoutNodeV2 | GameUiLayoutNodeV2[]> | undefined {
  if (!isPlainObject(raw)) {
    return undefined;
  }

  const entries: Array<[string, GameUiLayoutNodeV2 | GameUiLayoutNodeV2[]]> = [];
  for (const [key, value] of Object.entries(raw)) {
    if (!key.trim()) {
      continue;
    }
    if (Array.isArray(value)) {
      entries.push([key, value.map((item) => normalizeLayoutNodeV2(item, EMPTY_V2_NODE))]);
    } else if (isPlainObject(value)) {
      entries.push([key, normalizeLayoutNodeV2(value, EMPTY_V2_NODE)]);
    }
  }
  const slots = Object.fromEntries(entries);

  return Object.keys(slots).length > 0 ? slots : undefined;
}

function normalizePropRecord(raw: unknown): Record<string, GameUiPropValue> | undefined {
  if (!isPlainObject(raw)) {
    return undefined;
  }

  const props = Object.fromEntries(
    Object.entries(raw)
      .map(([key, value]) => {
        if (!key.trim()) {
          return null;
        }
        return [key, normalizePropValue(value)] as const;
      })
      .filter((entry): entry is [string, GameUiPropValue] => entry !== null),
  );

  return Object.keys(props).length > 0 ? props : undefined;
}

function normalizeActionReference(raw: unknown): GameUiActionReference | undefined {
  if (!isPlainObject(raw)) {
    return undefined;
  }

  const id = readOptionalString(raw.id);
  if (!id) {
    return undefined;
  }

  const args = isPlainObject(raw.args)
    ? Object.fromEntries(
        Object.entries(raw.args).map(([key, value]) => [key, normalizePropValue(value)]),
      )
    : undefined;

  return {
    id,
    args,
    content: typeof raw.content === "string" ? raw.content : undefined,
    content_template: typeof raw.content_template === "string" ? raw.content_template : undefined,
    mode: typeof raw.mode === "string" ? raw.mode : undefined,
  };
}

function normalizePropValue(raw: unknown): GameUiPropValue {
  if (raw === null || typeof raw === "string" || typeof raw === "number" || typeof raw === "boolean") {
    return raw;
  }

  if (Array.isArray(raw)) {
    return raw.map((item) => normalizePropValue(item));
  }

  if (isPlainObject(raw)) {
    return Object.fromEntries(
      Object.entries(raw).map(([key, value]) => [key, normalizePropValue(value)]),
    );
  }

  return null;
}

function normalizeNodeBase(value: Record<string, unknown>): GameUiNodeBase {
  return {
    id: readOptionalString(value.id),
    visible: typeof value.visible === "boolean" ? value.visible : true,
    class_name: readOptionalString(value.class_name),
    area: readOptionalString(value.area),
    width: readOptionalString(value.width),
    height: readOptionalString(value.height),
    min_width: readOptionalString(value.min_width),
    min_height: readOptionalString(value.min_height),
    max_width: readOptionalString(value.max_width),
    max_height: readOptionalString(value.max_height),
    padding: readOptionalString(value.padding),
    margin: readOptionalString(value.margin),
    align: readOptionalString(value.align),
    justify: readOptionalString(value.justify),
    style: normalizeStyleRecord(value.style),
  };
}

function normalizeAnchor(raw: unknown): GameUiAnchor | undefined {
  if (!isPlainObject(raw)) {
    return undefined;
  }

  const anchor: GameUiAnchor = {
    top: readOptionalString(raw.top),
    right: readOptionalString(raw.right),
    bottom: readOptionalString(raw.bottom),
    left: readOptionalString(raw.left),
  };

  return anchor.top || anchor.right || anchor.bottom || anchor.left ? anchor : undefined;
}

function normalizeMountOptions(raw: unknown): Partial<Record<GameUiMountId, GameUiMountOptions>> {
  if (!isPlainObject(raw)) {
    return {};
  }

  const next: Partial<Record<GameUiMountId, GameUiMountOptions>> = {};
  for (const [key, value] of Object.entries(raw)) {
    if (!isMountId(key) || !isPlainObject(value)) {
      continue;
    }

    next[key] = {
      visible: typeof value.visible === "boolean" ? value.visible : true,
      variant: readOptionalString(value.variant),
      chrome: readOptionalString(value.chrome),
      class_name: readOptionalString(value.class_name),
      max_width: readOptionalString(value.max_width),
      min_height: readOptionalString(value.min_height),
      sticky: readOptionalString(value.sticky),
      tab_order: Array.isArray(value.tab_order)
        ? value.tab_order.map((entry) => String(entry).trim()).filter(Boolean)
        : undefined,
      style: normalizeStyleRecord(value.style),
    };
  }

  return next;
}

function normalizeComponents(
  raw: unknown,
  fallback: Record<string, GameUiComponentStyleDefinition> = {},
): Record<string, GameUiComponentStyleDefinition> {
  if (!isPlainObject(raw)) {
    return structuredClone(fallback);
  }

  const components: Record<string, GameUiComponentStyleDefinition> = {};
  for (const [key, value] of Object.entries(raw)) {
    if (!isPlainObject(value)) {
      continue;
    }

    const variants = isPlainObject(value.variants)
      ? Object.fromEntries(
          Object.entries(value.variants)
            .filter(([, variantValue]) => isPlainObject(variantValue))
            .map(([variantKey, variantValue]) => [variantKey, normalizeStyleRecord(variantValue) ?? {}]),
        )
      : undefined;

    components[key] = {
      base: normalizeStyleRecord(value.base),
      variants: variants && Object.keys(variants).length > 0 ? variants : undefined,
    };
  }

  return components;
}

function normalizeTokens(raw: unknown, fallback: Record<string, string>): Record<string, string> {
  if (!isPlainObject(raw)) {
    return { ...fallback };
  }

  return {
    ...fallback,
    ...Object.fromEntries(
      Object.entries(raw)
        .map(([key, value]) => [key, typeof value === "string" ? value : String(value ?? "")])
        .filter(([key, value]) => key.trim() && value.trim()),
    ),
  };
}

function normalizeStyleRecord(raw: unknown): GameUiStyleRecord | undefined {
  if (!isPlainObject(raw)) {
    return undefined;
  }

  const style = Object.fromEntries(
    Object.entries(raw).filter(([, value]) => isStyleValue(value)),
  ) as GameUiStyleRecord;

  return Object.keys(style).length > 0 ? style : undefined;
}

function normalizeStringList(raw: unknown): string[] | undefined {
  if (!Array.isArray(raw)) {
    return undefined;
  }
  return raw.map((item) => String(item));
}

function normalizeStringMatrix(raw: unknown): string[][] | undefined {
  if (!Array.isArray(raw)) {
    return undefined;
  }

  return raw.map((row) => (Array.isArray(row) ? row.map((item) => String(item)) : []));
}

function readOptionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value : undefined;
}

function readRequestedSchemaVersion(raw: unknown): number | null {
  if (!isPlainObject(raw) || typeof raw.schema_version !== "number") {
    return null;
  }
  return raw.schema_version;
}

function isMountId(value: unknown): value is GameUiMountId {
  return typeof value === "string" && GAME_UI_MOUNT_IDS.includes(value as GameUiMountId);
}

function isSupportedSchemaVersion(value: number): value is GameUiDocument["schema_version"] {
  return SUPPORTED_GAME_UI_SCHEMA_VERSIONS.includes(
    value as (typeof SUPPORTED_GAME_UI_SCHEMA_VERSIONS)[number],
  );
}

function validateGameUiDocumentV2(raw: unknown): string | null {
  if (!isPlainObject(raw)) {
    return "UI document root must be an object.";
  }

  if (raw.schema_version !== 2) {
    return "schema_version must be 2 for a v2 UI document.";
  }

  const layout = validateLayoutRoot(raw.layout);
  if (layout.error) {
    return layout.error;
  }

  const layoutError = validateLayoutNodeV2(layout.root, "layout.root");
  if (layoutError) {
    return layoutError;
  }

  return validateCommonDocumentFields(raw);
}

function validateLayoutRoot(layoutValue: unknown): { root: unknown; error: string | null } {
  if (!isPlainObject(layoutValue)) {
    return { root: null, error: "layout must be an object." };
  }

  if (!isPlainObject(layoutValue.root)) {
    return { root: null, error: "layout.root must be an object." };
  }

  return { root: layoutValue.root, error: null };
}

function validateCommonDocumentFields(value: Record<string, unknown>): string | null {
  if (value.meta !== undefined && !isPlainObject(value.meta)) {
    return "meta must be an object.";
  }

  if (value.mounts !== undefined) {
    const mountsError = validateMountOptionsRecord(value.mounts, "mounts");
    if (mountsError) {
      return mountsError;
    }
  }

  if (value.components !== undefined) {
    const componentsError = validateComponentDefinitions(value.components, "components");
    if (componentsError) {
      return componentsError;
    }
  }

  if (value.tokens !== undefined) {
    const tokensError = validateTokenRecord(value.tokens, "tokens");
    if (tokensError) {
      return tokensError;
    }
  }

  if (value.effects !== undefined && !isPlainObject(value.effects)) {
    return "effects must be an object.";
  }

  if (value.custom_css !== undefined && typeof value.custom_css !== "string") {
    return "custom_css must be a string.";
  }

  if (value.state !== undefined) {
    if (!isPlainObject(value.state)) {
      return "state must be an object.";
    }
    for (const [stateKey, stateValue] of Object.entries(value.state)) {
      const propError = validatePropValue(stateValue, `state.${stateKey}`);
      if (propError) {
        return propError;
      }
    }
  }

  return null;
}

function validateLayoutNodeV2(raw: unknown, path: string): string | null {
  if (!isPlainObject(raw)) {
    return `${path} must be an object.`;
  }

  const type = raw.type;
  if (
    type !== "grid"
    && type !== "stack"
    && type !== "absolute"
    && type !== "component"
    && type !== "slot"
    && type !== "when"
    && type !== "for_each"
    && type !== "text"
    && type !== "image"
    && type !== "badge"
    && type !== "button"
    && type !== "checkbox"
  ) {
    return `${path}.type must be one of grid / stack / absolute / component / slot / when / for_each / text / image / badge / button / checkbox.`;
  }

  const sharedFieldError = validateSharedNodeFields(raw, path);
  if (sharedFieldError) {
    return sharedFieldError;
  }

  if (type === "grid" || type === "stack" || type === "absolute") {
    if (raw.children !== undefined && !Array.isArray(raw.children)) {
      return `${path}.children must be an array.`;
    }

    if (type === "grid") {
      if (raw.columns !== undefined && !isStringLikeArray(raw.columns)) {
        return `${path}.columns must be an array of strings or numbers.`;
      }
      if (raw.rows !== undefined && !isStringLikeArray(raw.rows)) {
        return `${path}.rows must be an array of strings or numbers.`;
      }
      if (raw.areas !== undefined) {
        if (!Array.isArray(raw.areas) || !raw.areas.every((row) => isStringLikeArray(row))) {
          return `${path}.areas must be a two-dimensional string array.`;
        }
        const rowLengths = raw.areas.map((row) => row.length);
        if (rowLengths.length > 1 && rowLengths.some((length) => length !== rowLengths[0])) {
          return `${path}.areas rows must all have the same length.`;
        }
      }
    }

    if (type === "stack" && raw.direction !== undefined && raw.direction !== "vertical" && raw.direction !== "horizontal") {
      return `${path}.direction must be vertical or horizontal.`;
    }

    const children = raw.children ?? [];
    for (let index = 0; index < children.length; index += 1) {
      const childError = validateLayoutNodeV2(children[index], `${path}.children[${index}]`);
      if (childError) {
        return childError;
      }
    }

    return null;
  }

  if (type === "component") {
    if (typeof raw.component !== "string" || !raw.component.trim()) {
      return `${path}.component must be a non-empty string.`;
    }
    if (raw.variant !== undefined && typeof raw.variant !== "string") {
      return `${path}.variant must be a string.`;
    }

    const anchorError = validateAnchor(raw.anchor, `${path}.anchor`);
    if (anchorError) {
      return anchorError;
    }

    if (raw.props !== undefined) {
      if (!isPlainObject(raw.props)) {
        return `${path}.props must be an object.`;
      }
      for (const [propKey, propValue] of Object.entries(raw.props)) {
        const propError = validatePropValue(propValue, `${path}.props.${propKey}`);
        if (propError) {
          return propError;
        }
      }
    }

    if (raw.slots !== undefined) {
      if (!isPlainObject(raw.slots)) {
        return `${path}.slots must be an object.`;
      }
      for (const [slotKey, slotValue] of Object.entries(raw.slots)) {
        if (Array.isArray(slotValue)) {
          for (let index = 0; index < slotValue.length; index += 1) {
            const slotError = validateLayoutNodeV2(
              slotValue[index],
              `${path}.slots.${slotKey}[${index}]`,
            );
            if (slotError) {
              return slotError;
            }
          }
          continue;
        }
        const slotError = validateLayoutNodeV2(slotValue, `${path}.slots.${slotKey}`);
        if (slotError) {
          return slotError;
        }
      }
    }

    return null;
  }

  if (type === "slot") {
    if (typeof raw.name !== "string" || !raw.name.trim()) {
      return `${path}.name must be a non-empty string.`;
    }
    return null;
  }

  if (type === "when") {
    if (typeof raw.expr !== "string" || !raw.expr.trim()) {
      return `${path}.expr must be a non-empty string.`;
    }
    return validateLayoutNodeV2(raw.child, `${path}.child`);
  }

  if (type === "text" || type === "badge") {
    if (typeof raw.text !== "string") {
      return `${path}.text must be a string.`;
    }
    if (raw.variant !== undefined && typeof raw.variant !== "string") {
      return `${path}.variant must be a string.`;
    }
    return null;
  }

  if (type === "image") {
    if (typeof raw.src !== "string" || !raw.src.trim()) {
      return `${path}.src must be a non-empty string.`;
    }
    if (raw.alt !== undefined && typeof raw.alt !== "string") {
      return `${path}.alt must be a string.`;
    }
    if (
      raw.fit !== undefined
      && raw.fit !== "cover"
      && raw.fit !== "contain"
      && raw.fit !== "fill"
      && raw.fit !== "none"
      && raw.fit !== "scale-down"
    ) {
      return `${path}.fit must be cover / contain / fill / none / scale-down.`;
    }
    return null;
  }

  if (type === "button") {
    if (typeof raw.label !== "string") {
      return `${path}.label must be a string.`;
    }
    if (raw.variant !== undefined && typeof raw.variant !== "string") {
      return `${path}.variant must be a string.`;
    }
    if (raw.disabled_when_empty_state !== undefined && typeof raw.disabled_when_empty_state !== "string") {
      return `${path}.disabled_when_empty_state must be a string.`;
    }
    if (raw.action !== undefined) {
      const actionError = validateActionReference(raw.action, `${path}.action`);
      if (actionError) {
        return actionError;
      }
    }
    return null;
  }

  if (type === "checkbox") {
    if (typeof raw.label !== "string") {
      return `${path}.label must be a string.`;
    }
    if (typeof raw.value !== "string") {
      return `${path}.value must be a string.`;
    }
    if (typeof raw.bind_checked_list !== "string" || !raw.bind_checked_list.trim()) {
      return `${path}.bind_checked_list must be a non-empty string.`;
    }
    if (raw.checked !== undefined && typeof raw.checked !== "boolean") {
      return `${path}.checked must be a boolean.`;
    }
    if (raw.disabled !== undefined && typeof raw.disabled !== "boolean") {
      return `${path}.disabled must be a boolean.`;
    }
    if (raw.variant !== undefined && typeof raw.variant !== "string") {
      return `${path}.variant must be a string.`;
    }
    return null;
  }

  if (typeof raw.source !== "string" || !raw.source.trim()) {
    return `${path}.source must be a non-empty string.`;
  }
  if (typeof raw.item_as !== "string" || !raw.item_as.trim()) {
    return `${path}.item_as must be a non-empty string.`;
  }
  if (raw.index_as !== undefined && typeof raw.index_as !== "string") {
    return `${path}.index_as must be a string.`;
  }
  if (raw.empty !== undefined) {
    const emptyError = validateLayoutNodeV2(raw.empty, `${path}.empty`);
    if (emptyError) {
      return emptyError;
    }
  }
  return validateLayoutNodeV2(raw.child, `${path}.child`);
}

function validateSharedNodeFields(value: Record<string, unknown>, path: string): string | null {
  const stringFields = [
    "id",
    "class_name",
    "area",
    "width",
    "height",
    "min_width",
    "min_height",
    "max_width",
    "max_height",
    "padding",
    "margin",
    "align",
    "justify",
    "gap",
  ] as const;

  if (value.visible !== undefined && typeof value.visible !== "boolean") {
    return `${path}.visible must be a boolean.`;
  }

  for (const field of stringFields) {
    if (value[field] !== undefined && typeof value[field] !== "string") {
      return `${path}.${field} must be a string.`;
    }
  }

  if (value.wrap !== undefined && typeof value.wrap !== "boolean") {
    return `${path}.wrap must be a boolean.`;
  }

  if (value.style !== undefined) {
    const styleError = validateStyleRecord(value.style, `${path}.style`);
    if (styleError) {
      return styleError;
    }
  }

  return null;
}

function validateAnchor(raw: unknown, path: string): string | null {
  if (raw === undefined) {
    return null;
  }
  if (!isPlainObject(raw)) {
    return `${path} must be an object.`;
  }

  for (const key of ["top", "right", "bottom", "left"] as const) {
    if (raw[key] !== undefined && typeof raw[key] !== "string") {
      return `${path}.${key} must be a string.`;
    }
  }

  return null;
}

function validateMountOptionsRecord(raw: unknown, path: string): string | null {
  if (!isPlainObject(raw)) {
    return `${path} must be an object.`;
  }

  for (const [mountKey, mountValue] of Object.entries(raw)) {
    if (!isMountId(mountKey)) {
      return `Unknown mount id at ${path}.${mountKey}.`;
    }
    if (!isPlainObject(mountValue)) {
      return `${path}.${mountKey} must be an object.`;
    }

    if (mountValue.visible !== undefined && typeof mountValue.visible !== "boolean") {
      return `${path}.${mountKey}.visible must be a boolean.`;
    }

    for (const field of ["variant", "chrome", "class_name", "max_width", "min_height", "sticky"] as const) {
      if (mountValue[field] !== undefined && typeof mountValue[field] !== "string") {
        return `${path}.${mountKey}.${field} must be a string.`;
      }
    }

    if (mountValue.tab_order !== undefined && !isStringArray(mountValue.tab_order)) {
      return `${path}.${mountKey}.tab_order must be an array of strings.`;
    }

    if (mountValue.style !== undefined) {
      const styleError = validateStyleRecord(mountValue.style, `${path}.${mountKey}.style`);
      if (styleError) {
        return styleError;
      }
    }
  }

  return null;
}

function validateComponentDefinitions(raw: unknown, path: string): string | null {
  if (!isPlainObject(raw)) {
    return `${path} must be an object.`;
  }

  for (const [componentKey, componentValue] of Object.entries(raw)) {
    if (!isPlainObject(componentValue)) {
      return `${path}.${componentKey} must be an object.`;
    }

    if (componentValue.base !== undefined) {
      const styleError = validateStyleRecord(componentValue.base, `${path}.${componentKey}.base`);
      if (styleError) {
        return styleError;
      }
    }

    if (componentValue.variants !== undefined) {
      if (!isPlainObject(componentValue.variants)) {
        return `${path}.${componentKey}.variants must be an object.`;
      }
      for (const [variantKey, variantValue] of Object.entries(componentValue.variants)) {
        const styleError = validateStyleRecord(
          variantValue,
          `${path}.${componentKey}.variants.${variantKey}`,
        );
        if (styleError) {
          return styleError;
        }
      }
    }
  }

  return null;
}

function validateTokenRecord(raw: unknown, path: string): string | null {
  if (!isPlainObject(raw)) {
    return `${path} must be an object.`;
  }

  for (const [tokenKey, tokenValue] of Object.entries(raw)) {
    if (!tokenKey.trim()) {
      return `${path} does not allow empty keys.`;
    }
    if (typeof tokenValue !== "string" && typeof tokenValue !== "number") {
      return `${path}.${tokenKey} must be a string or number.`;
    }
  }

  return null;
}

function validateStyleRecord(raw: unknown, path: string): string | null {
  if (!isPlainObject(raw)) {
    return `${path} must be an object.`;
  }

  for (const [styleKey, styleValue] of Object.entries(raw)) {
    if (!isStyleValue(styleValue)) {
      return `${path}.${styleKey} must be a string, number, boolean, or null.`;
    }
  }

  return null;
}

function validatePropValue(raw: unknown, path: string): string | null {
  if (raw === null || typeof raw === "string" || typeof raw === "number" || typeof raw === "boolean") {
    return null;
  }

  if (Array.isArray(raw)) {
    for (let index = 0; index < raw.length; index += 1) {
      const childError = validatePropValue(raw[index], `${path}[${index}]`);
      if (childError) {
        return childError;
      }
    }
    return null;
  }

  if (!isPlainObject(raw)) {
    return `${path} must be a primitive, array, or object.`;
  }

  for (const [key, value] of Object.entries(raw)) {
    const childError = validatePropValue(value, `${path}.${key}`);
    if (childError) {
      return childError;
    }
  }

  return null;
}

function validateActionReference(raw: unknown, path: string): string | null {
  if (!isPlainObject(raw)) {
    return `${path} must be an object.`;
  }

  if (typeof raw.id !== "string" || !raw.id.trim()) {
    return `${path}.id must be a non-empty string.`;
  }

  if (raw.args !== undefined) {
    if (!isPlainObject(raw.args)) {
      return `${path}.args must be an object.`;
    }
    for (const [argKey, argValue] of Object.entries(raw.args)) {
      const propError = validatePropValue(argValue, `${path}.args.${argKey}`);
      if (propError) {
        return propError;
      }
    }
  }

  if (raw.content !== undefined && typeof raw.content !== "string") {
    return `${path}.content must be a string.`;
  }

  if (raw.content_template !== undefined && typeof raw.content_template !== "string") {
    return `${path}.content_template must be a string.`;
  }

  if (raw.mode !== undefined && typeof raw.mode !== "string") {
    return `${path}.mode must be a string.`;
  }

  return null;
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function isStringLikeArray(value: unknown): value is Array<string | number> {
  return Array.isArray(value) && value.every((item) => typeof item === "string" || typeof item === "number");
}

function isStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((item) => typeof item === "string");
}

function isStyleValue(value: unknown): value is string | number | boolean | null {
  return value === null || typeof value === "string" || typeof value === "number" || typeof value === "boolean";
}

export function buildGameUiStylesheet(
  document: GameUiDocument,
  runtimeBackgroundImage?: string,
  scopeSelector = ".game-ui-root",
): string {
  const tokens = document.tokens ?? {};
  const tokenLines = Object.entries(tokens)
    .map(([key, value]) => `--game-ui-token-${sanitizeTokenKey(key)}: ${value};`)
    .join("\n");

  const bgColor = typeof tokens["color.bg"] === "string" ? tokens["color.bg"].trim() : "";
  const bgMapping = bgColor ? `\n--game-bg-to: ${bgColor};\n--game-bg-from: ${bgColor};\n--game-bg-via: ${bgColor};` : "";

  const legacyVarMap: Record<string, string> = {
    "color.text": "--game-text",
    "color.text-dim": "--game-text-dim",
    "color.text-muted": "--game-text-muted",
    "color.panel": "--game-bg-panel",
    "color.input": "--game-bg-input",
    "color.border": "--game-border",
    "color.player": "--game-player-bg",
    "color.speaker-blue": "--game-speaker-blue",
  };
  const legacyLines = Object.entries(legacyVarMap)
    .map(([tokenKey, cssVar]) => {
      const val = tokens[tokenKey];
      return typeof val === "string" ? `${cssVar}: ${val};` : "";
    })
    .filter(Boolean)
    .join("\n");
  const legacyMapping = legacyLines ? `\n${legacyLines}` : "";

  const cssBlocks: string[] = [
    `${scopeSelector} {\nposition: relative;\n${tokenLines}${bgMapping}${legacyMapping}\n}`,
    `${scopeSelector}.game-root {\ndisplay: flex;\nwidth: 100%;\nmin-width: 0;\nmin-height: 0;\nflex-direction: column;\ncolor: var(--game-ui-token-color-text, var(--game-text, inherit));\noverflow: hidden;\n}`,
    `${scopeSelector}.game-root--session {\nmin-height: 100vh;\nmin-height: 100dvh;\nheight: 100vh;\nheight: 100dvh;\n}`,
    `${scopeSelector}.game-root--preview {\nmin-height: 0;\nheight: 100%;\n}`,
    `${scopeSelector} .game-ui-layout {\nposition: relative;\nwidth: 100%;\nheight: 100%;\nmin-width: 0;\nmin-height: 0;\n}`,
    `${scopeSelector} .game-ui-node,\n${scopeSelector} .game-ui-mount,\n${scopeSelector} .game-ui-component,\n${scopeSelector} .game-ui-slot {\nbox-sizing: border-box;\nmin-width: 0;\nmin-height: 0;\n}`,
    `${scopeSelector} .game-typing-bubble {\nmax-width: 80px;\npadding: 12px 16px;\n}\n${scopeSelector} .game-typing-dots {\ndisplay: flex;\ngap: 4px;\nalign-items: center;\njustify-content: center;\nheight: 20px;\n}\n${scopeSelector} .game-typing-dot {\nwidth: 6px;\nheight: 6px;\nborder-radius: 50%;\nbackground: var(--game-ui-token-color-text-muted, var(--game-text-muted, rgba(255,255,255,0.4)));\nanimation: game-typing-bounce 1.4s ease-in-out infinite;\n}\n${scopeSelector} .game-typing-dot:nth-child(2) {\nanimation-delay: 0.2s;\n}\n${scopeSelector} .game-typing-dot:nth-child(3) {\nanimation-delay: 0.4s;\n}\n${scopeSelector} .game-message-row--typing {\nanimation: game-ui-typing-enter 200ms ease-out;\n}\n@keyframes game-typing-bounce {\n0%, 60%, 100% { transform: translateY(0); opacity: 0.4; }\n30% { transform: translateY(-4px); opacity: 1; }\n}\n@keyframes game-ui-typing-enter {\nfrom { opacity: 0; transform: translateY(6px); }\nto { opacity: 1; transform: translateY(0); }\n}`,
    `${scopeSelector} .game-mobile-error-notice {\ndisplay: flex;\nflex-direction: column;\ngap: 6px;\nwidth: 100%;\npadding: 12px 14px;\nborder-radius: var(--game-ui-token-radius-md, 14px);\nborder: 1px solid color-mix(in srgb, #dc2626 38%, transparent);\nbackground: color-mix(in srgb, #dc2626 12%, var(--game-ui-token-color-panel, rgba(255,255,255,0.06)));\ncolor: var(--game-ui-token-color-text, currentColor);\n}\n${scopeSelector} .game-mobile-error-title {\nfont-size: 13px;\nfont-weight: 700;\ncolor: color-mix(in srgb, #dc2626 72%, var(--game-ui-token-color-text, currentColor));\n}\n${scopeSelector} .game-mobile-error-summary {\nfont-size: 13px;\nline-height: 1.5;\ncolor: var(--game-ui-token-color-text-dim, var(--game-text-muted, inherit));\nwhite-space: pre-wrap;\n}\n${scopeSelector} .game-mobile-error-retry {\nalign-self: flex-start;\nmin-height: 34px;\nmargin-top: 2px;\npadding: 0 16px;\nborder-radius: 999px;\nborder: 0;\nbackground: #dc2626;\ncolor: #ffffff;\nfont-size: 13px;\nfont-weight: 600;\n}\n${scopeSelector} .game-mobile-error-retry:disabled {\nopacity: 0.6;\n}`,
    `${scopeSelector} .game-map-graph {\ndisplay: flex;\nmin-height: 0;\nheight: 100%;\nflex-direction: column;\ngap: 12px;\n}\n${scopeSelector} .game-map-flow-canvas {\nposition: relative;\nmin-height: 260px;\nheight: 100%;\noverflow: hidden;\nborder: 1px solid var(--game-border, rgba(255,255,255,0.16));\nborder-radius: var(--game-ui-token-radius-lg, 16px);\nbackground: color-mix(in srgb, var(--game-ui-token-color-panel, rgba(255,255,255,0.08)) 48%, transparent);\n}\n${scopeSelector} .game-map-flow-canvas .react-flow {\nfont-family: var(--game-ui-token-font-body, inherit);\n}\n${scopeSelector} .game-map-flow-background {\nopacity: 0.45;\n}\n${scopeSelector} .game-map-flow-node {\nwidth: 150px;\npadding: 8px 10px;\nborder: 1px solid var(--game-border, rgba(255,255,255,0.16));\nborder-radius: var(--game-ui-token-radius-md, 12px);\nbackground: var(--game-ui-token-color-panel, rgba(255,255,255,0.10));\ncolor: var(--game-ui-token-color-text, currentColor);\nfont-size: 12px;\nfont-weight: 700;\nline-height: 1.25;\ntext-align: center;\nbox-shadow: 0 10px 24px rgba(0,0,0,0.14);\n}\n${scopeSelector} .game-map-flow-node--current {\nborder-color: var(--game-ui-token-color-primary, var(--game-accent, currentColor));\nbackground: color-mix(in srgb, var(--game-ui-token-color-primary, var(--game-accent, currentColor)) 22%, var(--game-ui-token-color-panel, rgba(255,255,255,0.12)));\n}\n${scopeSelector} .game-map-flow-node--undiscovered {\nopacity: 0.58;\nfilter: saturate(0.74);\n}\n${scopeSelector} .game-map-flow-edge path {\nstroke: var(--game-border, rgba(255,255,255,0.22));\nstroke-width: 1.6;\n}\n${scopeSelector} .game-map-flow-edge--current path {\nstroke: var(--game-ui-token-color-primary, var(--game-accent, #dbeafe));\nstroke-width: 2.2;\n}\n${scopeSelector} .react-flow__controls {\nbox-shadow: 0 10px 24px rgba(0,0,0,0.18);\n}\n${scopeSelector} .react-flow__controls-button {\nbackground: var(--game-ui-token-color-panel, rgba(255,255,255,0.12));\nborder-color: var(--game-border, rgba(255,255,255,0.14));\ncolor: var(--game-ui-token-color-text, currentColor);\n}`,
    // Base sizing for scene-focus character portraits. Theme custom_css can
    // override these via more specific selectors; without this baseline a theme
    // that only restyles (e.g. sets a filter on) .game-avatar-image leaves the
    // <img> unsized, so the portrait never shows.
    `${scopeSelector} .game-avatar {\ndisplay: flex;\nalign-items: center;\njustify-content: center;\nwidth: 256px;\nheight: 320px;\nmax-width: 100%;\noverflow: hidden;\n}\n${scopeSelector} .game-avatar-image {\nwidth: 100%;\nheight: 100%;\nobject-fit: cover;\n}`,
    // Base styling for director-trace / chain-of-thought labels and lines.
    // These read as quiet grey captions on every platform and world. Desktop
    // seed UIs spell this out in their custom_css, but mobile seeds don't — so
    // without a baseline the labels fall back to default black body text on
    // mobile. Worlds can still override via more specific custom_css selectors.
    `${scopeSelector} .game-director-trace-title,\n${scopeSelector} .game-director-trace-label,\n${scopeSelector} .game-director-trace-line,\n${scopeSelector} .game-agent-answer-label {\nfont-size: 12px;\nfont-weight: 400;\nline-height: 1.55;\ncolor: var(--game-ui-token-color-text-muted, var(--game-text-muted, rgba(120,130,140,0.85)));\nopacity: 0.7;\n}\n${scopeSelector} .game-cot-label {\nfont-size: 12px;\nfont-weight: 600;\ncolor: var(--game-ui-token-color-text-muted, var(--game-text-muted, rgba(120,130,140,0.85)));\nopacity: 0.7;\n}\n${scopeSelector} .game-cot-content {\nfont-size: 12px;\nline-height: 1.55;\ncolor: var(--game-ui-token-color-text-muted, var(--game-text-muted, rgba(120,130,140,0.85)));\nopacity: 0.85;\nwhite-space: pre-wrap;\nword-break: break-word;\n}`,
  ];

  const background = (document.effects?.background ?? {}) as Record<string, unknown>;
  const authoredBackgroundImage = typeof background.image === "string" ? background.image.trim() : "";
  const runtimeBackgroundLayer = runtimeBackgroundImage ? `url("${runtimeBackgroundImage}")` : "";
  const backgroundImages = [runtimeBackgroundLayer, authoredBackgroundImage].filter(Boolean);
  const backgroundOverlay = typeof background.overlay === "string" ? background.overlay : "";

  if (backgroundImages.length > 0 || backgroundOverlay) {
    cssBlocks.push(`${scopeSelector}::before {
  content: "";
  position: absolute;
  inset: 0;
  pointer-events: none;
  z-index: 0;
  background-image: ${[backgroundOverlay, ...backgroundImages].filter(Boolean).join(", ")};
  background-size: ${typeof background.size === "string" ? background.size : "cover"};
  background-position: ${typeof background.position === "string" ? background.position : "center"};
  background-repeat: no-repeat;
  opacity: 1;
}`);
    cssBlocks.push(`${scopeSelector} > * { position: relative; z-index: 1; }`);
  }

  const pageEnter = (document.effects?.page_enter ?? {}) as Record<string, unknown>;
  if (pageEnter.enabled) {
    const duration = typeof pageEnter.duration === "string" ? pageEnter.duration : "220ms";
    const easing = typeof pageEnter.easing === "string" ? pageEnter.easing : "ease-out";
    cssBlocks.push(`${scopeSelector} {
  animation: game-ui-page-enter ${duration} ${easing};
}
@keyframes game-ui-page-enter {
  from { opacity: 0; transform: translateY(10px); }
  to { opacity: 1; transform: translateY(0); }
}`);
  }

  const messageReveal = (document.effects?.message_reveal ?? {}) as Record<string, unknown>;
  if (messageReveal.enabled) {
    const duration = typeof messageReveal.duration === "string" ? messageReveal.duration : "180ms";
    cssBlocks.push(`${scopeSelector} .game-message-row {
  animation: game-ui-message-enter ${duration} ease-out;
}
@keyframes game-ui-message-enter {
  from { opacity: 0; transform: translateY(6px); }
  to { opacity: 1; transform: translateY(0); }
}`);
  }

  const componentSelectors: Record<string, string> = {
    panel: ".game-ui-panel",
    button: ".game-ui-button",
    chip: ".game-ui-chip",
    message_bubble: ".game-ui-message-bubble",
    message_speaker: ".game-ui-message-speaker",
    input: ".game-ui-input",
    textarea: ".game-ui-textarea",
    badge: ".game-ui-badge",
    avatar: ".game-ui-avatar",
  };

  for (const [componentKey, definition] of Object.entries(document.components ?? {})) {
    const selector = componentSelectors[componentKey];
    if (!selector) {
      continue;
    }
    if (definition.base) {
      cssBlocks.push(`${scopeSelector} ${selector} {\n${styleRecordToCss(definition.base)}\n}`);
    }
    for (const [variantKey, variantStyles] of Object.entries(definition.variants ?? {})) {
      cssBlocks.push(`${scopeSelector} ${selector}[data-variant="${variantKey}"] {\n${styleRecordToCss(variantStyles)}\n}`);
    }
  }

  for (const [mountKey, mountValue] of Object.entries(document.mounts ?? {})) {
    if (!mountValue) {
      continue;
    }

    const mountSelector = `${scopeSelector} .game-ui-mount[data-mount="${mountKey}"]`;
    if (mountValue.style) {
      cssBlocks.push(`${mountSelector} {\n${styleRecordToCss(mountValue.style)}\n}`);
    }
    if (mountValue.max_width || mountValue.min_height || mountValue.sticky) {
      const stickyRules = mountValue.sticky === "bottom"
        ? "position: sticky;\nbottom: 0;"
        : "";
      cssBlocks.push(`${mountSelector} {\n${[
        mountValue.max_width ? `max-width: ${mountValue.max_width};` : "",
        mountValue.min_height ? `min-height: ${mountValue.min_height};` : "",
        stickyRules,
      ].filter(Boolean).join("\n")}\n}`);
    }
  }

  if (document.custom_css?.trim()) {
    cssBlocks.push(scopeCustomCss(document.custom_css.trim(), scopeSelector));
  }

  return cssBlocks.filter(Boolean).join("\n\n");
}

export function normalizeGameUiScopeId(scopeId: string): string {
  const normalized = scopeId.trim().replace(/[^a-zA-Z0-9_-]+/g, "-");
  return normalized || "game-ui";
}

export function createGameUiScopeSelector(scopeId: string): string {
  return `[data-game-ui-scope="${normalizeGameUiScopeId(scopeId)}"]`;
}

export function resolveSidePanelTabOrder<T extends { key: string }>(
  document: GameUiDocument,
  availableTabs: T[],
): T[] {
  const configuredOrder = document.mounts?.side_panel?.tab_order;
  if (!configuredOrder) {
    return [...availableTabs];
  }

  const byKey = new Map(availableTabs.map((tab) => [tab.key, tab] as const));
  const configuredTabs = configuredOrder
    .map((key) => byKey.get(key) ?? null)
    .filter((tab): tab is T => tab !== null);
  const configuredKeys = new Set(configuredTabs.map((tab) => tab.key));
  const unconfiguredTabs = availableTabs.filter((tab) => !configuredKeys.has(tab.key));
  return [...configuredTabs, ...unconfiguredTabs];
}

function styleRecordToCss(style: GameUiStyleRecord): string {
  return Object.entries(style)
    .map(([key, value]) => {
      const cssKey = normalizeCssPropertyKey(key);
      if (typeof value === "boolean") {
        return `${cssKey}: ${value ? "true" : "false"};`;
      }
      return `${cssKey}: ${String(value)};`;
    })
    .join("\n");
}

function sanitizeTokenKey(key: string): string {
  return key.replace(/[^a-zA-Z0-9_-]+/g, "-");
}

function scopeCustomCss(css: string, scopeSelector: string): string {
  return scopeCssBlock(css, scopeSelector);
}

function scopeCssBlock(css: string, scopeSelector: string): string {
  let cursor = 0;
  let output = "";

  while (cursor < css.length) {
    const token = findNextTopLevelToken(css, cursor);
    if (!token) {
      output += css.slice(cursor);
      break;
    }

    if (token.type === "semicolon") {
      output += css.slice(cursor, token.index + 1);
      cursor = token.index + 1;
      continue;
    }

    const prelude = css.slice(cursor, token.index);
    const block = readCssBlock(css, token.index);
    if (!block) {
      output += css.slice(cursor);
      break;
    }

    const trimmedPrelude = prelude.trim();
    if (!trimmedPrelude) {
      output += `${prelude}{${scopeCssBlock(block.body, scopeSelector)}}`;
      cursor = block.end;
      continue;
    }

    if (trimmedPrelude.startsWith("@")) {
      const atRuleName = trimmedPrelude.slice(1).match(/^[a-zA-Z-]+/)?.[0]?.toLowerCase() ?? "";
      if (shouldScopeNestedAtRule(atRuleName)) {
        output += `${prelude}{${scopeCssBlock(block.body, scopeSelector)}}`;
      } else {
        output += `${prelude}{${block.body}}`;
      }
      cursor = block.end;
      continue;
    }

    output += `${scopeSelectorList(prelude, scopeSelector)}{${block.body}}`;
    cursor = block.end;
  }

  return output;
}

function shouldScopeNestedAtRule(atRuleName: string): boolean {
  return atRuleName === "media"
    || atRuleName === "supports"
    || atRuleName === "container"
    || atRuleName === "layer"
    || atRuleName === "scope"
    || atRuleName === "starting-style";
}

function findNextTopLevelToken(
  css: string,
  start: number,
): { type: "brace" | "semicolon"; index: number } | null {
  let depthParen = 0;
  let depthBracket = 0;

  for (let index = start; index < css.length; index += 1) {
    const char = css[index];
    const next = css[index + 1];

    if (char === "\"" || char === "'") {
      index = skipCssString(css, index, char);
      continue;
    }

    if (char === "/" && next === "*") {
      index = skipCssComment(css, index);
      continue;
    }

    if (char === "(") {
      depthParen += 1;
      continue;
    }
    if (char === ")" && depthParen > 0) {
      depthParen -= 1;
      continue;
    }
    if (char === "[") {
      depthBracket += 1;
      continue;
    }
    if (char === "]" && depthBracket > 0) {
      depthBracket -= 1;
      continue;
    }

    if (depthParen === 0 && depthBracket === 0) {
      if (char === "{") {
        return { type: "brace", index };
      }
      if (char === ";") {
        return { type: "semicolon", index };
      }
    }
  }

  return null;
}

function readCssBlock(css: string, openBraceIndex: number): { body: string; end: number } | null {
  let depth = 0;

  for (let index = openBraceIndex; index < css.length; index += 1) {
    const char = css[index];
    const next = css[index + 1];

    if (char === "\"" || char === "'") {
      index = skipCssString(css, index, char);
      continue;
    }

    if (char === "/" && next === "*") {
      index = skipCssComment(css, index);
      continue;
    }

    if (char === "{") {
      depth += 1;
      continue;
    }

    if (char === "}") {
      depth -= 1;
      if (depth === 0) {
        return {
          body: css.slice(openBraceIndex + 1, index),
          end: index + 1,
        };
      }
    }
  }

  return null;
}

function scopeSelectorList(selectorList: string, scopeSelector: string): string {
  const segments: string[] = [];
  let segmentStart = 0;
  let depthParen = 0;
  let depthBracket = 0;

  for (let index = 0; index < selectorList.length; index += 1) {
    const char = selectorList[index];
    const next = selectorList[index + 1];

    if (char === "\"" || char === "'") {
      index = skipCssString(selectorList, index, char);
      continue;
    }

    if (char === "/" && next === "*") {
      index = skipCssComment(selectorList, index);
      continue;
    }

    if (char === "(") {
      depthParen += 1;
      continue;
    }
    if (char === ")" && depthParen > 0) {
      depthParen -= 1;
      continue;
    }
    if (char === "[") {
      depthBracket += 1;
      continue;
    }
    if (char === "]" && depthBracket > 0) {
      depthBracket -= 1;
      continue;
    }

    if (char === "," && depthParen === 0 && depthBracket === 0) {
      segments.push(selectorList.slice(segmentStart, index));
      segmentStart = index + 1;
    }
  }

  segments.push(selectorList.slice(segmentStart));

  return segments
    .map((segment) => scopeSingleSelector(segment, scopeSelector))
    .join(", ");
}

function scopeSingleSelector(selector: string, scopeSelector: string): string {
  const trimmed = selector.trim();
  if (!trimmed) {
    return trimmed;
  }

  if (trimmed.includes("&")) {
    return trimmed.replace(/&/g, scopeSelector);
  }

  const rootLikePattern = /^(?:\.game-ui-root|:root|body|html)\b/;
  const normalizedSelector = rootLikePattern.test(trimmed)
    ? trimmed.replace(rootLikePattern, scopeSelector)
    : trimmed;

  if (normalizedSelector === scopeSelector || normalizedSelector.startsWith(`${scopeSelector} `)) {
    return normalizedSelector;
  }
  if (
    normalizedSelector.startsWith(`${scopeSelector}:`)
    || normalizedSelector.startsWith(`${scopeSelector}[`)
    || normalizedSelector.startsWith(`${scopeSelector}.`)
  ) {
    return normalizedSelector;
  }

  return `${scopeSelector} ${normalizedSelector}`;
}

function skipCssString(source: string, start: number, quote: string): number {
  for (let index = start + 1; index < source.length; index += 1) {
    if (source[index] === "\\") {
      index += 1;
      continue;
    }
    if (source[index] === quote) {
      return index;
    }
  }
  return source.length - 1;
}

function skipCssComment(source: string, start: number): number {
  const end = source.indexOf("*/", start + 2);
  return end === -1 ? source.length - 1 : end + 1;
}

export function styleRecordToInlineStyle(style?: GameUiStyleRecord): CSSProperties {
  if (!style) {
    return {};
  }

  return Object.fromEntries(
    Object.entries(style).map(([key, value]) => [
      normalizeInlineStyleKey(key),
      value,
    ]),
  ) as CSSProperties;
}

function normalizeCssPropertyKey(key: string): string {
  if (key.startsWith("--")) {
    return key;
  }
  return key
    .replace(/_/g, "-")
    .replace(/([a-z0-9])([A-Z])/g, "$1-$2")
    .toLowerCase();
}

function normalizeInlineStyleKey(key: string): string {
  if (key.startsWith("--")) {
    return key;
  }
  return key
    .replace(/^-ms-/, "ms-")
    .replace(/[-_]+([a-zA-Z0-9])/g, (_, letter: string) => letter.toUpperCase());
}

export function normalizeAssetList(value: unknown): string[] {
  if (!Array.isArray(value)) {
    return [];
  }
  return value.map((item) => String(item).trim()).filter(Boolean);
}

export function resolvePreviewBackgroundAsset(config: UiAssetConfig, sceneName: string): string {
  const trimmed = sceneName.trim();
  if (trimmed) {
    const sceneAssets = config.local_scene_backgrounds[trimmed] ?? [];
    if (sceneAssets.length > 0) {
      return sceneAssets[0];
    }
  }
  return config.local_background_assets[0] ?? "";
}
