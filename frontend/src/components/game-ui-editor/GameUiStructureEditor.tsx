import { useEffect, useMemo, useState } from "react";
import type {
  GameUiAbsoluteNodeV2,
  GameUiComponentNode,
  GameUiDocumentV2,
  GameUiForEachNode,
  GameUiGridNodeV2,
  GameUiLayoutNodeV2,
  GameUiPlatform,
  GameUiSlotNode,
  GameUiStackNodeV2,
  GameUiStyleRecord,
  GameUiWhenNode,
} from "../../data/gameUi";

const COMPONENT_LIBRARY = [
  {
    id: "scene_header",
    propsSchema: {
      show_world_name: "boolean",
      show_location: "boolean",
      show_time_label: "boolean",
      show_player_identity: "boolean",
      show_visible_characters: "boolean",
      show_copy_button: "boolean",
      title_mode: "desktop|mobile",
    },
  },
  {
    id: "scene_focus",
    propsSchema: {
      show_avatar: "boolean",
      show_line: "boolean",
      avatar_variant: "string",
    },
  },
  {
    id: "character_bar",
    propsSchema: {
      empty_text: "string",
      max_items: "number",
    },
  },
  {
    id: "narration_card",
    propsSchema: {
      title: "string",
      show_copy_button: "boolean",
      empty_text: "string",
    },
  },
  {
    id: "message_list",
    propsSchema: {
      auto_scroll: "boolean",
      show_pending_state: "boolean",
      show_agent_reasoning: "boolean",
    },
  },
  {
    id: "input_composer",
    propsSchema: {
      placeholder: "string",
      submit_label: "string",
      editing_submit_label: "string",
      show_image_button: "boolean",
      show_audio_button: "boolean",
      show_session_meta: "boolean",
      enter_to_submit: "boolean",
    },
  },
  {
    id: "side_panel_tabs",
    propsSchema: {
      show_map_tab: "boolean",
      show_attribute_tabs: "boolean",
      empty_text: "string",
    },
  },
  {
    id: "floating_actions",
    propsSchema: {
      show_back: "boolean",
      show_debug: "boolean",
      show_settings: "boolean",
      layout: "row|column|wrap",
    },
  },
] as const;

type SupportedComponentId = (typeof COMPONENT_LIBRARY)[number]["id"];
type SupportedNodeType = GameUiLayoutNodeV2["type"];
type GameUiEditorPath = string[];

type GameUiStructureEditorProps = {
  platform: GameUiPlatform;
  document: GameUiDocumentV2;
  onChangeDocument: (document: GameUiDocumentV2) => void;
};

type JsonEditorFieldProps = {
  label: string;
  value: unknown;
  rows?: number;
  placeholder?: string;
  onCommit: (value: unknown) => void;
};

type ParentReference =
  | { kind: "children"; parent: GameUiGridNodeV2 | GameUiStackNodeV2 | GameUiAbsoluteNodeV2; index: number }
  | { kind: "child"; parent: GameUiWhenNode | GameUiForEachNode }
  | { kind: "empty"; parent: GameUiForEachNode }
  | { kind: "slot-single"; parent: GameUiComponentNode; slotName: string }
  | { kind: "slot-array"; parent: GameUiComponentNode; slotName: string; index: number };

function JsonEditorField({ label, value, rows = 6, placeholder, onCommit }: JsonEditorFieldProps) {
  const [draft, setDraft] = useState(() => stringifyJsonValue(value));
  const [parseError, setParseError] = useState<string | null>(null);

  useEffect(() => {
    setDraft(stringifyJsonValue(value));
    setParseError(null);
  }, [value]);

  function applyDraft() {
    const trimmed = draft.trim();
    if (!trimmed) {
      setParseError(null);
      onCommit(undefined);
      return;
    }

    try {
      onCommit(JSON.parse(trimmed));
      setParseError(null);
    } catch (error) {
      setParseError(error instanceof Error ? error.message : "Invalid JSON");
    }
  }

  return (
    <label className="editor-field">
      <span className="editor-field-label">{label}</span>
      <textarea
        value={draft}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={applyDraft}
        rows={rows}
        placeholder={placeholder}
        className="editor-field-input editor-field-textarea"
        spellCheck={false}
        style={{ fontFamily: "Consolas, 'Courier New', monospace", fontSize: 12 }}
      />
      {parseError ? <span className="text-muted" style={{ color: "var(--color-danger)" }}>{parseError}</span> : null}
    </label>
  );
}

