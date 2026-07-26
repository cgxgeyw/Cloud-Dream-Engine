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
    /// 会话级生成参数（第 8 项三级覆盖的最后一层，优先级最高）。
    /// 这是玩家偏好而非游戏状态：回滚/重新生成不还原它（见 rollback_session_to_turn）。
    #[serde(default)]
    pub generation_params: crate::models::generation_params::GenerationParams,
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

    /// 当前内容携带的媒体 parts（图片/音频等非文本部分），纯文本时为空。
    pub fn media_parts(&self) -> Vec<ContentPart> {
        match self {
            Self::Text(_) => Vec::new(),
            Self::Multipart(parts) => parts
                .iter()
                .filter(|part| part.part_type != "text")
                .cloned()
                .collect(),
        }
    }

    /// 给 prompt 用的文本形态：文本原样，媒体 parts 变成简短占位（[图片] / [音频 N 秒]）。
    /// 历史消息渲染用——媒体 base64 只随当前回合发送，历史里不重复携带（第 10 项）。
    pub fn as_prompt_text(&self) -> String {
        match self {
            Self::Text(s) => s.clone(),
            Self::Multipart(parts) => {
                let mut out = String::new();
                for part in parts {
                    match part.part_type.as_str() {
                        "text" => {
                            if let Some(text) = &part.text {
                                out.push_str(text);
                            }
                        }
                        "image_url" => out.push_str(" [图片]"),
                        "input_audio" => {
                            let placeholder = part
                                .input_audio
                                .as_ref()
                                .and_then(|audio| audio.duration_secs)
                                .filter(|secs| secs.is_finite() && *secs > 0.0)
                                .map(|secs| format!(" [音频 {} 秒]", secs.round().max(1.0) as u64))
                                .unwrap_or_else(|| " [音频]".to_string());
                            out.push_str(&placeholder);
                        }
                        _ => {}
                    }
                }
                out
            }
        }
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
    /// 录音时长（秒），由客户端在采集时写入；旧消息可能没有。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<f64>,
}

fn new_message_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// 把「文本 + 媒体 parts」组装成发往 OpenAI 兼容端点的 message content：
/// 无媒体时保持字符串（行为与纯文本时代完全一致）；有媒体时为
/// `[{type:"text",...}, {type:"image_url",...}, {type:"input_audio",...}]` 数组。
/// 落线前做两件归一化：剥掉 input_audio.data 的 data URL 前缀（OpenAI 要裸 base64）、
/// 摘掉内部字段 duration_secs（端点不认）。
pub fn build_wire_content(text: &str, media: &[ContentPart]) -> serde_json::Value {
    if media.is_empty() {
        return serde_json::Value::String(text.to_string());
    }
    let mut parts = Vec::with_capacity(media.len() + 1);
    parts.push(serde_json::json!({ "type": "text", "text": text }));
    for part in media {
        let mut value = serde_json::to_value(part).unwrap_or(serde_json::Value::Null);
        if part.part_type == "input_audio" {
            if let Some(audio) = value.get_mut("input_audio").and_then(|v| v.as_object_mut()) {
                audio.remove("duration_secs");
                if let Some(data) = audio.get("data").and_then(|v| v.as_str()) {
                    let stripped = strip_data_url_prefix(data);
                    if stripped != data {
                        audio.insert(
                            "data".to_string(),
                            serde_json::Value::String(stripped.to_string()),
                        );
                    }
                }
            }
        }
        parts.push(value);
    }
    serde_json::Value::Array(parts)
}

/// "data:audio/wav;base64,AAAA..." → "AAAA..."；非 data URL 原样返回。
fn strip_data_url_prefix(value: &str) -> &str {
    let trimmed = value.trim();
    if !trimmed.starts_with("data:") {
        return trimmed;
    }
    match trimmed.find(",") {
        Some(index) => &trimmed[index + 1..],
        None => trimmed,
    }
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// 稳定消息 ID。写入时生成；旧数据反序列化缺省时补发（开发期允许清库重建，
    /// 新协议数据从写入起就必须携带）。消息寻址用 session_id + message_id。
    #[serde(default = "new_message_id")]
    pub message_id: String,
    pub role: String,
    #[serde(default)]
    pub content: MessageContent,
    pub speaker: Option<String>,
    pub metadata: Option<serde_json::Value>,
    /// RFC3339 写入时间；旧数据可能为空串。
    #[serde(default)]
    pub created_at: String,
    /// 分支/重生成场景下来源消息的 ID；普通消息为 None。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_message_id: Option<String>,
}

