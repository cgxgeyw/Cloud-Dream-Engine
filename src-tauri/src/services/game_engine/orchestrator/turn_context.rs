use crate::models::character::{resolve_character_narration_prompt, CharacterDefinition};
use crate::models::memory::MemoryEntry;
use crate::models::model_config::ModelConfig;
use crate::models::session::*;
use crate::models::settings::AppSettings;
use crate::models::world::WorldDefinition;
use crate::services::game_engine::dialogue::DialoguePipeline;
use crate::services::game_engine::memory::MemoryService;
use crate::services::game_engine::prompting::{build_prompt_call, llm_chat_messages_to_values};
use crate::services::game_engine::structured_output::StructuredOutputFailure;
use rusqlite::{params, Connection};
use std::collections::HashMap;

pub(crate) fn resolve_settings(conn: &Connection) -> Result<AppSettings, String> {
    let mut stmt = conn
        .prepare("SELECT text_model_provider, default_text_model, image_model_provider, default_image_workflow, embedding_enabled, default_embedding_model, home_background_strategy, export_directory FROM settings WHERE id = 1")
        .map_err(|e| e.to_string())?;
    stmt.query_row([], |row| {
        Ok(AppSettings {
            text_model_provider: row.get(0)?,
            default_text_model: row.get(1)?,
            image_model_provider: row.get(2)?,
            default_image_workflow: row.get(3)?,
            embedding_enabled: row.get::<_, i64>(4)? != 0,
            default_embedding_model: row.get(5)?,
            home_background_strategy: row.get(6)?,
            export_directory: row.get(7)?,
        })
    })
    .map_err(|e| e.to_string())
}

pub(crate) fn resolve_text_model(
    conn: &Connection,
    preferred_ref: Option<&str>,
) -> Result<ModelConfig, String> {
    let settings = resolve_settings(conn)?;
    let repo = crate::db::repositories::model_repo::ModelRepository::new(conn);
    let text_models = repo.list(Some("text"))?;

    let model = preferred_ref
        .into_iter()
        .chain(std::iter::once(settings.default_text_model.as_str()))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .find_map(|candidate| {
            text_models.iter().find(|model| {
                model.id.eq_ignore_ascii_case(candidate)
                    || model.model_id.eq_ignore_ascii_case(candidate)
                    || model.name.eq_ignore_ascii_case(candidate)
            })
        })
        .cloned()
        .or_else(|| text_models.iter().find(|model| model.is_default).cloned())
        .or_else(|| text_models.first().cloned())
        .ok_or_else(|| "No text model configured".to_string())?;

    if model.base_url.trim().is_empty() {
        return Err("Selected text model base_url is empty".to_string());
    }
    if model.model_id.trim().is_empty() {
        return Err("Selected text model model_id is empty".to_string());
    }
    Ok(model)
}

pub(crate) struct CharacterPromptArtifacts {
    pub system_prompt: String,
    pub narration_prompt: String,
    pub user_prompt: String,
    pub init_payload: String,
    pub turn_payload: String,
    pub response_contract: serde_json::Value,
    pub scene_state: serde_json::Value,
    pub visibility_context: serde_json::Value,
    pub modules: Vec<serde_json::Value>,
    pub messages: Vec<crate::services::llm::client::ChatMessage>,
}

#[derive(Debug, Clone)]

pub(crate) struct CharacterMemoryContext {
    pub hit_turns: Vec<i32>,
    pub matched_memories: Vec<MemoryEntry>,
    pub event_timeline: Vec<MemoryEntry>,
    pub dialogue_focus: Vec<ChatMessage>,
}

