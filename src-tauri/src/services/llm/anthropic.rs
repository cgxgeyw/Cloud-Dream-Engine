use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::client::{ChatMessage, ChatRequest, ChatResponse, ChatToolCall, ChatToolChoice, Usage};

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: i32,
    messages: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    temperature: Option<f64>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct AnthropicContent {
    #[serde(rename = "type")]
    content_type: Option<String>,
    text: Option<String>,
    thinking: Option<String>,
    id: Option<String>,
    name: Option<String>,
    input: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct AnthropicUsage {
    input_tokens: Option<i32>,
    output_tokens: Option<i32>,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
    usage: Option<AnthropicUsage>,
}

fn build_anthropic_request(request: &ChatRequest, tool_mode: bool) -> AnthropicRequest {
    let mut system_parts = Vec::new();
    let mut messages = Vec::new();
    for message in &request.messages {
        match message.role.as_str() {
            "system" => {
                let text = message.content_text();
                if !text.trim().is_empty() {
                    system_parts.push(text);
                }
            }
            _ => messages.push(build_anthropic_message(message)),
        }
    }

    let include_tools = tool_mode
        && request.native_tool_calling.unwrap_or(false)
        && request
            .tools
            .as_ref()
            .map(|tools| !tools.is_empty())
            .unwrap_or(false);

    AnthropicRequest {
        model: request.model.clone(),
        max_tokens: request.max_tokens.unwrap_or(4096),
        messages,
        system: (!system_parts.is_empty()).then(|| system_parts.join("\n\n")),
        temperature: request.temperature,
        stream: false,
        tools: include_tools.then(|| {
            request
                .tools
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(|tool| {
                    serde_json::json!({
                        "name": tool.name,
                        "description": tool.description,
                        "input_schema": tool.input_schema,
                    })
                })
                .collect::<Vec<_>>()
        }),
        tool_choice: include_tools.then(|| build_anthropic_tool_choice(request)),
    }
}

/// 将 OpenAI 格式的多媒体内容转换为 Anthropic 格式
fn convert_content_part_to_anthropic(part: &serde_json::Value) -> Option<serde_json::Value> {
    let part_type = part.get("type")?.as_str()?;
    match part_type {
        "text" => Some(serde_json::json!({
            "type": "text",
            "text": part.get("text")?.as_str().unwrap_or("")
        })),
        "image_url" => {
            // OpenAI: { "type": "image_url", "image_url": { "url": "data:image/jpeg;base64,..." } }
            let url = part.get("image_url")?.get("url")?.as_str()?;
            let (media_type, data) = parse_data_url(url)?;
            Some(serde_json::json!({
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": media_type,
                    "data": data
                }
            }))
        }
        "input_audio" => {
            // OpenAI: { "type": "input_audio", "input_audio": { "data": "data:audio/wav;base64,...", "format": "wav" } }
            let audio = part.get("input_audio")?;
            let data_url = audio.get("data")?.as_str()?;
            let format = audio.get("format")?.as_str().unwrap_or("wav");
            // data_url 可能是 "data:audio/wav;base64,..." 或纯 base64
            let base64_data = if data_url.starts_with("data:") {
                parse_data_url(data_url).map(|(_, d)| d).unwrap_or_else(|| data_url.to_string())
            } else {
                data_url.to_string()
            };
            Some(serde_json::json!({
                "type": "text",
                "text": format!("[音频数据: {} 格式, base64 长度 {} 字符]", format, base64_data.len())
            }))
        }
        _ => None,
    }
}

/// 解析 data URL，返回 (media_type, base64_data)
fn parse_data_url(url: &str) -> Option<(String, String)> {
    let url = url.trim();
    if !url.starts_with("data:") {
        return None;
    }
    let after_scheme = &url[5..];
    let semicolon_pos = after_scheme.find(';')?;
    let media_type = after_scheme[..semicolon_pos].to_string();
    let after_semicolon = &after_scheme[semicolon_pos + 1..];
    if !after_semicolon.starts_with("base64,") {
        return None;
    }
    let data = after_semicolon[7..].to_string();
    Some((media_type, data))
}

