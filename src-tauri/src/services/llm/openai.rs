use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::client::{
    ChatMessage, ChatRequest, ChatResponse, ChatStreamChunk, ChatToolCall, ChatToolChoice,
    ModelInfo, Usage,
};

#[derive(Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<serde_json::Value>,
    temperature: Option<f64>,
    max_tokens: Option<i32>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct OpenAIUsage {
    prompt_tokens: Option<i32>,
    completion_tokens: Option<i32>,
    total_tokens: Option<i32>,
}

#[derive(Deserialize)]
struct OpenAIChoice {
    message: OpenAIMessageResponse,
}

#[derive(Deserialize)]
struct OpenAIMessageResponse {
    content: Option<String>,
    reasoning_content: Option<String>,
    tool_calls: Option<Vec<OpenAIToolCallResponse>>,
}

#[derive(Deserialize)]
struct OpenAIToolCallResponse {
    id: Option<String>,
    function: Option<OpenAIFunctionCallResponse>,
}

#[derive(Deserialize)]
struct OpenAIFunctionCallResponse {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Deserialize)]
struct OpenAIResponse {
    choices: Vec<OpenAIChoice>,
    usage: Option<OpenAIUsage>,
}

#[derive(Deserialize)]
struct OpenAIModel {
    id: String,
    owned_by: Option<String>,
}

#[derive(Deserialize)]
struct OpenAIModelListResponse {
    data: Vec<OpenAIModel>,
}

#[derive(Clone, Copy)]
enum StructuredOutputMode {
    JsonObject,
    JsonSchema,
    Omit,
}

#[derive(Clone, Copy)]
enum ToolMode {
    Include,
    /// 保留：用于显式构造不带工具的请求（当前流式/非流式均走 Include，
    /// 由 build_openai_request 内部按 native_tool_calling 决定是否真正附带工具）。
    #[allow(dead_code)]
    Omit,
}

fn build_openai_request(
    request: &ChatRequest,
    structured_mode: StructuredOutputMode,
    tool_mode: ToolMode,
) -> OpenAIRequest {
    let include_tools = matches!(tool_mode, ToolMode::Include)
        && request.native_tool_calling.unwrap_or(false)
        && request
            .tools
            .as_ref()
            .map(|tools| !tools.is_empty())
            .unwrap_or(false);

    let response_format = if include_tools || request.json_mode != Some(true) {
        None
    } else {
        match structured_mode {
            StructuredOutputMode::JsonObject => Some(serde_json::json!({ "type": "json_object" })),
            StructuredOutputMode::JsonSchema => Some(serde_json::json!({
                "type": "json_schema",
                "json_schema": {
                    "name": "structured_response",
                    "schema": request.response_schema.clone().unwrap_or_else(|| serde_json::json!({
                        "type": "object",
                        "additionalProperties": true
                    }))
                }
            })),
            StructuredOutputMode::Omit => None,
        }
    };

    OpenAIRequest {
        model: request.model.clone(),
        messages: request.messages.iter().map(build_openai_message).collect(),
        temperature: request.temperature,
        max_tokens: request.max_tokens,
        stream: false,
        stream_options: None,
        response_format,
        tools: include_tools.then(|| build_openai_tools(request)),
        tool_choice: include_tools.then(|| build_openai_tool_choice(request)),
    }
}

fn build_openai_message(message: &ChatMessage) -> serde_json::Value {
    match message.role.as_str() {
        "assistant" => {
            let mut value = serde_json::json!({
                "role": "assistant",
                "content": if message.content_is_empty() && message.tool_calls.is_some() {
                    serde_json::Value::Null
                } else {
                    message.content.clone()
                }
            });
            if let Some(reasoning_content) = &message.reasoning_content {
                if let Some(object) = value.as_object_mut() {
                    object.insert(
                        "reasoning_content".to_string(),
                        serde_json::Value::String(reasoning_content.clone()),
                    );
                }
            }
            if let Some(tool_calls) = &message.tool_calls {
                let items = tool_calls
                    .iter()
                    .map(|tool_call| {
                        serde_json::json!({
                            "id": tool_call.id,
                            "type": "function",
                            "function": {
                                "name": tool_call.tool_name,
                                "arguments": serde_json::to_string(&tool_call.arguments)
                                    .unwrap_or_else(|_| "{}".to_string()),
                            }
                        })
                    })
                    .collect::<Vec<_>>();
                if let Some(object) = value.as_object_mut() {
                    object.insert("tool_calls".to_string(), serde_json::Value::Array(items));
                }
            }
            value
        }
        "tool" => serde_json::json!({
            "role": "tool",
            "content": message.content,
            "tool_call_id": message.tool_call_id.clone().unwrap_or_default(),
        }),
        _ => serde_json::json!({
            "role": message.role,
            "content": message.content,
        }),
    }
}

