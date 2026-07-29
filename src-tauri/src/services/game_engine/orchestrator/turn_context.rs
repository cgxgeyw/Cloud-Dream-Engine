use crate::models::character::{resolve_character_narration_prompt, CharacterDefinition};
use crate::models::generation_params::GenerationParams;
use crate::models::mcp_tool::{director_config_allows_mcp_tool, MCP_TOOL_SCHEDULE_NOTIFICATION_ID};
use crate::models::memory::MemoryEntry;
use crate::models::model_config::ModelConfig;
use crate::models::session::*;
use crate::models::settings::AppSettings;
use crate::models::world::WorldDefinition;
use crate::services::game_engine::dialogue::DialoguePipeline;
use crate::services::game_engine::prompting::{
    build_prompt_call, llm_chat_messages_to_values, recent_messages_text, render_prompt_variables,
    resolve_prompt_modules, resolve_runtime_context_prompt,
};
use crate::services::game_engine::structured_output::StructuredOutputFailure;
use crate::services::notifications::notification_tool_definition;
use rusqlite::{params, Connection};
use std::collections::HashMap;

pub(crate) fn resolve_settings(conn: &Connection) -> Result<AppSettings, String> {
    crate::commands::settings::load_app_settings(conn)
}

/// 世界级生成参数：写在 `director_config.generation_params`，随世界包导入导出。
pub fn world_generation_params(world: &WorldDefinition) -> Option<GenerationParams> {
    GenerationParams::from_json(world.director_config.get("generation_params"))
}

/// 解析某次调用最终使用的生成参数（第 8 项）：
/// 角色内置默认 → 应用设置 → 世界 → 会话，逐层覆盖，只有本层显式给了值才生效。
fn resolve_generation_params(
    role: &str,
    settings: &AppSettings,
    world: &WorldDefinition,
    session: &SessionSnapshot,
) -> GenerationParams {
    let world_params = world_generation_params(world);
    let session_params =
        (!session.generation_params.is_empty()).then(|| session.generation_params.clone());
    GenerationParams::resolve_for_role(
        role,
        &settings.generation_params,
        world_params.as_ref(),
        session_params.as_ref(),
    )
}

/// 生成参数解析结果 + 模型连接配置里的 max_tokens 兜底。
/// 连接配置的 max_tokens 是「这个模型最多能出多少」，生成参数没显式配时沿用它，
/// 与改造前行为一致。
pub fn resolve_generation_params_with_model(
    role: &str,
    settings: &AppSettings,
    world: &WorldDefinition,
    session: &SessionSnapshot,
    model: &ModelConfig,
) -> GenerationParams {
    let mut resolved = resolve_generation_params(role, settings, world, session);
    if resolved.max_tokens.is_none() {
        resolved.max_tokens = Some(model.max_tokens);
    }
    resolved
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
    let mut system_prompt = dialogue_pipeline.build_character_system_prompt_with_contract(
        speaker_name,
        speaker_profile,
        None,
        None,
        crate::services::game_engine::memory::resolve_fact_extraction_enabled(world),
    );
    if let Some(interaction_contract) =
        crate::models::interaction::build_interaction_response_contract(&world.director_config)
    {
        system_prompt.push_str("\n\n");
        system_prompt.push_str(&interaction_contract);
    }
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
    let mut response_fields = vec![
        "speaker",
        "content",
        "narration",
        "session_attribute_updates",
        "character_attribute_updates",
        "memory_entries",
        "fact_extractions",
    ];
    if !crate::models::interaction::declared_interaction_kinds(&world.director_config).is_empty() {
        response_fields.push("interaction");
    }
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
    public_scene_state_lines: &[String],
    kv_vars: &std::collections::HashMap<String, String>,
    generation: &GenerationParams,
    player_media: &[ContentPart],
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
        public_scene_state_lines,
        scene_name,
        location,
        &session.visible_characters,
        kv_vars,
        player_media,
    );
    let notification_tool_allowed =
        director_config_allows_mcp_tool(&world.director_config, MCP_TOOL_SCHEDULE_NOTIFICATION_ID);
    let tools = notification_tool_allowed.then(|| vec![build_notification_chat_tool_definition()]);
    let native_tool_calling = tools
        .as_ref()
        .map(|items| !items.is_empty())
        .unwrap_or(false);
    crate::services::llm::client::ChatRequest {
        model: model.model_id.to_string(),
        messages: artifacts.messages,
        generation: generation.clone(),
        stream: Some(model.streaming_enabled && !native_tool_calling),
        json_mode: Some(true),
        response_schema: Some(build_character_response_schema(world)),
        tools,
        tool_choice: native_tool_calling
            .then_some(crate::services::llm::client::ChatToolChoice::Auto),
        native_tool_calling: native_tool_calling.then_some(true),
    }
}

