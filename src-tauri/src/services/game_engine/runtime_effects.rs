use std::collections::HashMap;

use chrono::Utc;
use rusqlite::Connection;

use crate::db::repositories::attribute_repo::AttributeRepository;
use crate::models::attribute::{AttributeSchema, AttributeValueUpsertRequest};
use crate::models::character::CharacterDefinition;
use crate::models::memory::{FactUpdate, MemoryEntry};
use crate::models::scheduled_notification::PendingScheduledNotification;
use crate::models::session::{
    ChatMessage, MessageContent, RuntimeAttributeItem, SceneRuntime, SessionSnapshot,
};
use crate::services::game_engine::orchestrator::DirectorDecision;

#[derive(Clone, Default)]
pub(crate) struct DirectorRuntimeApplication {
    pub scene_runtime: Option<SceneRuntime>,
    pub system_messages: Vec<ChatMessage>,
    pub system_log_lines: Vec<String>,
    pub tool_call_logs: Vec<String>,
    pub pending_notifications: Vec<PendingScheduledNotification>,
    pub scene_tags: Vec<String>,
    pub state_phase: String,
    pub player_stats: Option<Vec<String>>,
    pub session_attribute_updates: Vec<ParsedAttributeUpdate>,
    pub character_attribute_updates: Vec<ParsedCharacterAttributeUpdate>,
    pub memory_entries: Vec<MemoryEntry>,
    pub fact_updates: Vec<FactUpdate>,
}

#[derive(Clone)]
pub(crate) struct ParsedAttributeUpdate {
    pub schema_id: String,
    pub value: serde_json::Value,
}

#[derive(Clone)]
pub(crate) struct ParsedCharacterAttributeUpdate {
    pub character_id: String,
    pub schema_id: String,
    pub value: serde_json::Value,
}

