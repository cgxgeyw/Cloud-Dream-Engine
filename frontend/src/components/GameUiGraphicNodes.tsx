import { useEffect, useRef } from "react";
import type {
  GameUiCanvasNode,
  GameUiCanvasOp,
  GameUiSvgNode,
  GameUiSvgPrimitiveNode,
} from "../data/gameUi/types";
import type { GameUiRenderContext } from "../components/GameUiRenderer";

/**
 * SVG 与 Canvas 的宿主渲染器。
 *
 * 两条安全约束（与项目的既有立场一致）：
 * - **不解析作者提供的标记文本**。SVG 走图元节点映射到真实元素，Canvas 走声明式
 *   指令重放，两条路径都不存在 `dangerouslySetInnerHTML`，也不需要净化库。
 * - **不在渲染期执行作者代码**。Canvas 的绘图发生在 useEffect 里，逐条调用 2D
 *   context 的标准方法，作者只能描述"画什么"，不能描述"怎么执行"。
 */

/** 解析图元里的数值/字符串字段，支持 `{{$state.x}}` 与裸 `$state.x`。 */
function resolveField(value: string | undefined, context: GameUiRenderContext, fallback = 0): number {
  if (value === undefined) {
    return fallback;
  }
  const trimmed = value.trim();
  if (trimmed === "") {
    return fallback;
  }
  if (trimmed.startsWith("$") || trimmed.includes("{{")) {
    const resolved = resolveNumericField(value, context);
    return Number.isFinite(resolved) ? resolved : fallback;
  }
  const parsed = Number(trimmed);
  return Number.isFinite(parsed) ? parsed : fallback;
}

function resolveNumericField(value: string, context: GameUiRenderContext): number {
  const trimmed = value.trim();
  const path = trimmed.startsWith("$")
    ? trimmed
    : trimmed.startsWith("{{") && trimmed.endsWith("}}")
      ? trimmed.slice(2, -2).trim()
      : null;
  if (!path) {
    return Number.NaN;
  }
  const resolved = resolvePathFromContext(context, path);
  return typeof resolved === "number" ? resolved : Number(resolved);
}

function resolvePaintField(value: string | undefined, context: GameUiRenderContext): string | undefined {
  if (value === undefined) {
    return undefined;
  }
  const trimmed = value.trim();
  if (trimmed === "") {
    return undefined;
  }
  if (trimmed.startsWith("$") || trimmed.includes("{{")) {
    const resolved = resolveNumericField(value, context);
    return Number.isFinite(resolved) ? String(resolved) : undefined;
  }
  return trimmed;
}

function resolvePathFromContext(context: GameUiRenderContext, path: string): unknown {
  // 与GameUiRenderer 的绑定语义保持一致：state 在前，data / locals 作为根。
  const roots: Record<string, unknown> = {
    ...context.data,
    ...context.locals,
    state: context.state,
  };
  let current: unknown = roots;
  for (const segment of path.replace(/^\$/, "").split(".")) {
    if (!segment) {
      continue;
    }
    if (current === null || typeof current !== "object") {
      return undefined;
    }
    current = (current as Record<string, unknown>)[segment];
  }
  return current;
}

function primitiveTransform(node: GameUiSvgPrimitiveNode, context: GameUiRenderContext): string | undefined {
  const raw = node.rotate?.trim();
  if (!raw) {
    return undefined;
  }
  const angle = resolveField(raw, context);
  if (!Number.isFinite(angle) || angle === 0) {
    return undefined;
  }
  // 缺省绕自身几何中心旋转：圆用(cx,cy)，其余用 (x,y) 或 (0,0)。
  const primitive = node as unknown as Record<string, string | undefined>;
  const cx = primitive.cx !== undefined ? resolveField(primitive.cx, context) : undefined;
  const cy = primitive.cy !== undefined ? resolveField(primitive.cy, context) : undefined;
  const originRaw = node.origin?.trim();
  if (originRaw) {
    const parts = originRaw.split(/[\s,]+/).filter(Boolean);
    if (parts.length >= 2) {
      return `rotate(${angle} ${resolveField(parts[0], context)} ${resolveField(parts[1], context)})`;
    }
  }
  if (cx !== undefined && cy !== undefined) {
    return `rotate(${angle} ${cx} ${cy})`;
  }
  const ox = primitive.x !== undefined ? resolveField(primitive.x, context) : 0;
  const oy = primitive.y !== undefined ? resolveField(primitive.y, context) : 0;
  return `rotate(${angle} ${ox} ${oy})`;
}