fn build_openai_tools(request: &ChatRequest) -> Vec<serde_json::Value> {
    request
        .tools
        .clone()
        .unwrap_or_default()
        .into_iter()
        .map(|tool| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.input_schema,
                }
            })
        })
        .collect()
}

fn build_openai_tool_choice(request: &ChatRequest) -> serde_json::Value {
    match request.tool_choice.clone().unwrap_or(ChatToolChoice::Auto) {
        ChatToolChoice::Auto => serde_json::json!("auto"),
        ChatToolChoice::None => serde_json::json!("none"),
        ChatToolChoice::Required => serde_json::json!("required"),
        ChatToolChoice::Named(name) => serde_json::json!({
            "type": "function",
            "function": { "name": name }
        }),
    }
}

fn supports_response_format_fallback(
    status: reqwest::StatusCode,
    body: &str,
    request: &ChatRequest,
) -> bool {
    if request.json_mode != Some(true) || status != reqwest::StatusCode::BAD_REQUEST {
        return false;
    }

    // Any 400 that mentions response_format while we are requesting structured
    // output means this endpoint rejects our response_format; drop it and retry.
    // (Different OpenAI-compatible providers word this many ways, e.g.
    // "This response_format type is unavailable now", "unsupported", etc.)
    body.to_ascii_lowercase().contains("response_format")
}

async fn send_chat_completion(
    client: &Client,
    url: &str,
    api_key: &str,
    openai_request: &OpenAIRequest,
) -> Result<reqwest::Response, String> {
    client
        .post(url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(openai_request)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))
}

#[derive(Deserialize)]
struct OpenAIStreamChoiceDelta {
    content: Option<String>,
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenAIStreamToolCallDelta>>,
}

#[derive(Deserialize)]
struct OpenAIStreamToolCallDelta {
    #[serde(default)]
    index: Option<usize>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<OpenAIStreamFunctionDelta>,
}

#[derive(Deserialize)]
struct OpenAIStreamFunctionDelta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

