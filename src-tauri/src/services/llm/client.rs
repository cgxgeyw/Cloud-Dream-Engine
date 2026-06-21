use super::{anthropic, openai};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    /// 支持纯文本 (String) 或多媒体内容 (Array of ContentPart)
    pub content: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    pub speaker: Option<String>,
    pub tool_call_id: Option<String>,
    pub tool_calls: Option<Vec<ChatToolCall>>,
    pub metadata: Option<serde_json::Value>,
}

impl ChatMessage {
    /// 提取纯文本内容（如果是数组，拼接所有 text 部分）
    pub fn content_text(&self) -> String {
        match &self.content {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Array(parts) => parts
                .iter()
                .filter_map(|p| {
                    if p.get("type")?.as_str()? == "text" {
                        p.get("text")?.as_str().map(str::to_string)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join(""),
            _ => self.content.to_string(),
        }
    }

    /// 内容是否为空（纯文本或无文本部分）
    pub fn content_is_empty(&self) -> bool {
        self.content_text().trim().is_empty()
    }

    /// 内容是否为多媒体数组
    pub fn content_is_multimedia(&self) -> bool {
        matches!(self.content, serde_json::Value::Array(_))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatToolDefinition {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChatToolChoice {
    Auto,
    None,
    Required,
    Named(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatToolCall {
    pub id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<i32>,
    pub stream: Option<bool>,
    pub json_mode: Option<bool>,
    pub response_schema: Option<serde_json::Value>,
    pub tools: Option<Vec<ChatToolDefinition>>,
    pub tool_choice: Option<ChatToolChoice>,
    pub native_tool_calling: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub content: String,
    pub reasoning: Option<String>,
    pub tool_calls: Option<Vec<ChatToolCall>>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatStreamChunk {
    pub delta: String,
    pub reasoning_delta: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub total_tokens: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub owned_by: Option<String>,
}

pub struct LlmClient {
    http_client: reqwest::Client,
}

impl LlmClient {
    pub fn new() -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .unwrap_or_default();

        Self { http_client }
    }

    pub async fn chat_completion(
        &self,
        provider: &str,
        base_url: &str,
        api_key: &str,
        request: &ChatRequest,
    ) -> Result<ChatResponse, String> {
        match normalize_provider(provider).as_str() {
            "openai" | "ollama" | "lmstudio" => {
                openai::chat_completion(&self.http_client, base_url, api_key, request).await
            }
            "anthropic" => {
                anthropic::chat_completion(&self.http_client, base_url, api_key, request).await
            }
            _ => Err(format!("Unsupported provider: {}", provider)),
        }
    }

    pub async fn chat_completion_stream<F>(
        &self,
        provider: &str,
        base_url: &str,
        api_key: &str,
        request: &ChatRequest,
        on_chunk: F,
    ) -> Result<ChatResponse, String>
    where
        F: FnMut(ChatStreamChunk) + Send,
    {
        match normalize_provider(provider).as_str() {
            "openai" | "ollama" | "lmstudio" => {
                // openai 流式路径已支持原生工具调用增量累积，工具世界也走流式。
                openai::chat_completion_stream(
                    &self.http_client,
                    base_url,
                    api_key,
                    request,
                    on_chunk,
                )
                .await
            }
            "anthropic" => {
                anthropic::chat_completion(&self.http_client, base_url, api_key, request).await
            }
            _ => Err(format!("Unsupported provider: {}", provider)),
        }
    }

    pub async fn discover_models(
        &self,
        provider: &str,
        base_url: &str,
        api_key: &str,
    ) -> Result<Vec<ModelInfo>, String> {
        match normalize_provider(provider).as_str() {
            "openai" | "ollama" | "lmstudio" => {
                openai::list_models(&self.http_client, base_url, api_key).await
            }
            _ => Err(format!(
                "Model discovery not supported for provider: {}",
                provider
            )),
        }
    }
}

pub fn normalize_provider(provider: &str) -> String {
    match provider.trim().to_ascii_lowercase().as_str() {
        "openai-compatible" | "openai compatible" | "openai" => "openai".to_string(),
        "claude" | "anthropic" | "claude / anthropic" => "anthropic".to_string(),
        "lm studio" | "lmstudio" => "lmstudio".to_string(),
        "ollama" => "ollama".to_string(),
        _ => "openai".to_string(),
    }
}