export function GameUiStructureEditor({
  platform,
  document,
  onChangeDocument,
}: GameUiStructureEditorProps) {
  const [selectedPath, setSelectedPath] = useState<GameUiEditorPath>([]);
  const [newChildType, setNewChildType] = useState<SupportedNodeType>("component");
  const [newSlotName, setNewSlotName] = useState("content");
  const selectedNode = useMemo(
    () => getNodeAtPath(document.layout.root, selectedPath) ?? document.layout.root,
    [document.layout.root, selectedPath],
  );
  const selectionKey = useMemo(() => pathToKey(selectedPath), [selectedPath]);
  const componentHints = useMemo(
    () =>
      selectedNode.type === "component"
        ? COMPONENT_LIBRARY.find((entry) => entry.id === selectedNode.component)
        : null,
    [selectedNode],
  );

  useEffect(() => {
    if (getNodeAtPath(document.layout.root, selectedPath)) {
      return;
    }
    setSelectedPath([]);
  }, [document.layout.root, selectedPath]);

  function commit(mutator: (current: GameUiDocumentV2) => GameUiDocumentV2) {
    onChangeDocument(mutator(document));
  }

  function updateSelectedNode(mutator: (node: GameUiLayoutNodeV2) => GameUiLayoutNodeV2) {
    commit((current) => updateDocumentNode(current, selectedPath, mutator));
  }

  function replaceSelectedNodeType(nextType: SupportedNodeType) {
    updateSelectedNode((node) => createDefaultNode(nextType, pickComponentId(node)));
  }

  function addChildNode() {
    commit((current) =>
      appendChildNode(current, selectedPath, createDefaultNode(newChildType, pickComponentId(selectedNode))),
    );
  }

  function addSlotNode() {
    const slotName = newSlotName.trim();
    if (!slotName || selectedNode.type !== "component") {
      return;
    }
    commit((current) =>
      appendSlotNode(current, selectedPath, slotName, createDefaultNode(newChildType, pickComponentId(selectedNode))),
    );
  }

  function removeSelectedNode() {
    if (selectedPath.length === 0) {
      return;
    }
    commit((current) => removeDocumentNode(current, selectedPath));
    setSelectedPath(parentPathOf(selectedPath));
  }

  function moveSelectedNode(direction: -1 | 1) {
    commit((current) => moveDocumentNode(current, selectedPath, direction));
    setSelectedPath(shiftSiblingPath(selectedPath, direction));
  }

  function ensureEmptyBranch() {
    if (selectedNode.type !== "for_each" || selectedNode.empty) {
      return;
    }
    updateSelectedNode((node) => ({
      ...(node as GameUiForEachNode),
      empty: createDefaultNode("component", pickComponentId(node)),
    }));
  }

  function clearEmptyBranch() {
    if (selectedNode.type !== "for_each") {
      return;
    }
    updateSelectedNode((node) => {
      const next = structuredClone(node) as GameUiForEachNode;
      delete next.empty;
      return next;
    });
  }

  const canRemove = selectedPath.length > 0 && canRemovePath(document.layout.root, selectedPath);
  const canMoveUp = canMovePath(document.layout.root, selectedPath, -1);
  const canMoveDown = canMovePath(document.layout.root, selectedPath, 1);
  const supportsChildren = nodeSupportsChildren(selectedNode);

  return (
    <div
      className="editor-content"
      style={{
        marginTop: 12,
        border: "1px solid var(--color-border)",
        borderRadius: 16,
        padding: 12,
        display: "grid",
        gap: 12,
      }}
    >
      <div className="flex flex--items-center flex--justify-between world-editor-section-head" style={{ gap: 12 }}>
        <div style={{ display: "grid", gap: 6 }}>
          <div className="editor-field-label">结构化树编辑器</div>
          <div className="text-muted" style={{ fontSize: 12 }}>
            {platform === "desktop" ? "桌面端" : "移动端"} schema v2 组件树
          </div>
        </div>
        <div className="flex flex--gap-sm" style={{ flexWrap: "wrap" }}>
          <select
            value={newChildType}
            onChange={(event) => setNewChildType(event.target.value as SupportedNodeType)}
            className="editor-field-input editor-field-select"
            style={{ minWidth: 140 }}
          >
            <option value="component">component</option>
            <option value="stack">stack</option>
            <option value="grid">grid</option>
            <option value="absolute">absolute</option>
            <option value="slot">slot</option>
            <option value="when">when</option>
            <option value="for_each">for_each</option>
          </select>
          {supportsChildren ? (
            <button type="button" className="action-btn" onClick={addChildNode}>
              添加子节点
            </button>
          ) : null}
          {selectedNode.type === "component" ? (
            <>
              <input
                value={newSlotName}
                onChange={(event) => setNewSlotName(event.target.value)}
                className="editor-field-input"
                placeholder="slot name"
                style={{ minWidth: 140 }}
              />
              <button type="button" className="action-btn" onClick={addSlotNode}>
                Add Slot Node
              </button>
            </>
          ) : null}
        </div>
      </div>

      <div
        style={{
          display: "grid",
          gap: 12,
          gridTemplateColumns: "minmax(280px, 0.95fr) minmax(320px, 1.25fr)",
        }}
      >
        <div
          style={{
            border: "1px solid var(--color-border-light)",
            borderRadius: 14,
            padding: 12,
            background: "var(--color-surface-2)",
            display: "grid",
            gap: 8,
            alignContent: "start",
          }}
        >
          {renderTreeNode(document.layout.root, [], selectedPath, setSelectedPath)}
        </div>

        <div
          style={{
            border: "1px solid var(--color-border-light)",
            borderRadius: 14,
            padding: 12,
            display: "grid",
            gap: 12,
            alignContent: "start",
          }}
        >
          <div className="flex flex--items-center flex--justify-between" style={{ gap: 12 }}>
            <div>
              <div className="editor-field-label">{describeNode(selectedNode)}</div>
              <div className="text-muted" style={{ fontSize: 12 }}>{selectionKey === "__root" ? "layout.root" : selectionKey}</div>
            </div>
            <div className="flex flex--gap-sm" style={{ flexWrap: "wrap" }}>
              <button type="button" className="action-btn" onClick={() => moveSelectedNode(-1)} disabled={!canMoveUp}>
                上移
              </button>
              <button type="button" className="action-btn" onClick={() => moveSelectedNode(1)} disabled={!canMoveDown}>
                下移
              </button>
              <button type="button" className="action-btn action-btn--danger" onClick={removeSelectedNode} disabled={!canRemove}>
                删除节点
              </button>
            </div>
          </div>

          <div className="settings-form-grid" style={{ gap: 12 }}>
            <label className="editor-field">
              <span className="editor-field-label">节点类型</span>
              <select
                value={selectedNode.type}
                onChange={(event) => replaceSelectedNodeType(event.target.value as SupportedNodeType)}
                className="editor-field-input editor-field-select"
              >
                <option value="component">component</option>
                <option value="stack">stack</option>
                <option value="grid">grid</option>
                <option value="absolute">absolute</option>
                <option value="slot">slot</option>
                <option value="when">when</option>
                <option value="for_each">for_each</option>
              </select>
            </label>
            <label className="editor-field" style={{ justifyContent: "center" }}>
              <span className="editor-field-label">Visible</span>
              <input
                type="checkbox"
                checked={selectedNode.visible !== false}
                onChange={(event) => updateSelectedNode((node) => ({ ...node, visible: event.target.checked }))}
              />
            </label>
          </div>

          <div className="settings-form-grid" style={{ gap: 12 }}>
            {[
                { key: "id", label: "节点 ID" },
                { key: "class_name", label: "类名" },
                { key: "area", label: "网格区域" },
                { key: "width", label: "宽度" },
                { key: "height", label: "高度" },
                { key: "min_width", label: "最小宽度" },
                { key: "min_height", label: "最小高度" },
                { key: "max_width", label: "最大宽度" },
                { key: "max_height", label: "最大高度" },
                { key: "padding", label: "内边距" },
                { key: "margin", label: "外边距" },
                { key: "align", label: "对齐" },
              { key: "justify", label: "Justify" },
            ].map((field) => (
              <label key={field.key} className="editor-field">
                <span className="editor-field-label">{field.label}</span>
                <input
                  value={String((selectedNode as Record<string, unknown>)[field.key] ?? "")}
                  onChange={(event) =>
                    updateSelectedNode((node) =>
                      setOptionalField(node, field.key, event.target.value.trim() ? event.target.value : undefined),
                    )
                  }
                  className="editor-field-input"
                />
              </label>
            ))}
          </div>

          {selectedNode.type === "component" ? (
            <>
              <div className="settings-form-grid" style={{ gap: 12 }}>
                <label className="editor-field">
                  <span className="editor-field-label">组件</span>
                  <select
                    value={selectedNode.component}
                    onChange={(event) =>
                      updateSelectedNode((node) => ({
                        ...(node as GameUiComponentNode),
                        component: event.target.value,
                      }))
                    }
                    className="editor-field-input editor-field-select"
                  >
                    {COMPONENT_LIBRARY.map((entry) => (
                      <option key={entry.id} value={entry.id}>
                        {entry.id}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="editor-field">
                  <span className="editor-field-label">Variant</span>
                  <input
                    value={selectedNode.variant ?? ""}
                    onChange={(event) =>
                      updateSelectedNode((node) => setOptionalField(node, "variant", event.target.value.trim() ? event.target.value : undefined))
                    }
                    className="editor-field-input"
                  />
                </label>
              </div>
              {componentHints ? (
                <div className="text-muted" style={{ fontSize: 12 }}>
                  {Object.entries(componentHints.propsSchema)
                    .map(([key, value]) => `${key}: ${value}`)
                    .join(" | ")}
                </div>
              ) : null}
              <JsonEditorField
                label="Props"
                value={selectedNode.props}
                placeholder='{"placeholder":"Type here","show_audio_button":true}'
                onCommit={(value) =>
                  updateSelectedNode((node) => setOptionalField(node, "props", value as Record<string, unknown> | undefined))
                }
              />
              <JsonEditorField
                label="Anchor"
                value={selectedNode.anchor}
                placeholder='{"top":"16px","right":"16px"}'
                onCommit={(value) =>
                  updateSelectedNode((node) => setOptionalField(node, "anchor", value as Record<string, unknown> | undefined))
                }
              />
            </>
          ) : null}

          {selectedNode.type === "grid" ? (
            <>
              <label className="editor-field">
                <span className="editor-field-label">间距</span>
                <input
                  value={selectedNode.gap ?? ""}
                  onChange={(event) => updateSelectedNode((node) => setOptionalField(node, "gap", event.target.value.trim() ? event.target.value : undefined))}
                  className="editor-field-input"
                />
              </label>
              <JsonEditorField
                label="Columns"
                value={selectedNode.columns}
                placeholder='["1fr","2fr"]'
                onCommit={(value) => updateSelectedNode((node) => setOptionalField(node, "columns", value as string[] | undefined))}
              />
              <JsonEditorField
                label="Rows"
                value={selectedNode.rows}
                placeholder='["auto","1fr"]'
                onCommit={(value) => updateSelectedNode((node) => setOptionalField(node, "rows", value as string[] | undefined))}
              />
              <JsonEditorField
                label="Areas"
                value={selectedNode.areas}
                placeholder='[["header","header"],["scene","chat"]]'
                onCommit={(value) => updateSelectedNode((node) => setOptionalField(node, "areas", value as string[][] | undefined))}
              />
            </>
          ) : null}

          {selectedNode.type === "stack" ? (
            <div className="settings-form-grid" style={{ gap: 12 }}>
              <label className="editor-field">
                <span className="editor-field-label">方向</span>
                <select
                  value={selectedNode.direction ?? "vertical"}
                  onChange={(event) =>
                    updateSelectedNode((node) => ({
                      ...(node as GameUiStackNodeV2),
                      direction: event.target.value === "horizontal" ? "horizontal" : "vertical",
                    }))
                  }
                  className="editor-field-input editor-field-select"
                >
                  <option value="vertical">vertical</option>
                  <option value="horizontal">horizontal</option>
                </select>
              </label>
              <label className="editor-field">
                <span className="editor-field-label">Gap</span>
                <input
                  value={selectedNode.gap ?? ""}
                  onChange={(event) => updateSelectedNode((node) => setOptionalField(node, "gap", event.target.value.trim() ? event.target.value : undefined))}
                  className="editor-field-input"
                />
              </label>
              <label className="editor-field" style={{ justifyContent: "center" }}>
                <span className="editor-field-label">换行</span>
                <input
                  type="checkbox"
                  checked={selectedNode.wrap ?? false}
                  onChange={(event) => updateSelectedNode((node) => ({ ...(node as GameUiStackNodeV2), wrap: event.target.checked }))}
                />
              </label>
            </div>
          ) : null}

          {selectedNode.type === "slot" ? (
            <label className="editor-field">
              <span className="editor-field-label">Slot Name</span>
              <input
                value={selectedNode.name}
                onChange={(event) =>
                  updateSelectedNode((node) => ({
                    ...(node as GameUiSlotNode),
                    name: event.target.value,
                  }))
                }
                className="editor-field-input"
              />
            </label>
          ) : null}

          {selectedNode.type === "when" ? (
            <label className="editor-field">
              <span className="editor-field-label">Expression</span>
              <input
                value={selectedNode.expr}
                onChange={(event) =>
                  updateSelectedNode((node) => ({
                    ...(node as GameUiWhenNode),
                    expr: event.target.value,
                  }))
                }
                className="editor-field-input"
              />
            </label>
          ) : null}

          {selectedNode.type === "for_each" ? (
            <>
              <div className="settings-form-grid" style={{ gap: 12 }}>
                <label className="editor-field">
                  <span className="editor-field-label">数据源</span>
                  <input
                    value={selectedNode.source}
                    onChange={(event) =>
                      updateSelectedNode((node) => ({
                        ...(node as GameUiForEachNode),
                        source: event.target.value,
                      }))
                    }
                    className="editor-field-input"
                  />
                </label>
                <label className="editor-field">
                  <span className="editor-field-label">Item Alias</span>
                  <input
                    value={selectedNode.item_as}
                    onChange={(event) =>
                      updateSelectedNode((node) => ({
                        ...(node as GameUiForEachNode),
                        item_as: event.target.value,
                      }))
                    }
                    className="editor-field-input"
                  />
                </label>
                <label className="editor-field">
                  <span className="editor-field-label">索引别名</span>
                  <input
                    value={selectedNode.index_as ?? ""}
                    onChange={(event) =>
                      updateSelectedNode((node) => setOptionalField(node, "index_as", event.target.value.trim() ? event.target.value : undefined))
                    }
                    className="editor-field-input"
                  />
                </label>
              </div>
              <div className="flex flex--gap-sm" style={{ flexWrap: "wrap" }}>
                <button type="button" className="action-btn" onClick={ensureEmptyBranch} disabled={!!selectedNode.empty}>
                  Add Empty Branch
                </button>
                <button type="button" className="action-btn" onClick={clearEmptyBranch} disabled={!selectedNode.empty}>
                  Clear Empty Branch
                </button>
              </div>
            </>
          ) : null}

          <JsonEditorField
            label="Style"
            value={selectedNode.style as GameUiStyleRecord | undefined}
            placeholder='{"background":"rgba(0,0,0,0.4)","border_radius":"16px"}'
            onCommit={(value) =>
              updateSelectedNode((node) => setOptionalField(node, "style", value as GameUiStyleRecord | undefined))
            }
          />
        </div>
      </div>
    </div>
  );
}

function renderTreeNode(
  node: GameUiLayoutNodeV2,
  path: GameUiEditorPath,
  selectedPath: GameUiEditorPath,
  onSelect: (path: GameUiEditorPath) => void,
) {
  const isSelected = pathToKey(path) === pathToKey(selectedPath);
  const branches: Array<{ label: string; path: GameUiEditorPath; node: GameUiLayoutNodeV2 }> = [];

  if (nodeSupportsChildren(node)) {
    node.children?.forEach((child, index) => {
      branches.push({
        label: `children[${index}]`,
        path: [...path, `children:${index}`],
        node: child,
      });
    });
  }

  if (node.type === "component") {
    Object.entries(node.slots ?? {}).forEach(([slotName, slotValue]) => {
      if (Array.isArray(slotValue)) {
        slotValue.forEach((child, index) => {
          branches.push({
            label: `${slotName}[${index}]`,
            path: [...path, `slot:${slotName}:${index}`],
            node: child,
          });
        });
        return;
      }

      branches.push({
        label: slotName,
        path: [...path, `slot:${slotName}`],
        node: slotValue,
      });
    });
  }

  if (node.type === "when" || node.type === "for_each") {
    branches.push({
      label: "child",
      path: [...path, "child"],
      node: node.child,
    });
  }

  if (node.type === "for_each" && node.empty) {
    branches.push({
      label: "empty",
      path: [...path, "empty"],
      node: node.empty,
    });
  }

  return (
    <div key={pathToKey(path)} style={{ display: "grid", gap: 8 }}>
      <button
        type="button"
        className={`action-btn${isSelected ? " action-btn--accent" : ""}`}
        onClick={() => onSelect(path)}
        style={{ justifyContent: "space-between", textAlign: "left" }}
      >
        <span>{describeNode(node)}</span>
        <span className="text-muted" style={{ fontSize: 11 }}>
          {path.length === 0 ? "root" : path[path.length - 1]}
        </span>
      </button>
      {branches.length > 0 ? (
        <div style={{ borderLeft: "1px solid var(--color-border)", marginLeft: 8, paddingLeft: 12, display: "grid", gap: 8 }}>
          {branches.map((branch) => (
            <div key={pathToKey(branch.path)} style={{ display: "grid", gap: 6 }}>
              <div className="text-muted" style={{ fontSize: 11 }}>{branch.label}</div>
              {renderTreeNode(branch.node, branch.path, selectedPath, onSelect)}
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
}

function createDefaultNode(type: SupportedNodeType, preferredComponent?: string): GameUiLayoutNodeV2 {
  switch (type) {
    case "grid":
      return {
        type: "grid",
        columns: ["1fr"],
        rows: ["auto"],
        gap: "12px",
        children: [],
      };
    case "stack":
      return {
        type: "stack",
        direction: "vertical",
        gap: "12px",
        children: [],
      };
    case "absolute":
      return {
        type: "absolute",
        children: [],
      };
    case "slot":
      return {
        type: "slot",
        name: "content",
      };
    case "when":
      return {
        type: "when",
        expr: "$capabilities.supports_hover",
        child: createDefaultNode("component", preferredComponent),
      };
    case "for_each":
      return {
        type: "for_each",
        source: "$session.visible_characters",
        item_as: "item",
        child: createDefaultNode("component", preferredComponent),
      };
    case "component":
    default:
      return {
        type: "component",
        component: (preferredComponent as SupportedComponentId | undefined) ?? "scene_header",
        props: {},
      };
  }
}

function pickComponentId(node: GameUiLayoutNodeV2): string | undefined {
  return node.type === "component" ? node.component : undefined;
}

function pathToKey(path: GameUiEditorPath): string {
  return path.length > 0 ? path.join("/") : "__root";
}

function parentPathOf(path: GameUiEditorPath): GameUiEditorPath {
  return path.slice(0, -1);
}

function shiftSiblingPath(path: GameUiEditorPath, direction: -1 | 1): GameUiEditorPath {
  const next = [...path];
  const last = next[next.length - 1];
  if (!last) {
    return next;
  }
  const decoded = decodePathToken(last);
  if (decoded.kind === "children") {
    next[next.length - 1] = `children:${decoded.index + direction}`;
  }
  if (decoded.kind === "slot-array") {
    next[next.length - 1] = `slot:${decoded.slotName}:${decoded.index + direction}`;
  }
  return next;
}

function getNodeAtPath(root: GameUiLayoutNodeV2, path: GameUiEditorPath): GameUiLayoutNodeV2 | null {
  let current: GameUiLayoutNodeV2 | null = root;
  for (const token of path) {
    if (!current) {
      return null;
    }
    const decoded = decodePathToken(token);
    switch (decoded.kind) {
      case "children":
        if (!nodeSupportsChildren(current)) {
          return null;
        }
        current = current.children?.[decoded.index] ?? null;
        break;
      case "child":
        current = current.type === "when" || current.type === "for_each" ? current.child : null;
        break;
      case "empty":
        current = current.type === "for_each" ? current.empty ?? null : null;
        break;
      case "slot-single":
        current =
          current.type === "component" && current.slots
            ? ((current.slots[decoded.slotName] as GameUiLayoutNodeV2 | undefined) ?? null)
            : null;
        break;
      case "slot-array":
        if (current.type === "component" && current.slots) {
          const slots = current.slots as Record<string, GameUiLayoutNodeV2 | GameUiLayoutNodeV2[] | undefined>;
          const slotValue = slots[decoded.slotName];
          current = Array.isArray(slotValue) ? slotValue[decoded.index] ?? null : null;
        } else {
          current = null;
        }
        break;
    }
  }
  return current;
}

function updateDocumentNode(
  document: GameUiDocumentV2,
  path: GameUiEditorPath,
  mutator: (node: GameUiLayoutNodeV2) => GameUiLayoutNodeV2,
): GameUiDocumentV2 {
  const next = structuredClone(document) as GameUiDocumentV2;
  if (path.length === 0) {
    next.layout.root = mutator(next.layout.root);
    return next;
  }

  const parentReference = getParentReference(next.layout.root, path);
  if (!parentReference) {
    return document;
  }

  switch (parentReference.kind) {
    case "children":
      parentReference.parent.children = parentReference.parent.children?.map((child, index) =>
        index === parentReference.index ? mutator(child) : child,
      );
      break;
    case "child":
      parentReference.parent.child = mutator(parentReference.parent.child);
      break;
    case "empty":
      parentReference.parent.empty = parentReference.parent.empty ? mutator(parentReference.parent.empty) : parentReference.parent.empty;
      break;
    case "slot-single":
      parentReference.parent.slots = {
        ...(parentReference.parent.slots ?? {}),
        [parentReference.slotName]: mutator(parentReference.parent.slots?.[parentReference.slotName] as GameUiLayoutNodeV2),
      };
      break;
    case "slot-array":
      parentReference.parent.slots = {
        ...(parentReference.parent.slots ?? {}),
        [parentReference.slotName]: ((parentReference.parent.slots?.[parentReference.slotName] as GameUiLayoutNodeV2[]) ?? []).map((child, index) =>
          index === parentReference.index ? mutator(child) : child,
        ),
      };
      break;
  }

  return next;
}

function appendChildNode(
  document: GameUiDocumentV2,
  path: GameUiEditorPath,
  child: GameUiLayoutNodeV2,
): GameUiDocumentV2 {
  return updateDocumentNode(document, path, (node) => {
    if (!nodeSupportsChildren(node)) {
      return node;
    }
    return {
      ...node,
      children: [...(node.children ?? []), child],
    };
  });
}

function appendSlotNode(
  document: GameUiDocumentV2,
  path: GameUiEditorPath,
  slotName: string,
  child: GameUiLayoutNodeV2,
): GameUiDocumentV2 {
  return updateDocumentNode(document, path, (node) => {
    if (node.type !== "component") {
      return node;
    }
    const currentValue = node.slots?.[slotName];
    const nextSlots = { ...(node.slots ?? {}) } as Record<string, GameUiLayoutNodeV2 | GameUiLayoutNodeV2[]>;
    if (!currentValue) {
      nextSlots[slotName] = child;
    } else if (Array.isArray(currentValue)) {
      nextSlots[slotName] = [...currentValue, child];
    } else {
      nextSlots[slotName] = [currentValue, child];
    }
    return {
      ...node,
      slots: nextSlots,
    };
  });
}

function removeDocumentNode(document: GameUiDocumentV2, path: GameUiEditorPath): GameUiDocumentV2 {
  const next = structuredClone(document) as GameUiDocumentV2;
  const parentReference = getParentReference(next.layout.root, path);
  if (!parentReference) {
    return document;
  }

  switch (parentReference.kind) {
    case "children":
      parentReference.parent.children = (parentReference.parent.children ?? []).filter((_, index) => index !== parentReference.index);
      break;
    case "slot-single": {
      const nextSlots = { ...(parentReference.parent.slots ?? {}) };
      delete nextSlots[parentReference.slotName];
      parentReference.parent.slots = nextSlots;
      break;
    }
    case "slot-array": {
      const remaining = ((parentReference.parent.slots?.[parentReference.slotName] as GameUiLayoutNodeV2[]) ?? []).filter(
        (_, index) => index !== parentReference.index,
      );
      const nextSlots = { ...(parentReference.parent.slots ?? {}) };
      if (remaining.length === 0) {
        delete nextSlots[parentReference.slotName];
      } else if (remaining.length === 1) {
        nextSlots[parentReference.slotName] = remaining[0];
      } else {
        nextSlots[parentReference.slotName] = remaining;
      }
      parentReference.parent.slots = nextSlots;
      break;
    }
    case "child":
    case "empty":
      return document;
  }

  return next;
}

function moveDocumentNode(
  document: GameUiDocumentV2,
  path: GameUiEditorPath,
  direction: -1 | 1,
): GameUiDocumentV2 {
  const next = structuredClone(document) as GameUiDocumentV2;
  const parentReference = getParentReference(next.layout.root, path);
  if (!parentReference) {
    return document;
  }

  switch (parentReference.kind) {
    case "children": {
      const items = [...(parentReference.parent.children ?? [])];
      const targetIndex = parentReference.index + direction;
      if (targetIndex < 0 || targetIndex >= items.length) {
        return document;
      }
      const [item] = items.splice(parentReference.index, 1);
      items.splice(targetIndex, 0, item);
      parentReference.parent.children = items;
      return next;
    }
    case "slot-array": {
      const items = [...(((parentReference.parent.slots?.[parentReference.slotName] as GameUiLayoutNodeV2[]) ?? []))];
      const targetIndex = parentReference.index + direction;
      if (targetIndex < 0 || targetIndex >= items.length) {
        return document;
      }
      const [item] = items.splice(parentReference.index, 1);
      items.splice(targetIndex, 0, item);
      parentReference.parent.slots = {
        ...(parentReference.parent.slots ?? {}),
        [parentReference.slotName]: items,
      };
      return next;
    }
    case "child":
    case "empty":
    case "slot-single":
      return document;
  }
}

function canRemovePath(root: GameUiLayoutNodeV2, path: GameUiEditorPath): boolean {
  const reference = getParentReference(root, path);
  return reference?.kind === "children" || reference?.kind === "slot-single" || reference?.kind === "slot-array";
}

function canMovePath(root: GameUiLayoutNodeV2, path: GameUiEditorPath, direction: -1 | 1): boolean {
  const reference = getParentReference(root, path);
  if (!reference) {
    return false;
  }
  if (reference.kind === "children") {
    const targetIndex = reference.index + direction;
    return targetIndex >= 0 && targetIndex < (reference.parent.children?.length ?? 0);
  }
  if (reference.kind === "slot-array") {
    const slotItems = (reference.parent.slots?.[reference.slotName] as GameUiLayoutNodeV2[]) ?? [];
    const targetIndex = reference.index + direction;
    return targetIndex >= 0 && targetIndex < slotItems.length;
  }
  return false;
}

function getParentReference(root: GameUiLayoutNodeV2, path: GameUiEditorPath): ParentReference | null {
  if (path.length === 0) {
    return null;
  }
  const parentPath = parentPathOf(path);
  const parentNode = getNodeAtPath(root, parentPath);
  const token = decodePathToken(path[path.length - 1]);
  if (!parentNode) {
    return null;
  }

  switch (token.kind) {
    case "children":
      return nodeSupportsChildren(parentNode) ? { kind: "children", parent: parentNode, index: token.index } : null;
    case "child":
      return parentNode.type === "when" || parentNode.type === "for_each" ? { kind: "child", parent: parentNode } : null;
    case "empty":
      return parentNode.type === "for_each" ? { kind: "empty", parent: parentNode } : null;
    case "slot-single":
      return parentNode.type === "component" ? { kind: "slot-single", parent: parentNode, slotName: token.slotName } : null;
    case "slot-array":
      return parentNode.type === "component" ? { kind: "slot-array", parent: parentNode, slotName: token.slotName, index: token.index } : null;
  }
}

function nodeSupportsChildren(
  node: GameUiLayoutNodeV2,
): node is GameUiGridNodeV2 | GameUiStackNodeV2 | GameUiAbsoluteNodeV2 {
  return node.type === "grid" || node.type === "stack" || node.type === "absolute";
}

function describeNode(node: GameUiLayoutNodeV2): string {
  switch (node.type) {
    case "component":
      return `component:${node.component}`;
    case "slot":
      return `slot:${node.name}`;
    case "when":
      return `when:${node.expr || "condition"}`;
    case "for_each":
      return `for_each:${node.source || "items"}`;
    case "text":
      return `text:${node.text || "text"}`;
    case "image":
      return `image:${node.src || "image"}`;
    case "badge":
      return `badge:${node.text || "badge"}`;
    case "button":
      return `button:${node.label || "button"}`;
    case "checkbox":
      return `checkbox:${node.label || "checkbox"}`;
    case "grid":
    case "stack":
    case "absolute":
      return node.type;
    default: {
      const exhaustive: never = node;
      return exhaustive;
    }
  }
}

function stringifyJsonValue(value: unknown): string {
  if (value === undefined) {
    return "";
  }
  return `${JSON.stringify(value, null, 2)}\n`;
}

function setOptionalField<T extends Record<string, unknown>>(node: T, key: string, value: unknown): T {
  const next = { ...node } as Record<string, unknown>;
  if (
    value === undefined
    || value === null
    || (typeof value === "string" && !value.trim())
    || (Array.isArray(value) && value.length === 0)
    || (typeof value === "object" && !Array.isArray(value) && Object.keys(value as Record<string, unknown>).length === 0)
  ) {
    delete next[key];
  } else {
    next[key] = value;
  }
  return next as T;
}

function decodePathToken(token: string):
  | { kind: "children"; index: number }
  | { kind: "child" }
  | { kind: "empty" }
  | { kind: "slot-single"; slotName: string }
  | { kind: "slot-array"; slotName: string; index: number } {
  if (token === "child") {
    return { kind: "child" };
  }
  if (token === "empty") {
    return { kind: "empty" };
  }
  if (token.startsWith("children:")) {
    return { kind: "children", index: Number(token.slice("children:".length)) };
  }
  if (token.startsWith("slot:")) {
    const [, slotName, index] = token.split(":");
    if (index !== undefined) {
      return { kind: "slot-array", slotName, index: Number(index) };
    }
    return { kind: "slot-single", slotName };
  }
  return { kind: "child" };
}
