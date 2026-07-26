use chrono::Local;
use std::collections::HashMap;

use crate::models::world::WorldDefinition;

pub fn render_prompt_variables(template: &str) -> String {
    let current_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    template
        .replace("{{current_time}}", &current_time)
        .replace("{{当前时间}}", &current_time)
}

/// 读取会话级 variables 命名空间的 KV，供提示词 {{var:key}} 占位符使用。
/// 值统一转字符串：字符串原样，其它 JSON 值取其序列化文本。
pub fn load_prompt_kv_vars(
    conn: &rusqlite::Connection,
    session_id: &str,
) -> Result<std::collections::HashMap<String, String>, String> {
    let entries = crate::db::repositories::world_kv_repo::WorldKvRepository::new(conn)
        .list("session", session_id, "variables")?;
    Ok(entries
        .into_iter()
        .map(|entry| {
            let value = match &entry.value {
                serde_json::Value::String(text) => text.clone(),
                other => other.to_string(),
            };
            (entry.key, value)
        })
        .collect())
}

/// 关键词触发用的近期消息文本（最近 depth 条，拼接内容）。
pub fn recent_messages_text(
    messages: &[crate::models::session::ChatMessage],
    depth: usize,
) -> String {
    messages
        .iter()
        .rev()
        .take(depth)
        .rev()
        .map(|message| message.content.as_str().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

// ---- 提示词模块（第 6 项）----
// 模块在 prompt_presets 中声明，向后兼容旧字段（enabled/scope/order/content），新增：
//   name      展示名（调试留痕用）
//   position  system_prefix（系统前缀）| system_suffix（系统追加，默认）| depth:N（历史第 N 层）
//   keywords  可选关键词列表：命中近期消息才注入（无则总是注入）
//   keyword_scan_depth  关键词扫描的近期消息条数（默认 10）
// 核心响应契约不受模块影响：模块只能追加消息，不能修改已有内容。

/// 单个模块渲染后内容上限（字符）。
pub const PROMPT_MODULE_MAX_CONTENT: usize = 4000;
const MAX_MACRO_PASSES: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptPosition {
    SystemPrefix,
    SystemSuffix,
    Depth(usize),
}

/// 一个模块的解析结果：注入位置 + 渲染后内容 + 留痕原因。
#[derive(Debug, Clone)]
pub struct ResolvedPromptModule {
    pub name: String,
    pub position: PromptPosition,
    pub order: i64,
    pub content: String,
    pub injected: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Default)]
pub struct PromptModuleResolution {
    pub prefix: Vec<String>,
    pub suffix: Vec<String>,
    pub depth_insertions: Vec<(usize, String)>,
    pub traces: Vec<serde_json::Value>,
}

fn parse_prompt_position(raw: Option<&serde_json::Value>) -> PromptPosition {
    let Some(value) = raw.and_then(|value| value.as_str()) else {
        return PromptPosition::SystemSuffix;
    };
    let value = value.trim();
    match value {
        "system_prefix" => PromptPosition::SystemPrefix,
        "system_suffix" | "" => PromptPosition::SystemSuffix,
        _ => {
            if let Some(depth) = value.strip_prefix("depth:") {
                if let Ok(depth) = depth.trim().parse::<usize>() {
                    return PromptPosition::Depth(depth.min(20));
                }
            }
            PromptPosition::SystemSuffix
        }
    }
}

fn module_keywords(object: &serde_json::Map<String, serde_json::Value>) -> Vec<String> {
    object
        .get("keywords")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::trim))
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// 解析世界包的提示词模块：过滤、关键词判定、渲染、按位置分流，并产生留痕。
/// `kv_vars` 为会话级 variables 命名空间的 KV（{{var:key}} 的数据源）；
/// `recent_text` 为近期消息拼接文本（关键词命中判定用）。
pub fn resolve_prompt_modules(
    world: &WorldDefinition,
    target: &str,
    variables: &HashMap<String, String>,
    kv_vars: &HashMap<String, String>,
    recent_text: &str,
) -> PromptModuleResolution {
    let presets = world
        .director_config
        .get("prompt_presets")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    let mut resolved: Vec<ResolvedPromptModule> = Vec::new();
    for item in &presets {
        let Some(object) = item.as_object() else {
            continue;
        };
        let name = object
            .get("name")
            .and_then(|value| value.as_str())
            .unwrap_or("unnamed")
            .trim()
            .to_string();
        let enabled = object
            .get("enabled")
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        let scope = object
            .get("scope")
            .and_then(|value| value.as_str())
            .unwrap_or("both")
            .trim()
            .to_string();
        if !enabled || !(scope == "both" || scope == target) {
            continue;
        }
        let order = object
            .get("order")
            .and_then(|value| value.as_i64())
            .unwrap_or(0);
        let position = parse_prompt_position(object.get("position"));

        // 关键词触发：声明了 keywords 时，仅当近期消息命中才注入。
        let keywords = module_keywords(object);
        let (keyword_hit, reason) = if keywords.is_empty() {
            (true, "总是注入".to_string())
        } else {
            let hit = keywords
                .iter()
                .find(|keyword| recent_text.contains(keyword.as_str()));
            match hit {
                Some(keyword) => (true, format!("命中关键词: {keyword}")),
                None => (false, format!("关键词未命中（{} 个关键词）", keywords.len())),
            }
        };

        let raw = object
            .get("content")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let content = if keyword_hit {
            render_prompt_macros(raw, world, None, variables, kv_vars)
        } else {
            String::new()
        };
        let injected = keyword_hit && !content.trim().is_empty();
        let reason = if !keyword_hit {
            reason
        } else if content.trim().is_empty() {
            format!("{reason}，但渲染后为空")
        } else {
            reason
        };
        resolved.push(ResolvedPromptModule {
            name,
            position,
            order,
            content: content.trim().to_string(),
            injected,
            reason,
        });
    }
    resolved.sort_by_key(|module| module.order);

    let mut resolution = PromptModuleResolution::default();
    for module in resolved {
        resolution.traces.push(serde_json::json!({
            "name": format!("Prompt preset {}", module.name),
            "source": "World design / prompt preset",
            "position": match &module.position {
                PromptPosition::SystemPrefix => "system_prefix".to_string(),
                PromptPosition::SystemSuffix => "system_suffix".to_string(),
                PromptPosition::Depth(depth) => format!("depth:{depth}"),
            },
            "content": module.content,
            "editable": true,
            "sent": module.injected,
            "reason": module.reason,
        }));
        if !module.injected {
            continue;
        }
        match module.position {
            PromptPosition::SystemPrefix => resolution.prefix.push(module.content),
            PromptPosition::SystemSuffix => resolution.suffix.push(module.content),
            PromptPosition::Depth(depth) => {
                resolution.depth_insertions.push((depth, module.content))
            }
        }
    }
    resolution
}

