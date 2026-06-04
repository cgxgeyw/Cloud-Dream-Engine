use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSessionRecord {
    pub id: String,
    pub session_id: String,
    pub agent_type: String,
    pub status: String,
    pub connection_state: String,
    pub scene_presence_state: String,
    pub character_id: Option<String>,
    pub character_name: Option<String>,
    pub checkpoint_id: Option<String>,
    pub last_active_turn: i32,
    pub last_ack_message_index: i32,
    pub prompt_version: String,
    pub runtime_key: String,
    pub initialized_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCheckpointRecord {
    pub id: String,
    pub agent_session_id: String,
    pub turn_index: i32,
    pub checkpoint_type: String,
    pub payload: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnJournalEntryRecord {
    pub id: String,
    pub session_id: String,
    pub turn_index: i32,
    pub step: String,
    pub status: String,
    pub payload: serde_json::Value,
    pub created_at: String,
}