fn build_notification_chat_tool_definition() -> crate::services::llm::client::ChatToolDefinition {
    let tool = notification_tool_definition();
    crate::services::llm::client::ChatToolDefinition {
        name: tool
            .get("tool_name")
            .and_then(|value| value.as_str())
            .unwrap_or("schedule_notification")
            .to_string(),
        description: tool
            .get("description")
            .and_then(|value| value.as_str())
            .map(str::to_string),
        input_schema: tool
            .get("arguments_schema")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({ "type": "object" })),
    }
}

pub(crate) fn build_character_response_schema(
    world: &crate::models::world::WorldDefinition,
) -> serde_json::Value {
    let mut schema = serde_json::json!({
        "type": "object",
        "required": ["speaker", "content", "narration"],
        "additionalProperties": true,
        "properties": {
            "speaker": { "type": "string" },
            "content": { "type": "string" },
            "narration": { "type": "string" }
        }
    });
    let kinds = crate::models::interaction::declared_interaction_kinds(&world.director_config);
    if !kinds.is_empty() {
        schema["properties"]["interaction"] = serde_json::json!({
            "type": "object",
            "required": ["kind", "prompt", "config"],
            "additionalProperties": false,
            "properties": {
                "kind": { "type": "string", "enum": kinds },
                "prompt": { "type": "string" },
                "config": { "type": "object", "additionalProperties": true }
            }
        });
    }
    schema
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
    public_scene_state_lines: &[String],
) -> String {
    let speaker_character_id = speaker_profile.map(|profile| profile.id.as_str());
    let visibility_context = build_character_visibility_context_payload(
        visible_attribute_lines,
        visible_inventory_items,
        public_scene_state_lines,
        speaker_character_id,
        speaker_name,
    );
    let memory_context =
        build_character_memory_context(world, session, recalled_memories, memory_pool);
    serde_json::to_string_pretty(&serde_json::json!({
        "dialogue_history": {
            "recent_dialogue": build_recent_dialogue_payload(recent_messages, player_name),
            "memory_context": build_memory_context_payload(&memory_context, recent_messages, player_name),
        },
        "current_state": {
            "requested_speaker": speaker_name,
            "player_character_name": player_name,
            "player_input": player_input,
            "scene_state": build_character_scene_state_payload(
                session,
                location,
                scene_name,
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
                // 第 10 项：历史消息中的媒体渲染为占位文本，不重复下发 base64。
                "content": message.content.as_prompt_text(),
            })
        })
        .collect()
}

pub(crate) fn build_memory_entry_payload(memories: &[MemoryEntry]) -> Vec<serde_json::Value> {
    memories
        .iter()
        .map(|memory| {
            // 只发对"生成台词"有语义的字段：内容、谁说的、地点、在场者、相对轮次。
            // 删除内部标识符(id/conversation_id/event_id/item_id/scene_id)与
            // 内部追踪/排序字段(importance/created_at/layer/memory_type/source/keywords)。
            let mut entry = serde_json::Map::new();
            entry.insert("content".to_string(), serde_json::json!(memory.content));
            entry.insert(
                "turn_index".to_string(),
                serde_json::json!(memory.turn_index),
            );
            if let Some(speaker) = memory
                .speaker
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                entry.insert("speaker".to_string(), serde_json::json!(speaker));
            }
            if let Some(location) = memory
                .location
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                entry.insert("location".to_string(), serde_json::json!(location));
            }
            if !memory.participants.is_empty() {
                entry.insert(
                    "participants".to_string(),
                    serde_json::json!(memory.participants),
                );
            }
            serde_json::Value::Object(entry)
        })
        .collect()
}

