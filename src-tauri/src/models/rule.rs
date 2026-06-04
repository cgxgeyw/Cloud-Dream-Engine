use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleDefinition {
    pub id: String,
    pub scope: String,
    pub name: String,
    pub enabled: bool,
    pub priority: i32,
    pub description: String,
    pub condition: serde_json::Value,
    pub effects: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleCreateRequest {
    pub scope: String,
    pub name: String,
    pub enabled: bool,
    pub priority: i32,
    pub description: String,
    pub condition: serde_json::Value,
    pub effects: Vec<serde_json::Value>,
}
