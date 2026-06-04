import type { CSSProperties, ReactNode } from "react";
import {
  type GameUiComponentNode,
  type GameUiAnchor,
  type GameUiDocument,
  type GameUiDocumentV1,
  type GameUiDocumentV2,
  type GameUiForEachNode,
  type GameUiLayoutNode,
  type GameUiLayoutNodeV1,
  type GameUiLayoutNodeV2,
  type GameUiMountId,
  type GameUiMountNode,
  type GameUiMountOptions,
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
};

export function GameUiRenderer({
  document,
  mounts = {},
  componentRenderers,
  evaluateCondition,
  resolveLoopSource,
}: GameUiRendererProps) {
  return (
    <div className="game-ui-layout">
      {document.schema_version === 1
        ? renderV1Layout(document, mounts)
        : renderV2Layout(document, componentRenderers, evaluateCondition, resolveLoopSource)}
    </div>
  );
}

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
): ReactNode {
  return renderV2Node(
    document.layout.root,
    document,
    componentRenderers,
    evaluateCondition,
    resolveLoopSource,
    "root",
  );
}

function renderV2Node(
  node: GameUiLayoutNodeV2,
  document: GameUiDocumentV2,
  componentRenderers: Partial<Record<string, GameUiComponentRenderer>> | undefined,
  evaluateCondition: ((expr: string) => boolean) | undefined,
  resolveLoopSource: ((source: string) => unknown[]) | undefined,
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
      key,
    );
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
            `${key}-slot-${slotName}`,
          ),
        renderSlotValue: (value, keyPrefix = `${key}-slot`) =>
          renderSlotValue(
            value,
            document,
            componentRenderers,
            evaluateCondition,
            resolveLoopSource,
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
  key: string,
): ReactNode {
  const items = resolveLoopSource?.(node.source) ?? [];

  if (items.length === 0) {
    if (!node.empty) {
      return null;
    }
    return renderV2Node(
      node.empty,
      document,
      componentRenderers,
      evaluateCondition,
      resolveLoopSource,
      `${key}-empty`,
    );
  }

  return (
    <>
      {items.map((_, index) =>
        renderV2Node(
          node.child,
          document,
          componentRenderers,
          evaluateCondition,
          resolveLoopSource,
          `${key}-${index}`,
        ),
      )}
    </>
  );
}

function renderSlotValue(
  value: GameUiLayoutNodeV2 | GameUiLayoutNodeV2[] | undefined,
  document: GameUiDocumentV2,
  componentRenderers: Partial<Record<string, GameUiComponentRenderer>> | undefined,
  evaluateCondition: ((expr: string) => boolean) | undefined,
  resolveLoopSource: ((source: string) => unknown[]) | undefined,
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
    keyPrefix,
  );
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
