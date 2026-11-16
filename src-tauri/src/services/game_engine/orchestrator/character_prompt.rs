use crate::models::character::{
    resolve_character_narration_prompt, resolve_character_response_contract_prompt,
    CharacterDefinition,
};
use crate::models::generation_params::GenerationParams;
use crate::models::memory::MemoryEntry;
use crate::models::model_config::ModelConfig;
use crate::models::session::*;
use crate::models::world::WorldDefinition;
use crate::services::game_engine::dialogue::{
    build_character_system_prompt, DialoguePipeline, BUILTIN_MEMORY_CONTRACT,
    BUILTIN_RESPONSE_CONTRACT,
};
use crate::services::game_engine::memory::resolve_fact_extraction_enabled;
use crate::services::game_engine::prompting::{
    build_prompt_call, llm_chat_messages_to_values, resolve_prompt_section_order,
    resolve_runtime_context_prompt, CHARACTER_PROMPT_SECTION_IDS,
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
    _dialogue_pipeline: &DialoguePipeline,
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
    public_scene_state_lines: &[String],
    next_scene_name: &str,
    next_location: &str,
    visible_characters: &[String],
    kv_vars: &std::collections::HashMap<String, String>,
    player_media: &[ContentPart],
) -> CharacterPromptArtifacts {
    // 作者输入框的变量插值（内置 + 插值管理自定义 + 会话 KV）统一在此展开。
    let custom_vars = crate::services::game_engine::prompting::load_prompt_custom_vars(world);
    let prompt_ctx = crate::services::game_engine::prompting::PromptVariableContext {
        world: Some(world),
        session: Some(session),
        kv_vars: Some(kv_vars),
        custom_vars: Some(&custom_vars),
    };
    let visibility_context = build_character_visibility_context_payload(
        visible_attribute_lines,
        public_scene_state_lines,
    );
    let narration_prompt =
        crate::services::game_engine::prompting::render_prompt_variables_with_context(
            &resolve_character_narration_prompt(
                speaker_profile.map(|profile| profile.narration_prompt.as_str()),
            ),
            &prompt_ctx,
        );
    // 模板内部已做过时间类渲染，这里再按完整上下文补一遍其余变量。
    // 模型内部思考时先用渲染前的空模板去拼，再把骨架变量补一次，避免二次渲染误伤。
    let base_system_prompt =
        crate::services::game_engine::prompting::render_prompt_variables_with_context(
            &build_character_system_prompt(speaker_name, speaker_profile, None),
            &prompt_ctx,
        );
    // 角色卡可能只填了身份/背景而没填 system_prompt_template（世界主控新建角色常见）。
    // 这里补最小人设，并统一追加署名硬约束：否则模型容易把身份/官职当成 speaker，
    // 触发 domain_validation_failed 导致整个回合暂停。
    let speaker_lock = format!(
        "你正在扮演「{speaker_name}」。回复 JSON 的 \"speaker\" 字段必须逐字写作「{speaker_name}」，不得用身份、官职、称号或别称代替。"
    );
    let system_prompt = if base_system_prompt.trim().is_empty() {
        let mut fallback = format!("你是{speaker_name}。");
        if let Some(role) = speaker_profile
            .map(|profile| profile.role.trim())
            .filter(|value| !value.is_empty())
        {
            fallback.push_str(&format!("身份：{role}。"));
        }
        if let Some(background) = speaker_profile
            .map(|profile| profile.background_prompt.trim())
            .filter(|value| !value.is_empty())
        {
            fallback.push(' ');
            fallback.push_str(background);
        }
        format!("{}\n\n{speaker_lock}", fallback.trim())
    } else {
        format!("{}\n\n{speaker_lock}", base_system_prompt.trim())
    };
    let fact_extraction_enabled = resolve_fact_extraction_enabled(world);
    let custom_response_contract = resolve_character_response_contract_prompt(
        speaker_profile.map(|profile| profile.response_contract_prompt.as_str()),
    );
    // 自定义契约完全替代内置契约（不再追加 BUILTIN_RESPONSE_CONTRACT）：
    // 引擎必需字段（speaker/content/narration/pass）的格式说明由作者契约自行
    // 负责；留空时才发送内置契约。变量占位符照常展开。
    let response_contract_prompt = if custom_response_contract.trim().is_empty() {
        BUILTIN_RESPONSE_CONTRACT.to_string()
    } else {
        crate::services::game_engine::prompting::render_prompt_variables_with_context(
            custom_response_contract.trim(),
            &prompt_ctx,
        )
    };
    let runtime_context_prompt = resolve_runtime_context_prompt(world, Some(&prompt_ctx));
    let character_runtime_context_prompt = speaker_profile
        .map(|profile| {
            crate::services::game_engine::prompting::render_prompt_variables_with_context(
                &profile.runtime_system_prompt,
                &prompt_ctx,
            )
        })
        .unwrap_or_default()
        .trim()
        .to_string();
    let init_payload =
        build_character_init_payload(world, speaker_name, speaker_profile, session, &prompt_ctx);
    let turn_payload = build_character_turn_payload(
        world,
        speaker_name,
        session,
        player_character_name,
        location,
        scene_name,
        player_input,
        recent_messages,
        recalled_memories,
        memory_pool,
        visible_attribute_lines,
        public_scene_state_lines,
    );
    // 返回契约的字段清单/说明/更新格式从 return_format 单一事实源生成，
    // response_schema 请求参数也来自同一张表；这份 JSON 只进 trace，不随
    // messages 发给模型（模型看到的是 dialogue.rs 的 BUILTIN 契约文本）。
    let response_contract = serde_json::json!({
        "format": "json_object",
        "instructions": "只返回一个 JSON 对象。仅返回本回合发生变化的状态字段；未变化字段省略。会话/世界状态使用 session_attribute_updates，角色状态使用 character_attribute_updates。",
        "fields": crate::services::game_engine::return_format::character_contract_field_ids(),
        "runtime_update_format": crate::services::game_engine::return_format::character_runtime_update_format(),
        "field_descriptions": crate::services::game_engine::return_format::character_field_descriptions(),
        "tool_policy": crate::services::game_engine::return_format::CHARACTER_TOOL_POLICY,
    });
    let scene_state = serde_json::json!({
        "scene_name": next_scene_name,
        "location": next_location,
        "visible_characters": visible_characters,
        "player_character_name": player_character_name
    });
    let mut modules = Vec::new();
    let init_value: serde_json::Value = serde_json::from_str(&init_payload).unwrap_or_default();
    let turn_value: serde_json::Value = serde_json::from_str(&turn_payload).unwrap_or_default();
    let character = init_value
        .pointer("/basic_setting/character")
        .cloned()
        .unwrap_or_default();
    let world_background = init_value
        .pointer("/basic_setting/world")
        .cloned()
        .unwrap_or_default();
    let current_state = turn_value.get("current_state").cloned().unwrap_or_default();
    let mut prompt_visibility_context = visibility_context.clone();
    if let Some(context) = prompt_visibility_context.as_object_mut() {
        context.remove("visible_attribute_records");
    }
    // 对话历史始终发送最近 N 轮。Embedding/OAG 只决定额外注入哪一层，
    // 额外上下文统一从 oag_context 发一次；chat_history 仍只发最近对话，
    // 避免同一份记忆重复下发。
    let oag_context_value = turn_value
        .pointer("/dialogue_history/memory_context")
        .cloned()
        .unwrap_or_default();
    let chat_history_value = serde_json::json!({
        "recent_dialogue": turn_value
            .pointer("/dialogue_history/recent_dialogue")
            .cloned()
            .unwrap_or_default()
    });
    let mut sections = std::collections::HashMap::<
        String,
        (String, String, serde_json::Value, String, bool),
    >::new();
    sections.insert(
        "world_background".into(),
        (
            "世界背景信息".into(),
            "world.background_prompt".into(),
            world_background,
            "system".into(),
            false,
        ),
    );
    // 额外注入层（Embedding 记忆召回 / OAG 结构化信息）统一放在这里；
    // 没有命中时该 section 为空，不发送。
    sections.insert(
        "oag_context".into(),
        (
            "OAG/记忆检索注入信息".into(),
            "character_memory.retrieval".into(),
            oag_context_value,
            "system".into(),
            false,
        ),
    );
    sections.insert(
        "character_background".into(),
        (
            "角色长期背景".into(),
            "character.background_prompt".into(),
            character
                .get("background_prompt")
                .cloned()
                .unwrap_or_default(),
            "system".into(),
            true,
        ),
    );
    sections.insert(
        "character_rules".into(),
        (
            "角色设定与行为规则".into(),
            "character.system_prompt_template".into(),
            serde_json::Value::String(system_prompt.clone()),
            "system".into(),
            true,
        ),
    );
    sections.insert("character_attributes".into(), ("角色基础属性".into(), "character.attributes".into(), serde_json::json!({"role": character.get("role"), "attributes": character.get("attributes")}), "system".into(), false));
    sections.insert(
        "world_runtime_context".into(),
        (
            "世界运行时上下文".into(),
            "world.director_config.runtime_context_prompt".into(),
            serde_json::Value::String(runtime_context_prompt.clone()),
            "system".into(),
            true,
        ),
    );
    sections.insert(
        "character_runtime_context".into(),
        (
            "角色运行时上下文".into(),
            "character.runtime_system_prompt".into(),
            serde_json::Value::String(character_runtime_context_prompt.clone()),
            "system".into(),
            true,
        ),
    );
    sections.insert(
        "visible_characters".into(),
        (
            "角色列表与场景状态".into(),
            "runtime.scene".into(),
            serde_json::json!({
                "scene_state": current_state.get("scene_state").cloned().unwrap_or_default(),
                "visible_characters": visible_characters,
                "visibility_context": prompt_visibility_context,
            }),
            "system".into(),
            false,
        ),
    );
    sections.insert(
        "chat_history".into(),
        (
            "对话历史".to_string(),
            "runtime.dialogue_history".into(),
            chat_history_value,
            "user".into(),
            false,
        ),
    );
    sections.insert(
        "narration_prompt".into(),
        (
            "旁白提示词".into(),
            "character.narration_prompt".into(),
            serde_json::Value::String(narration_prompt.clone()),
            "system".into(),
            true,
        ),
    );
    sections.insert("player_input".into(), ("当前玩家输入".into(), "runtime.current_state".into(), serde_json::json!({"player_character_name": player_character_name, "player_input": current_state.get("player_input")}), "user".into(), false));
    sections.insert(
        "response_contract".into(),
        (
            "角色返回格式".into(),
            "character.response_contract".into(),
            serde_json::Value::String(response_contract_prompt),
            "system".into(),
            true,
        ),
    );
    sections.insert(
        "fact_memory_rules".into(),
        (
            "事实与记忆提取规则".into(),
            "framework.fact_memory_rules".into(),
            if fact_extraction_enabled {
                serde_json::json!(BUILTIN_MEMORY_CONTRACT)
            } else {
                serde_json::Value::Null
            },
            "system".into(),
            false,
        ),
    );
    let mut dynamic_ids = Vec::new();
    if let Some(records) = visibility_context
        .get("visible_attribute_records")
        .and_then(serde_json::Value::as_array)
    {
        for record in records {
            let Some(key) = record.get("key").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let prefix = if record
                .get("attribute_scope")
                .and_then(serde_json::Value::as_str)
                == Some("world")
            {
                "world_attribute"
            } else {
                "character_attribute"
            };
            let id = format!("{prefix}:{key}");
            let entry = sections.entry(id.clone()).or_insert_with(|| {
                (
                    "自定义属性".to_string(),
                    "runtime.visible_attribute".to_string(),
                    serde_json::json!([]),
                    "system".to_string(),
                    false,
                )
            });
            entry.2.as_array_mut().expect("attribute section stores values as an array").push(serde_json::json!({
                // key 必须随内容发给模型：角色按准确属性键写 character_attribute_updates，
                // 解析端对未知 key 静默丢弃；label/所属角色由作者在描述里补充。
                "key": key,
                "description": record.get("description").cloned().unwrap_or_default(),
                "enum_options": record.get("enum_options").cloned().unwrap_or_else(|| serde_json::json!([])),
                "current_value": record.get("current_value").cloned().unwrap_or(serde_json::Value::Null),
            }));
            if !dynamic_ids.contains(&id) {
                dynamic_ids.push(id);
            }
        }
    }
    let mut defaults = CHARACTER_PROMPT_SECTION_IDS
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>();
    defaults.extend(dynamic_ids);
    let default_refs = defaults.iter().map(String::as_str).collect::<Vec<_>>();
    let order = resolve_prompt_section_order(
        &world.director_config,
        "character_prompt_order",
        &default_refs,
    );
    let mut messages = Vec::new();
    for id in order {
        let Some((label, source, value, _role, editable)) = sections.get(&id) else {
            continue;
        };
        let section_content = if value.is_string() {
            value.as_str().unwrap_or_default().to_string()
        } else {
            serde_json::to_string_pretty(value).unwrap_or_else(|_| "null".to_string())
        };
        let is_dynamic_attribute =
            id.starts_with("world_attribute:") || id.starts_with("character_attribute:");
        let content = if is_dynamic_attribute {
            section_content
        } else {
            format!("【{label}】\n{section_content}")
        };
        let sent = !value.is_null()
            && !(id == "oag_context" && value.as_object().is_some_and(|map| map.is_empty()))
            && !content.trim().is_empty();
        modules.push(serde_json::json!({"id": id, "name": label, "source": source, "content": content, "editable": editable, "locked": false, "sent": sent}));
        if !sent {
            continue;
        }
        messages.push(crate::services::llm::client::ChatMessage {
            role: if id == "player_input" {
                "user".to_string()
            } else {
                "system".to_string()
            },
            content: if id == "player_input" {
                crate::models::session::build_wire_content(&content, player_media)
            } else {
                serde_json::Value::String(content)
            },
            reasoning_content: None,
            speaker: (id == "player_input").then(|| player_character_name.to_string()),
            tool_call_id: None,
            tool_calls: None,
            metadata: Some(serde_json::json!({"prompt_section_id": id})),
        });
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
        public_scene_state_lines,
        next_scene_name,
        next_location,
        visible_characters,
        kv_vars,
        player_media,
    );
    let modules = artifacts.modules;
    let prompt_call = build_prompt_call(
        "prompt_call_v2",
        "character",
        speaker_name,
        "character_response",
        "Respond to the player's input as the selected character",
        &artifacts.system_prompt,
        &artifacts.user_prompt,
        llm_chat_messages_to_values(&artifacts.messages),
        modules,
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
    prompt_ctx: &crate::services::game_engine::prompting::PromptVariableContext,
) -> String {
    let character_background_prompt = speaker_profile
        .map(|profile| {
            crate::services::game_engine::prompting::render_prompt_variables_with_context(
                &profile.background_prompt,
                prompt_ctx,
            )
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_world() -> WorldDefinition {
        WorldDefinition {
            id: "world-1".to_string(),
            name: "World".to_string(),
            genre: String::new(),
            background_prompt: String::new(),
            opening_scene: "开场".to_string(),
            summary: String::new(),
            time_system: String::new(),
            map_nodes: serde_json::json!({ "version": 1, "nodes": [] }),
            time_config: serde_json::json!({}),
            director_config: serde_json::json!({}),
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
            id: "sess-1".to_string(),
            world_name: "World".to_string(),
            location: "开场".to_string(),
            time_label: String::new(),
            current_speaker: String::new(),
            current_line: String::new(),
            player_character_id: "char-player".to_string(),
            player_character_name: "玩家".to_string(),
            visible_characters: Vec::new(),
            messages: Vec::new(),
            player_stats: Vec::new(),
            map_graph_nodes: Vec::new(),
            map_graph_edges: Vec::new(),
            system_log: Vec::new(),
            scene: SceneRuntime::default(),
            assets: AssetSelection::default(),
            state: SessionState::default(),
            generation_params: Default::default(),
        }
    }

    #[test]
    fn dynamic_attribute_sections_carry_the_attribute_key() {
        // 返回契约要求角色按准确属性键写 character_attribute_updates，而解析端
        // 对未知 key 静默丢弃：动态属性 section 的内容必须自带 key。
        let pipeline = DialoguePipeline::new();
        let world = test_world();
        let session = test_session();
        let lines = vec![serde_json::json!({
            "attribute_scope": "world",
            "key": "mortal_time",
            "description": "世界时间",
            "enum_options": [],
            "current_value": "卯时",
        })
        .to_string()];
        let artifacts = build_character_prompt_artifacts(
            &pipeline,
            &world,
            "韩立",
            None,
            &session,
            "玩家",
            "开场",
            "开场",
            "继续",
            &[],
            &[],
            &[],
            &lines,
            &[],
            "开场",
            "开场",
            &[],
            &std::collections::HashMap::new(),
            &[],
        );
        let section = artifacts
            .messages
            .iter()
            .find(|message| {
                message
                    .metadata
                    .as_ref()
                    .and_then(|meta| meta.get("prompt_section_id"))
                    .and_then(serde_json::Value::as_str)
                    == Some("world_attribute:mortal_time")
            })
            .expect("world_attribute section 应当被发送");
        let content = section.content.as_str().unwrap_or_default();
        assert!(
            content.contains("\"key\""),
            "动态属性 section 内容缺少 key 字段: {content}"
        );
        assert!(
            content.contains("mortal_time"),
            "动态属性 section 内容缺少属性键原文: {content}"
        );
    }

    #[test]
    fn chat_history_always_sent_and_memory_injected_once() {
        let pipeline = DialoguePipeline::new();
        let mut world = test_world();
        world.director_config = serde_json::json!({ "memory_mode": "embedding" });
        let session = test_session();
        let memory = MemoryEntry {
            id: "mem-1".to_string(),
            world_id: "world-1".to_string(),
            session_id: "sess-1".to_string(),
            character_id: "char-a".to_string(),
            layer: "working".to_string(),
            content: "银钥匙藏在12号柜".to_string(),
            source: "speaker_response".to_string(),
            importance: 0.8,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            turn_index: 1,
            conversation_id: None,
            event_id: None,
            item_id: None,
            scene_id: None,
            memory_type: "dialogue".to_string(),
            speaker: Some("韩立".to_string()),
            role: Some("agent".to_string()),
            location: Some("开场".to_string()),
            participants: vec!["玩家".to_string(), "韩立".to_string()],
            keywords: vec!["银钥匙".to_string()],
        };
        let build_artifacts = |world: &WorldDefinition| {
            build_character_prompt_artifacts(
                &pipeline,
                world,
                "韩立",
                None,
                &session,
                "玩家",
                "开场",
                "开场",
                "继续",
                &[],
                std::slice::from_ref(&memory),
                std::slice::from_ref(&memory),
                &[],
                &[],
                "开场",
                "开场",
                &[],
                &std::collections::HashMap::new(),
                &[],
            )
        };
        fn module<'a>(artifacts: &'a CharacterPromptArtifacts, id: &str) -> &'a serde_json::Value {
            artifacts
                .modules
                .iter()
                .find(|item| item.get("id").and_then(|value| value.as_str()) == Some(id))
                .unwrap_or_else(|| panic!("missing module {id}"))
        }

        let artifacts = build_artifacts(&world);
        assert_eq!(
            module(&artifacts, "oag_context")
                .get("sent")
                .and_then(|v| v.as_bool()),
            Some(true),
            "Embedding 模式应发送记忆注入 section"
        );
        assert!(
            module(&artifacts, "oag_context")
                .get("content")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .contains("银钥匙"),
            "记忆注入 section 应包含召回的记忆"
        );
        let chat_history = module(&artifacts, "chat_history")
            .get("content")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        assert!(
            chat_history.contains("recent_dialogue"),
            "对话历史必须始终发送"
        );
        assert!(
            !chat_history.contains("memory_context"),
            "记忆只能从 oag_context 注入一次，chat_history 不应重复"
        );
        assert_eq!(
            module(&artifacts, "fact_memory_rules")
                .get("sent")
                .and_then(|v| v.as_bool()),
            Some(false),
            "Embedding 模式不应发送事实/记忆提取契约"
        );

        // OAG 结构化层尚未接入，暂按 Embedding 召回运行；但事实抽取仍须显式开启。
        world.director_config = serde_json::json!({ "memory_mode": "oag" });
        let artifacts = build_artifacts(&world);
        assert_eq!(
            module(&artifacts, "oag_context")
                .get("sent")
                .and_then(|v| v.as_bool()),
            Some(true),
            "OAG 模式应发送注入 section（当前回退为 Embedding 召回）"
        );
        assert_eq!(
            module(&artifacts, "fact_memory_rules")
                .get("sent")
                .and_then(|v| v.as_bool()),
            Some(false),
            "OAG 模式不自动启用事实抽取"
        );
    }
}
