import { useEffect, useMemo, useState } from "react";
import {
  ArrowDown,
  ArrowUp,
  Box,
  Columns3,
  CornerDownRight,
  GitBranch,
  LayoutGrid,
  Layers,
  Plus,
  Repeat,
  Rows3,
  SquareStack,
  Trash2,
  type LucideIcon,
} from "lucide-react";
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
import "./GameUiStructureEditor.css";

type NodeFamily = "content" | "layout" | "logic";

const COMPONENT_LIBRARY = [
  {
    id: "scene_header",
    label: "顶栏 scene_header",
    description: "页面顶部信息条：世界名、地点、时间、当前玩家等。",
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
    label: "立绘焦点 scene_focus",
    description: "当前发言者的立绘与台词展示区。",
    propsSchema: {
      show_avatar: "boolean",
      show_line: "boolean",
      avatar_variant: "string",
    },
  },
  {
    id: "character_bar",
    label: "在场角色 character_bar",
    description: "显示当前场景在场角色的列表。",
    propsSchema: {
      empty_text: "string",
      max_items: "number",
    },
  },
  {
    id: "narration_card",
    label: "旁白卡 narration_card",
    description: "展示旁白 / 场景描述文本。",
    propsSchema: {
      title: "string",
      show_copy_button: "boolean",
      empty_text: "string",
    },
  },
  {
    id: "message_list",
    label: "对话列表 message_list",
    description: "滚动显示对话消息流。",
    propsSchema: {
      auto_scroll: "boolean",
      show_pending_state: "boolean",
      show_agent_reasoning: "boolean",
    },
  },
  {
    id: "input_composer",
    label: "输入框 input_composer",
    description: "玩家发言输入区与发送按钮。",
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
    label: "侧边栏 side_panel_tabs",
    description: "侧边状态面板，含地图 / 属性等标签页。",
    propsSchema: {
      show_map_tab: "boolean",
      show_attribute_tabs: "boolean",
      empty_text: "string",
    },
  },
  {
    id: "floating_actions",
    label: "悬浮按钮 floating_actions",
    description: "返回 / 调试 / 设置等悬浮操作按钮。",
    propsSchema: {
      show_back: "boolean",
      show_debug: "boolean",
      show_settings: "boolean",
      layout: "row|column|wrap",
    },
  },
] as const;

// 节点类型的中文名、说明、配色族与图标，供树、下拉框与检查器复用。
// 仅覆盖编辑器可创建的类型；其余类型（text/image/badge 等）回退到原始类型名。
const NODE_TYPE_META: Partial<
  Record<SupportedNodeType, { label: string; hint: string; family: NodeFamily; icon: LucideIcon }>
> = {
  component: { label: "组件 component", hint: "内置功能块（顶栏、立绘、对话列表等）", family: "content", icon: Box },
  stack: { label: "堆叠 stack", hint: "纵向或横向依次排列子节点", family: "layout", icon: SquareStack },
  grid: { label: "网格 grid", hint: "用列 / 行 / 区域定义网格布局", family: "layout", icon: LayoutGrid },
  absolute: { label: "浮层 absolute", hint: "绝对定位的覆盖层容器", family: "layout", icon: Layers },
  slot: { label: "插槽 slot", hint: "组件内具名的插入位", family: "logic", icon: CornerDownRight },
  when: { label: "条件 when", hint: "按表达式决定是否渲染子节点", family: "logic", icon: GitBranch },
  for_each: { label: "循环 for_each", hint: "按数据源数组重复渲染子节点", family: "logic", icon: Repeat },
};

function nodeTypeLabel(type: SupportedNodeType): string {
  return NODE_TYPE_META[type]?.label ?? type;
}

function nodeTypeHint(type: SupportedNodeType): string {
  return NODE_TYPE_META[type]?.hint ?? "";
}

function nodeFamily(type: SupportedNodeType): NodeFamily {
  return NODE_TYPE_META[type]?.family ?? "content";
}

