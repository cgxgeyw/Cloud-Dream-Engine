use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldRecord {
    pub id: String,
    pub world_id: String,
    pub collection: String,
    pub data: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldRecordWriteRequest {
    pub collection: String,
    pub data: serde_json::Value,
}
