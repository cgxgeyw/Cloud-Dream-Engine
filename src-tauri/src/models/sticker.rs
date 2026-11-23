use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StickerPack {
    pub id: String,
    /// `None` 表示公共图库；`Some(world_id)` 表示世界专属包。
    pub world_id: Option<String>,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub sticker_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StickerItem {
    pub id: String,
    pub pack_id: String,
    pub key: String,
    pub label: String,
    pub description: String,
    pub asset_path: String,
    pub sort_order: i64,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StickerPackCreateRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub world_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StickerPackUpdateRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub enabled: bool,
}
