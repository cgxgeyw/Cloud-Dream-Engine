use crate::models::character::CharacterDefinition;
use crate::models::generation_params::GENERATION_ROLE_CHARACTER;
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

use super::character_prompt::*;
use super::request_building::*;
use super::run::*;
use super::turn_context::*;
use super::writeback::*;
use crate::services::game_engine::prompting::load_prompt_kv_vars;

impl SessionOrchestrator {
    pub async fn run_speaker_turns(
        &self,
        db: &tokio::sync::Mutex<crate::db::Database>,
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
        player_media: &[ContentPart],
        next_scene_name: &str,
        next_location: &str,
        visible_chars: &[String],
        notification_runtime: Option<NotificationToolRuntime<'_>>,
        mut progress_callback: Option<&mut (dyn FnMut(SpeakerTurnProgress) + Send)>,
    ) -> Result<SpeakerTurnRunResult, String> {
        let completed_speaker_steps = completed_speaker_steps_from_journal(recovery_journal);
        // 会话级 variables KV（第 6 项 {{var:key}} 占位符的数据源）
        // 与应用级生成参数（第 8 项三级覆盖的第一层）一起在同一次加锁内读出。
        let (kv_vars, app_settings) = {
            let guard = db.lock().await;
            (
                load_prompt_kv_vars(guard.conn(), session_id)?,
                resolve_settings(guard.conn())?,
            )
        };
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
                        message_id: ChatMessage::generate_id(),
                        created_at: chrono::Utc::now().to_rfc3339(),
                        parent_message_id: None,
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
            // C1/H9: 召回分三步,嵌入 HTTP 必须在锁外执行。
            // 步骤 1(持锁):取候选记忆 + 词法打分 + 已存向量,不触网。
            let (
                speaker_model,
                visible_attribute_lines,
                visible_inventory_items,
                public_scene_state_lines,
                memory_pool,
                recent_messages,
                speaker_character_id_owned,
                mut recall_plan,
            ) = {
                let db_guard = db.lock().await;
                let conn = db_guard.conn();
                let speaker_model = resolve_text_model(
                    conn,
                    speaker_char
                        .map(|character| character.model.as_str())
                        .filter(|value| !value.trim().is_empty()),
                )?;
                // 第 10 项（校验点 B）：附件只发给声明了对应输入模态的模型，
                // 不支持即明确报错（中文文案），不静默丢弃。
                crate::models::model_config::ensure_media_supported(&speaker_model, player_media)?;
                let speaker_character_id = speaker_char.map(|character| character.id.as_str());
                let visible_attribute_lines =
                    load_character_visible_attribute_lines(conn, session, speaker_character_id)?;
                let visible_inventory_items =
                    filter_inventory_for_character(session, speaker_character_id, speaker_name);
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
                let recall_plan = memory_service.prepare_character_recall(
                    conn,
                    world,
                    &world.id,
                    &session.id,
                    speaker_character_id,
                    player_input,
                    &session.location,
                    Some(session.scene.scene_id.as_str()),
                    &build_turn_participants(visible_chars, &session.player_character_name),
                    recall_limit,
                    speaker_char.map(|character| character.memory_strategy.as_str()),
                )?;
                let recent_messages = slice_character_history(
                    &messages,
                    speaker_char
                        .map(|character| character.recent_dialogue_rounds)
                        .unwrap_or(2),
                    Some(session.player_character_name.as_str()),
                );
                (
                    speaker_model,
                    visible_attribute_lines,
                    visible_inventory_items,
                    public_scene_state_lines,
                    memory_pool,
                    recent_messages,
                    speaker_character_id.map(|value| value.to_string()),
                    recall_plan,
                )
            };
            let _ = speaker_character_id_owned;

            // 步骤 2(锁外):嵌入 HTTP / 本地推理。block_in_place 让阻塞调用不占用
            // 其它 tokio worker,且此处已不持有 DB 锁,慢嵌入端点不再卡住全库。
            tokio::task::block_in_place(|| memory_service.embed_recall_plan(&mut recall_plan));

            // 步骤 3(持锁):写回向量缓存并完成排序,随后构建请求。
            let (recalled_memories, speaker_request, speaker_provider, speaker_request_value) = {
                let db_guard = db.lock().await;
                let conn = db_guard.conn();
                let recalled_memories =
                    memory_service.finalize_character_recall(conn, recall_plan);
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
                    &public_scene_state_lines,
                    &kv_vars,
                    &resolve_generation_params_with_model(
                        GENERATION_ROLE_CHARACTER,
                        &app_settings,
                        world,
                        session,
                        &speaker_model,
                    ),
                    player_media,
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
                    // 第 10 项：multipart 消息中的媒体 base64 在 trace 里只留摘要。
                    "request": crate::services::game_engine::prompting::redact_request_value_for_trace(&speaker_request),
                });
                (
                    recalled_memories,
                    speaker_request,
                    speaker_provider,
                    speaker_request_value,
                )
            };
            if let Some(callback) = progress_callback.as_deref_mut() {
                let mut progress_messages = messages.clone();
                progress_messages.push(ChatMessage {
                    message_id: ChatMessage::generate_id(),
                    created_at: chrono::Utc::now().to_rfc3339(),
                    parent_message_id: None,
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
                                        message_id: ChatMessage::generate_id(),
                                        created_at: chrono::Utc::now().to_rfc3339(),
                                        parent_message_id: None,
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
                                            "message_kind": "agent_response",
                                            "reasoning": streamed_reasoning,
                                            "reasoning_expanded": false,
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
                        let db_guard = db.lock().await;
                        execute_speaker_notification_tool_calls(
                            db_guard.conn(),
                            notification_runtime.as_ref(),
                            session_id,
                            world,
                            turn_index,
                            speaker_name,
                            speaker_char
                                .map(|character| character.avatar_asset.as_str())
                                .unwrap_or(""),
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
                    let db_guard = db.lock().await;
                    let conn = db_guard.conn();
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
                            "narration": parsed_response.narration.clone(),
                        }),
                        serde_json::json!({
                            "speaker": parsed_response.speaker.clone(),
                            "content": parsed_response.content.clone(),
                            "narration": parsed_response.narration.clone(),
                        }),
                        &kv_vars,
                        &speaker_request.generation,
                        player_media,
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
                    let message_metadata = serde_json::json!({
                        "turn_index": turn_index,
                        "narration": parsed_response.narration.clone(),
                        "message_kind": "agent_response",
                        "reasoning": reasoning_text,
                        "reasoning_expanded": false,
                        "raw_response": raw_response.clone()
                    });
                    messages.push(ChatMessage {
                        message_id: ChatMessage::generate_id(),
                        created_at: chrono::Utc::now().to_rfc3339(),
                        parent_message_id: None,
                        role: "agent".to_string(),
                        content: MessageContent::Text(parsed_response.content.clone()),
                        speaker: Some(parsed_response.speaker.clone()),
                        metadata: Some(message_metadata),
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
                    let db_guard = db.lock().await;
                    let conn = db_guard.conn();
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
                        &kv_vars,
                        &speaker_request.generation,
                        player_media,
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

/// 通知工具调用失败时的统一结果 payload；
/// runtime 缺失与参数不是 JSON object 两条路径共用，保证结构一致。
fn speaker_notification_tool_error(call: &ChatToolCall, error: &str) -> serde_json::Value {
    serde_json::json!({
        "tool_name": "schedule_notification",
        "tool_call_id": call.id,
        "ok": false,
        "error": error,
    })
}

fn execute_speaker_notification_tool_calls(
    conn: &rusqlite::Connection,
    runtime: Option<&NotificationToolRuntime<'_>>,
    session_id: &str,
    world: &WorldDefinition,
    turn_index: i32,
    speaker_name: &str,
    speaker_avatar_asset: &str,
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
                        speaker_name: Some(speaker_name),
                        speaker_avatar_asset: Some(speaker_avatar_asset),
                    },
                    &call.id,
                    arguments,
                ),
                (None, _) => {
                    speaker_notification_tool_error(call, "notification runtime is not available")
                }
                (_, None) => {
                    speaker_notification_tool_error(call, "tool arguments must be a JSON object")
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::llm::client::{ChatRequest, ChatResponse};
    use rusqlite::Connection;

    fn test_world() -> WorldDefinition {
        WorldDefinition {
            id: "world-1".to_string(),
            name: "通知测试世界".to_string(),
            genre: String::new(),
            background_prompt: String::new(),
            opening_scene: "开场".to_string(),
            summary: String::new(),
            time_system: String::new(),
            map_nodes: serde_json::json!({ "version": 1, "nodes": [] }),
            triggers: Vec::new(),
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

    fn notification_call(id: &str) -> ChatToolCall {
        ChatToolCall {
            id: id.to_string(),
            tool_name: "schedule_notification".to_string(),
            arguments: serde_json::json!({
                "action": "create",
                "title": "提醒",
                "body": "该出发了",
                "delay_minutes": 30
            }),
        }
    }

    fn valid_pending_notification_json(tool_call_id: &str) -> serde_json::Value {
        serde_json::json!({
            "tool_call_id": tool_call_id,
            "source": "character",
            "title": "提醒",
            "body": "该出发了",
            "requested_time": "30分钟后",
            "scheduled_at": "2026-08-02T20:30:00Z",
            "arguments": { "action": "create" }
        })
    }

    // ---- parse_recovered_pending_notifications ----

    #[test]
    fn parse_recovered_pending_notifications_reads_valid_entries() {
        let payload = serde_json::json!({
            "llm_output": {
                "pending_notifications": [
                    valid_pending_notification_json("call-1"),
                    valid_pending_notification_json("call-2"),
                ]
            }
        });

        let parsed = parse_recovered_pending_notifications(&payload);

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].tool_call_id, "call-1");
        assert_eq!(parsed[0].source, "character");
        assert_eq!(parsed[0].title, "提醒");
        assert_eq!(parsed[0].scheduled_at, "2026-08-02T20:30:00Z");
        assert_eq!(parsed[1].tool_call_id, "call-2");
    }

    #[test]
    fn parse_recovered_pending_notifications_returns_empty_when_section_missing() {
        // 完全没有 llm_output
        assert!(parse_recovered_pending_notifications(&serde_json::json!({})).is_empty());
        // 有 llm_output 但没有 pending_notifications
        assert!(parse_recovered_pending_notifications(&serde_json::json!({
            "llm_output": { "speaker": "Alice" }
        }))
        .is_empty());
        // pending_notifications 为 null
        assert!(parse_recovered_pending_notifications(&serde_json::json!({
            "llm_output": { "pending_notifications": null }
        }))
        .is_empty());
    }

    #[test]
    fn parse_recovered_pending_notifications_returns_empty_for_non_array_section() {
        let object_payload = serde_json::json!({
            "llm_output": { "pending_notifications": { "tool_call_id": "call-1" } }
        });
        assert!(parse_recovered_pending_notifications(&object_payload).is_empty());

        let string_payload = serde_json::json!({
            "llm_output": { "pending_notifications": "not-an-array" }
        });
        assert!(parse_recovered_pending_notifications(&string_payload).is_empty());
    }

    #[test]
    fn parse_recovered_pending_notifications_skips_malformed_entries() {
        let payload = serde_json::json!({
            "llm_output": {
                "pending_notifications": [
                    valid_pending_notification_json("call-ok"),
                    { "tool_call_id": "call-missing-fields" },
                    "garbage",
                    42,
                ]
            }
        });

        let parsed = parse_recovered_pending_notifications(&payload);

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].tool_call_id, "call-ok");
    }

    // ---- execute_speaker_notification_tool_calls（runtime 需要 AppHandle，仅测纯路径）----

    #[test]
    fn execute_speaker_notification_tool_calls_ignores_unrelated_tools_and_empty_input() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        let world = test_world();

        // None 输入
        assert!(execute_speaker_notification_tool_calls(
            &conn, None, "sess-1", &world, 1, "Alice", "", None
        )
        .is_empty());
        // 空切片
        assert!(execute_speaker_notification_tool_calls(
            &conn,
            None,
            "sess-1",
            &world,
            1,
            "Alice",
            "",
            Some(&[])
        )
        .is_empty());
        // 非 schedule_notification 工具被过滤
        let unrelated = ChatToolCall {
            id: "call-x".to_string(),
            tool_name: "roll_dice".to_string(),
            arguments: serde_json::json!({}),
        };
        assert!(execute_speaker_notification_tool_calls(
            &conn,
            None,
            "sess-1",
            &world,
            1,
            "Alice",
            "",
            Some(&[unrelated])
        )
        .is_empty());
    }

    #[test]
    fn execute_speaker_notification_tool_calls_trims_tool_name_before_matching() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        let world = test_world();
        let padded = ChatToolCall {
            id: "call-padded".to_string(),
            tool_name: "  schedule_notification  ".to_string(),
            arguments: serde_json::json!({}),
        };

        let results = execute_speaker_notification_tool_calls(
            &conn,
            None,
            "sess-1",
            &world,
            1,
            "Alice",
            "",
            Some(std::slice::from_ref(&padded)),
        );

        // trim 后仍识别为通知工具，因此走到 runtime 缺失分支而不是被过滤
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].call.id, padded.id);
    }

    #[test]
    fn execute_speaker_notification_tool_calls_reports_missing_runtime() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        let world = test_world();
        let calls = vec![notification_call("call-1"), notification_call("call-2")];

        let results = execute_speaker_notification_tool_calls(
            &conn,
            None,
            "sess-1",
            &world,
            3,
            "Alice",
            "avatar.png",
            Some(&calls),
        );

        assert_eq!(results.len(), 2);
        for (call, item) in calls.iter().zip(results.iter()) {
            assert_eq!(item.call.id, call.id);
            assert_eq!(
                item.result,
                serde_json::json!({
                    "tool_name": "schedule_notification",
                    "tool_call_id": call.id,
                    "ok": false,
                    "error": "notification runtime is not available",
                })
            );
        }
    }

    #[test]
    fn execute_speaker_notification_tool_calls_prefers_runtime_error_over_argument_shape() {
        // match 分支顺序：runtime 缺失时不再校验参数形状，统一报 runtime 不可用。
        let conn = Connection::open_in_memory().expect("open sqlite");
        let world = test_world();
        let mut call = notification_call("call-1");
        call.arguments = serde_json::json!("not-an-object");

        let results = execute_speaker_notification_tool_calls(
            &conn,
            None,
            "sess-1",
            &world,
            1,
            "Alice",
            "",
            Some(std::slice::from_ref(&call)),
        );

        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].result.get("error").and_then(|v| v.as_str()),
            Some("notification runtime is not available")
        );
    }

    #[test]
    fn speaker_notification_tool_error_builds_expected_payload() {
        // 覆盖 execute 中 (_, None) 分支的 payload 形状（该分支需 AppHandle，无法直接走到）。
        let call = notification_call("call-9");

        let value =
            speaker_notification_tool_error(&call, "tool arguments must be a JSON object");

        assert_eq!(
            value,
            serde_json::json!({
                "tool_name": "schedule_notification",
                "tool_call_id": "call-9",
                "ok": false,
                "error": "tool arguments must be a JSON object",
            })
        );
    }

    // ---- build_speaker_tool_followup_request ----

    fn base_chat_request() -> ChatRequest {
        ChatRequest {
            model: "gpt-test".to_string(),
            messages: vec![crate::services::llm::client::ChatMessage {
                role: "user".to_string(),
                content: serde_json::Value::String("帮我安排一个提醒".to_string()),
                reasoning_content: None,
                speaker: None,
                tool_call_id: None,
                tool_calls: None,
                metadata: None,
            }],
            generation: Default::default(),
            stream: Some(true),
            json_mode: None,
            response_schema: None,
            tools: Some(vec![crate::services::llm::client::ChatToolDefinition {
                name: "schedule_notification".to_string(),
                description: None,
                input_schema: serde_json::json!({ "type": "object" }),
            }]),
            tool_choice: Some(crate::services::llm::client::ChatToolChoice::Auto),
            native_tool_calling: Some(true),
        }
    }

    fn sample_tool_results() -> Vec<SpeakerNotificationToolResult> {
        vec![
            SpeakerNotificationToolResult {
                call: notification_call("call-1"),
                result: serde_json::json!({ "ok": true, "notification_id": "n-1" }),
            },
            SpeakerNotificationToolResult {
                call: notification_call("call-2"),
                result: serde_json::json!({ "ok": false, "error": "boom" }),
            },
        ]
    }

    #[test]
    fn build_speaker_tool_followup_request_appends_assistant_and_tool_messages() {
        let request = base_chat_request();
        let response = ChatResponse {
            content: "{\"speaker\":\"Alice\",\"content\":\"好\"}".to_string(),
            reasoning: Some("think".to_string()),
            tool_calls: None,
            usage: None,
        };
        let tool_results = sample_tool_results();

        let followup = build_speaker_tool_followup_request(&request, &response, &tool_results);

        // 原消息 + assistant + 每个 tool result 一条
        assert_eq!(followup.messages.len(), 1 + 1 + tool_results.len());
        assert_eq!(followup.messages[0].role, "user");

        let assistant = &followup.messages[1];
        assert_eq!(assistant.role, "assistant");
        assert_eq!(
            assistant.content,
            serde_json::Value::String(response.content.clone())
        );
        assert_eq!(assistant.reasoning_content.as_deref(), Some("think"));
        assert!(assistant.tool_call_id.is_none());
        let assistant_tool_calls = assistant.tool_calls.as_ref().expect("assistant tool calls");
        assert_eq!(assistant_tool_calls.len(), 2);
        assert_eq!(assistant_tool_calls[0].id, "call-1");
        assert_eq!(assistant_tool_calls[1].id, "call-2");

        for (index, item) in tool_results.iter().enumerate() {
            let tool_message = &followup.messages[2 + index];
            assert_eq!(tool_message.role, "tool");
            assert_eq!(
                tool_message.tool_call_id.as_deref(),
                Some(item.call.id.as_str())
            );
            assert!(tool_message.tool_calls.is_none());
            assert!(tool_message.reasoning_content.is_none());
            let expected_content =
                serde_json::to_string(&item.result).expect("serialize tool result");
            assert_eq!(
                tool_message.content,
                serde_json::Value::String(expected_content)
            );
        }
    }

    #[test]
    fn build_speaker_tool_followup_request_forces_non_streaming_and_clears_tool_config() {
        let request = base_chat_request();
        let response = ChatResponse {
            content: String::new(),
            reasoning: None,
            tool_calls: None,
            usage: None,
        };
        let tool_results = sample_tool_results();

        let followup = build_speaker_tool_followup_request(&request, &response, &tool_results);

        assert_eq!(followup.stream, Some(false));
        assert!(followup.tools.is_none());
        assert!(followup.tool_choice.is_none());
        assert!(followup.native_tool_calling.is_none());
        // 其余请求配置保持原样
        assert_eq!(followup.model, request.model);
        assert_eq!(followup.generation, request.generation);
        assert!(followup.json_mode.is_none());
        assert!(followup.response_schema.is_none());
    }

    // ---- fallback_notification_tool_response ----

    #[test]
    fn fallback_notification_tool_response_returns_parseable_character_payload() {
        let raw = fallback_notification_tool_response("Alice");

        let value: serde_json::Value = serde_json::from_str(&raw).expect("fallback json");
        assert_eq!(value.get("speaker").and_then(|v| v.as_str()), Some("Alice"));
        assert_eq!(
            value.get("intent").and_then(|v| v.as_str()),
            Some("notification_tool_result")
        );
        assert_eq!(
            value.get("emotion").and_then(|v| v.as_str()),
            Some("neutral")
        );
        assert_eq!(value.get("narration").and_then(|v| v.as_str()), Some(""));
        assert_eq!(
            value.get("content").and_then(|v| v.as_str()),
            Some("\u{5df2}\u{5904}\u{7406}\u{901a}\u{77e5}\u{8bf7}\u{6c42}\u{3002}")
        );
    }
}
