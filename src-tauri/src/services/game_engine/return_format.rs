//! 模型返回格式的单一事实源。
//!
//! 导演路和角色路的返回格式此前散落多处手写：模型可见契约（response_contract
//! 的 core_fields / optional_fields / field_guide）、角色路仅 trace 的契约 JSON、
//! response_schema 请求参数，以及解析端容忍的旧字段，内容已经互相漂移过。
//! 本模块用一张表定义每个字段，契约与 schema 在运行时从表生成，防漂移测试
//! 保证两者永远一致。
//!
//! 约定：
//! - `guide` 有值的字段进模型可见契约：导演路进 `field_guide`，角色路进 trace
//!   契约的 `field_descriptions`。
//! - `property` 有值的字段进 response_schema 请求参数。
//! - `required` 为 true 的字段：导演路进 `core_fields` 与 schema.required，
//!   角色路进 schema.required。
//! - 已退役、仅解析端兼容的旧 JSON 字段不进活动表，统一登记在
//!   `DIRECTOR_LEGACY_RETURN_FIELD_IDS`；防漂移测试确保它们不会回流契约/schema。
//! - 解析端（director 的 parse_runtime_payload / 角色的 parse_character_response）
//!   按字符串字面量读取字段，无法从表生成；新增字段时改表的同时必须同步解析端，
//!   并检查 BUILTIN_RESPONSE_CONTRACT / BUILTIN_MEMORY_CONTRACT 的示例文本。

use serde_json::json;

pub struct ReturnFieldSpec {
    pub id: &'static str,
    /// 模型可见的字段说明；None = 不进契约。
    pub guide: Option<&'static str>,
    /// 每回合必填。
    pub required: bool,
    /// response_schema 请求参数里的 property；None = 不进 schema。
    pub property: Option<serde_json::Value>,
}

fn spec(
    id: &'static str,
    guide: Option<&'static str>,
    required: bool,
    property: Option<serde_json::Value>,
) -> ReturnFieldSpec {
    ReturnFieldSpec {
        id,
        guide,
        required,
        property,
    }
}

// ---------------------------------------------------------------------------
// 导演（世界主控）返回格式
// ---------------------------------------------------------------------------

/// 导演返回的活动字段表：场景/名册/新角色/换角/背景图/立绘/交互已迁往工具调用，
/// 不再作为 JSON 字段宣传（见 project.md「导演输出契约」）。
pub fn director_return_fields() -> Vec<ReturnFieldSpec> {
    vec![
        spec(
            "planned_speakers",
            Some("核心字段，每回合都返回。按发言顺序列出本回合要发言的角色名。不要包含玩家角色；只能使用当前场景角色，或通过 change_scene 工具 new_characters 新建的角色。没有角色发言时返回空数组。"),
            true,
            Some(json!({
                "type": "array",
                "items": { "type": "string" }
            })),
        ),
        spec(
            "event",
            Some("核心字段，每回合都返回。用旁白口吻写本回合实际发生的剧情推进（1~3 句）：谁做了什么、出现什么新情况、局面发生了怎样的变化或代价。这段文字会原样展示给玩家，并进入后续历史；不要写角色台词、不要复述玩家发言、不要只写气氛或心理活动，也不要以旁白口吻替玩家做决定。即使没有角色发言，也必须给出一件推动剧情的新事件。"),
            true,
            Some(json!({ "type": "string" })),
        ),
        spec(
            "world_phase",
            Some("叙事阶段，只能是 opening、escalation 或 crisis；仅在阶段推进时返回。"),
            false,
            Some(json!({ "type": "string" })),
        ),
        spec(
            "current_line",
            Some("一条非对话的旁白或舞台提示，仅在确实需要更新场景时返回；角色对白由角色模型单独生成。"),
            false,
            Some(json!({ "type": "string" })),
        ),
        spec(
            "session_attribute_updates",
            Some("会话/世界自定义属性的更新，格式为 { key, value }；key 必须是已定义的属性键。时间等世界状态也应使用这里更新，不要使用固定时间字段。"),
            false,
            Some(json!({
                "type": "array",
                "description": "当前会话的自定义属性更新；key 必须匹配属性定义中的键。",
                "items": {
                    "type": "object",
                    "required": ["key", "value"],
                    "additionalProperties": true,
                    "properties": {
                        "key": { "type": "string" },
                        "value": {}
                    }
                }
            })),
        ),
        spec(
            "character_attribute_updates",
            Some("角色自定义属性的更新，格式为 { character_name, key, value }；key 必须匹配 current_state.runtime_attributes 中的属性键。"),
            false,
            Some(json!({
                "type": "array",
                "description": "会话角色的自定义属性更新；key 必须匹配属性定义中的键。",
                "items": {
                    "type": "object",
                    "required": ["character_name", "key", "value"],
                    "additionalProperties": true,
                    "properties": {
                        "character_name": { "type": "string" },
                        "key": { "type": "string" },
                        "value": {}
                    }
                }
            })),
        ),
    ]
}

