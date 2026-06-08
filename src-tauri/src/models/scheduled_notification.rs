use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledNotification {
    pub id: String,
    pub session_id: String,
    pub world_name: String,
    pub source: String,
    pub title: String,
    pub body: String,
    pub scheduled_at: String,
    pub created_at: String,
    pub fired_at: Option<String>,
    pub status: String,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct ScheduledNotificationCreate {
    pub session_id: String,
    pub world_name: String,
    pub source: String,
    pub title: String,
    pub body: String,
    pub scheduled_at: String,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingScheduledNotification {
    pub tool_call_id: String,
    pub source: String,
    pub title: String,
    pub body: String,
    pub requested_time: String,
    pub scheduled_at: String,
    pub arguments: serde_json::Value,
}