pub(crate) fn build_character_prompt_artifacts(
    dialogue_pipeline: &DialoguePipeline,
    world: &WorldDefinition,
    speaker_name: &str,
    speaker_profile: Option<&CharacterDefinition>,
    session: &SessionSnapshot,
    player_character_name: &str,
    location: &str,
    scene_name: &str,
    player_input: &str,
    recent_messages: &[ChatMessage],
    recalled_memories: &[MemoryEntry],
    memory_pool: &[MemoryEntry],
    visible_attribute_lines: &[String],
    visible_inventory_items: &[InventoryItem],
    visible_inventory_lines: &[String],
    public_scene_state_lines: &[String],
    next_scene_name: &str,
    next_location: &str,
    visible_characters: &[String],
) -> CharacterPromptArtifacts {
    let speaker_character_id = speaker_profile.map(|profile| profile.id.as_str());
    let visibility_context = build_character_visibility_context_payload(
        visible_attribute_lines,
        visible_inventory_items,
        visible_inventory_lines,
        public_scene_state_lines,
        speaker_character_id,
        speaker_name,
    );
    let narration_prompt = resolve_character_narration_prompt(
        speaker_profile.map(|profile| profile.narration_prompt.as_str()),
    );
    let mut system_prompt = dialogue_pipeline.build_character_system_prompt_with_contract(
        speaker_name,
        speaker_profile,
        None,
        None,
    );
    // 将世界背景信息加入角色系统提示词
    if !world.background_prompt.trim().is_empty() {
        system_prompt = format!(
            "【世界背景】\n{}\n\n{}",
            world.background_prompt.trim(),
            system_prompt
        );
    }
    let init_payload = build_character_init_payload(speaker_name, speaker_profile, session);
    let turn_payload = build_character_turn_payload(
        world,
        speaker_name,
        speaker_profile,
        session,
        player_character_name,
        location,
        scene_name,
        player_input,
        recent_messages,
        recalled_memories,
        memory_pool,
        visible_attribute_lines,
        visible_inventory_items,
        visible_inventory_lines,
        public_scene_state_lines,
    );
    let response_contract = serde_json::json!({
        "format": "json_object",
        "fields": ["speaker", "content", "intent", "emotion", "narration"],
    });
    let scene_state = serde_json::json!({
        "scene_name": next_scene_name,
        "location": next_location,
        "visible_characters": visible_characters,
        "player_character_name": player_character_name
    });
    let modules = vec![
        serde_json::json!({
            "name": "character_system_prompt",
            "source": "dialogue_pipeline.build_character_system_prompt_with_contract",
            "content": system_prompt.clone(),
            "editable": false,
            "sent": true
        }),
        serde_json::json!({
            "name": "character_narration_prompt",
            "source": "character.narration_prompt",
            "content": narration_prompt.clone(),
            "editable": true,
            "sent": !narration_prompt.trim().is_empty()
        }),
        serde_json::json!({
            "name": "init_payload",
            "source": "build_character_init_payload",
            "content": init_payload.clone(),
            "editable": false,
            "sent": true
        }),
        serde_json::json!({
            "name": "turn_payload",
            "source": "build_character_turn_payload",
            "content": turn_payload.clone(),
            "editable": false,
            "sent": true
        }),
        serde_json::json!({
            "name": "visibility_context",
            "source": "build_character_visibility_context_payload",
            "content": serde_json::to_string_pretty(&visibility_context).unwrap_or_else(|_| "{}".to_string()),
            "editable": false,
            "sent": true
        }),
        serde_json::json!({
            "name": "scene_state",
            "source": "character_runtime.scene_state",
            "content": serde_json::to_string_pretty(&scene_state).unwrap_or_else(|_| "{}".to_string()),
            "editable": false,
            "sent": true
        }),
        serde_json::json!({
            "name": "response_contract",
            "source": "character_runtime.response_contract",
            "content": serde_json::to_string_pretty(&response_contract).unwrap_or_else(|_| "{}".to_string()),
            "editable": false,
            "sent": true
        }),
    ];
    let messages = vec![
        crate::services::llm::client::ChatMessage {
            role: "system".to_string(),
            content: serde_json::Value::String(system_prompt.clone()),
            reasoning_content: None,
            speaker: None,
            tool_call_id: None,
            tool_calls: None,
            metadata: None,
        },
        crate::services::llm::client::ChatMessage {
            role: "system".to_string(),
            content: serde_json::Value::String(narration_prompt.clone()),
            reasoning_content: None,
            speaker: None,
            tool_call_id: None,
            tool_calls: None,
            metadata: None,
        },
        crate::services::llm::client::ChatMessage {
            role: "system".to_string(),
            content: serde_json::Value::String(init_payload.clone()),
            reasoning_content: None,
            speaker: None,
            tool_call_id: None,
            tool_calls: None,
            metadata: None,
        },
        crate::services::llm::client::ChatMessage {
            role: "user".to_string(),
            content: serde_json::Value::String(turn_payload.clone()),
            reasoning_content: None,
            speaker: Some(player_character_name.to_string()),
            tool_call_id: None,
            tool_calls: None,
            metadata: None,
        },
    ];

    CharacterPromptArtifacts {
        system_prompt,
        narration_prompt,
        user_prompt: turn_payload.clone(),
        init_payload,
        turn_payload,
        response_contract,
        scene_state,
        visibility_context,
        modules,
        messages,
    }
}

