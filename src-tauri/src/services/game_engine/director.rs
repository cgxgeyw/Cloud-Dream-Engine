use crate::models::character::CharacterCreateRequest;
use crate::models::character::CharacterDefinition;
use crate::models::generation_params::GenerationParams;
use crate::models::interaction::{
    declared_director_interaction_kinds, validate_interaction_candidate, MessageInteraction,
    INTERACTION_STATUS_PENDING,
};
use crate::models::mcp_tool::{McpToolDefinition, MCP_TOOL_SCHEDULE_NOTIFICATION_ID, is_builtin_mcp_tool_id};
use crate::models::model_config::ModelConfig;
use crate::models::session::{ChatMessage, InventoryItem, MessageContent, SessionSnapshot};
use crate::models::world::WorldDefinition;
use crate::services::game_engine::prompting::{
    build_prompt_call, llm_chat_messages_to_values, recent_messages_text, resolve_prompt_modules,
    render_prompt_variables, resolve_runtime_context_prompt,
};
use crate::services::llm::client::{
    ChatRequest, ChatToolCall, ChatToolChoice, ChatToolDefinition, LlmClient,
};
use crate::services::map_topology::extract_scene_names;
use crate::services::notifications::{
    notification_tool_definition, pending_notification_from_tool_call, NotificationScheduler,
    NotificationToolContext, NotificationToolRuntime,
};
use std::collections::BTreeSet;

mod speaker_selection;

use self::speaker_selection::parse_planned_speakers;

#[derive(Debug, Clone, Default)]
pub struct WorldDirectorService;

/// 统一读取 director_config.allow_player_character_switch;缺省按 true(允许换角)。
/// 各处拿到 bool 后的处理语义不同(删 prompt 契约字段/解析忽略/工具效果忽略),保持不变。
fn allow_player_character_switch(world: &WorldDefinition) -> bool {
    world
        .director_config
        .get("allow_player_character_switch")
        .and_then(|value| value.as_bool())
        .unwrap_or(true)
}

