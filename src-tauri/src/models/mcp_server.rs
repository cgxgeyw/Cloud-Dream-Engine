use serde::{Deserialize, Serialize};

/// MCP server 连接配置（第 7 项）。
/// 传输方式：
///   stdio —— 本地子进程，仅桌面端；安卓不能起子进程。
///   http  —— Streamable HTTP / SSE，全平台可用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    /// "stdio" | "http"
    pub transport: String,
    /// stdio: 可执行文件
    pub command: String,
    /// stdio: 命令行参数
    pub args: Vec<String>,
    /// stdio: 附加环境变量（对象，值为字符串）
    pub env: serde_json::Value,
    /// http: 端点地址
    pub url: String,
    /// http: 附加请求头（对象，值为字符串）
    pub headers: serde_json::Value,
    /// http: 凭据，非空时作为 Authorization: Bearer 发送。不在留痕中回显。
    pub auth_token: String,
    pub enabled: bool,
    /// 单次调用超时（毫秒）
    pub timeout_ms: i64,
    /// 单次调用结果大小上限（字节）
    pub max_result_bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerUpsertRequest {
    pub name: String,
    pub transport: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: serde_json::Value,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub headers: serde_json::Value,
    #[serde(default)]
    pub auth_token: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub timeout_ms: i64,
    #[serde(default)]
    pub max_result_bytes: i64,
}

fn default_true() -> bool {
    true
}

pub const DEFAULT_MCP_TIMEOUT_MS: i64 = 15_000;
pub const MAX_MCP_TIMEOUT_MS: i64 = 120_000;
pub const DEFAULT_MCP_MAX_RESULT_BYTES: i64 = 32_768;
pub const MAX_MCP_MAX_RESULT_BYTES: i64 = 1_048_576;

pub const TRANSPORT_STDIO: &str = "stdio";
pub const TRANSPORT_HTTP: &str = "http";

/// 当前平台是否支持该传输方式。安卓无法起本地子进程，stdio 不可用。
pub fn transport_supported_on_platform(transport: &str) -> bool {
    match transport {
        TRANSPORT_STDIO => !cfg!(target_os = "android"),
        TRANSPORT_HTTP => true,
        _ => false,
    }
}

/// 平台不支持时的明确原因（用于 UI 与工具结果错误）。
pub fn transport_unsupported_reason(transport: &str) -> String {
    match transport {
        TRANSPORT_STDIO => {
            "本平台不支持 stdio 传输：安卓无法启动本地子进程，请改用 http 方式连接远程 MCP server"
                .to_string()
        }
        TRANSPORT_HTTP => String::new(),
        other => format!("未知的 MCP 传输方式：{other}"),
    }
}

impl McpServerConfig {
    /// 连接前的可用性检查：启用状态、传输支持、必填字段。
    pub fn validate_ready(&self) -> Result<(), String> {
        if !self.enabled {
            return Err(format!("MCP server 未启用：{}", self.name));
        }
        if !transport_supported_on_platform(&self.transport) {
            return Err(transport_unsupported_reason(&self.transport));
        }
        match self.transport.as_str() {
            TRANSPORT_STDIO if self.command.trim().is_empty() => {
                Err("stdio 传输缺少启动命令".to_string())
            }
            TRANSPORT_HTTP if self.url.trim().is_empty() => {
                Err("http 传输缺少端点地址".to_string())
            }
            TRANSPORT_STDIO | TRANSPORT_HTTP => Ok(()),
            other => Err(format!("未知的 MCP 传输方式：{other}")),
        }
    }

    pub fn resolved_timeout_ms(&self) -> u64 {
        self.timeout_ms.clamp(1_000, MAX_MCP_TIMEOUT_MS) as u64
    }

    pub fn resolved_max_result_bytes(&self) -> usize {
        self.max_result_bytes.clamp(1_024, MAX_MCP_MAX_RESULT_BYTES) as usize
    }

    /// 字符串映射字段（env / headers）取值，忽略非字符串项。
    pub fn string_map(value: &serde_json::Value) -> Vec<(String, String)> {
        value
            .as_object()
            .map(|object| {
                object
                    .iter()
                    .filter_map(|(key, value)| {
                        value.as_str().map(|value| (key.clone(), value.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}
