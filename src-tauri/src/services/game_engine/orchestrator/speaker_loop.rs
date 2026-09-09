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
        mcp_tools: &[crate::models::mcp_tool::McpToolDefinition],
        mcp_servers: &[crate::models::mcp_server::McpServerConfig],
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
                    mcp_tools,
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
                            handle_speaker_stream_chunk(
                                &chunk,
                                dialogue_pipeline,
                                &messages,
                                speaker_name,
                                turn_index,
                                &mut streamed_raw_response,
                                &mut streamed_reasoning,
                                &mut streamed_partial,
                                &mut progress_callback,
                            );
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
                    // 工具循环：与主控路同键同钳制（director_tool_loop_limit，默认 4、
                    // 范围 1-12）。此前角色路只做一轮 follow-up，模型无法「查完再追问」。
                    let tool_loop_limit = crate::services::game_engine::director::
                        WorldDirectorService::new()
                        .resolve_tool_loop_limit(world);
                    let notification_allowed =
                        world_allows_mcp_tool(world, MCP_TOOL_SCHEDULE_NOTIFICATION_ID);
                    let mut response = response;
                    let mut active_request = speaker_request.clone();
                    let mut all_tool_results: Vec<SpeakerNotificationToolResult> = Vec::new();
                    for _ in 0..tool_loop_limit {
                        // 工具执行前先广播一次「谁在调用什么工具」的占位进度，
                        // 让前端在（非流式的）工具决策阶段也有可见反馈。
                        if let Some(calls) = response
                            .tool_calls
                            .as_deref()
                            .filter(|calls| !calls.is_empty())
                        {
                            if let Some(callback) = progress_callback.as_deref_mut() {
                                let mut tool_messages = messages.clone();
                                tool_messages.push(ChatMessage {
                                    message_id: ChatMessage::generate_id(),
                                    created_at: chrono::Utc::now().to_rfc3339(),
                                    parent_message_id: None,
                                    role: "agent".to_string(),
                                    content: MessageContent::Text(String::new()),
                                    speaker: Some(speaker_name.clone()),
                                    metadata: Some(serde_json::json!({
                                        "turn_index": turn_index,
                                        "message_kind": "agent_response",
                                        "tool_activity": build_tool_activity_value(
                                            calls,
                                            mcp_tools,
                                            "calling",
                                        ),
                                    })),
                                });
                                callback(SpeakerTurnProgress {
                                    messages: tool_messages,
                                    speaker_name: speaker_name.clone(),
                                    narration: None,
                                    is_placeholder: true,
                                    is_error: false,
                                });
                            }
                        }
                        let mut round_results = Vec::new();
                        if notification_allowed {
                            let db_guard = db.lock().await;
                            round_results.extend(execute_speaker_notification_tool_calls(
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
                            ));
                        }
                        round_results.extend(
                            execute_speaker_custom_tool_calls(
                                world,
                                mcp_tools,
                                mcp_servers,
                                response.tool_calls.as_deref(),
                            )
                            .await,
                        );
                        if round_results.is_empty() {
                            break;
                        }
                        let followup_request = build_speaker_tool_followup_request(
                            &active_request,
                            &response,
                            &round_results,
                            speaker_model.streaming_enabled,
                        );
                        all_tool_results.extend(round_results);
                        // 追问保留工具（模型可补调失败/遗漏的工具）且按模型配置
                        // 流式：玩家在「查完数据写回复」阶段能看到思维链与正文逐字
                        // 出现。每轮追问前清空流式累积器，避免把上一轮的流式内容
                        // 拼进本轮解析；流式失败回退非流式时同样清空（已被污染）。
                        let followup_result = if speaker_model.streaming_enabled {
                            streamed_raw_response.clear();
                            streamed_reasoning.clear();
                            streamed_partial = None;
                            let streamed = llm_client
                                .chat_completion_stream(
                                    &speaker_provider,
                                    &speaker_model.base_url,
                                    &speaker_model.api_key,
                                    &followup_request,
                                    |chunk| {
                                        handle_speaker_stream_chunk(
                                            &chunk,
                                            dialogue_pipeline,
                                            &messages,
                                            speaker_name,
                                            turn_index,
                                            &mut streamed_raw_response,
                                            &mut streamed_reasoning,
                                            &mut streamed_partial,
                                            &mut progress_callback,
                                        );
                                    },
                                )
                                .await;
                            match streamed {
                                Ok(response) => Ok(response),
                                Err(_) => {
                                    streamed_raw_response.clear();
                                    streamed_reasoning.clear();
                                    streamed_partial = None;
                                    llm_client
                                        .chat_completion(
                                            &speaker_provider,
                                            &speaker_model.base_url,
                                            &speaker_model.api_key,
                                            &followup_request,
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
                                    &followup_request,
                                )
                                .await
                        };
                        match followup_result {
                            Ok(followup_response) => {
                                active_request = followup_request;
                                response = followup_response;
                            }
                            Err(_) => {
                                if response.content.trim().is_empty() {
                                    response = crate::services::llm::client::ChatResponse {
                                        content: fallback_notification_tool_response(speaker_name),
                                        reasoning: response.reasoning.clone(),
                                        tool_calls: None,
                                        usage: response.usage.clone(),
                                        // 兜底文案是本地合成的完整内容，不是被截断的模型输出。
                                        finish_reason: None,
                                    };
                                }
                                break;
                            }
                        }
                    }
                    let notification_tool_results = all_tool_results;
                    // max_tokens 预算被思维链烧光时（finish_reason=length 且正文一个字都没
                    // 出来），静默落到「没有台词」占位会掩盖真实原因。用放大的预算非流式
                    // 自动重试一次：同样的消息再生成一遍，多数情况下能拿到完整正文。
                    if response.content.trim().is_empty()
                        && response
                            .tool_calls
                            .as_deref()
                            .map(|calls| calls.is_empty())
                            .unwrap_or(true)
                        && response.finish_reason.as_deref() == Some("length")
                    {
                        let current_max = active_request.generation.max_tokens.unwrap_or(0);
                        let bumped_max = current_max.saturating_mul(4).clamp(4096, 16000);
                        if bumped_max > current_max {
                            let mut retry_request = active_request.clone();
                            retry_request.generation.max_tokens = Some(bumped_max);
                            retry_request.stream = Some(false);
                            if let Ok(retry_response) = llm_client
                                .chat_completion(
                                    &speaker_provider,
                                    &speaker_model.base_url,
                                    &speaker_model.api_key,
                                    &retry_request,
                                )
                                .await
                            {
                                response = retry_response;
                            }
                        }
                    }
                    let response = response;
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
                    let mut message_metadata = serde_json::json!({
                        "turn_index": turn_index,
                        "narration": parsed_response.narration.clone(),
                        "message_kind": "agent_response",
                        "reasoning": reasoning_text,
                        "reasoning_expanded": false,
                        "raw_response": raw_response.clone()
                    });
                    // 本回合实际执行过的工具记入最终消息 metadata，
                    // 前端在回复下方显示「已调用：…」。
                    if !notification_tool_results.is_empty() {
                        let done_calls = notification_tool_results
                            .iter()
                            .map(|item| item.call.clone())
                            .collect::<Vec<_>>();
                        message_metadata.as_object_mut().unwrap().insert(
                            "tool_activity".to_string(),
                            build_tool_activity_value(&done_calls, mcp_tools, "done"),
                        );
                    }
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

/// 执行角色发起的 schedule_notification 调用。该工具要写库并注册系统通知，
/// 因此与走 MCP 执行器的自定义工具分开处理（自定义工具见
/// `execute_speaker_custom_tool_calls`）。
#[allow(clippy::too_many_arguments)]
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

/// 执行角色发起的自定义 MCP 工具调用（世界包通过 allowed_mcp_tool_ids 授权的那些）。
///
/// 复用主控路的 `execute_pending_mcp_tool_calls`：白名单校验、`builtin_http` 本地执行
/// 与外部 MCP server 分派全在那一份实现里，此处只做调用形状的转换（ChatToolCall →
/// tool_calls JSON → 按下标取回结果）。
async fn execute_speaker_custom_tool_calls(
    world: &WorldDefinition,
    mcp_tools: &[crate::models::mcp_tool::McpToolDefinition],
    mcp_servers: &[crate::models::mcp_server::McpServerConfig],
    tool_calls: Option<&[ChatToolCall]>,
) -> Vec<SpeakerNotificationToolResult> {
    let custom_calls = tool_calls
        .unwrap_or(&[])
        .iter()
        .filter(|call| call.tool_name.trim() != "schedule_notification")
        .cloned()
        .collect::<Vec<_>>();
    if custom_calls.is_empty() {
        return Vec::new();
    }
    let parsed = serde_json::json!({
        "tool_calls": custom_calls
            .iter()
            .map(|call| serde_json::json!({
                "id": call.id,
                "tool_name": call.tool_name,
                "arguments": call.arguments,
            }))
            .collect::<Vec<_>>(),
    });
    let director = crate::services::game_engine::director::WorldDirectorService::new();
    let results = director
        .execute_pending_mcp_tool_calls(&parsed, world, mcp_tools, mcp_servers)
        .await;
    custom_calls
        .into_iter()
        .enumerate()
        .map(|(index, call)| {
            let mut entry = results
                .get(&index)
                .and_then(|value| value.as_object().cloned())
                .unwrap_or_else(|| {
                    // results 里没有该下标 = 超出单轮工具调用上限被丢弃（不是未授权），
                    // 文案必须说清，否则模型会误以为工具不可用、向玩家报错原因。
                    serde_json::json!({
                        "ok": false,
                        "error": format!(
                            "超出单轮工具调用上限，本次未执行：{}。请减少单次并发的工具调用数量，分多轮调用。",
                            call.tool_name.trim()
                        ),
                    })
                    .as_object()
                    .cloned()
                    .unwrap_or_default()
                });
            entry.insert(
                "id".to_string(),
                serde_json::Value::String(call.id.clone()),
            );
            entry.insert(
                "tool_name".to_string(),
                serde_json::Value::String(call.tool_name.trim().to_string()),
            );
            SpeakerNotificationToolResult {
                call,
                result: serde_json::Value::Object(entry),
            }
        })
        .collect()
}

/// 首呼与工具追问共用的流式 chunk 处理：累积原文/思维链/部分解析结果，
/// 并向进度回调推送逐字更新的中间消息。
#[allow(clippy::too_many_arguments)]
fn handle_speaker_stream_chunk(
    chunk: &crate::services::llm::client::ChatStreamChunk,
    dialogue_pipeline: &DialoguePipeline,
    messages: &[ChatMessage],
    speaker_name: &str,
    turn_index: i32,
    streamed_raw_response: &mut String,
    streamed_reasoning: &mut String,
    streamed_partial: &mut Option<crate::services::game_engine::dialogue::ParsedCharacterResponse>,
    progress_callback: &mut Option<&mut (dyn FnMut(SpeakerTurnProgress) + Send)>,
) {
    let has_reasoning_delta = chunk.reasoning_delta.is_some();
    if let Some(reasoning_delta) = chunk.reasoning_delta.as_deref() {
        streamed_reasoning.push_str(reasoning_delta);
    }
    if !chunk.delta.is_empty() {
        streamed_raw_response.push_str(&chunk.delta);
        if let Some(parsed_partial) =
            dialogue_pipeline.extract_partial_character_response(streamed_raw_response, speaker_name)
        {
            *streamed_partial = Some(parsed_partial);
        }
    }
    if let Some(callback) = progress_callback.as_deref_mut() {
        if has_reasoning_delta || streamed_partial.is_some() {
            let partial = streamed_partial.clone();
            let mut progress_messages = messages.to_vec();
            progress_messages.push(ChatMessage {
                message_id: ChatMessage::generate_id(),
                created_at: chrono::Utc::now().to_rfc3339(),
                parent_message_id: None,
                role: "agent".to_string(),
                content: MessageContent::Text(
                    partial
                        .as_ref()
                        .map(|value| value.content.clone())
                        .unwrap_or_default(),
                ),
                speaker: Some(
                    partial
                        .as_ref()
                        .map(|value| value.speaker.clone())
                        .unwrap_or_else(|| speaker_name.to_string()),
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
                    .unwrap_or_else(|| speaker_name.to_string()),
                narration: partial
                    .as_ref()
                    .map(|value| value.narration.clone())
                    .filter(|value| !value.trim().is_empty()),
                is_placeholder: false,
                is_error: false,
            });
        }
    }
}

/// 工具调用进度的 metadata 值。status 为 "calling"（执行前的占位进度）或
/// "done"（回合结束后的最终消息记录）。显示名优先取工具定义里的中文名，
/// 取不到回退模型侧调用名；"done" 时按 id 去重（同一工具可能调用多轮）。
fn build_tool_activity_value(
    tool_calls: &[ChatToolCall],
    mcp_tools: &[crate::models::mcp_tool::McpToolDefinition],
    status: &str,
) -> serde_json::Value {
    let mut seen_ids = std::collections::HashSet::new();
    let mut tools = Vec::new();
    for call in tool_calls {
        let call_name = call.tool_name.trim();
        if call_name.is_empty() {
            continue;
        }
        if status == "done" && !seen_ids.insert(call_name.to_string()) {
            continue;
        }
        let display_name = mcp_tools
            .iter()
            .find(|tool| tool.tool_name.trim() == call_name)
            .map(|tool| tool.name.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| call_name.to_string());
        let mut entry = serde_json::json!({ "id": call_name, "name": display_name });
        if status == "calling" {
            if let Some(preview) = summarize_tool_call_arguments(&call.arguments) {
                entry
                    .as_object_mut()
                    .unwrap()
                    .insert("args_preview".to_string(), serde_json::Value::String(preview));
            }
        }
        tools.push(entry);
    }
    serde_json::json!({ "status": status, "tools": tools })
}

/// 把工具参数压成一行短摘要（取前两个标量参数值，截断 40 字符），
/// 供「正在调用」状态行显示，例如 `1.600519, 60`。
fn summarize_tool_call_arguments(arguments: &serde_json::Value) -> Option<String> {
    let object = arguments.as_object()?;
    let mut parts = Vec::new();
    for value in object.values() {
        let rendered = match value {
            serde_json::Value::String(text) => text.trim().to_string(),
            serde_json::Value::Number(number) => number.to_string(),
            serde_json::Value::Bool(flag) => flag.to_string(),
            _ => continue,
        };
        if rendered.is_empty() {
            continue;
        }
        parts.push(rendered);
        if parts.len() >= 2 {
            break;
        }
    }
    if parts.is_empty() {
        return None;
    }
    // serde_json 未开 preserve_order 时 map 按键排序，参数顺序不可依赖；
    // 排序后拼接保证输出确定（数字开头的代码类参数通常排在前面）。
    parts.sort();
    const MAX_PREVIEW_CHARS: usize = 40;
    let joined = parts.join(", ");
    if joined.chars().count() > MAX_PREVIEW_CHARS {
        let mut truncated = joined.chars().take(MAX_PREVIEW_CHARS).collect::<String>();
        truncated.push('…');
        Some(truncated)
    } else {
        Some(joined)
    }
}

/// 工具结果回灌后的追问请求。与主控路一致：追问**保留** tools/tool_choice/
/// native_tool_calling，模型才能对失败或遗漏的工具发起多轮补调（否则模型想
/// 重试时没有工具可用，容易交白卷——content 只剩空白）。流式按模型配置透传；
/// openai 流式路径支持工具调用增量累积，anthropic 不支持时由调用方回退非流式。
fn build_speaker_tool_followup_request(
    request: &crate::services::llm::client::ChatRequest,
    response: &crate::services::llm::client::ChatResponse,
    tool_results: &[SpeakerNotificationToolResult],
    stream: bool,
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
    followup.stream = Some(stream);
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
#[path = "speaker_loop_tests.rs"]
mod tests;