function nodeIcon(type: SupportedNodeType): LucideIcon {
  return NODE_TYPE_META[type]?.icon ?? Box;
}

const NODE_TYPE_ORDER: SupportedNodeType[] = [
  "component",
  "stack",
  "grid",
  "absolute",
  "slot",
  "when",
  "for_each",
];

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
  const [newSlotName, setNewSlotName] = useState("");
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
  const selectedFamily = nodeFamily(selectedNode.type);
  const SelectedIcon = nodeIcon(selectedNode.type);

  return (
    <div className="gst-root">
      <div className="gst-head">
        <div className="gst-head-title">
          <Layers size={17} />
          界面结构
        </div>
        <div className="gst-head-sub">
          {platform === "desktop" ? "桌面端" : "移动端"}界面 · schema v2 组件树
        </div>
        <div className="gst-head-help">
          可视化搭建游戏页面：左侧是页面的结构树，点任意节点即可在右侧改它的属性，不必手写下方 JSON。
          节点按职能分三类，颜色对应——
        </div>
        <div className="gst-legend">
          <span className="gst-legend-item">
            <span className="gst-legend-dot" data-family="content" />
            内容块（顶栏、立绘、对话列表等真正显示的内容）
          </span>
          <span className="gst-legend-item">
            <span className="gst-legend-dot" data-family="layout" />
            布局容器（堆叠 / 网格 / 浮层，负责排版）
          </span>
          <span className="gst-legend-item">
            <span className="gst-legend-dot" data-family="logic" />
            逻辑节点（条件 / 循环 / 插槽，负责动态显示）
          </span>
        </div>
      </div>

      <div className="gst-body">
        {/* 左：结构树 + 底部「新增节点」工具条 */}
        <div className="gst-pane">
          <div className="gst-pane-head">
            <Layers size={13} />
            结构树
          </div>
          <div className="gst-tree">
            {renderTreeNode(document.layout.root, [], selectedPath, setSelectedPath)}
          </div>

          <div className="gst-add">
            <div className="gst-add-title">新增节点</div>
            <div className="gst-add-row">
              <select
                value={newChildType}
                onChange={(event) => setNewChildType(event.target.value as SupportedNodeType)}
                className="editor-field-input editor-field-select"
                style={{ minWidth: 150, flex: "1 1 auto" }}
                aria-label="选择要新增的节点类型"
              >
                {NODE_TYPE_ORDER.map((type) => (
                  <option key={type} value={type}>
                    {nodeTypeLabel(type)}
                  </option>
                ))}
              </select>
              {supportsChildren ? (
                <button type="button" className="action-btn" onClick={addChildNode}>
                  <Plus size={15} style={{ marginRight: 4 }} />
                  添加到子节点
                </button>
              ) : null}
            </div>
            {selectedNode.type === "component" ? (
              <div className="gst-add-row">
                <input
                  value={newSlotName}
                  onChange={(event) => setNewSlotName(event.target.value)}
                  className="editor-field-input"
                  placeholder="插槽名称，如 content"
                  style={{ minWidth: 150, flex: "1 1 auto" }}
                  aria-label="插槽名称"
                />
                <button type="button" className="action-btn" onClick={addSlotNode}>
                  <Plus size={15} style={{ marginRight: 4 }} />
                  添加到插槽
                </button>
              </div>
            ) : null}
            <div className="gst-add-hint">
              {supportsChildren
                ? "新节点会加到当前选中节点的下一层。"
                : selectedNode.type === "component"
                  ? "组件不放子节点，但可以往它的插槽里添加。"
                  : `当前选中的是「${nodeTypeLabel(selectedNode.type)}」，它不能直接放子节点。先在上方选中一个堆叠 / 网格 / 浮层容器再添加。`}
            </div>
          </div>
        </div>

        {/* 右：检查器 */}
        <div className="gst-pane">
          <div className="gst-pane-head">
            <SquareStack size={13} />
            节点属性
          </div>
          <div className="gst-inspector">
            <div className="gst-identity" data-family={selectedFamily}>
              <span className="gst-identity-icon">
                <SelectedIcon size={18} />
              </span>
              <div className="gst-identity-text">
                <div className="gst-identity-name">{nodeDisplayName(selectedNode)}</div>
                <div className="gst-identity-path">{selectionKey === "__root" ? "layout.root（根节点）" : selectionKey}</div>
              </div>
              <div className="gst-identity-actions">
                <button
                  type="button"
                  className="gst-icon-btn"
                  onClick={() => moveSelectedNode(-1)}
                  disabled={!canMoveUp}
                  title="上移"
                  aria-label="上移"
                >
                  <ArrowUp size={16} />
                </button>
                <button
                  type="button"
                  className="gst-icon-btn"
                  onClick={() => moveSelectedNode(1)}
                  disabled={!canMoveDown}
                  title="下移"
                  aria-label="下移"
                >
                  <ArrowDown size={16} />
                </button>
                <button
                  type="button"
                  className="gst-icon-btn"
                  data-tone="danger"
                  onClick={removeSelectedNode}
                  disabled={!canRemove}
                  title="删除节点"
                  aria-label="删除节点"
                >
                  <Trash2 size={16} />
                </button>
              </div>
            </div>

            <div className="gst-group">
              <div className="gst-group-title">基本</div>
              <div className="gst-grid-2">
                <label className="editor-field">
                  <span className="editor-field-label">节点类型（替换当前节点）</span>
                  <select
                    value={selectedNode.type}
                    onChange={(event) => replaceSelectedNodeType(event.target.value as SupportedNodeType)}
                    className="editor-field-input editor-field-select"
                  >
                    {NODE_TYPE_ORDER.map((type) => (
                      <option key={type} value={type}>
                        {nodeTypeLabel(type)}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="editor-field">
                  <span className="editor-field-label">是否显示</span>
                  <select
                    value={selectedNode.visible === false ? "hidden" : "shown"}
                    onChange={(event) =>
                      updateSelectedNode((node) => ({ ...node, visible: event.target.value === "shown" }))
                    }
                    className="editor-field-input editor-field-select"
                  >
                    <option value="shown">显示</option>
                    <option value="hidden">隐藏</option>
                  </select>
                </label>
              </div>
              <div className="gst-field-hint">
                {nodeTypeHint(selectedNode.type)}。切换类型会把当前节点替换为新类型并重置其内容。
              </div>
            </div>

            <div className="gst-group">
              <div className="gst-group-title">外观（接样式表）</div>
              <label className="editor-field">
                <span className="editor-field-label">类名 class_name</span>
                <input
                  value={String((selectedNode as Record<string, unknown>).class_name ?? "")}
                  onChange={(event) =>
                    updateSelectedNode((node) =>
                      setOptionalField(node, "class_name", event.target.value.trim() ? event.target.value : undefined),
                    )
                  }
                  className="editor-field-input"
                  placeholder="例如 poetry-topbar"
                />
              </label>
              <div className="gst-field-hint">
                给节点起一个类名，再到上方 Raw JSONC 的 custom_css 里写 <code>.类名 {"{…}"}</code> 即可美化它——
                飞花令等精致界面主要靠这一步，而不是逐个填下面的尺寸。
              </div>
              <label className="editor-field">
                <span className="editor-field-label">节点 ID（可选，用于定位）</span>
                <input
                  value={String((selectedNode as Record<string, unknown>).id ?? "")}
                  onChange={(event) =>
                    updateSelectedNode((node) =>
                      setOptionalField(node, "id", event.target.value.trim() ? event.target.value : undefined),
                    )
                  }
                  className="editor-field-input"
                />
              </label>
            </div>

            <div className="gst-group">
              <div className="gst-group-title">布局与尺寸（一般留空，由样式表控制）</div>
              <div className="settings-form-grid" style={{ gap: 12 }}>
                {[
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
              { key: "justify", label: "主轴对齐" },
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
                        {entry.label}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="editor-field">
                  <span className="editor-field-label">变体 variant</span>
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
                <div className="text-muted" style={{ fontSize: 12, lineHeight: 1.6 }}>
                  <div>{componentHints.description}</div>
                  <div>
                    可用属性：
                    {Object.entries(componentHints.propsSchema)
                      .map(([key, value]) => `${key}: ${value}`)
                      .join(" | ")}
                  </div>
                </div>
              ) : null}
              <JsonEditorField
                label="属性 Props（JSON）"
                value={selectedNode.props}
                placeholder='{"placeholder":"请输入","show_audio_button":true}'
                onCommit={(value) =>
                  updateSelectedNode((node) => setOptionalField(node, "props", value as Record<string, unknown> | undefined))
                }
              />
              <JsonEditorField
                label="锚点 Anchor（JSON）"
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
                <span className="editor-field-label">间距 gap</span>
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
              <span className="editor-field-label">插槽名称</span>
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
              <span className="editor-field-label">条件表达式</span>
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
                  <span className="editor-field-label">元素别名</span>
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
                  添加空数据分支
                </button>
                <button type="button" className="action-btn" onClick={clearEmptyBranch} disabled={!selectedNode.empty}>
                  移除空数据分支
                </button>
              </div>
            </>
          ) : null}

          <JsonEditorField
            label="样式 Style（JSON）"
            value={selectedNode.style as GameUiStyleRecord | undefined}
            placeholder='{"background":"rgba(0,0,0,0.4)","border_radius":"16px"}'
            onCommit={(value) =>
              updateSelectedNode((node) => setOptionalField(node, "style", value as GameUiStyleRecord | undefined))
            }
          />
          </div>
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
    <div key={pathToKey(path)} style={{ display: "grid", gap: 2 }}>
      <button
        type="button"
        className="gst-node"
        data-family={nodeFamily(node.type)}
        data-selected={isSelected}
        onClick={() => onSelect(path)}
      >
        <span className="gst-node-icon">
          {(() => {
            const Icon = nodeIcon(node.type);
            return <Icon size={15} />;
          })()}
        </span>
        <span className="gst-node-name">{nodeDisplayName(node)}</span>
        {nodeDetailText(node) ? <span className="gst-node-id">{nodeDetailText(node)}</span> : null}
      </button>
      {branches.length > 0 ? (
        <div className="gst-tree-children">
          {branches.map((branch) => (
            <div key={pathToKey(branch.path)} style={{ display: "grid", gap: 2 }}>
              <div className="gst-branch-label">
                <CornerDownRight size={11} />
                {branch.label}
              </div>
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

// 组件的中文名（取自 COMPONENT_LIBRARY 的 label），未知组件回退到原 id。
function componentLabel(componentId: string): string {
  return COMPONENT_LIBRARY.find((entry) => entry.id === componentId)?.label ?? componentId;
}

// 树与检查器标题用的人类可读名称（中文优先）。
function nodeDisplayName(node: GameUiLayoutNodeV2): string {
  switch (node.type) {
    case "component":
      return componentLabel(node.component);
    case "slot":
      return `插槽 ${node.name}`;
    case "when":
      return "条件 when";
    case "for_each":
      return "循环 for_each";
    case "text":
      return "文本 text";
    case "image":
      return "图片 image";
    case "badge":
      return "徽标 badge";
    case "button":
      return "按钮 button";
    case "checkbox":
      return "复选 checkbox";
    case "grid":
      return "网格 grid";
    case "stack":
      return "堆叠 stack";
    case "absolute":
      return "浮层 absolute";
    default:
      return (node as { type: string }).type;
  }
}

// 节点的次要细节（等宽字体显示），帮助区分同类节点。
function nodeDetailText(node: GameUiLayoutNodeV2): string {
  switch (node.type) {
    case "component":
      return node.component;
    case "when":
      return node.expr || "";
    case "for_each":
      return node.source || "";
    case "text":
      return node.text || "";
    case "button":
      return node.label || "";
    case "badge":
      return node.text || "";
    case "checkbox":
      return node.label || "";
    case "image":
      return node.src || "";
    default:
      return "";
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