pub(crate) fn apply_director_runtime_effects(
    conn: &Connection,
    scene_manager: &crate::services::game_engine::scene::SceneManager,
    world: &crate::models::world::WorldDefinition,
    session: &SessionSnapshot,
    characters: &[CharacterDefinition],
    turn_index: i32,
    player_input: &str,
    parsed: &serde_json::Value,
) -> Result<DirectorRuntimeApplication, String> {
    let mut runtime = DirectorRuntimeApplication {
        system_messages: Vec::new(),
        system_log_lines: Vec::new(),
        tool_call_logs: parse_tool_call_logs(parsed.get("tool_calls")),
        pending_notifications: parse_pending_notifications(parsed.get("pending_notifications")),
        scene_tags: parse_string_array(
            parsed
                .get("next_scene_tags")
                .or_else(|| parsed.get("scene_tags")),
            &session.scene.temporary_tags,
        ),
        state_phase: parsed
            .get("state_phase")
            .or_else(|| parsed.get("world_phase"))
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| session.state.phase.clone()),
        player_stats: parse_optional_string_array(
            parsed.get("player_stats").or_else(|| parsed.get("stats")),
        ),
        ..DirectorRuntimeApplication::default()
    };
    runtime.system_messages.extend(parse_tool_call_messages(
        parsed.get("tool_calls"),
        turn_index,
    ));

    let attribute_repo = AttributeRepository::new(conn);
    let schema_map = attribute_repo
        .list_schemas(None)?
        .into_iter()
        .filter(|schema| schema.world_id.as_deref().is_none_or(|id| id == world.id))
        .map(|schema| (schema.key.trim().to_string(), schema))
        .collect::<HashMap<_, _>>();

    runtime.session_attribute_updates = parse_attribute_updates(
        parsed
            .get("session_attribute_updates")
            .or_else(|| parsed.get("attribute_updates")),
        &schema_map,
    );
    runtime.character_attribute_updates = parse_character_attribute_updates(
        parsed.get("character_attribute_updates"),
        &schema_map,
        characters,
        &session.id,
    );

    let runtime_session_attributes =
        load_runtime_session_attributes(conn, &session.id, &schema_map)?;
    let director_decision = DirectorDecision {
        world_phase: parsed
            .get("world_phase")
            .or_else(|| parsed.get("state_phase"))
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| session.state.phase.clone()),
        next_location: parsed
            .get("next_location")
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        next_scene_name: parsed
            .get("next_scene_name")
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        next_scene_background_hint: parsed
            .get("next_scene_background_hint")
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        scene_visible_characters: parse_string_array(parsed.get("scene_visible_characters"), &[]),
    };
    let has_explicit_scene_visible = parsed
        .get("scene_visible_characters")
        .and_then(|value| value.as_array())
        .is_some();

    for update in &runtime.session_attribute_updates {
        attribute_repo.upsert_value(&AttributeValueUpsertRequest {
            schema_id: update.schema_id.clone(),
            owner_type: "session".to_string(),
            owner_id: session.id.clone(),
            value: update.value.clone(),
            source: "director".to_string(),
        })?;
    }
    for update in &runtime.character_attribute_updates {
        attribute_repo.upsert_value(&AttributeValueUpsertRequest {
            schema_id: update.schema_id.clone(),
            owner_type: "session_character".to_string(),
            owner_id: format!("{}:{}", session.id, update.character_id),
            value: update.value.clone(),
            source: "director".to_string(),
        })?;
    }
    let scene_runtime = scene_manager.refresh_scene(
        session,
        &director_decision,
        if !has_explicit_scene_visible && director_decision.scene_visible_characters.is_empty() {
            &session.visible_characters
        } else {
            &director_decision.scene_visible_characters
        },
        &runtime_session_attributes,
    );
    runtime.scene_runtime = Some(scene_runtime.scene.clone());
    runtime
        .system_log_lines
        .extend(scene_runtime.debug_lines.clone());
    runtime
        .system_messages
        .extend(
            scene_runtime
                .system_messages
                .iter()
                .map(|content| ChatMessage {
                    message_id: ChatMessage::generate_id(),
                    created_at: chrono::Utc::now().to_rfc3339(),
                    parent_message_id: None,
                    role: "system".to_string(),
                    content: MessageContent::Text(content.clone()),
                    speaker: None,
                    metadata: Some(serde_json::json!({
                        "turn_index": turn_index,
                        "action_type": "scene_runtime_message",
                    })),
                }),
        );
    if runtime.scene_tags.is_empty() {
        runtime.scene_tags = scene_runtime.scene.temporary_tags.clone();
    } else {
        runtime.scene_tags = unique_strings(
            runtime
                .scene_tags
                .iter()
                .chain(scene_runtime.scene.temporary_tags.iter())
                .cloned()
                .collect(),
        );
    }
    runtime.memory_entries = parse_memory_entries(
        world,
        session,
        characters,
        turn_index,
        player_input,
        parsed.get("memory_events"),
    );
    runtime.fact_updates = parse_fact_extractions(parsed.get("fact_extractions"));
    Ok(runtime)
}

pub(crate) fn apply_director_runtime_effects_with_preface(
    conn: &Connection,
    scene_manager: &crate::services::game_engine::scene::SceneManager,
    world: &crate::models::world::WorldDefinition,
    session: &SessionSnapshot,
    characters: &[CharacterDefinition],
    turn_index: i32,
    player_input: &str,
    parsed: &serde_json::Value,
    pre_runtime_system_messages: &[ChatMessage],
) -> Result<DirectorRuntimeApplication, String> {
    let mut runtime = apply_director_runtime_effects(
        conn,
        scene_manager,
        world,
        session,
        characters,
        turn_index,
        player_input,
        parsed,
    )?;
    runtime
        .system_messages
        .splice(0..0, pre_runtime_system_messages.iter().cloned());
    Ok(runtime)
}

