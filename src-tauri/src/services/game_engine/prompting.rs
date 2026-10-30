use crate::models::session::SessionSnapshot;
use crate::models::world::WorldDefinition;
use chrono::Local;
use std::collections::HashMap;

fn current_time_text() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn current_date_text() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

/// 提示词变量上下文：内置变量 + 插值管理的自定义变量 + 会话 KV 变量。
/// 供所有作者可编辑的提示词输入框（运行时上下文、返回格式、系统提示、
/// 背景、旁白等）在发送前展开 `{{变量}}` 占位符。
pub struct PromptVariableContext<'a> {
    pub world: Option<&'a WorldDefinition>,
    pub session: Option<&'a SessionSnapshot>,
    /// 会话级 variables 命名空间的 KV（{{var:key}} 的数据源）。
    pub kv_vars: Option<&'a HashMap<String, String>>,
    /// 插值管理（director_config.prompt_variables）定义的世界级自定义变量。
    pub custom_vars: Option<&'a HashMap<String, String>>,
}

impl PromptVariableContext<'_> {
    pub fn empty() -> Self {
        Self {
            world: None,
            session: None,
            kv_vars: None,
            custom_vars: None,
        }
    }

    /// 解析一个变量名。无法识别时返回 None（占位符按原文保留，方便在预览里发现拼写错误）。
    pub fn resolve(&self, raw_name: &str) -> Option<String> {
        let name = raw_name.trim();
        if name.is_empty() {
            return None;
        }
        // 1) 插值管理里的自定义变量优先：作者可覆盖任何内置名字。
        if let Some(value) = self.custom_vars.and_then(|map| map.get(name)) {
            return Some(value.clone());
        }
        // 2) 内置扁平变量。
        let builtin = match name {
            "time" | "current_time" | "当前时间" => Some(current_time_text()),
            "date" | "current_date" | "当前日期" => Some(current_date_text()),
            "user" | "player" | "玩家" => self
                .session
                .map(|session| session.player_character_name.clone()),
            "world_name" | "世界名" => self.world.map(|world| world.name.clone()),
            "location" | "地点" => self.session.map(|session| session.location.clone()),
            "scene" | "scene_name" | "场景" => {
                self.session.map(|session| session.scene.name.clone())
            }
            _ => None,
        };
        if builtin.is_some() {
            return builtin;
        }
        // 3) 命名空间变量（恢复 v1 插值系统的老写法，世界包文档仍在使用）。
        if let Some(key) = name.strip_prefix("var:") {
            // 会话 KV 是运行时动态值：缺失时替换为空串，避免占位符原文泄漏进提示词。
            return Some(
                self.kv_vars
                    .and_then(|map| map.get(key.trim()))
                    .cloned()
                    .unwrap_or_default(),
            );
        }
        if let Some(field) = name.strip_prefix("world:").map(str::trim) {
            return match (field, self.world) {
                ("name", Some(world)) => Some(world.name.clone()),
                ("genre", Some(world)) => Some(world.genre.clone()),
                ("summary", Some(world)) => Some(world.summary.clone()),
                ("opening_scene", Some(world)) => Some(world.opening_scene.clone()),
                _ => None,
            };
        }
        if let Some(field) = name.strip_prefix("session:").map(str::trim) {
            return match (field, self.session) {
                ("location", Some(session)) => Some(session.location.clone()),
                ("time", Some(session)) => Some(session.time_label.clone()),
                ("scene", Some(session)) => Some(session.scene.name.clone()),
                ("player", Some(session)) => Some(session.player_character_name.clone()),
                _ => None,
            };
        }
        if let Some(options) = name.strip_prefix("random:") {
            let items: Vec<&str> = options
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .collect();
            if items.is_empty() {
                return None;
            }
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.subsec_nanos() as usize)
                .unwrap_or(0);
            return Some(items[nanos % items.len()].to_string());
        }
        None
    }
}

/// 自定义变量可以引用内置变量（值里再写 {{user}} 等），因此做多轮展开；
/// 上限防止自定义变量互相引用成环导致死循环。
const MAX_MACRO_PASSES: usize = 3;