pub(crate) fn build_character_prompt_trace(
    dialogue_pipeline: &DialoguePipeline,
    world: &WorldDefinition,
    speaker_name: &str,
    speaker_profile: Option<&CharacterDefinition>,
    session: &SessionSnapshot,
    player_character_name: &str,
    location: &str,
    scene_name: &str,
    player_input: &str,
    recent_messages: &[ChatMessage],
    recalled_memories: &[MemoryEntry],
    memory_pool: &[MemoryEntry],
    visible_attribute_lines: &[String],
    visible_inventory_items: &[InventoryItem],
    visible_inventory_lines: &[String],
    public_scene_state_lines: &[String],
    next_scene_name: &str,
    next_location: &str,
    visible_characters: &[String],
    speaker_provider: &str,
    speaker_model: &ModelConfig,
    request_value: serde_json::Value,
    response_value: serde_json::Value,
    raw_model_return: String,
    processed_model_return: serde_json::Value,
    written_result: serde_json::Value,
) -> serde_json::Value {
    let artifacts = build_character_prompt_artifacts(
        dialogue_pipeline,
        world,
        speaker_name,
        speaker_profile,
        session,
        player_character_name,
        location,
        scene_name,
        player_input,
        recent_messages,
        recalled_memories,
        memory_pool,
        visible_attribute_lines,
        visible_inventory_items,
        visible_inventory_lines,
        public_scene_state_lines,
        next_scene_name,
        next_location,
        visible_characters,
    );
    let prompt_call = build_prompt_call(
        "prompt_call_v2",
        "character",
        speaker_name,
        "character_response",
        "Respond to the player's input as the selected character",
        &artifacts.system_prompt,
        &artifacts.user_prompt,
        llm_chat_messages_to_values(&artifacts.messages),
        artifacts.modules,
        artifacts.response_contract,
        serde_json::json!({
            "provider": speaker_provider,
            "base_url": speaker_model.base_url,
            "model_id": speaker_model.model_id,
            "request": request_value,
            "response": response_value,
            "narration_prompt": artifacts.narration_prompt,
            "init_payload": artifacts.init_payload,
            "turn_payload": artifacts.turn_payload,
            "scene_state": artifacts.scene_state,
            "visibility_context": artifacts.visibility_context,
        }),
    );
    let mut prompt_call = prompt_call;
    if let Some(object) = prompt_call.as_object_mut() {
        object.insert(
            "raw_model_return".to_string(),
            if raw_model_return.trim().is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::Value::String(raw_model_return)
            },
        );
        object.insert("processed_model_return".to_string(), processed_model_return);
        object.insert("written_result".to_string(), written_result);
    }
    prompt_call
}

pub(crate) fn build_character_chat_request(
    dialogue_pipeline: &DialoguePipeline,
    world: &WorldDefinition,
    model: &ModelConfig,
    speaker_name: &str,
    speaker_profile: Option<&CharacterDefinition>,
    session: &SessionSnapshot,
    player_character_name: &str,
    location: &str,
    scene_name: &str,
    player_input: &str,
    recent_messages: &[ChatMessage],
    recalled_memories: &[MemoryEntry],
    memory_pool: &[MemoryEntry],
    visible_attribute_lines: &[String],
    visible_inventory_items: &[InventoryItem],
    visible_inventory_lines: &[String],
    public_scene_state_lines: &[String],
) -> crate::services::llm::client::ChatRequest {
    let artifacts = build_character_prompt_artifacts(
        dialogue_pipeline,
        world,
        speaker_name,
        speaker_profile,
        session,
        player_character_name,
        location,
        scene_name,
        player_input,
        recent_messages,
        recalled_memories,
        memory_pool,
        visible_attribute_lines,
        visible_inventory_items,
        visible_inventory_lines,
        public_scene_state_lines,
        scene_name,
        location,
        &session.visible_characters,
    );
    crate::services::llm::client::ChatRequest {
        model: model.model_id.to_string(),
        messages: artifacts.messages,
        temperature: Some(0.8),
        max_tokens: Some(model.max_tokens),
        stream: Some(false),
        json_mode: Some(true),
        response_schema: Some(build_character_response_schema()),
        tools: None,
        tool_choice: None,
        native_tool_calling: None,
    }
}

pub(crate) fn build_character_response_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": ["speaker", "content", "intent", "emotion", "narration"],
        "additionalProperties": true,
        "properties": {
            "speaker": { "type": "string" },
            "content": { "type": "string" },
            "intent": { "type": "string" },
            "emotion": { "type": "string" },
            "narration": { "type": "string" }
        }
    })
}

pub(crate) fn build_director_transport_failure(
    provider: &str,
    model: &ModelConfig,
    turn_index: i32,
    raw_text: &str,
    error: &str,
) -> StructuredOutputFailure {
    StructuredOutputFailure {
        stage:
            crate::services::game_engine::structured_output::StructuredFailureStage::DirectorMain,
        failure_code: "provider_payload_missing".to_string(),
        summary: "导演请求失败，未获得可用结构化输出".to_string(),
        provider: provider.to_string(),
        model_id: model.model_id.clone(),
        turn_index,
        speaker_name: None,
        raw_text_excerpt: if raw_text.trim().is_empty() {
            error.to_string()
        } else {
            raw_text.trim().chars().take(280).collect()
        },
        repair_summary: Some(error.to_string()),
        schema_errors: Vec::new(),
        domain_errors: vec![error.to_string()],
    }
}

pub(crate) fn build_character_init_payload(
    speaker_name: &str,
    speaker_profile: Option<&CharacterDefinition>,
    session: &SessionSnapshot,
) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "basic_setting": {
            "character": {
                "name": speaker_name,
                "role": speaker_profile.map(|profile| profile.role.clone()).unwrap_or_default(),
                "attributes": speaker_profile.map(|profile| profile.attributes.clone()).unwrap_or_default(),
                "custom_tabs": speaker_profile.map(|profile| profile.custom_tabs.clone()).unwrap_or_default(),
            },
            "world": {
                "world_name": session.world_name,
            }
        }
    }))
    .unwrap_or_else(|_| "{}".to_string())
}