pub(crate) fn build_memory_context_payload(
    memory_context: &CharacterMemoryContext,
    recent_messages: &[ChatMessage],
    player_character_name: &str,
) -> serde_json::Value {
    // 去重：event_timeline 与 matched_memories 可能命中同一条记忆(按 id 去重)；
    // dialogue_focus 与顶层 recent_dialogue 在轮次重叠时是同一批消息(按内容去重)。
    let matched_ids: std::collections::HashSet<&str> = memory_context
        .matched_memories
        .iter()
        .map(|memory| memory.id.as_str())
        .collect();
    let event_timeline: Vec<MemoryEntry> = memory_context
        .event_timeline
        .iter()
        .filter(|memory| !matched_ids.contains(memory.id.as_str()))
        .cloned()
        .collect();
    let recent_contents: std::collections::HashSet<String> = recent_messages
        .iter()
        .map(|message| message.content.trim())
        .collect();
    let dialogue_focus: Vec<ChatMessage> = memory_context
        .dialogue_focus
        .iter()
        .filter(|message| !recent_contents.contains(&message.content.trim()))
        .cloned()
        .collect();

    let mut context = serde_json::Map::new();
    if !memory_context.hit_turns.is_empty() {
        context.insert(
            "hit_turns".to_string(),
            serde_json::json!(memory_context.hit_turns),
        );
    }
    if !memory_context.matched_memories.is_empty() {
        context.insert(
            "matched_memories".to_string(),
            serde_json::json!(build_memory_entry_payload(&memory_context.matched_memories)),
        );
    }
    if !event_timeline.is_empty() {
        context.insert(
            "event_timeline".to_string(),
            serde_json::json!(build_memory_entry_payload(&event_timeline)),
        );
    }
    if !dialogue_focus.is_empty() {
        context.insert(
            "dialogue_focus".to_string(),
            serde_json::json!(build_recent_dialogue_payload(
                &dialogue_focus,
                player_character_name
            )),
        );
    }
    serde_json::Value::Object(context)
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
    public_scene_state_lines: &[String],
    character_id: Option<&str>,
    character_name: &str,
) -> serde_json::Value {
    // 同一批可见数据原来存了三套(records 结构化 + lines 行文本，且 scene_state 还有一份)。
    // 这里把 visibility_context 定为唯一来源，只保留结构化 records，删除行文本副本；
    // scene_state 里的 visible_*/public_* 拷贝已移除。空值不发。
    let mut context = serde_json::Map::new();
    if !public_scene_state_lines.is_empty() {
        context.insert(
            "public_scene_state_lines".to_string(),
            serde_json::json!(public_scene_state_lines),
        );
    }
    let attribute_records = build_visible_attribute_records(visible_attribute_lines);
    if !attribute_records.is_empty() {
        context.insert(
            "visible_attribute_records".to_string(),
            serde_json::json!(attribute_records),
        );
    }
    let inventory_records =
        build_visible_inventory_records(visible_inventory_items, character_id, character_name);
    if !inventory_records.is_empty() {
        context.insert(
            "visible_inventory_records".to_string(),
            serde_json::json!(inventory_records),
        );
    }
    serde_json::Value::Object(context)
}

