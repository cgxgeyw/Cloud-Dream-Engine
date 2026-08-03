use crate::models::character::{resolve_character_narration_prompt, CharacterDefinition};
use crate::models::generation_params::GenerationParams;
use crate::models::memory::MemoryEntry;
use crate::models::model_config::ModelConfig;
use crate::models::session::*;
use crate::models::world::WorldDefinition;
use crate::services::game_engine::dialogue::DialoguePipeline;
use crate::services::game_engine::prompting::{
    build_prompt_call, llm_chat_messages_to_values, recent_messages_text, render_prompt_variables,
    resolve_prompt_modules, resolve_runtime_context_prompt,
};

use super::turn_context::{
    build_character_turn_payload, build_character_visibility_context_payload,
};

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
    public_scene_state_lines: &[String],
    next_scene_name: &str,
    next_location: &str,
    visible_characters: &[String],
    kv_vars: &std::collections::HashMap<String, String>,
    player_media: &[ContentPart],
) -> CharacterPromptArtifacts {
    let speaker_character_id = speaker_profile.map(|profile| profile.id.as_str());
    let visibility_context = build_character_visibility_context_payload(
        visible_attribute_lines,
        visible_inventory_items,
        public_scene_state_lines,
        speaker_character_id,
        speaker_name,
    );
    let narration_prompt = render_prompt_variables(&resolve_character_narration_prompt(
        speaker_profile.map(|profile| profile.narration_prompt.as_str()),
    ));
    let system_prompt = dialogue_pipeline.build_character_system_prompt_with_contract(
        speaker_name,
        speaker_profile,
        None,
        None,
        crate::services::game_engine::memory::resolve_fact_extraction_enabled(world),
    );
    let runtime_context_prompt = resolve_runtime_context_prompt(world);
    let character_runtime_context_prompt = speaker_profile
        .map(|profile| render_prompt_variables(&profile.runtime_system_prompt))
        .unwrap_or_default()
        .trim()
        .to_string();
    let preset_variables = {
        let mut vars = std::collections::HashMap::new();
        vars.insert("user".to_string(), player_character_name.trim().to_string());
        vars.insert("char".to_string(), speaker_name.to_string());
        vars.insert("world".to_string(), world.name.clone());
        vars.insert(
            "scene".to_string(),
            if scene_name.trim().is_empty() {
                location.to_string()
            } else {
                scene_name.to_string()
            },
        );
        vars.insert("time".to_string(), session.time_label.clone());
        vars
    };
    let module_resolution = resolve_prompt_modules(
        world,
        "character",
        &preset_variables,
        kv_vars,
        &recent_messages_text(&session.messages, 10),
    );
    let init_payload = build_character_init_payload(world, speaker_name, speaker_profile, session);
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
        public_scene_state_lines,
    );
    let response_fields = vec![
        "speaker",
        "content",
        "narration",
        "session_attribute_updates",
        "character_attribute_updates",
        "memory_entries",
        "fact_extractions",
    ];
    let response_contract = serde_json::json!({
        "format": "json_object",
        "fields": response_fields,
        "runtime_update_format": {
            "session_attribute_updates": [
                { "key": "attribute_key", "value": "new_value" }
            ],
            "character_attribute_updates": [
                { "character_name": "target_character_name", "key": "attribute_key", "value": "new_value" }
            ],
            "memory_entries": [
                { "content": "memory text", "character_names": ["target_character_name"] }
            ],
            "fact_extractions": [
                { "subject": "entity", "predicate": "relation", "object": "entity or value", "action": "upsert | invalidate" }
            ]
        },
        "tool_policy": "Use provider-native tool_calls for every allowed tool; never include tool_calls, tool names, or tool arguments in the JSON body."
    });
    let scene_state = serde_json::json!({
        "scene_name": next_scene_name,
        "location": next_location,
        "visible_characters": visible_characters,
        "player_character_name": player_character_name
    });
    let mut modules = vec![
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
    if !runtime_context_prompt.trim().is_empty() {
        modules.insert(
            1,
            serde_json::json!({
                "name": "runtime_context",
                "source": "world.director_config.runtime_context_prompt",
                "content": runtime_context_prompt.clone(),
                "editable": true,
                "sent": true
            }),
        );
    }
    if !character_runtime_context_prompt.trim().is_empty() {
        modules.insert(
            1,
            serde_json::json!({
                "name": "character_runtime_context",
                "source": "character.runtime_system_prompt",
                "content": character_runtime_context_prompt.clone(),
                "editable": true,
                "sent": true
            }),
        );
    }
    for (offset, trace) in module_resolution.traces.iter().enumerate() {
        modules.insert(1 + offset, trace.clone());
    }
    let mut messages: Vec<crate::services::llm::client::ChatMessage> = module_resolution
        .prefix
        .iter()
        .map(|content| crate::services::llm::client::ChatMessage {
            role: "system".to_string(),
            content: serde_json::Value::String(content.clone()),
            reasoning_content: None,
            speaker: None,
            tool_call_id: None,
            tool_calls: None,
            metadata: None,
        })
        .collect();
    messages.push(crate::services::llm::client::ChatMessage {
        role: "system".to_string(),
        content: serde_json::Value::String(system_prompt.clone()),
        reasoning_content: None,
        speaker: None,
        tool_call_id: None,
        tool_calls: None,
        metadata: None,
    });
    for content in &module_resolution.suffix {
        messages.push(crate::services::llm::client::ChatMessage {
            role: "system".to_string(),
            content: serde_json::Value::String(content.clone()),
            reasoning_content: None,
            speaker: None,
            tool_call_id: None,
            tool_calls: None,
            metadata: None,
        });
    }
    if !character_runtime_context_prompt.trim().is_empty() {
        messages.push(crate::services::llm::client::ChatMessage {
            role: "system".to_string(),
            content: serde_json::Value::String(character_runtime_context_prompt.clone()),
            reasoning_content: None,
            speaker: None,
            tool_call_id: None,
            tool_calls: None,
            metadata: None,
        });
    }
    if !runtime_context_prompt.trim().is_empty() {
        messages.push(crate::services::llm::client::ChatMessage {
            role: "system".to_string(),
            content: serde_json::Value::String(runtime_context_prompt.clone()),
            reasoning_content: None,
            speaker: None,
            tool_call_id: None,
            tool_calls: None,
            metadata: None,
        });
    }
    if !narration_prompt.trim().is_empty() {
        messages.push(crate::services::llm::client::ChatMessage {
            role: "system".to_string(),
            content: serde_json::Value::String(narration_prompt.clone()),
            reasoning_content: None,
            speaker: None,
            tool_call_id: None,
            tool_calls: None,
            metadata: None,
        });
    }
    messages.extend([
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
            // 第 10 项：当前回合玩家附件以 multipart 随本条消息下发（仅当前回合）。
            content: crate::models::session::build_wire_content(&turn_payload, player_media),
            reasoning_content: None,
            speaker: Some(player_character_name.to_string()),
            tool_call_id: None,
            tool_calls: None,
            metadata: None,
        },
    ]);

    // 历史深度插入（第 6 项）：depth:N = 距末尾 N 条处插入系统消息。
    for (depth, content) in &module_resolution.depth_insertions {
        if content.trim().is_empty() {
            continue;
        }
        let index = messages.len().saturating_sub(*depth);
        messages.insert(
            index,
            crate::services::llm::client::ChatMessage {
                role: "system".to_string(),
                content: serde_json::Value::String(content.clone()),
                reasoning_content: None,
                speaker: None,
                tool_call_id: None,
                tool_calls: None,
                metadata: None,
            },
        );
    }
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
    kv_vars: &std::collections::HashMap<String, String>,
    generation: &GenerationParams,
    player_media: &[ContentPart],
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
        public_scene_state_lines,
        next_scene_name,
        next_location,
        visible_characters,
        kv_vars,
        player_media,
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
            // 第 8 项：本次实际使用的采样参数与被 provider 过滤掉的项。
            "request_params": crate::services::llm::param_support::describe_params_for_trace(
                speaker_provider,
                generation,
                serde_json::json!({ "json_mode": true }),
            ),
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

