use crate::models::mcp_tool::{is_builtin_mcp_tool_id, McpToolDefinition, MCP_TOOL_SCHEDULE_NOTIFICATION_ID};
use crate::models::session::{InventoryItem, SessionSnapshot};
use crate::models::world::WorldDefinition;
use crate::services::llm::client::{ChatRequest, ChatToolDefinition};
use crate::services::notifications::notification_tool_definition;
use std::collections::BTreeSet;

pub(crate) fn build_director_inventory_records(items: &[InventoryItem]) -> Vec<serde_json::Value> {
    // 对齐角色侧瘦身：删内部 UUID(item_id/owner_id)；保留主控决策需要的
    // name/category/quantity/owner_type/visibility；description/tags/disclosed_to 空值不发。
    items
        .iter()
        .map(|item| {
            let mut record = serde_json::Map::new();
            record.insert("name".to_string(), serde_json::json!(item.name));
            record.insert("category".to_string(), serde_json::json!(item.category));
            record.insert("quantity".to_string(), serde_json::json!(item.quantity));
            record.insert("owner_type".to_string(), serde_json::json!(item.owner_type));
            record.insert("visibility".to_string(), serde_json::json!(item.visibility));
            if !item.description.trim().is_empty() {
                record.insert("description".to_string(), serde_json::json!(item.description));
            }
            if !item.tags.is_empty() {
                record.insert("tags".to_string(), serde_json::json!(item.tags));
            }
            if !item.disclosed_to.is_empty() {
                record.insert("disclosed_to".to_string(), serde_json::json!(item.disclosed_to));
            }
            serde_json::Value::Object(record)
        })
        .collect()
}