pub(crate) fn build_character_turn_payload(
    world: &WorldDefinition,
    speaker_name: &str,
    speaker_profile: Option<&CharacterDefinition>,
    session: &SessionSnapshot,
    player_name: &str,
    location: &str,
    scene_name: &str,
    player_input: &str,
    recent_messages: &[ChatMessage],
    recalled_memories: &[MemoryEntry],
    memory_pool: &[MemoryEntry],
    visible_attribute_lines: &[String],
    visible_inventory_items: &[InventoryItem],
    visible_inventory_lines: &[String],
    public_scene_state_lines: &[String],
) -> String {
    let speaker_character_id = speaker_profile.map(|profile| profile.id.as_str());
    let visibility_context = build_character_visibility_context_payload(
        visible_attribute_lines,
        visible_inventory_items,
        visible_inventory_lines,
        public_scene_state_lines,
        speaker_character_id,
        speaker_name,
    );
    let memory_context =
        build_character_memory_context(world, session, recalled_memories, memory_pool);
    serde_json::to_string_pretty(&serde_json::json!({
        "dialogue_history": {
            "recent_dialogue": build_recent_dialogue_payload(recent_messages, player_name),
            "memory_context": build_memory_context_payload(&memory_context, player_name),
        },
        "current_state": {
            "requested_speaker": speaker_name,
            "player_character_name": player_name,
            "player_input": player_input,
            "scene_state": build_character_scene_state_payload(
                session,
                location,
                scene_name,
                visible_attribute_lines,
                visible_inventory_items,
                speaker_character_id,
                speaker_name,
            ),
        },
        "visibility_context": visibility_context
    }))
    .unwrap_or_else(|_| "{}".to_string())
}

pub(crate) fn build_recent_dialogue_payload(
    recent_messages: &[ChatMessage],
    player_character_name: &str,
) -> Vec<serde_json::Value> {
    recent_messages
        .iter()
        .map(|message| {
            serde_json::json!({
                "role": message.role,
                "speaker": resolve_history_speaker(message, Some(player_character_name)),
                "content": message.content,
            })
        })
        .collect()
}

pub(crate) fn build_memory_entry_payload(memories: &[MemoryEntry]) -> Vec<serde_json::Value> {
    memories
        .iter()
        .map(|memory| {
            serde_json::json!({
                "id": memory.id,
                "layer": memory.layer,
                "memory_type": memory.memory_type,
                "source": memory.source,
                "importance": memory.importance,
                "created_at": memory.created_at,
                "conversation_id": memory.conversation_id,
                "event_id": memory.event_id,
                "item_id": memory.item_id,
                "speaker": memory.speaker,
                "role": memory.role,
                "content": memory.content,
                "turn_index": memory.turn_index,
                "location": memory.location,
                "scene_id": memory.scene_id,
                "participants": memory.participants,
                "keywords": memory.keywords,
            })
        })
        .collect()
}

pub(crate) fn build_memory_context_payload(
    memory_context: &CharacterMemoryContext,
    player_character_name: &str,
) -> serde_json::Value {
    serde_json::json!({
        "hit_turns": memory_context.hit_turns,
        "matched_memories": build_memory_entry_payload(&memory_context.matched_memories),
        "event_timeline": build_memory_entry_payload(&memory_context.event_timeline),
        "dialogue_focus": build_recent_dialogue_payload(
            &memory_context.dialogue_focus,
            player_character_name,
        ),
    })
}

pub(crate) fn build_character_memory_context(
    world: &WorldDefinition,
    session: &SessionSnapshot,
    recalled_memories: &[MemoryEntry],
    memory_pool: &[MemoryEntry],
) -> CharacterMemoryContext {
    let hit_turn_limit = resolve_character_memory_hit_turns(world);
    let event_window_rounds = resolve_character_memory_event_window_rounds(world);
    let dialogue_window_rounds = resolve_character_memory_dialogue_window_rounds(world);

    let mut hit_turns = Vec::new();
    let mut matched_memories = Vec::new();
    for memory in recalled_memories {
        if memory.turn_index <= 0 || hit_turns.contains(&memory.turn_index) {
            continue;
        }
        hit_turns.push(memory.turn_index);
        matched_memories.push(memory.clone());
        if hit_turns.len() >= hit_turn_limit {
            break;
        }
    }

    let event_timeline =
        collect_windowed_event_memories(memory_pool, &hit_turns, event_window_rounds);
    let dialogue_focus =
        collect_windowed_dialogue_messages(&session.messages, &hit_turns, dialogue_window_rounds);

    CharacterMemoryContext {
        hit_turns,
        matched_memories,
        event_timeline,
        dialogue_focus,
    }
}

