import { useEffect, useState } from "react";
import {
  createMcpServer,
  deleteMcpServer,
  fetchMcpServers,
  probeMcpServer,
  updateMcpServer,
  getRuntimePlatform,
  type McpServerConfig,
  type McpServerUpsertRequest,
  type McpTransport,
} from "../data/apiAdapter";
import { SurfacePanel } from "./ScreenLayout";
import { showToast } from "./Toast";

const emptyServer: McpServerUpsertRequest = {
  name: "",
  transport: "http",
  command: "",
  args: [],
  env: {},
  url: "",
  headers: {},
  auth_token: "",
  enabled: true,
  timeout_ms: 15000,
  max_result_bytes: 32768,
};

function argsToText(values: string[]) {
  return values.join(" ");
}

function textToArgs(value: string) {
  return value
    .split(/\s+/)
    .map((item) => item.trim())
    .filter(Boolean);
}

function mapToText(value: Record<string, string>) {
  return Object.entries(value)
    .map(([key, item]) => `${key}=${item}`)
    .join("\n");
}

/** 每行 KEY=VALUE 解析为字符串映射，忽略空行与缺少等号的行。 */
function textToMap(value: string): Record<string, string> {
  const result: Record<string, string> = {};
  for (const line of value.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    const index = trimmed.indexOf("=");
    if (index <= 0) continue;
    result[trimmed.slice(0, index).trim()] = trimmed.slice(index + 1).trim();
  }
  return result;
}

