use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDefinition {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub description: String,
    pub hooks: Vec<String>,
}
