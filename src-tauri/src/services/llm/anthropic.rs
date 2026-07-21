use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::client::{
    ChatMessage, ChatRequest, ChatResponse, ChatStreamChunk, ChatToolCall, ChatToolChoice, Usage,
};

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
    build_anthropic_request_with_stream(request, tool_mode, false)
}

fn build_anthropic_request_with_stream(
    request: &ChatRequest,
    tool_mode: bool,
    stream: bool,
) -> AnthropicRequest {
    let include_tools = tool_mode
        && request.native_tool_calling.unwrap_or(false)
        && request
            .tools
            .as_ref()
            .map(|tools| !tools.is_empty())
            .unwrap_or(false);

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
            // M8: 本次请求不带 tools 定义时,历史里的 tool_use/tool_result 块必须降级为文本,
            // 否则 Anthropic 看到 tool_use 块却无 tools 定义会返回 400。
            _ => messages.push(build_anthropic_message_with_tools(message, include_tools)),
        }
    }

    AnthropicRequest {
        model: request.model.clone(),
        max_tokens: request.max_tokens.unwrap_or(4096),
        messages,
        system: (!system_parts.is_empty()).then(|| system_parts.join("\n\n")),
        temperature: request.temperature,
        stream,
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
            // L12: text 字段缺失时退回空串而非丢弃整块,避免结构异常输入静默丢内容。
            "text": part.get("text").and_then(|value| value.as_str()).unwrap_or("")
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

fn build_anthropic_message_with_tools(
    message: &ChatMessage,
    include_tools: bool,
) -> serde_json::Value {
    match message.role.as_str() {
        "assistant" => {
            let mut blocks = Vec::new();
            if !message.content_is_empty() {
                blocks.push(serde_json::json!({
                    "type": "text",
                    "text": message.content_text(),
                }));
            }
            // M8: 不带 tools 定义时跳过 tool_use 块,避免 400。
            if include_tools {
                for tool_call in message.tool_calls.clone().unwrap_or_default() {
                    blocks.push(serde_json::json!({
                        "type": "tool_use",
                        "id": tool_call.id,
                        "name": tool_call.tool_name,
                        "input": tool_call.arguments,
                    }));
                }
            }
            // 全部块都被剥离时(仅有 tool_use 且 include_tools=false),退回空文本占位,
            // 保持 assistant 轮次存在但不含非法块。
            if blocks.is_empty() {
                return serde_json::json!({
                    "role": "assistant",
                    "content": String::new(),
                });
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
        "tool" => {
            // M8: 不带 tools 定义时,把 tool_result 降级为普通 user 文本,避免 400。
            if include_tools {
                serde_json::json!({
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": message.tool_call_id.clone().unwrap_or_default(),
                        "content": message.content_text(),
                    }],
                })
            } else {
                serde_json::json!({
                    "role": "user",
                    "content": message.content_text(),
                })
            }
        }
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

    if !response.status().is_success() {
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

/// H5: 流式工具调用累积器,按 content block index 聚合 id/name 与 input_json_delta 片段。
#[derive(Default)]
struct StreamingToolUse {
    id: String,
    name: String,
    input_json: String,
}

/// H5: Anthropic Messages API 的真流式实现(SSE)。
/// 事件流形如 `event: <type>\n data: <json>\n\n`;data 的 JSON 自带 `type` 字段,
/// 因此只解析 data 行即可。逐字 text_delta 通过 on_chunk 实时回调。
pub async fn chat_completion_stream<F>(
    client: &Client,
    base_url: &str,
    api_key: &str,
    request: &ChatRequest,
    mut on_chunk: F,
) -> Result<ChatResponse, String>
where
    F: FnMut(ChatStreamChunk) + Send,
{
    use futures::StreamExt;

    let url = format!("{}/v1/messages", base_url.trim_end_matches('/'));
    let response = client
        .post(&url)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("Content-Type", "application/json")
        .json(&build_anthropic_request_with_stream(request, true, true))
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("API error {}: {}", status, body));
    }

    let mut content = String::new();
    let mut reasoning = String::new();
    // 按 content block index 暂存工具调用片段。
    let mut tool_uses: std::collections::BTreeMap<usize, StreamingToolUse> =
        std::collections::BTreeMap::new();
    let mut usage: Option<Usage> = None;
    let mut prompt_tokens = 0;
    let mut completion_tokens = 0;

    let mut stream = response.bytes_stream();
    let mut pending = String::new();
    while let Some(chunk) = stream.next().await {
        let bytes = match chunk {
            Ok(bytes) => bytes,
            Err(error) => {
                // 与 openai 路径一致:已推送过内容则按部分完成返回,避免 UI 与落库不一致。
                if content.trim().is_empty() && reasoning.trim().is_empty() && tool_uses.is_empty() {
                    return Err(format!("Failed to read stream: {}", error));
                }
                break;
            }
        };
        pending.push_str(&String::from_utf8_lossy(&bytes));

        while let Some(split_at) = pending.find('\n') {
            let line = pending[..split_at].trim().to_string();
            pending = pending[split_at + 1..].to_string();
            if line.is_empty() || !line.starts_with("data:") {
                continue;
            }
            let data = line["data:".len()..].trim();
            if data.is_empty() {
                continue;
            }
            apply_anthropic_stream_event(
                data,
                &mut content,
                &mut reasoning,
                &mut tool_uses,
                &mut prompt_tokens,
                &mut completion_tokens,
                &mut on_chunk,
            );
        }
    }

    if prompt_tokens > 0 || completion_tokens > 0 {
        usage = Some(Usage {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
        });
    }

    let tool_calls = tool_uses
        .into_values()
        .filter(|item| !item.name.trim().is_empty())
        .enumerate()
        .map(|(position, item)| {
            let id = if item.id.trim().is_empty() {
                format!("tool-call-{position}")
            } else {
                item.id
            };
            let arguments = {
                let trimmed = item.input_json.trim();
                if trimmed.is_empty() {
                    serde_json::json!({})
                } else {
                    serde_json::from_str::<serde_json::Value>(trimmed)
                        .unwrap_or_else(|_| serde_json::json!({}))
                }
            };
            ChatToolCall {
                id,
                tool_name: item.name.trim().to_string(),
                arguments,
            }
        })
        .collect::<Vec<_>>();

    Ok(ChatResponse {
        content,
        reasoning: (!reasoning.trim().is_empty()).then(|| reasoning.clone()),
        tool_calls: if tool_calls.is_empty() {
            None
        } else {
            Some(tool_calls)
        },
        usage,
    })
}

#[allow(clippy::too_many_arguments)]
fn apply_anthropic_stream_event<F>(
    data: &str,
    content: &mut String,
    reasoning: &mut String,
    tool_uses: &mut std::collections::BTreeMap<usize, StreamingToolUse>,
    prompt_tokens: &mut i32,
    completion_tokens: &mut i32,
    on_chunk: &mut F,
) where
    F: FnMut(ChatStreamChunk) + Send,
{
    let Ok(event) = serde_json::from_str::<serde_json::Value>(data) else {
        return;
    };
    let event_type = event.get("type").and_then(|value| value.as_str()).unwrap_or("");
    match event_type {
        "message_start" => {
            if let Some(input) = event
                .get("message")
                .and_then(|m| m.get("usage"))
                .and_then(|u| u.get("input_tokens"))
                .and_then(|v| v.as_i64())
            {
                *prompt_tokens = input as i32;
            }
        }
        "content_block_start" => {
            let index = event.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            if let Some(block) = event.get("content_block") {
                if block.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
                    let slot = tool_uses.entry(index).or_default();
                    if let Some(id) = block.get("id").and_then(|v| v.as_str()) {
                        slot.id = id.to_string();
                    }
                    if let Some(name) = block.get("name").and_then(|v| v.as_str()) {
                        slot.name = name.to_string();
                    }
                }
            }
        }
        "content_block_delta" => {
            let index = event.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let Some(delta) = event.get("delta") else {
                return;
            };
            match delta.get("type").and_then(|v| v.as_str()) {
                Some("text_delta") => {
                    if let Some(text) = delta.get("text").and_then(|v| v.as_str()) {
                        if !text.is_empty() {
                            content.push_str(text);
                            on_chunk(ChatStreamChunk {
                                delta: text.to_string(),
                                reasoning_delta: None,
                            });
                        }
                    }
                }
                Some("thinking_delta") => {
                    if let Some(thinking) = delta.get("thinking").and_then(|v| v.as_str()) {
                        if !thinking.is_empty() {
                            reasoning.push_str(thinking);
                            on_chunk(ChatStreamChunk {
                                delta: String::new(),
                                reasoning_delta: Some(thinking.to_string()),
                            });
                        }
                    }
                }
                Some("input_json_delta") => {
                    if let Some(partial) = delta.get("partial_json").and_then(|v| v.as_str()) {
                        tool_uses.entry(index).or_default().input_json.push_str(partial);
                    }
                }
                _ => {}
            }
        }
        "message_delta" => {
            if let Some(output) = event
                .get("usage")
                .and_then(|u| u.get("output_tokens"))
                .and_then(|v| v.as_i64())
            {
                *completion_tokens = output as i32;
            }
        }
        _ => {}
    }
}
