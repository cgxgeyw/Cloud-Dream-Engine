import { useState, type CSSProperties, type Dispatch, type ReactNode, type SetStateAction } from "react";
import {
  type GameUiActionReference,
  type GameUiBadgeNode,
  type GameUiButtonNode,
  type GameUiCheckboxNode,
  type GameUiComponentNode,
  type GameUiAnchor,
  type GameUiDocument,
  type GameUiDocumentV1,
  type GameUiDocumentV2,
  type GameUiForEachNode,
  type GameUiImageNode,
  type GameUiLayoutNode,
  type GameUiLayoutNodeV1,
  type GameUiLayoutNodeV2,
  type GameUiMountId,
  type GameUiMountNode,
  type GameUiMountOptions,
  type GameUiTextNode,
  styleRecordToInlineStyle,
} from "../data/gameUi";

export type GameUiComponentRendererContext = {
  document: GameUiDocumentV2;
  node: GameUiComponentNode;
  renderSlot: (slotName: string) => ReactNode;
  renderSlotValue: (
    value: GameUiLayoutNodeV2 | GameUiLayoutNodeV2[] | undefined,
    keyPrefix?: string,
  ) => ReactNode;
};

export type GameUiComponentRenderer = (
  context: GameUiComponentRendererContext,
) => ReactNode;

type GameUiRendererProps = {
  document: GameUiDocument;
  mounts?: Partial<Record<GameUiMountId, ReactNode>>;
  componentRenderers?: Partial<Record<string, GameUiComponentRenderer>>;
  evaluateCondition?: (expr: string) => boolean;
  resolveLoopSource?: (source: string) => unknown[];
  runtimeData?: Record<string, unknown>;
  onAction?: (action: GameUiActionReference, context: GameUiRenderContext) => void | Promise<void>;
};

export type GameUiRenderContext = {
  state: Record<string, unknown>;
  data: Record<string, unknown>;
  locals: Record<string, unknown>;
};

export function GameUiRenderer({
  document,
  mounts = {},
  componentRenderers,
  evaluateCondition,
  resolveLoopSource,
  runtimeData = {},
  onAction,
}: GameUiRendererProps) {
  const [uiState, setUiState] = useState<Record<string, unknown>>(() =>
    document.schema_version === 2 ? normalizeInitialState(document.state) : {},
  );
  const context: GameUiRenderContext = {
    state: uiState,
    data: runtimeData,
    locals: {},
  };
  const rendererActions: GameUiElementActions = {
    setUiState,
    onAction,
  };

  return (
    <div className="game-ui-layout">
      {document.schema_version === 1
        ? renderV1Layout(document, mounts)
        : renderV2Layout(document, componentRenderers, evaluateCondition, resolveLoopSource, context, rendererActions)}
    </div>
  );
}

type GameUiElementActions = {
  setUiState: Dispatch<SetStateAction<Record<string, unknown>>>;
  onAction?: (action: GameUiActionReference, context: GameUiRenderContext) => void | Promise<void>;
};

function renderV1Layout(
  document: GameUiDocumentV1,
  mounts: Partial<Record<GameUiMountId, ReactNode>>,
): ReactNode {
  return renderV1Node(document.layout.root, document, mounts, "root");
}

function renderV1Node(
  node: GameUiLayoutNodeV1,
  document: GameUiDocumentV1,
  mounts: Partial<Record<GameUiMountId, ReactNode>>,
  key: string,
): ReactNode {
  if (node.visible === false) {
    return null;
  }

  if (node.type === "mount") {
    return renderMountNode(node, document.mounts?.[node.mount], mounts[node.mount], key);
  }

  if (node.type === "absolute") {
    const children = (node.children ?? [])
      .map((child, index) => renderV1Node(child, document, mounts, `${key}-${index}`))
      .filter((child) => child !== null);
    if (children.length === 0) {
      return null;
    }
    return (
      <div
        key={key}
        className={["game-ui-node", "game-ui-node--absolute", node.class_name].filter(Boolean).join(" ")}
        style={buildNodeStyle(node)}
      >
        {children}
      </div>
    );
  }

  if (node.type === "stack") {
    return (
      <div
        key={key}
        className={["game-ui-node", "game-ui-node--stack", node.class_name].filter(Boolean).join(" ")}
        style={buildNodeStyle(node)}
      >
        {(node.children ?? []).map((child, index) => renderV1Node(child, document, mounts, `${key}-${index}`))}
      </div>
    );
  }

  return (
    <div
      key={key}
      className={["game-ui-node", "game-ui-node--grid", node.class_name].filter(Boolean).join(" ")}
      style={buildNodeStyle(node)}
    >
      {(node.children ?? []).map((child, index) => renderV1Node(child, document, mounts, `${key}-${index}`))}
    </div>
  );
}

