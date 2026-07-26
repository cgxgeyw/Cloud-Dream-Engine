use crate::models::mcp_server::{McpServerConfig, TRANSPORT_HTTP, TRANSPORT_STDIO};
use serde_json::{json, Value};
use tokio::io::AsyncBufReadExt;
use std::time::Duration;

/// 客户端声明的协议版本。server 返回其它版本时按其返回值继续（MCP 允许协商）。
const PROTOCOL_VERSION: &str = "2025-06-18";
const CLIENT_NAME: &str = "dream-narrative-engine";

fn jsonrpc_request(id: u64, method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

fn jsonrpc_notification(method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "method": method, "params": params })
}

/// 从 JSON-RPC 响应体取 result，error 转成可读错误。
fn parse_jsonrpc_result(value: &Value) -> Result<Value, String> {
    if let Some(error) = value.get("error") {
        let code = error.get("code").and_then(Value::as_i64).unwrap_or(0);
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown error");
        return Err(format!("MCP server 返回错误 {code}: {message}"));
    }
    value
        .get("result")
        .cloned()
        .ok_or_else(|| "MCP 响应缺少 result 字段".to_string())
}

fn initialize_params() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": {},
        "clientInfo": { "name": CLIENT_NAME, "version": env!("CARGO_PKG_VERSION") },
    })
}

/// 一次 MCP 会话：连接 → initialize → 若干请求 → 关闭。
/// stdio 每次调用起一个子进程（一次会话内完成 initialize + tools/call），
/// 避免维护常驻进程池的生命周期问题；HTTP 侧保留 Mcp-Session-Id。
pub enum McpSession {
    Stdio(StdioSession),
    Http(HttpSession),
}

impl McpSession {
    pub async fn connect(config: &McpServerConfig) -> Result<Self, String> {
        config.validate_ready()?;
        let timeout = Duration::from_millis(config.resolved_timeout_ms());
        let mut session = match config.transport.as_str() {
            TRANSPORT_STDIO => McpSession::Stdio(StdioSession::spawn(config)?),
            TRANSPORT_HTTP => McpSession::Http(HttpSession::new(config)?),
            other => return Err(format!("未知的 MCP 传输方式：{other}")),
        };
        let init = tokio::time::timeout(timeout, session.request("initialize", initialize_params()))
            .await
            .map_err(|_| format!("MCP initialize 超时（{} ms）", config.resolved_timeout_ms()))??;
        let _ = init;
        session
            .notify("notifications/initialized", json!({}))
            .await?;
        Ok(session)
    }

    pub async fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        match self {
            McpSession::Stdio(session) => session.request(method, params).await,
            McpSession::Http(session) => session.request(method, params).await,
        }
    }

    async fn notify(&mut self, method: &str, params: Value) -> Result<(), String> {
        match self {
            McpSession::Stdio(session) => session.notify(method, params).await,
            McpSession::Http(session) => session.notify(method, params).await,
        }
    }

    pub async fn shutdown(self) {
        if let McpSession::Stdio(session) = self {
            session.shutdown().await;
        }
    }
}