function paintProps(node: GameUiSvgPrimitiveNode, context: GameUiRenderContext) {
  const transform = primitiveTransform(node, context);
  return {
    className: node.class_name,
    fill: resolvePaintField(node.fill, context),
    fillOpacity: resolvePaintField(node.fill_opacity, context),
    stroke: resolvePaintField(node.stroke, context),
    strokeWidth: resolvePaintField(node.stroke_width, context),
    strokeOpacity: resolvePaintField(node.stroke_opacity, context),
    strokeLinecap: node.stroke_linecap,
    strokeLinejoin: node.stroke_linejoin,
    strokeDasharray: resolvePaintField(node.stroke_dasharray, context),
    opacity: resolvePaintField(node.opacity, context),
    transform,
  };
}

function renderSvgPrimitive(node: GameUiSvgPrimitiveNode, context: GameUiRenderContext) {
  const paint = paintProps(node, context);
  switch (node.type) {
    case "svg_circle":
      return (
        <circle
          key={node.type}
          cx={resolveField(node.cx, context)}
          cy={resolveField(node.cy, context)}
          r={resolveField(node.r, context)}
          {...paint}
        />
      );
    case "svg_rect":
      return (
        <rect
          key={node.type}
          x={resolveField(node.x, context)}
          y={resolveField(node.y, context)}
          width={resolveField(node.width, context)}
          height={resolveField(node.height, context)}
          rx={node.rx === undefined ? undefined : resolveField(node.rx, context)}
          ry={node.ry === undefined ? undefined : resolveField(node.ry, context)}
          {...paint}
        />
      );
    case "svg_line":
      return (
        <line
          key={node.type}
          x1={resolveField(node.x1, context)}
          y1={resolveField(node.y1, context)}
          x2={resolveField(node.x2, context)}
          y2={resolveField(node.y2, context)}
          {...paint}
        />
      );
    case "svg_path":
      return (
        <path
          key={node.type}
          d={resolvePaintField(node.d, context) ?? ""}
          fillRule={node.fill_rule}
          {...paint}
        />
      );
    case "svg_polygon":
      return (
        <polygon
          key={node.type}
          points={resolvePaintField(node.points, context) ?? ""}
          {...paint}
        />
      );
    case "svg_text":
      return (
        <text
          key={node.type}
          x={resolveField(node.x, context)}
          y={resolveField(node.y, context)}
          fontSize={node.font_size === undefined ? undefined : resolveField(node.font_size, context)}
          fontFamily={node.font_family}
          fontWeight={node.font_weight}
          textAnchor={node.text_anchor}
          dominantBaseline={node.dominant_baseline}
          {...paint}
        >
          {resolveTextField(node.text, context)}
        </text>
      );
    default:
      return null;
  }
}

function resolveTextField(template: string, context: GameUiRenderContext): string {
  if (!template) {
    return "";
  }
  const trimmed = template.trim();
  if (trimmed.startsWith("$") && !trimmed.includes(" ")) {
    const resolved = resolvePathFromContext(context, trimmed);
    return resolved === undefined || resolved === null ? "" : String(resolved);
  }
  return template.replace(/\{\{\s*([^}]+?)\s*\}\}/g, (_, expression: string) => {
    const resolved = resolvePathFromContext(context, expression.trim());
    return resolved === undefined || resolved === null ? "" : String(resolved);
  });
}

export function SvgNodeView({ node, context }: { node: GameUiSvgNode; context: GameUiRenderContext }) {
  return (
    <div
      className={["game-ui-node", "game-ui-svg", node.class_name].filter(Boolean).join(" ")}
      style={{
        // 缺省撑满可用空间：SVG 坐标系由 viewBox 决定尺寸，容器不给尺寸就画不出。
        width: node.width ?? "100%",
        height: node.height ?? "100%",
        minWidth: node.min_width,
        minHeight: node.min_height,
        maxWidth: node.max_width,
        maxHeight: node.max_height,
        ...resolveStyleRecordForNode(node, context),
      }}
    >
      <svg
        viewBox={node.view_box}
        preserveAspectRatio={node.preserve_aspect_ratio ?? "xMidYMid meet"}
        width="100%"
        height="100%"
        role="img"
        aria-hidden={node.view_box ? undefined : true}
      >
        {(node.children ?? []).map((child, index) => (
          <PrimitiveSvgChild key={`svg-primitive-${index}`} node={child} context={context} />
        ))}
      </svg>
    </div>
  );
}

