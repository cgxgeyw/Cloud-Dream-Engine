use serde::{Deserialize, Serialize};

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
}
