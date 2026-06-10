use crate::models::character::CharacterCreateRequest;
use crate::models::character::CharacterDefinition;
use crate::db::Database;
use crate::models::mcp_tool::{McpToolDefinition, MCP_TOOL_SCHEDULE_NOTIFICATION_ID};
use crate::models::model_config::ModelConfig;
use crate::models::session::{ChatMessage, MessageContent, SessionSnapshot};
use crate::models::world::WorldDefinition;
use crate::services::game_engine::prompting::{
    build_prompt_call, llm_chat_messages_to_values, render_prompt_variables,
    resolve_runtime_context_prompt,
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

#[derive(Debug, Clone, Default)]
pub struct WorldDirectorService;

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
    ) -> serde_json::Value {
        let history_rounds = self.resolve_director_history_rounds(world);
        let chat_history = self.build_history_dialogue(
            &session.messages,
            history_rounds,
            Some(session.player_character_name.as_str()),
        );
        let payload = self.build_runtime_turn_payload_with_mcp_tools(
            world,
            session,
            characters,
            player_input,
            chat_history.clone(),
            mcp_tools,
        );
        let system_prompt = self.resolve_director_system_prompt(world);
        let runtime_context_prompt = resolve_runtime_context_prompt(world);
        let payload_text =
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string());
        let tool_loop_messages = tool_loop_messages.unwrap_or_default();
        return build_prompt_call(
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
                &payload_text,
                tool_loop_messages.clone(),
            ),
            self.build_director_prompt_modules(
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
        let mut modules = self.build_prompt_presets(world, session, characters);
        modules.push(serde_json::json!({
            "name": "???????",
            "source": "???? / ???????",
            "content": system_prompt,
            "editable": true,
            "sent": true
        }));
        modules.push(serde_json::json!({
            "name": "瀹㈣涓栫晫璧勬枡",
            "source": "???????",
            "content": serde_json::to_string_pretty(payload.get("basic_setting").unwrap_or(&serde_json::Value::Null)).unwrap_or_else(|_| "{}".to_string()),
            "editable": false,
            "sent": true
        }));
        modules.push(serde_json::json!({
            "name": "Current state",
            "source": "Runtime state",
            "content": serde_json::to_string_pretty(payload.get("current_state").unwrap_or(&serde_json::Value::Null)).unwrap_or_else(|_| "{}".to_string()),
            "editable": false,
            "sent": true
        }));
        modules.push(serde_json::json!({
            "name": "Chat history",
            "source": "Session history",
            "content": serde_json::to_string_pretty(payload.get("chat_history").unwrap_or(&serde_json::Value::Null)).unwrap_or_else(|_| "[]".to_string()),
            "editable": false,
            "sent": true
        }));
        modules.push(serde_json::json!({
            "name": "Tool data",
            "source": "System tool registry",
            "content": serde_json::to_string_pretty(payload.get("tool_data").unwrap_or(&serde_json::Value::Null)).unwrap_or_else(|_| "{}".to_string()),
            "editable": false,
            "sent": true
        }));
        serde_json::json!({
            "schema_version": "prompt_call_v1",
            "recipient_type": "director",
            "recipient_name": "world_director",
            "stage": stage,
            "purpose": "Decide world state, tool calls and speaker order",
            "modules": modules,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": payload_text }
            ],
            "final_sent_content": format!("{}\n\n{}", system_prompt, payload_text),
            "raw_model_return": serde_json::Value::Null,
            "return_processing": serde_json::Value::Null,
            "processed_model_return": serde_json::Value::Null,
            "written_result": serde_json::Value::Null,
            "raw_debug": {
                "payload": payload,
                "tool_loop_messages": tool_loop_messages
            }
        })
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
        user_prompt: &str,
        tool_loop_messages: Vec<serde_json::Value>,
    ) -> Vec<serde_json::Value> {
        let mut messages = vec![
            serde_json::json!({
                "role": "system",
                "content": system_prompt,
            }),
        ];
        if !runtime_context_prompt.trim().is_empty() {
            messages.push(serde_json::json!({
                "role": "system",
                "content": runtime_context_prompt,
            }));
        }
        messages.push(
            serde_json::json!({
                "role": "user",
                "content": user_prompt,
            }),
        );
        messages.extend(tool_loop_messages);
        messages
    }

    fn build_director_prompt_modules(
        &self,
        world: &WorldDefinition,
        session: &SessionSnapshot,
        characters: &[CharacterDefinition],
        payload: &serde_json::Value,
        system_prompt: &str,
        runtime_context_prompt: &str,
    ) -> Vec<serde_json::Value> {
        let mut modules = self.build_prompt_presets(world, session, characters);
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
        serde_json::json!({
            "basic_setting": {
                "world_name": session.world_name,
                "background_prompt": world.background_prompt,
                "opening_scene": world.opening_scene,
                "world_character_roster": world_character_roster,
                "time_system": world.time_system,
            },
            "current_state": {
                "player_input": player_input,
                "player_character_name": session.player_character_name,
                "location": session.location,
                "time_label": session.time_label,
                "current_scene_character_roster": session.visible_characters,
                "scene_name": session.scene.name,
                "scene_tags": session.scene.temporary_tags,
                "state_metrics": session.state.metrics,
                "inventory_items": session.inventory_items,
            },
            "chat_history": chat_history,
            "tool_data": {
                "available_tools": self.build_director_tool_capabilities(world, mcp_tools),
                "tool_protocol": self.build_tool_protocol(),
                "visual_capabilities": visual_capabilities
            },
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
                    "character_visual_directives"
                ],
                "forbidden_fields": [
                    "state_tags",
                    "system_messages",
                    "system_log"
                ],
                "notes": [
                    "Omit unchanged fields.",
                    "Do not rebuild the full session state.",
                    "Only include current_line when a non-dialogue scene update is necessary.",
                    "Do not include the player character name in scene_visible_characters or planned_speakers; the player is implicitly present.",
                    "If you introduce a character who is not already in current_scene_character_roster or world_character_roster, create that character first by returning generated_characters, then include the same name in scene_visible_characters / planned_speakers.",
                    "Each generated_characters item must include at least name, role and background_prompt.",
                    "background_prompt must be a usable portrayal brief for the later character model, not just a label or one-word tag.",
                    "Do not place a new character directly into scene_visible_characters or planned_speakers without generated_characters.",
                    "Use native tool calling whenever a tool is needed.",
                    "Do not return a tool_calls field inside the JSON body."
                ]
            }
        })
    }

    fn build_director_response_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "additionalProperties": true,
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
                }
            }
        })
    }

    pub fn build_chat_request_from_prompt_call(
        &self,
        prompt_call: &serde_json::Value,
        model_id: &str,
        max_tokens: i32,
        stream_enabled: bool,
    ) -> ChatRequest {
        let messages = prompt_call
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
        let tools = prompt_call
            .get("raw_debug")
            .and_then(|value| value.get("payload"))
            .and_then(|value| value.get("tool_data"))
            .and_then(|value| value.get("available_tools"))
            .and_then(|value| value.as_array())
            .map(|items| {
                items
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
                    .collect::<Vec<_>>()
            })
            .filter(|items| !items.is_empty());
        let native_tools_active = tools
            .as_ref()
            .map(|items| !items.is_empty())
            .unwrap_or(false);
        ChatRequest {
            model: model_id.to_string(),
            messages,
            temperature: Some(0.7),
            max_tokens: Some(max_tokens),
            stream: Some(stream_enabled && !native_tools_active),
            json_mode: Some(true),
            response_schema: Some(self.build_director_response_schema()),
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
    ) -> serde_json::Value {
        let system_prompt = request_messages
            .first()
            .map(|message| message.content_text())
            .unwrap_or_default();
        let history_rounds = self.resolve_director_history_rounds(world);
        let chat_history = self.build_history_dialogue(
            &session.messages,
            history_rounds,
            Some(session.player_character_name.as_str()),
        );
        let payload =
            self.build_runtime_turn_payload(world, session, characters, player_input, chat_history);
        let user_prompt = request_messages
            .iter()
            .rev()
            .find(|message| message.role == "user")
            .map(|message| message.content_text())
            .unwrap_or_default();
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
        _loop_limit: usize,
        turn_index: i32,
        notification_runtime: Option<NotificationToolRuntime<'_>>,
        mut progress_callback: Option<&mut (dyn FnMut(DirectorLoopStreamProgress) + Send)>,
    ) -> Result<DirectorLoopRunResult, String> {
        let mut active_request = initial_request;
        let mut traces = Vec::new();
        loop {
            let started = std::time::Instant::now();
            let request_used = active_request.clone();
            let native_tools_active = active_request.native_tool_calling.unwrap_or(false)
                && active_request
                    .tools
                    .as_ref()
                    .map(|tools| !tools.is_empty())
                    .unwrap_or(false);
            let response = if model.streaming_enabled && !native_tools_active {
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
                "request": serde_json::to_value(&request_used).unwrap_or_default(),
            });
            let response_value = serde_json::json!({
                "provider": provider,
                "model_id": model.model_id,
                "status": "completed",
                "latency_ms": started.elapsed().as_millis() as i64,
                "response": serde_json::to_value(&response).unwrap_or_default(),
            });
            let tool_enriched = self.apply_tool_call_effects_with_notifications(
                &parsed,
                session,
                world,
                characters,
                notification_runtime,
                turn_index,
            );
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
                return Ok(DirectorLoopRunResult {
                    parsed: tool_enriched,
                    traces,
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
        self.apply_tool_call_effects_with_notifications(parsed, session, world, characters, None, 0)
    }

    fn apply_tool_call_effects_with_notifications(
        &self,
        parsed: &serde_json::Value,
        session: &SessionSnapshot,
        world: &WorldDefinition,
        characters: &[CharacterDefinition],
        notification_runtime: Option<NotificationToolRuntime<'_>>,
        turn_index: i32,
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
        for tool_call in &tool_calls {
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
                    let scene_name = arg_string(&arguments, "scene_name")
                        .or_else(|| arg_string(&arguments, "location"))
                        .unwrap_or_else(|| session.scene.name.clone());
                    let scene_description = arg_string(&arguments, "scene_description")
                        .or_else(|| arg_string(&arguments, "scene_background_hint"))
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
                    if let Some(new_characters) =
                        arguments.get("new_characters").and_then(|v| v.as_array())
                    {
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
                "switch_player_character" => {
                    let target_character_name =
                        arg_string(&arguments, "target_character_name").unwrap_or_default();
                    if !target_character_name.is_empty()
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
                                "reason": arg_string(&arguments, "reason").unwrap_or_else(|| "tool_switch".to_string()),
                                "location": arg_string(&arguments, "location").unwrap_or_else(|| session.location.clone()),
                                "scene_name": arg_string(&arguments, "scene_name").unwrap_or_else(|| session.scene.name.clone()),
                                "scene_background_hint": arg_string(&arguments, "scene_background_hint").unwrap_or_else(|| session.scene.background_hint.clone()),
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
                "generate_image" => {
                    let kind =
                        arg_string(&arguments, "kind").unwrap_or_else(|| "background".to_string());
                    let prompt = arg_string(&arguments, "prompt").unwrap_or_default();
                    if !prompt.is_empty() {
                        if kind == "portrait" {
                            if let Some(character_name) = arg_string(&arguments, "character_name") {
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
                "schedule_notification" => {
                    if !schedule_notification_allowed {
                        tool_results.push(serde_json::json!({
                            "id": call_id,
                            "tool_name": "schedule_notification",
                            "ok": false,
                            "error": "schedule_notification is not allowed for this world",
                        }));
                        continue;
                    }
                    if let Some(runtime) = notification_runtime {
                        let result = match Database::new(&runtime.data_dir.to_path_buf()) {
                            Ok(db) => NotificationScheduler::execute_tool_call(
                                db.conn(),
                                runtime.app,
                                runtime.data_dir,
                                NotificationToolContext {
                                    session_id: &session.id,
                                    world_id: &world.id,
                                    world_name: &world.name,
                                    turn_index,
                                },
                                &call_id,
                                &arguments,
                            ),
                            Err(error) => serde_json::json!({
                                "id": call_id,
                                "tool_name": "schedule_notification",
                                "tool_call_id": call_id,
                                "ok": false,
                                "error": error,
                            }),
                        };
                        tool_results.push(result);
                        continue;
                    }
                    match pending_notification_from_tool_call(&session.id, &call_id, &arguments) {
                        Ok(pending) => {
                            let scheduled_at = pending.scheduled_at.clone();
                            let body = pending.body.clone();
                            let title = pending.title.clone();
                            pending_notifications.push(
                                serde_json::to_value(&pending).unwrap_or_else(|_| {
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
                _ => {
                    tool_results.push(serde_json::json!({
                        "id": call_id,
                        "tool_name": tool_name,
                        "ok": false,
                        "error": format!("Tool execution is not implemented for custom MCP tool: {tool_name}"),
                    }));
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
                temperature: previous_request.temperature,
                max_tokens: previous_request.max_tokens,
                stream: previous_request.stream,
                json_mode: previous_request.json_mode,
                response_schema: previous_request.response_schema.clone(),
                tools: previous_request.tools.clone(),
                tool_choice: previous_request.tool_choice.clone(),
                native_tool_calling: previous_request.native_tool_calling,
            });
        }
        let _ = parsed;
        let _ = tool_enriched;
        Err("Director tool follow-up requires native tool_calls".to_string())
    }

    pub fn resolve_tool_loop_limit(&self, world: &WorldDefinition) -> usize {
        world
            .director_config
            .get("director_tool_loop_limit")
            .and_then(|value| value.as_i64())
            .map(|value| value.clamp(1, 12) as usize)
            .unwrap_or(4)
    }

    pub fn resolve_tool_call_limit(&self, world: &WorldDefinition) -> usize {
        world
            .director_config
            .get("director_tool_call_limit")
            .and_then(|value| value.as_i64())
            .map(|value| value.clamp(1, 8) as usize)
            .unwrap_or(4)
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
        let termination_mode = world
            .director_config
            .get("director_tool_loop_termination")
            .and_then(|value| value.as_str())
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .unwrap_or("tool_calls_present");
        match termination_mode {
            "tool_calls_present" => !self.extract_tool_calls(parsed, None).is_empty(),
            _ => !self.extract_tool_calls(parsed, None).is_empty(),
        }
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
        let scene_visible_characters = parse_scene_visible_characters(
            parsed.get("scene_visible_characters"),
            &session.player_character_name,
        );
        let merged_visible = if let Some(explicit) = scene_visible_characters.clone() {
            explicit
        } else {
            session.visible_characters.clone()
        };
        let planned_speakers = parse_planned_speakers(
            parsed.get("planned_speakers"),
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
            planned_speakers,
            generated_character_payloads,
            character_visual_directives,
            switch_character_proposal: parse_switch_character_proposal(
                parsed.get("switch_character_proposal"),
                &session.player_character_name,
            ),
        }
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
                "Each generated_characters item must contain a usable role and portrayal brief, not just a name.",
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
            .map(|character| {
                serde_json::json!({
                    "character_name": character.name,
                    "portrait_count": character.portrait_assets.len(),
                    "visible": session.visible_characters.iter().any(|name| name == &character.name),
                })
            })
            .collect::<Vec<_>>();
        serde_json::json!({
            "background_source_mode": background_source_mode,
            "portrait_source_mode": portrait_source_mode,
            "local_background_assets_count": local_background_assets_count,
            "local_scene_background_keys": local_scene_background_keys,
            "character_portrait_counts": character_portrait_counts,
            "runtime_image_generation_enabled": self
                .resolve_world_allowed_tool_ids(world)
                .iter()
                .any(|id| id == "mcp-tool-image-generation"),
        })
    }

    fn build_prompt_presets(
        &self,
        world: &WorldDefinition,
        session: &SessionSnapshot,
        _characters: &[CharacterDefinition],
    ) -> Vec<serde_json::Value> {
        let variables = self.template_variables(world, session, "");
        let mut presets = world
            .director_config
            .get("prompt_presets")
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
                if !enabled || !(scope == "both" || scope == "director") {
                    return None;
                }
                let content = self.render_template(
                    object
                        .get("content")
                        .and_then(|value| value.as_str())
                        .unwrap_or(""),
                    &variables,
                );
                if content.trim().is_empty() {
                    return None;
                }
                let name = object
                    .get("name")
                    .and_then(|value| value.as_str())
                    .unwrap_or("unnamed preset")
                    .trim()
                    .to_string();
                let order = object
                    .get("order")
                    .and_then(|value| value.as_i64())
                    .unwrap_or(0);
                Some(serde_json::json!({
                    "name": format!("Prompt preset {name}"),
                    "source": "World design / prompt preset",
                    "content": content,
                    "editable": true,
                    "sent": true,
                    "order": order,
                }))
            })
            .collect::<Vec<_>>();
        presets.sort_by_key(|item| {
            item.get("order")
                .and_then(|value| value.as_i64())
                .unwrap_or(0)
        });
        for item in &mut presets {
            if let Some(obj) = item.as_object_mut() {
                obj.remove("order");
            }
        }
        if !world.director_runtime_system_prompt.trim().is_empty() {
            presets.insert(
                0,
                serde_json::json!({
                    "name": "World director prompt",
                    "source": "World design / world director prompt",
                    "content": world.director_runtime_system_prompt.trim(),
                    "editable": true,
                    "sent": true
                }),
            );
        }
        presets
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

    fn render_template(
        &self,
        text: &str,
        variables: &std::collections::HashMap<String, String>,
    ) -> String {
        let mut rendered = text.to_string();
        for (key, value) in variables {
            rendered = rendered.replace(&format!("{{{{{key}}}}}"), value);
        }
        rendered
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
                // 淇濈暀鍘熷 content锛堝彲鑳芥槸瀛楃涓叉垨澶氬獟浣撴暟缁勶級
                let content = message.content.clone();
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

    fn build_director_tool_capabilities(
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
            serde_json::json!({
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
            }),
        ];
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
        for tool in mcp_tools {
            if !tool.enabled || !allowed.contains(&tool.id) || is_builtin_mcp_tool_id(&tool.id) {
                continue;
            }
            if mcp_tool_exposure_mode(&tool.exposure_policy) == "disabled" {
                continue;
            }
            let tool_name = tool.tool_name.trim();
            if tool_name.is_empty() {
                continue;
            }
            tools.push(serde_json::json!({
                "tool_name": tool_name,
                "description": tool.description.trim(),
                "arguments_schema": tool.input_schema.clone(),
                "server_name": tool.server_name.clone(),
                "mcp_tool_id": tool.id.clone(),
            }));
        }
        tools
    }



    fn resolve_world_allowed_tool_ids(&self, world: &WorldDefinition) -> Vec<String> {
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

fn repair_common_json_issues(raw: &str) -> String {
    raw.replace('\u{201c}', "\"")
        .replace('\u{201d}', "\"")
        .replace('\u{2018}', "'")
        .replace('\u{2019}', "'")
        .replace(",}", "}")
        .replace(",]", "]")
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

fn parse_planned_speakers(
    value: Option<&serde_json::Value>,
    visible_character_names: &[String],
    fallback: &[String],
    player_character_name: &str,
    player_input: &str,
    world_phase: &str,
    history_messages: &[ChatMessage],
) -> Vec<String> {
    let visible_set = visible_character_names
        .iter()
        .filter(|name| !name.trim().is_empty() && name.as_str() != player_character_name)
        .cloned()
        .collect::<BTreeSet<_>>();
    let parsed = parse_string_list(value)
        .into_iter()
        .filter(|name| visible_set.contains(name))
        .collect::<Vec<_>>();
    if !parsed.is_empty() {
        return parsed.into_iter().take(4).collect();
    }
    let fallback_visible = fallback
        .iter()
        .filter(|name| visible_set.contains(*name))
        .take(4)
        .cloned()
        .collect::<Vec<_>>();
    if !fallback_visible.is_empty() {
        return fallback_visible;
    }
    let visible = visible_set.into_iter().collect::<Vec<_>>();
    if visible.is_empty() {
        return Vec::new();
    }
    let speaker_limit = resolve_speaker_limit(player_input, world_phase, &visible);
    let mentioned = mentioned_character_names(player_input, &visible);
    let mut ranked = visible
        .iter()
        .map(|name| {
            let mut score = 1.0f64;
            if mentioned.iter().any(|item| item == name) {
                score += 0.85;
            }
            score += recent_speaker_penalty(history_messages, name);
            (name.clone(), score)
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut selected = Vec::new();
    for name in mentioned {
        if !selected.contains(&name) {
            selected.push(name);
        }
        if selected.len() >= speaker_limit {
            break;
        }
    }
    for (name, _) in ranked {
        if selected.contains(&name) {
            continue;
        }
        selected.push(name);
        if selected.len() >= speaker_limit {
            break;
        }
    }
    if selected.is_empty() {
        return Vec::new();
    }
    selected
}

fn resolve_speaker_limit(
    player_input: &str,
    world_phase: &str,
    visible_character_names: &[String],
) -> usize {
    let visible_count = visible_character_names.len();
    if visible_count <= 1 {
        return visible_count;
    }
    let mentioned_count = mentioned_character_names(player_input, visible_character_names).len();
    let group_prompt = is_group_prompt(player_input);
    let mut limit = 2usize;
    if group_prompt || mentioned_count >= 2 || matches!(world_phase, "escalation" | "crisis") {
        limit = 3;
    }
    visible_count.min(limit.max(mentioned_count).max(1))
}

fn recent_speaker_penalty(history_messages: &[ChatMessage], character_name: &str) -> f64 {
    let recent_speakers = history_messages
        .iter()
        .rev()
        .filter(|message| message.role == "agent")
        .filter_map(|message| message.speaker.as_deref())
        .map(|speaker| speaker.trim().to_string())
        .filter(|speaker| !speaker.is_empty())
        .take(3)
        .collect::<Vec<_>>();
    let mut penalty = 0.0;
    for (index, recent_speaker) in recent_speakers.iter().enumerate() {
        if recent_speaker != character_name {
            continue;
        }
        penalty += match index {
            0 => -0.45,
            1 => -0.18,
            _ => -0.08,
        };
    }
    penalty
}

fn mentioned_character_names(
    player_input: &str,
    visible_character_names: &[String],
) -> Vec<String> {
    let input = player_input.trim();
    if input.is_empty() {
        return Vec::new();
    }
    let mut matched = visible_character_names
        .iter()
        .filter_map(|name| {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                return None;
            }
            input.find(trimmed).map(|idx| (idx, trimmed.to_string()))
        })
        .collect::<Vec<_>>();
    matched.sort_by_key(|item| item.0);
    matched.into_iter().fold(Vec::new(), |mut acc, (_, name)| {
        if !acc.contains(&name) {
            acc.push(name);
        }
        acc
    })
}

fn is_group_prompt(player_input: &str) -> bool {
    [
        "浣犱滑",
        "澶у",
        "鍚勪綅",
        "together",
        "鍒嗗埆",
        "杞祦",
        "閮借",
        "everyone",
        "鎸ㄤ釜",
    ]
    .iter()
    .any(|marker| player_input.contains(marker))
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

fn is_builtin_mcp_tool_id(id: &str) -> bool {
    matches!(
        id,
        "mcp-tool-list-scenes"
            | "mcp-tool-list-characters"
            | "mcp-tool-change-scene"
            | "mcp-tool-switch-player-character"
            | "mcp-tool-image-generation"
    ) || id == MCP_TOOL_SCHEDULE_NOTIFICATION_ID
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
            temperature: Some(0.7),
            max_tokens: Some(500),
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
            temperature: Some(0.7),
            max_tokens: Some(500),
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
        let schema = service.build_director_response_schema();
        let properties = schema
            .get("properties")
            .and_then(|value| value.as_object())
            .expect("schema properties");

        assert!(properties.contains_key("planned_speakers"));
        assert!(properties.contains_key("next_scene_background_hint"));
        assert!(properties.contains_key("next_scene_tags"));
        assert!(properties.contains_key("generated_characters"));
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
}