/// 列出 server 暴露的工具（用于配置页发现工具与连通性测试）。
pub async fn list_server_tools(config: &McpServerConfig) -> Result<Vec<Value>, String> {
    let timeout = Duration::from_millis(config.resolved_timeout_ms());
    let mut session = McpSession::connect(config).await?;
    let result = tokio::time::timeout(timeout, session.request("tools/list", json!({}))).await;
    session.shutdown().await;
    let result = result
        .map_err(|_| format!("MCP tools/list 超时（{} ms）", config.resolved_timeout_ms()))??;
    Ok(result
        .get("tools")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

/// 调用一个工具，返回 server 的 result（未做大小裁剪，由 executor 负责）。
pub async fn call_server_tool(
    config: &McpServerConfig,
    tool_name: &str,
    arguments: Value,
) -> Result<Value, String> {
    let timeout = Duration::from_millis(config.resolved_timeout_ms());
    let mut session = McpSession::connect(config).await?;
    let params = json!({ "name": tool_name, "arguments": arguments });
    let result = tokio::time::timeout(timeout, session.request("tools/call", params)).await;
    session.shutdown().await;
    result.map_err(|_| {
        format!(
            "MCP 工具 {tool_name} 调用超时（{} ms）",
            config.resolved_timeout_ms()
        )
    })?
}

// ---- stdio 传输（本地子进程，仅桌面端）----

pub struct StdioSession {
    child: tokio::process::Child,
    stdin: tokio::process::ChildStdin,
    reader: tokio::io::Lines<tokio::io::BufReader<tokio::process::ChildStdout>>,
    next_id: u64,
}

impl StdioSession {
    fn spawn(config: &McpServerConfig) -> Result<Self, String> {
        use std::process::Stdio;
        let mut command = tokio::process::Command::new(config.command.trim());
        command
            .args(&config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        for (key, value) in McpServerConfig::string_map(&config.env) {
            command.env(key, value);
        }
        #[cfg(windows)]
        {
            // 不弹出控制台窗口（CREATE_NO_WINDOW）
            command.creation_flags(0x0800_0000);
        }
        let mut child = command
            .spawn()
            .map_err(|error| format!("启动 MCP server 进程失败（{}）：{error}", config.command))?;
        let stdin = child.stdin.take().ok_or("无法取得子进程 stdin")?;
        let stdout = child.stdout.take().ok_or("无法取得子进程 stdout")?;
        Ok(Self {
            child,
            stdin,
            reader: tokio::io::BufReader::new(stdout).lines(),
            next_id: 1,
        })
    }

    async fn write_line(&mut self, payload: &Value) -> Result<(), String> {
        use tokio::io::AsyncWriteExt;
        let mut line = serde_json::to_string(payload).map_err(|error| error.to_string())?;
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|error| format!("写入 MCP server stdin 失败：{error}"))?;
        self.stdin
            .flush()
            .await
            .map_err(|error| format!("刷新 MCP server stdin 失败：{error}"))
    }

    async fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        self.write_line(&jsonrpc_request(id, method, params)).await?;
        // server 可能先推送通知/日志行，读到 id 匹配的响应为止。
        loop {
            let line = self
                .reader
                .next_line()
                .await
                .map_err(|error| format!("读取 MCP server stdout 失败：{error}"))?;
            let Some(line) = line else {
                return Err("MCP server 在返回响应前关闭了 stdout".to_string());
            };
            let Ok(value) = serde_json::from_str::<Value>(line.trim()) else {
                continue;
            };
            if value.get("id").and_then(Value::as_u64) == Some(id) {
                return parse_jsonrpc_result(&value);
            }
        }
    }

    async fn notify(&mut self, method: &str, params: Value) -> Result<(), String> {
        self.write_line(&jsonrpc_notification(method, params)).await
    }

    async fn shutdown(mut self) {
        drop(self.stdin);
        let _ = self.child.kill().await;
    }
}

// ---- Streamable HTTP 传输（全平台，安卓唯一可用方式）----

pub struct HttpSession {
    client: reqwest::Client,
    url: String,
    headers: Vec<(String, String)>,
    auth_token: String,
    /// initialize 响应带回的 Mcp-Session-Id，后续请求需回传。
    session_id: Option<String>,
    next_id: u64,
}

impl HttpSession {
    fn new(config: &McpServerConfig) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.resolved_timeout_ms()))
            .build()
            .map_err(|error| format!("构建 HTTP 客户端失败：{error}"))?;
        Ok(Self {
            client,
            url: config.url.trim().to_string(),
            headers: McpServerConfig::string_map(&config.headers),
            auth_token: config.auth_token.trim().to_string(),
            session_id: None,
            next_id: 1,
        })
    }

    fn build_post(&self, payload: &Value) -> reqwest::RequestBuilder {
        let mut builder = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("MCP-Protocol-Version", PROTOCOL_VERSION);
        for (key, value) in &self.headers {
            builder = builder.header(key, value);
        }
        if !self.auth_token.is_empty() {
            builder = builder.bearer_auth(&self.auth_token);
        }
        if let Some(session_id) = &self.session_id {
            builder = builder.header("Mcp-Session-Id", session_id);
        }
        builder.json(payload)
    }

    async fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let response = self
            .build_post(&jsonrpc_request(id, method, params))
            .send()
            .await
            .map_err(|error| format!("请求 MCP server 失败：{error}"))?;
        if let Some(session_id) = response
            .headers()
            .get("mcp-session-id")
            .and_then(|value| value.to_str().ok())
        {
            self.session_id = Some(session_id.to_string());
        }
        let status = response.status();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let body = response
            .text()
            .await
            .map_err(|error| format!("读取 MCP 响应失败：{error}"))?;
        if !status.is_success() {
            let detail = body.chars().take(300).collect::<String>();
            return Err(format!("MCP server 返回 HTTP {status}：{detail}"));
        }
        let value = if content_type.contains("text/event-stream") {
            parse_sse_response(&body, id)?
        } else {
            serde_json::from_str::<Value>(body.trim())
                .map_err(|error| format!("解析 MCP 响应 JSON 失败：{error}"))?
        };
        parse_jsonrpc_result(&value)
    }

    async fn notify(&mut self, method: &str, params: Value) -> Result<(), String> {
        // 通知无响应体；网络抖动不应让整次调用失败，失败仅忽略。
        let _ = self
            .build_post(&jsonrpc_notification(method, params))
            .send()
            .await;
        Ok(())
    }
}

/// 从 SSE 响应体里取出 id 匹配的那条 JSON-RPC 消息。
fn parse_sse_response(body: &str, id: u64) -> Result<Value, String> {
    let mut fallback = None;
    for line in body.lines() {
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(data.trim()) else {
            continue;
        };
        if value.get("id").and_then(Value::as_u64) == Some(id) {
            return Ok(value);
        }
        if fallback.is_none() && (value.get("result").is_some() || value.get("error").is_some()) {
            fallback = Some(value);
        }
    }
    fallback.ok_or_else(|| "MCP SSE 响应中没有匹配的 JSON-RPC 消息".to_string())
}
