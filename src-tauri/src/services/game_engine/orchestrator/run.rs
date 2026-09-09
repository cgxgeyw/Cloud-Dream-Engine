use crate::models::attribute::AttributeValue;
use crate::models::character::CharacterDefinition;
use crate::models::mcp_tool::McpToolDefinition;
use crate::models::model_config::ModelConfig;
use crate::models::scheduled_notification::PendingScheduledNotification;
use crate::models::session::*;
use crate::models::world::WorldDefinition;
use crate::services::assets::resolver::AssetResolver;
use crate::services::game_engine::director::{
    DirectorLoopIterationTrace, DirectorLoopStreamProgress, ParsedDirectorRuntimePayload,
    WorldDirectorService,
};
use crate::services::game_engine::runtime_effects::DirectorRuntimeApplication;
use crate::services::game_engine::structured_output::{
    validate_director_payload, StructuredOutputFailure,
};
use crate::services::llm::client::LlmClient;
use crate::services::notifications::NotificationToolRuntime;
use rusqlite::Connection;
use std::collections::HashMap;

use super::request_building::*;
use super::turn_context::*;
use super::writeback::*;

#[derive(Debug, Clone)]
pub struct DirectorDecision {
    pub world_phase: String,
    pub next_location: Option<String>,
    pub next_scene_name: Option<String>,
    pub next_scene_background_hint: Option<String>,
    pub scene_visible_characters: Vec<String>,
}

pub struct SessionOrchestrator;

pub struct DirectorTurnRecovery {
    pub resume_incomplete_turn: bool,
    pub recovered_completed_payload: Option<serde_json::Value>,
}

pub struct DirectorTurnRun {
    pub parsed: serde_json::Value,
    pub runtime_payload: ParsedDirectorRuntimePayload,
    pub traces: Vec<DirectorLoopIterationTrace>,
    pub trace_message: Option<DirectorTraceMessage>,
    pub model: ModelConfig,
    pub provider: String,
    pub tool_loop_limit: usize,
}

pub struct SpeakerTurnRunResult {
    pub messages: Vec<ChatMessage>,
    pub failure: Option<StructuredOutputFailure>,
    pub pending_notifications: Vec<PendingScheduledNotification>,
    pub runtime_payloads: Vec<serde_json::Value>,
}

pub struct PreparedTurnContext {
    pub session: SessionSnapshot,
    pub world: WorldDefinition,
    pub characters: Vec<CharacterDefinition>,
    pub turn_index: i32,
    pub recovery_journal: Vec<serde_json::Value>,
    pub resume_incomplete_turn: bool,
    pub image_model: Option<ModelConfig>,
    pub director_model: ModelConfig,
    pub messages: Vec<ChatMessage>,
    pub director_completed_payload: Option<serde_json::Value>,
}

pub struct SessionAssetContext {
    pub session: SessionSnapshot,
    pub world: WorldDefinition,
    pub characters: Vec<CharacterDefinition>,
    pub image_model: Option<ModelConfig>,
}

pub struct SwitchPlayerCharacterContext {
    pub session: SessionSnapshot,
    pub world: WorldDefinition,
    pub characters: Vec<CharacterDefinition>,
    pub new_character: CharacterDefinition,
    pub image_model: Option<ModelConfig>,
}

#[derive(Debug, Clone)]

pub struct DirectorRuntimePreparation {
    pub parsed_runtime: serde_json::Value,
    pub next_location: String,
    pub next_scene_name: String,
    pub current_line: Option<String>,
    pub next_scene_background_hint: String,
    pub next_time_label: String,
    pub scene_visible_characters: Option<Vec<String>>,
    pub scene_visible_characters_explicit: bool,
    pub planned_speakers: Vec<String>,
    pub visible_chars: Vec<String>,
    pub pre_runtime_system_messages: Vec<ChatMessage>,
}

#[derive(Debug, Clone)]

pub struct SpeakerTurnProgress {
    pub messages: Vec<ChatMessage>,
    pub speaker_name: String,
    pub narration: Option<String>,
    pub is_placeholder: bool,
    pub is_error: bool,
}

#[derive(Debug, Clone)]

pub struct DirectorTraceMessage {
    pub trace_text: String,
    pub trace_lines: Vec<String>,
    pub reasoning: Option<String>,
}

