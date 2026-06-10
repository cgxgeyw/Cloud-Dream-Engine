use serde::{Deserialize, Serialize};

pub const MCP_TOOL_SCHEDULE_NOTIFICATION_ID: &str = "mcp-tool-schedule-notification";

pub fn director_config_allows_mcp_tool(
    director_config: &serde_json::Value,
    tool_id: &str,
) -> bool {
    director_config
        .get("allowed_mcp_tool_ids")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(str::trim)
                .any(|item| item == tool_id)
        })
        .unwrap_or(false)
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
}
