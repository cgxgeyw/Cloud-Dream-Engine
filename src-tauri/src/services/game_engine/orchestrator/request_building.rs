use crate::models::character::CharacterDefinition;
use crate::models::generation_params::GenerationParams;
use crate::models::mcp_tool::{director_config_allows_mcp_tool, MCP_TOOL_SCHEDULE_NOTIFICATION_ID};
use crate::models::memory::MemoryEntry;
use crate::models::model_config::ModelConfig;
use crate::models::session::*;
use crate::models::settings::AppSettings;
use crate::models::world::WorldDefinition;
use crate::services::game_engine::dialogue::DialoguePipeline;
use crate::services::game_engine::structured_output::StructuredOutputFailure;
use crate::services::notifications::notification_tool_definition;
use rusqlite::Connection;

use super::character_prompt::build_character_prompt_artifacts;

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
        response_schema: Some(build_character_response_schema()),
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

pub(crate) fn build_character_response_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": ["speaker", "content", "narration"],
        "additionalProperties": true,
        "properties": {
            "speaker": { "type": "string" },
            "content": { "type": "string" },
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