pub(crate) fn collect_windowed_event_memories(
    memory_pool: &[MemoryEntry],
    hit_turns: &[i32],
    window_rounds: i32,
) -> Vec<MemoryEntry> {
    if hit_turns.is_empty() {
        return Vec::new();
    }

    let mut deduped = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for memory in memory_pool {
        if memory.turn_index <= 0
            || !memory_matches_turn_windows(memory.turn_index, hit_turns, window_rounds)
            || !is_event_memory(memory)
        {
            continue;
        }
        let dedupe_key = memory.event_id.clone().unwrap_or_else(|| {
            format!(
                "{}|{}|{}|{}|{}",
                memory.turn_index,
                memory.source,
                memory.speaker.as_deref().unwrap_or_default(),
                memory.role.as_deref().unwrap_or_default(),
                memory.content.trim()
            )
        });
        if seen.insert(dedupe_key) {
            deduped.push(memory.clone());
        }
    }
    deduped.sort_by(|left, right| {
        left.turn_index
            .cmp(&right.turn_index)
            .then_with(|| left.created_at.cmp(&right.created_at))
            .then_with(|| left.content.cmp(&right.content))
    });
    deduped
}

pub(crate) fn collect_windowed_dialogue_messages(
    messages: &[ChatMessage],
    hit_turns: &[i32],
    window_rounds: i32,
) -> Vec<ChatMessage> {
    if hit_turns.is_empty() {
        return Vec::new();
    }

    messages
        .iter()
        .filter(|message| matches!(message.role.as_str(), "player" | "agent"))
        .filter_map(|message| {
            let turn_index = extract_message_turn_index(message)?;
            if !memory_matches_turn_windows(turn_index, hit_turns, window_rounds) {
                return None;
            }
            Some(message.clone())
        })
        .collect()
}

pub(crate) fn memory_matches_turn_windows(
    turn_index: i32,
    hit_turns: &[i32],
    window_rounds: i32,
) -> bool {
    hit_turns
        .iter()
        .any(|hit_turn| (turn_index - *hit_turn).abs() <= window_rounds)
}

pub(crate) fn is_event_memory(memory: &MemoryEntry) -> bool {
    memory.memory_type != "dialogue"
        || memory.layer == "canonical_event"
        || !matches!(memory.source.as_str(), "player_action" | "speaker_response")
}

pub(crate) fn extract_message_turn_index(message: &ChatMessage) -> Option<i32> {
    message
        .metadata
        .as_ref()
        .and_then(|meta| meta.get("turn_index"))
        .and_then(|value| value.as_i64())
        .map(|value| value as i32)
}

pub(crate) fn build_character_visibility_context_payload(
    visible_attribute_lines: &[String],
    visible_inventory_items: &[InventoryItem],
    visible_inventory_lines: &[String],
    public_scene_state_lines: &[String],
    character_id: Option<&str>,
    character_name: &str,
) -> serde_json::Value {
    serde_json::json!({
        "public_scene_state_lines": public_scene_state_lines,
        "visible_attribute_lines": visible_attribute_lines,
        "visible_inventory_lines": visible_inventory_lines,
        "visible_attribute_records": build_visible_attribute_records(visible_attribute_lines),
        "visible_inventory_records": build_visible_inventory_records(
            visible_inventory_items,
            character_id,
            character_name,
        ),
    })
}

pub(crate) fn build_character_scene_state_payload(
    session: &SessionSnapshot,
    location: &str,
    scene_name: &str,
    visible_attribute_lines: &[String],
    visible_inventory_items: &[InventoryItem],
    character_id: Option<&str>,
    character_name: &str,
) -> serde_json::Value {
    serde_json::json!({
        "world_name": session.world_name,
        "location": location,
        "time_label": session.time_label,
        "scene_name": scene_name,
        "scene_id": session.scene.scene_id,
        "scene_tags": session.scene.temporary_tags,
        "state_phase": session.state.phase,
        "state_tags": session.state.tags,
        "state_metrics": session.state.metrics,
        "present_characters": collect_memory_participants(session),
        "discovered_locations": session
            .map_graph_nodes
            .iter()
            .filter(|node| node.discovered && !node.label.trim().is_empty())
            .map(|node| node.label.clone())
            .collect::<Vec<_>>(),
        "visible_attributes": build_visible_attribute_records(visible_attribute_lines),
        "visible_inventory": build_visible_inventory_records(
            visible_inventory_items,
            character_id,
            character_name,
        ),
        "public_attributes": build_visible_attribute_records(visible_attribute_lines)
            .into_iter()
            .filter(|item| item.get("owner_relation").and_then(|value| value.as_str()) == Some("public"))
            .collect::<Vec<_>>(),
        "public_items": build_visible_inventory_records(
            visible_inventory_items,
            character_id,
            character_name,
        )
        .into_iter()
        .filter(|item| item.get("knowledge_scope").and_then(|value| value.as_str()) == Some("public"))
        .collect::<Vec<_>>(),
    })
}

pub(crate) fn build_visible_attribute_records(
    visible_attribute_lines: &[String],
) -> Vec<serde_json::Value> {
    visible_attribute_lines
        .iter()
        .filter_map(|line| parse_attribute_trace_line(line))
        .collect()
}