pub struct RuntimeMutationInput<'a> {
    pub asset_resolver: &'a AssetResolver,
    pub data_dir: &'a std::path::Path,
    pub session: &'a SessionSnapshot,
    pub messages: &'a [ChatMessage],
    pub world: &'a WorldDefinition,
    pub characters: &'a [CharacterDefinition],
    pub turn_index: i32,
    pub next_location: &'a str,
    pub next_time_label: &'a str,
    pub next_scene_name: &'a str,
    pub current_line: Option<&'a str>,
    pub next_scene_background_hint: String,
    pub planned_speakers: &'a [String],
    pub scene_visible_characters_explicit: bool,
    pub scene_visible_characters: &'a Option<Vec<String>>,
    pub visible_chars: &'a [String],
    pub runtime_application: &'a DirectorRuntimeApplication,
    pub image_model: Option<&'a ModelConfig>,
    pub parsed_runtime: &'a serde_json::Value,
}

pub struct SwitchPlayerCharacterInput<'a> {
    pub asset_resolver: &'a AssetResolver,
    pub data_dir: &'a std::path::Path,
    pub session: &'a SessionSnapshot,
    pub world: &'a WorldDefinition,
    pub characters: &'a [CharacterDefinition],
    pub new_character: &'a CharacterDefinition,
    pub proposal: Option<&'a SwitchCharacterProposal>,
    pub image_model: Option<&'a ModelConfig>,
}

pub struct TurnWritebackInput<'a> {
    pub conn: &'a Connection,
    pub director_service: &'a WorldDirectorService,
    pub recovery_journal: &'a [serde_json::Value],
    pub session_id: &'a str,
    pub turn_index: i32,
    pub runtime_application: &'a DirectorRuntimeApplication,
    pub updated: &'a SessionSnapshot,
    pub session: &'a SessionSnapshot,
    pub world: &'a WorldDefinition,
    pub characters: &'a [CharacterDefinition],
    pub director_runtime: &'a serde_json::Value,
    pub planned_speakers: &'a [String],
    pub scene_visible_characters: &'a Option<Vec<String>>,
    pub director_loop_traces: &'a [DirectorLoopIterationTrace],
    pub director_provider: &'a str,
    pub director_model: &'a ModelConfig,
    pub player_input: &'a str,
    pub director_tool_loop_limit: usize,
}

pub fn build_director_trace_message_from_stream_progress(
    progress: &DirectorLoopStreamProgress,
) -> DirectorTraceMessage {
    let tool_calls = progress
        .tool_enriched
        .get("tool_calls")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    let planned_speakers = progress
        .tool_enriched
        .get("planned_speakers")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect::<Vec<_>>();
    let next_scene_name = progress
        .tool_enriched
        .get("next_scene_name")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let next_location = progress
        .tool_enriched
        .get("next_location")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let world_phase = progress
        .tool_enriched
        .get("world_phase")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let mut trace_lines = Vec::new();
    if !world_phase.is_empty() {
        trace_lines.push(format!("\u{9636}\u{6bb5}\u{ff1a}{world_phase}"));
    }
    if !next_scene_name.is_empty() {
        trace_lines.push(format!("\u{573a}\u{666f}\u{ff1a}{next_scene_name}"));
    }
    if !next_location.is_empty() {
        trace_lines.push(format!("\u{5730}\u{70b9}\u{ff1a}{next_location}"));
    }
    if !planned_speakers.is_empty() {
        trace_lines.push(format!(
            "\u{53d1}\u{8a00}\u{987a}\u{5e8f}\u{ff1a}{}",
            planned_speakers.join(" / ")
        ));
    }
    if !tool_calls.is_empty() {
        let tool_names = tool_calls
            .iter()
            .filter_map(|item| {
                item.get("tool_name")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
            })
            .collect::<Vec<_>>();
        if !tool_names.is_empty() {
            trace_lines.push(format!(
                "\u{5de5}\u{5177}\u{8c03}\u{7528}\u{ff1a}{}",
                tool_names.join(" / ")
            ));
        }
    }
    DirectorTraceMessage {
        trace_text: if trace_lines.is_empty() {
            "\u{4e16}\u{754c}\u{4e3b}\u{63a7}\u{6b63}\u{5728}\u{601d}\u{8003}...".to_string()
        } else {
            trace_lines.join("\n")
        },
        trace_lines,
        reasoning: progress
            .reasoning
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
    }
}

