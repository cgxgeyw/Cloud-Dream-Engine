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

/** 世界包可订阅的事件。interaction_answered 随交互消息（第 5 项）落地。 */
export type WorldLogicEventName = "session_start" | "turn_completed" | "interaction_answered";

export type WorldLogicConfig = {
  runtime: "disabled" | "sandbox-js-v1";
  source: string;
  entry?: string;
  timeout_ms: number;
  /** 事件 → logic.js 中的处理函数名。事件发生时宿主按次调用 Worker。 */
  events?: Partial<Record<WorldLogicEventName, string>>;
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

export type WorldUiEnvelopeV3 = {
  runtime_version: 3;
  capabilities: string[];
  assets: UiAssetConfig;
  storage: WorldStorageConfig;
  logic: WorldLogicConfig;
  entries: Record<GameUiPlatform, WorldUiEntry>;
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
  result_state?: string;
  error_state?: string;
  pending_state?: string;
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
  // 有 children 的按钮（例如把整根图表柱子做成可点区域）可以不写 label，
  // 可读名称由子内容本身承担。
  label?: string;
  children?: GameUiLayoutNodeV2[];
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

/**
 * SVG 图元。声明式而非裸标记字符串：世界包写`<svg>` 文本需要净化器，
 * 而项目刻意不引入 dangerouslySetInnerHTML，因此把图元拆成可校验的节点。
 * 数值字段走字符串 + `{{$state.x}}` 插值，与现有节点的绑定方式一致。
 */
export type GameUiSvgNode = GameUiNodeBase & {
  type: "svg";
  /** viewBox="minX minY width height"，决定坐标系与缩放。 */
  view_box: string;
  preserve_aspect_ratio?: string;
  children?: GameUiSvgPrimitiveNode[];
};

/** SVG 子图元。不再嵌套布局节点——布局由父 svg 与外层 grid/stack 负责。 */
export type GameUiSvgPrimitiveNode =
  | GameUiSvgCircleNode
  | GameUiSvgRectNode
  | GameUiSvgLineNode
  | GameUiSvgPathNode
  | GameUiSvgPolygonNode
  | GameUiSvgTextNode;

type GameUiSvgPaintNode = {
  class_name?: string;
  fill?: string;
  fill_opacity?: string;
  stroke?: string;
  stroke_width?: string;
  stroke_opacity?: string;
  stroke_linecap?: "butt" | "round" | "square";
  stroke_linejoin?: "miter" | "round" | "bevel";
  stroke_dasharray?: string;
  opacity?: string;
  rotate?: string;
  /** 旋转中心，缺省为图元自身几何中心（cx/cy 或 x/y）。 */
  origin?: string;
};

export type GameUiSvgCircleNode = GameUiSvgPaintNode & {
  type: "svg_circle";
  cx: string;
  cy: string;
  r: string;
};

export type GameUiSvgRectNode = GameUiSvgPaintNode & {
  type: "svg_rect";
  x: string;
  y: string;
  width: string;
  height: string;
  rx?: string;
  ry?: string;
};

export type GameUiSvgLineNode = GameUiSvgPaintNode & {
  type: "svg_line";
  x1: string;
  y1: string;
  x2: string;
  y2: string;
};

export type GameUiSvgPathNode = GameUiSvgPaintNode & {
  type: "svg_path";
  d: string;
  fill_rule?: "nonzero" | "evenodd";
};

/** points 形如 "0,0 10,5 20,0"，由宿主拆分后交给 `<polygon points>`。 */
export type GameUiSvgPolygonNode = GameUiSvgPaintNode & {
  type: "svg_polygon";
  points: string;
};

export type GameUiSvgTextNode = GameUiSvgPaintNode & {
  type: "svg_text";
  x: string;
  y: string;
  text: string;
  font_size?: string;
  font_family?: string;
  font_weight?: string;
  text_anchor?: "start" | "middle" | "end";
  dominant_baseline?:
    | "alphabetic"
    | "hanging"
    | "ideographic"
    | "mathematical"
    | "middle"
    | "central"
    | "text-after-edge"
    | "text-before-edge";
};

export type GameUiCanvasNode = GameUiNodeBase & {
  type: "canvas";
  /** 逻辑画布宽高（CSS 像素），同时决定位图尺寸。 */
  width: string;
  height: string;
  /** 绘制指令，按顺序执行。作者不能提供可执行代码。 */
  ops?: GameUiCanvasOp[];
};

/**
 * Canvas 绘制指令。刻意做成数据而非脚本：宿主在 useEffect 里逐条重放，
 * 世界包无法借此执行任意代码。所有数值字段支持 `{{$state.x}}`。
 */
export type GameUiCanvasOp =
  | { type: "clear"; color?: string }
  | { type: "rect"; x: string; y: string; width: string; height: string; fill?: string; stroke?: string; stroke_width?: string }
  | { type: "circle"; x: string; y: string; radius: string; fill?: string; stroke?: string; stroke_width?: string }
  | { type: "arc"; x: string; y: string; radius: string; start_angle: string; end_angle: string; fill?: string; stroke?: string; stroke_width?: string }
  | { type: "line"; x1: string; y1: string; x2: string; y2: string; stroke?: string; stroke_width?: string }
  | { type: "text"; x: string; y: string; text: string; font_size?: string; font_family?: string; font_weight?: string; fill?: string; text_align?: "left" | "center" | "right" }
  | { type: "image"; x: string; y: string; width: string; height: string; src: string };

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
  | GameUiCheckboxNode
  | GameUiSvgNode
  | GameUiCanvasNode;

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