pub(crate) fn build_character_init_payload(
    world: &WorldDefinition,
    speaker_name: &str,
    speaker_profile: Option<&CharacterDefinition>,
    session: &SessionSnapshot,
) -> String {
    let character_background_prompt = speaker_profile
        .map(|profile| render_prompt_variables(&profile.background_prompt))
        .unwrap_or_default();

    // 角色字段空值不发(对齐 scene_state 标准)。
    let mut character = serde_json::Map::new();
    character.insert("name".to_string(), serde_json::json!(speaker_name));
    if let Some(role) = speaker_profile
        .map(|profile| profile.role.trim())
        .filter(|value| !value.is_empty())
    {
        character.insert("role".to_string(), serde_json::json!(role));
    }
    if !character_background_prompt.trim().is_empty() {
        character.insert(
            "background_prompt".to_string(),
            serde_json::json!(character_background_prompt),
        );
    }
    if let Some(attributes) = speaker_profile
        .map(|profile| &profile.attributes)
        .filter(|attributes| !attributes.is_empty())
    {
        character.insert("attributes".to_string(), serde_json::json!(attributes));
    }

    serde_json::to_string_pretty(&serde_json::json!({
        "basic_setting": {
            "character": character,
            "world": {
                "world_name": session.world_name,
                "background_prompt": world.background_prompt,
            }
        }
    }))
    .unwrap_or_else(|_| "{}".to_string())
}