// ---- 占位符（宏）----
// 纯文本替换，不执行代码。白名单：
//   {{var:key}}            会话级 variables KV（第 3 项）
//   {{world:name|genre|summary|opening_scene}}
//   {{session:location|time}}
//   {{date}} / {{current_time}} / {{当前时间}}
//   {{random:a,b,c}}
//   {{key}}                调用方变量（既有行为）
// 未知占位符原样保留（作者能直接看到笔误），最多展开 3 轮，单模块 4000 字符上限。

pub fn render_prompt_macros(
    template: &str,
    world: &WorldDefinition,
    session: Option<&crate::models::session::SessionSnapshot>,
    variables: &HashMap<String, String>,
    kv_vars: &HashMap<String, String>,
) -> String {
    let mut content = template.to_string();
    for _ in 0..MAX_MACRO_PASSES {
        let before = content.clone();
        content = render_macro_pass(&content, world, session, variables, kv_vars);
        if content == before {
            break;
        }
    }
    if content.len() > PROMPT_MODULE_MAX_CONTENT {
        content.truncate(PROMPT_MODULE_MAX_CONTENT);
    }
    content
}

fn render_macro_pass(
    input: &str,
    world: &WorldDefinition,
    session: Option<&crate::models::session::SessionSnapshot>,
    variables: &HashMap<String, String>,
    kv_vars: &HashMap<String, String>,
) -> String {
    let mut output = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("{{") {
        let Some(end_offset) = rest[start + 2..].find("}}") else {
            break;
        };
        let end = start + 2 + end_offset + 2;
        output.push_str(&rest[..start]);
        let token = &rest[start..end];
        let replacement = token
            .strip_prefix("{{")
            .and_then(|inner| inner.strip_suffix("}}"))
            .map(str::trim)
            .and_then(|name| resolve_macro(name, world, session, variables, kv_vars));
        match replacement {
            Some(value) => output.push_str(&value),
            None => output.push_str(token),
        }
        rest = &rest[end..];
    }
    output.push_str(rest);
    output
}