impl ChatMessage {
    /// 生成新的稳定消息 ID。
    pub fn generate_id() -> String {
        new_message_id()
    }

    // new/with_metadata 目前由测试和第 2 项（重新生成）使用。
    #[allow(dead_code)]
    pub fn new(
        role: impl Into<String>,
        content: MessageContent,
        speaker: Option<String>,
    ) -> Self {
        Self {
            message_id: new_message_id(),
            role: role.into(),
            content,
            speaker,
            metadata: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            parent_message_id: None,
        }
    }

    #[allow(dead_code)] // 同上，第 2 项使用
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

impl SessionSnapshot {
    /// 按稳定 ID 定位消息。
    #[allow(dead_code)] // 由 load_message_runtime_proposals 和第 2 项（重新生成）使用
    pub fn find_message(&self, message_id: &str) -> Option<&ChatMessage> {
        self.messages
            .iter()
            .find(|message| message.message_id == message_id)
    }

    /// 按稳定 ID 编辑消息内容；消息不存在时返回 false。
    pub fn edit_message_content(
        &mut self,
        message_id: &str,
        new_content: MessageContent,
    ) -> bool {
        let Some(message) = self
            .messages
            .iter_mut()
            .find(|message| message.message_id == message_id)
        else {
            return false;
        };
        message.content = new_content;
        true
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn image_part() -> ContentPart {
        ContentPart {
            part_type: "image_url".to_string(),
            text: None,
            image_url: Some(ImageUrl {
                url: "data:image/png;base64,QUJD".to_string(),
            }),
            input_audio: None,
        }
    }

    fn audio_part(duration: Option<f64>) -> ContentPart {
        ContentPart {
            part_type: "input_audio".to_string(),
            text: None,
            image_url: None,
            input_audio: Some(InputAudio {
                data: "data:audio/wav;base64,QUJD".to_string(),
                format: "wav".to_string(),
                duration_secs: duration,
            }),
        }
    }

    #[test]
    fn media_parts_returns_only_non_text_parts() {
        let content = MessageContent::Multipart(vec![
            image_part(),
            audio_part(None),
            ContentPart {
                part_type: "text".to_string(),
                text: Some("看图".to_string()),
                image_url: None,
                input_audio: None,
            },
        ]);
        let media = content.media_parts();
        assert_eq!(media.len(), 2);
        assert!(media.iter().all(|part| part.part_type != "text"));
        assert!(MessageContent::Text("纯文本".to_string())
            .media_parts()
            .is_empty());
    }

    #[test]
    fn prompt_text_renders_media_placeholders() {
        let content = MessageContent::Multipart(vec![
            ContentPart {
                part_type: "text".to_string(),
                text: Some("看这个".to_string()),
                image_url: None,
                input_audio: None,
            },
            image_part(),
            audio_part(Some(7.6)),
            audio_part(None),
        ]);
        assert_eq!(content.as_prompt_text(), "看这个 [图片] [音频 8 秒] [音频]");
        assert_eq!(
            MessageContent::Text("纯文本".to_string()).as_prompt_text(),
            "纯文本"
        );
    }

    #[test]
    fn wire_content_stays_a_plain_string_without_media() {
        let value = build_wire_content("hello", &[]);
        assert_eq!(value, serde_json::json!("hello"));
    }

    #[test]
    fn wire_content_normalizes_media_parts_for_openai() {
        let value = build_wire_content(" payload ", &[image_part(), audio_part(Some(3.0))]);
        let parts = value.as_array().expect("multipart content");
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], serde_json::json!({ "type": "text", "text": " payload " }));
        assert_eq!(
            parts[1].pointer("/image_url/url").and_then(|v| v.as_str()),
            Some("data:image/png;base64,QUJD")
        );
        // OpenAI 的 input_audio.data 要裸 base64，且不带内部字段 duration_secs。
        let audio = parts[2].get("input_audio").expect("audio part");
        assert_eq!(audio.get("data").and_then(|v| v.as_str()), Some("QUJD"));
        assert!(audio.get("duration_secs").is_none());
    }
}