/// 流式工具调用分片累积器：按 index 聚合 id / function.name / arguments 片段。
#[derive(Default)]
struct StreamingToolCall {
    id: String,
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct OpenAIStreamChoice {
    delta: Option<OpenAIStreamChoiceDelta>,
}

#[derive(Deserialize)]
struct OpenAIStreamChunkResponse {
    choices: Option<Vec<OpenAIStreamChoice>>,
    usage: Option<OpenAIUsage>,
}

fn apply_stream_data_chunk<F>(
    data: &str,
    content: &mut String,
    reasoning: &mut String,
    tool_calls: &mut Vec<StreamingToolCall>,
    usage: &mut Option<Usage>,
    on_chunk: &mut F,
) -> bool
where
    F: FnMut(ChatStreamChunk) + Send,
{
    if data == "[DONE]" {
        return true;
    }
    let Ok(parsed) = serde_json::from_str::<OpenAIStreamChunkResponse>(data) else {
        return false;
    };
    // usage 通常出现在流末尾的独立 chunk（choices 为空）中，记录最后一次见到的值。
    if let Some(u) = parsed.usage {
        *usage = Some(Usage {
            prompt_tokens: u.prompt_tokens.unwrap_or(0),
            completion_tokens: u.completion_tokens.unwrap_or(0),
            total_tokens: u.total_tokens.unwrap_or(0),
        });
    }
    for choice in parsed.choices.unwrap_or_default() {
        let Some(delta) = choice.delta else {
            continue;
        };
        let content_delta = delta.content.unwrap_or_default();
        let reasoning_delta = delta.reasoning_content;
        if !content_delta.is_empty() {
            content.push_str(&content_delta);
        }
        if let Some(reasoning_piece) = &reasoning_delta {
            reasoning.push_str(reasoning_piece);
        }
        if let Some(call_deltas) = delta.tool_calls {
            accumulate_tool_call_deltas(tool_calls, call_deltas);
        }
        if !content_delta.is_empty() || reasoning_delta.is_some() {
            on_chunk(ChatStreamChunk {
                delta: content_delta,
                reasoning_delta,
            });
        }
    }
    false
}

/// 将一批工具调用分片并入累积缓冲。OpenAI 流式协议里同一个工具调用的
/// id/name/arguments 会跨多个 chunk 分片到达，用 index 对齐；缺失 index
/// 时退化为追加到最后一个调用。
fn accumulate_tool_call_deltas(
    tool_calls: &mut Vec<StreamingToolCall>,
    deltas: Vec<OpenAIStreamToolCallDelta>,
) {
    for delta in deltas {
        let index = delta.index.unwrap_or_else(|| tool_calls.len().saturating_sub(1));
        if index >= tool_calls.len() {
            tool_calls.resize_with(index + 1, StreamingToolCall::default);
        }
        let slot = &mut tool_calls[index];
        if let Some(id) = delta.id {
            if !id.trim().is_empty() {
                slot.id = id;
            }
        }
        if let Some(function) = delta.function {
            if let Some(name) = function.name {
                if !name.trim().is_empty() {
                    slot.name = name;
                }
            }
            if let Some(arguments) = function.arguments {
                slot.arguments.push_str(&arguments);
            }
        }
    }
}

/// 把累积好的流式工具调用组装成 ChatToolCall 列表（丢弃无名调用）。
fn finalize_streaming_tool_calls(tool_calls: Vec<StreamingToolCall>) -> Option<Vec<ChatToolCall>> {
    let calls = tool_calls
        .into_iter()
        .filter(|call| !call.name.trim().is_empty())
        .enumerate()
        .map(|(position, call)| {
            let id = if call.id.trim().is_empty() {
                format!("tool-call-{position}")
            } else {
                call.id
            };
            ChatToolCall {
                id,
                tool_name: call.name.trim().to_string(),
                arguments: parse_tool_call_arguments(&call.arguments),
            }
        })
        .collect::<Vec<_>>();
    if calls.is_empty() {
        None
    } else {
        Some(calls)
    }
}

fn extract_message_content(message: &OpenAIMessageResponse, request: &ChatRequest) -> String {
    let content = message.content.clone().unwrap_or_default();
    if !content.trim().is_empty() {
        return content;
    }

    let reasoning = message.reasoning_content.clone().unwrap_or_default();
    if request.json_mode == Some(true)
        && !reasoning.trim().is_empty()
        && serde_json::from_str::<serde_json::Value>(reasoning.trim())
            .map(|value| value.is_object() || value.is_array())
            .unwrap_or(false)
    {
        return reasoning;
    }

    String::new()
}

fn parse_tool_call_arguments(raw: &str) -> serde_json::Value {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return serde_json::json!({});
    }
    serde_json::from_str::<serde_json::Value>(trimmed)
        .unwrap_or_else(|_| serde_json::Value::String(trimmed.to_string()))
}

fn extract_tool_calls(message: &OpenAIMessageResponse) -> Option<Vec<ChatToolCall>> {
    let tool_calls = message
        .tool_calls
        .as_ref()?
        .iter()
        .filter_map(|item| {
            let function = item.function.as_ref()?;
            let name = function.name.as_deref()?.trim();
            if name.is_empty() {
                return None;
            }
            Some(ChatToolCall {
                id: item
                    .id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("tool-call")
                    .to_string(),
                tool_name: name.to_string(),
                arguments: parse_tool_call_arguments(function.arguments.as_deref().unwrap_or("{}")),
            })
        })
        .collect::<Vec<_>>();
    if tool_calls.is_empty() {
        None
    } else {
        Some(tool_calls)
    }
}