pub fn build_streaming_director_trace_chat_message(
    trace_message: &DirectorTraceMessage,
    turn_index: i32,
) -> ChatMessage {
    ChatMessage {
        message_id: ChatMessage::generate_id(),
        created_at: chrono::Utc::now().to_rfc3339(),
        parent_message_id: None,
        role: "system".to_string(),
        content: MessageContent::Text(trace_message.trace_text.clone()),
        speaker: None,
        metadata: Some(serde_json::json!({
            "turn_index": turn_index,
            "action_type": "director_trace",
            "message_kind": "director_trace",
            "trace_source": "world_director",
            "trace_text": trace_message.trace_text,
            "trace_lines": trace_message.trace_lines,
            "reasoning": trace_message.reasoning,
            "reasoning_expanded": true,
            "world_phase": "",
            "next_scene_name": "",
            "next_location": "",
            "next_time_label": "",
            "planned_speakers": [],
        })),
    }
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;

impl SessionOrchestrator {
    pub async fn run_director_turn(
        &self,
        llm_client: &LlmClient,
        world_director: &WorldDirectorService,
        model: ModelConfig,
        recovery: DirectorTurnRecovery,
        session: &SessionSnapshot,
        world: &WorldDefinition,
        characters: &[CharacterDefinition],
        turn_index: i32,
        player_input: &str,
        player_media: &[crate::models::session::ContentPart],
        mcp_tools: &[McpToolDefinition],
        mcp_servers: &[crate::models::mcp_server::McpServerConfig],
        kv_vars: &std::collections::HashMap<String, String>,
        runtime_attributes: &crate::models::session::SessionRuntimeAttributesResponse,
        generation: &crate::models::generation_params::GenerationParams,
        notification_runtime: Option<NotificationToolRuntime<'_>>,
        mut progress_callback: Option<&mut (dyn FnMut(DirectorLoopStreamProgress) + Send)>,
    ) -> Result<DirectorTurnRun, StructuredOutputFailure> {
        let provider = normalize_provider_name(&model.provider);
        let tool_loop_limit = world_director.resolve_tool_loop_limit(world);

        // 恢复分支同样要把"是否被 token 上限截断"带出来，否则截断会在这条路径上
        // 退化成误导性的 json_parse_failed。
        let mut recovered_truncated_by_token_limit = false;
        let parsed = if recovery.resume_incomplete_turn {
            if let Some(payload) = recovery.recovered_completed_payload {
                payload
            } else {
                let prompt_call = world_director.build_runtime_prompt_call_with_mcp_tools(
                    world,
                    session,
                    characters,
                    player_input,
                    "director_decision",
                    None,
                    mcp_tools,
                    kv_vars,
                    Some(runtime_attributes),
                    player_media,
                );
                let request = world_director.build_chat_request_from_prompt_call(
                    &prompt_call,
                    world,
                    &model.model_id,
                    generation,
                    model.streaming_enabled,
                );
                let loop_result = if let Some(callback) = progress_callback.as_deref_mut() {
                    world_director
                        .run_director_tool_loop(
                            llm_client,
                            &provider,
                            &model,
                            session,
                            world,
                            characters,
                            request,
                            turn_index,
                            mcp_tools,
                            mcp_servers,
                            notification_runtime,
                            Some(callback),
                        )
                        .await
                        .map_err(|error| {
                            build_director_transport_failure(
                                &provider,
                                &model,
                                turn_index,
                                player_input,
                                &error,
                            )
                        })?
                } else {
                    world_director
                        .run_director_tool_loop(
                            llm_client,
                            &provider,
                            &model,
                            session,
                            world,
                            characters,
                            request,
                            turn_index,
                            mcp_tools,
                            mcp_servers,
                            notification_runtime,
                            None,
                        )
                        .await
                        .map_err(|error| {
                            build_director_transport_failure(
                                &provider,
                                &model,
                                turn_index,
                                player_input,
                                &error,
                            )
                        })?
                };
                recovered_truncated_by_token_limit = loop_result.truncated_by_token_limit;
                loop_result.parsed
            }
        } else {
            let prompt_call = world_director.build_runtime_prompt_call_with_mcp_tools(
                world,
                session,
                characters,
                player_input,
                "director_decision",
                None,
                mcp_tools,
                kv_vars,
                Some(runtime_attributes),
                player_media,
            );
            let request = world_director.build_chat_request_from_prompt_call(
                &prompt_call,
                world,
                &model.model_id,
                generation,
                model.streaming_enabled,
            );
            let loop_result = if let Some(callback) = progress_callback.as_deref_mut() {
                world_director
                    .run_director_tool_loop(
                        llm_client,
                        &provider,
                        &model,
                        session,
                        world,
                        characters,
                        request,
                        turn_index,
                        mcp_tools,
                        mcp_servers,
                        notification_runtime,
                        Some(callback),
                    )
                    .await
                    .map_err(|error| {
                        build_director_transport_failure(
                            &provider,
                            &model,
                            turn_index,
                            player_input,
                            &error,
                        )
                    })?
            } else {
                world_director
                    .run_director_tool_loop(
                        llm_client,
                        &provider,
                        &model,
                        session,
                        world,
                        characters,
                        request,
                        turn_index,
                        mcp_tools,
                        mcp_servers,
                        notification_runtime,
                        None,
                    )
                    .await
                    .map_err(|error| {
                        build_director_transport_failure(
                            &provider,
                            &model,
                            turn_index,
                            player_input,
                            &error,
                        )
                    })?
            };
            let raw_text = loop_result
                .traces
                .last()
                .and_then(|trace| trace.response_value.get("response"))
                .and_then(|value| value.get("content"))
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let world_character_roster = characters
                .iter()
                .map(|character| character.name.clone())
                .collect::<Vec<_>>();
            validate_director_payload(
                &loop_result.parsed,
                &session.player_character_name,
                &session.visible_characters,
                &world_character_roster,
                &provider,
                &model.model_id,
                turn_index,
                raw_text,
                None,
                loop_result.truncated_by_token_limit,
            )?;
            let runtime_payload = world_director.parse_runtime_payload(
                &loop_result.parsed,
                session,
                world,
                player_input,
            );
            let trace_message = loop_result.traces.last().map(build_director_trace_message);
            return Ok(DirectorTurnRun {
                parsed: loop_result.parsed,
                runtime_payload,
                traces: loop_result.traces,
                trace_message,
                model,
                provider,
                tool_loop_limit,
            });
        };
        let world_character_roster = characters
            .iter()
            .map(|character| character.name.clone())
            .collect::<Vec<_>>();
        validate_director_payload(
            &parsed,
            &session.player_character_name,
            &session.visible_characters,
            &world_character_roster,
            &provider,
            &model.model_id,
            turn_index,
            "",
            None,
            recovered_truncated_by_token_limit,
        )?;
        let runtime_payload =
            world_director.parse_runtime_payload(&parsed, session, world, player_input);
        Ok(DirectorTurnRun {
            parsed,
            runtime_payload,
            traces: Vec::new(),
            trace_message: None,
            model,
            provider,
            tool_loop_limit,
        })
    }

    pub fn get_session_runtime_attributes(
        &self,
        conn: &Connection,
        session_id: &str,
    ) -> Result<SessionRuntimeAttributesResponse, String> {
        let session_repo = crate::db::repositories::session_repo::SessionRepository::new(conn);
        let session = session_repo
            .get(session_id)?
            .ok_or_else(|| "Session not found".to_string())?;
        let world = resolve_world_for_session(conn, &session)?;
        let character_names = crate::db::repositories::character_repo::CharacterRepository::new(conn)
            .list_by_world(&world.id)?
            .into_iter()
            .map(|character| (character.id, character.name))
            .collect::<HashMap<_, _>>();
        let attribute_repo =
            crate::db::repositories::attribute_repo::AttributeRepository::new(conn);
        let declared_attribute_keys = world
            .ui_theme_config
            .get("attribute_schemas")
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        Some((
                            item.get("scope")?.as_str()?.trim().to_string(),
                            item.get("key")?.as_str()?.trim().to_string(),
                        ))
                    })
                    .collect::<std::collections::HashSet<_>>()
            })
            .unwrap_or_default();
        let schema_map = attribute_repo
            .list_schemas(None)?
            .into_iter()
            .filter(|schema| {
                declared_attribute_keys.contains(&(schema.scope.clone(), schema.key.clone()))
                    || schema
                        .display_policy
                        .get("applicable_world_ids")
                        .and_then(|value| value.as_array())
                        .map(|ids| ids.iter().any(|id| id.as_str() == Some(world.id.as_str())))
                        .unwrap_or(false)
            })
            .map(|schema| (schema.id.clone(), schema))
            .collect::<HashMap<_, _>>();
        let session_attributes =
            attribute_repo.list_values(Some("session"), Some(session_id), None)?;
        let character_attributes = attribute_repo
            .list_values(Some("session_character"), None, None)?
            .into_iter()
            .filter(|value| value.owner_id.starts_with(&(session.id.clone() + ":")))
            .collect::<Vec<_>>();
        let runtime_session_attributes = session_attributes
            .iter()
            .filter_map(|value| build_runtime_attribute_item(value, &schema_map))
            .collect::<Vec<_>>();
        let mut grouped_values = HashMap::<String, Vec<AttributeValue>>::new();
        for value in character_attributes {
            grouped_values
                .entry(value.owner_id.clone())
                .or_default()
                .push(value);
        }
        let mut runtime_character_groups = Vec::new();
        for (owner_id, values) in grouped_values {
            let character_id = owner_id.split(':').next_back().unwrap_or_default();
            let owner_label = character_names
                .get(character_id)
                .cloned()
                .unwrap_or_else(|| character_id.to_string());
            let items = values
                .iter()
                .filter_map(|value| build_runtime_attribute_item(value, &schema_map))
                .collect::<Vec<_>>();
            runtime_character_groups.push(RuntimeAttributeGroup {
                owner_type: "session_character".to_string(),
                owner_id,
                owner_label,
                items,
            });
        }
        Ok(SessionRuntimeAttributesResponse {
            session_attributes: vec![RuntimeAttributeGroup {
                owner_type: "session".to_string(),
                owner_id: session.id,
                owner_label: session.world_name,
                items: runtime_session_attributes,
            }],
            character_attributes: runtime_character_groups,
        })
    }

    pub fn prepare_turn_context(
        &self,
        conn: &Connection,
        session_id: &str,
        request: &PlayerActionRequest,
    ) -> Result<PreparedTurnContext, String> {
        let replay_turn_index = if request.action_mode.requires_replay() {
            let turn_index = request
                .resend_from_turn_index
                .ok_or_else(|| "Replay action requires resend_from_turn_index".to_string())?;
            if turn_index <= 0 {
                return Err("Replay action requires a positive resend_from_turn_index".to_string());
            }
            Some(turn_index)
        } else {
            if request.resend_from_turn_index.is_some() {
                return Err("Submit action does not accept resend_from_turn_index".to_string());
            }
            None
        };
        let loaded_session = {
            let session_repo = crate::db::repositories::session_repo::SessionRepository::new(conn);
            session_repo
                .get(session_id)?
                .ok_or_else(|| "Session not found".to_string())?
        };
        let recovery_journal = if let Some(turn_index) = replay_turn_index {
            load_turn_journal(conn, session_id, turn_index)?
        } else {
            Vec::new()
        };
        let resume_incomplete_turn = replay_turn_index.is_some()
            && !recovery_journal.is_empty()
            && !journal_has_completed_step(&recovery_journal, "finished");
        let session = if let Some(turn_index) = replay_turn_index {
            if resume_incomplete_turn {
                loaded_session.clone()
            } else {
                rollback_session_to_turn(conn, &loaded_session, turn_index)?
            }
        } else {
            loaded_session
        };
        let world = resolve_world_for_session(conn, &session)?;
        let characters = {
            let char_repo = crate::db::repositories::character_repo::CharacterRepository::new(conn);
            char_repo.list_by_world(&world.id)?
        };
        let settings = resolve_settings(conn)?;
        let image_model = resolve_default_image_model(conn, &settings)?;
        let director_model = resolve_text_model(
            conn,
            world
                .director_config
                .get("director_model")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty()),
        )?;
        // 续跑未完成回合(resume)时必须复用原回合号:若另取 next_turn_index,
        // created/snapshot_created 会因旧 journal 去重检查而跳过、不再写入新回合,
        // 而 structured_output_failed 却会记到新回合,导致之后每次重发都在
        // load_resume_player_request 处报 "Missing created payload"(无法二次重发的根因)。
        let turn_index = match (resume_incomplete_turn, replay_turn_index) {
            (true, Some(replay_index)) => replay_index,
            _ => next_turn_index(conn, session_id)?,
        };
        if !journal_has_completed_step(&recovery_journal, "created") {
            append_turn_journal(
                conn,
                session_id,
                turn_index,
                "created",
                "completed",
                serde_json::json!({
                    "player_input": request.content.clone(),
                    "action_mode": request.action_mode.as_str(),
                    "player_character_name": session.player_character_name.clone(),
                }),
            )?;
        }
        if !journal_has_completed_step(&recovery_journal, "snapshot_created") {
            append_turn_journal(
                conn,
                session_id,
                turn_index,
                "snapshot_created",
                "completed",
                serde_json::json!({
                    "session_snapshot": session.clone(),
                    "attribute_values": collect_runtime_attribute_values(conn, &session.id)?,
                    "memory_entities": query_rows_json(
                        conn,
                        "SELECT * FROM memory_entities WHERE session_id = ?1",
                        &[&session.id],
                    )?,
                    "memory_relations": query_rows_json(
                        conn,
                        "SELECT * FROM memory_relations WHERE session_id = ?1",
                        &[&session.id],
                    )?,
                }),
            )?;
        }
        ensure_agent_session(
            conn,
            &session.id,
            "director",
            "director",
            None,
            None,
            "present",
            turn_index,
        )?;
        for visible_name in session
            .visible_characters
            .iter()
            .chain(std::iter::once(&session.player_character_name))
        {
            if let Some(character) = characters.iter().find(|item| item.name == *visible_name) {
                ensure_agent_session(
                    conn,
                    &session.id,
                    "character",
                    &format!("character:{}", character.id),
                    Some(character.id.as_str()),
                    Some(character.name.as_str()),
                    "present",
                    turn_index,
                )?;
            }
        }
        let effective_recovery_journal = if resume_incomplete_turn {
            recovery_journal
        } else {
            Vec::new()
        };
        let mut messages = session.messages.clone();
        messages.push(ChatMessage {
            message_id: ChatMessage::generate_id(),
            created_at: chrono::Utc::now().to_rfc3339(),
            parent_message_id: None,
            role: "player".to_string(),
            content: request.content.clone(),
            speaker: Some(session.player_character_name.clone()),
            metadata: Some(serde_json::json!({
                "turn_index": turn_index,
                "message_kind": "player_action"
            })),
        });
        let director_completed_payload = if resume_incomplete_turn
            && journal_has_completed_step(&effective_recovery_journal, "director_completed")
        {
            journal_payload(&effective_recovery_journal, "director_completed")
                .map(|value| recovered_director_payload_to_result(&value))
        } else {
            None
        };
        Ok(PreparedTurnContext {
            session,
            world,
            characters,
            turn_index,
            recovery_journal: effective_recovery_journal,
            resume_incomplete_turn,
            image_model,
            director_model,
            messages,
            director_completed_payload,
        })
    }

    pub fn prepare_director_runtime(
        &self,
        conn: &Connection,
        director_service: &WorldDirectorService,
        recovery_journal: &[serde_json::Value],
        session_id: &str,
        turn_index: i32,
        session: &SessionSnapshot,
        world: &WorldDefinition,
        characters: &mut Vec<CharacterDefinition>,
        parsed: &serde_json::Value,
        director_runtime_payload: &ParsedDirectorRuntimePayload,
        director_trace_message: Option<&DirectorTraceMessage>,
    ) -> Result<DirectorRuntimePreparation, String> {
        let mut next_location = director_runtime_payload.next_location.clone();
        let mut next_scene_name = director_runtime_payload.next_scene_name.clone();
        let current_line = director_runtime_payload.current_line.clone();
        let mut next_scene_background_hint = director_runtime_payload
            .next_scene_background_hint
            .clone()
            .unwrap_or_else(|| session.scene.background_hint.clone());
        let next_time_label = director_runtime_payload.next_time_label.clone();
        let scene_visible_characters = director_runtime_payload.scene_visible_characters.clone();
        let scene_visible_characters_explicit = scene_visible_characters.is_some();
        let planned_speakers = director_runtime_payload.planned_speakers.clone();
        let mut visible_chars = session.visible_characters.clone();
        if scene_visible_characters.is_none() {
            for generated in &director_runtime_payload.generated_character_payloads {
                if let Some(name) = generated.get("name").and_then(|value| value.as_str()) {
                    let name = name.trim();
                    if !name.is_empty()
                        && name != session.player_character_name
                        && !visible_chars.iter().any(|item| item == name)
                    {
                        visible_chars.push(name.to_string());
                    }
                }
            }
        } else if let Some(explicit_visible) = scene_visible_characters.as_ref() {
            visible_chars = Vec::new();
            for char_name in explicit_visible {
                if !visible_chars.contains(char_name) && char_name != &session.player_character_name
                {
                    visible_chars.push(char_name.clone());
                }
            }
        }

        let mut parsed_runtime = parsed.clone();
        if let Some(object) = parsed_runtime.as_object_mut() {
            object.insert(
                "world_phase".to_string(),
                serde_json::Value::String(director_runtime_payload.world_phase.clone()),
            );
            object.insert(
                "next_location".to_string(),
                serde_json::Value::String(next_location.clone()),
            );
            object.insert(
                "next_scene_name".to_string(),
                serde_json::Value::String(next_scene_name.clone()),
            );
            if let Some(current_line) = current_line.clone() {
                object.insert(
                    "current_line".to_string(),
                    serde_json::Value::String(current_line),
                );
            }
            object.insert(
                "next_time_label".to_string(),
                serde_json::Value::String(next_time_label.clone()),
            );
            object.insert(
                "scene_visible_characters".to_string(),
                serde_json::Value::Array(
                    scene_visible_characters
                        .clone()
                        .unwrap_or_default()
                        .into_iter()
                        .map(serde_json::Value::String)
                        .collect(),
                ),
            );
            object.insert(
                "planned_speakers".to_string(),
                serde_json::Value::Array(
                    planned_speakers
                        .iter()
                        .cloned()
                        .map(serde_json::Value::String)
                        .collect(),
                ),
            );
            if let Some(background_hint) =
                director_runtime_payload.next_scene_background_hint.clone()
            {
                object.insert(
                    "next_scene_background_hint".to_string(),
                    serde_json::Value::String(background_hint),
                );
            }
            if !director_runtime_payload.next_scene_tags.is_empty() {
                object.insert(
                    "next_scene_tags".to_string(),
                    serde_json::Value::Array(
                        director_runtime_payload
                            .next_scene_tags
                            .iter()
                            .cloned()
                            .map(serde_json::Value::String)
                            .collect(),
                    ),
                );
            }
            if let Some(value) = director_runtime_payload.background_asset_name.clone() {
                object.insert(
                    "background_asset_name".to_string(),
                    serde_json::Value::String(value),
                );
            }
            if let Some(value) = director_runtime_payload.background_asset_path.clone() {
                object.insert(
                    "background_asset_path".to_string(),
                    serde_json::Value::String(value),
                );
            }
            if let Some(value) = director_runtime_payload
                .background_generation_prompt
                .clone()
            {
                object.insert(
                    "background_generation_prompt".to_string(),
                    serde_json::Value::String(value),
                );
            }
            if !director_runtime_payload
                .character_visual_directives
                .is_empty()
            {
                object.insert(
                    "character_visual_directives".to_string(),
                    serde_json::Value::Array(
                        director_runtime_payload.character_visual_directives.clone(),
                    ),
                );
            }
            if let Some(value) = director_runtime_payload.switch_character_proposal.clone() {
                object.insert("switch_character_proposal".to_string(), value);
            }
        }

        let mut pre_runtime_system_messages = Vec::<ChatMessage>::new();
        if let Some(interaction) = director_runtime_payload.interaction.clone() {
            pre_runtime_system_messages.push(ChatMessage {
                message_id: ChatMessage::generate_id(),
                created_at: chrono::Utc::now().to_rfc3339(),
                parent_message_id: None,
                role: "system".to_string(),
                content: MessageContent::Text(interaction.prompt.clone()),
                speaker: None,
                metadata: Some(serde_json::json!({
                    "turn_index": turn_index,
                    "action_type": "world_interaction",
                    "message_kind": "world_interaction",
                    "interaction_source": "world_director",
                    "interaction": interaction,
                })),
            });
        }
        if let Some(trace_message) = director_trace_message {
            pre_runtime_system_messages.push(build_director_trace_chat_message(
                trace_message,
                turn_index,
                director_runtime_payload,
                false,
            ));
        }
        let mut created_character_ids_this_turn = Vec::<String>::new();
        let mut proposal_scene_tags: Option<Vec<String>> = None;
        let switch_target_name = director_runtime_payload
            .switch_character_proposal
            .as_ref()
            .and_then(|value| value.get("target_character_name"))
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        for generated in director_runtime_payload
            .generated_character_payloads
            .clone()
        {
            if let Some(created) = director_service
                .create_generated_character_if_missing(conn, world, characters, &generated)?
            {
                let for_switch_character = switch_target_name
                    .as_deref()
                    .map(|name| name == created.name)
                    .unwrap_or(false);
                if created.name != session.player_character_name
                    && !visible_chars.contains(&created.name)
                {
                    visible_chars.push(created.name.clone());
                }
                if !created_character_ids_this_turn.contains(&created.id) {
                    created_character_ids_this_turn.push(created.id.clone());
                }
                pre_runtime_system_messages.push(ChatMessage {
                    message_id: ChatMessage::generate_id(),
                    created_at: chrono::Utc::now().to_rfc3339(),
                    parent_message_id: None,
                    role: "system".to_string(),
                    content: MessageContent::Text(format!("character created: {}", created.name)),
                    speaker: None,
                    metadata: Some(serde_json::json!({
                        "turn_index": turn_index,
                        "action_type": "character_created",
                        "character_id": created.id,
                        "character_name": created.name,
                        "character_role": created.role,
                        "character_background_prompt": created.background_prompt,
                        "for_switch_character": for_switch_character,
                    })),
                });
            }
        }

        if let Some((creation_messages, proposal_message)) = director_service
            .materialize_switch_proposal_message(
                conn,
                world,
                session,
                characters,
                turn_index,
                director_runtime_payload.switch_character_proposal.as_ref(),
            )?
        {
            for message in &creation_messages {
                if message
                    .metadata
                    .as_ref()
                    .and_then(|meta| meta.get("action_type"))
                    .and_then(|value| value.as_str())
                    == Some("character_created")
                {
                    if let Some(character_id) = message
                        .metadata
                        .as_ref()
                        .and_then(|meta| meta.get("character_id"))
                        .and_then(|value| value.as_str())
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty())
                    {
                        if !created_character_ids_this_turn.contains(&character_id) {
                            created_character_ids_this_turn.push(character_id);
                        }
                    }
                }
            }
            pre_runtime_system_messages.extend(creation_messages);
            if let Some(metadata) = proposal_message.metadata.as_ref() {
                if let Some(location) = metadata.get("location").and_then(|value| value.as_str()) {
                    let location = location.trim();
                    if !location.is_empty() {
                        next_location = location.to_string();
                    }
                }
                if let Some(scene_name) =
                    metadata.get("scene_name").and_then(|value| value.as_str())
                {
                    let scene_name = scene_name.trim();
                    if !scene_name.is_empty() {
                        next_scene_name = scene_name.to_string();
                    }
                }
                if let Some(scene_background_hint) = metadata
                    .get("scene_background_hint")
                    .and_then(|value| value.as_str())
                {
                    let scene_background_hint = scene_background_hint.trim();
                    if !scene_background_hint.is_empty() {
                        next_scene_background_hint = scene_background_hint.to_string();
                    }
                }
                proposal_scene_tags = metadata
                    .get("scene_tags")
                    .and_then(|value| value.as_array())
                    .map(|items| {
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
                    });
                let proposal_visible = metadata
                    .get("scene_character_roster")
                    .and_then(|value| value.as_array())
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| item.as_str())
                            .map(|item| item.trim().to_string())
                            .filter(|item| {
                                !item.is_empty() && *item != session.player_character_name
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                visible_chars = merge_visible_characters(
                    &visible_chars,
                    proposal_visible,
                    &session.player_character_name,
                );
            }
            pre_runtime_system_messages.push(proposal_message);
        }

        if !created_character_ids_this_turn.is_empty()
            && !journal_has_completed_step(recovery_journal, "characters_created")
        {
            append_turn_journal(
                conn,
                session_id,
                turn_index,
                "characters_created",
                "completed",
                serde_json::json!({
                    "character_ids": created_character_ids_this_turn,
                }),
            )?;
        }
        if let Some(object) = parsed_runtime.as_object_mut() {
            object.insert(
                "next_location".to_string(),
                serde_json::Value::String(next_location.clone()),
            );
            object.insert(
                "next_scene_name".to_string(),
                serde_json::Value::String(next_scene_name.clone()),
            );
            object.insert(
                "next_scene_background_hint".to_string(),
                serde_json::Value::String(next_scene_background_hint.clone()),
            );
            // 关键一致性约束：写回 parsed_runtime 的必须是解析后的最终名册
            // visible_chars（显式替换或"旧名册+新角色"合并的结果），而不是导演原始
            // 输出。runtime_effects 的 refresh_scene 用它派生 scene.present_characters，
            // writeback 又把同一份 visible_chars 持久化为 session.visible_characters，
            // 单一来源保证两者不会脱节。
            object.insert(
                "scene_visible_characters".to_string(),
                serde_json::Value::Array(
                    visible_chars
                        .iter()
                        .filter(|name| *name != &session.player_character_name)
                        .cloned()
                        .map(serde_json::Value::String)
                        .collect(),
                ),
            );
            if let Some(scene_tags) = proposal_scene_tags {
                object.insert(
                    "next_scene_tags".to_string(),
                    serde_json::Value::Array(
                        scene_tags
                            .into_iter()
                            .map(serde_json::Value::String)
                            .collect(),
                    ),
                );
            }
        }
        Ok(DirectorRuntimePreparation {
            parsed_runtime,
            next_location,
            next_scene_name,
            current_line,
            next_scene_background_hint,
            next_time_label,
            scene_visible_characters,
            scene_visible_characters_explicit,
            planned_speakers,
            visible_chars,
            pre_runtime_system_messages,
        })
    }
}