pub(crate) fn build_visible_inventory_records(
    visible_inventory_items: &[InventoryItem],
    character_id: Option<&str>,
    character_name: &str,
) -> Vec<serde_json::Value> {
    let normalized_character_id = character_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let normalized_character_name = character_name.trim().to_string();
    visible_inventory_items
        .iter()
        .map(|item| {
            let knowledge_scope = if item.owner_type == "character"
                && normalized_character_id
                    .as_ref()
                    .map(|value| value == &item.owner_id)
                    .unwrap_or(false)
            {
                "owned"
            } else if item.disclosed_to.iter().any(|value| {
                value == &normalized_character_name
                    || normalized_character_id
                        .as_ref()
                        .map(|character_id| value == character_id)
                        .unwrap_or(false)
            }) {
                "disclosed"
            } else {
                "public"
            };
            serde_json::json!({
                "item_id": item.item_id,
                "name": item.name,
                "category": item.category,
                "quantity": item.quantity,
                "description": item.description,
                "tags": item.tags,
                "owner_type": item.owner_type,
                "owner_id": item.owner_id,
                "visibility": item.visibility,
                "knowledge_scope": knowledge_scope,
                "disclosed_to": item.disclosed_to,
            })
        })
        .collect()
}

pub(crate) fn parse_attribute_trace_line(line: &str) -> Option<serde_json::Value> {
    let trimmed = line.trim();
    let (prefix_end, prefix_close) = (trimmed.find('[')?, trimmed.find(']')?);
    if prefix_end != 0 || prefix_close <= 1 {
        return None;
    }
    let relation = trimmed[1..prefix_close].trim();
    let rest = trimmed[prefix_close + 1..].trim();
    let (key, value) = rest.split_once(" = ")?;
    Some(serde_json::json!({
        "owner_relation": relation,
        "key": key.trim(),
        "value": value.trim(),
    }))
}

pub(crate) fn collect_memory_participants(session: &SessionSnapshot) -> Vec<String> {
    let mut participants = vec![session.player_character_name.clone()];
    participants.extend(session.visible_characters.iter().cloned());
    participants
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .fold(Vec::new(), |mut acc, item| {
            if !acc.contains(&item) {
                acc.push(item);
            }
            acc
        })
}

pub(crate) fn build_public_scene_state_lines(
    session: &SessionSnapshot,
    visible_attribute_lines: &[String],
    visible_inventory_items: &[InventoryItem],
) -> Vec<String> {
    let mut lines = vec![
        format!("world={}", session.world_name),
        format!("location={}", session.location),
        format!("time={}", session.time_label),
        format!("scene={}", session.scene.name),
        format!(
            "scene_tags={}",
            if session.scene.temporary_tags.is_empty() {
                "N/A".to_string()
            } else {
                session.scene.temporary_tags.join(" / ")
            }
        ),
        format!(
            "present_characters={}",
            collect_memory_participants(session).join(" / ")
        ),
    ];
    let discovered = session
        .map_graph_nodes
        .iter()
        .filter(|node| node.discovered && !node.label.trim().is_empty())
        .map(|node| node.label.clone())
        .collect::<Vec<_>>();
    if !discovered.is_empty() {
        lines.push(format!("discovered_locations={}", discovered.join(" / ")));
    }
    lines.extend(
        visible_attribute_lines
            .iter()
            .filter(|line| line.starts_with("[public]"))
            .cloned(),
    );
    lines.extend(
        visible_inventory_items
            .iter()
            .filter(|item| item.visibility == "public")
            .map(|item| {
                if item.quantity > 1 {
                    format!("public_item={} x{}", item.name, item.quantity)
                } else {
                    format!("public_item={}", item.name)
                }
            }),
    );
    lines
}

pub(crate) fn summarize_json_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(boolean) => boolean.to_string(),
        serde_json::Value::Number(number) => number.to_string(),
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Array(items) => items
            .iter()
            .map(summarize_json_value)
            .collect::<Vec<_>>()
            .join(" / "),
        serde_json::Value::Object(_) => serde_json::to_string(value).unwrap_or_default(),
    }
}

pub(crate) fn load_character_visible_attribute_lines(
    conn: &Connection,
    session: &SessionSnapshot,
    character_id: Option<&str>,
) -> Result<Vec<String>, String> {
    let Some(character_id) = character_id.filter(|value| !value.trim().is_empty()) else {
        return Ok(Vec::new());
    };
    let attribute_repo = crate::db::repositories::attribute_repo::AttributeRepository::new(conn);
    let schema_map = attribute_repo
        .list_schemas(None)?
        .into_iter()
        .map(|schema| (schema.id.clone(), schema))
        .collect::<HashMap<_, _>>();
    let mut values = attribute_repo.list_values(Some("session"), Some(&session.id), None)?;
    values.extend(
        attribute_repo
            .list_values(Some("session_character"), None, None)?
            .into_iter()
            .filter(|value| value.owner_id.starts_with(&(session.id.clone() + ":"))),
    );

    let mut lines = Vec::new();
    for value in values {
        let Some(schema) = schema_map.get(&value.schema_id) else {
            continue;
        };
        if value.owner_type == "session" {
            if !schema
                .access_policy
                .get("agent_self_read")
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
            {
                continue;
            }
            lines.push(format!(
                "[public] {} = {}",
                schema.key,
                summarize_json_value(&value.value)
            ));
            continue;
        }

        let Some((_, owner_character_id)) = value.owner_id.split_once(':') else {
            continue;
        };
        let can_view = if owner_character_id == character_id {
            schema
                .access_policy
                .get("agent_self_read")
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
        } else {
            schema
                .access_policy
                .get("agent_other_read")
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
        };
        if !can_view {
            continue;
        }
        let relation = if owner_character_id == character_id {
            "self"
        } else {
            "other"
        };
        lines.push(format!(
            "[{}] {} = {}",
            relation,
            schema.key,
            summarize_json_value(&value.value)
        ));
    }
    lines.sort();
    Ok(lines)
}

