import { useEffect, useState, useMemo } from "react";
import { useNavigate } from "react-router-dom";
import { useIsMobile } from "../components/ResponsiveLayout";
import {
  createMcpTool,
  deleteMcpTool,
  fetchMcpTools,
  updateMcpTool,
  type McpToolResponse,
  type McpToolUpsertRequest,
} from "../data/apiAdapter";
import { ScreenLayout, SurfacePanel } from "../components/ScreenLayout";
import { showToast } from "../components/Toast";

const defaultInputSchema: Record<string, unknown> = {
  type: "object",
  properties: {},
};

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
};

const defaultInputSchemaText = JSON.stringify(defaultInputSchema, null, 2);

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
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editorOpen, setEditorOpen] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

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

  function openCreateEditor() {
    setEditingId(null);
    setDraft(emptyDraft);
    setKeywordText("");
    setSchemaText(defaultInputSchemaText);
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
    });
    setKeywordText(keywordsToText(tool.trigger_keywords));
    setSchemaText(schemaToText(tool.input_schema));
    setEditorOpen(true);
    setError(null);
  }

  function closeEditor() {
    setEditingId(null);
    setDraft(emptyDraft);
    setKeywordText("");
    setSchemaText(defaultInputSchemaText);
    setEditorOpen(false);
  }

  async function saveDraft() {
    try {
      setSaving(true);
      setError(null);
      const inputSchema = parseInputSchema(schemaText);
      const payload = { ...draft, trigger_keywords: textToKeywords(keywordText), input_schema: inputSchema };
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

  // ===== Desktop Layout (双栏同时显示) =====
  const desktopLayout = (
    <ScreenLayout
      title="MCP 工具管理"
      subtitle="登记世界主控可按需调用的工具。未触发时不会把工具列表发送给模型。"
      toolbar={<button type="button" className="action-btn" onClick={() => navigate("/")}>返回首页</button>}
      maxWidth={1120}
    >
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
                    <div className="text-muted" style={{ marginTop: 4 }}>{tool.server_name} / {tool.tool_name}</div>
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
            <label className="editor-field"><span className="editor-field-label">MCP 服务</span><input value={draft.server_name} onChange={(e) => setDraft({ ...draft, server_name: e.target.value })} /></label>
            <label className="editor-field"><span className="editor-field-label">工具名</span><input value={draft.tool_name} onChange={(e) => setDraft({ ...draft, tool_name: e.target.value })} /></label>
            <label className="editor-field"><span className="editor-field-label">说明</span><textarea value={draft.description} onChange={(e) => setDraft({ ...draft, description: e.target.value })} /></label>
            <label className="editor-field"><span className="editor-field-label">参数 Schema</span><textarea value={schemaText} onChange={(e) => setSchemaText(e.target.value)} spellCheck={false} style={{ minHeight: 180, fontFamily: "Consolas, 'SFMono-Regular', monospace" }} /></label>
            <label className="editor-field"><span className="editor-field-label">触发词</span><textarea value={keywordText} onChange={(e) => setKeywordText(e.target.value)} placeholder="逗号或换行分隔" /></label>
            <label className="editor-field"><span className="editor-field-label">暴露策略</span><select value={resolveExposurePolicyMode(draft.exposure_policy)} onChange={(e) => setDraft({ ...draft, exposure_policy: e.target.value })}><option value="on-demand">按需暴露</option><option value="manual-only">仅手动</option><option value="disabled">禁用</option></select></label>
            <label className="editor-field"><span className="editor-field-label">风险等级</span><select value={draft.risk_level} onChange={(e) => setDraft({ ...draft, risk_level: e.target.value })}><option value="low">低</option><option value="medium">中</option><option value="high">高</option></select></label>
            <label style={{ display: "flex", gap: 8, alignItems: "center" }}><input type="checkbox" checked={draft.enabled} onChange={(e) => setDraft({ ...draft, enabled: e.target.checked })} />启用</label>
            <div style={{ display: "flex", gap: 8 }}>
              <button type="button" className="action-btn action-btn--accent" disabled={saving || !draft.name.trim() || !draft.server_name.trim() || !draft.tool_name.trim()} onClick={() => void saveDraft()}>{saving ? "保存中..." : "保存"}</button>
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

                <label className="field-label">
                  <span className="field-label-text">服务名</span>
                  <input
                    value={draft.server_name}
                    onChange={(event) => setDraft({ ...draft, server_name: event.target.value })}
                    className="field-input"
                  />
                </label>

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
                  disabled={saving || !draft.name.trim() || !draft.server_name.trim() || !draft.tool_name.trim()}
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
            <button type="button" className="action-btn action-btn--accent" onClick={openCreateEditor}>
              + 新增工具
            </button>
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

  return isMobile ? mobileLayout : desktopLayout;
}