function renderV2Layout(
  document: GameUiDocumentV2,
  componentRenderers: Partial<Record<string, GameUiComponentRenderer>> | undefined,
  evaluateCondition: ((expr: string) => boolean) | undefined,
  resolveLoopSource: ((source: string) => unknown[]) | undefined,
  context: GameUiRenderContext,
  actions: GameUiElementActions,
): ReactNode {
  return renderV2Node(
    document.layout.root,
    document,
    componentRenderers,
    evaluateCondition,
    resolveLoopSource,
    context,
    actions,
    "root",
  );
}

function renderV2Node(
  node: GameUiLayoutNodeV2,
  document: GameUiDocumentV2,
  componentRenderers: Partial<Record<string, GameUiComponentRenderer>> | undefined,
  evaluateCondition: ((expr: string) => boolean) | undefined,
  resolveLoopSource: ((source: string) => unknown[]) | undefined,
  context: GameUiRenderContext,
  actions: GameUiElementActions,
  key: string,
): ReactNode {
  if (node.visible === false) {
    return null;
  }

  if (node.type === "component") {
    return renderComponentNode(
      node,
      document,
      componentRenderers,
      evaluateCondition,
      resolveLoopSource,
      context,
      actions,
      key,
    );
  }

  if (node.type === "text") {
    return renderTextNode(node, context, key);
  }

  if (node.type === "image") {
    return renderImageNode(node, context, key);
  }

  if (node.type === "badge") {
    return renderBadgeNode(node, context, key);
  }

  if (node.type === "button") {
    return renderButtonNode(node, context, actions, key);
  }

  if (node.type === "checkbox") {
    return renderCheckboxNode(node, context, actions, key);
  }

  if (node.type === "slot") {
    return (
      <div
        key={key}
        className={["game-ui-slot", node.class_name].filter(Boolean).join(" ")}
        data-slot={node.name}
        style={buildNodeStyle(node)}
      />
    );
  }

  if (node.type === "when") {
    if (evaluateCondition && evaluateCondition(node.expr) === false) {
      return null;
    }
    return renderV2Node(
      node.child,
      document,
      componentRenderers,
      evaluateCondition,
      resolveLoopSource,
      context,
      actions,
      `${key}-child`,
    );
  }

  if (node.type === "for_each") {
    return renderLoopNode(
      node,
      document,
      componentRenderers,
      evaluateCondition,
      resolveLoopSource,
      context,
      actions,
      key,
    );
  }

  if (node.type === "absolute") {
    const children = (node.children ?? [])
      .map((child, index) =>
        renderV2Node(
          child,
          document,
          componentRenderers,
          evaluateCondition,
          resolveLoopSource,
          context,
          actions,
          `${key}-${index}`,
        ),
      )
      .filter((child) => child !== null);
    if (children.length === 0) {
      return null;
    }
    return (
      <div
        key={key}
        className={["game-ui-node", "game-ui-node--absolute", node.class_name].filter(Boolean).join(" ")}
        style={buildNodeStyle(node)}
      >
        {children}
      </div>
    );
  }

  if (node.type === "stack") {
    return (
      <div
        key={key}
        className={["game-ui-node", "game-ui-node--stack", node.class_name].filter(Boolean).join(" ")}
        style={buildNodeStyle(node)}
      >
        {(node.children ?? []).map((child, index) =>
          renderV2Node(
            child,
            document,
            componentRenderers,
            evaluateCondition,
            resolveLoopSource,
            context,
            actions,
            `${key}-${index}`,
          ),
        )}
      </div>
    );
  }

  return (
    <div
      key={key}
      className={["game-ui-node", "game-ui-node--grid", node.class_name].filter(Boolean).join(" ")}
      style={buildNodeStyle(node)}
    >
      {(node.children ?? []).map((child, index) =>
        renderV2Node(
          child,
          document,
          componentRenderers,
          evaluateCondition,
          resolveLoopSource,
          context,
          actions,
          `${key}-${index}`,
        ),
      )}
    </div>
  );
}

