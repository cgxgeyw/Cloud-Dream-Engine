export type GameUiPlatform = "desktop" | "mobile";
export const LATEST_SUPPORTED_GAME_UI_SCHEMA_VERSION = 2;
export const DEFAULT_GAME_UI_SCHEMA_VERSION = LATEST_SUPPORTED_GAME_UI_SCHEMA_VERSION;
export const SUPPORTED_GAME_UI_SCHEMA_VERSIONS = [
  LATEST_SUPPORTED_GAME_UI_SCHEMA_VERSION,
] as const;
export const SUPPORTED_GAME_UI_SCHEMA_VERSION = LATEST_SUPPORTED_GAME_UI_SCHEMA_VERSION;

/**
 * 游戏页挂载点稳定契约（Stable Mount Contract）。
 *
 * 这 9 个 mount ID 仅服务于 schema v1 的兼容层。v2 组件树模式不再以扩展 mount 数量作为演进方向，
 * 新功能应优先通过注册组件、绑定和 action 进入系统。
 *
 * @see docs/world-package-game-ui-implementation-breakdown.md
 * @frozen 2026-06-01 — v1 兼容保留，v2 演进。
 */
export const GAME_UI_MOUNT_IDS = [
  "header",
  "scene",
  "scene_focus",
  "character_bar",
  "narration",
  "message_list",
  "side_panel",
  "input_area",
  "floating_actions",
] as const;
export type GameUiMountId = (typeof GAME_UI_MOUNT_IDS)[number];

export type UiAssetConfig = {
  background_source_mode: string;
  portrait_source_mode: string;
  runtime_image_generation_enabled: boolean;
  local_background_assets: string[];
  local_scene_backgrounds: Record<string, string[]>;
};

export type WorldUiEnvelope = {
  assets: UiAssetConfig;
  desktop_file: string;
  mobile_file: string;
};

export type GameUiStyleRecord = Record<string, string | number | boolean | null | undefined>;

export type GameUiComponentStyleDefinition = {
  base?: GameUiStyleRecord;
  variants?: Record<string, GameUiStyleRecord>;
};

export type GameUiMountOptions = {
  visible?: boolean;
  variant?: string;
  chrome?: string;
  class_name?: string;
  max_width?: string;
  min_height?: string;
  sticky?: string;
  tab_order?: string[];
  style?: GameUiStyleRecord;
};

export type GameUiNodeBase = {
  id?: string;
  visible?: boolean;
  class_name?: string;
  area?: string;
  width?: string;
  height?: string;
  min_width?: string;
  min_height?: string;
  max_width?: string;
  max_height?: string;
  padding?: string;
  margin?: string;
  align?: string;
  justify?: string;
  style?: GameUiStyleRecord;
};

export type GameUiAnchor = {
  top?: string;
  right?: string;
  bottom?: string;
  left?: string;
};

export type GameUiPrimitivePropValue = string | number | boolean | null;
export type GameUiBindingValue = string;

export type GameUiActionReference = {
  id: string;
  args?: Record<string, GameUiPropValue>;
  content?: string;
  content_template?: string;
  mode?: string;
};

export type GameUiPropValue =
  | GameUiPrimitivePropValue
  | GameUiBindingValue
  | GameUiActionReference
  | GameUiPropValue[]
  | { [key: string]: GameUiPropValue };

export type GameUiGridNodeV2 = GameUiNodeBase & {
  type: "grid";
  columns?: string[];
  rows?: string[];
  areas?: string[][];
  gap?: string;
  children?: GameUiLayoutNodeV2[];
};

export type GameUiStackNodeV2 = GameUiNodeBase & {
  type: "stack";
  direction?: "vertical" | "horizontal";
  wrap?: boolean;
  gap?: string;
  children?: GameUiLayoutNodeV2[];
};

export type GameUiAbsoluteNodeV2 = GameUiNodeBase & {
  type: "absolute";
  children?: GameUiLayoutNodeV2[];
};

export type GameUiComponentNode = GameUiNodeBase & {
  type: "component";
  component: string;
  variant?: string;
  props?: Record<string, GameUiPropValue>;
  slots?: Record<string, GameUiLayoutNodeV2 | GameUiLayoutNodeV2[]>;
  anchor?: GameUiAnchor;
};

export type GameUiSlotNode = GameUiNodeBase & {
  type: "slot";
  name: string;
};

export type GameUiWhenNode = GameUiNodeBase & {
  type: "when";
  expr: string;
  child: GameUiLayoutNodeV2;
};

export type GameUiForEachNode = GameUiNodeBase & {
  type: "for_each";
  source: string;
  item_as: string;
  index_as?: string;
  empty?: GameUiLayoutNodeV2;
  child: GameUiLayoutNodeV2;
};

export type GameUiTextNode = GameUiNodeBase & {
  type: "text";
  text: string;
  variant?: string;
};

export type GameUiImageNode = GameUiNodeBase & {
  type: "image";
  src: string;
  alt?: string;
  fit?: "cover" | "contain" | "fill" | "none" | "scale-down";
};

export type GameUiBadgeNode = GameUiNodeBase & {
  type: "badge";
  text: string;
  variant?: string;
};

export type GameUiButtonNode = GameUiNodeBase & {
  type: "button";
  label: string;
  variant?: string;
  disabled_when_empty_state?: string;
  action?: GameUiActionReference;
};

export type GameUiCheckboxNode = GameUiNodeBase & {
  type: "checkbox";
  label: string;
  value: string;
  bind_checked_list: string;
  checked?: boolean;
  disabled?: boolean;
  variant?: string;
};

export type GameUiLayoutNodeV2 =
  | GameUiGridNodeV2
  | GameUiStackNodeV2
  | GameUiAbsoluteNodeV2
  | GameUiComponentNode
  | GameUiSlotNode
  | GameUiWhenNode
  | GameUiForEachNode
  | GameUiTextNode
  | GameUiImageNode
  | GameUiBadgeNode
  | GameUiButtonNode
  | GameUiCheckboxNode;

export type GameUiGridNode = GameUiGridNodeV2;
export type GameUiStackNode = GameUiStackNodeV2;
export type GameUiAbsoluteNode = GameUiAbsoluteNodeV2;

export type GameUiDocumentBase = {
  meta?: Record<string, unknown>;
  components?: Record<string, GameUiComponentStyleDefinition>;
  tokens?: Record<string, string>;
  effects?: Record<string, unknown>;
  custom_css?: string;
};

export type GameUiDocumentV2 = GameUiDocumentBase & {
  schema_version: 2;
  state?: Record<string, GameUiPropValue>;
  layout: {
    root: GameUiLayoutNodeV2;
  };
  mounts?: Partial<Record<GameUiMountId, GameUiMountOptions>>;
};

export type GameUiDocument = GameUiDocumentV2;
export type GameUiLayoutNode = GameUiLayoutNodeV2;

export type ParsedGameUiDocument = {
  document: GameUiDocument;
  source: string;
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
