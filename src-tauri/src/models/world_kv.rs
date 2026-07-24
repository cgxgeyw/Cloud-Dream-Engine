use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldKvEntry {
    pub world_id: String,
    pub namespace: String,
    pub key: String,
    pub value: serde_json::Value,
    pub updated_at: String,
}