fn append_runtime_attributes(
    payload: &mut serde_json::Value,
    session: &SessionSnapshot,
    runtime: &crate::models::session::SessionRuntimeAttributesResponse,
) {
    let to_records = |groups: &[crate::models::session::RuntimeAttributeGroup]| {
        groups
            .iter()
            .map(|group| {
                serde_json::json!({
                    "owner_type": group.owner_type,
                    "owner_id": group.owner_id,
                    "owner_label": group.owner_label,
                    "attributes": group.items.iter().map(|item| {
                        serde_json::json!({
                            "key": item.key,
                            "label": item.label,
                            "value_type": item.value_type,
                            "value": item.value,
                        })
                    }).collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>()
    };
    // session_character 分组的 owner_id 生产格式固定为 `{session_id}:{character_id}`
    // (orchestrator/run.rs 建组、runtime_effects.rs 写入、save_repo.rs 分支复制均如此),
    // 因此用完整 owner_id 精确匹配,避免 character_id 互为后缀时拿错组。
    let player_owner_id = format!("{}:{}", session.id, session.player_character_id);
    let player_attributes = runtime
        .character_attributes
        .iter()
        .find(|group| group.owner_id == player_owner_id)
        .map(|group| {
            group.items.iter().map(|item| {
                serde_json::json!({
                    "key": item.key,
                    "label": item.label,
                    "value_type": item.value_type,
                    "value": item.value,
                })
            }).collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let Some(current_state) = payload
        .get_mut("current_state")
        .and_then(|value| value.as_object_mut())
    else {
        return;
    };
    current_state.insert(
        "runtime_attributes".to_string(),
        serde_json::json!({
            "session": to_records(&runtime.session_attributes),
            "player": {
                "character_name": session.player_character_name,
                "attributes": player_attributes,
            },
            "characters": to_records(&runtime.character_attributes),
        }),
    );
}

#[derive(Debug, Clone)]
pub struct DirectorLoopIterationTrace {
    pub iteration: usize,
    pub request: ChatRequest,
    pub request_value: serde_json::Value,
    pub response_value: serde_json::Value,
    pub parsed: serde_json::Value,
    pub tool_enriched: serde_json::Value,
}

#[derive(Debug, Clone, Default)]
pub struct DirectorLoopRunResult {
    pub parsed: serde_json::Value,
    pub traces: Vec<DirectorLoopIterationTrace>,
    /// 最后一次模型返回是否因触达 token 上限被截断。放大预算重试后仍截断时为 true,
    /// 用于把失败报成"输出被截断"而不是误导性的"模型没返回 JSON"。
    pub truncated_by_token_limit: bool,
}

#[derive(Debug, Clone, Default)]
pub struct DirectorLoopStreamProgress {
    pub tool_enriched: serde_json::Value,
    pub reasoning: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ParsedDirectorRuntimePayload {
    pub world_phase: String,
    pub next_location: String,
    pub next_scene_name: String,
    pub current_line: Option<String>,
    pub next_scene_background_hint: Option<String>,
    pub background_asset_name: Option<String>,
    pub background_asset_path: Option<String>,
    pub background_generation_prompt: Option<String>,
    pub next_scene_tags: Vec<String>,
    pub next_time_label: String,
    pub scene_visible_characters: Option<Vec<String>>,
    pub planned_speakers: Vec<String>,
    pub generated_character_payloads: Vec<serde_json::Value>,
    pub character_visual_directives: Vec<serde_json::Value>,
    pub switch_character_proposal: Option<serde_json::Value>,
    pub interaction: Option<MessageInteraction>,
}

impl WorldDirectorService {
    pub fn new() -> Self {
        Self
    }

    #[allow(unreachable_code)]
    pub fn build_runtime_prompt_call(
        &self,
        world: &WorldDefinition,
        session: &SessionSnapshot,
        characters: &[CharacterDefinition],
        player_input: &str,
        stage: &str,
        tool_loop_messages: Option<Vec<serde_json::Value>>,
    ) -> serde_json::Value {
        self.build_runtime_prompt_call_with_mcp_tools(
            world,
            session,
            characters,
            player_input,
            stage,
            tool_loop_messages,
            &[],
            &std::collections::HashMap::new(),
            None,
            &[],
        )
    }

    pub fn build_runtime_prompt_call_with_mcp_tools(
        &self,
        world: &WorldDefinition,
        session: &SessionSnapshot,
        characters: &[CharacterDefinition],
        player_input: &str,
        stage: &str,
        tool_loop_messages: Option<Vec<serde_json::Value>>,
        mcp_tools: &[McpToolDefinition],
        kv_vars: &std::collections::HashMap<String, String>,
        runtime_attributes: Option<&crate::models::session::SessionRuntimeAttributesResponse>,
        player_media: &[crate::models::session::ContentPart],
    ) -> serde_json::Value {
        let history_rounds = self.resolve_director_history_rounds(world);
        let chat_history = self.build_history_dialogue(
            &session.messages,
            history_rounds,
            Some(session.player_character_name.as_str()),
        );
        let mut payload = self.build_runtime_turn_payload_with_mcp_tools(
            world,
            session,
            characters,
            player_input,
            chat_history.clone(),
            mcp_tools,
        );
        if let Some(attributes) = runtime_attributes {
            append_runtime_attributes(&mut payload, session, attributes);
        }
        let system_prompt = self.resolve_director_system_prompt(world);
        let runtime_context_prompt = resolve_runtime_context_prompt(world);
        let module_resolution = resolve_prompt_modules(
            world,
            "director",
            &self.template_variables(world, session, ""),
            kv_vars,
            &recent_messages_text(&session.messages, 10),
        );
        let payload_text =
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string());
        let tool_loop_messages = tool_loop_messages.unwrap_or_default();
        let mut prompt_call = build_prompt_call(
            "prompt_call_v2",
            "director",
            "world_director",
            stage,
            "Decide world state, tool calls and speaker order",
            &system_prompt,
            &payload_text,
            self.build_runtime_prompt_messages(
                &system_prompt,
                &runtime_context_prompt,
                &module_resolution.prefix,
                &module_resolution.suffix,
                &payload_text,
                tool_loop_messages.clone(),
                player_media,
            ),
            self.build_director_prompt_modules(
                &module_resolution.traces,
                world,
                session,
                characters,
                &payload,
                &system_prompt,
                &runtime_context_prompt,
            ),
            payload
                .get("response_contract")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
            serde_json::json!({
                "payload": payload,
                "tool_loop_messages": tool_loop_messages,
            }),
        );
        // 历史深度插入（第 6 项）：由 build_chat_request_from_prompt_call 在实际请求中落位。
        if !module_resolution.depth_insertions.is_empty() {
            prompt_call["depth_insertions"] = serde_json::json!(
                module_resolution
                    .depth_insertions
                    .iter()
                    .map(|(depth, content)| serde_json::json!({ "depth": depth, "content": content }))
                    .collect::<Vec<_>>()
            );
        }
        prompt_call
    }

    pub fn attach_prompt_call_result(
        &self,
        mut prompt_call: serde_json::Value,
        raw_model_return: Option<&str>,
        return_processing: serde_json::Value,
        processed_model_return: serde_json::Value,
        written_result: serde_json::Value,
    ) -> serde_json::Value {
        if let Some(object) = prompt_call.as_object_mut() {
            object.insert(
                "raw_model_return".to_string(),
                raw_model_return
                    .map(|value| serde_json::Value::String(value.to_string()))
                    .unwrap_or(serde_json::Value::Null),
            );
            object.insert("return_processing".to_string(), return_processing);
            object.insert("processed_model_return".to_string(), processed_model_return);
            object.insert("written_result".to_string(), written_result);
        }
        prompt_call
    }

    #[allow(unreachable_code)]
    fn resolve_director_system_prompt(&self, world: &WorldDefinition) -> String {
        render_prompt_variables(&world.director_runtime_system_prompt)
            .trim()
            .to_string()
    }

    fn build_runtime_prompt_messages(
        &self,
        system_prompt: &str,
        runtime_context_prompt: &str,
        prefix_contents: &[String],
        suffix_contents: &[String],
        user_prompt: &str,
        tool_loop_messages: Vec<serde_json::Value>,
        player_media: &[crate::models::session::ContentPart],
    ) -> Vec<serde_json::Value> {
        // system_prefix 模块在核心系统提示之前，system_suffix 在其后。
        let mut messages: Vec<serde_json::Value> = prefix_contents
            .iter()
            .map(|content| serde_json::json!({ "role": "system", "content": content }))
            .collect();
        messages.push(serde_json::json!({
            "role": "system",
            "content": system_prompt,
        }));
        for content in suffix_contents {
            messages.push(serde_json::json!({
                "role": "system",
                "content": content,
            }));
        }
        if !runtime_context_prompt.trim().is_empty() {
            messages.push(serde_json::json!({
                "role": "system",
                "content": runtime_context_prompt,
            }));
        }
        messages.push(
            serde_json::json!({
                "role": "user",
                // 第 10 项：当前回合玩家附件以 multipart 随本条消息下发（仅当前回合）；
                // 无附件时 content 保持字符串，行为与纯文本时代一致。
                "content": crate::models::session::build_wire_content(user_prompt, player_media),
            }),
        );
        messages.extend(tool_loop_messages);
        messages
    }

    fn build_director_prompt_modules(
        &self,
        module_traces: &[serde_json::Value],
        _world: &WorldDefinition,
        _session: &SessionSnapshot,
        _characters: &[CharacterDefinition],
        payload: &serde_json::Value,
        system_prompt: &str,
        runtime_context_prompt: &str,
    ) -> Vec<serde_json::Value> {
        let mut modules = module_traces.to_vec();
        modules.push(serde_json::json!({
            "name": "world_director_prompt",
            "source": "world.director_runtime_system_prompt",
            "content": system_prompt,
            "editable": true,
            "sent": true
        }));
        if !runtime_context_prompt.trim().is_empty() {
            modules.push(serde_json::json!({
                "name": "runtime_context",
                "source": "world.director_config.runtime_context_prompt",
                "content": runtime_context_prompt,
                "editable": true,
                "sent": true
            }));
        }
        modules.push(serde_json::json!({
            "name": "basic_setting",
            "source": "runtime_payload.basic_setting",
            "content": serde_json::to_string_pretty(payload.get("basic_setting").unwrap_or(&serde_json::Value::Null)).unwrap_or_else(|_| "{}".to_string()),
            "editable": false,
            "sent": true
        }));
        modules.push(serde_json::json!({
            "name": "current_state",
            "source": "runtime_payload.current_state",
            "content": serde_json::to_string_pretty(payload.get("current_state").unwrap_or(&serde_json::Value::Null)).unwrap_or_else(|_| "{}".to_string()),
            "editable": false,
            "sent": true
        }));
        modules.push(serde_json::json!({
            "name": "chat_history",
            "source": "runtime_payload.chat_history",
            "content": serde_json::to_string_pretty(payload.get("chat_history").unwrap_or(&serde_json::Value::Null)).unwrap_or_else(|_| "[]".to_string()),
            "editable": false,
            "sent": true
        }));
        modules.push(serde_json::json!({
            "name": "tool_data",
            "source": "runtime_payload.tool_data",
            "content": serde_json::to_string_pretty(payload.get("tool_data").unwrap_or(&serde_json::Value::Null)).unwrap_or_else(|_| "{}".to_string()),
            "editable": false,
            "sent": true
        }));
        modules.push(serde_json::json!({
            "name": "response_contract",
            "source": "runtime_payload.response_contract",
            "content": serde_json::to_string_pretty(payload.get("response_contract").unwrap_or(&serde_json::Value::Null)).unwrap_or_else(|_| "{}".to_string()),
            "editable": false,
            "sent": true
        }));
        modules
    }

    pub fn build_runtime_turn_payload(
        &self,
        world: &WorldDefinition,
        session: &SessionSnapshot,
        characters: &[CharacterDefinition],
        player_input: &str,
        chat_history: Vec<serde_json::Value>,
    ) -> serde_json::Value {
        self.build_runtime_turn_payload_with_mcp_tools(
            world,
            session,
            characters,
            player_input,
            chat_history,
            &[],
        )
    }

    pub fn build_runtime_turn_payload_with_mcp_tools(
        &self,
        world: &WorldDefinition,
        session: &SessionSnapshot,
        characters: &[CharacterDefinition],
        player_input: &str,
        chat_history: Vec<serde_json::Value>,
        mcp_tools: &[McpToolDefinition],
    ) -> serde_json::Value {
        let world_character_roster = characters
            .iter()
            .map(|character| character.name.clone())
            .collect::<Vec<_>>();
        let visual_capabilities = self.build_visual_capabilities(world, session, characters);

        // basic_setting：空值字段不发(对齐角色侧瘦身标准)。
        let mut basic_setting = serde_json::Map::new();
        basic_setting.insert("world_name".to_string(), serde_json::json!(session.world_name));
        if !world.background_prompt.trim().is_empty() {
            basic_setting.insert(
                "background_prompt".to_string(),
                serde_json::json!(world.background_prompt),
            );
        }
        if !world_character_roster.is_empty() {
            basic_setting.insert(
                "world_character_roster".to_string(),
                serde_json::json!(world_character_roster),
            );
        }
        if !world.time_system.trim().is_empty() {
            basic_setting.insert("time_system".to_string(), serde_json::json!(world.time_system));
        }

        // current_state：空值字段不发；inventory 删内部 UUID。
        let mut current_state = serde_json::Map::new();
        current_state.insert("player_input".to_string(), serde_json::json!(player_input));
        current_state.insert(
            "player_character_name".to_string(),
            serde_json::json!(session.player_character_name),
        );
        if !session.location.trim().is_empty() {
            current_state.insert("location".to_string(), serde_json::json!(session.location));
        }
        if !session.time_label.trim().is_empty() {
            current_state.insert("time_label".to_string(), serde_json::json!(session.time_label));
        }
        if !session.visible_characters.is_empty() {
            current_state.insert(
                "current_scene_character_roster".to_string(),
                serde_json::json!(session.visible_characters),
            );
        }
        if !session.scene.temporary_tags.is_empty() {
            current_state.insert(
                "scene_tags".to_string(),
                serde_json::json!(session.scene.temporary_tags),
            );
        }
        if !session.state.metrics.is_empty() {
            current_state.insert(
                "state_metrics".to_string(),
                serde_json::json!(session.state.metrics),
            );
        }
        let inventory_items = build_director_inventory_records(&session.inventory_items);
        if !inventory_items.is_empty() {
            current_state.insert("inventory_items".to_string(), serde_json::json!(inventory_items));
        }

        // tool_data：visual_capabilities 为空时不发。
        let mut tool_data = serde_json::Map::new();
        tool_data.insert(
            "available_tools".to_string(),
            serde_json::json!(self.build_director_tool_capabilities(world, mcp_tools)),
        );
        tool_data.insert("tool_protocol".to_string(), self.build_tool_protocol());
        if !visual_capabilities.is_null() {
            tool_data.insert("visual_capabilities".to_string(), visual_capabilities);
        }

        let allow_player_character_switch = allow_player_character_switch(world);
        let mut payload = serde_json::json!({
            "basic_setting": basic_setting,
            "current_state": current_state,
            "chat_history": chat_history,
            "tool_data": tool_data,
            "response_contract": {
                "required_style": "json_only",
                "return_policy": "return_changed_fields_only",
                "core_fields": [
                    "planned_speakers",
                    "switch_character_proposal",
                ],
                "optional_fields_when_changed": [
                    "world_phase",
                    "next_scene_name",
                    "next_location",
                    "next_time_label",
                    "scene_visible_characters",
                    "generated_characters",
                    "current_line",
                    "next_scene_background_hint",
                    "next_scene_tags",
                    "character_visual_directives",
                    "inventory_items",
                    "session_attribute_updates",
                    "character_attribute_updates"
                ],
                "how_to_fill": "Return a single JSON object. Always include planned_speakers (even if empty). Include switch_character_proposal and every optional field ONLY when it applies / changes this turn; omit unchanged fields entirely rather than echoing the current state back. Never resend basic_setting, current_state, or chat_history. See field_guide for what each field means and when to send it.",
                "field_guide": {
                    "planned_speakers": "CORE, return every turn. Ordered list of character names who should speak this turn, in speaking order. Never include the player character (they are implicitly present). Use only names already in current_scene_character_roster, or names you create via generated_characters in this same response. Return an empty array if no character should speak.",
                    "switch_character_proposal": "CORE field, but return it ONLY when the player clearly asks to take control of / play as a different character. Object: { target_character_name, reason, and optionally location, scene_name, scene_background_hint, scene_tags, visible_characters if the switch also moves the scene }. Omit entirely on normal turns.",
                    "world_phase": "Narrative tension stage. Must be exactly one of: \"opening\", \"escalation\", \"crisis\". Return only when the phase advances; any other value is ignored.",
                    "next_scene_name": "Optional short label for the new scene. Return only on a scene transition; it may be freely generated.",
                    "next_location": "Optional rough location label for the new scene. Keep it short and practical; return only when the place changes.",
                    "next_time_label": "New in-world time label (e.g. a clock time or time-of-day). Return only when time advances.",
                    "scene_visible_characters": "The COMPLETE list of characters present in the scene AFTER this turn (this replaces the current roster, it is not appended). Exclude the player character. Return only when the on-stage cast changes.",
                    "generated_characters": "Create brand-new characters here BEFORE naming them in scene_visible_characters or planned_speakers. Each item requires name, role, background_prompt (a usable portrayal brief for the later character model, not a one-word label). Return only when introducing someone not already in current_scene_character_roster or world_character_roster.",
                    "current_line": "A single non-dialogue narration/stage line (scene description, ambience, transition). Do NOT put character speech here — character dialogue is produced separately. Return only when a non-dialogue scene update is genuinely needed.",
                    "next_scene_background_hint": "Short description of the new scene's visual background. Return only on a scene change.",
                    "next_scene_tags": "Atmosphere/state tags for the new scene. Return only when the tags change.",
                    "character_visual_directives": "Per-character visual/portrait directives. Return only when a character's visual state should change.",
                    "inventory_items": "The COMPLETE player inventory after this turn. Array items use { item_id, name, category, quantity, description, tags, owner_type, owner_id, visibility, disclosed_to }. Return only when an item is gained, lost, consumed, transferred, or its quantity changes.",
                    "session_attribute_updates": "Updates to session/world runtime custom attributes. Array of { key, value } where key matches an existing attribute schema key. Return only when a value changes.",
                    "character_attribute_updates": "Updates to the current player or a scene character's runtime custom attributes. Array of { character_name, key, value } where key exactly matches a key listed in current_state.runtime_attributes. Return changes caused by this turn, including health, stamina, resources, equipment, techniques, progression, or other declared state."
                },
                "runtime_update_format": {
                    "session_attribute_updates": [
                        { "key": "attribute_key", "value": "new_value" }
                    ],
                    "character_attribute_updates": [
                        { "character_name": "target_character_name", "key": "attribute_key", "value": "new_value" }
                    ]
                },
                "forbidden_fields": [
                    "state_tags",
                    "system_messages",
                    "system_log"
                ],
                "notes": [
                    "Omit unchanged fields.",
                    "Do not rebuild the full session state.",
                    "Only include current_line when a non-dialogue scene update is necessary.",
                    "Use session_attribute_updates to modify session/world runtime custom attributes by schema key.",
                    "Use character_attribute_updates to modify the current player or a scene character by character_name and an exact schema key from current_state.runtime_attributes.",
                    "When an action has a concrete cost or consequence, update every affected runtime attribute in the same turn instead of describing the change only in prose.",
                    "Do not include the player character name in scene_visible_characters or planned_speakers; the player is implicitly present."
                ]
            }
        });
        if !allow_player_character_switch {
            if let Some(response_contract) = payload
                .get_mut("response_contract")
                .and_then(|value| value.as_object_mut())
            {
                if let Some(core_fields) = response_contract
                    .get_mut("core_fields")
                    .and_then(|value| value.as_array_mut())
                {
                    core_fields.retain(|field| field.as_str() != Some("switch_character_proposal"));
                }
                if let Some(field_guide) = response_contract
                    .get_mut("field_guide")
                    .and_then(|value| value.as_object_mut())
                {
                    field_guide.remove("switch_character_proposal");
                }
                response_contract.insert(
                    "how_to_fill".to_string(),
                    serde_json::json!("Return a single JSON object. Always include planned_speakers (even if empty). Include optional fields ONLY when they apply / change this turn; omit unchanged fields entirely rather than echoing the current state back. Never resend basic_setting, current_state, or chat_history. See field_guide for what each field means and when to send it."),
                );
            }
        }
        payload
    }

    fn build_director_response_schema(&self, world: &WorldDefinition) -> serde_json::Value {
        let mut schema = serde_json::json!({
            "type": "object",
            "additionalProperties": true,
            "required": ["planned_speakers"],
            "properties": {
                "world_phase": { "type": "string" },
                "next_scene_name": { "type": "string" },
                "next_location": { "type": "string" },
                "next_time_label": { "type": "string" },
                "current_line": { "type": "string" },
                "next_scene_background_hint": { "type": "string" },
                "next_scene_tags": {
                    "type": "array",
                    "items": { "type": "string" }
                },
                "scene_visible_characters": {
                    "type": "array",
                    "items": { "type": "string" }
                },
                "planned_speakers": {
                    "type": "array",
                    "items": { "type": "string" }
                },
                "generated_characters": {
                    "type": "array",
                    "description": "Create new characters before referencing them in scene_visible_characters or planned_speakers.",
                    "items": {
                        "type": "object",
                        "required": ["name", "role", "background_prompt"],
                        "additionalProperties": true,
                        "properties": {
                            "name": {
                                "type": "string",
                                "description": "New character name. Must exactly match the later visible/planned name."
                            },
                            "role": {
                                "type": "string",
                                "description": "Identity and dramatic function in this world."
                            },
                            "background_prompt": {
                                "type": "string",
                                "description": "Usable portrayal brief for the future character model. Include identity, relation to the current scene/player, speaking style, and current stance or goal."
                            },
                            "memory_strategy": { "type": "string" },
                            "recent_dialogue_rounds": { "type": "integer" },
                            "attributes": {
                                "type": "array",
                                "items": { "type": "string" }
                            },
                            "model": { "type": "string" }
                        }
                    }
                },
                "switch_character_proposal": {
                    "type": "object",
                    "properties": {
                        "target_character_name": { "type": "string" },
                        "reason": { "type": "string" },
                        "location": { "type": "string" },
                        "scene_name": { "type": "string" },
                        "scene_background_hint": { "type": "string" },
                        "scene_tags": {
                            "type": "array",
                            "items": { "type": "string" }
                        },
                        "visible_characters": {
                            "type": "array",
                            "items": { "type": "string" }
                        }
                    }
                },
                "character_visual_directives": {
                    "type": "array"
                },
                "inventory_items": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["name", "quantity"],
                        "additionalProperties": true,
                        "properties": {
                            "item_id": { "type": "string" },
                            "name": { "type": "string" },
                            "category": { "type": "string" },
                            "quantity": { "type": "integer" },
                            "description": { "type": "string" },
                            "tags": { "type": "array", "items": { "type": "string" } },
                            "owner_type": { "type": "string" },
                            "owner_id": { "type": "string" },
                            "visibility": { "type": "string" },
                            "disclosed_to": { "type": "array", "items": { "type": "string" } }
                        }
                    }
                },
                "session_attribute_updates": {
                    "type": "array",
                    "description": "Runtime custom attribute updates for the current session. key must match an attribute schema key.",
                    "items": {
                        "type": "object",
                        "required": ["key", "value"],
                        "additionalProperties": true,
                        "properties": {
                            "key": { "type": "string" },
                            "value": {}
                        }
                    }
                },
                "character_attribute_updates": {
                    "type": "array",
                    "description": "Runtime custom attribute updates for session characters. key must match an attribute schema key.",
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
                }
            }
        });
        let kinds = declared_director_interaction_kinds(&world.director_config);
        if !kinds.is_empty() {
            schema["properties"]["interaction"] = serde_json::json!({
                "type": "object",
                "description": "A pending player action issued by the world director, not by a character. Use only for a meaningful decision that must be resolved before any character speaks. When present, planned_speakers must be an empty array.",
                "required": ["kind", "prompt", "config"],
                "additionalProperties": false,
                "properties": {
                    "kind": { "type": "string", "enum": kinds },
                    "prompt": { "type": "string" },
                    "config": { "type": "object", "additionalProperties": true }
                }
            });
        }
        let allow_player_character_switch = allow_player_character_switch(world);
        if !allow_player_character_switch {
            if let Some(properties) = schema
                .get_mut("properties")
                .and_then(|value| value.as_object_mut())
            {
                properties.remove("switch_character_proposal");
            }
        }
        schema
    }

    pub fn build_chat_request_from_prompt_call(
        &self,
        prompt_call: &serde_json::Value,
        world: &WorldDefinition,
        model_id: &str,
        generation: &GenerationParams,
        stream_enabled: bool,
    ) -> ChatRequest {
        let mut messages: Vec<crate::services::llm::client::ChatMessage> = prompt_call
            .get("messages")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|message| {
                let role = message.get("role")?.as_str()?.to_string();
                // content 鍙互鏄瓧绗︿覆鎴栨暟缁勶紙澶氬獟浣撳唴瀹癸級
                let content = message.get("content")?.clone();
                Some(crate::services::llm::client::ChatMessage {
                    role,
                    content,
                    reasoning_content: None,
                    speaker: None,
                    tool_call_id: None,
                    tool_calls: None,
                    metadata: None,
                })
            })
            .collect::<Vec<_>>();
        // 历史深度插入（第 6 项）：depth:N 表示插到距末尾 N 条的位置。
        if let Some(insertions) = prompt_call
            .get("depth_insertions")
            .and_then(|value| value.as_array())
        {
            for insertion in insertions {
                let depth = insertion
                    .get("depth")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(0) as usize;
                let content = insertion
                    .get("content")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                if content.is_empty() {
                    continue;
                }
                let index = messages.len().saturating_sub(depth);
                messages.insert(
                    index,
                    crate::services::llm::client::ChatMessage {
                        role: "system".to_string(),
                        content: serde_json::Value::String(content),
                        reasoning_content: None,
                        speaker: None,
                        tool_call_id: None,
                        tool_calls: None,
                        metadata: None,
                    },
                );
            }
        }
        let tools = prompt_call
            .get("raw_debug")
            .and_then(|value| value.get("payload"))
            .and_then(|value| value.get("tool_data"))
            .and_then(|value| value.get("available_tools"))
            .and_then(|value| value.as_array())
            .map(|items| tool_capabilities_to_chat_definitions(items))
            .filter(|items| !items.is_empty());
        let native_tools_active = tools
            .as_ref()
            .map(|items| !items.is_empty())
            .unwrap_or(false);
        ChatRequest {
            model: model_id.to_string(),
            messages,
            generation: generation.clone(),
            stream: Some(stream_enabled),
            json_mode: Some(true),
            response_schema: Some(self.build_director_response_schema(world)),
            tools,
            tool_choice: native_tools_active.then_some(ChatToolChoice::Auto),
            native_tool_calling: native_tools_active.then_some(true),
        }
    }

    pub fn build_prompt_trace(
        &self,
        request_messages: &[crate::services::llm::client::ChatMessage],
        request_value: &serde_json::Value,
        response_value: &serde_json::Value,
        parsed: &serde_json::Value,
        tool_enriched: &serde_json::Value,
        iteration: usize,
        world: &WorldDefinition,
        session: &SessionSnapshot,
        characters: &[CharacterDefinition],
        provider: &str,
        model: &ModelConfig,
        player_input: &str,
        loop_limit: usize,
        stage: &str,
        generation: &GenerationParams,
    ) -> serde_json::Value {
        let system_prompt = request_messages
            .first()
            .map(|message| message.content_text())
            .unwrap_or_default();
        let user_prompt = request_messages
            .iter()
            .rev()
            .find(|message| message.role == "user")
            .map(|message| message.content_text())
            .unwrap_or_default();
        let payload = serde_json::from_str::<serde_json::Value>(&user_prompt).unwrap_or_else(|_| {
            let history_rounds = self.resolve_director_history_rounds(world);
            let chat_history = self.build_history_dialogue(
                &session.messages,
                history_rounds,
                Some(session.player_character_name.as_str()),
            );
            self.build_runtime_turn_payload(world, session, characters, player_input, chat_history)
        });
        let raw_model_return = self.extract_raw_model_return_text(response_value);
        let return_processing = self.apply_return_processing(world, &raw_model_return);
        let processed_model_return = return_processing
            .get("after")
            .and_then(|value| value.as_str())
            .map(|text| self.parse_loose_json(text))
            .unwrap_or_else(|| parsed.clone());
        let tool_loop_messages = request_messages
            .iter()
            .filter(|message| {
                message.role == "tool"
                    || message
                        .metadata
                        .as_ref()
                        .and_then(|meta| meta.get("tool_phase"))
                        .and_then(|value| value.as_bool())
                        .unwrap_or(false)
            })
            .map(|message| {
                serde_json::json!({
                    "role": message.role,
                    "content": message.content,
                    "metadata": message.metadata,
                })
            })
            .collect::<Vec<_>>();
        let prompt_call = build_prompt_call(
            "prompt_call_v2",
            "director",
            "world_director",
            stage,
            if iteration == 1 {
                "Decide next scene state and planned speakers"
            } else {
                "Resolve director tool loop iteration before final world-state decision"
            },
            &system_prompt,
            &user_prompt,
            llm_chat_messages_to_values(request_messages),
            self.build_director_prompt_modules(
                &resolve_prompt_modules(
                    world,
                    "director",
                    &self.template_variables(world, session, ""),
                    &std::collections::HashMap::new(),
                    &recent_messages_text(&session.messages, 10),
                )
                .traces,
                world,
                session,
                characters,
                &payload,
                &self.resolve_director_system_prompt(world),
                &resolve_runtime_context_prompt(world),
            ),
            payload
                .get("response_contract")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
            serde_json::json!({
                "payload": payload,
                "iteration": iteration,
                "provider": provider,
                "base_url": model.base_url,
                "model_id": model.model_id,
                "request": request_value,
                "response": response_value,
                // 第 8 项：本次实际使用的采样参数与被 provider 过滤掉的项。
                "request_params": crate::services::llm::param_support::describe_params_for_trace(
                    provider,
                    generation,
                    serde_json::json!({ "json_mode": true }),
                ),
                "loop_limit": loop_limit,
                "loop_iterations": iteration,
                "tool_calls": parsed.get("tool_calls").cloned().unwrap_or_else(|| serde_json::Value::Array(vec![])),
                "tool_results": tool_enriched.get("tool_results").cloned().unwrap_or_else(|| serde_json::Value::Array(vec![])),
                "tool_loop_messages": tool_loop_messages,
            }),
        );
        self.attach_prompt_call_result(
            prompt_call,
            if raw_model_return.trim().is_empty() {
                None
            } else {
                Some(raw_model_return.as_str())
            },
            return_processing,
            processed_model_return,
            tool_enriched.clone(),
        )
    }

    fn extract_raw_model_return_text(&self, response_value: &serde_json::Value) -> String {
        response_value
            .get("response")
            .and_then(|value| value.get("content"))
            .and_then(|value| value.as_str())
            .map(|value| value.to_string())
            .unwrap_or_default()
    }

    fn apply_return_processing(
        &self,
        world: &WorldDefinition,
        raw_text: &str,
    ) -> serde_json::Value {
        let mut text = raw_text.to_string();
        let mut applied_rules = Vec::new();
        let mut rules = world
            .director_config
            .get("return_processing_rules")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|item| {
                let object = item.as_object()?;
                let enabled = object
                    .get("enabled")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(true);
                let scope = object
                    .get("scope")
                    .and_then(|value| value.as_str())
                    .unwrap_or("both")
                    .trim()
                    .to_string();
                let pattern = object
                    .get("pattern")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_string();
                let replacement = object
                    .get("replacement")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_string();
                let order = object
                    .get("order")
                    .and_then(|value| value.as_i64())
                    .unwrap_or(0);
                let name = object
                    .get("name")
                    .and_then(|value| value.as_str())
                    .unwrap_or("unnamed_rule")
                    .to_string();
                if !enabled
                    || !(scope == "both" || scope == "director")
                    || pattern.trim().is_empty()
                {
                    return None;
                }
                Some((order, name, pattern, replacement))
            })
            .collect::<Vec<_>>();
        rules.sort_by_key(|(order, _, _, _)| *order);
        for (_, name, pattern, replacement) in rules {
            match regex::Regex::new(&pattern) {
                Ok(re) => {
                    let count = re.find_iter(&text).count();
                    if count > 0 {
                        text = re.replace_all(&text, replacement.as_str()).to_string();
                        applied_rules.push(serde_json::json!({
                            "name": name,
                            "pattern": pattern,
                            "replacement": replacement,
                            "count": count,
                        }));
                    }
                }
                Err(err) => {
                    applied_rules.push(serde_json::json!({
                        "name": name,
                        "pattern": pattern,
                        "error": err.to_string(),
                        "count": 0,
                    }));
                }
            }
        }
        serde_json::json!({
            "before": raw_text,
            "after": text,
            "applied_rules": applied_rules,
        })
    }

    pub async fn run_director_tool_loop(
        &self,
        llm: &LlmClient,
        provider: &str,
        model: &ModelConfig,
        session: &SessionSnapshot,
        world: &WorldDefinition,
        characters: &[CharacterDefinition],
        initial_request: ChatRequest,
        turn_index: i32,
        mcp_tools: &[McpToolDefinition],
        mcp_servers: &[crate::models::mcp_server::McpServerConfig],
        notification_runtime: Option<NotificationToolRuntime<'_>>,
        mut progress_callback: Option<&mut (dyn FnMut(DirectorLoopStreamProgress) + Send)>,
    ) -> Result<DirectorLoopRunResult, String> {
        let mut active_request = initial_request;
        let mut traces = Vec::new();
        let mut json_repair_attempts = 0usize;
        let mut token_limit_retries = 0usize;
        loop {
            let started = std::time::Instant::now();
            let request_used = active_request.clone();
            let response = if model.streaming_enabled {
                let mut streamed_raw_response = String::new();
                let mut streamed_reasoning = String::new();
                match llm
                    .chat_completion_stream(
                        provider,
                        &model.base_url,
                        &model.api_key,
                        &active_request,
                        |chunk| {
                            if let Some(reasoning_delta) = chunk.reasoning_delta.as_deref() {
                                streamed_reasoning.push_str(reasoning_delta);
                            }
                            if !chunk.delta.is_empty() {
                                streamed_raw_response.push_str(&chunk.delta);
                            }
                            if let Some(callback) = progress_callback.as_deref_mut() {
                                let parsed = self.parse_loose_json(&streamed_raw_response);
                                // 流式局部解析仅用于 UI 预览，不触发 MCP 副作用。
                                let tool_enriched = self
                                    .apply_tool_call_effects(&parsed, session, world, characters);
                                callback(DirectorLoopStreamProgress {
                                    tool_enriched,
                                    reasoning: if streamed_reasoning.trim().is_empty() {
                                        None
                                    } else {
                                        Some(streamed_reasoning.clone())
                                    },
                                });
                            }
                        },
                    )
                    .await
                {
                    Ok(response) => {
                        let parsed_stream_response = self.parse_loose_json(&response.content);
                        if response.content.trim().is_empty() || !parsed_stream_response.is_object()
                        {
                            llm.chat_completion(
                                provider,
                                &model.base_url,
                                &model.api_key,
                                &active_request,
                            )
                            .await?
                        } else {
                            response
                        }
                    }
                    Err(_) => {
                        llm.chat_completion(
                            provider,
                            &model.base_url,
                            &model.api_key,
                            &active_request,
                        )
                        .await?
                    }
                }
            } else {
                llm.chat_completion(provider, &model.base_url, &model.api_key, &active_request)
                    .await?
            };
            let parsed_body = self.parse_loose_json(&response.content);
            let parsed_body = self.remove_response_body_tool_calls(&parsed_body);
            let parsed = self.merge_native_tool_calls(&parsed_body, response.tool_calls.as_deref());
            let request_value = serde_json::json!({
                "provider": provider,
                "base_url": model.base_url,
                "model_id": model.model_id,
                // 第 10 项：multipart 消息中的媒体 base64 在 trace 里只留摘要。
                "request": crate::services::game_engine::prompting::redact_request_value_for_trace(&request_used),
            });
            let response_value = serde_json::json!({
                "provider": provider,
                "model_id": model.model_id,
                "status": "completed",
                "latency_ms": started.elapsed().as_millis() as i64,
                "response": serde_json::to_value(&response).unwrap_or_default(),
            });
            // H7: schedule_notification 工具需要 DB。统一在此 async 层向主 AppState 加锁取连接,
            // 传入(同步的)效果处理函数,避免其内部 Database::new() 另开独立连接。
            use tauri::Manager;
            let notification_state = notification_runtime
                .as_ref()
                .map(|runtime| runtime.app.state::<crate::state::AppState>());
            let notification_db_guard = match notification_state.as_ref() {
                Some(state) => Some(state.db.lock().await),
                None => None,
            };
            let runtime_for_iter = notification_runtime.as_ref().map(|runtime| {
                NotificationToolRuntime {
                    app: runtime.app,
                    data_dir: runtime.data_dir,
                }
            });
            // 第 7 项：自定义 MCP 工具需要 await，先在此异步层执行，再把结果交给同步效果函数。
            let mcp_results = self
                .execute_pending_mcp_tool_calls(&parsed, world, mcp_tools, mcp_servers)
                .await;
            let tool_enriched = self.apply_tool_call_effects_with_notifications(
                &parsed,
                session,
                world,
                characters,
                runtime_for_iter,
                notification_db_guard.as_ref().map(|guard| guard.conn()),
                turn_index,
                &mcp_results,
            );
            drop(notification_db_guard);
            let iteration = traces.len() + 1;
            traces.push(DirectorLoopIterationTrace {
                iteration,
                request: active_request.clone(),
                request_value,
                response_value,
                parsed: parsed.clone(),
                tool_enriched: tool_enriched.clone(),
            });
            if !self.should_continue_tool_loop(world, &parsed, iteration) {
                // 输出被 max_tokens 截断(推理模型把预算烧在思考上,content 为空)时,
                // 必须先放大预算原样重试。此分支必须排在 JSON 修复之前:修复是往同一
                // 请求追加消息,prompt 更长而预算不变,只会再截断一次。
                if director_output_needs_json_repair(&parsed)
                    && response.is_truncated_by_token_limit()
                    && token_limit_retries < DIRECTOR_TOKEN_LIMIT_RETRIES
                {
                    if let Some(grown_request) = grow_token_budget(&active_request) {
                        token_limit_retries += 1;
                        active_request = grown_request;
                        continue;
                    }
                }
                // 导演最终输出不是合法 JSON 对象时,把坏输出和解析错误反馈给模型重出,
                // 最多 DIRECTOR_JSON_REPAIR_ATTEMPTS 轮;仍失败则原样返回,由
                // validate_director_payload 走 json_parse_failed 路径。
                if director_output_needs_json_repair(&parsed)
                    && json_repair_attempts < DIRECTOR_JSON_REPAIR_ATTEMPTS
                {
                    json_repair_attempts += 1;
                    active_request =
                        build_director_json_repair_request(&active_request, &response.content);
                    continue;
                }
                return Ok(DirectorLoopRunResult {
                    parsed: tool_enriched,
                    traces,
                    truncated_by_token_limit: response.is_truncated_by_token_limit(),
                });
            }
            active_request = self.build_tool_followup_request(
                &active_request,
                &parsed,
                &tool_enriched,
                response
                    .tool_calls
                    .as_ref()
                    .map(|calls| !calls.is_empty())
                    .unwrap_or(false),
                response.reasoning.clone(),
            )?;
        }
    }

    pub fn apply_tool_call_effects(
        &self,
        parsed: &serde_json::Value,
        session: &SessionSnapshot,
        world: &WorldDefinition,
        characters: &[CharacterDefinition],
    ) -> serde_json::Value {
        self.apply_tool_call_effects_with_notifications(
            parsed,
            session,
            world,
            characters,
            None,
            None,
            0,
            &std::collections::HashMap::new(),
        )
    }

    /// 自定义 MCP 工具的异步预执行（第 7 项）。
    ///
    /// 同步的效果处理函数无法 await，因此在这里先按世界白名单把自定义工具真正调用一遍，
    /// 结果按 tool_calls 下标回填（与效果函数使用同一套 extract_tool_calls + limit，下标对齐）。
    pub async fn execute_pending_mcp_tool_calls(
        &self,
        parsed: &serde_json::Value,
        world: &WorldDefinition,
        mcp_tools: &[McpToolDefinition],
        servers: &[crate::models::mcp_server::McpServerConfig],
    ) -> std::collections::HashMap<usize, serde_json::Value> {
        let mut results = std::collections::HashMap::new();
        let tool_calls = self.extract_tool_calls(parsed, Some(self.resolve_tool_call_limit(world)));
        if tool_calls.is_empty() {
            return results;
        }
        let allowed = self
            .resolve_world_allowed_tool_ids(world)
            .into_iter()
            .collect::<BTreeSet<_>>();
        for (index, tool_call) in tool_calls.iter().enumerate() {
            let Some(tool_name) = tool_call
                .get("tool_name")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            if is_builtin_tool_name(tool_name) {
                continue;
            }
            let arguments = tool_call
                .get("arguments")
                .or_else(|| tool_call.get("args"))
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            // 只允许世界包已授权、且已绑定可用 server 的工具。
            let matched = mcp_tools.iter().find(|tool| {
                tool.tool_name.trim() == tool_name
                    && allowed.contains(&tool.id)
                    && !is_builtin_mcp_tool_id(&tool.id)
            });
            let Some(tool) = matched else {
                results.insert(
                    index,
                    serde_json::json!({
                        "ok": false,
                        "error": format!("工具未在本世界授权或未配置：{tool_name}"),
                    }),
                );
                continue;
            };
            // 本地工具（builtin_http）由核心直接执行，不需要 MCP server。
            if crate::models::mcp_tool::is_local_tool(tool) {
                let outcome =
                    crate::services::mcp::execute_local_http_tool(tool, arguments).await;
                results.insert(
                    index,
                    serde_json::json!({
                        "ok": outcome.ok,
                        "result": outcome.result,
                        "error": outcome.error,
                        "truncated": outcome.truncated,
                        "server_name": "builtin-local",
                    }),
                );
                continue;
            }
            let server = servers.iter().find(|server| server.id == tool.server_id);
            let Some(server) = server else {
                results.insert(
                    index,
                    serde_json::json!({
                        "ok": false,
                        "error": format!("工具 {tool_name} 未绑定 MCP server，无法执行"),
                    }),
                );
                continue;
            };
            let outcome =
                crate::services::mcp::execute_mcp_tool_call(tool, server, arguments).await;
            results.insert(
                index,
                serde_json::json!({
                    "ok": outcome.ok,
                    "result": outcome.result,
                    "error": outcome.error,
                    "truncated": outcome.truncated,
                    "server_name": server.name.clone(),
                }),
            );
        }
        results
    }

    fn apply_tool_call_effects_with_notifications(
        &self,
        parsed: &serde_json::Value,
        session: &SessionSnapshot,
        world: &WorldDefinition,
        characters: &[CharacterDefinition],
        notification_runtime: Option<NotificationToolRuntime<'_>>,
        notification_conn: Option<&rusqlite::Connection>,
        turn_index: i32,
        // execute_pending_mcp_tool_calls 的产物，按 tool_calls 下标对齐。
        mcp_results: &std::collections::HashMap<usize, serde_json::Value>,
    ) -> serde_json::Value {
        let tool_calls = self.extract_tool_calls(parsed, Some(self.resolve_tool_call_limit(world)));
        if tool_calls.is_empty() {
            return parsed.clone();
        }
        let mut merged = parsed.as_object().cloned().unwrap_or_default();
        let mut tool_results = merged
            .get("tool_results")
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default();
        let mut pending_notifications = merged
            .get("pending_notifications")
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default();
        let schedule_notification_allowed = self
            .resolve_world_allowed_tool_ids(world)
            .iter()
            .any(|id| id == MCP_TOOL_SCHEDULE_NOTIFICATION_ID);
        for (tool_call_index, tool_call) in tool_calls.iter().enumerate() {
            let Some(tool_call_obj) = tool_call.as_object() else {
                continue;
            };
            let tool_name = tool_call_obj
                .get("tool_name")
                .and_then(|value| value.as_str())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            let Some(tool_name) = tool_name else {
                continue;
            };
            let call_id = tool_call_obj
                .get("id")
                .and_then(|value| value.as_str())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| format!("{}_{}", tool_name, uuid::Uuid::new_v4()));
            let arguments = tool_call_obj
                .get("arguments")
                .or_else(|| tool_call_obj.get("args"))
                .and_then(|value| value.as_object())
                .cloned()
                .unwrap_or_default();
            match tool_name.as_str() {
                "list_scenes" => {
                    tool_results.push(serde_json::json!({
                        "id": call_id,
                        "tool_name": "list_scenes",
                        "ok": true,
                        "result": {
                            "scenes": extract_scene_names(&world.map_nodes),
                            "current_scene": session.scene.name,
                            "current_location": session.location,
                        }
                    }));
                }
                "list_characters" => {
                    tool_results.push(serde_json::json!({
                        "id": call_id,
                        "tool_name": "list_characters",
                        "ok": true,
                        "result": {
                            "current_player_character_name": session.player_character_name,
                            "current_scene_character_roster": session.visible_characters,
                            "world_characters": characters.iter().map(|character| serde_json::json!({
                                "id": character.id,
                                "name": character.name,
                                "role": character.role,
                            })).collect::<Vec<_>>(),
                        }
                    }));
                }
                "change_scene" => {
                    self.apply_change_scene_tool(
                        session,
                        &call_id,
                        &arguments,
                        &mut merged,
                        &mut tool_results,
                    );
                }
                "switch_player_character" => {
                    self.apply_switch_player_character_tool(
                        session,
                        world,
                        &call_id,
                        &arguments,
                        &mut merged,
                        &mut tool_results,
                    );
                }
                "generate_image" => {
                    self.apply_generate_image_tool(
                        &call_id,
                        &arguments,
                        &mut merged,
                        &mut tool_results,
                    );
                }
                "schedule_notification" => {
                    self.apply_schedule_notification_tool(
                        session,
                        world,
                        schedule_notification_allowed,
                        notification_runtime,
                        notification_conn,
                        turn_index,
                        &call_id,
                        &arguments,
                        &mut pending_notifications,
                        &mut tool_results,
                    );
                }
                _ => {
                    // 自定义 MCP 工具：结果来自 execute_pending_mcp_tool_calls 的异步预执行。
                    self.apply_custom_mcp_tool_result(
                        mcp_results,
                        tool_call_index,
                        &call_id,
                        &tool_name,
                        &arguments,
                        &mut tool_results,
                    );
                }
            }
        }
        if !pending_notifications.is_empty() {
            merged.insert(
                "pending_notifications".to_string(),
                serde_json::Value::Array(pending_notifications),
            );
        }
        if !tool_results.is_empty() {
            merged.insert(
                "tool_results".to_string(),
                serde_json::Value::Array(tool_results),
            );
        }
        serde_json::Value::Object(merged)
    }

    /// change_scene 工具效果:写入场景切换字段并回执。
    fn apply_change_scene_tool(
        &self,
        session: &SessionSnapshot,
        call_id: &str,
        arguments: &serde_json::Map<String, serde_json::Value>,
        merged: &mut serde_json::Map<String, serde_json::Value>,
        tool_results: &mut Vec<serde_json::Value>,
    ) {
        let scene_name = arg_string(arguments, "scene_name")
            .or_else(|| arg_string(arguments, "location"))
            .unwrap_or_else(|| session.scene.name.clone());
        let scene_description = arg_string(arguments, "scene_description")
            .or_else(|| arg_string(arguments, "scene_background_hint"))
            .unwrap_or_else(|| session.scene.background_hint.clone());
        merged.insert(
            "next_location".to_string(),
            serde_json::Value::String(scene_name.clone()),
        );
        merged.insert(
            "next_scene_name".to_string(),
            serde_json::Value::String(scene_name),
        );
        merged.insert(
            "scene_background_hint".to_string(),
            serde_json::Value::String(scene_description),
        );
        if let Some(new_characters) = arguments.get("new_characters").and_then(|v| v.as_array()) {
            if !new_characters.is_empty() {
                merged.insert(
                    "generated_characters".to_string(),
                    serde_json::Value::Array(new_characters.clone()),
                );
            }
        }
        if let Some(scene_character_roster) = arguments
            .get("scene_character_roster")
            .and_then(|v| v.as_array())
        {
            merged.insert(
                "scene_visible_characters".to_string(),
                serde_json::Value::Array(scene_character_roster.clone()),
            );
        }
        tool_results.push(serde_json::json!({
            "id": call_id,
            "tool_name": "change_scene",
            "ok": true,
            "arguments": arguments,
        }));
    }

    /// switch_player_character 工具效果:生成换角提案并回执。
    fn apply_switch_player_character_tool(
        &self,
        session: &SessionSnapshot,
        world: &WorldDefinition,
        call_id: &str,
        arguments: &serde_json::Map<String, serde_json::Value>,
        merged: &mut serde_json::Map<String, serde_json::Value>,
        tool_results: &mut Vec<serde_json::Value>,
    ) {
        let target_character_name =
            arg_string(arguments, "target_character_name").unwrap_or_default();
        if allow_player_character_switch(world)
            && !target_character_name.is_empty()
            && target_character_name != session.player_character_name
        {
            let scene_character_roster = {
                let values = arg_string_list(arguments.get("scene_character_roster"));
                if values.is_empty() {
                    session.visible_characters.clone()
                } else {
                    values
                }
            };
            merged.insert(
                "switch_character_proposal".to_string(),
                serde_json::json!({
                    "target_character_name": target_character_name,
                    "reason": arg_string(arguments, "reason").unwrap_or_else(|| "tool_switch".to_string()),
                    "location": arg_string(arguments, "location").unwrap_or_else(|| session.location.clone()),
                    "scene_name": arg_string(arguments, "scene_name").unwrap_or_else(|| session.scene.name.clone()),
                    "scene_background_hint": arg_string(arguments, "scene_background_hint").unwrap_or_else(|| session.scene.background_hint.clone()),
                    "scene_tags": arg_string_list(arguments.get("scene_tags")),
                    "scene_character_roster": scene_character_roster,
                }),
            );
        }
        tool_results.push(serde_json::json!({
            "id": call_id,
            "tool_name": "switch_player_character",
            "ok": true,
            "arguments": arguments,
        }));
    }

    /// generate_image 工具效果:写入背景/立绘生成指令并回执。
    fn apply_generate_image_tool(
        &self,
        call_id: &str,
        arguments: &serde_json::Map<String, serde_json::Value>,
        merged: &mut serde_json::Map<String, serde_json::Value>,
        tool_results: &mut Vec<serde_json::Value>,
    ) {
        let kind = arg_string(arguments, "kind").unwrap_or_else(|| "background".to_string());
        let prompt = arg_string(arguments, "prompt").unwrap_or_default();
        if !prompt.is_empty() {
            if kind == "portrait" {
                if let Some(character_name) = arg_string(arguments, "character_name") {
                    let mut directives = merged
                        .get("character_visual_directives")
                        .and_then(|value| value.as_array())
                        .cloned()
                        .unwrap_or_default();
                    directives.push(serde_json::json!({
                        "character_name": character_name,
                        "generation_prompt": prompt,
                    }));
                    merged.insert(
                        "character_visual_directives".to_string(),
                        serde_json::Value::Array(directives),
                    );
                }
            } else {
                merged.insert(
                    "background_generation_prompt".to_string(),
                    serde_json::Value::String(prompt),
                );
            }
        }
        tool_results.push(serde_json::json!({
            "id": call_id,
            "tool_name": "generate_image",
            "ok": true,
            "result": {
                "status": "accepted",
                "arguments": arguments,
            }
        }));
    }

    /// schedule_notification 工具效果:落库或写入 pending_notifications 并回执。
    #[allow(clippy::too_many_arguments)]
    fn apply_schedule_notification_tool(
        &self,
        session: &SessionSnapshot,
        world: &WorldDefinition,
        schedule_notification_allowed: bool,
        notification_runtime: Option<NotificationToolRuntime<'_>>,
        notification_conn: Option<&rusqlite::Connection>,
        turn_index: i32,
        call_id: &str,
        arguments: &serde_json::Map<String, serde_json::Value>,
        pending_notifications: &mut Vec<serde_json::Value>,
        tool_results: &mut Vec<serde_json::Value>,
    ) {
        if !schedule_notification_allowed {
            tool_results.push(serde_json::json!({
                "id": call_id,
                "tool_name": "schedule_notification",
                "ok": false,
                "error": "schedule_notification is not allowed for this world",
            }));
            return;
        }
        if let Some(runtime) = notification_runtime {
            // H7: 使用调用方传入的主连接(在 async 层加锁取得),不再 Database::new()。
            let result = match notification_conn {
                Some(conn) => NotificationScheduler::execute_tool_call(
                    conn,
                    runtime.app,
                    runtime.data_dir,
                    NotificationToolContext {
                        session_id: &session.id,
                        world_id: &world.id,
                        world_name: &world.name,
                        turn_index,
                        speaker_name: None,
                        speaker_avatar_asset: None,
                    },
                    call_id,
                    arguments,
                ),
                None => serde_json::json!({
                    "id": call_id,
                    "tool_name": "schedule_notification",
                    "tool_call_id": call_id,
                    "ok": false,
                    "error": "notification database connection is unavailable",
                }),
            };
            tool_results.push(result);
            return;
        }
        match pending_notification_from_tool_call(&session.id, call_id, arguments) {
            Ok(pending) => {
                let scheduled_at = pending.scheduled_at.clone();
                let body = pending.body.clone();
                let title = pending.title.clone();
                pending_notifications.push(
                    serde_json::to_value(&pending).unwrap_or_else(|err| {
                        // L11: 序列化失败会构造残缺对象,通知可能永不触发;记录日志便于排查。
                        #[cfg(debug_assertions)]
                        eprintln!(
                            "[director] serialize pending notification failed (session={}, call={}): {err}",
                            session.id, call_id
                        );
                        #[cfg(not(debug_assertions))]
                        let _ = err;
                        serde_json::json!({
                            "tool_call_id": call_id,
                            "source": format!("tool:schedule_notification:{}:{}", session.id, call_id),
                            "title": title,
                            "body": body,
                            "scheduled_at": scheduled_at,
                        })
                    }),
                );
                tool_results.push(serde_json::json!({
                    "id": call_id,
                    "tool_name": "schedule_notification",
                    "ok": true,
                    "result": {
                        "status": "scheduled",
                        "scheduled_at": scheduled_at,
                        "content": body,
                        "title": title,
                    }
                }));
            }
            Err(error) => {
                tool_results.push(serde_json::json!({
                    "id": call_id,
                    "tool_name": "schedule_notification",
                    "ok": false,
                    "error": error,
                }));
            }
        }
    }

    /// 自定义 MCP 工具效果:回填 execute_pending_mcp_tool_calls 的异步预执行结果。
    fn apply_custom_mcp_tool_result(
        &self,
        mcp_results: &std::collections::HashMap<usize, serde_json::Value>,
        tool_call_index: usize,
        call_id: &str,
        tool_name: &str,
        arguments: &serde_json::Map<String, serde_json::Value>,
        tool_results: &mut Vec<serde_json::Value>,
    ) {
        match mcp_results.get(&tool_call_index) {
            Some(outcome) => {
                let mut entry = outcome.as_object().cloned().unwrap_or_default();
                entry.insert(
                    "id".to_string(),
                    serde_json::Value::String(call_id.to_string()),
                );
                entry.insert(
                    "tool_name".to_string(),
                    serde_json::Value::String(tool_name.to_string()),
                );
                entry.insert("arguments".to_string(), serde_json::json!(arguments));
                tool_results.push(serde_json::Value::Object(entry));
            }
            None => {
                tool_results.push(serde_json::json!({
                    "id": call_id,
                    "tool_name": tool_name,
                    "ok": false,
                    "error": format!("未找到工具 {tool_name} 的执行结果：该工具未配置 MCP server 或未在本世界授权"),
                }));
            }
        }
    }

    pub fn parse_loose_json(&self, raw: &str) -> serde_json::Value {
        let trimmed = raw.trim();
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
            return value;
        }
        let stripped = trimmed
            .strip_prefix("```json")
            .or_else(|| trimmed.strip_prefix("```JSON"))
            .or_else(|| trimmed.strip_prefix("```"))
            .map(|value| value.trim())
            .and_then(|value| value.strip_suffix("```"))
            .map(str::trim)
            .unwrap_or(trimmed);
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(stripped) {
            return value;
        }
        if let Some(candidate) = extract_first_balanced_json_segment(stripped) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&candidate) {
                return value;
            }
            let repaired_candidate = repair_common_json_issues(&candidate);
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&repaired_candidate) {
                return value;
            }
        }
        let repaired = repair_common_json_issues(stripped);
        serde_json::from_str::<serde_json::Value>(&repaired).unwrap_or_default()
    }

    fn merge_native_tool_calls(
        &self,
        parsed: &serde_json::Value,
        native_tool_calls: Option<&[ChatToolCall]>,
    ) -> serde_json::Value {
        let Some(native_tool_calls) = native_tool_calls else {
            return parsed.clone();
        };
        if native_tool_calls.is_empty() {
            return parsed.clone();
        }
        let serialized_tool_calls = native_tool_calls
            .iter()
            .map(|tool_call| {
                serde_json::json!({
                    "id": tool_call.id,
                    "tool_name": tool_call.tool_name,
                    "arguments": tool_call.arguments,
                })
            })
            .collect::<Vec<_>>();
        let mut merged = parsed.as_object().cloned().unwrap_or_default();
        merged.insert(
            "tool_calls".to_string(),
            serde_json::Value::Array(serialized_tool_calls),
        );
        serde_json::Value::Object(merged)
    }

    fn remove_response_body_tool_calls(&self, parsed: &serde_json::Value) -> serde_json::Value {
        let Some(object) = parsed.as_object() else {
            return parsed.clone();
        };
        if !object.contains_key("tool_calls") {
            return parsed.clone();
        }
        let mut stripped = object.clone();
        stripped.remove("tool_calls");
        serde_json::Value::Object(stripped)
    }

    pub fn build_tool_followup_request(
        &self,
        previous_request: &crate::services::llm::client::ChatRequest,
        parsed: &serde_json::Value,
        tool_enriched: &serde_json::Value,
        used_native_tool_calls: bool,
        reasoning_content: Option<String>,
    ) -> Result<crate::services::llm::client::ChatRequest, String> {
        let mut messages = previous_request.messages.clone();
        if used_native_tool_calls {
            let tool_calls = self
                .extract_tool_calls(parsed, None)
                .into_iter()
                .filter_map(|tool_call| {
                    let object = tool_call.as_object()?;
                    let name = object
                        .get("tool_name")
                        .and_then(|value| value.as_str())
                        .map(str::trim)
                        .filter(|value| !value.is_empty())?;
                    Some(ChatToolCall {
                        id: object
                            .get("id")
                            .and_then(|value| value.as_str())
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .unwrap_or("tool-call")
                            .to_string(),
                        tool_name: name.to_string(),
                        arguments: object
                            .get("arguments")
                            .cloned()
                            .unwrap_or_else(|| serde_json::json!({})),
                    })
                })
                .collect::<Vec<_>>();
            messages.push(crate::services::llm::client::ChatMessage {
                role: "assistant".to_string(),
                content: serde_json::Value::String(String::new()),
                reasoning_content: reasoning_content,
                speaker: None,
                tool_call_id: None,
                tool_calls: Some(tool_calls.clone()),
                metadata: Some(serde_json::json!({
                    "tool_phase": true,
                    "tool_calls": parsed.get("tool_calls").cloned().unwrap_or_else(|| serde_json::Value::Array(vec![])),
                })),
            });
            for result in tool_enriched
                .get("tool_results")
                .and_then(|value| value.as_array())
                .cloned()
                .unwrap_or_default()
            {
                let call_id = result
                    .get("id")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string())
                    .unwrap_or_default();
                messages.push(crate::services::llm::client::ChatMessage {
                    role: "tool".to_string(),
                    content: serde_json::Value::String(serde_json::to_string(&result).map_err(|e| e.to_string())?),
                    reasoning_content: None,
                    speaker: None,
                    tool_call_id: Some(call_id),
                    tool_calls: None,
                    metadata: Some(serde_json::json!({
                        "tool_phase": true,
                    })),
                });
            }
            return Ok(crate::services::llm::client::ChatRequest {
                model: previous_request.model.clone(),
                messages,
                generation: previous_request.generation.clone(),
                stream: previous_request.stream,
                json_mode: previous_request.json_mode,
                response_schema: previous_request.response_schema.clone(),
                tools: previous_request.tools.clone(),
                tool_choice: previous_request.tool_choice.clone(),
                native_tool_calling: previous_request.native_tool_calling,
            });
        }
        Err("Director tool follow-up requires native tool_calls".to_string())
    }

    pub fn resolve_tool_loop_limit(&self, world: &WorldDefinition) -> usize {
        world
            .director_config
            .get("director_tool_loop_limit")
            .and_then(|value| value.as_i64())
            .map(|value| {
                value.clamp(DIRECTOR_TOOL_LOOP_LIMIT_MIN, DIRECTOR_TOOL_LOOP_LIMIT_MAX) as usize
            })
            .unwrap_or(DIRECTOR_TOOL_LOOP_LIMIT_DEFAULT)
    }

    pub fn resolve_tool_call_limit(&self, world: &WorldDefinition) -> usize {
        world
            .director_config
            .get("director_tool_call_limit")
            .and_then(|value| value.as_i64())
            .map(|value| {
                value.clamp(DIRECTOR_TOOL_CALL_LIMIT_MIN, DIRECTOR_TOOL_CALL_LIMIT_MAX) as usize
            })
            .unwrap_or(DIRECTOR_TOOL_CALL_LIMIT_DEFAULT)
    }

    pub fn resolve_runtime_stage_label(
        &self,
        world: &WorldDefinition,
        request_messages: &[crate::services::llm::client::ChatMessage],
    ) -> String {
        let labels = world
            .director_config
            .get("director_stage_labels")
            .and_then(|value| value.as_object());
        let default_label = labels
            .and_then(|value| value.get("default_turn"))
            .and_then(|value| value.as_str())
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .unwrap_or("normal turn");
        let tool_loop_label = labels
            .and_then(|value| value.get("tool_loop_turn"))
            .and_then(|value| value.as_str())
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .unwrap_or("宸ュ叿璋冪敤鍥炲悎");
        if request_messages.iter().any(|message| {
            message
                .metadata
                .as_ref()
                .and_then(|meta| meta.get("tool_phase"))
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
        }) {
            tool_loop_label.to_string()
        } else {
            default_label.to_string()
        }
    }

    pub fn should_continue_tool_loop(
        &self,
        world: &WorldDefinition,
        parsed: &serde_json::Value,
        iteration: usize,
    ) -> bool {
        if iteration >= self.resolve_tool_loop_limit(world) {
            return false;
        }
        // 唯一的终止语义:模型还在发 tool_calls 就继续循环,直到 loop limit。
        // (曾经的 director_tool_loop_termination 配置从未实现第二种模式,已移除。)
        !self.extract_tool_calls(parsed, None).is_empty()
    }

    pub fn parse_runtime_payload(
        &self,
        parsed: &serde_json::Value,
        session: &SessionSnapshot,
        world: &WorldDefinition,
        player_input: &str,
    ) -> ParsedDirectorRuntimePayload {
        let allow_scene_transition = world
            .director_config
            .get("allow_scene_transition")
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        let allow_npc_spawn = world
            .director_config
            .get("allow_npc_spawn")
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        let allow_player_character_switch = allow_player_character_switch(world);
        let world_phase = normalize_llm_text(parsed.get("world_phase"))
            .filter(|value| matches!(value.as_str(), "opening" | "escalation" | "crisis"))
            .unwrap_or_else(|| session.state.phase.clone());
        let next_location = if allow_scene_transition {
            normalize_llm_text(parsed.get("next_location"))
                .unwrap_or_else(|| session.location.clone())
        } else {
            session.location.clone()
        };
        let next_scene_name = if allow_scene_transition {
            normalize_llm_text(parsed.get("next_scene_name"))
                .or_else(|| {
                    if next_location != session.location {
                        Some(next_location.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| session.scene.name.clone())
        } else {
            session.scene.name.clone()
        };
        let current_line = normalize_llm_text(parsed.get("current_line"))
            .filter(|line| !looks_like_director_authored_speech(line));
        let next_scene_background_hint = normalize_llm_text(
            parsed
                .get("next_scene_background_hint")
                .or_else(|| parsed.get("scene_background_hint")),
        )
        .or_else(|| {
            if next_scene_name == session.scene.name {
                Some(session.scene.background_hint.clone())
            } else {
                None
            }
        });
        let background_generation_prompt =
            normalize_llm_text(parsed.get("background_generation_prompt"));
        let background_asset_name = normalize_llm_text(parsed.get("background_asset_name"));
        let background_asset_path = normalize_llm_text(parsed.get("background_asset_path"));
        let next_scene_tags = parse_next_scene_tags(
            parsed
                .get("next_scene_tags")
                .or_else(|| parsed.get("scene_tags")),
            &session.scene.temporary_tags,
            &next_scene_name,
            &session.scene.name,
        );
        let next_time_label = parse_next_time_label(
            parsed.get("next_time_label"),
            session,
            world,
            &session.time_label,
        );
        // scene_visible_characters 是导演输出的"每轮更新指令"（wire 字段，勿改名）：
        // 显式给出即对在场名册做完整替换；缺省(None)则沿用 session.visible_characters
        // （持久状态），新生成角色的并入发生在 run.rs 的名册解析阶段。
        let scene_visible_characters = parse_scene_visible_characters(
            parsed.get("scene_visible_characters"),
            &session.player_character_name,
        );
        let merged_visible = if let Some(explicit) = scene_visible_characters.clone() {
            explicit
        } else {
            session.visible_characters.clone()
        };
        let interaction = Self::parse_director_interaction(parsed.get("interaction"), world);
        let planned_speakers = parse_planned_speakers(
            parse_string_list(parsed.get("planned_speakers")),
            &merged_visible,
            &session.visible_characters,
            &session.player_character_name,
            player_input,
            &world_phase,
            &session.messages,
        );
        let generated_character_payloads = if allow_npc_spawn {
            normalize_generated_character_items(collect_generated_character_items(parsed), session)
        } else {
            Vec::new()
        };
        let character_visual_directives =
            parse_character_visual_directives(parsed.get("character_visual_directives"));
        ParsedDirectorRuntimePayload {
            world_phase,
            next_location,
            next_scene_name,
            current_line,
            next_scene_background_hint,
            background_asset_name,
            background_asset_path,
            background_generation_prompt,
            next_scene_tags,
            next_time_label,
            scene_visible_characters,
            planned_speakers: if interaction.is_some() { Vec::new() } else { planned_speakers },
            generated_character_payloads,
            character_visual_directives,
            switch_character_proposal: allow_player_character_switch.then(|| {
                parse_switch_character_proposal(
                    parsed.get("switch_character_proposal"),
                    &session.player_character_name,
                )
            }).flatten(),
            interaction,
        }
    }

    fn parse_director_interaction(
    raw: Option<&serde_json::Value>,
    world: &WorldDefinition,
) -> Option<MessageInteraction> {
    let raw = raw?;
    let (kind, prompt, config) = validate_interaction_candidate(raw).ok()?;
    if !declared_director_interaction_kinds(&world.director_config)
        .iter()
        .any(|allowed| allowed == &kind)
    {
        return None;
    }
    Some(MessageInteraction {
        interaction_id: ChatMessage::generate_id(),
        kind,
        prompt,
        config,
        status: INTERACTION_STATUS_PENDING.to_string(),
        answer: None,
        answered_at: None,
    })
}

    fn extract_tool_calls(
        &self,
        parsed: &serde_json::Value,
        limit: Option<usize>,
    ) -> Vec<serde_json::Value> {
        let mut tool_calls = parsed
            .get("tool_calls")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|item| item.as_object().is_some())
            .collect::<Vec<_>>();
        if let Some(limit) = limit {
            tool_calls.truncate(limit);
        }
        tool_calls
    }

    pub fn create_generated_character_if_missing(
        &self,
        conn: &rusqlite::Connection,
        world: &WorldDefinition,
        characters: &mut Vec<CharacterDefinition>,
        generated: &serde_json::Value,
    ) -> Result<Option<CharacterDefinition>, String> {
        let name = normalize_llm_text(generated.get("name"))
            .or_else(|| normalize_llm_text(generated.get("character_name")));
        let Some(name) = name else {
            return Ok(None);
        };
        if let Some(existing) = characters.iter().find(|item| item.name == name).cloned() {
            return Ok(Some(existing));
        }
        let role = normalize_llm_text(generated.get("role"))
            .or_else(|| normalize_llm_text(generated.get("identity")))
            .unwrap_or_else(|| "scene character".to_string());
        let background_prompt = normalize_llm_text(generated.get("background_prompt"))
            .or_else(|| normalize_llm_text(generated.get("description")))
            .or_else(|| normalize_llm_text(generated.get("profile")))
            .or_else(|| {
                let location =
                    normalize_llm_text(generated.get("initial_location")).unwrap_or_default();
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
        let request = CharacterCreateRequest {
            name: name.clone(),
            role,
            background_prompt,
            model: normalize_llm_text(generated.get("model")).unwrap_or_default(),
            memory_strategy: normalize_llm_text(generated.get("memory_strategy"))
                .unwrap_or_else(|| "recent".to_string()),
            recent_dialogue_rounds: generated
                .get("recent_dialogue_rounds")
                .and_then(|value| value.as_i64())
                .map(|value| value as i32)
                .unwrap_or(6)
                .max(1),
            attributes: vec![],
            portrait_assets: vec![],
            avatar_asset: String::new(),
            system_prompt_template: String::new(),
            response_contract_prompt: String::new(),
            narration_prompt: String::new(),
            runtime_system_prompt: String::new(),
        };
        let created = crate::db::repositories::character_repo::CharacterRepository::new(conn)
            .create(&world.id, &request)?;
        let enriched = created;
        characters.push(enriched.clone());
        Ok(Some(enriched))
    }

    pub fn materialize_switch_proposal_message(
        &self,
        conn: &rusqlite::Connection,
        world: &WorldDefinition,
        session: &SessionSnapshot,
        characters: &mut Vec<CharacterDefinition>,
        turn_index: i32,
        proposal: Option<&serde_json::Value>,
    ) -> Result<Option<(Vec<ChatMessage>, ChatMessage)>, String> {
        let Some(proposal) = proposal.and_then(|value| value.as_object()) else {
            return Ok(None);
        };
        let target_name = proposal
            .get("target_character_name")
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let Some(target_name) = target_name else {
            return Ok(None);
        };
        if target_name == session.player_character_name {
            return Ok(None);
        }
        let player_profile = characters
            .iter()
            .find(|character| character.id == session.player_character_id)
            .cloned()
            .or_else(|| {
                characters
                    .iter()
                    .find(|character| character.name == session.player_character_name)
                    .cloned()
            });
        let mut creation_messages = Vec::new();
        let target_character = if let Some(existing) = characters
            .iter()
            .find(|character| character.name == target_name)
            .cloned()
        {
            existing
        } else {
            let created = crate::db::repositories::character_repo::CharacterRepository::new(conn)
                .create(
                &world.id,
                &CharacterCreateRequest {
                    name: target_name.clone(),
                    role: proposal
                        .get("target_role")
                        .and_then(|value| value.as_str())
                        .unwrap_or("companion")
                        .trim()
                        .to_string(),
                    background_prompt: proposal
                        .get("target_background_prompt")
                        .and_then(|value| value.as_str())
                        .or_else(|| proposal.get("reason").and_then(|value| value.as_str()))
                        .unwrap_or("")
                        .trim()
                        .to_string(),
                    model: player_profile
                        .as_ref()
                        .map(|character| character.model.trim().to_string())
                        .unwrap_or_default(),
                    memory_strategy: player_profile
                        .as_ref()
                        .map(|character| {
                            let strategy = character.memory_strategy.trim();
                            if strategy.is_empty() {
                                "recent".to_string()
                            } else {
                                strategy.to_string()
                            }
                        })
                        .unwrap_or_else(|| "recent".to_string()),
                    recent_dialogue_rounds: player_profile
                        .as_ref()
                        .map(|character| character.recent_dialogue_rounds.max(1))
                        .unwrap_or(6),
                    attributes: vec![],
                    portrait_assets: vec![],
                    avatar_asset: String::new(),
                    system_prompt_template: String::new(),
                    response_contract_prompt: String::new(),
                    narration_prompt: String::new(),
                    runtime_system_prompt: String::new(),
                },
            )?;
            let enriched = created;
            creation_messages
                .push(self.build_character_created_message(turn_index, &enriched, true));
            characters.push(enriched.clone());
            enriched
        };
        let sanitized_scene_character_roster = proposal
            .get("scene_character_roster")
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str())
                    .map(|item| item.trim().to_string())
                    .filter(|item| {
                        !item.is_empty()
                            && *item != target_character.name
                            && *item != session.player_character_name
                    })
                    .fold(Vec::<String>::new(), |mut acc, item| {
                        if !acc.contains(&item) {
                            acc.push(item);
                        }
                        acc
                    })
            })
            .unwrap_or_default();
        let resolved_location = proposal
            .get("location")
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .or_else(|| {
                proposal
                    .get("next_location")
                    .and_then(|value| value.as_str())
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
            })
            .or_else(|| {
                proposal
                    .get("scene_name")
                    .and_then(|value| value.as_str())
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
            })
            .unwrap_or_else(|| session.location.clone());
        let resolved_scene_name = proposal
            .get("scene_name")
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| resolved_location.clone());
        let resolved_background_hint = proposal
            .get("scene_background_hint")
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| resolved_scene_name.clone());
        let message = ChatMessage {
            message_id: ChatMessage::generate_id(),
            created_at: chrono::Utc::now().to_rfc3339(),
            parent_message_id: None,
            role: "system".to_string(),
            content: MessageContent::Text(
                proposal
                    .get("reason")
                    .and_then(|value| value.as_str())
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| format!("Suggest switching to: {}", target_name)),
            ),
            speaker: None,
            metadata: Some(serde_json::json!({
                "turn_index": turn_index,
                "action_type": "switch_character",
                "target_character_name": target_character.name,
                "target_character_id": target_character.id,
                "target_role": target_character.role,
                "target_background_prompt": target_character.background_prompt,
                "target_created_in_turn": creation_messages.iter().any(|item| {
                    item
                        .metadata
                        .as_ref()
                        .and_then(|meta| meta.get("character_name"))
                        .and_then(|value| value.as_str())
                        .map(|value| value == target_name)
                        .unwrap_or(false)
                }),
                "location": resolved_location,
                "scene_name": resolved_scene_name,
                "scene_background_hint": resolved_background_hint,
                "scene_tags": proposal.get("scene_tags").cloned().unwrap_or_else(|| serde_json::Value::Array(vec![])),
                "scene_character_roster": sanitized_scene_character_roster,
            })),
        };
        Ok(Some((creation_messages, message)))
    }

    fn build_tool_protocol(&self) -> serde_json::Value {
        serde_json::json!({
            "format": {
                "description": "When a tool is needed, use the provider-native tool_calls channel. Each native tool call must carry exactly three logical fields after normalization: id (unique identifier string), tool_name (must match exactly one of the available_tools names above), and arguments (a JSON object of parameters, or {} if the tool takes no parameters). Do not place tool calls, tool names, or tool arguments inside the JSON response body.",
                "examples": [
                    {
                        "id": "call-1",
                        "tool_name": "list_scenes",
                        "arguments": {}
                    },
                    {
                        "id": "call-2",
                        "tool_name": "change_scene",
                        "arguments": {
                            "scene_name": "Throne Room",
                            "scene_character_roster": ["King", "Guard"]
                        }
                    },
                    {
                        "id": "call-3",
                        "tool_name": "switch_player_character",
                        "arguments": {
                            "target_character_name": "Captain",
                            "reason": "Player wants to follow the captain"
                        }
                    }
                ]
            },
            "rules": [
                "tool_name must be exactly one of the tool_name values listed in available_tools; do not invent tool names.",
                "Use provider-native tool_calls for every tool invocation.",
                "Do not encode tool calls, tool names, or tool arguments inside the JSON response body.",
                "JSON-body tool_calls are invalid and will be ignored by the runtime.",
                "Call tools only when you genuinely need information from a tool; do not fabricate tool calls for decoration.",
                "If the player asks to add a new participant who is not already in the current scene or world roster, return generated_characters first, then place that character into scene_visible_characters and planned_speakers in the same response.",
                "Each generated_characters item must include at least name, role and background_prompt, where background_prompt is a usable portrayal brief for the later character model, not just a label or one-word tag.",
                "Do not place a new name directly into scene_visible_characters or planned_speakers without generated_characters.",
                "After tool_results are provided, return final world-state JSON."
            ]
        })
    }

    fn build_visual_capabilities(
        &self,
        world: &WorldDefinition,
        session: &SessionSnapshot,
        characters: &[CharacterDefinition],
    ) -> serde_json::Value {
        let background_source_mode = world
            .ui_theme_config
            .get("background_source_mode")
            .and_then(|v| v.as_str())
            .unwrap_or("local-first")
            .to_string();
        let portrait_source_mode = world
            .ui_theme_config
            .get("portrait_source_mode")
            .and_then(|v| v.as_str())
            .or_else(|| {
                world
                    .ui_theme_config
                    .get("background_source_mode")
                    .and_then(|v| v.as_str())
            })
            .unwrap_or("local-first")
            .to_string();
        let local_background_assets_count = world
            .ui_theme_config
            .get("local_background_assets")
            .and_then(|v| v.as_array())
            .map(|items| items.len())
            .unwrap_or(0);
        let local_scene_background_keys = world
            .ui_theme_config
            .get("local_scene_backgrounds")
            .and_then(|v| v.as_object())
            .map(|obj| obj.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        let character_portrait_counts = characters
            .iter()
            .filter(|character| !character.portrait_assets.is_empty())
            .map(|character| {
                serde_json::json!({
                    "character_name": character.name,
                    "portrait_count": character.portrait_assets.len(),
                    "visible": session.visible_characters.iter().any(|name| name == &character.name),
                })
            })
            .collect::<Vec<_>>();
        let runtime_image_generation_enabled = self
            .resolve_world_allowed_tool_ids(world)
            .iter()
            .any(|id| id == "mcp-tool-image-generation");

        // 无本地素材、无角色立绘、且未启用运行时图像生成时，整块视觉能力没有信息量，省略。
        if local_background_assets_count == 0
            && local_scene_background_keys.is_empty()
            && character_portrait_counts.is_empty()
            && !runtime_image_generation_enabled
        {
            return serde_json::Value::Null;
        }

        let mut capabilities = serde_json::Map::new();
        capabilities.insert(
            "background_source_mode".to_string(),
            serde_json::json!(background_source_mode),
        );
        capabilities.insert(
            "portrait_source_mode".to_string(),
            serde_json::json!(portrait_source_mode),
        );
        if local_background_assets_count > 0 {
            capabilities.insert(
                "local_background_assets_count".to_string(),
                serde_json::json!(local_background_assets_count),
            );
        }
        if !local_scene_background_keys.is_empty() {
            capabilities.insert(
                "local_scene_background_keys".to_string(),
                serde_json::json!(local_scene_background_keys),
            );
        }
        if !character_portrait_counts.is_empty() {
            capabilities.insert(
                "character_portrait_counts".to_string(),
                serde_json::json!(character_portrait_counts),
            );
        }
        capabilities.insert(
            "runtime_image_generation_enabled".to_string(),
            serde_json::json!(runtime_image_generation_enabled),
        );
        serde_json::Value::Object(capabilities)
    }

    fn template_variables(
        &self,
        world: &WorldDefinition,
        session: &SessionSnapshot,
        char_name: &str,
    ) -> std::collections::HashMap<String, String> {
        let mut vars = std::collections::HashMap::new();
        vars.insert(
            "user".to_string(),
            session.player_character_name.trim().to_string(),
        );
        vars.insert("char".to_string(), char_name.to_string());
        vars.insert("world".to_string(), world.name.clone());
        vars.insert(
            "scene".to_string(),
            if session.scene.name.trim().is_empty() {
                session.location.clone()
            } else {
                session.scene.name.clone()
            },
        );
        vars.insert("time".to_string(), session.time_label.clone());
        vars
    }

    fn build_history_dialogue(
        &self,
        messages: &[ChatMessage],
        previous_rounds: i32,
        current_player_name: Option<&str>,
    ) -> Vec<serde_json::Value> {
        if previous_rounds <= 0 {
            return Vec::new();
        }
        let selected = self.slice_director_history(messages, previous_rounds, current_player_name);
        selected
            .into_iter()
            .map(|message| {
                let role = message.role.trim().to_string();
                // 第 10 项：历史消息中的媒体不重复下发（base64 会随每轮膨胀），
                // 统一渲染成 [图片] / [音频 N 秒] 占位文本。
                let content = message.content.as_prompt_text();
                let speaker = self.resolve_history_speaker(&message, current_player_name);
                let mut payload = serde_json::json!({
                    "role": role,
                    "content": content,
                });
                if let Some(object) = payload.as_object_mut() {
                    if !speaker.trim().is_empty() {
                        object.insert("speaker".to_string(), serde_json::Value::String(speaker));
                    }
                    if let Some(metadata) = message.metadata.clone() {
                        if metadata.is_object()
                            && !metadata
                                .as_object()
                                .unwrap_or(&Default::default())
                                .is_empty()
                        {
                            object.insert("metadata".to_string(), metadata);
                        }
                    }
                }
                payload
            })
            .collect()
    }

    pub(crate) fn build_director_tool_capabilities(
        &self,
        world: &WorldDefinition,
        mcp_tools: &[McpToolDefinition],
    ) -> Vec<serde_json::Value> {
        let allowed = self.resolve_world_allowed_tool_ids(world);
        let allowed = allowed.into_iter().collect::<BTreeSet<_>>();
        let mut tools = vec![
            serde_json::json!({
                "tool_name": "list_scenes",
                "description": "List available scenes in the current world.",
                "arguments_schema": { "type": "object", "properties": {} }
            }),
            serde_json::json!({
                "tool_name": "list_characters",
                "description": "List characters in the current world and characters currently visible in the scene.",
                "arguments_schema": { "type": "object", "properties": {} }
            }),
            serde_json::json!({
                "tool_name": "change_scene",
                "description": "Switch to a target scene or create a new scene. scene_character_roster sets visible characters. new_characters may create new characters and should include name, role, and background_prompt.",
                "arguments_schema": {
                    "type": "object",
                    "required": ["scene_name"],
                    "properties": {
                        "scene_name": { "type": "string" },
                        "scene_description": { "type": "string" },
                        "scene_character_roster": { "type": "array", "items": { "type": "string" } },
                        "new_characters": { "type": "array", "items": { "type": "object" } }
                    }
                }
            }),
        ];
        let allow_player_character_switch = allow_player_character_switch(world);
        if allow_player_character_switch {
            tools.push(serde_json::json!({
                "tool_name": "switch_player_character",
                "description": "Switch the player viewpoint to another character and explain the visible character roster after switching.",
                "arguments_schema": {
                    "type": "object",
                    "required": ["target_character_name"],
                    "properties": {
                        "target_character_name": { "type": "string" },
                        "reason": { "type": "string" },
                        "scene_character_roster": { "type": "array", "items": { "type": "string" } },
                        "scene_name": { "type": "string" },
                        "scene_background_hint": { "type": "string" }
                    }
                }
            }));
        }
        if allowed.contains("mcp-tool-image-generation") {
            tools.push(serde_json::json!({
                "tool_name": "generate_image",
                "description": "Generate a background or portrait image from prompt text.",
                "arguments_schema": {
                    "type": "object",
                    "required": ["kind", "prompt"],
                    "properties": {
                        "kind": { "type": "string", "enum": ["background", "portrait"] },
                        "prompt": { "type": "string" },
                        "character_name": { "type": "string" }
                    }
                }
            }));
        }
        if allowed.contains(MCP_TOOL_SCHEDULE_NOTIFICATION_ID) {
            tools.push(notification_tool_definition());
        }
        tools.extend(custom_mcp_tool_capabilities(&allowed, mcp_tools));
        tools
    }




    fn resolve_world_allowed_tool_ids(&self, world: &WorldDefinition) -> Vec<String> {
        world_allowed_tool_ids(world)
    }

    fn resolve_director_history_rounds(&self, world: &WorldDefinition) -> i32 {
        world
            .director_config
            .get("history_dialogue_rounds")
            .and_then(|value| value.as_i64())
            .map(|value| value as i32)
            .unwrap_or(6)
    }

    fn slice_director_history(
        &self,
        messages: &[ChatMessage],
        previous_rounds: i32,
        current_player_name: Option<&str>,
    ) -> Vec<ChatMessage> {
        if previous_rounds <= 0 {
            return Vec::new();
        }
        let mut selected = Vec::new();
        let mut player_messages_seen = 0;
        for message in messages.iter().rev() {
            if message.role.trim().is_empty() || message.content.trim().is_empty() {
                continue;
            }
            selected.push(message.clone());
            if message.role == "player" || self.is_player_message(message, current_player_name) {
                player_messages_seen += 1;
                if player_messages_seen >= previous_rounds {
                    break;
                }
            }
        }
        selected.reverse();
        selected
    }

    fn resolve_history_speaker(
        &self,
        message: &ChatMessage,
        current_player_name: Option<&str>,
    ) -> String {
        if message.role == "player" || self.is_player_message(message, current_player_name) {
            return self.resolved_player_speaker(current_player_name);
        }
        message
            .speaker
            .as_deref()
            .map(|speaker| speaker.trim().to_string())
            .filter(|speaker| !speaker.is_empty())
            .unwrap_or_else(|| message.role.clone())
    }

    fn resolved_player_speaker(&self, player_character_name: Option<&str>) -> String {
        player_character_name
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "player".to_string())
    }

    fn is_player_message(&self, message: &ChatMessage, current_player_name: Option<&str>) -> bool {
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

    fn build_character_created_message(
        &self,
        turn_index: i32,
        character: &CharacterDefinition,
        for_switch_character: bool,
    ) -> ChatMessage {
        ChatMessage {
            message_id: ChatMessage::generate_id(),
            created_at: chrono::Utc::now().to_rfc3339(),
            parent_message_id: None,
            role: "system".to_string(),
            content: MessageContent::Text(format!("New character joined: {}", character.name)),
            speaker: None,
            metadata: Some(serde_json::json!({
                "turn_index": turn_index,
                "action_type": "character_created",
                "character_id": character.id,
                "character_name": character.name,
                "character_role": character.role,
                "character_background_prompt": character.background_prompt,
                "for_switch_character": for_switch_character,
            })),
        }
    }
}