function renderMountNode(
  node: GameUiMountNode,
  mountOptions: GameUiMountOptions | undefined,
  content: ReactNode,
  key: string,
): ReactNode {
  if (mountOptions?.visible === false || !content) {
    return null;
  }

  return (
    <div
      key={key}
      className={[
        "game-ui-node",
        "game-ui-node--mount",
        "game-ui-mount",
        `game-ui-mount--${node.mount}`,
        toMountClassSuffix(node.mount) === node.mount ? undefined : `game-ui-mount--${toMountClassSuffix(node.mount)}`,
        node.class_name,
        mountOptions?.class_name,
      ].filter(Boolean).join(" ")}
      data-mount={node.mount}
      data-variant={mountOptions?.variant}
      data-chrome={mountOptions?.chrome}
      style={buildNodeStyle(node, mountOptions)}
    >
      {content}
    </div>
  );
}

function renderComponentNode(
  node: GameUiComponentNode,
  document: GameUiDocumentV2,
  componentRenderers: Partial<Record<string, GameUiComponentRenderer>> | undefined,
  evaluateCondition: ((expr: string) => boolean) | undefined,
  resolveLoopSource: ((source: string) => unknown[]) | undefined,
  context: GameUiRenderContext,
  actions: GameUiElementActions,
  key: string,
): ReactNode {
  const renderer = componentRenderers?.[node.component];
  const content = renderer
    ? renderer({
        document,
        node,
        renderSlot: (slotName) =>
          renderSlotValue(
            node.slots?.[slotName],
            document,
            componentRenderers,
            evaluateCondition,
            resolveLoopSource,
            context,
            actions,
            `${key}-slot-${slotName}`,
          ),
        renderSlotValue: (value, keyPrefix = `${key}-slot`) =>
          renderSlotValue(
            value,
            document,
            componentRenderers,
            evaluateCondition,
            resolveLoopSource,
            context,
            actions,
            keyPrefix,
          ),
      })
    : (
      <div className="game-ui-component-missing" data-missing-component={node.component}>
        {`Missing component renderer: ${node.component}`}
      </div>
    );

  if (content === null) {
    return null;
  }

  return (
    <div
      key={key}
      className={[
        "game-ui-component",
        `game-ui-component--${node.component.replace(/_/g, "-")}`,
        node.class_name,
      ].filter(Boolean).join(" ")}
      data-component={node.component}
      data-variant={node.variant}
      style={buildNodeStyle(node)}
    >
      {content}
    </div>
  );
}

function renderLoopNode(
  node: GameUiForEachNode,
  document: GameUiDocumentV2,
  componentRenderers: Partial<Record<string, GameUiComponentRenderer>> | undefined,
  evaluateCondition: ((expr: string) => boolean) | undefined,
  resolveLoopSource: ((source: string) => unknown[]) | undefined,
  context: GameUiRenderContext,
  actions: GameUiElementActions,
  key: string,
): ReactNode {
  const items = resolveLoopSource?.(node.source) ?? resolvePath(context, node.source);
  const normalizedItems = Array.isArray(items) ? items : [];

  if (normalizedItems.length === 0) {
    if (!node.empty) {
      return null;
    }
    return renderV2Node(
      node.empty,
      document,
      componentRenderers,
      evaluateCondition,
      resolveLoopSource,
      context,
      actions,
      `${key}-empty`,
    );
  }

  return (
    <>
      {normalizedItems.map((item, index) => {
        const loopContext = {
          ...context,
          locals: {
            ...context.locals,
            [node.item_as]: item,
            ...(node.index_as ? { [node.index_as]: index } : {}),
          },
        };
        return renderV2Node(
          node.child,
          document,
          componentRenderers,
          evaluateCondition,
          resolveLoopSource,
          loopContext,
          actions,
          `${key}-${index}`,
        );
      })}
    </>
  );
}