fn resolve_macro(
    name: &str,
    world: &WorldDefinition,
    session: Option<&crate::models::session::SessionSnapshot>,
    variables: &HashMap<String, String>,
    kv_vars: &HashMap<String, String>,
) -> Option<String> {
    if let Some(key) = name.strip_prefix("var:") {
        return kv_vars.get(key.trim()).cloned().or(Some(String::new()));
    }
    if let Some(field) = name.strip_prefix("world:") {
        return match field.trim() {
            "name" => Some(world.name.clone()),
            "genre" => Some(world.genre.clone()),
            "summary" => Some(world.summary.clone()),
            "opening_scene" => Some(world.opening_scene.clone()),
            _ => None,
        };
    }
    if let Some(field) = name.strip_prefix("session:") {
        return match (field.trim(), session) {
            ("location", Some(session)) => Some(session.location.clone()),
            ("time", Some(session)) => Some(session.time_label.clone()),
            _ => None,
        };
    }
    if let Some(options) = name.strip_prefix("random:") {
        let options: Vec<&str> = options
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .collect();
        if options.is_empty() {
            return None;
        }
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.subsec_nanos() as usize)
            .unwrap_or(0);
        return Some(options[nanos % options.len()].to_string());
    }
    if name == "date" {
        return Some(chrono::Local::now().format("%Y-%m-%d").to_string());
    }
    if name == "current_time" || name == "当前时间" {
        return Some(
            chrono::Local::now()
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
        );
    }
    // 既有 {{key}} 变量
    variables.get(name).cloned()
}

pub fn resolve_runtime_context_prompt(world: &WorldDefinition) -> String {
    world
        .director_config
        .get("runtime_context_prompt")
        .and_then(|value| value.as_str())
        .map(render_prompt_variables)
        .unwrap_or_default()
        .trim()
        .to_string()
}

pub fn llm_chat_message_to_value(
    message: &crate::services::llm::client::ChatMessage,
) -> serde_json::Value {
    serde_json::json!({
        "role": message.role,
        // 第 10 项：multipart 消息进 trace 前脱敏——媒体 base64 只留摘要，
        // 避免 turn_journal 膨胀、调试面板渲染几十 MB 的 data URL。
        "content": redact_media_for_trace(&message.content),
        "reasoning_content": message.reasoning_content,
        "speaker": message.speaker,
        "metadata": message.metadata,
    })
}

/// 序列化完整请求进 trace（request_value）时的入口：messages[].content 里的
/// 媒体 base64 同样替换为摘要。
pub fn redact_request_value_for_trace(
    request: &crate::services::llm::client::ChatRequest,
) -> serde_json::Value {
    let mut value = serde_json::to_value(request).unwrap_or_default();
    if let Some(messages) = value.get_mut("messages").and_then(|v| v.as_array_mut()) {
        for message in messages.iter_mut() {
            let Some(content) = message.get("content").cloned() else {
                continue;
            };
            if let Some(object) = message.as_object_mut() {
                object.insert("content".to_string(), redact_media_for_trace(&content));
            }
        }
    }
    value
}

/// 把 multipart content 里的媒体 part 替换为 `[模态 base64 已省略, N 字节]` 文本摘要；
/// 纯文本或非数组 content 原样返回。
fn redact_media_for_trace(content: &serde_json::Value) -> serde_json::Value {
    let serde_json::Value::Array(parts) = content else {
        return content.clone();
    };
    let redacted = parts
        .iter()
        .map(|part| {
            let part_type = part.get("type").and_then(|v| v.as_str()).unwrap_or("");
            match part_type {
                "image_url" => {
                    let bytes = part
                        .pointer("/image_url/url")
                        .and_then(|v| v.as_str())
                        .map(str::len)
                        .unwrap_or(0);
                    serde_json::json!({
                        "type": "text",
                        "text": format!("[图片 base64 已省略, {bytes} 字节]"),
                    })
                }
                "input_audio" => {
                    let bytes = part
                        .pointer("/input_audio/data")
                        .and_then(|v| v.as_str())
                        .map(str::len)
                        .unwrap_or(0);
                    let format = part
                        .pointer("/input_audio/format")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    serde_json::json!({
                        "type": "text",
                        "text": format!("[音频 {format} base64 已省略, {bytes} 字节]"),
                    })
                }
                _ => part.clone(),
            }
        })
        .collect::<Vec<_>>();
    serde_json::Value::Array(redacted)
}

pub fn llm_chat_messages_to_values(
    messages: &[crate::services::llm::client::ChatMessage],
) -> Vec<serde_json::Value> {
    messages.iter().map(llm_chat_message_to_value).collect()
}