fn build_director_inventory_records(items: &[InventoryItem]) -> Vec<serde_json::Value> {
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

fn extract_first_balanced_json_segment(raw: &str) -> Option<String> {
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
const DIRECTOR_JSON_REPAIR_ATTEMPTS: usize = 2;

/// 输出被 max_tokens 截断时,重试放大 token 预算的最大次数与倍率。
///
/// 推理模型(deepseek-v4-flash / o 系列等)会先把预算烧在思考上,预算不足时
/// content 为空、finish_reason = "length"。此时追加"你上次输出不是 JSON"的
/// 修复消息只会让 prompt 更长、再次截断——必须放大预算重试。
const DIRECTOR_TOKEN_LIMIT_RETRIES: usize = 2;
const DIRECTOR_TOKEN_LIMIT_GROWTH: i32 = 3;
/// 放大后的预算上限,避免世界包配了极大值时把单轮成本推到失控。
const DIRECTOR_TOKEN_LIMIT_CEILING: i32 = 32_000;
/// 未显式配置 max_tokens 时,按此值作为放大的起点。
const DIRECTOR_TOKEN_LIMIT_FALLBACK: i32 = 4_000;

/// 把请求的 max_tokens 放大一档,返回 None 表示已到上限、无需再试。
fn grow_token_budget(request: &ChatRequest) -> Option<ChatRequest> {
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
const DIRECTOR_TOOL_LOOP_LIMIT_DEFAULT: usize = 4;
const DIRECTOR_TOOL_LOOP_LIMIT_MIN: i64 = 1;
const DIRECTOR_TOOL_LOOP_LIMIT_MAX: i64 = 12;

/// 单轮导演输出中允许处理的工具调用条数(director_tool_call_limit)的缺省值与允许范围。
const DIRECTOR_TOOL_CALL_LIMIT_DEFAULT: usize = 8;
const DIRECTOR_TOOL_CALL_LIMIT_MIN: i64 = 1;
const DIRECTOR_TOOL_CALL_LIMIT_MAX: i64 = 8;

/// 导演最终输出是否需要 LLM 修复重试:解析结果不是非空 JSON 对象。
fn director_output_needs_json_repair(parsed: &serde_json::Value) -> bool {
    parsed
        .as_object()
        .map(|object| object.is_empty())
        .unwrap_or(true)
}

/// 修复反馈消息(中文):告知解析错误,要求模型只输出修正后的完整 JSON。
fn build_director_json_repair_feedback(parse_error: &str) -> String {
    format!(
        "你上一次的输出无法解析为 JSON 对象(错误:{parse_error})。请重新输出完整、合法的 JSON 对象,只输出 JSON 本身,不要包含解释文字或 Markdown 代码围栏。"
    )
}

/// 构造 JSON 修复请求:在原对话后追加"模型的坏输出 + 解析错误反馈",让模型重出。
/// 请求其它部分(model/generation/json_mode/tools 等)保持不变,仍走原有请求构造与流式逻辑。
fn build_director_json_repair_request(
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

fn repair_common_json_issues(raw: &str) -> String {
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
fn sanitize_json_control_chars(raw: &str) -> String {
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
fn complete_truncated_json(raw: &str) -> String {
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

fn arg_string(arguments: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn normalize_llm_text(value: Option<&serde_json::Value>) -> Option<String> {
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

fn looks_like_director_authored_speech(line: &str) -> bool {
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

fn parse_string_list(value: Option<&serde_json::Value>) -> Vec<String> {
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

fn parse_scene_visible_characters(
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

fn parse_next_scene_tags(
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

fn parse_next_time_label(
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

fn parse_clock_minutes(value: &str) -> Option<i32> {
    let (hour, minute) = value.split_once(':')?;
    let hour = hour.parse::<i32>().ok()?;
    let minute = minute.parse::<i32>().ok()?;
    if !(0..=23).contains(&hour) || !(0..=59).contains(&minute) {
        return None;
    }
    Some(hour * 60 + minute)
}

fn parse_character_visual_directives(value: Option<&serde_json::Value>) -> Vec<serde_json::Value> {
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

fn parse_switch_character_proposal(
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

fn collect_generated_character_items(parsed: &serde_json::Value) -> Vec<serde_json::Value> {
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

fn normalize_generated_character_items(
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

fn arg_string_list(value: Option<&serde_json::Value>) -> Vec<String> {
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
fn custom_mcp_tool_capabilities(
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
fn is_builtin_tool_name(tool_name: &str) -> bool {
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

fn mcp_tool_exposure_mode(policy: &serde_json::Value) -> &str {
    policy
        .as_str()
        .or_else(|| policy.get("mode").and_then(|value| value.as_str()))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("on-demand")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::session::{AssetSelection, SceneRuntime, SessionState};

    fn sample_world(director_config: serde_json::Value) -> WorldDefinition {
        WorldDefinition {
            id: "world-1".to_string(),
            name: "World".to_string(),
            genre: "".to_string(),
            background_prompt: "".to_string(),
            opening_scene: "Dock".to_string(),
            summary: "".to_string(),
            time_system: "".to_string(),
            map_nodes: serde_json::json!({ "version": 1, "nodes": [] }),
            triggers: vec![],
            time_config: serde_json::json!({}),
            director_config,
            ui_theme_config: serde_json::json!({}),
            director_system_prompt_base: "".to_string(),
            director_runtime_system_prompt: "".to_string(),
            opening_messages: vec![],
            opening_character_ids: vec![],
            player_character_id: Some("char-player".to_string()),
        }
    }

    fn sample_session() -> SessionSnapshot {
        SessionSnapshot {
            id: "sess-1".to_string(),
            world_name: "World".to_string(),
            location: "Dock".to_string(),
            time_label: "Night".to_string(),
            current_speaker: "Alice".to_string(),
            current_line: "".to_string(),
            player_character_id: "char-player".to_string(),
            player_character_name: "Player".to_string(),
            visible_characters: vec!["Alice".to_string(), "Bob".to_string()],
            messages: vec![],
            player_stats: vec![],
            map_graph_nodes: vec![],
            map_graph_edges: vec![],
            inventory_items: vec![],
            system_log: vec![],
            scene: SceneRuntime {
                scene_id: "dock-scene".to_string(),
                name: "Dock".to_string(),
                background_hint: "rain".to_string(),
                temporary_tags: vec![],
                present_characters: vec![
                    "Player".to_string(),
                    "Alice".to_string(),
                    "Bob".to_string(),
                ],
            },
            assets: AssetSelection::default(),
            state: SessionState::default(),
            generation_params: Default::default(),
        }
    }

    #[test]
    fn resolve_tool_loop_limit_respects_bounds() {
        let service = WorldDirectorService::new();
        let low_world = sample_world(serde_json::json!({ "director_tool_loop_limit": 0 }));
        let high_world = sample_world(serde_json::json!({ "director_tool_loop_limit": 99 }));
        let mid_world = sample_world(serde_json::json!({ "director_tool_loop_limit": 6 }));

        assert_eq!(service.resolve_tool_loop_limit(&low_world), 1);
        assert_eq!(service.resolve_tool_loop_limit(&high_world), 12);
        assert_eq!(service.resolve_tool_loop_limit(&mid_world), 6);
    }

    #[test]
    fn resolve_tool_call_limit_respects_bounds() {
        let service = WorldDirectorService::new();
        let low_world = sample_world(serde_json::json!({ "director_tool_call_limit": 0 }));
        let high_world = sample_world(serde_json::json!({ "director_tool_call_limit": 99 }));
        let mid_world = sample_world(serde_json::json!({ "director_tool_call_limit": 3 }));

        assert_eq!(service.resolve_tool_call_limit(&low_world), 1);
        assert_eq!(service.resolve_tool_call_limit(&high_world), 8);
        assert_eq!(service.resolve_tool_call_limit(&mid_world), 3);
    }

    fn runtime_attribute_group(
        owner_id: &str,
        key: &str,
    ) -> crate::models::session::RuntimeAttributeGroup {
        crate::models::session::RuntimeAttributeGroup {
            owner_type: "session_character".to_string(),
            owner_id: owner_id.to_string(),
            owner_label: owner_id.to_string(),
            items: vec![crate::models::session::RuntimeAttributeItem {
                schema_id: format!("schema-{key}"),
                key: key.to_string(),
                label: key.to_string(),
                value_type: "number".to_string(),
                value: serde_json::json!(1),
                source: "test".to_string(),
                display_policy: serde_json::json!({}),
                influence_policy: serde_json::json!({}),
            }],
        }
    }

    #[test]
    fn append_runtime_attributes_matches_player_group_by_exact_owner_id() {
        // "li" 与 "han-li" 互为后缀:玩家组必须按完整 owner_id 精确匹配。
        let mut session = sample_session();
        session.player_character_id = "li".to_string();
        let mut payload = serde_json::json!({ "current_state": {} });
        let runtime = crate::models::session::SessionRuntimeAttributesResponse {
            session_attributes: vec![],
            character_attributes: vec![
                runtime_attribute_group("sess-1:han-li", "han-li-attr"),
                runtime_attribute_group("sess-1:li", "player-attr"),
            ],
        };

        append_runtime_attributes(&mut payload, &session, &runtime);

        let player = &payload["current_state"]["runtime_attributes"]["player"];
        let attrs = player["attributes"].as_array().unwrap();
        assert_eq!(attrs.len(), 1);
        assert_eq!(attrs[0]["key"], serde_json::json!("player-attr"));
    }

    #[test]
    fn append_runtime_attributes_ignores_foreign_owner_with_same_character_suffix() {
        // 其它来源的 owner_id 即使以 ":li" 结尾(旧后缀匹配会误中),也不能当作玩家组。
        let mut session = sample_session();
        session.player_character_id = "li".to_string();
        let mut payload = serde_json::json!({ "current_state": {} });
        let runtime = crate::models::session::SessionRuntimeAttributesResponse {
            session_attributes: vec![],
            character_attributes: vec![runtime_attribute_group("other-sess:li", "foreign-attr")],
        };

        append_runtime_attributes(&mut payload, &session, &runtime);

        let player = &payload["current_state"]["runtime_attributes"]["player"];
        let attrs = player["attributes"].as_array().unwrap();
        assert!(attrs.is_empty());
    }

    #[test]
    fn resolve_director_prompt_is_empty_when_world_prompt_empty() {
        let service = WorldDirectorService::new();
        let world = sample_world(serde_json::json!({}));

        let prompt = service.resolve_director_system_prompt(&world);

        assert!(prompt.trim().is_empty());
    }

    #[test]
    fn parse_runtime_payload_drops_director_authored_speech_line() {
        let service = WorldDirectorService::new();
        let world = sample_world(serde_json::json!({}));
        let session = sample_session();
        let parsed = serde_json::json!({
            "current_line": "Li Bai said: poem",
            "planned_speakers": ["Alice"]
        });

        let payload = service.parse_runtime_payload(&parsed, &session, &world, "continue");

        assert!(payload.current_line.is_none());
    }

    #[test]
    fn build_tool_followup_request_rejects_response_body_tool_calls() {
        let service = WorldDirectorService::new();
        let request = ChatRequest {
            model: "model".to_string(),
            messages: vec![crate::services::llm::client::ChatMessage {
                role: "system".to_string(),
                content: serde_json::json!("prompt"),
                reasoning_content: None,
                speaker: None,
                tool_call_id: None,
                tool_calls: None,
                metadata: None,
            }],
            generation: GenerationParams {
                temperature: Some(0.7),
                max_tokens: Some(500),
                ..Default::default()
            },
            stream: Some(false),
            json_mode: Some(true),
            response_schema: None,
            tools: None,
            tool_choice: None,
            native_tool_calling: None,
        };
        let parsed = serde_json::json!({
            "tool_calls": [
                { "id": "call-1", "tool_name": "list_scenes", "arguments": {} }
            ]
        });
        let tool_enriched = serde_json::json!({
            "tool_results": [
                { "id": "call-1", "tool_name": "list_scenes", "ok": true }
            ]
        });

        let error = service
            .build_tool_followup_request(&request, &parsed, &tool_enriched, false, None)
            .expect_err("response body tool calls should be rejected");

        assert!(error.contains("native tool_calls"));
    }

    #[test]
    fn build_tool_followup_request_uses_native_tool_messages_when_requested() {
        let service = WorldDirectorService::new();
        let request = ChatRequest {
            model: "model".to_string(),
            messages: vec![crate::services::llm::client::ChatMessage {
                role: "system".to_string(),
                content: serde_json::json!("prompt"),
                reasoning_content: None,
                speaker: None,
                tool_call_id: None,
                tool_calls: None,
                metadata: None,
            }],
            generation: GenerationParams {
                temperature: Some(0.7),
                max_tokens: Some(500),
                ..Default::default()
            },
            stream: Some(false),
            json_mode: Some(true),
            response_schema: None,
            tools: Some(vec![ChatToolDefinition {
                name: "list_scenes".to_string(),
                description: Some("List scenes".to_string()),
                input_schema: serde_json::json!({ "type": "object" }),
            }]),
            tool_choice: Some(ChatToolChoice::Auto),
            native_tool_calling: Some(true),
        };
        let parsed = serde_json::json!({
            "tool_calls": [
                { "id": "call-1", "tool_name": "list_scenes", "arguments": {} }
            ]
        });
        let tool_enriched = serde_json::json!({
            "tool_results": [
                { "id": "call-1", "tool_name": "list_scenes", "ok": true, "result": { "scenes": [] } }
            ]
        });

        let followup = service
            .build_tool_followup_request(&request, &parsed, &tool_enriched, true, None)
            .expect("followup request");

        assert_eq!(followup.messages.len(), 3);
        assert_eq!(followup.messages[1].role, "assistant");
        assert_eq!(followup.messages[2].role, "tool");
        assert_eq!(followup.messages[2].tool_call_id.as_deref(), Some("call-1"));
        assert_eq!(
            followup.messages[1]
                .tool_calls
                .as_ref()
                .map(|items| items.len()),
            Some(1)
        );
    }

    #[test]
    fn native_tool_calls_override_response_body_tool_calls() {
        let service = WorldDirectorService::new();
        let parsed_body = serde_json::json!({
            "world_phase": "runtime",
            "tool_calls": [
                { "id": "body-call", "tool_name": "change_scene", "arguments": { "scene_name": "Body" } }
            ]
        });
        let native_calls = vec![ChatToolCall {
            id: "native-call".to_string(),
            tool_name: "list_scenes".to_string(),
            arguments: serde_json::json!({}),
        }];

        let stripped = service.remove_response_body_tool_calls(&parsed_body);
        let merged = service.merge_native_tool_calls(&stripped, Some(&native_calls));
        let tool_calls = merged
            .get("tool_calls")
            .and_then(|value| value.as_array())
            .expect("native tool calls should be merged");

        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0]["id"], "native-call");
        assert_eq!(tool_calls[0]["tool_name"], "list_scenes");
    }

    #[test]
    fn resolve_runtime_stage_label_uses_world_config() {
        let service = WorldDirectorService::new();
        let world = sample_world(serde_json::json!({
            "director_stage_labels": {
                "default_turn": "棣栬疆",
                "tool_loop_turn": "tool turn"
            }
        }));

        let default_stage = service.resolve_runtime_stage_label(&world, &[]);
        assert_eq!(default_stage, "棣栬疆");

        let tool_loop_stage = service.resolve_runtime_stage_label(
            &world,
            &[crate::services::llm::client::ChatMessage {
                role: "user".to_string(),
                content: serde_json::json!("{}"),
                reasoning_content: None,
                speaker: None,
                tool_call_id: None,
                tool_calls: None,
                metadata: Some(serde_json::json!({ "tool_phase": true })),
            }],
        );
        assert_eq!(tool_loop_stage, "tool turn");
    }

    #[test]
    fn should_continue_tool_loop_requires_model_to_return_tool_calls() {
        let service = WorldDirectorService::new();
        let world = sample_world(serde_json::json!({ "director_tool_loop_limit": 4 }));

        assert!(service.should_continue_tool_loop(
            &world,
            &serde_json::json!({
                "tool_calls": [{ "tool_name": "list_scenes", "arguments": {} }]
            }),
            1,
        ));
        assert!(!service.should_continue_tool_loop(
            &world,
            &serde_json::json!({
                "tool_results": [{ "tool_name": "list_scenes", "ok": true }]
            }),
            1,
        ));
        assert!(!service.should_continue_tool_loop(
            &world,
            &serde_json::json!({
                "tool_calls": [{ "tool_name": "list_scenes", "arguments": {} }]
            }),
            4,
        ));
    }

    #[test]
    fn parse_runtime_payload_respects_scene_transition_flag() {
        let service = WorldDirectorService::new();
        let world = sample_world(serde_json::json!({ "allow_scene_transition": false }));
        let session = sample_session();
        let parsed = serde_json::json!({
            "next_location": "Tower",
            "next_scene_name": "Tower",
            "next_time_label": "Dawn",
            "scene_visible_characters": ["Alice"],
            "planned_speakers": ["Alice"],
        });

        let payload = service.parse_runtime_payload(&parsed, &session, &world, "move");
        assert_eq!(payload.next_location, "Dock");
        assert_eq!(payload.next_scene_name, "Dock");
        assert_eq!(payload.next_time_label, "Dawn");
    }

    #[test]
    fn director_interaction_waits_for_player_before_speakers_run() {
        let service = WorldDirectorService::new();
        let world = sample_world(serde_json::json!({
            "director_interaction_kinds": ["choice"]
        }));
        let session = sample_session();
        let parsed = serde_json::json!({
            "planned_speakers": ["Alice"],
            "interaction": {
                "kind": "choice",
                "prompt": "Choose a route",
                "config": {
                    "options": [
                        { "id": "river", "label": "River path" },
                        { "id": "ridge", "label": "Ridge path" }
                    ]
                }
            }
        });

        let payload = service.parse_runtime_payload(&parsed, &session, &world, "continue");

        assert!(payload.planned_speakers.is_empty());
        let interaction = payload.interaction.expect("director interaction");
        assert_eq!(interaction.kind, "choice");
        assert_eq!(interaction.status, INTERACTION_STATUS_PENDING);
    }

    #[test]
    fn parse_runtime_payload_sanitizes_switch_character_proposal() {
        let service = WorldDirectorService::new();
        let world = sample_world(serde_json::json!({ "allow_scene_transition": true }));
        let session = sample_session();
        let parsed = serde_json::json!({
            "switch_character_proposal": {
                "target_character_name": "Alice",
                "reason": "Need stealth expert",
                "scene_character_roster": ["Alice", "Player", "Bob"],
                "scene_name": "Warehouse"
            }
        });

        let payload = service.parse_runtime_payload(&parsed, &session, &world, "switch");
        let proposal = payload
            .switch_character_proposal
            .expect("switch proposal should exist");
        let visible = proposal
            .get("scene_character_roster")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();
        let visible_names = visible
            .iter()
            .filter_map(|item| item.as_str())
            .collect::<Vec<_>>();

        assert!(!visible_names.contains(&"Player"));
        assert!(!visible_names.contains(&"Alice"));
        assert!(visible_names.contains(&"Bob"));
    }

    #[test]
    fn parse_runtime_payload_drops_switch_proposal_when_disabled() {
        let service = WorldDirectorService::new();
        let world = sample_world(serde_json::json!({
            "allow_player_character_switch": false
        }));
        let session = sample_session();
        let parsed = serde_json::json!({
            "switch_character_proposal": {
                "target_character_name": "Alice",
                "reason": "Need stealth expert"
            }
        });

        let payload = service.parse_runtime_payload(&parsed, &session, &world, "switch");

        assert!(payload.switch_character_proposal.is_none());
    }

    #[test]
    fn build_runtime_turn_payload_uses_unambiguous_character_keys() {
        let service = WorldDirectorService::new();
        let world = sample_world(serde_json::json!({}));
        let session = sample_session();
        let characters = vec![
            CharacterDefinition {
                id: "char-alice".to_string(),
                name: "Alice".to_string(),
                world_id: "world-1".to_string(),
                role: "Scout".to_string(),
                background_prompt: String::new(),
                model: "test-model".to_string(),
                memory_strategy: "recent".to_string(),
                recent_dialogue_rounds: 6,
                attributes: vec![],
                portrait_assets: vec![],
                avatar_asset: String::new(),
                system_prompt_template: String::new(),
                response_contract_prompt: String::new(),
                narration_prompt: String::new(),
                runtime_system_prompt: String::new(),
            },
            CharacterDefinition {
                id: "char-bob".to_string(),
                name: "Bob".to_string(),
                world_id: "world-1".to_string(),
                role: "Guard".to_string(),
                background_prompt: String::new(),
                model: "test-model".to_string(),
                memory_strategy: "recent".to_string(),
                recent_dialogue_rounds: 6,
                attributes: vec![],
                portrait_assets: vec![],
                avatar_asset: String::new(),
                system_prompt_template: String::new(),
                response_contract_prompt: String::new(),
                narration_prompt: String::new(),
                runtime_system_prompt: String::new(),
            },
        ];

        let payload =
            service.build_runtime_turn_payload(&world, &session, &characters, "hello", Vec::new());

        assert_eq!(
            payload
                .get("basic_setting")
                .and_then(|value| value.get("world_character_roster"))
                .and_then(|value| value.as_array())
                .map(|items| items.len()),
            Some(2)
        );
        assert!(payload.get("available_characters").is_none());
        assert!(payload
            .get("current_state")
            .and_then(|value| value.get("current_scene_character_roster"))
            .is_some());
        assert!(payload
            .get("current_state")
            .and_then(|value| value.get("visible_characters"))
            .is_none());
        assert!(payload
            .get("current_state")
            .and_then(|value| value.get("scene_present_characters"))
            .is_none());
    }

    #[test]
    fn build_runtime_turn_payload_uses_minimal_director_contract() {
        let service = WorldDirectorService::new();
        let world = sample_world(serde_json::json!({}));
        let session = sample_session();
        let payload =
            service.build_runtime_turn_payload(&world, &session, &[], "hello", Vec::new());

        let current_state = payload
            .get("current_state")
            .and_then(|value| value.as_object())
            .expect("current_state");
        assert!(payload
            .get("basic_setting")
            .and_then(|value| value.get("opening_scene"))
            .is_none());
        assert!(!current_state.contains_key("scene_name"));
        assert!(!current_state.contains_key("state_tags"));
        assert!(!current_state.contains_key("system_log"));

        let response_contract = payload
            .get("response_contract")
            .and_then(|value| value.as_object())
            .expect("response contract");
        assert_eq!(
            response_contract
                .get("return_policy")
                .and_then(|value| value.as_str()),
            Some("return_changed_fields_only")
        );
        assert_eq!(
            response_contract
                .get("forbidden_fields")
                .and_then(|value| value.as_array())
                .map(|items| items
                    .iter()
                    .filter_map(|item| item.as_str())
                    .collect::<Vec<_>>()),
            Some(vec!["state_tags", "system_messages", "system_log"])
        );
        assert!(!response_contract.contains_key("tool_call_fallback_field"));
        let optional_fields = response_contract
            .get("optional_fields_when_changed")
            .and_then(|value| value.as_array())
            .map(|items| items
                .iter()
                .filter_map(|item| item.as_str())
                .collect::<Vec<_>>())
            .unwrap_or_default();
        assert!(optional_fields.contains(&"session_attribute_updates"));
        assert!(optional_fields.contains(&"character_attribute_updates"));
        assert!(response_contract.contains_key("runtime_update_format"));
        assert!(response_contract
            .get("notes")
            .and_then(|value| value.as_array())
            .map(|items| items
                .iter()
                .filter_map(|item| item.as_str())
                .any(|item| item.contains(
                    "player character name in scene_visible_characters or planned_speakers"
                )))
            .unwrap_or(false));
    }

    #[test]
    fn director_response_schema_omits_removed_runtime_log_fields() {
        let service = WorldDirectorService::new();
        let world = sample_world(serde_json::json!({}));
        let schema = service.build_director_response_schema(&world);
        let properties = schema
            .get("properties")
            .and_then(|value| value.as_object())
            .expect("schema properties");

        assert!(properties.contains_key("planned_speakers"));
        assert!(properties.contains_key("next_scene_background_hint"));
        assert!(properties.contains_key("next_scene_tags"));
        assert!(properties.contains_key("generated_characters"));
        assert!(properties.contains_key("session_attribute_updates"));
        assert!(properties.contains_key("character_attribute_updates"));
        assert!(!properties.contains_key("tool_calls"));
        assert!(!properties.contains_key("state_tags"));
        assert!(!properties.contains_key("system_messages"));
        assert!(!properties.contains_key("system_log"));
        let generated = properties
            .get("generated_characters")
            .and_then(|value| value.get("items"))
            .and_then(|value| value.as_object())
            .expect("generated characters schema");
        let required = generated
            .get("required")
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        assert!(required.contains(&"name"));
        assert!(required.contains(&"role"));
        assert!(required.contains(&"background_prompt"));
    }

    #[test]
    fn repair_common_json_issues_escapes_bare_newlines_inside_strings() {
        let broken = "{\"current_line\": \"第一行\n第二行\"}";
        let repaired = repair_common_json_issues(broken);
        let parsed =
            serde_json::from_str::<serde_json::Value>(&repaired).expect("修复后应能解析");
        assert_eq!(
            parsed.get("current_line").and_then(|value| value.as_str()),
            Some("第一行\n第二行")
        );
    }

    #[test]
    fn repair_common_json_issues_removes_bare_carriage_returns_in_strings() {
        let broken = "{\"current_line\": \"甲\r\n乙\"}";
        let repaired = repair_common_json_issues(broken);
        let parsed =
            serde_json::from_str::<serde_json::Value>(&repaired).expect("修复后应能解析");
        assert_eq!(
            parsed.get("current_line").and_then(|value| value.as_str()),
            Some("甲\n乙")
        );
    }

    #[test]
    fn repair_common_json_issues_completes_truncated_json() {
        let truncated = "{\"world_phase\": \"opening\", \"planned_speakers\": [\"Alice\"";
        let repaired = repair_common_json_issues(truncated);
        let parsed =
            serde_json::from_str::<serde_json::Value>(&repaired).expect("补全后应能解析");
        assert_eq!(
            parsed.get("world_phase").and_then(|value| value.as_str()),
            Some("opening")
        );
        assert_eq!(
            parsed
                .get("planned_speakers")
                .and_then(|value| value.as_array())
                .map(|items| items.len()),
            Some(1)
        );
    }

    #[test]
    fn repair_common_json_issues_completes_unterminated_string() {
        let truncated = "{\"current_line\": \"雾从江面升起";
        let repaired = repair_common_json_issues(truncated);
        let parsed =
            serde_json::from_str::<serde_json::Value>(&repaired).expect("补全后应能解析");
        assert_eq!(
            parsed.get("current_line").and_then(|value| value.as_str()),
            Some("雾从江面升起")
        );
    }

    #[test]
    fn repair_common_json_issues_keeps_valid_json_untouched() {
        let valid = "{\"a\": 1, \"b\": [1, 2], \"c\": \"x\\n\"}";
        assert_eq!(repair_common_json_issues(valid), valid);
    }

    #[test]
    fn director_output_needs_json_repair_only_for_non_object_or_empty() {
        assert!(director_output_needs_json_repair(&serde_json::Value::Null));
        assert!(director_output_needs_json_repair(&serde_json::json!(
            "just text"
        )));
        assert!(director_output_needs_json_repair(&serde_json::json!({})));
        assert!(!director_output_needs_json_repair(&serde_json::json!({
            "planned_speakers": ["Alice"]
        })));
    }

    #[test]
    fn build_director_json_repair_request_appends_output_and_feedback() {
        let request = ChatRequest {
            model: "model".to_string(),
            messages: vec![crate::services::llm::client::ChatMessage {
                role: "system".to_string(),
                content: serde_json::json!("prompt"),
                reasoning_content: None,
                speaker: None,
                tool_call_id: None,
                tool_calls: None,
                metadata: None,
            }],
            generation: GenerationParams::default(),
            stream: Some(false),
            json_mode: Some(true),
            response_schema: None,
            tools: None,
            tool_choice: None,
            native_tool_calling: None,
        };

        let repaired = build_director_json_repair_request(&request, "{broken json");

        assert_eq!(repaired.messages.len(), 3);
        assert_eq!(repaired.messages[1].role, "assistant");
        assert_eq!(
            repaired.messages[1].content.as_str(),
            Some("{broken json")
        );
        assert_eq!(repaired.messages[2].role, "user");
        let feedback = repaired.messages[2]
            .content
            .as_str()
            .expect("feedback text");
        assert!(feedback.contains("无法解析为 JSON 对象"));
        assert!(feedback.contains("只输出 JSON 本身"));
        // 原请求的 model/json_mode 等参数保持不变
        assert_eq!(repaired.model, "model");
        assert_eq!(repaired.json_mode, Some(true));
    }
}