export function McpServersPanel({ onServersChanged }: { onServersChanged?: () => void }) {
  const [servers, setServers] = useState<McpServerConfig[]>([]);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editorOpen, setEditorOpen] = useState(false);
  const [draft, setDraft] = useState<McpServerUpsertRequest>(emptyServer);
  const [argsText, setArgsText] = useState("");
  const [envText, setEnvText] = useState("");
  const [headersText, setHeadersText] = useState("");
  const [saving, setSaving] = useState(false);
  const [probingId, setProbingId] = useState<string | null>(null);
  const isAndroid = getRuntimePlatform() === "android";

  async function loadServers() {
    try {
      setServers(await fetchMcpServers());
    } catch (error) {
      showToast(`加载 MCP server 失败：${String(error)}`, "error");
    }
  }

  useEffect(() => {
    void loadServers();
  }, []);

  function openCreate() {
    setEditingId(null);
    // 安卓无法起子进程，默认给 http，避免用户配出一个必然失败的 server。
    setDraft({ ...emptyServer, transport: "http" });
    setArgsText("");
    setEnvText("");
    setHeadersText("");
    setEditorOpen(true);
  }

  function beginEdit(server: McpServerConfig) {
    setEditingId(server.id);
    setDraft({
      name: server.name,
      transport: server.transport,
      command: server.command,
      args: server.args,
      env: server.env,
      url: server.url,
      headers: server.headers,
      auth_token: server.auth_token,
      enabled: server.enabled,
      timeout_ms: server.timeout_ms,
      max_result_bytes: server.max_result_bytes,
    });
    setArgsText(argsToText(server.args));
    setEnvText(mapToText(server.env));
    setHeadersText(mapToText(server.headers));
    setEditorOpen(true);
  }

  async function saveDraft() {
    setSaving(true);
    try {
      const payload: McpServerUpsertRequest = {
        ...draft,
        args: textToArgs(argsText),
        env: textToMap(envText),
        headers: textToMap(headersText),
      };
      if (editingId) {
        await updateMcpServer(editingId, payload);
      } else {
        await createMcpServer(payload);
      }
      setEditorOpen(false);
      await loadServers();
      onServersChanged?.();
      showToast("MCP server 已保存", "success");
    } catch (error) {
      showToast(`保存失败：${String(error)}`, "error");
    } finally {
      setSaving(false);
    }
  }

  async function removeServer(server: McpServerConfig) {
    if (!window.confirm(`删除 MCP server「${server.name}」？绑定它的工具会变为未绑定。`)) {
      return;
    }
    try {
      await deleteMcpServer(server.id);
      await loadServers();
      onServersChanged?.();
      showToast("已删除", "success");
    } catch (error) {
      showToast(`删除失败：${String(error)}`, "error");
    }
  }

  async function probe(server: McpServerConfig) {
    setProbingId(server.id);
    try {
      const result = await probeMcpServer(server.id);
      if (result.ok) {
        const names = result.tools
          .map((tool) => String((tool as Record<string, unknown>).name ?? ""))
          .filter(Boolean);
        showToast(
          `连接成功，发现 ${names.length} 个工具：${names.slice(0, 6).join(", ")}`,
          "success",
        );
      } else {
        showToast(result.error ?? "连接失败", "error");
      }
    } catch (error) {
      showToast(`连接测试失败：${String(error)}`, "error");
    } finally {
      setProbingId(null);
    }
  }

  return (
    <SurfacePanel className="surface-panel--pad-lg">
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "flex-start",
          gap: 12,
          marginBottom: 14,
        }}
      >
        <div>
          <strong style={{ fontSize: 18 }}>MCP 服务连接</strong>
          <div className="text-muted" style={{ marginTop: 4, fontSize: 13 }}>
            配置外部 MCP server；只有连得上的 server 上的工具才会下发给模型
          </div>
        </div>
        <button type="button" className="action-btn action-btn--accent" onClick={openCreate}>
          新建连接
        </button>
      </div>
      {isAndroid ? (
        <div className="text-muted" style={{ marginBottom: 12 }}>
          当前为安卓平台：只能使用 http 方式连接远程 MCP server，stdio（本地进程）不可用。
        </div>
      ) : null}
      {servers.length === 0 ? (
        <div className="text-muted">还没有配置 MCP server。世界包需要外部工具能力时，先在这里连上对应的 server。</div>
      ) : (
        <div style={{ display: "grid", gap: 10 }}>
          {servers.map((server) => {
            const unsupported = isAndroid && server.transport === "stdio";
            return (
              <div
                key={server.id}
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  gap: 12,
                  padding: 12,
                  borderRadius: 10,
                  border: "1px solid rgba(255,255,255,0.08)",
                  background: "rgba(0,0,0,0.18)",
                }}
              >
                <div style={{ minWidth: 0 }}>
                  <div style={{ fontWeight: 600 }}>
                    {server.name}
                    {server.enabled ? null : <span className="text-muted">（已停用）</span>}
                  </div>
                  <div className="text-muted" style={{ marginTop: 4, fontSize: 12 }}>
                    {server.transport === "stdio"
                      ? `stdio · ${server.command} ${argsToText(server.args)}`
                      : `http · ${server.url}`}
                    {` · 超时 ${server.timeout_ms}ms · 结果上限 ${server.max_result_bytes}B`}
                  </div>
                  {unsupported ? (
                    <div style={{ marginTop: 4, fontSize: 12, color: "#ffb4a2" }}>
                      本平台不支持 stdio：安卓无法启动本地子进程，请改用 http 方式
                    </div>
                  ) : null}
                </div>
                <div style={{ display: "flex", gap: 8, flexShrink: 0 }}>
                  <button
                    type="button"
                    className="action-btn"
                    disabled={probingId === server.id}
                    onClick={() => void probe(server)}
                  >
                    {probingId === server.id ? "测试中..." : "测试连接"}
                  </button>
                  <button type="button" className="action-btn" onClick={() => beginEdit(server)}>
                    编辑
                  </button>
                  <button type="button" className="action-btn" onClick={() => void removeServer(server)}>
                    删除
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      )}
      {editorOpen ? (
        <div style={{ marginTop: 16, display: "grid", gap: 10 }}>
          <label className="editor-field">
            <span className="editor-field-label">名称</span>
            <input value={draft.name} onChange={(e) => setDraft({ ...draft, name: e.target.value })} />
          </label>
          <label className="editor-field">
            <span className="editor-field-label">传输方式</span>
            <select
              value={draft.transport}
              onChange={(e) => setDraft({ ...draft, transport: e.target.value as McpTransport })}
            >
              <option value="http">http（远程，全平台可用）</option>
              <option value="stdio" disabled={isAndroid}>
                stdio（本地进程{isAndroid ? "，本平台不支持" : "，仅桌面端"}）
              </option>
            </select>
          </label>
          {draft.transport === "stdio" ? (
            <>
              <label className="editor-field">
                <span className="editor-field-label">启动命令</span>
                <input
                  value={draft.command}
                  placeholder="npx"
                  onChange={(e) => setDraft({ ...draft, command: e.target.value })}
                />
              </label>
              <label className="editor-field">
                <span className="editor-field-label">命令参数（空格分隔）</span>
                <input
                  value={argsText}
                  placeholder="-y @modelcontextprotocol/server-filesystem ./data"
                  onChange={(e) => setArgsText(e.target.value)}
                />
              </label>
              <label className="editor-field">
                <span className="editor-field-label">环境变量（每行 KEY=VALUE）</span>
                <textarea rows={3} value={envText} onChange={(e) => setEnvText(e.target.value)} />
              </label>
            </>
          ) : (
            <>
              <label className="editor-field">
                <span className="editor-field-label">端点地址</span>
                <input
                  value={draft.url}
                  placeholder="https://example.com/mcp"
                  onChange={(e) => setDraft({ ...draft, url: e.target.value })}
                />
              </label>
              <label className="editor-field">
                <span className="editor-field-label">凭据（Bearer Token，留空表示不带）</span>
                <input
                  type="password"
                  value={draft.auth_token}
                  onChange={(e) => setDraft({ ...draft, auth_token: e.target.value })}
                />
              </label>
              <label className="editor-field">
                <span className="editor-field-label">附加请求头（每行 KEY=VALUE）</span>
                <textarea rows={3} value={headersText} onChange={(e) => setHeadersText(e.target.value)} />
              </label>
            </>
          )}
          <div style={{ display: "flex", gap: 10 }}>
            <label className="editor-field" style={{ flex: 1 }}>
              <span className="editor-field-label">调用超时（ms）</span>
              <input
                type="number"
                value={draft.timeout_ms}
                onChange={(e) => setDraft({ ...draft, timeout_ms: Number(e.target.value) || 0 })}
              />
            </label>
            <label className="editor-field" style={{ flex: 1 }}>
              <span className="editor-field-label">结果上限（字节）</span>
              <input
                type="number"
                value={draft.max_result_bytes}
                onChange={(e) => setDraft({ ...draft, max_result_bytes: Number(e.target.value) || 0 })}
              />
            </label>
          </div>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={draft.enabled}
              onChange={(e) => setDraft({ ...draft, enabled: e.target.checked })}
            />
            <span>启用</span>
          </label>
          <div style={{ display: "flex", gap: 10 }}>
            <button
              type="button"
              className="action-btn action-btn--accent"
              disabled={
                saving ||
                !draft.name.trim() ||
                (draft.transport === "stdio" ? !draft.command.trim() : !draft.url.trim())
              }
              onClick={() => void saveDraft()}
            >
              {saving ? "保存中..." : "保存"}
            </button>
            <button type="button" className="action-btn" onClick={() => setEditorOpen(false)}>
              取消
            </button>
          </div>
        </div>
      ) : null}
    </SurfacePanel>
  );
}