pub(crate) fn filter_inventory_for_character(
    session: &SessionSnapshot,
    character_id: Option<&str>,
    character_name: &str,
) -> Vec<InventoryItem> {
    let character_name = character_name.trim().to_string();
    let character_id = character_id.map(|value| value.trim().to_string());
    session
        .inventory_items
        .iter()
        .filter(|item| {
            item.visibility == "public"
                || item.disclosed_to.iter().any(|value| {
                    value == &character_name
                        || character_id
                            .as_ref()
                            .map(|character_id| value == character_id)
                            .unwrap_or(false)
                })
                || character_id
                    .as_ref()
                    .map(|character_id| {
                        item.owner_type == "character" && item.owner_id == *character_id
                    })
                    .unwrap_or(false)
        })
        .cloned()
        .collect()
}

pub(crate) fn summarize_visible_inventory_lines(
    items: &[InventoryItem],
    character_id: Option<&str>,
) -> Vec<String> {
    items
        .iter()
        .map(|item| {
            let knowledge_scope = if item.visibility == "public" {
                "public"
            } else if character_id
                .map(|value| item.owner_type == "character" && item.owner_id == value)
                .unwrap_or(false)
            {
                "owned"
            } else {
                "shared"
            };
            let detail = if item.quantity > 1 {
                format!("{} x{}", item.name, item.quantity)
            } else {
                item.name.clone()
            };
            if item.description.trim().is_empty() {
                format!("[{}] {}", knowledge_scope, detail)
            } else {
                format!(
                    "[{}] {} ({})",
                    knowledge_scope,
                    detail,
                    item.description.trim()
                )
            }
        })
        .collect()
}

pub(crate) fn slice_character_history(
    messages: &[ChatMessage],
    previous_rounds: i32,
    current_player_name: Option<&str>,
) -> Vec<ChatMessage> {
    let max_previous_rounds = previous_rounds.max(0);
    let dialogue_messages = annotate_player_message_speakers(messages, current_player_name)
        .into_iter()
        .filter(|message| {
            !message.content.trim().is_empty()
                && (message.role == "player" || message.role == "agent")
        })
        .collect::<Vec<_>>();
    if dialogue_messages.is_empty() {
        return Vec::new();
    }

    let mut selected = Vec::new();
    let mut player_messages_seen = 0;
    for message in dialogue_messages.into_iter().rev() {
        let is_player =
            message.role == "player" || is_player_message(&message, current_player_name);
        selected.push(message);
        if is_player {
            player_messages_seen += 1;
            if player_messages_seen > max_previous_rounds {
                break;
            }
        }
    }
    selected.reverse();
    selected
}

pub(crate) fn annotate_player_message_speakers(
    messages: &[ChatMessage],
    current_player_name: Option<&str>,
) -> Vec<ChatMessage> {
    let has_switch_marker = messages.iter().any(|message| {
        message.role == "system" && message.content.trim().contains("player switch")
    });
    let mut resolved_player_speaker = if let Some(player_name) = current_player_name {
        if has_switch_marker {
            "player".to_string()
        } else {
            resolved_player_speaker(Some(player_name))
        }
    } else {
        "player".to_string()
    };

    let mut annotated = Vec::with_capacity(messages.len());
    for message in messages {
        if message.role == "system" {
            if let Some(speaker_name) = extract_player_view_switch_speaker(message.content.as_str()) {
                resolved_player_speaker = speaker_name;
            }
            annotated.push(message.clone());
            continue;
        }
        if message.role == "player" {
            let raw_speaker = message
                .speaker
                .as_deref()
                .map(|speaker| speaker.trim().to_string())
                .filter(|speaker| !speaker.is_empty() && speaker != "player");
            annotated.push(ChatMessage {
                role: message.role.clone(),
                content: message.content.clone(),
                speaker: Some(raw_speaker.unwrap_or_else(|| resolved_player_speaker.clone())),
                metadata: message.metadata.clone(),
            });
            continue;
        }
        annotated.push(message.clone());
    }
    annotated
}

pub(crate) fn extract_player_view_switch_speaker(content: &str) -> Option<String> {
    let trimmed = content.trim();
    let marker = "player switch";
    trimmed
        .find(marker)
        .map(|index| trimmed[..index].trim().to_string())
        .filter(|speaker| !speaker.is_empty())
}

pub(crate) fn resolved_player_speaker(player_character_name: Option<&str>) -> String {
    player_character_name
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "player".to_string())
}