pub fn build_prompt_call(
    schema_version: &str,
    recipient_type: &str,
    recipient_name: &str,
    stage: &str,
    purpose: &str,
    system_prompt: &str,
    user_prompt: &str,
    messages: Vec<serde_json::Value>,
    modules: Vec<serde_json::Value>,
    response_contract: serde_json::Value,
    raw_debug: serde_json::Value,
) -> serde_json::Value {
    let final_sent_content = messages
        .iter()
        .map(|item| {
            let role = item
                .get("role")
                .and_then(|value| value.as_str())
                .unwrap_or("user");
            let content = item
                .get("content")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            format!("[{role}] {content}")
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    serde_json::json!({
        "schema_version": schema_version,
        "recipient_type": recipient_type,
        "recipient_name": recipient_name,
        "stage": stage,
        "purpose": purpose,
        "system_prompt": system_prompt,
        "user_prompt": user_prompt,
        "response_contract": response_contract,
        "modules": modules,
        "messages": messages,
        "final_sent_content": final_sent_content,
        "raw_model_return": serde_json::Value::Null,
        "return_processing": serde_json::Value::Null,
        "processed_model_return": serde_json::Value::Null,
        "written_result": serde_json::Value::Null,
        "raw_debug": raw_debug,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world_with_presets(presets: serde_json::Value) -> WorldDefinition {
        WorldDefinition {
            id: "w".to_string(),
            name: "诗会".to_string(),
            genre: String::new(),
            background_prompt: String::new(),
            opening_scene: String::new(),
            summary: String::new(),
            time_system: String::new(),
            map_nodes: serde_json::json!({}),
            triggers: vec![],
            time_config: serde_json::json!({}),
            director_config: serde_json::json!({ "prompt_presets": presets }),
            ui_theme_config: serde_json::json!({}),
            director_system_prompt_base: String::new(),
            director_runtime_system_prompt: String::new(),
            opening_messages: vec![],
            opening_character_ids: vec![],
            player_character_id: None,
        }
    }

    #[test]
    fn collects_scope_enabled_ordered_and_rendered_presets() {
        let world = world_with_presets(serde_json::json!([
            { "content": "B second", "scope": "director", "enabled": true, "order": 2 },
            { "content": "A first for {{char}}", "scope": "both", "enabled": true, "order": 1 },
            { "content": "char only", "scope": "character", "enabled": true, "order": 0 },
            { "content": "disabled", "scope": "director", "enabled": false, "order": 0 },
            { "content": "   ", "scope": "director", "enabled": true, "order": 5 },
        ]));
        let mut vars = HashMap::new();
        vars.insert("char".to_string(), "李白".to_string());
        let kv = HashMap::new();

        let director = resolve_prompt_modules(&world, "director", &vars, &kv, "");
        assert_eq!(
            director.suffix,
            vec!["A first for 李白".to_string(), "B second".to_string()]
        );

        let character = resolve_prompt_modules(&world, "character", &vars, &kv, "");
        assert_eq!(
            character.suffix,
            vec!["char only".to_string(), "A first for 李白".to_string()]
        );
    }

    #[test]
    fn empty_when_no_presets() {
        let world = world_with_presets(serde_json::json!([]));
        let resolution = resolve_prompt_modules(&world, "director", &HashMap::new(), &HashMap::new(), "");
        assert!(resolution.suffix.is_empty());
        assert!(resolution.prefix.is_empty());
        assert!(resolution.traces.is_empty());
    }
}

#[cfg(test)]
mod media_trace_tests {
    use super::*;

    #[test]
    fn trace_redacts_media_base64_but_keeps_text() {
        let message = crate::services::llm::client::ChatMessage {
            role: "user".to_string(),
            content: serde_json::json!([
                { "type": "text", "text": "payload" },
                { "type": "image_url", "image_url": { "url": "data:image/png;base64,QUJDREVGRw==" } },
                { "type": "input_audio", "input_audio": { "data": "QUJDREVGRw==", "format": "wav" } }
            ]),
            reasoning_content: None,
            speaker: None,
            tool_call_id: None,
            tool_calls: None,
            metadata: None,
        };
        let value = llm_chat_message_to_value(&message);
        let serialized = value.to_string();
        assert!(!serialized.contains("QUJDREVGRw"), "trace 里不应再出现 base64: {serialized}");
        assert!(serialized.contains("payload"));
        assert!(serialized.contains("图片 base64 已省略"));
        assert!(serialized.contains("音频 wav base64 已省略"));

        // 纯文本消息不受影响
        let plain = crate::services::llm::client::ChatMessage {
            content: serde_json::Value::String("纯文本".to_string()),
            ..message
        };
        assert_eq!(
            llm_chat_message_to_value(&plain).get("content").and_then(|v| v.as_str()),
            Some("纯文本")
        );
    }
}
