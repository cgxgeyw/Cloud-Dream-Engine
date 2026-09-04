use serde::{Deserialize, Serialize};

pub const MCP_TOOL_SCHEDULE_NOTIFICATION_ID: &str = "mcp-tool-schedule-notification";

/// 内置工具（引擎自带实现，不走外部 MCP server）。
pub fn is_builtin_mcp_tool_id(id: &str) -> bool {
    matches!(
        id,
        "mcp-tool-list-scenes"
            | "mcp-tool-list-characters"
            | "mcp-tool-change-scene"
            | "mcp-tool-switch-player-character"
            | "mcp-tool-image-generation"
    ) || id == MCP_TOOL_SCHEDULE_NOTIFICATION_ID
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub server_name: String,
    pub tool_name: String,
    pub enabled: bool,
    pub exposure_policy: serde_json::Value,
    pub risk_level: String,
    pub trigger_keywords: Vec<String>,
    pub input_schema: serde_json::Value,
    /// 绑定的 MCP server（mcp_servers.id）。为空表示未绑定，不会下发给模型。
    #[serde(default)]
    pub server_id: String,
    /// 实现方式："mcp"（默认，走外部 server）| "builtin_http"（Rust 核心直接执行，无需 server）。
    #[serde(default = "default_impl_kind")]
    pub impl_kind: String,
    /// builtin_http 的执行配置（generic / template 两种模式，见 services::mcp::local）。
    #[serde(default)]
    pub impl_config: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolCreateRequest {
    pub name: String,
    pub description: String,
    pub server_name: String,
    pub tool_name: String,
    pub enabled: bool,
    pub exposure_policy: serde_json::Value,
    pub risk_level: String,
    pub trigger_keywords: Vec<String>,
    pub input_schema: serde_json::Value,
    #[serde(default)]
    pub server_id: String,
    #[serde(default = "default_impl_kind")]
    pub impl_kind: String,
    #[serde(default)]
    pub impl_config: serde_json::Value,
}

pub const MCP_TOOL_IMPL_MCP: &str = "mcp";
pub const MCP_TOOL_IMPL_BUILTIN_HTTP: &str = "builtin_http";

fn default_impl_kind() -> String {
    MCP_TOOL_IMPL_MCP.to_string()
}

/// 本地工具：由 Rust 核心直接执行，不需要绑定外部 MCP server。
pub fn is_local_tool(tool: &McpToolDefinition) -> bool {
    tool.impl_kind.trim() == MCP_TOOL_IMPL_BUILTIN_HTTP
}