fn load_runtime_session_attributes(
    conn: &Connection,
    session_id: &str,
    schema_map: &HashMap<String, AttributeSchema>,
) -> Result<Vec<RuntimeAttributeItem>, String> {
    let values =
        AttributeRepository::new(conn).list_values(Some("session"), Some(session_id), None)?;
    let mut items = Vec::new();
    for value in values {
        let Some(schema) = schema_map.values().find(|item| item.id == value.schema_id) else {
            continue;
        };
        items.push(RuntimeAttributeItem {
            schema_id: value.schema_id.clone(),
            key: schema.key.clone(),
            label: schema.label.clone(),
            description: schema.description.clone(),
            value_type: schema.value_type.clone(),
            enum_options: schema.enum_options.clone(),
            value: value.value.clone(),
            source: value.source.clone(),
            display_policy: serde_json::to_value(&schema.display_policy).unwrap_or_default(),
            influence_policy: serde_json::to_value(&schema.influence_policy).unwrap_or_default(),
        });
    }
    Ok(items)
}

fn parse_optional_string_array(value: Option<&serde_json::Value>) -> Option<Vec<String>> {
    value.and_then(|value| {
        let parsed = parse_string_array(Some(value), &[]);
        if parsed.is_empty() {
            None
        } else {
            Some(parsed)
        }
    })
}

fn parse_string_array(value: Option<&serde_json::Value>, fallback: &[String]) -> Vec<String> {
    let Some(value) = value else {
        return fallback.to_vec();
    };
    let Some(items) = value.as_array() else {
        return fallback.to_vec();
    };
    items
        .iter()
        .filter_map(|item| item.as_str())
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .fold(Vec::<String>::new(), |mut acc, item| {
            if !acc.contains(&item) {
                acc.push(item);
            }
            acc
        })
}

fn parse_tool_call_logs(value: Option<&serde_json::Value>) -> Vec<String> {
    let Some(items) = value.and_then(|value| value.as_array()) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| item.as_object())
        .filter_map(|item| {
            let tool_name = item
                .get("tool_name")
                .and_then(|value| value.as_str())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())?;
            let arguments = item
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            Some(format!(
                "Director requested tool: {} {}",
                tool_name,
                serde_json::to_string(&arguments).unwrap_or_default()
            ))
        })
        .collect()
}

fn parse_tool_call_messages(
    value: Option<&serde_json::Value>,
    turn_index: i32,
) -> Vec<ChatMessage> {
    let Some(items) = value.and_then(|value| value.as_array()) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| item.as_object())
        .filter_map(|item| {
            let tool_name = item
                .get("tool_name")
                .and_then(|value| value.as_str())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())?;
            Some(ChatMessage {
                message_id: ChatMessage::generate_id(),
                created_at: chrono::Utc::now().to_rfc3339(),
                parent_message_id: None,
                role: "system".to_string(),
                content: MessageContent::Text(format!("主控请求工具：{}", tool_name)),
                speaker: None,
                metadata: Some(serde_json::json!({
                    "turn_index": turn_index,
                    "action_type": "tool_call",
                    "tool_call": item,
                })),
            })
        })
        .collect()
}

fn parse_pending_notifications(
    value: Option<&serde_json::Value>,
) -> Vec<PendingScheduledNotification> {
    let Some(items) = value.and_then(|value| value.as_array()) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            serde_json::from_value::<PendingScheduledNotification>(item.clone()).ok()
        })
        .collect()
}

fn parse_attribute_updates(
    value: Option<&serde_json::Value>,
    schema_map: &HashMap<String, AttributeSchema>,
) -> Vec<ParsedAttributeUpdate> {
    let Some(items) = value.and_then(|value| value.as_array()) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| item.as_object())
        .filter_map(|item| {
            let key = item
                .get("key")
                .and_then(|value| value.as_str())?
                .trim()
                .to_string();
            let schema = schema_map.get(&key)?;
            Some(ParsedAttributeUpdate {
                schema_id: schema.id.clone(),
                value: item
                    .get("value")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
            })
        })
        .collect()
}

