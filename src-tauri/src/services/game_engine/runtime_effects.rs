use std::collections::HashMap;

use chrono::Utc;
use rusqlite::Connection;

use crate::db::repositories::attribute_repo::AttributeRepository;
use crate::models::attribute::{AttributeSchema, AttributeValueUpsertRequest};
use crate::models::character::CharacterDefinition;
use crate::models::memory::MemoryEntry;
use crate::models::scheduled_notification::PendingScheduledNotification;
use crate::models::session::{
    ChatMessage, InventoryItem, MessageContent, RuntimeAttributeItem, SceneRuntime, SessionSnapshot,
};
use crate::services::game_engine::orchestrator::DirectorDecision;

#[derive(Clone, Default)]
pub(crate) struct DirectorRuntimeApplication {
    pub scene_runtime: Option<SceneRuntime>,
    pub inventory_items: Option<Vec<InventoryItem>>,
    pub system_messages: Vec<ChatMessage>,
    pub system_log_lines: Vec<String>,
    pub tool_call_logs: Vec<String>,
    pub pending_notifications: Vec<PendingScheduledNotification>,
    pub scene_tags: Vec<String>,
    pub state_tags: Vec<String>,
    pub state_metrics: HashMap<String, f64>,
    pub state_phase: String,
    pub player_stats: Option<Vec<String>>,
    pub session_attribute_updates: Vec<ParsedAttributeUpdate>,
    pub character_attribute_updates: Vec<ParsedCharacterAttributeUpdate>,
    pub memory_entries: Vec<MemoryEntry>,
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
    inventory_service: &crate::services::game_engine::inventory::InventoryService,
    trigger_engine: &crate::services::game_engine::trigger::TriggerEngineService,
    rule_engine: &crate::services::game_engine::rule::RuleEngineService,
    scene_manager: &crate::services::game_engine::scene::SceneManager,
    state_engine: &crate::services::game_engine::state::StateEngineService,
    world: &crate::models::world::WorldDefinition,
    session: &SessionSnapshot,
    characters: &[CharacterDefinition],
    turn_index: i32,
    player_input: &str,
    parsed: &serde_json::Value,
) -> Result<DirectorRuntimeApplication, String> {
    let mut runtime = DirectorRuntimeApplication {
        inventory_items: parse_optional_inventory_items(
            parsed
                .get("inventory_items")
                .or_else(|| parsed.get("inventory")),
            &session.inventory_items,
        ),
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
        state_tags: session.state.tags.clone(),
        state_metrics: parse_metrics_object(
            parsed
                .get("state_metrics")
                .or_else(|| parsed.get("session_metrics"))
                .or_else(|| parsed.get("metrics")),
            &session.state.metrics,
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

    let inventory_runtime = inventory_service.evaluate_turn(
        session,
        player_input,
        parsed
            .get("next_location")
            .and_then(|value| value.as_str())
            .map(|value| value.trim())
            .filter(|value| !value.is_empty()),
    );
    if runtime.inventory_items.is_none() {
        runtime.inventory_items = Some(inventory_runtime.inventory_items.clone());
    }
    runtime
        .system_log_lines
        .extend(inventory_runtime.debug_lines.clone());
    runtime
        .system_messages
        .extend(
            inventory_runtime
                .system_messages
                .iter()
                .map(|content| ChatMessage {
                    role: "system".to_string(),
                    content: MessageContent::Text(content.clone()),
                    speaker: None,
                    metadata: Some(serde_json::json!({
                        "turn_index": turn_index,
                        "action_type": "inventory_runtime_message",
                    })),
                }),
        );

    let attribute_repo = AttributeRepository::new(conn);
    let schema_map = attribute_repo
        .list_schemas(None)?
        .into_iter()
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

    let mut runtime_session_attributes =
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

    let trigger_evaluation = trigger_engine.evaluate_turn(
        session,
        player_input,
        &director_decision,
        &runtime_session_attributes,
    );
    runtime
        .system_log_lines
        .extend(trigger_evaluation.debug_lines.clone());
    runtime
        .system_messages
        .extend(
            trigger_evaluation
                .system_messages
                .iter()
                .map(|content| ChatMessage {
                    role: "system".to_string(),
                    content: MessageContent::Text(content.clone()),
                    speaker: None,
                    metadata: Some(serde_json::json!({
                        "turn_index": turn_index,
                        "action_type": "trigger_runtime_message",
                    })),
                }),
        );
    for update in &trigger_evaluation.attribute_updates {
        if let Some(schema) = schema_map.get(update.schema_key.as_str()) {
            runtime
                .session_attribute_updates
                .push(ParsedAttributeUpdate {
                    schema_id: schema.id.clone(),
                    value: update.value.clone(),
                });
        }
    }
    let rules =
        crate::db::repositories::rule_repo::RuleRepository::new(conn).list(Some("session"))?;
    let rule_evaluation = rule_engine.evaluate_turn(
        session,
        player_input,
        &director_decision,
        &trigger_evaluation,
        &runtime_session_attributes,
        &session.state,
        &rules,
    );
    runtime
        .system_log_lines
        .extend(rule_evaluation.debug_lines.clone());
    runtime
        .system_messages
        .extend(
            rule_evaluation
                .system_messages
                .iter()
                .map(|content| ChatMessage {
                    role: "system".to_string(),
                    content: MessageContent::Text(content.clone()),
                    speaker: None,
                    metadata: Some(serde_json::json!({
                        "turn_index": turn_index,
                        "action_type": "rule_runtime_message",
                    })),
                }),
        );
    for update in &rule_evaluation.attribute_updates {
        if update.owner_type.trim() == "session_character" {
            if let Some(schema) = schema_map.get(update.schema_key.as_str()) {
                let character_id = update
                    .owner_id
                    .trim()
                    .split(':')
                    .next_back()
                    .unwrap_or_default()
                    .to_string();
                if !character_id.is_empty() {
                    runtime
                        .character_attribute_updates
                        .push(ParsedCharacterAttributeUpdate {
                            character_id,
                            schema_id: schema.id.clone(),
                            value: update.value.clone(),
                        });
                }
            }
        } else if let Some(schema) = schema_map.get(update.schema_key.as_str()) {
            runtime
                .session_attribute_updates
                .push(ParsedAttributeUpdate {
                    schema_id: schema.id.clone(),
                    value: update.value.clone(),
                });
        }
    }

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
    runtime_session_attributes = load_runtime_session_attributes(conn, &session.id, &schema_map)?;
    let state_result = state_engine.evaluate_turn(
        session,
        player_input,
        &director_decision,
        &trigger_evaluation,
        &rule_evaluation,
        &runtime_session_attributes,
    );
    runtime.state_metrics = state_result.state.metrics.clone();
    runtime.state_tags = state_result.state.tags.clone();
    runtime.state_phase = state_result.state.phase.clone();
    runtime
        .system_log_lines
        .extend(state_result.debug_lines.clone());
    runtime
        .system_messages
        .extend(
            state_result
                .system_messages
                .iter()
                .map(|content| ChatMessage {
                    role: "system".to_string(),
                    content: MessageContent::Text(content.clone()),
                    speaker: None,
                    metadata: Some(serde_json::json!({
                        "turn_index": turn_index,
                        "action_type": "state_runtime_message",
                    })),
                }),
        );

    runtime.memory_entries = parse_memory_entries(
        world,
        session,
        characters,
        turn_index,
        player_input,
        parsed.get("memory_events"),
    );
    runtime
        .memory_entries
        .extend(trigger_evaluation.memory_events.iter().flat_map(|event| {
            let targets = parse_memory_target_characters(
                Some(&serde_json::Value::Array(
                    event
                        .character_names
                        .iter()
                        .cloned()
                        .map(serde_json::Value::String)
                        .collect(),
                )),
                characters,
                session,
            );
            targets.into_iter().map(|character_id| {
                build_memory_entry(
                    world,
                    session,
                    turn_index,
                    &character_id,
                    "short_term",
                    event.content.as_str(),
                    event.source.as_str(),
                    event.importance,
                    event.memory_type.as_str(),
                    event.speaker.as_deref(),
                    event.role.as_deref(),
                    event
                        .location
                        .as_deref()
                        .or(Some(session.location.as_str())),
                    event
                        .scene_id
                        .as_deref()
                        .or(Some(session.scene.scene_id.as_str())),
                    if event.participants.is_empty() {
                        session.scene.present_characters.clone()
                    } else {
                        event.participants.clone()
                    },
                )
            })
        }));
    runtime
        .memory_entries
        .extend(rule_evaluation.memory_events.iter().flat_map(|event| {
            let targets = parse_memory_target_characters(
                Some(&serde_json::Value::Array(
                    event
                        .character_names
                        .iter()
                        .cloned()
                        .map(serde_json::Value::String)
                        .collect(),
                )),
                characters,
                session,
            );
            targets.into_iter().map(|character_id| {
                build_memory_entry(
                    world,
                    session,
                    turn_index,
                    &character_id,
                    "short_term",
                    event.content.as_str(),
                    event.source.as_str(),
                    event.importance,
                    event.memory_type.as_str(),
                    None,
                    Some("system"),
                    event
                        .location
                        .as_deref()
                        .or(Some(session.location.as_str())),
                    event
                        .scene_id
                        .as_deref()
                        .or(Some(session.scene.scene_id.as_str())),
                    if event.participants.is_empty() {
                        session.scene.present_characters.clone()
                    } else {
                        event.participants.clone()
                    },
                )
            })
        }));

    Ok(runtime)
}

pub(crate) fn apply_director_runtime_effects_with_preface(
    conn: &Connection,
    inventory_service: &crate::services::game_engine::inventory::InventoryService,
    trigger_engine: &crate::services::game_engine::trigger::TriggerEngineService,
    rule_engine: &crate::services::game_engine::rule::RuleEngineService,
    scene_manager: &crate::services::game_engine::scene::SceneManager,
    state_engine: &crate::services::game_engine::state::StateEngineService,
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
        inventory_service,
        trigger_engine,
        rule_engine,
        scene_manager,
        state_engine,
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
            value_type: schema.value_type.clone(),
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

fn parse_metrics_object(
    value: Option<&serde_json::Value>,
    fallback: &HashMap<String, f64>,
) -> HashMap<String, f64> {
    let Some(object) = value.and_then(|value| value.as_object()) else {
        return fallback.clone();
    };
    let mut merged = fallback.clone();
    for (key, value) in object {
        let Some(number) = value
            .as_f64()
            .or_else(|| value.as_i64().map(|item| item as f64))
        else {
            continue;
        };
        let key = key.trim();
        if !key.is_empty() {
            merged.insert(key.to_string(), number);
        }
    }
    merged
}

fn parse_optional_inventory_items(
    value: Option<&serde_json::Value>,
    fallback: &[InventoryItem],
) -> Option<Vec<InventoryItem>> {
    let Some(items) = value.and_then(|value| value.as_array()) else {
        return None;
    };
    let mut parsed = Vec::new();
    for item in items {
        let Some(object) = item.as_object() else {
            continue;
        };
        let name = object
            .get("name")
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let item_id = object
            .get("item_id")
            .or_else(|| object.get("id"))
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .or_else(|| {
                name.as_ref()
                    .map(|value| format!("item-{}", slugify_scene_id(value)))
            });
        let Some(name) = name else {
            continue;
        };
        let quantity = object
            .get("quantity")
            .and_then(|value| value.as_i64())
            .map(|value| value as i32)
            .unwrap_or(1)
            .max(0);
        if quantity <= 0 {
            continue;
        }
        parsed.push(InventoryItem {
            item_id: item_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            name,
            category: object
                .get("category")
                .and_then(|value| value.as_str())
                .unwrap_or("misc")
                .trim()
                .to_string(),
            quantity,
            description: object
                .get("description")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .trim()
                .to_string(),
            tags: parse_string_array(object.get("tags"), &[]),
            owner_type: object
                .get("owner_type")
                .and_then(|value| value.as_str())
                .unwrap_or("player")
                .trim()
                .to_string(),
            owner_id: object
                .get("owner_id")
                .and_then(|value| value.as_str())
                .unwrap_or("player")
                .trim()
                .to_string(),
            visibility: object
                .get("visibility")
                .and_then(|value| value.as_str())
                .unwrap_or("private")
                .trim()
                .to_string(),
            disclosed_to: parse_string_array(object.get("disclosed_to"), &[]),
        });
    }
    if parsed.is_empty() {
        Some(fallback.to_vec())
    } else {
        Some(parsed)
    }
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
        .filter_map(|item| serde_json::from_value::<PendingScheduledNotification>(item.clone()).ok())
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
                parse_memory_target_characters(item.get("character_names"), characters, session);
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

fn parse_memory_target_characters(
    value: Option<&serde_json::Value>,
    characters: &[CharacterDefinition],
    session: &SessionSnapshot,
) -> Vec<String> {
    let mut target_ids = Vec::new();
    for name in parse_string_array(value, &[]) {
        if let Some(character_id) = characters
            .iter()
            .find(|character| character.name == name)
            .map(|character| character.id.clone())
        {
            target_ids.push(character_id);
        }
    }
    if target_ids.is_empty() {
        let participant_names = session
            .scene
            .present_characters
            .iter()
            .chain(std::iter::once(&session.player_character_name))
            .cloned()
            .collect::<Vec<_>>();
        for name in participant_names {
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

fn slugify_scene_id(value: &str) -> String {
    let mut normalized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    while normalized.contains("--") {
        normalized = normalized.replace("--", "-");
    }
    let normalized = normalized.trim_matches('-').to_string();
    if normalized.is_empty() {
        "scene-switch".to_string()
    } else {
        normalized
    }
}
