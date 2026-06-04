use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub id: String,
    pub world_name: String,
    pub location: String,
    pub time_label: String,
    pub current_speaker: String,
    pub current_line: String,
    pub player_character_id: String,
    pub player_character_name: String,
    pub visible_characters: Vec<String>,
    pub messages: Vec<ChatMessage>,
    pub player_stats: Vec<String>,
    pub map_graph_nodes: Vec<SessionMapNode>,
    pub map_graph_edges: Vec<SessionMapEdge>,
    pub inventory_items: Vec<InventoryItem>,
    pub system_log: Vec<String>,
    pub scene: SceneRuntime,
    pub assets: AssetSelection,
    pub state: SessionState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Multipart(Vec<ContentPart>),
}

impl MessageContent {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Text(s) => s.as_str(),
            Self::Multipart(parts) => {
                // Extract text from first text part, or empty string
                for p in parts {
                    if p.part_type == "text" {
                        if let Some(ref t) = p.text {
                            return t.as_str();
                        }
                    }
                }
                ""
            }
        }
    }

    pub fn as_dbg(&self) -> String {
        match self {
            Self::Text(s) => s.clone(),
            Self::Multipart(_) => "[multipart content]".to_string(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.as_str().is_empty()
    }

    pub fn trim(&self) -> String {
        self.as_str().trim().to_string()
    }

    pub fn contains(&self, pat: &str) -> bool {
        self.as_str().contains(pat)
    }

    pub fn extract_player_view_switch_speaker(&self) -> Option<String> {
        let s = self.as_str();
        if s.contains("player switch to ") {
            s.split("player switch to ")
                .nth(1)
                .map(|v| v.trim().to_string())
        } else {
            None
        }
    }
}

impl Default for MessageContent {
    fn default() -> Self {
        Self::Text(String::new())
    }
}

impl From<String> for MessageContent {
    fn from(s: String) -> Self {
        Self::Text(s)
    }
}

impl From<&str> for MessageContent {
    fn from(s: &str) -> Self {
        Self::Text(s.to_string())
    }
}

impl std::fmt::Display for MessageContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl PartialEq<&str> for MessageContent {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<MessageContent> for &str {
    fn eq(&self, other: &MessageContent) -> bool {
        *self == other.as_str()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentPart {
    #[serde(rename = "type")]
    pub part_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<ImageUrl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_audio: Option<InputAudio>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrl {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputAudio {
    pub data: String,
    pub format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    #[serde(default)]
    pub content: MessageContent,
    pub speaker: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMapNode {
    pub node_id: String,
    pub label: String,
    pub discovered: bool,
    pub current: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMapEdge {
    pub edge_id: String,
    pub source_node_id: String,
    pub target_node_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryItem {
    pub item_id: String,
    pub name: String,
    pub category: String,
    pub quantity: i32,
    pub description: String,
    pub tags: Vec<String>,
    pub owner_type: String,
    pub owner_id: String,
    pub visibility: String,
    pub disclosed_to: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SceneRuntime {
    pub scene_id: String,
    pub name: String,
    pub background_hint: String,
    pub temporary_tags: Vec<String>,
    pub present_characters: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AssetSelection {
    pub background_hint: String,
    pub active_speaker_portrait: String,
    pub background_asset_path: Option<String>,
    pub active_speaker_portrait_path: Option<String>,
    pub background_generation_prompt: String,
    pub active_speaker_generation_prompt: String,
    pub visible_character_portraits: Vec<CharacterVisualState>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CharacterVisualState {
    pub character_name: String,
    pub portrait_hint: String,
    pub portrait_asset_path: Option<String>,
    #[serde(default)]
    pub generation_prompt: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionState {
    pub metrics: HashMap<String, f64>,
    pub tags: Vec<String>,
    pub phase: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCreateRequest {
    pub world_id: String,
    pub player_character_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerActionMode {
    Submit,
    Resend,
    Edit,
}

impl PlayerActionMode {
    pub fn requires_replay(self) -> bool {
        matches!(self, Self::Resend | Self::Edit)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Submit => "submit",
            Self::Resend => "resend",
            Self::Edit => "edit",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerActionRequest {
    #[serde(default)]
    pub content: MessageContent,
    pub action_mode: PlayerActionMode,
    pub resend_from_turn_index: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryFailedLlmStepRequest {
    pub retry_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchCharacterRequest {
    pub player_character_id: String,
    pub proposal: Option<SwitchCharacterProposal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchCharacterProposal {
    pub target_character_name: Option<String>,
    pub reason: Option<String>,
    pub location: Option<String>,
    pub scene_name: Option<String>,
    pub scene_background_hint: Option<String>,
    pub scene_tags: Vec<String>,
    pub visible_characters: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRuntimeAttributesResponse {
    pub session_attributes: Vec<RuntimeAttributeGroup>,
    pub character_attributes: Vec<RuntimeAttributeGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeAttributeGroup {
    pub owner_type: String,
    pub owner_id: String,
    pub owner_label: String,
    pub items: Vec<RuntimeAttributeItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeAttributeItem {
    pub schema_id: String,
    pub key: String,
    pub label: String,
    pub value_type: String,
    pub value: serde_json::Value,
    pub source: String,
    pub display_policy: serde_json::Value,
    pub influence_policy: serde_json::Value,
}
