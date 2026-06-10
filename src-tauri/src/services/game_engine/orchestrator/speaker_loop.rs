use crate::models::character::CharacterDefinition;
use crate::models::mcp_tool::MCP_TOOL_SCHEDULE_NOTIFICATION_ID;
use crate::models::session::*;
use crate::models::world::WorldDefinition;
use crate::services::game_engine::dialogue::DialoguePipeline;
use crate::services::game_engine::memory::MemoryService;
use crate::services::game_engine::structured_output::{
    validate_character_payload, StructuredOutputFailure,
};
use crate::services::llm::client::{ChatToolCall, LlmClient};
use crate::services::notifications::{
    NotificationScheduler, NotificationToolContext, NotificationToolRuntime,
};

use super::run::*;
use super::turn_context::*;
use super::writeback::*;

impl SessionOrchestrator {
    pub async fn run_speaker_turns(
        &self,
        db: tokio::sync::MutexGuard<'_, crate::db::Database>,
        llm_client: &LlmClient,
        dialogue_pipeline: &DialoguePipeline,
        memory_service: &MemoryService,
        session_id: &str,
        turn_index: i32,
        recovery_journal: &[serde_json::Value],
        session: &SessionSnapshot,
        world: &WorldDefinition,
        characters: &[CharacterDefinition],
        mut messages: Vec<ChatMessage>,
        planned_speakers: &[String],
        player_input: &str,
        next_scene_name: &str,
        next_location: &str,
        visible_chars: &[String],
        notification_runtime: Option<NotificationToolRuntime<'_>>,
        mut progress_callback: Option<&mut (dyn FnMut(SpeakerTurnProgress) + Send)>,
    ) -> Result<SpeakerTurnRunResult, String> {
        let completed_speaker_steps = completed_speaker_steps_from_journal(recovery_journal);
        let mut pending_notifications = Vec::new();
        let mut runtime_payloads = Vec::<serde_json::Value>::new();
        let mut speaker_step_index = 0;
        for speaker_name in planned_speakers {
            if speaker_name == &session.player_character_name {
                continue;
            }
            speaker_step_index += 1;
            let journal_step_index = speaker_step_index;
            if completed_speaker_steps.contains(&journal_step_index) {
                if let Some(payload) = journal_payload(
                    recovery_journal,
                    &format!("speaker_{}_completed", journal_step_index),
                ) {
                    let recovered_content = payload
                        .get("llm_output")
                        .and_then(|value| value.get("content"))
                        .and_then(|value| value.as_str())
                        .map(|value| value.to_string())
                        .unwrap_or_default();
                    let recovered_speaker = payload
                        .get("llm_output")
                        .and_then(|value| value.get("speaker"))
                        .and_then(|value| value.as_str())
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty())
                        .unwrap_or_else(|| speaker_name.clone());
                    messages.push(ChatMessage {
                        role: "agent".to_string(),
                        content: MessageContent::Text(recovered_content),
                        speaker: Some(recovered_speaker),
                        metadata: Some(serde_json::json!({
                            "turn_index": turn_index,
                            "recovered": true,
                            "message_kind": "agent_response"
                        })),
                    });
                    pending_notifications.extend(parse_recovered_pending_notifications(&payload));
                    continue;
                }
            }

            let speaker_char = characters.iter().find(|c| c.name == *speaker_name);
            let (
                speaker_model,
                visible_attribute_lines,
                visible_inventory_items,
                visible_inventory_lines,
                public_scene_state_lines,
                memory_pool,
                recalled_memories,
                recent_messages,
                speaker_request,
                speaker_provider,
                speaker_request_value,
            ) = {
                let conn = db.conn();
                let speaker_model = resolve_text_model(
                    conn,
                    speaker_char
                        .map(|character| character.model.as_str())
                        .filter(|value| !value.trim().is_empty()),
                )?;
                let speaker_character_id = speaker_char.map(|character| character.id.as_str());
                let visible_attribute_lines =
                    load_character_visible_attribute_lines(conn, session, speaker_character_id)?;
                let visible_inventory_items =
                    filter_inventory_for_character(session, speaker_character_id, speaker_name);
                let visible_inventory_lines = summarize_visible_inventory_lines(
                    &visible_inventory_items,
                    speaker_character_id,
                );
                let public_scene_state_lines = build_public_scene_state_lines(
                    session,
                    &visible_attribute_lines,
                    &visible_inventory_items,
                );
                let memory_pool =
                    load_character_memory_pool(conn, &world.id, &session.id, speaker_character_id)?;
                let recall_limit = std::cmp::max(
                    resolve_character_memory_recall_limit(speaker_char),
                    (resolve_character_memory_hit_turns(world) as i32 * 6).max(12),
                );
                let recalled_memories = load_recent_character_memories(
                    memory_service,
                    conn,
                    world,
                    &world.id,
                    &session.id,
                    speaker_character_id,
                    player_input,
                    &session.location,
                    Some(session.scene.scene_id.clone()),
                    &build_turn_participants(visible_chars, &session.player_character_name),
                    recall_limit,
                )?;
                let recent_messages = slice_character_history(
                    &messages,
                    speaker_char
                        .map(|character| character.recent_dialogue_rounds)
                        .unwrap_or(2),
                    Some(session.player_character_name.as_str()),
                );
                let mut speaker_request = build_character_chat_request(
                    dialogue_pipeline,
                    world,
                    &speaker_model,
                    speaker_name,
                    speaker_char,
                    session,
                    &session.player_character_name,
                    &session.location,
                    next_scene_name,
                    player_input,
                    &recent_messages,
                    &recalled_memories,
                    &memory_pool,
                    &visible_attribute_lines,
                    &visible_inventory_items,
                    &visible_inventory_lines,
                    &public_scene_state_lines,
                );
                speaker_request.stream = Some(
                    speaker_model.streaming_enabled
                        && !speaker_request.native_tool_calling.unwrap_or(false),
                );
                let speaker_provider = normalize_provider_name(&speaker_model.provider);
                let speaker_request_value = serde_json::json!({
                    "provider": speaker_provider,
                    "base_url": speaker_model.base_url,
                    "model_id": speaker_model.model_id,
                    "request": serde_json::to_value(&speaker_request).unwrap_or_default(),
                });
                (
                    speaker_model,
                    visible_attribute_lines,
                    visible_inventory_items,
                    visible_inventory_lines,
                    public_scene_state_lines,
                    memory_pool,
                    recalled_memories,
                    recent_messages,
                    speaker_request,
                    speaker_provider,
                    speaker_request_value,
                )
            };
            if let Some(callback) = progress_callback.as_deref_mut() {
                let mut progress_messages = messages.clone();
                progress_messages.push(ChatMessage {
                    role: "agent".to_string(),
                    content: MessageContent::Text(String::new()),
                    speaker: Some(speaker_name.clone()),
                    metadata: Some(serde_json::json!({
                        "turn_index": turn_index,
                        "message_kind": "agent_response",
                    })),
                });
                callback(SpeakerTurnProgress {
                    messages: progress_messages,
                    speaker_name: speaker_name.clone(),
                    narration: None,
                    is_placeholder: true,
                    is_error: false,
                });
            }
            let speaker_started_at = std::time::Instant::now();
            let mut streamed_raw_response = String::new();
            let mut streamed_reasoning = String::new();
            let mut streamed_partial: Option<
                crate::services::game_engine::dialogue::ParsedCharacterResponse,
            > = None;
            let llm_result = if speaker_request.stream.unwrap_or(speaker_model.streaming_enabled) {
                let streamed_result = llm_client
                    .chat_completion_stream(
                        &speaker_provider,
                        &speaker_model.base_url,
                        &speaker_model.api_key,
                        &speaker_request,
                        |chunk| {
                            let has_reasoning_delta = chunk.reasoning_delta.is_some();
                            if let Some(reasoning_delta) = chunk.reasoning_delta.as_deref() {
                                streamed_reasoning.push_str(reasoning_delta);
                            }
                            if !chunk.delta.is_empty() {
                                streamed_raw_response.push_str(&chunk.delta);
                                if let Some(parsed_partial) = dialogue_pipeline
                                    .extract_partial_character_response(
                                        &streamed_raw_response,
                                        speaker_name,
                                    )
                                {
                                    streamed_partial = Some(parsed_partial);
                                }
                            }
                            if let Some(callback) = progress_callback.as_deref_mut() {
                                if has_reasoning_delta || streamed_partial.is_some() {
                                    let partial = streamed_partial.clone();
                                    let mut progress_messages = messages.clone();
                                    progress_messages.push(ChatMessage {
                                        role: "agent".to_string(),
                                        content: MessageContent::Text(
                                            partial
                                                .as_ref()
                                                .map(|value| value.content.clone())
                                                .unwrap_or_default()
                                        ),
                                        speaker: Some(
                                            partial
                                                .as_ref()
                                                .map(|value| value.speaker.clone())
                                                .unwrap_or_else(|| speaker_name.clone()),
                                        ),
                                        metadata: Some(serde_json::json!({
                                            "turn_index": turn_index,
                                            "intent": partial.as_ref().map(|value| value.intent.clone()).unwrap_or_default(),
                                            "emotion": partial.as_ref().map(|value| value.emotion.clone()).unwrap_or_default(),
                                            "message_kind": "agent_response",
                                            "reasoning": streamed_reasoning,
                                            "reasoning_expanded": true,
                                        })),
                                    });
                                    callback(SpeakerTurnProgress {
                                        messages: progress_messages,
                                        speaker_name: partial
                                            .as_ref()
                                            .map(|value| value.speaker.clone())
                                            .unwrap_or_else(|| speaker_name.clone()),
                                        narration: partial
                                            .as_ref()
                                            .map(|value| value.narration.clone())
                                            .filter(|value| !value.trim().is_empty()),
                                        is_placeholder: false,
                                        is_error: false,
                                    });
                                }
                            }
                        },
                    )
                    .await;
                match streamed_result {
                    Ok(response) => Ok(response),
                    Err(_) => {
                        llm_client
                            .chat_completion(
                                &speaker_provider,
                                &speaker_model.base_url,
                                &speaker_model.api_key,
                                &speaker_request,
                            )
                            .await
                    }
                }
            } else {
                llm_client
                    .chat_completion(
                        &speaker_provider,
                        &speaker_model.base_url,
                        &speaker_model.api_key,
                        &speaker_request,
                    )
                    .await
            };
            match llm_result {
                Ok(response) => {
                    let notification_tool_results = if world_allows_mcp_tool(
                        world,
                        MCP_TOOL_SCHEDULE_NOTIFICATION_ID,
                    ) {
                        execute_speaker_notification_tool_calls(
                            db.conn(),
                            notification_runtime.as_ref(),
                            session_id,
                            world,
                            turn_index,
                            response.tool_calls.as_deref(),
                        )
                    } else {
                        Vec::new()
                    };
                    let response = if notification_tool_results.is_empty() {
                        response
                    } else {
                        let followup_request = build_speaker_tool_followup_request(
                            &speaker_request,
                            &response,
                            &notification_tool_results,
                        );
                        match llm_client
                            .chat_completion(
                                &speaker_provider,
                                &speaker_model.base_url,
                                &speaker_model.api_key,
                                &followup_request,
                            )
                            .await
                        {
                            Ok(followup_response) => followup_response,
                            Err(_) if response.content.trim().is_empty() => {
                                crate::services::llm::client::ChatResponse {
                                    content: fallback_notification_tool_response(speaker_name),
                                    reasoning: response.reasoning.clone(),
                                    tool_calls: None,
                                    usage: response.usage.clone(),
                                }
                            }
                            Err(_) => response,
                        }
                    };
                    let conn = db.conn();
                    let speaker_latency_ms = speaker_started_at.elapsed().as_millis() as i64;
                    let reasoning_text = response.reasoning.clone().unwrap_or_default();
                    let speaker_response_value = serde_json::json!({
                        "provider": speaker_provider,
                        "model_id": speaker_model.model_id,
                        "status": "completed",
                        "latency_ms": speaker_latency_ms,
                        "response": serde_json::to_value(&response).unwrap_or_default(),
                        "notification_tool_results": notification_tool_results.iter().map(|item| item.result.clone()).collect::<Vec<_>>(),
                    });
                    let parsed_response = if let Some(partial) = streamed_partial
                        .as_ref()
                        .filter(|_| !response.content.trim().is_empty())
                    {
                        let final_parsed = dialogue_pipeline
                            .parse_character_response(&response.content, speaker_name);
                        if final_parsed.content.trim().is_empty() {
                            partial.clone()
                        } else {
                            final_parsed
                        }
                    } else if !response.content.trim().is_empty() {
                        dialogue_pipeline.parse_character_response(&response.content, speaker_name)
                    } else if let Some(partial) = streamed_partial.clone() {
                        partial
                    } else {
                        dialogue_pipeline
                            .parse_character_response(&streamed_raw_response, speaker_name)
                    };
                    let prompt_trace = build_character_prompt_trace(
                        dialogue_pipeline,
                        world,
                        speaker_name,
                        speaker_char,
                        session,
                        &session.player_character_name,
                        &session.location,
                        next_scene_name,
                        player_input,
                        &recent_messages,
                        &recalled_memories,
                        &memory_pool,
                        &visible_attribute_lines,
                        &visible_inventory_items,
                        &visible_inventory_lines,
                        &public_scene_state_lines,
                        next_scene_name,
                        next_location,
                        visible_chars,
                        &speaker_provider,
                        &speaker_model,
                        speaker_request_value.clone(),
                        speaker_response_value.clone(),
                        if response.content.trim().is_empty() {
                            streamed_raw_response.clone()
                        } else {
                            response.content.clone()
                        },
                        serde_json::json!({
                            "speaker": parsed_response.speaker.clone(),
                            "content": parsed_response.content.clone(),
                            "intent": parsed_response.intent.clone(),
                            "emotion": parsed_response.emotion.clone(),
                            "narration": parsed_response.narration.clone(),
                        }),
                        serde_json::json!({
                            "speaker": parsed_response.speaker.clone(),
                            "content": parsed_response.content.clone(),
                            "intent": parsed_response.intent.clone(),
                            "emotion": parsed_response.emotion.clone(),
                            "narration": parsed_response.narration.clone(),
                        }),
                    );
                    let raw_response = if response.content.trim().is_empty() {
                        streamed_raw_response.clone()
                    } else {
                        response.content.clone()
                    };
                    if let Err(failure) = validate_character_payload(
                        &parsed_response,
                        speaker_name,
                        &speaker_provider,
                        &speaker_model.model_id,
                        turn_index,
                        &raw_response,
                    ) {
                        let display_message =
                            self.build_structured_failure_chat_message(&failure, turn_index, None);
                        if let Some(callback) = progress_callback.as_deref_mut() {
                            let mut failure_messages = messages.clone();
                            failure_messages.push(display_message.clone());
                            callback(SpeakerTurnProgress {
                                messages: failure_messages,
                                speaker_name: speaker_name.clone(),
                                narration: None,
                                is_placeholder: false,
                                is_error: true,
                            });
                        }
                        let _ = append_turn_journal(
                            conn,
                            session_id,
                            turn_index,
                            &format!("speaker_{}_completed", journal_step_index),
                            "failed",
                            serde_json::json!({
                                "llm_output": {
                                    "speaker": speaker_name,
                                    "status": "invalid",
                                },
                                "failure": serde_json::to_value(&failure).unwrap_or_default(),
                            }),
                        );
                        return Ok(SpeakerTurnRunResult {
                            messages,
                            failure: Some(failure),
                            pending_notifications,
                            runtime_payloads,
                        });
                    }

                    let speaker_pending_notifications = Vec::new();
                    if let Some(raw_payload) = parsed_response.raw_payload.clone() {
                        runtime_payloads.push(raw_payload);
                    }
                    messages.push(ChatMessage {
                        role: "agent".to_string(),
                        content: MessageContent::Text(parsed_response.content.clone()),
                        speaker: Some(parsed_response.speaker.clone()),
                        metadata: Some(serde_json::json!({
                            "turn_index": turn_index,
                            "intent": parsed_response.intent.clone(),
                            "emotion": parsed_response.emotion.clone(),
                            "narration": parsed_response.narration.clone(),
                            "message_kind": "agent_response",
                            "reasoning": reasoning_text,
                            "reasoning_expanded": false,
                            "raw_response": raw_response.clone()
                        })),
                    });
                    if let Some(callback) = progress_callback.as_deref_mut() {
                        callback(SpeakerTurnProgress {
                            messages: messages.clone(),
                            speaker_name: parsed_response.speaker.clone(),
                            narration: Some(parsed_response.narration.clone())
                                .filter(|value| !value.trim().is_empty()),
                            is_placeholder: false,
                            is_error: false,
                        });
                    }
                    let _ = record_prompt_call(
                        conn,
                        session_id,
                        turn_index,
                        "character",
                        "character_response",
                        speaker_name,
                        prompt_trace,
                    );
                    let _ = record_llm_call(
                        conn,
                        session_id,
                        turn_index,
                        "character_response",
                        speaker_name,
                        speaker_request_value,
                        speaker_response_value.clone(),
                    );
                    let _ = append_turn_journal(
                        conn,
                        session_id,
                        turn_index,
                        &format!("speaker_{}_completed", journal_step_index),
                        "completed",
                        serde_json::json!({
                            "llm_output": {
                                "speaker": parsed_response.speaker.clone(),
                                "content": parsed_response.content.clone(),
                                "intent": parsed_response.intent.clone(),
                                "emotion": parsed_response.emotion.clone(),
                                "narration": parsed_response.narration.clone(),
                                "raw_content": response.content.clone(),
                                "pending_notifications": speaker_pending_notifications,
                                "notification_tool_results": notification_tool_results.iter().map(|item| item.result.clone()).collect::<Vec<_>>(),
                            }
                        }),
                    );
                    pending_notifications.extend(speaker_pending_notifications);
                }
                Err(e) => {
                    let conn = db.conn();
                    let speaker_latency_ms = speaker_started_at.elapsed().as_millis() as i64;
                    let speaker_response_value = serde_json::json!({
                        "provider": speaker_provider,
                        "model_id": speaker_model.model_id,
                        "status": "failed",
                        "latency_ms": speaker_latency_ms,
                        "error": e.clone(),
                    });
                    let prompt_trace = build_character_prompt_trace(
                        dialogue_pipeline,
                        world,
                        speaker_name,
                        speaker_char,
                        session,
                        &session.player_character_name,
                        &session.location,
                        next_scene_name,
                        player_input,
                        &recent_messages,
                        &recalled_memories,
                        &memory_pool,
                        &visible_attribute_lines,
                        &visible_inventory_items,
                        &visible_inventory_lines,
                        &public_scene_state_lines,
                        next_scene_name,
                        next_location,
                        visible_chars,
                        &speaker_provider,
                        &speaker_model,
                        speaker_request_value.clone(),
                        speaker_response_value.clone(),
                        String::new(),
                        serde_json::json!({ "error": e.clone() }),
                        serde_json::json!({ "error": e.clone() }),
                    );
                    let _ = record_prompt_call(
                        conn,
                        session_id,
                        turn_index,
                        "character",
                        "character_response",
                        speaker_name,
                        prompt_trace,
                    );
                    let _ = record_llm_call(
                        conn,
                        session_id,
                        turn_index,
                        "character_response",
                        speaker_name,
                        speaker_request_value,
                        speaker_response_value.clone(),
                    );
                    let _ = append_turn_journal(
                        conn,
                        session_id,
                        turn_index,
                        &format!("speaker_{}_completed", journal_step_index),
                        "failed",
                        serde_json::json!({
                            "llm_output": {
                                "speaker": speaker_name,
                                "status": "error",
                            },
                            "error": e,
                        }),
                    );
                    let provider_error = speaker_response_value
                        .get("error")
                        .and_then(|value| value.as_str())
                        .unwrap_or("speaker request failed")
                        .to_string();
                    let failure = StructuredOutputFailure {
                        stage: crate::services::game_engine::structured_output::StructuredFailureStage::SpeakerResponse,
                        failure_code: "provider_payload_missing".to_string(),
                        summary: "角色请求失败，未获得可用结构化输出".to_string(),
                        provider: speaker_provider.clone(),
                        model_id: speaker_model.model_id.clone(),
                        turn_index,
                        speaker_name: Some(speaker_name.clone()),
                        raw_text_excerpt: String::new(),
                        repair_summary: Some("provider request failed".to_string()),
                        schema_errors: Vec::new(),
                        domain_errors: vec![provider_error],
                    };
                    if let Some(callback) = progress_callback.as_deref_mut() {
                        let mut failure_messages = messages.clone();
                        failure_messages.push(
                            self.build_structured_failure_chat_message(&failure, turn_index, None),
                        );
                        callback(SpeakerTurnProgress {
                            messages: failure_messages,
                            speaker_name: speaker_name.clone(),
                            narration: None,
                            is_placeholder: false,
                            is_error: true,
                        });
                    }
                    return Ok(SpeakerTurnRunResult {
                        messages,
                        failure: Some(failure),
                        pending_notifications,
                        runtime_payloads,
                    });
                }
            }
        }
        Ok(SpeakerTurnRunResult {
            messages,
            failure: None,
            pending_notifications,
            runtime_payloads,
        })
    }
}