function PrimitiveSvgChild({ node, context }: { node: GameUiSvgPrimitiveNode; context: GameUiRenderContext }) {
  return renderSvgPrimitive(node, context);
}

/** Canvas 位图尺寸取逻辑宽高（CSS 像素），乘设备像素比以免高分屏发虚。 */
export function CanvasNodeView({ node, context }: { node: GameUiCanvasNode; context: GameUiRenderContext }) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const logicalWidth = resolveField(node.width, context, 300);
  const logicalHeight = resolveField(node.height, context, 150);
  const ops = node.ops ?? [];

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) {
      return;
    }
    const ratio = Math.max(1, Math.min(3, globalThis.devicePixelRatio || 1));
    canvas.width = Math.max(1, Math.round(logicalWidth * ratio));
    canvas.height = Math.max(1, Math.round(logicalHeight * ratio));
    const ctx = canvas.getContext("2d");
    if (!ctx) {
      return;
    }
    ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
    replayCanvasOps(ctx, ops, context);
    // 图片指令走异步解码，解码完成后用同一个回调再画一次。
    const redraw = () => {
      ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
      replayCanvasOps(ctx, ops, context);
    };
    canvasImageListeners.set(canvas, redraw);
    // ops 每次渲染都是新数组，逐一展开其中的绑定字段作为依赖。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [logicalWidth, logicalHeight, ...ops.flatMap((op) => Object.values(op).map((v) => (typeof v === "string" ? v : JSON.stringify(v))))]);

  return (
    <div
      className={["game-ui-node", "game-ui-canvas", node.class_name].filter(Boolean).join(" ")}
      style={{
        // 位图尺寸由 width / height 决定；缺省给 300×150 的逻辑画布。
        width: node.width ?? "300px",
        height: node.height ?? "150px",
        minWidth: node.min_width,
        minHeight: node.min_height,
        ...resolveStyleRecordForNode(node, context),
      }}
    >
      <canvas
        ref={canvasRef}
        style={{ width: "100%", height: "100%", display: "block" }}
        role="img"
        aria-label="canvas"
      />
    </div>
  );
}

function replayCanvasOps(
  ctx: CanvasRenderingContext2D,
  ops: GameUiCanvasOp[],
  context: GameUiRenderContext,
) {
  ctx.globalAlpha = 1;
  ctx.setLineDash([]);
  for (const op of ops) {
    switch (op.type) {
      case "clear":
        ctx.clearRect(0, 0, ctx.canvas.width, ctx.canvas.height);
        if (op.color) {
          ctx.fillStyle = resolvePaintField(op.color, context) ?? "#000";
          ctx.fillRect(0, 0, ctx.canvas.width, ctx.canvas.height);
        }
        break;
      case "rect": {
        const width = resolveField(op.width, context);
        const height = resolveField(op.height, context);
        applyPaint(ctx, op, context);
        if (op.fill) {
          ctx.fillRect(resolveField(op.x, context), resolveField(op.y, context), width, height);
        }
        if (op.stroke) {
          ctx.strokeRect(resolveField(op.x, context), resolveField(op.y, context), width, height);
        }
        break;
      }
      case "circle": {
        const radius = Math.abs(resolveField(op.radius, context));
        applyPaint(ctx, op, context);
        ctx.beginPath();
        ctx.arc(resolveField(op.x, context), resolveField(op.y, context), radius, 0, Math.PI * 2);
        if (op.fill) {
          ctx.fill();
        }
        if (op.stroke) {
          ctx.stroke();
        }
        break;
      }
      case "arc": {
        // 约定角度单位为「度」，与 CSS / 导演语境一致，避免世界包作者再换算。
        const radius = Math.abs(resolveField(op.radius, context));
        applyPaint(ctx, op, context);
        ctx.beginPath();
        ctx.arc(
          resolveField(op.x, context),
          resolveField(op.y, context),
          radius,
          (resolveField(op.start_angle, context) * Math.PI) / 180,
          (resolveField(op.end_angle, context) * Math.PI) / 180,
        );
        if (op.fill) {
          ctx.fill();
        }
        if (op.stroke) {
          ctx.stroke();
        }
        break;
      }
      case "line": {
        applyPaint(ctx, op, context);
        ctx.beginPath();
        ctx.moveTo(resolveField(op.x1, context), resolveField(op.y1, context));
        ctx.lineTo(resolveField(op.x2, context), resolveField(op.y2, context));
        ctx.stroke();
        break;
      }
      case "text": {
        ctx.fillStyle = resolvePaintField(op.fill, context) ?? ctx.fillStyle;
        ctx.font = buildCanvasFont(op, context);
        ctx.textAlign = op.text_align ?? "left";
        ctx.textBaseline = "alphabetic";
        ctx.fillText(resolveTextField(op.text, context), resolveField(op.x, context), resolveField(op.y, context));
        break;
      }
      case "image": {
        const src = resolvePaintField(op.src, context);
        if (!src) {
          break;
        }
        const image = loadCanvasImage(src, ctx.canvas);
        if (image) {
          ctx.drawImage(
            image,
            resolveField(op.x, context),
            resolveField(op.y, context),
            resolveField(op.width, context),
            resolveField(op.height, context),
          );
        }
        break;
      }
      default:
        break;
    }
  }
}

