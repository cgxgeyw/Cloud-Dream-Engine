import { useEffect, useState, useMemo, useRef, type ChangeEvent } from "react";
import { useNavigate } from "react-router-dom";
import { useIsMobile } from "../components/ResponsiveLayout";
import {
  createMcpTool,
  deleteMcpTool,
  exportMcpTools,
  fetchMcpTools,
  importMcpTools,
  updateMcpTool,
  fetchMcpServers,
  type McpServerConfig,
  type McpToolResponse,
  type McpToolUpsertRequest,
} from "../data/apiAdapter";
import { McpServersPanel } from "../components/McpServersPanel";
import { ScreenLayout, SurfacePanel } from "../components/ScreenLayout";
import { showToast } from "../components/Toast";

const defaultInputSchema: Record<string, unknown> = {
  type: "object",
  properties: {},
};

/** 引擎硬编码内置工具 id（与后端 is_builtin_mcp_tool_id 保持一致）：显示为“内置”，不参与导出。 */
const ENGINE_BUILTIN_TOOL_IDS = new Set([
  "mcp-tool-list-scenes",
  "mcp-tool-list-characters",
  "mcp-tool-change-scene",
  "mcp-tool-switch-player-character",
  "mcp-tool-image-generation",
  "mcp-tool-schedule-notification",
]);

const emptyDraft: McpToolUpsertRequest = {
  name: "",
  description: "",
  server_name: "",
  tool_name: "",
  enabled: true,
  exposure_policy: "on-demand",
  risk_level: "low",
  trigger_keywords: [],
  input_schema: defaultInputSchema,
  server_id: "",
  impl_kind: "mcp",
  impl_config: {},
};

const defaultInputSchemaText = JSON.stringify(defaultInputSchema, null, 2);
const defaultImplConfigText = "{}";

function keywordsToText(values: string[]) {
  return values.join(", ");
}

function textToKeywords(value: string) {
  return value
    .split(/[,，\n]/)
    .map((item) => item.trim())
    .filter(Boolean);
}

function schemaToText(value: unknown) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return defaultInputSchemaText;
  }
  return JSON.stringify(value, null, 2) ?? defaultInputSchemaText;
}

function parseInputSchema(value: string): Record<string, unknown> {
  const trimmed = value.trim();
  if (!trimmed) {
    return defaultInputSchema;
  }
  const parsed = JSON.parse(trimmed) as unknown;
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error("参数 Schema 必须是 JSON 对象");
  }
  return parsed as Record<string, unknown>;
}

function implConfigToText(value: unknown) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return defaultImplConfigText;
  }
  return JSON.stringify(value, null, 2) ?? defaultImplConfigText;
}

function parseImplConfig(value: string): Record<string, unknown> {
  const trimmed = value.trim();
  if (!trimmed) {
    return {};
  }
  const parsed = JSON.parse(trimmed) as unknown;
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error("本地执行配置必须是 JSON 对象");
  }
  return parsed as Record<string, unknown>;
}

function resolveToolKindLabel(tool: McpToolResponse): string {
  if (ENGINE_BUILTIN_TOOL_IDS.has(tool.id)) {
    return "内置";
  }
  if (tool.impl_kind === "builtin_http") {
    return "本地";
  }
  return "MCP";
}

const toolKindBadgeStyle = {
  fontSize: 12,
  padding: "1px 6px",
  marginLeft: 6,
  borderRadius: 6,
  border: "1px solid currentColor",
  opacity: 0.75,
  verticalAlign: "middle",
} as const;

function resolveExposurePolicyMode(policy: string | Record<string, unknown> | undefined): string {
  if (typeof policy === "string") {
    return policy.trim() || "on-demand";
  }
  const mode = typeof policy?.mode === "string" ? policy.mode.trim() : "";
  return mode || "on-demand";
}

function resolveExposurePolicyLabel(policy: string | Record<string, unknown> | undefined): string {
  switch (resolveExposurePolicyMode(policy)) {
    case "manual-only":
      return "仅手动";
    case "disabled":
      return "禁用";
    default:
      return "按需";
  }
}

function resolveRiskLevelLabel(level: string | undefined): string {
  switch ((level ?? "").trim()) {
    case "high":
      return "高风险";
    case "medium":
      return "中风险";
    default:
      return "低风险";
  }
}