fn parse_recovered_pending_notifications(
    payload: &serde_json::Value,
) -> Vec<crate::models::scheduled_notification::PendingScheduledNotification> {
    payload
        .get("llm_output")
        .and_then(|value| value.get("pending_notifications"))
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|value| serde_json::from_value(value.clone()).ok())
        .collect()
}

struct SpeakerNotificationToolResult {
    call: ChatToolCall,
    result: serde_json::Value,
}

fn execute_speaker_notification_tool_calls(
    conn: &rusqlite::Connection,
    runtime: Option<&NotificationToolRuntime<'_>>,
    session_id: &str,
    world: &WorldDefinition,
    turn_index: i32,
    tool_calls: Option<&[ChatToolCall]>,
) -> Vec<SpeakerNotificationToolResult> {
    tool_calls
        .unwrap_or(&[])
        .iter()
        .filter_map(|call| {
            if call.tool_name.trim() != "schedule_notification" {
                return None;
            }
            let result = match (runtime, call.arguments.as_object()) {
                (Some(runtime), Some(arguments)) => NotificationScheduler::execute_tool_call(
                    conn,
                    runtime.app,
                    runtime.data_dir,
                    NotificationToolContext {
                        session_id,
                        world_id: &world.id,
                        world_name: &world.name,
                        turn_index,
                    },
                    &call.id,
                    arguments,
                ),
                (None, _) => serde_json::json!({
                    "tool_name": "schedule_notification",
                    "tool_call_id": call.id,
                    "ok": false,
                    "error": "notification runtime is not available",
                }),
                (_, None) => serde_json::json!({
                    "tool_name": "schedule_notification",
                    "tool_call_id": call.id,
                    "ok": false,
                    "error": "tool arguments must be a JSON object",
                }),
            };
            Some(SpeakerNotificationToolResult {
                call: call.clone(),
                result,
            })
        })
        .collect()
}