/// 已退役的导演 JSON 字段：效果由工具完成，解析端仍兼容（工具执行器把效果
/// 写进这些内部字段再走同一条解析管线）。新代码不得把它们加回契约或 schema。
/// 本清单是登记性质的注册表，供防漂移测试与排查文档消费，不发给模型。
#[allow(dead_code)]
pub const DIRECTOR_LEGACY_RETURN_FIELD_IDS: &[&str] = &[
    "next_scene_name",
    "next_location",
    "next_scene_background_hint",
    "scene_background_hint",
    "next_scene_tags",
    "scene_tags",
    "scene_visible_characters",
    "generated_characters",
    "switch_character_proposal",
    "interaction",
    "memory_events",
    "fact_extractions",
    "background_asset_name",
    "background_asset_path",
    "background_generation_prompt",
    "background_generation_name",
    "character_visual_directives",
];

/// 契约明令禁止的返回字段。
pub const DIRECTOR_FORBIDDEN_FIELDS: &[&str] = &[
    "state_tags",
    "system_messages",
    "system_log",
    "character_visual_directives",
];

/// 每回合必填的核心字段（core_fields / schema.required）。
pub fn director_core_field_ids() -> Vec<&'static str> {
    director_return_fields()
        .into_iter()
        .filter(|field| field.required)
        .map(|field| field.id)
        .collect()
}

/// 仅变化时返回的可选字段（optional_fields_when_changed）。
pub fn director_optional_changed_field_ids() -> Vec<&'static str> {
    director_return_fields()
        .into_iter()
        .filter(|field| !field.required && field.guide.is_some())
        .map(|field| field.id)
        .collect()
}

/// 模型可见的字段说明（field_guide）。
pub fn director_field_guide() -> serde_json::Value {
    let mut guide = serde_json::Map::new();
    for field in director_return_fields() {
        if let Some(text) = field.guide {
            guide.insert(
                field.id.to_string(),
                serde_json::Value::String(text.to_string()),
            );
        }
    }
    serde_json::Value::Object(guide)
}

/// 结构化输出的更新格式示例（runtime_update_format）。
pub fn director_runtime_update_format() -> serde_json::Value {
    json!({
        "session_attribute_updates": [
            { "key": "世界属性键", "value": "新值" }
        ],
        "character_attribute_updates": [
            { "character_name": "角色名称", "key": "角色属性键", "value": "新值" }
        ]
    })
}

/// 导演 response_schema 请求参数，从活动字段表生成。
pub fn director_response_schema() -> serde_json::Value {
    let fields = director_return_fields();
    let mut properties = serde_json::Map::new();
    for field in &fields {
        if let Some(property) = &field.property {
            properties.insert(field.id.to_string(), property.clone());
        }
    }
    json!({
        "type": "object",
        "additionalProperties": true,
        "required": director_core_field_ids(),
        "properties": serde_json::Value::Object(properties),
    })
}

// ---------------------------------------------------------------------------
// 角色返回格式
// ---------------------------------------------------------------------------

/// 角色返回字段表。`property` 只覆盖 schema 严格声明的最小集合
/// （speaker/content/narration/pass）；状态与记忆字段靠
/// additionalProperties: true 放行，由解析端容错读取。
pub fn character_return_fields() -> Vec<ReturnFieldSpec> {
    vec![
        spec(
            "speaker",
            Some("当前扮演角色名称。"),
            true,
            Some(json!({ "type": "string" })),
        ),
        spec(
            "content",
            Some("角色本回合的发言内容。"),
            true,
            Some(json!({ "type": "string" })),
        ),
        spec(
            "narration",
            Some("可选的场景旁白。"),
            true,
            Some(json!({ "type": "string" })),
        ),
        spec(
            "pass",
            Some("本回合是否交还发言顺序。"),
            false,
            Some(json!({
                "type": "boolean",
                "description": "Set true when this character intentionally stays silent this turn. content and narration must then be empty strings."
            })),
        ),
        spec(
            "session_attribute_updates",
            Some("会话/世界自定义属性更新，key 必须匹配世界属性定义；时间等世界状态也通过此字段更新。"),
            false,
            None,
        ),
        spec(
            "character_attribute_updates",
            Some("角色自定义属性更新，key 必须匹配角色属性定义。"),
            false,
            None,
        ),
        spec(
            "memory_entries",
            Some("需要长期保留的记忆。"),
            false,
            None,
        ),
        spec(
            "fact_extractions",
            Some("从本回合内容中提取的事实关系。"),
            false,
            None,
        ),
    ]
}

/// 角色正文字段的容错别名：模型写坏标准字段名时，解析端按此顺序打捞正文。
/// dialogue.rs 的打捞逻辑必须使用这份列表，保证别名认知与格式表同源。
pub const CHARACTER_CONTENT_FIELD_ALIASES: &[&str] = &[
    "content", "response", "message", "text", "say", "dialogue", "line", "reply",
];

/// 角色 trace 契约的字段清单（fields），按表顺序输出。
pub fn character_contract_field_ids() -> Vec<&'static str> {
    character_return_fields()
        .into_iter()
        .map(|field| field.id)
        .collect()
}