export function McpToolsPage() {
  const isMobile = useIsMobile();
  const navigate = useNavigate();
  const [tools, setTools] = useState<McpToolResponse[]>([]);
  const [draft, setDraft] = useState<McpToolUpsertRequest>(emptyDraft);
  const [keywordText, setKeywordText] = useState("");
  const [schemaText, setSchemaText] = useState(defaultInputSchemaText);
  const [implConfigText, setImplConfigText] = useState(defaultImplConfigText);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editorOpen, setEditorOpen] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [importing, setImporting] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const importInputRef = useRef<HTMLInputElement>(null);

  async function loadTools() {
    setLoading(true);
    setError(null);
    try {
      setTools(await fetchMcpTools());
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : "加载 MCP 工具失败");
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void loadTools();
  }, []);

  const activeCount = useMemo(() => tools.filter((tool) => tool.enabled).length, [tools]);

  // 工具需要绑定一个 MCP server 才能真正执行（第 7 项）。
  const [servers, setServers] = useState<McpServerConfig[]>([]);
  async function loadServers() {
    try {
      setServers(await fetchMcpServers());
    } catch {
      setServers([]);
    }
  }
  useEffect(() => {
    void loadServers();
  }, []);

  function renderServerPicker() {
    return (
      <label className="editor-field">
        <span className="editor-field-label">绑定 MCP server</span>
        <select
          value={draft.server_id}
          onChange={(event) => setDraft({ ...draft, server_id: event.target.value })}
        >
          <option value="">未绑定（不会下发给模型）</option>
          {servers.map((server) => (
            <option key={server.id} value={server.id}>
              {server.name}（{server.transport}）
            </option>
          ))}
        </select>
      </label>
    );
  }

  function renderImplKindPicker() {
    return (
      <label className="editor-field">
        <span className="editor-field-label">实现方式</span>
        <select value={draft.impl_kind} onChange={(event) => changeImplKind(event.target.value)}>
          <option value="mcp">外部 MCP server</option>
          <option value="builtin_http">本地 HTTP</option>
        </select>
      </label>
    );
  }

  function renderImplConfigEditor(style?: { minHeight?: number }) {
    return (
      <label className="editor-field">
        <span className="editor-field-label">本地执行配置（JSON）</span>
        <textarea
          value={implConfigText}
          onChange={(event) => setImplConfigText(event.target.value)}
          spellCheck={false}
          placeholder='{"mode":"generic"}'
          style={{ minHeight: style?.minHeight ?? 120, fontFamily: "Consolas, 'SFMono-Regular', monospace" }}
        />
      </label>
    );
  }

  function openCreateEditor() {
    setEditingId(null);
    setDraft(emptyDraft);
    setKeywordText("");
    setSchemaText(defaultInputSchemaText);
    setImplConfigText(defaultImplConfigText);
    setEditorOpen(true);
    setError(null);
  }

  function beginEdit(tool: McpToolResponse) {
    setEditingId(tool.id);
    setDraft({
      name: tool.name,
      description: tool.description,
      server_name: tool.server_name,
      tool_name: tool.tool_name,
      enabled: tool.enabled,
      exposure_policy: resolveExposurePolicyMode(tool.exposure_policy),
      risk_level: tool.risk_level,
      trigger_keywords: tool.trigger_keywords,
      input_schema: tool.input_schema ?? defaultInputSchema,
      server_id: tool.server_id ?? "",
      impl_kind: tool.impl_kind || "mcp",
      impl_config: tool.impl_config ?? {},
    });
    setKeywordText(keywordsToText(tool.trigger_keywords));
    setSchemaText(schemaToText(tool.input_schema));
    setImplConfigText(implConfigToText(tool.impl_config));
    setEditorOpen(true);
    setError(null);
  }

  function closeEditor() {
    setEditingId(null);
    setDraft(emptyDraft);
    setKeywordText("");
    setSchemaText(defaultInputSchemaText);
    setImplConfigText(defaultImplConfigText);
    setEditorOpen(false);
  }

  function changeImplKind(implKind: string) {
    // 本地实现不需要绑定外部 MCP server，切换时清空绑定。
    setDraft({ ...draft, impl_kind: implKind, server_id: implKind === "builtin_http" ? "" : draft.server_id });
  }

  async function saveDraft() {
    let implConfig: Record<string, unknown> = {};
    if (draft.impl_kind === "builtin_http") {
      try {
        implConfig = parseImplConfig(implConfigText);
      } catch (parseError) {
        showToast(parseError instanceof Error ? parseError.message : "本地执行配置不是合法 JSON", "error");
        return;
      }
    }
    try {
      setSaving(true);
      setError(null);
      const inputSchema = parseInputSchema(schemaText);
      const payload = {
        ...draft,
        trigger_keywords: textToKeywords(keywordText),
        input_schema: inputSchema,
        impl_config: implConfig,
        server_id: draft.impl_kind === "builtin_http" ? "" : draft.server_id,
      };
      const saved = editingId ? await updateMcpTool(editingId, payload) : await createMcpTool(payload);
      setTools((current) =>
        editingId ? current.map((tool) => (tool.id === saved.id ? saved : tool)) : [saved, ...current],
      );
      closeEditor();
      showToast(editingId ? "工具已更新" : "工具已创建");
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : "保存 MCP 工具失败");
    } finally {
      setSaving(false);
    }
  }

  async function toggleTool(tool: McpToolResponse) {
    try {
      const updated = await updateMcpTool(tool.id, {
        ...tool,
        enabled: !tool.enabled,
        exposure_policy: resolveExposurePolicyMode(tool.exposure_policy),
      });
      setTools((current) => current.map((item) => (item.id === updated.id ? updated : item)));
    } catch (toggleError) {
      setError(toggleError instanceof Error ? toggleError.message : "更新工具状态失败");
    }
  }

  async function removeTool(toolId: string) {
    try {
      await deleteMcpTool(toolId);
      setTools((current) => current.filter((tool) => tool.id !== toolId));
      if (editingId === toolId) {
        closeEditor();
      }
      showToast("工具已删除");
    } catch (deleteError) {
      setError(deleteError instanceof Error ? deleteError.message : "删除 MCP 工具失败");
    }
  }

  function pickImportFile() {
    importInputRef.current?.click();
  }

  async function handleImportFile(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    event.target.value = "";
    if (!file) {
      return;
    }
    try {
      setImporting(true);
      setError(null);
      const text = await file.text();
      const summary = await importMcpTools(text);
      showToast(`导入完成：新增 ${summary.imported}，更新 ${summary.updated}，跳过 ${summary.skipped}`, "success");
      await loadTools();
    } catch (importError) {
      const message = importError instanceof Error ? importError.message : String(importError);
      setError(message);
      showToast(message, "error");
    } finally {
      setImporting(false);
    }
  }

  async function handleExport() {
    const ids = tools.filter((tool) => !ENGINE_BUILTIN_TOOL_IDS.has(tool.id)).map((tool) => tool.id);
    if (ids.length === 0) {
      showToast("没有可导出的工具（内置工具不导出）", "error");
      return;
    }
    try {
      setExporting(true);
      setError(null);
      const json = await exportMcpTools(ids);
      const blob = new Blob([json], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = "mcp-tools-export.json";
      document.body.append(anchor);
      anchor.click();
      anchor.remove();
      URL.revokeObjectURL(url);
      showToast("工具包已导出", "success");
    } catch (exportError) {
      const message = exportError instanceof Error ? exportError.message : "导出工具失败";
      setError(message);
      showToast(message, "error");
    } finally {
      setExporting(false);
    }
  }

  // ===== Desktop Layout (双栏同时显示) =====
  const desktopLayout = (
    <ScreenLayout
      title="MCP 工具管理"
      subtitle="登记世界主控可按需调用的工具。未触发时不会把工具列表发送给模型。"
      toolbar={
        <div style={{ display: "flex", gap: 8 }}>
          <button type="button" className="action-btn" disabled={importing} onClick={pickImportFile}>
            {importing ? "导入中..." : "导入工具"}
          </button>
          <button type="button" className="action-btn" disabled={exporting} onClick={() => void handleExport()}>
            {exporting ? "导出中..." : "导出工具"}
          </button>
          <button type="button" className="action-btn" onClick={() => navigate("/")}>返回首页</button>
        </div>
      }
      maxWidth={1120}
    >
      <div style={{ marginBottom: 16 }}>
        <McpServersPanel onServersChanged={() => void loadServers()} />
      </div>
      <div className="mcp-desktop-grid">
        <SurfacePanel className="surface-panel--pad-lg">
          <div style={{ display: "flex", justifyContent: "space-between", gap: 12, marginBottom: 16 }}>
            <div>
              <strong style={{ fontSize: 20 }}>工具清单</strong>
              <div className="text-muted" style={{ marginTop: 4 }}>已启用 {activeCount} / {tools.length}</div>
            </div>
            <button type="button" className="action-btn action-btn--accent" onClick={openCreateEditor}>
              + {"\u65b0\u589e\u5de5\u5177"}
            </button>
          </div>
          {loading ? <div>正在加载 MCP 工具...</div> : null}
          {error ? <div className="error-text">{error}</div> : null}
          {!loading && tools.length === 0 ? <div className="empty-text">暂无 MCP 工具。</div> : null}
          <div style={{ display: "grid", gap: 12 }}>
            {tools.map((tool) => (
              <div key={tool.id} className="mcp-tool-card">
                <div style={{ display: "flex", justifyContent: "space-between", gap: 12 }}>
                  <div>
                    <strong>{tool.name}</strong>
                    <span style={toolKindBadgeStyle}>{resolveToolKindLabel(tool)}</span>
                    <div className="text-muted" style={{ marginTop: 4 }}>{tool.server_name} / {tool.tool_name}{tool.server_id ? "" : " · 未绑定 server"}</div>
                  </div>
                  <span>{tool.enabled ? "启用" : "停用"} · {resolveExposurePolicyLabel(tool.exposure_policy)} · {resolveRiskLevelLabel(tool.risk_level)}</span>
                </div>
                {tool.description ? <p className="text-muted">{tool.description}</p> : null}
                <div style={{ display: "flex", gap: 8 }}>
                  <button type="button" className="action-btn" onClick={() => beginEdit(tool)}>编辑</button>
                  <button type="button" className="action-btn" onClick={() => void toggleTool(tool)}>{tool.enabled ? "停用" : "启用"}</button>
                  <button type="button" className="action-btn action-btn--danger" onClick={() => void removeTool(tool.id)}>删除</button>
                </div>
              </div>
            ))}
          </div>
        </SurfacePanel>

        <SurfacePanel className="surface-panel--pad-lg mcp-editor-panel">
          <strong style={{ fontSize: 20 }}>{editingId ? "编辑工具" : "新增工具"}</strong>
          <div className="grid grid--gap-sm" style={{ marginTop: 14 }}>
            <label className="editor-field"><span className="editor-field-label">显示名称</span><input value={draft.name} onChange={(e) => setDraft({ ...draft, name: e.target.value })} /></label>
            {renderImplKindPicker()}
            {draft.impl_kind === "builtin_http" ? renderImplConfigEditor({ minHeight: 140 }) : (
              <>
                <label className="editor-field"><span className="editor-field-label">MCP 服务（显示名）</span><input value={draft.server_name} onChange={(e) => setDraft({ ...draft, server_name: e.target.value })} /></label>
                {renderServerPicker()}
              </>
            )}
            <label className="editor-field"><span className="editor-field-label">工具名</span><input value={draft.tool_name} onChange={(e) => setDraft({ ...draft, tool_name: e.target.value })} /></label>
            <label className="editor-field"><span className="editor-field-label">说明</span><textarea value={draft.description} onChange={(e) => setDraft({ ...draft, description: e.target.value })} /></label>
            <label className="editor-field"><span className="editor-field-label">参数 Schema</span><textarea value={schemaText} onChange={(e) => setSchemaText(e.target.value)} spellCheck={false} style={{ minHeight: 180, fontFamily: "Consolas, 'SFMono-Regular', monospace" }} /></label>
            <label className="editor-field"><span className="editor-field-label">触发词</span><textarea value={keywordText} onChange={(e) => setKeywordText(e.target.value)} placeholder="逗号或换行分隔" /></label>
            <label className="editor-field"><span className="editor-field-label">暴露策略</span><select value={resolveExposurePolicyMode(draft.exposure_policy)} onChange={(e) => setDraft({ ...draft, exposure_policy: e.target.value })}><option value="on-demand">按需暴露</option><option value="manual-only">仅手动</option><option value="disabled">禁用</option></select></label>
            <label className="editor-field"><span className="editor-field-label">风险等级</span><select value={draft.risk_level} onChange={(e) => setDraft({ ...draft, risk_level: e.target.value })}><option value="low">低</option><option value="medium">中</option><option value="high">高</option></select></label>
            <label style={{ display: "flex", gap: 8, alignItems: "center" }}><input type="checkbox" checked={draft.enabled} onChange={(e) => setDraft({ ...draft, enabled: e.target.checked })} />启用</label>
            <div style={{ display: "flex", gap: 8 }}>
              <button type="button" className="action-btn action-btn--accent" disabled={saving || !draft.name.trim() || !draft.tool_name.trim() || (draft.impl_kind !== "builtin_http" && !draft.server_name.trim())} onClick={() => void saveDraft()}>{saving ? "保存中..." : "保存"}</button>
              <button type="button" className="action-btn" onClick={closeEditor}>清空</button>
            </div>
          </div>
        </SurfacePanel>
      </div>
    </ScreenLayout>
  );

  // ===== Mobile Layout (列表/编辑器切换) =====
  const mobileLayout = (
    <ScreenLayout title="MCP 工具" compactHeader maxWidth={980}>
      {editorOpen ? null : (
        <div style={{ marginBottom: 12 }}>
          <McpServersPanel onServersChanged={() => void loadServers()} />
        </div>
      )}
      {editorOpen ? (
        <div className="settings-page-shell">
          <div className="settings-detail-head">
            <button type="button" className="action-btn" onClick={closeEditor}>
              返回工具列表
            </button>
            <div className="settings-detail-head-copy">
              <strong>{editingId ? "编辑工具" : "新增工具"}</strong>
            </div>
          </div>

          <SurfacePanel className="surface-panel--pad-lg">
            {error ? <div className="error-text">{error}</div> : null}

            <div className="settings-section">
              <div className="settings-form-grid">
                <label className="field-label">
                  <span className="field-label-text">显示名称</span>
                  <input
                    value={draft.name}
                    onChange={(event) => setDraft({ ...draft, name: event.target.value })}
                    className="field-input"
                  />
                </label>

                {renderImplKindPicker()}

                {draft.impl_kind === "builtin_http" ? null : (
                  <>
                    <label className="field-label">
                      <span className="field-label-text">服务名</span>
                      <input
                        value={draft.server_name}
                        onChange={(event) => setDraft({ ...draft, server_name: event.target.value })}
                        className="field-input"
                      />
                    </label>
                    {renderServerPicker()}
                  </>
                )}

                <label className="field-label">
                  <span className="field-label-text">工具名</span>
                  <input
                    value={draft.tool_name}
                    onChange={(event) => setDraft({ ...draft, tool_name: event.target.value })}
                    className="field-input"
                  />
                </label>

                <label className="field-label">
                  <span className="field-label-text">风险等级</span>
                  <select
                    value={draft.risk_level}
                    onChange={(event) => setDraft({ ...draft, risk_level: event.target.value })}
                    className="field-input"
                  >
                    <option value="low">低</option>
                    <option value="medium">中</option>
                    <option value="high">高</option>
                  </select>
                </label>
              </div>

              <label className="field-label">
                <span className="field-label-text">说明</span>
                <textarea
                  value={draft.description}
                  onChange={(event) => setDraft({ ...draft, description: event.target.value })}
                  className="field-input"
                  style={{ minHeight: 120, resize: "vertical" }}
                />
              </label>

              {draft.impl_kind === "builtin_http" ? renderImplConfigEditor({ minHeight: 140 }) : null}

              <label className="field-label">
                <span className="field-label-text">参数 Schema</span>
                <textarea
                  value={schemaText}
                  onChange={(event) => setSchemaText(event.target.value)}
                  className="field-input"
                  spellCheck={false}
                  style={{ minHeight: 160, resize: "vertical", fontFamily: "Consolas, 'SFMono-Regular', monospace" }}
                />
              </label>

              <label className="field-label">
                <span className="field-label-text">触发词</span>
                <textarea
                  value={keywordText}
                  onChange={(event) => setKeywordText(event.target.value)}
                  className="field-input"
                  style={{ minHeight: 120, resize: "vertical" }}
                  placeholder="用逗号或换行分隔"
                />
              </label>

              <div className="settings-form-grid">
                <label className="field-label">
                  <span className="field-label-text">暴露策略</span>
                  <select
                    value={resolveExposurePolicyMode(draft.exposure_policy)}
                    onChange={(event) => setDraft({ ...draft, exposure_policy: event.target.value })}
                    className="field-input"
                  >
                    <option value="on-demand">按需暴露</option>
                    <option value="manual-only">仅手动</option>
                    <option value="disabled">禁用</option>
                  </select>
                </label>

                <label className="field-label">
                  <span className="field-label-text">启用状态</span>
                  <div className="settings-inline-toggle">
                    <input
                      type="checkbox"
                      checked={draft.enabled}
                      onChange={(event) => setDraft({ ...draft, enabled: event.target.checked })}
                    />
                  </div>
                </label>
              </div>

              <div className="settings-form-actions">
                <button
                  type="button"
                  className="action-btn action-btn--accent"
                  disabled={saving || !draft.name.trim() || !draft.tool_name.trim() || (draft.impl_kind !== "builtin_http" && !draft.server_name.trim())}
                  onClick={() => void saveDraft()}
                >
                  {saving ? "保存中..." : "保存"}
                </button>
                <button type="button" className="action-btn" onClick={closeEditor} disabled={saving}>
                  取消
                </button>
              </div>
            </div>
          </SurfacePanel>
        </div>
      ) : (
        <div className="settings-page-shell">
          <div className="mcp-mobile-header">
            <div className="settings-detail-head-copy">
              <strong>{"MCP \u5de5\u5177"}</strong>
            </div>
            <div style={{ display: "flex", gap: 8 }}>
              <button type="button" className="action-btn" disabled={importing} onClick={pickImportFile}>
                导入
              </button>
              <button type="button" className="action-btn" disabled={exporting} onClick={() => void handleExport()}>
                导出
              </button>
              <button type="button" className="action-btn action-btn--accent" onClick={openCreateEditor}>
                + 新增工具
              </button>
            </div>
          </div>

          <SurfacePanel className="surface-panel--pad-lg">
            {loading ? <div>正在加载 MCP 工具...</div> : null}
            {error ? <div className="error-text">{error}</div> : null}
            {!loading && tools.length === 0 ? <div className="empty-text">暂无 MCP 工具。</div> : null}

            {!loading && tools.length > 0 ? (
              <div className="mcp-tool-list">
                {tools.map((tool) => {
                  return (
                  <div key={tool.id} className="mcp-tool-card">
                    <div className="mcp-tool-card-head">
                      <div className="mcp-tool-card-copy">
                        <strong>{tool.name}</strong>
                        <span style={toolKindBadgeStyle}>{resolveToolKindLabel(tool)}</span>
                      </div>
                      <div className="mcp-tool-card-meta">
                        <button
                          type="button"
                          className={`mcp-tool-meta-btn mcp-tool-state${tool.enabled ? " mcp-tool-state--enabled" : ""}`}
                          onClick={() => void toggleTool(tool)}
                        >
                          {tool.enabled ? "启用" : "停用"}
                        </button>
                      </div>
                    </div>

                    <div className="mcp-tool-card-actions">
                      <button type="button" className="action-btn" onClick={() => beginEdit(tool)}>
                        编辑
                      </button>
                      <button type="button" className="action-btn" onClick={() => void toggleTool(tool)}>
                        {tool.enabled ? "停用" : "启用"}
                      </button>
                      <button type="button" className="action-btn action-btn--danger" onClick={() => void removeTool(tool.id)}>
                        删除
                      </button>
                    </div>
                  </div>
                  );
                })}
              </div>
            ) : null}
          </SurfacePanel>
        </div>
      )}
    </ScreenLayout>
  );

  return (
    <>
      <input
        ref={importInputRef}
        type="file"
        accept=".json,application/json"
        style={{ display: "none" }}
        onChange={(event) => void handleImportFile(event)}
      />
      {isMobile ? mobileLayout : desktopLayout}
    </>
  );
}
