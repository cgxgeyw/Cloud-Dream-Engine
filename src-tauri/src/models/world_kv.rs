use serde::{Deserialize, Serialize};

/// 变量的属主类型：world（世界，跨存档共享）/ session（世界存档）/ character（存档内角色）。
pub const KV_OWNER_WORLD: &str = "world";
pub const KV_OWNER_SESSION: &str = "session";
pub const KV_OWNER_CHARACTER: &str = "character";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldKvEntry {
    pub owner_type: String,
    pub owner_id: String,
    pub namespace: String,
    pub key: String,
    pub value: serde_json::Value,
    pub updated_at: String,
}