function applyPaint(
  ctx: CanvasRenderingContext2D,
  op: { fill?: string; stroke?: string; stroke_width?: string },
  context: GameUiRenderContext,
) {
  if (op.fill) {
    ctx.fillStyle = resolvePaintField(op.fill, context) ?? ctx.fillStyle;
  }
  if (op.stroke) {
    ctx.strokeStyle = resolvePaintField(op.stroke, context) ?? ctx.strokeStyle;
  }
  ctx.lineWidth = op.stroke_width ? resolveField(op.stroke_width, context, 1) : 1;
}

function buildCanvasFont(
  op: Extract<GameUiCanvasOp, { type: "text" }>,
  context: GameUiRenderContext,
): string {
  const size = op.font_size ? resolveField(op.font_size, context, 12) : 12;
  const weight = op.font_weight ? resolvePaintField(op.font_weight, context) ?? "400" : "400";
  const family = op.font_family ? resolvePaintField(op.font_family, context) : "sans-serif";
  return `${weight} ${size}px ${family}`;
}

/** 图片是异步解码的，无法在同一帧内画完；解码完成后触发重绘请求。 */
const canvasImageCache = new Map<string, HTMLImageElement>();
const canvasImageListeners = new WeakMap<HTMLCanvasElement, () => void>();

function loadCanvasImage(src: string, canvas: HTMLCanvasElement): HTMLImageElement | null {
  const cached = canvasImageCache.get(src);
  if (cached) {
    return cached.complete && cached.naturalWidth > 0 ? cached : null;
  }
  if (typeof Image === "undefined") {
    return null;
  }
  const image = new Image();
  canvasImageCache.set(src, image);
  image.onload = () => {
    canvasImageListeners.get(canvas)?.();
  };
  image.src = src;
  return null;
}

// 样式记录走渲染器同一套绑定语义，避免 svg/canvas 节点与其它节点的样式表现分叉。
function resolveStyleRecordForNode(node: { style?: Record<string, unknown> }, context: GameUiRenderContext) {
  const out: Record<string, string | number> = {};
  if (!node.style) {
    return out;
  }
  for (const [key, raw] of Object.entries(node.style)) {
    if (typeof raw === "number") {
      out[key] = raw;
      continue;
    }
    if (typeof raw === "boolean") {
      out[key] = raw ? 1 : 0;
      continue;
    }
    if (typeof raw !== "string") {
      continue;
    }
    const trimmed = raw.trim();
    if (!trimmed.startsWith("$") && !trimmed.includes("{{")) {
      out[key] = raw;
      continue;
    }
    const path = trimmed.startsWith("$")
      ? trimmed
      : trimmed.replace(/^\{\{/, "").replace(/\}\}$/, "").trim();
    const resolved = resolvePathFromContext(context, path);
    if (typeof resolved === "number") {
      out[key] = resolved;
    } else if (typeof resolved === "boolean") {
      out[key] = resolved ? 1 : 0;
    } else if (resolved !== undefined && resolved !== null) {
      out[key] = String(resolved);
    }
  }
  return out;
}