function renderSlotValue(
  value: GameUiLayoutNodeV2 | GameUiLayoutNodeV2[] | undefined,
  document: GameUiDocumentV2,
  componentRenderers: Partial<Record<string, GameUiComponentRenderer>> | undefined,
  evaluateCondition: ((expr: string) => boolean) | undefined,
  resolveLoopSource: ((source: string) => unknown[]) | undefined,
  context: GameUiRenderContext,
  actions: GameUiElementActions,
  keyPrefix: string,
): ReactNode {
  if (!value) {
    return null;
  }

  if (Array.isArray(value)) {
    return value.map((item, index) =>
      renderV2Node(
        item,
        document,
        componentRenderers,
        evaluateCondition,
        resolveLoopSource,
        context,
        actions,
        `${keyPrefix}-${index}`,
      ),
    );
  }

  return renderV2Node(
    value,
    document,
    componentRenderers,
    evaluateCondition,
    resolveLoopSource,
    context,
    actions,
    keyPrefix,
  );
}

function renderTextNode(
  node: GameUiTextNode,
  context: GameUiRenderContext,
  key: string,
): ReactNode {
  return (
    <span
      key={key}
      className={["game-ui-node", "game-ui-text", node.class_name].filter(Boolean).join(" ")}
      data-variant={node.variant}
      style={buildNodeStyle(node)}
    >
      {resolveText(node.text, context)}
    </span>
  );
}

function renderImageNode(
  node: GameUiImageNode,
  context: GameUiRenderContext,
  key: string,
): ReactNode {
  const src = resolveText(node.src, context).trim();
  if (!src) {
    return null;
  }

  return (
    <img
      key={key}
      className={["game-ui-node", "game-ui-image", node.class_name].filter(Boolean).join(" ")}
      src={src}
      alt={resolveText(node.alt ?? "", context)}
      style={{
        ...buildNodeStyle(node),
        objectFit: node.fit,
      }}
    />
  );
}

function renderBadgeNode(
  node: GameUiBadgeNode,
  context: GameUiRenderContext,
  key: string,
): ReactNode {
  return (
    <span
      key={key}
      className={["game-ui-node", "game-ui-badge", node.class_name].filter(Boolean).join(" ")}
      data-variant={node.variant}
      style={buildNodeStyle(node)}
    >
      {resolveText(node.text, context)}
    </span>
  );
}

function renderButtonNode(
  node: GameUiButtonNode,
  context: GameUiRenderContext,
  actions: GameUiElementActions,
  key: string,
): ReactNode {
  const disabled = node.disabled_when_empty_state
    ? normalizeStringArray(context.state[node.disabled_when_empty_state]).length === 0
    : false;
  return (
    <button
      key={key}
      type="button"
      className={["game-ui-node", "game-ui-button", "game-ui-dsl-button", node.class_name].filter(Boolean).join(" ")}
      data-variant={node.variant ?? "primary"}
      style={buildNodeStyle(node)}
      disabled={disabled}
      onClick={() => {
        if (disabled) {
          return;
        }
        if (node.action) {
          void actions.onAction?.(node.action, context);
        }
      }}
    >
      {resolveText(node.label, context)}
    </button>
  );
}

function renderCheckboxNode(
  node: GameUiCheckboxNode,
  context: GameUiRenderContext,
  actions: GameUiElementActions,
  key: string,
): ReactNode {
  const stateKey = node.bind_checked_list.trim();
  const value = resolveText(node.value, context);
  const selectedValues = normalizeStringArray(context.state[stateKey]);
  const checked = node.checked === true || selectedValues.includes(value);

  return (
    <label
      key={key}
      className={["game-ui-node", "game-ui-checkbox", node.class_name].filter(Boolean).join(" ")}
      data-variant={node.variant}
      data-checked={checked ? "true" : "false"}
      style={buildNodeStyle(node)}
    >
      <input
        type="checkbox"
        checked={checked}
        disabled={node.disabled}
        value={value}
        onChange={(event) => {
          if (node.disabled) {
            return;
          }
          actions.setUiState((previous) => {
            const previousValues = normalizeStringArray(previous[stateKey]);
            const nextValues = event.target.checked
              ? [...previousValues.filter((item) => item !== value), value]
              : previousValues.filter((item) => item !== value);
            return {
              ...previous,
              [stateKey]: nextValues,
            };
          });
        }}
      />
      <span className="game-ui-checkbox-label">{resolveText(node.label, context)}</span>
    </label>
  );
}

function normalizeInitialState(raw: Record<string, unknown> | undefined): Record<string, unknown> {
  if (!raw) {
    return {};
  }
  return structuredClone(raw);
}