fn build_anthropic_message(message: &ChatMessage) -> serde_json::Value {
    match message.role.as_str() {
        "assistant" => {
            let mut blocks = Vec::new();
            if !message.content_is_empty() {
                blocks.push(serde_json::json!({
                    "type": "text",
                    "text": message.content_text(),
                }));
            }
            for tool_call in message.tool_calls.clone().unwrap_or_default() {
                blocks.push(serde_json::json!({
                    "type": "tool_use",
                    "id": tool_call.id,
                    "name": tool_call.tool_name,
                    "input": tool_call.arguments,
                }));
            }
            serde_json::json!({
                "role": "assistant",
                "content": if blocks.len() == 1 && blocks[0].get("type").and_then(|value| value.as_str()) == Some("text") {
                    serde_json::Value::String(message.content_text())
                } else {
                    serde_json::Value::Array(blocks)
                },
            })
        }
        "tool" => serde_json::json!({
            "role": "user",
            "content": [{
                "type": "tool_result",
                "tool_use_id": message.tool_call_id.clone().unwrap_or_default(),
                "content": message.content_text(),
            }],
        }),
        _ => {
            // 处理多媒体内容
            if message.content_is_multimedia() {
                if let serde_json::Value::Array(parts) = &message.content {
                    let anthropic_parts: Vec<serde_json::Value> = parts
                        .iter()
                        .filter_map(convert_content_part_to_anthropic)
                        .collect();
                    if !anthropic_parts.is_empty() {
                        return serde_json::json!({
                            "role": message.role,
                            "content": anthropic_parts,
                        });
                    }
                }
            }
            // 纯文本回退
            serde_json::json!({
                "role": message.role,
                "content": message.content_text(),
            })
        }
    }
}

fn build_anthropic_tool_choice(request: &ChatRequest) -> serde_json::Value {
    match request.tool_choice.clone().unwrap_or(ChatToolChoice::Auto) {
        ChatToolChoice::Auto => serde_json::json!({ "type": "auto" }),
        ChatToolChoice::None => serde_json::json!({ "type": "auto" }),
        ChatToolChoice::Required => serde_json::json!({ "type": "any" }),
        ChatToolChoice::Named(name) => serde_json::json!({
            "type": "tool",
            "name": name,
        }),
    }
}

pub async fn chat_completion(
    client: &Client,
    base_url: &str,
    api_key: &str,
    request: &ChatRequest,
) -> Result<ChatResponse, String> {
    let url = format!("{}/v1/messages", base_url.trim_end_matches('/'));
    let tool_mode = true;
    let response = client
        .post(&url)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("Content-Type", "application/json")
        .json(&build_anthropic_request(request, tool_mode))
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    loop {
        if response.status().is_success() {
            break;
        }
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("API error {}: {}", status, body));
    }

    let anthropic_response: AnthropicResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    let mut content = String::new();
    let mut reasoning = None;
    let mut tool_calls = Vec::new();

    for block in &anthropic_response.content {
        match block.content_type.as_deref() {
            Some("text") => {
                if let Some(text) = &block.text {
                    content.push_str(text);
                }
            }
            Some("thinking") => {
                if let Some(thinking) = &block.thinking {
                    reasoning = Some(thinking.clone());
                }
            }
            Some("tool_use") => {
                let name = block.name.as_deref().map(str::trim).unwrap_or("");
                if !name.is_empty() {
                    tool_calls.push(ChatToolCall {
                        id: block
                            .id
                            .as_deref()
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .unwrap_or("tool-call")
                            .to_string(),
                        tool_name: name.to_string(),
                        arguments: block.input.clone().unwrap_or_else(|| serde_json::json!({})),
                    });
                }
            }
            _ => {
                if let Some(text) = &block.text {
                    content.push_str(text);
                }
            }
        }
    }

    let usage = anthropic_response.usage.map(|u| Usage {
        prompt_tokens: u.input_tokens.unwrap_or(0),
        completion_tokens: u.output_tokens.unwrap_or(0),
        total_tokens: u.input_tokens.unwrap_or(0) + u.output_tokens.unwrap_or(0),
    });

    Ok(ChatResponse {
        content,
        reasoning,
        tool_calls: if tool_calls.is_empty() {
            None
        } else {
            Some(tool_calls)
        },
        usage,
    })
}