/// 角色 trace 契约的字段说明（field_descriptions）。
pub fn character_field_descriptions() -> serde_json::Value {
    let mut descriptions = serde_json::Map::new();
    for field in character_return_fields() {
        if let Some(text) = field.guide {
            descriptions.insert(
                field.id.to_string(),
                serde_json::Value::String(text.to_string()),
            );
        }
    }
    serde_json::Value::Object(descriptions)
}

/// 角色 trace 契约的结构化更新格式示例（runtime_update_format）。
pub fn character_runtime_update_format() -> serde_json::Value {
    json!({
        "session_attribute_updates": [
            { "key": "世界属性键", "value": "新值" }
        ],
        "character_attribute_updates": [
            { "character_name": "角色名称", "key": "角色属性键", "value": "新值" }
        ],
        "memory_entries": [
            { "content": "记忆内容", "character_names": ["角色名称"] }
        ],
        "fact_extractions": [
            { "subject": "实体", "predicate": "关系", "object": "实体或值", "action": "upsert | invalidate" }
        ]
    })
}

/// 角色工具调用策略（trace 契约 tool_policy）。
pub const CHARACTER_TOOL_POLICY: &str =
    "所有允许的工具都必须使用服务商原生 tool_calls 调用；不要在 JSON 正文中填写 tool_calls、工具名或工具参数。";

/// 角色 response_schema 请求参数，从字段表生成。
pub fn character_response_schema() -> serde_json::Value {
    let fields = character_return_fields();
    let mut properties = serde_json::Map::new();
    for field in &fields {
        if let Some(property) = &field.property {
            properties.insert(field.id.to_string(), property.clone());
        }
    }
    json!({
        "type": "object",
        "required": fields
            .iter()
            .filter(|field| field.required)
            .map(|field| field.id)
            .collect::<Vec<_>>(),
        "additionalProperties": true,
        "properties": serde_json::Value::Object(properties),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 导演表内每个活动字段必须同时进契约和 schema——两边从同一张表生成，
    /// 任何只改一边的漂移都会在这里暴露。
    #[test]
    fn director_contract_and_schema_come_from_one_table() {
        let fields = director_return_fields();
        let active_ids: Vec<&str> = fields.iter().map(|field| field.id).collect();
        let guide_ids = director_field_guide();
        let schema = director_response_schema();

        for id in &active_ids {
            assert!(
                guide_ids.get(*id).is_some(),
                "活动字段 {id} 缺少模型可见说明"
            );
            assert!(
                schema.pointer(&format!("/properties/{id}")).is_some(),
                "活动字段 {id} 缺少 schema property"
            );
        }
        assert_eq!(
            director_core_field_ids(),
            vec!["planned_speakers", "event"],
            "core 字段由 required 标记生成"
        );
        assert_eq!(
            schema.get("required").unwrap(),
            &serde_json::json!(["planned_speakers", "event"])
        );
        // 退役字段不得回流契约或 schema。
        for legacy in DIRECTOR_LEGACY_RETURN_FIELD_IDS {
            assert!(
                !active_ids.contains(legacy),
                "退役字段 {legacy} 被加回了活动表"
            );
            assert!(guide_ids.get(*legacy).is_none());
            assert!(schema.pointer(&format!("/properties/{legacy}")).is_none());
        }
        for forbidden in DIRECTOR_FORBIDDEN_FIELDS {
            assert!(guide_ids.get(*forbidden).is_none());
            assert!(schema
                .pointer(&format!("/properties/{forbidden}"))
                .is_none());
        }
    }

    /// 角色表：schema 只声明最小严格集合，trace 契约覆盖全部字段。
    #[test]
    fn character_schema_and_contract_match_the_table() {
        let fields = character_return_fields();
        let ids: Vec<&str> = fields.iter().map(|field| field.id).collect();
        assert_eq!(
            ids,
            vec![
                "speaker",
                "content",
                "narration",
                "pass",
                "session_attribute_updates",
                "character_attribute_updates",
                "memory_entries",
                "fact_extractions",
            ]
        );
        assert_eq!(character_contract_field_ids(), ids);

        let schema = character_response_schema();
        assert_eq!(
            schema.get("required").unwrap(),
            &serde_json::json!(["speaker", "content", "narration"])
        );
        let properties = schema
            .get("properties")
            .and_then(|value| value.as_object())
            .expect("schema properties");
        for id in properties.keys() {
            assert!(ids.contains(&id.as_str()), "schema 出现表外字段 {id}");
        }
        assert_eq!(properties.len(), 4);

        let descriptions = character_field_descriptions();
        for id in &ids {
            assert!(
                descriptions.get(*id).is_some(),
                "字段 {id} 缺少 trace 契约说明"
            );
        }
        // 正文的容错别名必须包含标准字段本身，且不含旁白/发言人等非正文字段。
        assert!(CHARACTER_CONTENT_FIELD_ALIASES.contains(&"content"));
        assert!(!CHARACTER_CONTENT_FIELD_ALIASES.contains(&"narration"));
        assert!(!CHARACTER_CONTENT_FIELD_ALIASES.contains(&"speaker"));
    }
}