fn parse_character_attribute_updates(
    value: Option<&serde_json::Value>,
    schema_map: &HashMap<String, AttributeSchema>,
    characters: &[CharacterDefinition],
    session_id: &str,
) -> Vec<ParsedCharacterAttributeUpdate> {
    let Some(items) = value.and_then(|value| value.as_array()) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| item.as_object())
        .filter_map(|item| {
            let key = item
                .get("key")
                .and_then(|value| value.as_str())?
                .trim()
                .to_string();
            let schema = schema_map.get(&key)?;
            let character_name = item
                .get("character_name")
                .or_else(|| item.get("speaker"))
                .and_then(|value| value.as_str())
                .map(|value| value.trim().to_string())?;
            let character_id = characters
                .iter()
                .find(|character| character.name == character_name)
                .map(|character| character.id.clone())
                .or_else(|| {
                    item.get("character_id")
                        .and_then(|value| value.as_str())
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty())
                })?;
            let _owner_id = format!("{session_id}:{character_id}");
            Some(ParsedCharacterAttributeUpdate {
                character_id,
                schema_id: schema.id.clone(),
                value: item
                    .get("value")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
            })
        })
        .collect()
}

fn parse_memory_entries(
    world: &crate::models::world::WorldDefinition,
    session: &SessionSnapshot,
    characters: &[CharacterDefinition],
    turn_index: i32,
    player_input: &str,
    value: Option<&serde_json::Value>,
) -> Vec<MemoryEntry> {
    let Some(items) = value.and_then(|value| value.as_array()) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| item.as_object())
        .flat_map(|item| {
            let content = item
                .get("content")
                .and_then(|value| value.as_str())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            let Some(content) = content else {
                return Vec::new();
            };
            let character_targets =
                parse_memory_target_characters(item.get("character_names"), characters);
            let participants =
                parse_string_array(item.get("participants"), &session.scene.present_characters);
            let importance = item
                .get("importance")
                .and_then(|value| value.as_f64())
                .unwrap_or(0.5);
            character_targets
                .into_iter()
                .map(|character_id| {
                    build_memory_entry(
                        world,
                        session,
                        turn_index,
                        &character_id,
                        item.get("layer")
                            .and_then(|value| value.as_str())
                            .unwrap_or("short_term"),
                        &content,
                        item.get("source")
                            .and_then(|value| value.as_str())
                            .unwrap_or("director"),
                        importance,
                        item.get("memory_type")
                            .and_then(|value| value.as_str())
                            .unwrap_or("event"),
                        item.get("speaker").and_then(|value| value.as_str()),
                        item.get("role").and_then(|value| value.as_str()),
                        item.get("location")
                            .and_then(|value| value.as_str())
                            .or(Some(session.location.as_str())),
                        item.get("scene_id")
                            .and_then(|value| value.as_str())
                            .or(Some(session.scene.scene_id.as_str())),
                        if participants.is_empty() {
                            vec![
                                session.player_character_name.clone(),
                                player_input.trim().to_string(),
                            ]
                        } else {
                            participants.clone()
                        },
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// 解析 LLM 输出的 fact_extractions 数组为结构化事实更新。
/// 非法项直接丢弃(与 parse_memory_entries 的容错策略一致):
/// subject/predicate 为空丢弃;upsert 缺 object 丢弃;action 只认 invalidate,其余视为 upsert。
fn parse_fact_extractions(value: Option<&serde_json::Value>) -> Vec<FactUpdate> {
    let Some(items) = value.and_then(|value| value.as_array()) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| item.as_object())
        .filter_map(|item| {
            let subject = item
                .get("subject")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())?
                .to_string();
            let predicate = item
                .get("predicate")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())?
                .to_string();
            let object = item
                .get("object")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .unwrap_or_default()
                .to_string();
            let action = item
                .get("action")
                .and_then(|value| value.as_str())
                .map(|value| value.trim().to_lowercase())
                .filter(|value| value == "invalidate")
                .unwrap_or_else(|| "upsert".to_string());
            if action == "upsert" && object.is_empty() {
                return None;
            }
            Some(FactUpdate {
                subject,
                predicate,
                object,
                action,
                subject_type: item
                    .get("subject_type")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .unwrap_or_default()
                    .to_string(),
                object_type: item
                    .get("object_type")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .unwrap_or_default()
                    .to_string(),
                confidence: item
                    .get("confidence")
                    .and_then(|value| value.as_f64())
                    .unwrap_or(0.7),
                asserted_by: item
                    .get("asserted_by")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("director")
                    .to_string(),
                asserted_by_id: item
                    .get("asserted_by_id")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string),
                visibility: match item
                    .get("visibility")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                {
                    Some("public") => "public".to_string(),
                    Some("private") => "private".to_string(),
                    _ => "scene".to_string(),
                },
                truth_status: "asserted".to_string(),
                witnesses: parse_string_array(item.get("witnesses"), &[]),
            })
        })
        .collect()
}

fn parse_memory_target_characters(
    value: Option<&serde_json::Value>,
    characters: &[CharacterDefinition],
) -> Vec<String> {
    // 受众缺失或无法解析时丢弃，不能把模型记忆广播给在场角色。
    let mut target_ids = Vec::new();
    for name in parse_string_array(value, &[]) {
        if let Some(character_id) = characters
            .iter()
            .find(|character| character.name == name)
            .map(|character| character.id.clone())
        {
            if !target_ids.contains(&character_id) {
                target_ids.push(character_id);
            }
        }
    }
    target_ids
}

fn normalize_memory_text(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_whitespace() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
}

fn extract_memory_keywords(content: &str, location: &str, participants: &[String]) -> Vec<String> {
    let mut keywords = vec![normalize_memory_text(location)];
    keywords.extend(
        participants
            .iter()
            .map(|value| normalize_memory_text(value))
            .filter(|value| !value.is_empty()),
    );
    keywords.extend(
        content
            .split(|character: char| {
                character.is_whitespace() || matches!(character, ',' | '，' | '。' | ':' | '：')
            })
            .map(normalize_memory_text)
            .filter(|value| value.len() >= 2),
    );
    keywords.retain(|value| !value.is_empty());
    keywords.sort();
    keywords.dedup();
    keywords
}

fn build_memory_entry(
    world: &crate::models::world::WorldDefinition,
    session: &SessionSnapshot,
    turn_index: i32,
    character_id: &str,
    layer: &str,
    content: &str,
    source: &str,
    importance: f64,
    memory_type: &str,
    speaker: Option<&str>,
    role: Option<&str>,
    location: Option<&str>,
    scene_id: Option<&str>,
    participants: Vec<String>,
) -> MemoryEntry {
    MemoryEntry {
        id: format!("mem-{}", uuid::Uuid::new_v4().simple()),
        world_id: world.id.clone(),
        session_id: session.id.clone(),
        character_id: character_id.to_string(),
        layer: layer.trim().to_string(),
        content: content.trim().to_string(),
        source: source.trim().to_string(),
        importance,
        created_at: Utc::now().to_rfc3339(),
        turn_index,
        conversation_id: Some(session.id.clone()),
        event_id: None,
        item_id: None,
        scene_id: scene_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        memory_type: memory_type.trim().to_string(),
        speaker: speaker
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        role: role
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        location: location
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        participants: participants
            .into_iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>(),
        keywords: extract_memory_keywords(
            content,
            location.unwrap_or_else(|| session.location.as_str()),
            &session.scene.present_characters,
        ),
    }
}

fn unique_strings(values: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::<String>::new();
    let mut result = Vec::new();
    for value in values {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() || !seen.insert(trimmed.clone()) {
            continue;
        }
        result.push(trimmed);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{parse_fact_extractions, parse_memory_target_characters};
    use crate::models::character::CharacterDefinition;

    fn memory_character(id: &str, name: &str) -> CharacterDefinition {
        CharacterDefinition {
            id: id.to_string(),
            name: name.to_string(),
            world_id: "world-1".to_string(),
            role: String::new(),
            background_prompt: String::new(),
            model: String::new(),
            memory_strategy: String::new(),
            recent_dialogue_rounds: 8,
            attributes: vec![],
            portrait_assets: vec![],
            avatar_asset: String::new(),
            system_prompt_template: String::new(),
            response_contract_prompt: String::new(),
            narration_prompt: String::new(),
            runtime_system_prompt: String::new(),
            knowledge_base_ids: vec![],
        }
    }

    #[test]
    fn memory_targets_fail_closed_without_explicit_known_characters() {
        let characters = vec![
            memory_character("char-a", "Alice"),
            memory_character("char-b", "Bob"),
        ];
        for value in [
            None,
            Some(serde_json::json!([])),
            Some(serde_json::json!(["Unknown"])),
            Some(serde_json::json!("Alice")),
        ] {
            assert!(parse_memory_target_characters(value.as_ref(), &characters).is_empty());
        }
        assert_eq!(
            parse_memory_target_characters(
                Some(&serde_json::json!(["Alice", "Bob", "Alice", "Unknown"])),
                &characters,
            ),
            vec!["char-a".to_string(), "char-b".to_string()]
        );
    }

    #[test]
    fn parses_fact_extractions_with_defaults() {
        let payload = serde_json::json!([
            { "subject": "银钥匙", "predicate": "藏在", "object": "12号柜" },
            { "subject": "鲍勃", "predicate": "不知道", "object": "银钥匙的位置", "confidence": 0.9 }
        ]);
        let facts = parse_fact_extractions(Some(&payload));
        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0].subject, "银钥匙");
        assert_eq!(facts[0].action, "upsert");
        assert_eq!(facts[0].confidence, 0.7);
        assert_eq!(facts[0].asserted_by, "director");
        assert_eq!(facts[0].visibility, "scene");
        assert_eq!(facts[0].truth_status, "asserted");
        assert!(facts[0].witnesses.is_empty());
        assert_eq!(facts[1].confidence, 0.9);
    }

    #[test]
    fn fact_provenance_is_preserved_but_truth_remains_asserted() {
        let payload = serde_json::json!([{
            "subject": "银钥匙",
            "predicate": "藏在",
            "object": "12号柜",
            "asserted_by": "爱丽丝",
            "asserted_by_id": "char-alice",
            "visibility": "private",
            "truth_status": "confirmed",
            "witnesses": ["玩家", "爱丽丝"]
        }]);
        let facts = parse_fact_extractions(Some(&payload));
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].asserted_by, "爱丽丝");
        assert_eq!(facts[0].asserted_by_id.as_deref(), Some("char-alice"));
        assert_eq!(facts[0].visibility, "private");
        assert_eq!(facts[0].truth_status, "asserted");
        assert_eq!(facts[0].witnesses, vec!["玩家", "爱丽丝"]);
    }

    #[test]
    fn drops_fact_extraction_items_missing_required_fields() {
        let payload = serde_json::json!([
            { "subject": "", "predicate": "藏在", "object": "12号柜" },
            { "subject": "银钥匙", "object": "12号柜" },
            { "subject": "银钥匙", "predicate": "藏在" },
            "not-an-object"
        ]);
        assert!(parse_fact_extractions(Some(&payload)).is_empty());
    }

    #[test]
    fn invalidate_action_allows_empty_object() {
        let payload = serde_json::json!([
            { "subject": "银钥匙", "predicate": "藏在", "action": "invalidate" },
            { "subject": "银钥匙", "predicate": "藏在", "object": "12号柜", "action": "INVALIDATE" }
        ]);
        let facts = parse_fact_extractions(Some(&payload));
        assert_eq!(facts.len(), 2);
        assert!(facts.iter().all(|fact| fact.action == "invalidate"));
    }

    #[test]
    fn non_array_fact_extractions_returns_empty() {
        assert!(parse_fact_extractions(None).is_empty());
        assert!(parse_fact_extractions(Some(&serde_json::json!({"subject": "x"}))).is_empty());
    }
}