function normalizeStringArray(value: unknown): string[] {
  return Array.isArray(value)
    ? value.map((item) => String(item)).filter(Boolean)
    : [];
}

function resolveText(template: string, context: GameUiRenderContext): string {
  const trimmed = template.trim();
  if (trimmed.startsWith("$") && !trimmed.includes(" ")) {
    return stringifyTemplateValue(resolvePath(context, trimmed));
  }

  return template.replace(/\{\{\s*([^}]+?)\s*\}\}/g, (_, expression: string) =>
    stringifyTemplateValue(resolvePath(context, expression.trim())),
  );
}

function resolvePath(context: GameUiRenderContext, expression: string): unknown {
  const normalized = expression.startsWith("$") ? expression.slice(1) : expression;
  if (!normalized) {
    return "";
  }

  const [root, ...parts] = normalized.split(".").filter(Boolean);
  let current: unknown;
  if (root === "state") {
    current = context.state;
  } else if (root === "data") {
    current = context.data;
  } else if (root in context.locals) {
    current = context.locals[root];
  } else {
    current = context.data[root];
  }

  for (const part of parts) {
    if (current == null) {
      return "";
    }
    if (Array.isArray(current)) {
      const index = Number(part);
      current = Number.isInteger(index) ? current[index] : undefined;
      continue;
    }
    if (typeof current === "object") {
      current = (current as Record<string, unknown>)[part];
      continue;
    }
    return "";
  }

  return current;
}

function stringifyTemplateValue(value: unknown): string {
  if (value == null) {
    return "";
  }
  if (typeof value === "string") {
    return value;
  }
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  if (Array.isArray(value)) {
    return value.map((item) => stringifyTemplateValue(item)).filter(Boolean).join("、");
  }
  try {
    return JSON.stringify(value);
  } catch {
    return "";
  }
}

function toMountClassSuffix(mountId: GameUiMountId): string {
  return mountId.replace(/_/g, "-");
}

function buildNodeStyle(
  node: GameUiLayoutNode,
  mountOptions?: GameUiMountOptions,
): CSSProperties {
  const style: CSSProperties = {
    width: node.width,
    height: node.height,
    minWidth: node.min_width,
    minHeight: mountOptions?.min_height ?? node.min_height,
    maxWidth: mountOptions?.max_width ?? node.max_width,
    maxHeight: node.max_height,
    padding: node.padding,
    margin: node.margin,
    alignItems: node.align,
    justifyContent: node.justify,
    ...styleRecordToInlineStyle(node.style),
    ...styleRecordToInlineStyle(mountOptions?.style),
  };

  if (node.type === "grid") {
    style.display = "grid";
    style.position = "relative";
    style.gridTemplateColumns = node.columns?.length ? node.columns.join(" ") : undefined;
    style.gridTemplateRows = node.rows?.length ? node.rows.join(" ") : undefined;
    style.gridTemplateAreas = node.areas?.length
      ? node.areas.map((row) => `"${row.join(" ")}"`).join(" ")
      : undefined;
    style.gap = node.gap;
  }

  if (node.area) {
    style.gridArea = node.area;
  }

  if (node.type === "stack") {
    style.display = "flex";
    style.position = "relative";
    style.flexDirection = node.direction === "horizontal" ? "row" : "column";
    style.flexWrap = node.wrap ? "wrap" : "nowrap";
    style.gap = node.gap;
  }

  if (node.type === "absolute") {
    style.position = "absolute";
    style.inset = 0;
    style.pointerEvents = "none";
    style.zIndex = 30;
  }

  if (node.type === "mount") {
    applyAnchorStyle(style, node.anchor);
    if (mountOptions?.sticky === "bottom") {
      style.position = "sticky";
      style.bottom = 0;
      style.zIndex = 10;
    }
  }

  if (node.type === "component") {
    applyAnchorStyle(style, node.anchor);
  }

  return style;
}

function applyAnchorStyle(style: CSSProperties, anchor: GameUiAnchor | undefined) {
  if (!anchor) {
    return;
  }

  style.position = "absolute";
  style.top = anchor.top;
  style.right = anchor.right;
  style.bottom = anchor.bottom;
  style.left = anchor.left;
  style.zIndex = 20;
  style.pointerEvents = "auto";
}