pub(crate) fn build_character_scene_state_payload(
    session: &SessionSnapshot,
    location: &str,
    scene_name: &str,
) -> serde_json::Value {
    // 只放入对模型有意义且非空的字段：
    // - 删除 scene_id（内部 UUID，模型零语义）
    // - 删除 world_name（已在 init_payload.basic_setting.world 提供，避免重复）
    // - 数组/映射/标签为空时不放入，减少噪声与 token
    let mut state = serde_json::Map::new();
    state.insert("location".to_string(), serde_json::json!(location));
    state.insert("scene_name".to_string(), serde_json::json!(scene_name));

    let time_label = session.time_label.trim();
    if !time_label.is_empty() {
        state.insert("time_label".to_string(), serde_json::json!(time_label));
    }
    if !session.scene.temporary_tags.is_empty() {
        state.insert(
            "scene_tags".to_string(),
            serde_json::json!(session.scene.temporary_tags),
        );
    }
    let state_phase = session.state.phase.trim();
    if !state_phase.is_empty() {
        state.insert("state_phase".to_string(), serde_json::json!(state_phase));
    }
    if !session.state.tags.is_empty() {
        state.insert(
            "state_tags".to_string(),
            serde_json::json!(session.state.tags),
        );
    }
    if !session.state.metrics.is_empty() {
        state.insert(
            "state_metrics".to_string(),
            serde_json::json!(session.state.metrics),
        );
    }
    let present_characters = collect_memory_participants(session);
    if !present_characters.is_empty() {
        state.insert(
            "present_characters".to_string(),
            serde_json::json!(present_characters),
        );
    }
    let discovered_locations = session
        .map_graph_nodes
        .iter()
        .filter(|node| node.discovered && !node.label.trim().is_empty())
        .map(|node| node.label.clone())
        .collect::<Vec<_>>();
    if !discovered_locations.is_empty() {
        state.insert(
            "discovered_locations".to_string(),
            serde_json::json!(discovered_locations),
        );
    }

    // 可见属性/物品(及其 public 子集)由 visibility_context 统一承载，这里不再重复。
    serde_json::Value::Object(state)
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
            // 删除内部 UUID(item_id/owner_id)；visibility 与派生的 knowledge_scope
            // 信息重叠，只留 knowledge_scope；description/tags/disclosed_to 空值不发。
            let mut record = serde_json::Map::new();
            record.insert("name".to_string(), serde_json::json!(item.name));
            record.insert("category".to_string(), serde_json::json!(item.category));
            record.insert("quantity".to_string(), serde_json::json!(item.quantity));
            record.insert("owner_type".to_string(), serde_json::json!(item.owner_type));
            record.insert(
                "knowledge_scope".to_string(),
                serde_json::json!(knowledge_scope),
            );
            if !item.description.trim().is_empty() {
                record.insert(
                    "description".to_string(),
                    serde_json::json!(item.description),
                );
            }
            if !item.tags.is_empty() {
                record.insert("tags".to_string(), serde_json::json!(item.tags));
            }
            if !item.disclosed_to.is_empty() {
                record.insert(
                    "disclosed_to".to_string(),
                    serde_json::json!(item.disclosed_to),
                );
            }
            serde_json::Value::Object(record)
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
            if let Some(speaker_name) = extract_player_view_switch_speaker(message.content.as_str())
            {
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
                message_id: ChatMessage::generate_id(),
                created_at: chrono::Utc::now().to_rfc3339(),
                parent_message_id: None,
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
    let Some(profile) = speaker_profile else {
        return 8;
    };
    let strategy = profile.memory_strategy.trim();
    if strategy.is_empty() {
        return 8;
    }
    let overrides = crate::services::game_engine::memory::parse_memory_strategy(strategy);
    if overrides.disabled {
        // 策略为"不记"时召回条数归零(prepare_character_recall 也会短路,双保险)。
        return 0;
    }
    extract_recall_limit_hint(strategy).unwrap_or(8)
}

/// 从 memory_strategy 自由文本里提取"<数字>轮" / "<数字> turns"这类条数线索。
/// 手工扫描数字游标,不引 regex;取不到返回 None。
fn extract_recall_limit_hint(strategy: &str) -> Option<i32> {
    let lower = strategy.to_lowercase();
    let chars: Vec<char> = lower.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        if chars[index].is_ascii_digit() {
            let start = index;
            while index < chars.len() && chars[index].is_ascii_digit() {
                index += 1;
            }
            let number: String = chars[start..index].iter().collect();
            let suffix: String = chars[index..]
                .iter()
                .skip_while(|ch| ch.is_whitespace())
                .take(5)
                .collect();
            if suffix.starts_with('轮') || suffix.starts_with("turn") {
                return number.parse::<i32>().ok().map(|value| value.clamp(1, 32));
            }
        } else {
            index += 1;
        }
    }
    None
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

#[cfg(test)]
mod tests {
    use super::*;

    fn profile_with_strategy(memory_strategy: &str) -> CharacterDefinition {
        CharacterDefinition {
            id: "char-a".to_string(),
            name: "Alice".to_string(),
            world_id: "world-1".to_string(),
            role: "".to_string(),
            background_prompt: "".to_string(),
            model: "".to_string(),
            memory_strategy: memory_strategy.to_string(),
            recent_dialogue_rounds: 8,
            attributes: vec![],
            portrait_assets: vec![],
            avatar_asset: String::new(),
            system_prompt_template: "".to_string(),
            response_contract_prompt: "".to_string(),
            narration_prompt: "".to_string(),
            runtime_system_prompt: "".to_string(),
        }
    }

    #[test]
    fn recall_limit_reads_turn_hint_from_strategy() {
        assert_eq!(
            resolve_character_memory_recall_limit(Some(&profile_with_strategy(
                "记住最近12轮的关键信息"
            ))),
            12
        );
        assert_eq!(
            resolve_character_memory_recall_limit(Some(&profile_with_strategy("recall 5 turns"))),
            5
        );
        // 超出上限的线索被钳制
        assert_eq!(
            resolve_character_memory_recall_limit(Some(&profile_with_strategy("99轮"))),
            32
        );
    }

    #[test]
    fn recall_limit_falls_back_to_default() {
        assert_eq!(resolve_character_memory_recall_limit(None), 8);
        assert_eq!(
            resolve_character_memory_recall_limit(Some(&profile_with_strategy(""))),
            8
        );
        // 没有轮数线索的描述句 → 默认 8
        assert_eq!(
            resolve_character_memory_recall_limit(Some(&profile_with_strategy(
                "记住宴会中的人际变化与诗句往来。"
            ))),
            8
        );
        // 年份这类数字后面不跟"轮/turn",不应被当成线索
        assert_eq!(
            resolve_character_memory_recall_limit(Some(&profile_with_strategy(
                "2024年的约定要记住"
            ))),
            8
        );
    }

    #[test]
    fn recall_limit_zero_when_strategy_disabled() {
        assert_eq!(
            resolve_character_memory_recall_limit(Some(&profile_with_strategy("off"))),
            0
        );
        assert_eq!(
            resolve_character_memory_recall_limit(Some(&profile_with_strategy("无记忆"))),
            0
        );
    }

    // ---- 第 8 项：生成参数三级覆盖（走真实 DB 读写，验收「不改代码即可调整」）----

    fn params_test_world(generation_params: serde_json::Value) -> WorldDefinition {
        WorldDefinition {
            id: "world-params".to_string(),
            name: "参数世界".to_string(),
            genre: String::new(),
            background_prompt: String::new(),
            opening_scene: "开场".to_string(),
            summary: String::new(),
            time_system: String::new(),
            map_nodes: serde_json::json!({ "version": 1, "nodes": [] }),
            triggers: Vec::new(),
            time_config: serde_json::json!({}),
            director_config: serde_json::json!({ "generation_params": generation_params }),
            ui_theme_config: serde_json::json!({}),
            director_system_prompt_base: String::new(),
            director_runtime_system_prompt: String::new(),
            opening_messages: Vec::new(),
            opening_character_ids: Vec::new(),
            player_character_id: None,
        }
    }

    fn params_test_session(id: &str, world_name: &str) -> SessionSnapshot {
        SessionSnapshot {
            id: id.to_string(),
            world_name: world_name.to_string(),
            location: "开场".to_string(),
            time_label: String::new(),
            current_speaker: String::new(),
            current_line: String::new(),
            player_character_id: String::new(),
            player_character_name: "玩家".to_string(),
            visible_characters: Vec::new(),
            messages: Vec::new(),
            player_stats: Vec::new(),
            map_graph_nodes: Vec::new(),
            map_graph_edges: Vec::new(),
            inventory_items: Vec::new(),
            system_log: Vec::new(),
            scene: Default::default(),
            assets: Default::default(),
            state: Default::default(),
            generation_params: Default::default(),
        }
    }

    #[test]
    fn character_request_carries_player_media_as_multipart() {
        let world = params_test_world(serde_json::json!({}));
        let session = params_test_session("session-media", "参数世界");
        let character = profile_with_strategy("");
        let pipeline = DialoguePipeline::new();
        let media = vec![crate::models::session::ContentPart {
            part_type: "image_url".to_string(),
            text: None,
            image_url: Some(crate::models::session::ImageUrl {
                url: "data:image/png;base64,QUJD".to_string(),
            }),
            input_audio: None,
        }];

        let request = build_character_chat_request(
            &pipeline,
            &world,
            &params_test_model(),
            "Alice",
            Some(&character),
            &session,
            "玩家",
            "开场",
            "开场",
            "看图说话",
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &std::collections::HashMap::new(),
            &GenerationParams::default(),
            &media,
        );

        let user_message = request
            .messages
            .iter()
            .rev()
            .find(|message| message.role == "user")
            .expect("user message");
        let parts = user_message
            .content
            .as_array()
            .expect("multipart user content");
        assert_eq!(parts[0]["type"], "text");
        assert!(parts[0]["text"].as_str().unwrap_or("").contains("看图说话"));
        assert_eq!(
            parts[1].pointer("/image_url/url").and_then(|v| v.as_str()),
            Some("data:image/png;base64,QUJD")
        );

        // 无媒体时 content 仍是字符串（纯文本时代行为不变）
        let plain_request = build_character_chat_request(
            &pipeline,
            &world,
            &params_test_model(),
            "Alice",
            Some(&character),
            &session,
            "玩家",
            "开场",
            "开场",
            "纯文本",
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &std::collections::HashMap::new(),
            &GenerationParams::default(),
            &[],
        );
        let plain_user = plain_request
            .messages
            .iter()
            .rev()
            .find(|message| message.role == "user")
            .expect("user message");
        assert!(plain_user.content.is_string());
    }

    fn params_test_model() -> ModelConfig {
        ModelConfig {
            id: "m-params".to_string(),
            name: "test".to_string(),
            model_type: "text".to_string(),
            provider: "openai".to_string(),
            model_id: "gpt-test".to_string(),
            base_url: "http://localhost".to_string(),
            api_key: String::new(),
            max_tokens: 1234,
            streaming_enabled: false,
            is_default: true,
            input_modalities: Vec::new(),
        }
    }

    #[test]
    fn session_generation_params_survive_a_db_round_trip() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        crate::db::schema::create_tables(&conn).expect("create schema");
        let repo = crate::db::repositories::session_repo::SessionRepository::new(&conn);
        let mut session = params_test_session("sess-params", "参数世界");
        session.generation_params = GenerationParams {
            temperature: Some(1.25),
            stop: Some(vec!["【完】".to_string()]),
            ..Default::default()
        };
        repo.upsert(&session).expect("upsert session");

        let loaded = repo
            .get("sess-params")
            .expect("load session")
            .expect("session exists");

        assert_eq!(loaded.generation_params.temperature, Some(1.25));
        assert_eq!(
            loaded.generation_params.stop,
            Some(vec!["【完】".to_string()])
        );
        assert!(
            loaded.generation_params.top_p.is_none(),
            "没配的参数读回来仍是没配"
        );
    }

    #[test]
    fn world_and_session_layers_override_app_settings_without_code_changes() {
        let mut settings = AppSettings::default();
        settings.generation_params = GenerationParams {
            temperature: Some(0.4),
            top_p: Some(0.95),
            ..Default::default()
        };
        let world = params_test_world(serde_json::json!({ "temperature": 1.15, "seed": 99 }));
        let mut session = params_test_session("sess-1", &world.name);

        // 只有应用 + 世界两层时：世界层的 temperature 生效，应用层的 top_p 仍在。
        let world_level = resolve_generation_params(
            crate::models::generation_params::GENERATION_ROLE_CHARACTER,
            &settings,
            &world,
            &session,
        );
        assert_eq!(world_level.temperature, Some(1.15));
        assert_eq!(world_level.top_p, Some(0.95));
        assert_eq!(world_level.seed, Some(99));

        // 存档自己再覆盖一层：会话层最高优先，其余层不受影响。
        session.generation_params = GenerationParams {
            temperature: Some(0.1),
            ..Default::default()
        };
        let session_level = resolve_generation_params(
            crate::models::generation_params::GENERATION_ROLE_CHARACTER,
            &settings,
            &world,
            &session,
        );
        assert_eq!(session_level.temperature, Some(0.1));
        assert_eq!(session_level.top_p, Some(0.95));
        assert_eq!(session_level.seed, Some(99));
    }

    #[test]
    fn max_tokens_falls_back_to_the_model_connection_config() {
        // 连接配置的 max_tokens 是「这个模型最多出多少」，生成参数没配时必须沿用它，
        // 否则改造后请求会丢掉 max_tokens、与改造前行为不一致。
        let settings = AppSettings::default();
        let world = params_test_world(serde_json::json!({}));
        let session = params_test_session("sess-1", &world.name);

        let resolved = resolve_generation_params_with_model(
            crate::models::generation_params::GENERATION_ROLE_DIRECTOR,
            &settings,
            &world,
            &session,
            &params_test_model(),
        );

        assert_eq!(resolved.max_tokens, Some(1234));
        assert_eq!(resolved.temperature, Some(0.7), "导演内置默认不变");
    }

    #[test]
    fn world_layer_ignores_a_malformed_generation_params_block() {
        let settings = AppSettings::default();
        let world = params_test_world(serde_json::json!("温度高一点"));
        let session = params_test_session("sess-1", &world.name);

        let resolved = resolve_generation_params(
            crate::models::generation_params::GENERATION_ROLE_CHARACTER,
            &settings,
            &world,
            &session,
        );

        assert_eq!(
            resolved.temperature,
            Some(0.8),
            "世界包写坏了就当没配，回落到内置默认而不是报错"
        );
    }
}