pub(crate) fn extract_first_balanced_json_segment(raw: &str) -> Option<String> {
    let start_index = raw
        .char_indices()
        .find(|(_, ch)| *ch == '{' || *ch == '[')
        .map(|(index, _)| index)?;
    let chars = raw[start_index..].char_indices();
    let mut stack = Vec::new();
    let mut in_string = false;
    let mut escaped = false;
    for (offset, ch) in chars {
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            match ch {
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' | '[' => stack.push(ch),
            '}' => {
                if stack.pop() != Some('{') {
                    return None;
                }
                if stack.is_empty() {
                    return Some(raw[start_index..=start_index + offset].to_string());
                }
            }
            ']' => {
                if stack.pop() != Some('[') {
                    return None;
                }
                if stack.is_empty() {
                    return Some(raw[start_index..=start_index + offset].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

/// 导演最终输出无法解析为 JSON 对象时,携带解析错误让模型重出的最大修复轮次。
pub(crate) const DIRECTOR_JSON_REPAIR_ATTEMPTS: usize = 2;

/// 输出被 max_tokens 截断时,重试放大 token 预算的最大次数与倍率。
///
/// 推理模型(deepseek-v4-flash / o 系列等)会先把预算烧在思考上,预算不足时
/// content 为空、finish_reason = "length"。此时追加"你上次输出不是 JSON"的
/// 修复消息只会让 prompt 更长、再次截断——必须放大预算重试。
pub(crate) const DIRECTOR_TOKEN_LIMIT_RETRIES: usize = 2;
pub(crate) const DIRECTOR_TOKEN_LIMIT_GROWTH: i32 = 3;
/// 放大后的预算上限,避免世界包配了极大值时把单轮成本推到失控。
pub(crate) const DIRECTOR_TOKEN_LIMIT_CEILING: i32 = 32_000;
/// 未显式配置 max_tokens 时,按此值作为放大的起点。
pub(crate) const DIRECTOR_TOKEN_LIMIT_FALLBACK: i32 = 4_000;

/// 把请求的 max_tokens 放大一档,返回 None 表示已到上限、无需再试。
pub(crate) fn grow_token_budget(request: &ChatRequest) -> Option<ChatRequest> {
    let current = request
        .generation
        .max_tokens
        .filter(|value| *value > 0)
        .unwrap_or(DIRECTOR_TOKEN_LIMIT_FALLBACK);
    if current >= DIRECTOR_TOKEN_LIMIT_CEILING {
        return None;
    }
    let grown = current
        .saturating_mul(DIRECTOR_TOKEN_LIMIT_GROWTH)
        .min(DIRECTOR_TOKEN_LIMIT_CEILING);
    if grown <= current {
        return None;
    }
    let mut grown_request = request.clone();
    grown_request.generation.max_tokens = Some(grown);
    Some(grown_request)
}

/// 导演工具循环轮次上限(director_tool_loop_limit)的缺省值与允许范围。
pub(crate) const DIRECTOR_TOOL_LOOP_LIMIT_DEFAULT: usize = 4;
pub(crate) const DIRECTOR_TOOL_LOOP_LIMIT_MIN: i64 = 1;
pub(crate) const DIRECTOR_TOOL_LOOP_LIMIT_MAX: i64 = 12;

/// 单轮导演输出中允许处理的工具调用条数(director_tool_call_limit)的缺省值与允许范围。
pub(crate) const DIRECTOR_TOOL_CALL_LIMIT_DEFAULT: usize = 8;
pub(crate) const DIRECTOR_TOOL_CALL_LIMIT_MIN: i64 = 1;
pub(crate) const DIRECTOR_TOOL_CALL_LIMIT_MAX: i64 = 8;

/// 导演最终输出是否需要 LLM 修复重试:解析结果不是非空 JSON 对象。
pub(crate) fn director_output_needs_json_repair(parsed: &serde_json::Value) -> bool {
    parsed
        .as_object()
        .map(|object| object.is_empty())
        .unwrap_or(true)
}

/// 修复反馈消息(中文):告知解析错误,要求模型只输出修正后的完整 JSON。
pub(crate) fn build_director_json_repair_feedback(parse_error: &str) -> String {
    format!(
        "你上一次的输出无法解析为 JSON 对象(错误:{parse_error})。请重新输出完整、合法的 JSON 对象,只输出 JSON 本身,不要包含解释文字或 Markdown 代码围栏。"
    )
}

/// 构造 JSON 修复请求:在原对话后追加"模型的坏输出 + 解析错误反馈",让模型重出。
/// 请求其它部分(model/generation/json_mode/tools 等)保持不变,仍走原有请求构造与流式逻辑。
pub(crate) fn build_director_json_repair_request(
    previous_request: &ChatRequest,
    previous_output: &str,
) -> ChatRequest {
    let parse_error = match serde_json::from_str::<serde_json::Value>(previous_output.trim()) {
        Ok(_) => "输出不是合法的 JSON 对象".to_string(),
        Err(error) => error.to_string(),
    };
    let mut request = previous_request.clone();
    request.messages.push(crate::services::llm::client::ChatMessage {
        role: "assistant".to_string(),
        content: serde_json::Value::String(previous_output.to_string()),
        reasoning_content: None,
        speaker: None,
        tool_call_id: None,
        tool_calls: None,
        metadata: Some(serde_json::json!({ "json_repair": true })),
    });
    request.messages.push(crate::services::llm::client::ChatMessage {
        role: "user".to_string(),
        content: serde_json::Value::String(build_director_json_repair_feedback(&parse_error)),
        reasoning_content: None,
        speaker: None,
        tool_call_id: None,
        tool_calls: None,
        metadata: Some(serde_json::json!({ "json_repair": true })),
    });
    request
}

pub(crate) fn repair_common_json_issues(raw: &str) -> String {
    let replaced = raw
        .replace('\u{201c}', "\"")
        .replace('\u{201d}', "\"")
        .replace('\u{2018}', "'")
        .replace('\u{2019}', "'")
        .replace(",}", "}")
        .replace(",]", "]");
    let sanitized = sanitize_json_control_chars(&replaced);
    complete_truncated_json(&sanitized)
}

/// 剔除/转义 JSON 里的非法控制字符。字符串值内部的裸 \r 直接去掉、裸换行和
/// 裸制表符转成 \\n / \\t 转义(保留原有语义),其余 C0 控制字符移除;字符串外
/// 只移除非法控制符,合法空白(空格/\t/\r/\n)原样保留。
pub(crate) fn sanitize_json_control_chars(raw: &str) -> String {
    let mut output = String::with_capacity(raw.len());
    let mut in_string = false;
    let mut escaped = false;
    for ch in raw.chars() {
        if in_string {
            if escaped {
                output.push(ch);
                escaped = false;
                continue;
            }
            match ch {
                '\\' => {
                    output.push(ch);
                    escaped = true;
                }
                '"' => {
                    output.push(ch);
                    in_string = false;
                }
                '\r' => {}
                '\n' => output.push_str("\\n"),
                '\t' => output.push_str("\\t"),
                ch if (ch as u32) < 0x20 => {}
                _ => output.push(ch),
            }
            continue;
        }
        match ch {
            '"' => {
                output.push(ch);
                in_string = true;
            }
            ch if (ch as u32) < 0x20 && !matches!(ch, '\t' | '\n' | '\r') => {}
            _ => output.push(ch),
        }
    }
    output
}

/// 截断 JSON 补全:文本以 { 或 [ 开头但括号未闭合时(常见于输出被 max_tokens
/// 截断),先补上未闭合的字符串引号,再按逆序补 ] / }。已平衡的文本原样返回。
pub(crate) fn complete_truncated_json(raw: &str) -> String {
    let trimmed_start = raw.trim_start();
    if !trimmed_start.starts_with('{') && !trimmed_start.starts_with('[') {
        return raw.to_string();
    }
    let mut stack: Vec<char> = Vec::new();
    let mut in_string = false;
    let mut escaped = false;
    for ch in raw.chars() {
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            match ch {
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' | '[' => stack.push(ch),
            '}' | ']' => {
                stack.pop();
            }
            _ => {}
        }
    }
    if stack.is_empty() && !in_string {
        return raw.to_string();
    }
    let mut completed = raw.to_string();
    if in_string {
        // 末尾悬空的转义反斜杠先自成一对,否则补上的引号会被它转义掉。
        if escaped {
            completed.push('\\');
        }
        completed.push('"');
    }
    while let Some(opener) = stack.pop() {
        completed.push(if opener == '{' { '}' } else { ']' });
    }
    completed
}

pub(crate) fn arg_string(arguments: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(crate) fn normalize_llm_text(value: Option<&serde_json::Value>) -> Option<String> {
    let value = value?;
    let normalized = match value {
        serde_json::Value::String(item) => item.trim().to_string(),
        _ => value.to_string().trim().trim_matches('"').to_string(),
    };
    if normalized.is_empty() {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "none" | "null" | "undefined" => None,
        _ => Some(normalized),
    }
}

pub(crate) fn looks_like_director_authored_speech(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.contains('"')
        || trimmed.contains('\u{201c}')
        || trimmed.contains('\u{201d}')
        || trimmed.contains('\u{2018}')
        || trimmed.contains('\u{2019}')
    {
        return true;
    }
    if trimmed.contains('?') || trimmed.contains(": ") {
        return true;
    }
    [
        "said", "says", "asked", "answered", "replied", "spoke", "opened", "blurted",
        "鎺ヤ护", "鍑哄彞", "绛旀洶", "鍚熷嚭", "蹇靛嚭",
    ]
    .iter()
    .any(|marker| trimmed.contains(marker))
}

pub(crate) fn parse_string_list(value: Option<&serde_json::Value>) -> Vec<String> {
    let Some(items) = value.and_then(|value| value.as_array()) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| normalize_llm_text(Some(item)))
        .fold(Vec::<String>::new(), |mut acc, item| {
            if !acc.contains(&item) {
                acc.push(item);
            }
            acc
        })
}

pub(crate) fn parse_scene_visible_characters(
    value: Option<&serde_json::Value>,
    player_character_name: &str,
) -> Option<Vec<String>> {
    let Some(raw) = value else {
        return None;
    };
    let Some(_) = raw.as_array() else {
        return None;
    };
    Some(
        parse_string_list(Some(raw))
            .into_iter()
            .filter(|name| name != player_character_name)
            .collect(),
    )
}

pub(crate) fn parse_next_scene_tags(
    value: Option<&serde_json::Value>,
    fallback: &[String],
    next_scene_name: &str,
    current_scene_name: &str,
) -> Vec<String> {
    let parsed = parse_string_list(value);
    if !parsed.is_empty() {
        return parsed;
    }
    if next_scene_name == current_scene_name {
        return fallback.iter().filter(|item| !item.trim().is_empty()).fold(
            Vec::<String>::new(),
            |mut acc, item| {
                if !acc.contains(item) {
                    acc.push(item.clone());
                }
                acc
            },
        );
    }
    Vec::new()
}

pub(crate) fn parse_next_time_label(
    value: Option<&serde_json::Value>,
    session: &SessionSnapshot,
    world: &WorldDefinition,
    fallback: &str,
) -> String {
    let Some(candidate) = normalize_llm_text(value) else {
        return fallback.to_string();
    };
    let time_config = world.time_config.as_object();
    let mode = time_config
        .and_then(|config| config.get("mode"))
        .and_then(|value| value.as_str())
        .unwrap_or("labels");
    if mode == "24h" {
        if parse_clock_minutes(&candidate).is_some() {
            return candidate;
        }
        return fallback.to_string();
    }
    let labels = time_config
        .and_then(|config| config.get("labels"))
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| normalize_llm_text(Some(item)))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if labels.is_empty() {
        return candidate;
    }
    if candidate == session.time_label || labels.iter().any(|item| item == &candidate) {
        candidate
    } else {
        fallback.to_string()
    }
}

pub(crate) fn parse_clock_minutes(value: &str) -> Option<i32> {
    let (hour, minute) = value.split_once(':')?;
    let hour = hour.parse::<i32>().ok()?;
    let minute = minute.parse::<i32>().ok()?;
    if !(0..=23).contains(&hour) || !(0..=59).contains(&minute) {
        return None;
    }
    Some(hour * 60 + minute)
}

pub(crate) fn parse_character_visual_directives(value: Option<&serde_json::Value>) -> Vec<serde_json::Value> {
    let Some(items) = value.and_then(|value| value.as_array()) else {
        return Vec::new();
    };
    let mut parsed = Vec::new();
    let mut seen = BTreeSet::new();
    for item in items {
        let Some(object) = item.as_object() else {
            continue;
        };
        let Some(character_name) = normalize_llm_text(object.get("character_name")) else {
            continue;
        };
        if !seen.insert(character_name.clone()) {
            continue;
        }
        let portrait_hint = normalize_llm_text(object.get("portrait_hint"));
        let portrait_asset_name = normalize_llm_text(object.get("portrait_asset_name"));
        let portrait_asset_path = normalize_llm_text(object.get("portrait_asset_path"));
        let generation_prompt = normalize_llm_text(object.get("generation_prompt"));
        if portrait_hint.is_none()
            && portrait_asset_name.is_none()
            && portrait_asset_path.is_none()
            && generation_prompt.is_none()
        {
            continue;
        }
        parsed.push(serde_json::json!({
            "character_name": character_name,
            "portrait_hint": portrait_hint.unwrap_or_default(),
            "portrait_asset_name": portrait_asset_name,
            "portrait_asset_path": portrait_asset_path,
            "generation_prompt": generation_prompt,
        }));
    }
    parsed
}

pub(crate) fn parse_switch_character_proposal(
    raw: Option<&serde_json::Value>,
    player_character_name: &str,
) -> Option<serde_json::Value> {
    let raw = raw?;
    if let Some(target_name) = raw.as_str().map(|value| value.trim().to_string()) {
        if target_name.is_empty() || target_name == player_character_name {
            return None;
        }
        return Some(serde_json::json!({
            "target_character_name": target_name.clone(),
            "reason": target_name,
        }));
    }
    let object = raw.as_object()?;
    let target_name = normalize_llm_text(object.get("target_character_name"))?;
    if target_name == player_character_name {
        return None;
    }
    let reason = normalize_llm_text(object.get("reason")).unwrap_or_else(|| target_name.clone());
    let next_location = normalize_llm_text(object.get("next_location"));
    let scene_name = normalize_llm_text(object.get("scene_name"));
    let scene_background_hint = normalize_llm_text(object.get("scene_background_hint"));
    let scene_tags = parse_string_list(object.get("scene_tags"));
    let scene_character_roster = parse_string_list(object.get("scene_character_roster"))
        .into_iter()
        .filter(|name| name != player_character_name && name != &target_name)
        .collect::<Vec<_>>();
    Some(serde_json::json!({
        "target_character_name": target_name,
        "reason": reason,
        "location": next_location.clone(),
        "next_location": next_location,
        "scene_name": scene_name,
        "scene_background_hint": scene_background_hint,
        "scene_tags": scene_tags,
        "scene_character_roster": scene_character_roster,
    }))
}

pub(crate) fn collect_generated_character_items(parsed: &serde_json::Value) -> Vec<serde_json::Value> {
    let mut items = Vec::new();
    if let Some(top) = parsed
        .get("generated_characters")
        .and_then(|value| value.as_array())
    {
        items.extend(top.iter().cloned());
    }
    if let Some(nested) = parsed
        .get("switch_character_proposal")
        .and_then(|value| value.as_object())
        .and_then(|proposal| proposal.get("generated_characters"))
        .and_then(|value| value.as_array())
    {
        items.extend(nested.iter().cloned());
    }
    items
}

pub(crate) fn normalize_generated_character_items(
    items: Vec<serde_json::Value>,
    session: &SessionSnapshot,
) -> Vec<serde_json::Value> {
    let mut normalized = Vec::new();
    let mut seen = BTreeSet::new();
    let existing_visible = session
        .visible_characters
        .iter()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect::<BTreeSet<_>>();

    for item in items {
        let Some(object) = item.as_object() else {
            continue;
        };
        let Some(name) = normalize_llm_text(object.get("name"))
            .or_else(|| normalize_llm_text(object.get("character_name")))
        else {
            continue;
        };
        if existing_visible.contains(&name) || !seen.insert(name.clone()) {
            continue;
        }
        let role = normalize_llm_text(object.get("role"))
            .or_else(|| normalize_llm_text(object.get("identity")))
            .unwrap_or_default();
        let background_prompt = normalize_llm_text(object.get("background_prompt"))
            .or_else(|| normalize_llm_text(object.get("description")))
            .or_else(|| normalize_llm_text(object.get("profile")))
            .or_else(|| {
                let location =
                    normalize_llm_text(object.get("initial_location")).unwrap_or_default();
                let parts = [role.clone(), location]
                    .into_iter()
                    .filter(|item| !item.trim().is_empty())
                    .collect::<Vec<_>>();
                if parts.is_empty() {
                    None
                } else {
                    Some(parts.join(" / "))
                }
            })
            .unwrap_or_default();
        let model = normalize_llm_text(object.get("model")).unwrap_or_default();
        let memory_strategy = normalize_llm_text(object.get("memory_strategy")).unwrap_or_default();
        let world_name = normalize_llm_text(object.get("world_name"))
            .unwrap_or_else(|| session.world_name.clone());
        let attributes = object
            .get("attributes")
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| normalize_llm_text(Some(item)))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        normalized.push(serde_json::json!({
            "name": name,
            "world_name": world_name,
            "role": role,
            "background_prompt": background_prompt,
            "model": model,
            "memory_strategy": memory_strategy,
            "attributes": attributes,
        }));
        if normalized.len() >= 4 {
            break;
        }
    }
    normalized
}

pub(crate) fn arg_string_list(value: Option<&serde_json::Value>) -> Vec<String> {
    value
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(|value| value.trim().to_string()))
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

/// 角色（含 agent_chat 的唯一 agent）可用的工具能力表。
///
/// 与主控表的区别：不含 list_scenes / list_characters / change_scene /
/// switch_player_character / generate_image —— 这些是导演职权，其效果函数会写
/// 场景切换、换玩家角色等字段，角色路没有对应的写回通道，下发了也执行不了。
/// 角色能拿到的是 schedule_notification 与世界授权的自定义 MCP 工具。
pub(crate) fn build_character_tool_capabilities(
    world: &WorldDefinition,
    mcp_tools: &[McpToolDefinition],
) -> Vec<serde_json::Value> {
    let allowed = world_allowed_tool_ids(world)
        .into_iter()
        .collect::<BTreeSet<_>>();
    let mut tools = Vec::new();
    if allowed.contains(MCP_TOOL_SCHEDULE_NOTIFICATION_ID) {
        tools.push(notification_tool_definition());
    }
    tools.extend(custom_mcp_tool_capabilities(&allowed, mcp_tools));
    tools
}

/// 世界授权的工具 id 列表。`WorldDirectorService::resolve_world_allowed_tool_ids`
/// 是它的方法形态包装，两者共用这一份读取逻辑。
pub(crate) fn world_allowed_tool_ids(world: &WorldDefinition) -> Vec<String> {
    world
        .director_config
        .get("allowed_mcp_tool_ids")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(|value| value.trim().to_string()))
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

/// 世界授权的自定义 MCP 工具（排除引擎内置 id）的能力条目。主控表与角色表共用，
/// 保证同一个工具在两条路上下发给模型的 name/description/schema 完全一致。
pub(crate) fn custom_mcp_tool_capabilities(
    allowed: &BTreeSet<String>,
    mcp_tools: &[McpToolDefinition],
) -> Vec<serde_json::Value> {
    mcp_tools
        .iter()
        .filter(|tool| {
            tool.enabled && allowed.contains(&tool.id) && !is_builtin_mcp_tool_id(&tool.id)
        })
        .filter(|tool| mcp_tool_exposure_mode(&tool.exposure_policy) != "disabled")
        .filter(|tool| !tool.tool_name.trim().is_empty())
        .map(|tool| {
            serde_json::json!({
                "tool_name": tool.tool_name.trim(),
                "description": tool.description.trim(),
                "arguments_schema": tool.input_schema.clone(),
                "server_name": tool.server_name.clone(),
                "mcp_tool_id": tool.id.clone(),
            })
        })
        .collect()
}

/// 把 `build_director_tool_capabilities` 产出的工具能力 JSON 转成下发给模型的
/// `ChatToolDefinition`。主控路与角色路共用这一份实现：不管工具来自引擎内置还是
/// 用户导入的工具包，发给模型的形状都一致。
pub(crate) fn tool_capabilities_to_chat_definitions(
    capabilities: &[serde_json::Value],
) -> Vec<ChatToolDefinition> {
    capabilities
        .iter()
        .filter_map(|tool| {
            let object = tool.as_object()?;
            let name = object
                .get("tool_name")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())?;
            Some(ChatToolDefinition {
                name: name.to_string(),
                description: object
                    .get("description")
                    .and_then(|value| value.as_str())
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty()),
                input_schema: object
                    .get("arguments_schema")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({ "type": "object" })),
            })
        })
        .collect()
}

/// 内置工具的模型侧调用名。这些由同步效果函数处理，不走 MCP 执行器。
pub(crate) fn is_builtin_tool_name(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "list_scenes"
            | "list_characters"
            | "change_scene"
            | "switch_player_character"
            | "generate_image"
            | "schedule_notification"
    )
}

pub(crate) fn mcp_tool_exposure_mode(policy: &serde_json::Value) -> &str {
    policy
        .as_str()
        .or_else(|| policy.get("mode").and_then(|value| value.as_str()))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("on-demand")
}
