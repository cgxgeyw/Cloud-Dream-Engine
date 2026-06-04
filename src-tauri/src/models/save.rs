use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveSummary {
    pub id: String,
    pub session_id: String,
    pub title: String,
    pub world_name: String,
    pub updated_at: String,
    pub progress: String,
    pub summary: String,
    pub player_character_name: Option<String>,
    pub parent_save_id: Option<String>,
    pub branch_root_save_id: Option<String>,
    pub branch_label: Option<String>,
    pub turn_index: i32,
}