pub fn render_prompt_variables(template: &str) -> String {
    render_prompt_variables_with_context(template, &PromptVariableContext::empty())
}

pub fn render_prompt_variables_with_context(template: &str, ctx: &PromptVariableContext) -> String {
    let mut output = template.to_string();
    for _ in 0..MAX_MACRO_PASSES {
        let next = render_single_pass(&output, ctx);
        if next == output {
            break;
        }
        output = next;
    }
    output
}

fn render_single_pass(template: &str, ctx: &PromptVariableContext) -> String {
    let mut output = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        output.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end_offset) = after.find("}}") else {
            output.push_str(&rest[start..]);
            return output;
        };
        let token_end = start + 2 + end_offset + 2;
        match ctx.resolve(&after[..end_offset]) {
            Some(value) => output.push_str(&value),
            // 未识别的占位符按原文保留：预览里看得见，比静默清空好排查。
            None => output.push_str(&rest[start..token_end]),
        }
        rest = &rest[token_end..];
    }
    output.push_str(rest);
    output
}

/// 插值管理（世界级自定义提示词变量）：`director_config.prompt_variables`。
/// 值统一转字符串；空键跳过。随世界包导入导出。
pub fn load_prompt_custom_vars(world: &WorldDefinition) -> HashMap<String, String> {
    world
        .director_config
        .get("prompt_variables")
        .and_then(|value| value.as_object())
        .map(|map| {
            map.iter()
                .filter_map(|(key, value)| {
                    let key = key.trim();
                    if key.is_empty() {
                        return None;
                    }
                    let text = match value {
                        serde_json::Value::String(text) => text.clone(),
                        serde_json::Value::Null => return None,
                        other => other.to_string(),
                    };
                    Some((key.to_string(), text))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 读取会话级 variables 命名空间的 KV，供提示词 {{var:key}} 占位符使用。
/// 值统一转字符串：字符串原样，其它 JSON 值取其序列化文本。
pub fn load_prompt_kv_vars(
    conn: &rusqlite::Connection,
    session_id: &str,
) -> Result<std::collections::HashMap<String, String>, String> {
    let entries = crate::db::repositories::world_kv_repo::WorldKvRepository::new(conn).list(
        "session",
        session_id,
        "variables",
    )?;
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

pub const DIRECTOR_PROMPT_SECTION_IDS: &[&str] = &[
    "world_background",
    "oag_context",
    "director_rules",
    "world_runtime_context",
    "scene_characters",
    "chat_history",
    "player_input",
    "response_contract",
    "tool_loop_context",
];

pub const CHARACTER_PROMPT_SECTION_IDS: &[&str] = &[
    "world_background",
    "oag_context",
    "character_background",
    "character_rules",
    "character_attributes",
    "world_runtime_context",
    "character_runtime_context",
    "visible_characters",
    "chat_history",
    "narration_prompt",
    "player_input",
    "response_contract",
    "fact_memory_rules",
];

/// Apply a world-configured order to message sections.
/// Unknown and duplicate IDs are ignored; omitted sections follow in default order.
pub fn resolve_prompt_section_order(
    config: &serde_json::Value,
    key: &str,
    defaults: &[&str],
) -> Vec<String> {
    let requested = config
        .get(key)
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .filter(|id| defaults.contains(id))
        .fold(Vec::<&str>::new(), |mut ordered, id| {
            if !ordered.contains(&id) {
                ordered.push(id);
            }
            ordered
        });
    let mut movable = requested;
    for id in defaults.iter().copied() {
        if !movable.contains(&id) {
            movable.push(id);
        }
    }
    movable.into_iter().map(str::to_string).collect()
}

pub fn resolve_runtime_context_prompt(
    world: &WorldDefinition,
    ctx: Option<&PromptVariableContext>,
) -> String {
    let empty = PromptVariableContext::empty();
    let ctx = ctx.unwrap_or(&empty);
    world
        .director_config
        .get("runtime_context_prompt")
        .and_then(|value| value.as_str())
        .map(|text| render_prompt_variables_with_context(text, ctx))
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
                .map(redact_media_for_trace)
                .map(|value| match value {
                    serde_json::Value::String(text) => text,
                    serde_json::Value::Array(parts) => parts
                        .iter()
                        .filter_map(|part| part.get("text").and_then(serde_json::Value::as_str))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    serde_json::Value::Null => String::new(),
                    other => other.to_string(),
                })
                .unwrap_or_default();
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

    #[test]
    fn prompt_plan_ignores_legacy_tool_list_and_preserves_context_order() {
        let plan = serde_json::json!({
            "director_prompt_order": ["tool_list", "world_runtime_context", "chat_history", "unknown", "chat_history", "world_background"]
        });
        let ordered = resolve_prompt_section_order(
            &plan,
            "director_prompt_order",
            &[
                "world_background",
                "director_rules",
                "world_runtime_context",
                "chat_history",
                "player_input",
            ],
        );
        assert_eq!(
            ordered,
            vec![
                "world_runtime_context",
                "chat_history",
                "world_background",
                "director_rules",
                "player_input"
            ]
        );
    }
}

#[cfg(test)]
mod variable_tests {
    use super::*;

    fn test_world(director_config: serde_json::Value) -> WorldDefinition {
        WorldDefinition {
            id: "world-vars".to_string(),
            name: "云梦泽".to_string(),
            genre: "仙侠".to_string(),
            background_prompt: String::new(),
            opening_scene: "山门".to_string(),
            summary: "一场大梦".to_string(),
            time_system: String::new(),
            map_nodes: serde_json::json!({ "version": 1, "nodes": [] }),
            time_config: serde_json::json!({}),
            director_config,
            ui_theme_config: serde_json::json!({}),
            director_system_prompt_base: String::new(),
            director_runtime_system_prompt: String::new(),
            opening_messages: Vec::new(),
            opening_character_ids: Vec::new(),
            player_character_id: None,
        }
    }

    fn test_session() -> SessionSnapshot {
        SessionSnapshot {
            id: "sess-vars".to_string(),
            world_name: "云梦泽".to_string(),
            location: "七玄门".to_string(),
            time_label: "清晨".to_string(),
            current_speaker: String::new(),
            current_line: String::new(),
            player_character_id: "char-1".to_string(),
            player_character_name: "陆谨".to_string(),
            visible_characters: Vec::new(),
            messages: Vec::new(),
            player_stats: Vec::new(),
            map_graph_nodes: Vec::new(),
            map_graph_edges: Vec::new(),
            system_log: Vec::new(),
            scene: crate::models::session::SceneRuntime {
                name: "山门问路".to_string(),
                ..Default::default()
            },
            assets: Default::default(),
            state: Default::default(),
            generation_params: Default::default(),
        }
    }

    #[test]
    fn resolves_builtins_custom_and_session_vars() {
        let world = test_world(serde_json::json!({
            "prompt_variables": { "师弟": "陆师弟" }
        }));
        let session = test_session();
        let kv = HashMap::from([("favor".to_string(), "救过韩立".to_string())]);
        let custom = load_prompt_custom_vars(&world);
        let ctx = PromptVariableContext {
            world: Some(&world),
            session: Some(&session),
            kv_vars: Some(&kv),
            custom_vars: Some(&custom),
        };

        let rendered = render_prompt_variables_with_context(
            "{{user}}在{{location}}（{{scene}}），世界：{{world_name}}；{{师弟}}欠人情：{{var:favor}}",
            &ctx,
        );
        assert!(
            rendered.contains("陆谨在七玄门（山门问路）"),
            "实际: {rendered}"
        );
        assert!(rendered.contains("世界：云梦泽"));
        assert!(rendered.contains("陆师弟欠人情：救过韩立"));
    }

    #[test]
    fn unknown_stays_literal_and_missing_var_empties() {
        let ctx = PromptVariableContext::empty();
        assert_eq!(
            render_prompt_variables_with_context("未知 {{nope}} 保留", &ctx),
            "未知 {{nope}} 保留"
        );
        // var: 是运行时动态值，缺失替换为空串而不是留下占位符。
        assert_eq!(
            render_prompt_variables_with_context("前{{var:missing}}后", &ctx),
            "前后"
        );
    }

    #[test]
    fn custom_vars_can_reference_builtins() {
        let world = test_world(serde_json::json!({
            "prompt_variables": { "intro": "{{user}}行至{{location}}" }
        }));
        let session = test_session();
        let custom = load_prompt_custom_vars(&world);
        let ctx = PromptVariableContext {
            world: Some(&world),
            session: Some(&session),
            kv_vars: None,
            custom_vars: Some(&custom),
        };
        assert_eq!(
            render_prompt_variables_with_context("话说{{intro}}。", &ctx),
            "话说陆谨行至七玄门。"
        );
    }

    #[test]
    fn custom_var_reference_cycles_terminate() {
        let custom = HashMap::from([
            ("a".to_string(), "{{b}}".to_string()),
            ("b".to_string(), "{{a}}".to_string()),
        ]);
        let ctx = PromptVariableContext {
            custom_vars: Some(&custom),
            ..PromptVariableContext::empty()
        };
        // 成环时多轮展开必须终止，不允许死循环。
        let _rendered = render_prompt_variables_with_context("{{a}}", &ctx);
    }

    #[test]
    fn random_picks_one_of_the_options() {
        let ctx = PromptVariableContext::empty();
        let rendered = render_prompt_variables_with_context("{{random:甲,乙,丙}}", &ctx);
        assert!(
            rendered == "甲" || rendered == "乙" || rendered == "丙",
            "实际: {rendered}"
        );
    }

    #[test]
    fn load_prompt_custom_vars_normalizes_values() {
        let world = test_world(serde_json::json!({
            "prompt_variables": { "num": 3, "flag": true, "text": "文本", "": "空键", "null": null }
        }));
        let vars = load_prompt_custom_vars(&world);
        assert_eq!(vars.get("num").map(String::as_str), Some("3"));
        assert_eq!(vars.get("flag").map(String::as_str), Some("true"));
        assert_eq!(vars.get("text").map(String::as_str), Some("文本"));
        assert!(!vars.contains_key(""));
        assert!(!vars.contains_key("null"));
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
        assert!(
            !serialized.contains("QUJDREVGRw"),
            "trace 里不应再出现 base64: {serialized}"
        );
        assert!(serialized.contains("payload"));
        assert!(serialized.contains("图片 base64 已省略"));
        assert!(serialized.contains("音频 wav base64 已省略"));

        let call = build_prompt_call(
            "test",
            "character",
            "Alice",
            "character_response",
            "test",
            "",
            "",
            vec![value.clone()],
            vec![],
            serde_json::Value::Null,
            serde_json::Value::Null,
        );
        let summary = call["final_sent_content"].as_str().expect("trace summary");
        assert!(summary.starts_with("[user] payload\n"));
        assert!(summary.contains("图片 base64 已省略"));
        assert!(summary.contains("音频 wav base64 已省略"));
        assert!(!summary.contains("QUJDREVGRw"));
        assert_eq!(call["messages"][0], value);
        // The summary also redacts media if a caller supplies raw multipart content.
        let raw_call = build_prompt_call(
            "test",
            "character",
            "Alice",
            "character_response",
            "test",
            "",
            "",
            vec![serde_json::json!({"role": "user", "content": message.content})],
            vec![],
            serde_json::Value::Null,
            serde_json::Value::Null,
        );
        assert_eq!(raw_call["final_sent_content"], call["final_sent_content"]);

        // 纯文本消息不受影响
        let plain = crate::services::llm::client::ChatMessage {
            content: serde_json::Value::String("纯文本".to_string()),
            ..message
        };
        assert_eq!(
            llm_chat_message_to_value(&plain)
                .get("content")
                .and_then(|v| v.as_str()),
            Some("纯文本")
        );
    }
}