fn build_speaker_tool_followup_request(
    request: &crate::services::llm::client::ChatRequest,
    response: &crate::services::llm::client::ChatResponse,
    tool_results: &[SpeakerNotificationToolResult],
) -> crate::services::llm::client::ChatRequest {
    let mut messages = request.messages.clone();
    messages.push(crate::services::llm::client::ChatMessage {
        role: "assistant".to_string(),
        content: serde_json::Value::String(response.content.clone()),
        reasoning_content: response.reasoning.clone(),
        speaker: None,
        tool_call_id: None,
        tool_calls: Some(tool_results.iter().map(|item| item.call.clone()).collect()),
        metadata: None,
    });
    for item in tool_results {
        messages.push(crate::services::llm::client::ChatMessage {
            role: "tool".to_string(),
            content: serde_json::Value::String(
                serde_json::to_string(&item.result).unwrap_or_else(|_| "{}".to_string()),
            ),
            reasoning_content: None,
            speaker: None,
            tool_call_id: Some(item.call.id.clone()),
            tool_calls: None,
            metadata: None,
        });
    }

    let mut followup = request.clone();
    followup.messages = messages;
    followup.stream = Some(false);
    followup.tools = None;
    followup.tool_choice = None;
    followup.native_tool_calling = None;
    followup
}

fn fallback_notification_tool_response(speaker_name: &str) -> String {
    serde_json::json!({
        "speaker": speaker_name,
        "content": "\u{5df2}\u{5904}\u{7406}\u{901a}\u{77e5}\u{8bf7}\u{6c42}\u{3002}",
        "intent": "notification_tool_result",
        "emotion": "neutral",
        "narration": ""
    })
    .to_string()
}