pub async fn chat_completion(
    client: &Client,
    base_url: &str,
    api_key: &str,
    request: &ChatRequest,
) -> Result<ChatResponse, String> {
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let mut structured_mode = StructuredOutputMode::JsonObject;
    let tool_mode = ToolMode::Include;

    let mut response = send_chat_completion(
        client,
        &url,
        api_key,
        &build_openai_request(request, structured_mode, tool_mode),
    )
    .await?;

    loop {
        if response.status().is_success() {
            break;
        }
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        // 仅在还能进一步降级 structured 模式时才重试，否则会无限循环：
        // 已经降到 Omit 仍失败说明问题不在 response_format，直接返回错误
        // （与 chat_completion_stream 的处理保持一致）。
        let next_mode = match structured_mode {
            StructuredOutputMode::JsonObject => Some(StructuredOutputMode::JsonSchema),
            StructuredOutputMode::JsonSchema => Some(StructuredOutputMode::Omit),
            StructuredOutputMode::Omit => None,
        };
        if let Some(next_mode) = next_mode {
            if supports_response_format_fallback(status, &body, request) {
                structured_mode = next_mode;
                response = send_chat_completion(
                    client,
                    &url,
                    api_key,
                    &build_openai_request(request, structured_mode, tool_mode),
                )
                .await?;
                continue;
            }
        }
        return Err(format!("API error {}: {}", status, body));
    }

    let openai_response: OpenAIResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    // 空 choices 说明服务端以 200 包装了异常（内容过滤、配额、上游错误等），
    // 不能当作“正常的空回复”静默返回，否则调用方无法区分成功与失败。
    if openai_response.choices.is_empty() {
        return Err("API returned no choices".to_string());
    }

    let content = openai_response
        .choices
        .first()
        .map(|c| extract_message_content(&c.message, request))
        .unwrap_or_default();
    let reasoning = openai_response
        .choices
        .first()
        .and_then(|c| c.message.reasoning_content.clone());
    let tool_calls = openai_response
        .choices
        .first()
        .and_then(|c| extract_tool_calls(&c.message));

    let usage = openai_response.usage.map(|u| Usage {
        prompt_tokens: u.prompt_tokens.unwrap_or(0),
        completion_tokens: u.completion_tokens.unwrap_or(0),
        total_tokens: u.total_tokens.unwrap_or(0),
    });

    Ok(ChatResponse {
        content,
        reasoning,
        tool_calls,
        usage,
    })
}

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
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let mut structured_mode = if request.json_mode == Some(true) {
        StructuredOutputMode::JsonObject
    } else {
        StructuredOutputMode::Omit
    };

    let response = loop {
        let streaming_request = OpenAIRequest {
            stream: true,
            // 请求服务端在流末尾回送一个带 usage 的 chunk，否则流式路径拿不到 token 计量。
            stream_options: Some(serde_json::json!({ "include_usage": true })),
            ..build_openai_request(request, structured_mode, ToolMode::Include)
        };
        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&streaming_request)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        if response.status().is_success() {
            break response;
        }

        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        // Some OpenAI-compatible endpoints reject response_format. Drop it and
        // retry (same escalation as the non-streaming path) instead of failing.
        if supports_response_format_fallback(status, &body, request) {
            structured_mode = match structured_mode {
                StructuredOutputMode::JsonObject => StructuredOutputMode::JsonSchema,
                StructuredOutputMode::JsonSchema => StructuredOutputMode::Omit,
                StructuredOutputMode::Omit => {
                    return Err(format!("API error {}: {}", status, body));
                }
            };
            continue;
        }
        return Err(format!("API error {}: {}", status, body));
    };

    let mut content = String::new();
    let mut reasoning = String::new();
    let mut tool_calls: Vec<StreamingToolCall> = Vec::new();
    let mut stream = response.bytes_stream();
    let mut pending = String::new();
    let mut finished = false;
    let mut usage: Option<Usage> = None;

    use futures::StreamExt;
    while let Some(chunk) = stream.next().await {
        let bytes = match chunk {
            Ok(bytes) => bytes,
            Err(error) => {
                // 流读取中途失败时，已累积的 delta 已经通过 on_chunk 推送到 UI。
                // 若已有内容/推理/工具调用，则按部分完成处理，返回已得内容，
                // 避免 UI 显示了半截回复却又收到整体失败、显示与落库不一致。
                if content.trim().is_empty()
                    && reasoning.trim().is_empty()
                    && tool_calls.is_empty()
                {
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
            if apply_stream_data_chunk(
                data,
                &mut content,
                &mut reasoning,
                &mut tool_calls,
                &mut usage,
                &mut on_chunk,
            ) {
                finished = true;
                break;
            }
        }
        if finished {
            break;
        }
    }

    if !finished {
        let tail = pending.trim();
        if let Some(data) = tail.strip_prefix("data:").map(str::trim) {
            apply_stream_data_chunk(
                data,
                &mut content,
                &mut reasoning,
                &mut tool_calls,
                &mut usage,
                &mut on_chunk,
            );
        }
    }

    Ok(ChatResponse {
        content: if content.trim().is_empty()
            && request.json_mode == Some(true)
            && serde_json::from_str::<serde_json::Value>(reasoning.trim())
                .map(|value| value.is_object() || value.is_array())
                .unwrap_or(false)
        {
            reasoning.clone()
        } else {
            content
        },
        reasoning: if reasoning.trim().is_empty() {
            None
        } else {
            Some(reasoning)
        },
        tool_calls: finalize_streaming_tool_calls(tool_calls),
        usage,
    })
}

pub async fn list_models(
    client: &Client,
    base_url: &str,
    api_key: &str,
) -> Result<Vec<ModelInfo>, String> {
    let url = format!("{}/models", base_url.trim_end_matches('/'));

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("API error {}: {}", status, body));
    }

    let model_list: OpenAIModelListResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    let models = model_list
        .data
        .iter()
        .map(|m| ModelInfo {
            id: m.id.clone(),
            name: m.id.clone(),
            owned_by: m.owned_by.clone(),
        })
        .collect();

    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::llm::client::{ChatToolChoice, ChatToolDefinition};

    #[test]
    fn retries_without_response_format_for_openai_compatible_400() {
        let request = ChatRequest {
            model: "test-model".to_string(),
            messages: vec![],
            temperature: Some(0.1),
            max_tokens: Some(32),
            stream: Some(false),
            json_mode: Some(true),
            response_schema: None,
            tools: None,
            tool_choice: None,
            native_tool_calling: None,
        };

        assert!(supports_response_format_fallback(
            reqwest::StatusCode::BAD_REQUEST,
            "{\"error\":\"'response_format.type' must be 'json_schema' or 'text'\"}",
            &request,
        ));
    }

    #[test]
    fn retries_for_response_format_unavailable_wording() {
        // Regression: deepseek-v4-flash style endpoints return this exact 400.
        let request = ChatRequest {
            model: "test-model".to_string(),
            messages: vec![],
            temperature: Some(0.1),
            max_tokens: Some(32),
            stream: Some(true),
            json_mode: Some(true),
            response_schema: None,
            tools: None,
            tool_choice: None,
            native_tool_calling: None,
        };

        assert!(supports_response_format_fallback(
            reqwest::StatusCode::BAD_REQUEST,
            "{\"error\":{\"message\":\"This response_format type is unavailable now\",\"type\":\"invalid_request_error\"}}",
            &request,
        ));
    }

    #[test]
    fn does_not_retry_for_non_json_requests() {
        let request = ChatRequest {
            model: "test-model".to_string(),
            messages: vec![],
            temperature: Some(0.1),
            max_tokens: Some(32),
            stream: Some(false),
            json_mode: Some(false),
            response_schema: None,
            tools: None,
            tool_choice: None,
            native_tool_calling: None,
        };

        assert!(!supports_response_format_fallback(
            reqwest::StatusCode::BAD_REQUEST,
            "{\"error\":\"unsupported response_format\"}",
            &request,
        ));
    }

    #[test]
    fn uses_reasoning_content_when_json_mode_returns_empty_content() {
        let request = ChatRequest {
            model: "test-model".to_string(),
            messages: vec![],
            temperature: Some(0.1),
            max_tokens: Some(32),
            stream: Some(false),
            json_mode: Some(true),
            response_schema: None,
            tools: None,
            tool_choice: None,
            native_tool_calling: None,
        };
        let message = OpenAIMessageResponse {
            content: Some(String::new()),
            reasoning_content: Some("{\"ok\":true}".to_string()),
            tool_calls: None,
        };

        assert_eq!(extract_message_content(&message, &request), "{\"ok\":true}");
    }

    #[test]
    fn ignores_non_json_reasoning_content_when_content_is_empty() {
        let request = ChatRequest {
            model: "test-model".to_string(),
            messages: vec![],
            temperature: Some(0.1),
            max_tokens: Some(32),
            stream: Some(false),
            json_mode: Some(true),
            response_schema: None,
            tools: None,
            tool_choice: None,
            native_tool_calling: None,
        };
        let message = OpenAIMessageResponse {
            content: None,
            reasoning_content: Some("Let me think step by step".to_string()),
            tool_calls: None,
        };

        assert!(extract_message_content(&message, &request).is_empty());
    }

    #[test]
    fn stream_done_chunk_reports_completion() {
        let mut content = String::new();
        let mut reasoning = String::new();
        let mut tool_calls = Vec::new();
        let mut usage = None;
        let mut chunks = Vec::new();

        let finished = apply_stream_data_chunk(
            "[DONE]",
            &mut content,
            &mut reasoning,
            &mut tool_calls,
            &mut usage,
            &mut |chunk| {
                chunks.push(chunk.delta);
            },
        );

        assert!(finished);
        assert!(content.is_empty());
        assert!(reasoning.is_empty());
        assert!(chunks.is_empty());
    }

    #[test]
    fn stream_accumulates_split_tool_call_deltas() {
        let mut content = String::new();
        let mut reasoning = String::new();
        let mut tool_calls = Vec::new();
        let mut usage = None;
        let mut noop = |_chunk: ChatStreamChunk| {};

        // 工具调用分片跨多个 chunk：先 id+name，再 arguments 分两段。
        apply_stream_data_chunk(
            "{\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"get_weather\",\"arguments\":\"{\\\"city\\\":\"}}]}}]}",
            &mut content,
            &mut reasoning,
            &mut tool_calls,
            &mut usage,
            &mut noop,
        );
        apply_stream_data_chunk(
            "{\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"北京\\\"}\"}}]}}]}",
            &mut content,
            &mut reasoning,
            &mut tool_calls,
            &mut usage,
            &mut noop,
        );

        let finalized = finalize_streaming_tool_calls(tool_calls).expect("tool calls");
        assert_eq!(finalized.len(), 1);
        assert_eq!(finalized[0].id, "call_1");
        assert_eq!(finalized[0].tool_name, "get_weather");
        assert_eq!(finalized[0].arguments["city"], "北京");
    }

    #[test]
    fn serializes_assistant_tool_calls_into_openai_message_shape() {
        let message = ChatMessage {
            role: "assistant".to_string(),
            content: serde_json::json!(""),
            reasoning_content: None,
            speaker: None,
            tool_call_id: None,
            tool_calls: Some(vec![ChatToolCall {
                id: "call-1".to_string(),
                tool_name: "list_scenes".to_string(),
                arguments: serde_json::json!({}),
            }]),
            metadata: None,
        };

        let serialized = build_openai_message(&message);
        assert_eq!(
            serialized
                .get("tool_calls")
                .and_then(|value| value.as_array())
                .map(Vec::len),
            Some(1)
        );
        assert!(serialized.get("content").is_some());
    }

    #[test]
    fn parses_native_tool_calls_from_openai_response_message() {
        let message = OpenAIMessageResponse {
            content: None,
            reasoning_content: None,
            tool_calls: Some(vec![OpenAIToolCallResponse {
                id: Some("call-1".to_string()),
                function: Some(OpenAIFunctionCallResponse {
                    name: Some("list_scenes".to_string()),
                    arguments: Some("{\"scene_name\":\"Dock\"}".to_string()),
                }),
            }]),
        };

        let tool_calls = extract_tool_calls(&message).expect("tool calls");
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].tool_name, "list_scenes");
        assert_eq!(tool_calls[0].arguments["scene_name"], "Dock");
    }

    #[test]
    fn omits_response_format_when_native_tools_are_included() {
        let request = ChatRequest {
            model: "test-model".to_string(),
            messages: vec![],
            temperature: Some(0.1),
            max_tokens: Some(32),
            stream: Some(false),
            json_mode: Some(true),
            response_schema: None,
            tools: Some(vec![ChatToolDefinition {
                name: "list_scenes".to_string(),
                description: Some("List scenes".to_string()),
                input_schema: serde_json::json!({ "type": "object" }),
            }]),
            tool_choice: Some(ChatToolChoice::Auto),
            native_tool_calling: Some(true),
        };

        let serialized = build_openai_request(
            &request,
            StructuredOutputMode::JsonSchema,
            ToolMode::Include,
        );

        assert!(serialized.response_format.is_none());
        assert!(serialized.tools.is_some());
        assert!(serialized.tool_choice.is_some());
    }
}
