use serde::{Deserialize, Serialize};

pub const MCP_TOOL_SCHEDULE_NOTIFICATION_ID: &str = "mcp-tool-schedule-notification";
pub const MCP_TOOL_SEARCH_KNOWLEDGE_ID: &str = "mcp-tool-search-knowledge";
pub const MCP_TOOL_LIST_SCENES_ID: &str = "mcp-tool-list-scenes";
pub const MCP_TOOL_LIST_CHARACTERS_ID: &str = "mcp-tool-list-characters";
pub const MCP_TOOL_CHANGE_SCENE_ID: &str = "mcp-tool-change-scene";
pub const MCP_TOOL_SWITCH_PLAYER_CHARACTER_ID: &str = "mcp-tool-switch-player-character";
pub const MCP_TOOL_BACKGROUND_GENERATION_ID: &str = "mcp-tool-background-generation";
pub const MCP_TOOL_PORTRAIT_GENERATION_ID: &str = "mcp-tool-portrait-generation";

/// 只有世界主控能使用、角色能力表必须排除的内置工具。
pub fn is_director_only_mcp_tool_id(id: &str) -> bool {
    matches!(
        id,
        MCP_TOOL_LIST_SCENES_ID
            | MCP_TOOL_LIST_CHARACTERS_ID
            | MCP_TOOL_CHANGE_SCENE_ID
            | MCP_TOOL_SWITCH_PLAYER_CHARACTER_ID
            | MCP_TOOL_BACKGROUND_GENERATION_ID
            | MCP_TOOL_PORTRAIT_GENERATION_ID
    )
}

/// 内置工具（引擎自带实现，不走外部 MCP server）。
pub fn is_builtin_mcp_tool_id(id: &str) -> bool {
    is_director_only_mcp_tool_id(id)
        || matches!(id, MCP_TOOL_SEARCH_KNOWLEDGE_ID)
        || id == MCP_TOOL_SCHEDULE_NOTIFICATION_ID
}

/// 世界配置中的规范工具授权。只读白名单，旧布尔字段不参与判断。
pub fn config_allows_mcp_tool(config: &serde_json::Value, tool_id: &str) -> bool {
    config
        .get("allowed_mcp_tool_ids")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .any(|id| id.trim() == tool_id)
        })
        .unwrap_or(false)
}

/// 生图总开关：运行时开关与两个主控工具授权必须同时成立。
pub fn config_allows_director_image_generation(
    director_config: &serde_json::Value,
    ui_theme_config: &serde_json::Value,
) -> bool {
    let runtime_enabled = ui_theme_config
        .get("assets")
        .and_then(|value| value.get("runtime_image_generation_enabled"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    runtime_enabled
        && config_allows_mcp_tool(director_config, MCP_TOOL_BACKGROUND_GENERATION_ID)
        && config_allows_mcp_tool(director_config, MCP_TOOL_PORTRAIT_GENERATION_ID)
}

/// 保存/读取世界时，以运行时生图开关同步两个专用工具的授权，并清理已移除的旧工具 id。
pub fn synchronize_director_image_generation_permissions(
    mut director_config: serde_json::Value,
    runtime_enabled: bool,
) -> serde_json::Value {
    if !director_config.is_object() {
        director_config = serde_json::json!({});
    }
    let Some(object) = director_config.as_object_mut() else {
        return director_config;
    };
    let mut allowed = object
        .get("allowed_mcp_tool_ids")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    allowed.retain(|id| {
        id != "mcp-tool-image-generation"
            && id != MCP_TOOL_BACKGROUND_GENERATION_ID
            && id != MCP_TOOL_PORTRAIT_GENERATION_ID
    });
    if runtime_enabled {
        for tool_id in [
            MCP_TOOL_BACKGROUND_GENERATION_ID,
            MCP_TOOL_PORTRAIT_GENERATION_ID,
        ] {
            if !allowed.iter().any(|id| id == tool_id) {
                allowed.push(tool_id.to_string());
            }
        }
    }
    object.insert(
        "allowed_mcp_tool_ids".to_string(),
        serde_json::Value::Array(allowed.into_iter().map(serde_json::Value::String).collect()),
    );
    director_config
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
