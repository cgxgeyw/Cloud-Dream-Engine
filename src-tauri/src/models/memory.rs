use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub world_id: String,
    pub session_id: String,
    pub character_id: String,
    pub layer: String,
    pub content: String,
    pub source: String,
    pub importance: f64,
    pub created_at: String,
    pub turn_index: i32,
    pub conversation_id: Option<String>,
    pub event_id: Option<String>,
    pub item_id: Option<String>,
    pub scene_id: Option<String>,
    pub memory_type: String,
    pub speaker: Option<String>,
    pub role: Option<String>,
    pub location: Option<String>,
    pub participants: Vec<String>,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryQueryParams {
    pub world_id: Option<String>,
    pub session_id: Option<String>,
    pub character_id: Option<String>,
    pub layer: Option<String>,
    pub limit: Option<i32>,
}