pub(crate) fn is_player_message(message: &ChatMessage, current_player_name: Option<&str>) -> bool {
    current_player_name
        .map(|player_name| {
            message
                .speaker
                .as_deref()
                .map(|speaker| speaker.trim() == player_name.trim())
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

pub(crate) fn resolve_history_speaker(
    message: &ChatMessage,
    current_player_name: Option<&str>,
) -> String {
    if message.role == "player" || is_player_message(message, current_player_name) {
        return resolved_player_speaker(current_player_name);
    }
    message
        .speaker
        .as_deref()
        .map(|speaker| speaker.trim().to_string())
        .filter(|speaker| !speaker.is_empty())
        .unwrap_or_else(|| message.role.clone())
}

pub(crate) fn resolve_character_memory_recall_limit(
    speaker_profile: Option<&CharacterDefinition>,
) -> i32 {
    speaker_profile
        .and_then(|profile| profile.custom_tabs.get("memory_recall_limit"))
        .and_then(|value| value.trim().parse::<i32>().ok())
        .map(|value| value.clamp(1, 24))
        .unwrap_or(8)
}

pub(crate) fn resolve_character_memory_hit_turns(world: &WorldDefinition) -> usize {
    world
        .director_config
        .get("character_memory_hit_turns")
        .and_then(|value| value.as_i64())
        .map(|value| value.clamp(1, 6) as usize)
        .unwrap_or(2)
}

pub(crate) fn resolve_character_memory_event_window_rounds(world: &WorldDefinition) -> i32 {
    world
        .director_config
        .get("character_memory_event_window_rounds")
        .and_then(|value| value.as_i64())
        .map(|value| value.clamp(0, 20) as i32)
        .unwrap_or(10)
}

pub(crate) fn resolve_character_memory_dialogue_window_rounds(world: &WorldDefinition) -> i32 {
    world
        .director_config
        .get("character_memory_dialogue_window_rounds")
        .and_then(|value| value.as_i64())
        .map(|value| value.clamp(0, 6) as i32)
        .unwrap_or(2)
}

pub(crate) fn load_character_memory_pool(
    conn: &Connection,
    world_id: &str,
    session_id: &str,
    character_id: Option<&str>,
) -> Result<Vec<MemoryEntry>, String> {
    let Some(character_id) = character_id
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    else {
        return Ok(Vec::new());
    };
    let repo = crate::db::repositories::memory_repo::MemoryRepository::new(conn);
    repo.list(&crate::models::memory::MemoryQueryParams {
        world_id: Some(world_id.to_string()),
        session_id: Some(session_id.to_string()),
        character_id: Some(character_id.to_string()),
        layer: None,
        limit: Some(200),
    })
}

pub(crate) fn load_recent_character_memories(
    memory_service: &MemoryService,
    conn: &Connection,
    world: &WorldDefinition,
    world_id: &str,
    session_id: &str,
    character_id: Option<&str>,
    query_text: &str,
    location: &str,
    scene_id: Option<String>,
    participants: &[String],
    limit: i32,
) -> Result<Vec<MemoryEntry>, String> {
    memory_service.recall_entries_for_character(
        conn,
        world,
        world_id,
        session_id,
        character_id,
        query_text,
        location,
        scene_id.as_deref(),
        participants,
        limit,
    )
}

pub(crate) fn resolve_default_image_model(
    conn: &Connection,
    settings: &AppSettings,
) -> Result<Option<ModelConfig>, String> {
    let repo = crate::db::repositories::model_repo::ModelRepository::new(conn);
    let models = repo.list(Some("image"))?;
    if models.is_empty() {
        return Ok(None);
    }
    if !settings.default_image_workflow.trim().is_empty() {
        if let Some(model) = models
            .iter()
            .find(|model| {
                model.id == settings.default_image_workflow
                    || model.model_id == settings.default_image_workflow
                    || model.name == settings.default_image_workflow
            })
            .cloned()
        {
            return Ok(Some(model));
        }
    }
    if let Some(model) = models.iter().find(|item| item.is_default).cloned() {
        return Ok(Some(model));
    }
    if let Some(model) = models
        .iter()
        .find(|model| model.provider.trim() == settings.image_model_provider.trim())
        .cloned()
    {
        return Ok(Some(model));
    }
    Ok(models.into_iter().next())
}

pub(crate) fn next_turn_index(conn: &Connection, session_id: &str) -> Result<i32, String> {
    let mut stmt = conn
        .prepare("SELECT COALESCE(MAX(turn_index), 0) FROM turn_journal WHERE session_id = ?1")
        .map_err(|e| e.to_string())?;
    let max_turn: i32 = stmt
        .query_row(params![session_id], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    Ok(max_turn + 1)
}

pub(crate) fn resolve_world_for_session(
    conn: &Connection,
    session: &SessionSnapshot,
) -> Result<WorldDefinition, String> {
    let world_repo = crate::db::repositories::world_repo::WorldRepository::new(conn);
    world_repo
        .list()?
        .into_iter()
        .find(|world| world.name == session.world_name)
        .ok_or_else(|| "World not found".to_string())
}
