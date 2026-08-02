use rusqlite::Connection;
use tauri::{AppHandle, State};

use crate::events::session_events::SessionEventEmitter;
use crate::models::session::*;
use crate::services::game_engine::service_mode::{resolve_service_runtime_config, ServiceMode};
use crate::services::notifications::{
    NotificationScheduler, NotificationToolInput, NotificationToolRuntime,
};
use crate::state::AppState;

#[tauri::command]
pub async fn get_session(
    state: State<'_, AppState>,
    id: String,
) -> Result<SessionSnapshot, String> {
    let prepared = {
        let db = state.db.lock().await;
        crate::services::notifications::sync_session_schedule_attribute(db.conn(), &id)?;
        state
            .services
            .runtime
            .session_orchestrator
            .prepare_get_session_context(db.conn(), &id)?
    };
    let updated = state
        .services
        .runtime
        .session_orchestrator
        .resolve_session_assets(
            &state.services.runtime.asset_resolver,
            &state.data_dir,
            &prepared.session,
            &prepared.world,
            &prepared.characters,
            prepared.image_model.as_ref(),
        )
        .await;
    {
        let db = state.db.lock().await;
        let merged = state
            .services
            .runtime
            .session_orchestrator
            .persist_resolved_session_assets(db.conn(), &id, &updated)?;
        if let Some(overlay) = state
            .services
            .runtime
            .session_orchestrator
            .build_incomplete_turn_overlay(db.conn(), &merged)?
        {
            return Ok(overlay);
        }
        Ok(merged)
    }
}

#[tauri::command]
pub async fn create_session(
    state: State<'_, AppState>,
    request: SessionCreateRequest,
) -> Result<SessionSnapshot, String> {
    let prepared = {
        let db = state.db.lock().await;
        state
            .services
            .runtime
            .session_orchestrator
            .prepare_create_session_context(
                db.conn(),
                &request.world_id,
                request.player_character_id.as_deref(),
            )?
    };
    let updated = state
        .services
        .runtime
        .session_orchestrator
        .resolve_session_assets(
            &state.services.runtime.asset_resolver,
            &state.data_dir,
            &prepared.session,
            &prepared.world,
            &prepared.characters,
            prepared.image_model.as_ref(),
        )
        .await;
    {
        let db = state.db.lock().await;
        state
            .services
            .runtime
            .session_orchestrator
            .persist_session_snapshot(db.conn(), &updated)?;
    }
    Ok(updated)
}

#[tauri::command]
pub async fn submit_player_action(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    request: PlayerActionRequest,
) -> Result<SessionSnapshot, String> {
    let _mutation_permit = state.session_mutations.try_acquire(&session_id)?;
    submit_player_action_inner(app, state, session_id, request).await
}

async fn submit_player_action_inner(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    request: PlayerActionRequest,
) -> Result<SessionSnapshot, String> {
    let prepared = {
        let db = state.db.lock().await;
        state
            .services
            .runtime
            .session_orchestrator
            .prepare_turn_context(db.conn(), &session_id, &request)?
    };
    let crate::services::game_engine::orchestrator::PreparedTurnContext {
        session,
        world,
        mut characters,
        turn_index,
        recovery_journal,
        resume_incomplete_turn,
        image_model,
        director_model,
        messages,
        director_completed_payload,
    } = prepared;
    // 第 10 项（校验点 A）：玩家附件只发给声明了对应输入模态的模型。
    // 导演模式校验导演模型 + 全部角色模型（任一都可能被安排发言）；
    // 不支持即提交即报错，不写回合数据、不发 HTTP。
    let player_media = request.content.media_parts();
    if !player_media.is_empty() {
        let db = state.db.lock().await;
        let mut checked_model_ids = std::collections::HashSet::new();
        let mut check_model = |model: &crate::models::model_config::ModelConfig| -> Result<(), String> {
            if checked_model_ids.insert(model.id.clone()) {
                crate::models::model_config::ensure_media_supported(model, &player_media)?;
            }
            Ok(())
        };
        check_model(&director_model)?;
        for character in &characters {
            let model = crate::services::game_engine::orchestrator::resolve_text_model(
                db.conn(),
                Some(character.model.as_str()).filter(|value| !value.trim().is_empty()),
            )?;
            check_model(&model)?;
        }
    }
    // 重放（重发/编辑）会回滚到目标回合之前：取消被覆盖回合调度的未触发通知，避免重复提醒。
    if request.action_mode.requires_replay() {
        if let Some(from_turn_index) = request.resend_from_turn_index {
            let db = state.db.lock().await;
            cancel_notifications_for_turns(db.conn(), &app, &session_id, from_turn_index)?;
        }
    }
    let service_config = resolve_service_runtime_config(&world);
    if service_config.service_mode == ServiceMode::AgentChat {
        return run_agent_chat_player_action(
            &app,
            &state,
            session_id,
            request,
            service_config,
            session,
            world,
            characters,
            turn_index,
            recovery_journal,
            image_model,
            messages,
        )
        .await;
    }

    let director_service = &state.services.runtime.world_director;
    // 第 7 项：只有绑定了「本平台可用」server 的自定义工具才下发给模型，
    // 避免模型调用一个必然失败的工具（安卓上的 stdio 配置即属此类）。
    let (mcp_tools, mcp_servers) = {
        let db = state.db.lock().await;
        let tools =
            crate::db::repositories::mcp_tool_repo::McpToolRepository::new(db.conn()).list()?;
        let servers =
            crate::db::repositories::mcp_server_repo::McpServerRepository::new(db.conn()).list()?;
        let executable = crate::services::mcp::executor::filter_executable_tools(&tools, &servers);
        (executable, servers)
    };
    // 会话级 variables KV（第 6 项 {{var:key}} 占位符的数据源）
    // 与导演的生成参数（第 8 项：应用→世界→会话三级覆盖）一起在同一次加锁内读出。
    let (kv_vars, runtime_attributes, director_generation) = {
        let db = state.db.lock().await;
        let settings = crate::commands::settings::load_app_settings(db.conn())?;
        (
            crate::services::game_engine::prompting::load_prompt_kv_vars(db.conn(), &session_id)?,
            state
                .services
                .runtime
                .session_orchestrator
                .get_session_runtime_attributes(db.conn(), &session_id)?,
            crate::services::game_engine::orchestrator::resolve_generation_params_with_model(
                crate::models::generation_params::GENERATION_ROLE_DIRECTOR,
                &settings,
                &world,
                &session,
                &director_model,
            ),
        )
    };
    let mut emit_director_progress =
        |progress: crate::services::game_engine::director::DirectorLoopStreamProgress| {
            let trace_message =
                crate::services::game_engine::orchestrator::build_director_trace_message_from_stream_progress(&progress);
            let trace_chat_message =
                crate::services::game_engine::orchestrator::build_streaming_director_trace_chat_message(
                    &trace_message,
                    turn_index,
                );
            let snapshot =
                build_director_progress_snapshot(&session, &messages, trace_chat_message);
            SessionEventEmitter::emit_snapshot_logged(&app, &session_id, &snapshot);
        };
    let director_turn = state
        .services
        .runtime
        .session_orchestrator
        .run_director_turn(
            &state.services.llm_client,
            director_service,
            director_model,
            crate::services::game_engine::orchestrator::DirectorTurnRecovery {
                resume_incomplete_turn,
                recovered_completed_payload: director_completed_payload,
            },
            &session,
            &world,
            &characters,
            turn_index,
            request.content.as_str(),
            &player_media,
            &mcp_tools,
            &mcp_servers,
            &kv_vars,
            &runtime_attributes,
            &director_generation,
            Some(NotificationToolRuntime {
                app: &app,
                data_dir: &state.data_dir,
            }),
            Some(&mut emit_director_progress),
        )
        .await;
    let director_turn = match director_turn {
        Ok(value) => value,
        Err(failure) => {
            let overlay = {
                let db = state.db.lock().await;
                let failure_message = state
                    .services
                    .runtime
                    .session_orchestrator
                    .record_structured_output_failure(
                        db.conn(),
                        &session_id,
                        turn_index,
                        &request,
                        &failure,
                    )?;
                let mut snapshot = session.clone();
                snapshot.messages = messages.clone();
                snapshot.messages.push(failure_message);
                snapshot
            };
            SessionEventEmitter::emit_snapshot_logged(&app, &session_id, &overlay);
            return Ok(overlay);
        }
    };

    let runtime_preparation = {
        let db = state.db.lock().await;
        state
            .services
            .runtime
            .session_orchestrator
            .prepare_director_runtime(
                db.conn(),
                director_service,
                &recovery_journal,
                &session_id,
                turn_index,
                &session,
                &world,
                &mut characters,
                &director_turn.parsed,
                &director_turn.runtime_payload,
                director_turn.trace_message.as_ref(),
            )?
    };

    {
        let snapshot = build_progress_snapshot(
            &session,
            &runtime_preparation,
            &crate::services::game_engine::orchestrator::SpeakerTurnProgress {
                messages: messages.clone(),
                speaker_name: String::new(),
                narration: None,
                is_placeholder: false,
                is_error: false,
            },
            turn_index,
            session.messages.len().saturating_add(1),
        );
        SessionEventEmitter::emit_snapshot_logged(&app, &session_id, &snapshot);
    }

    let speaker_turn_result = {
        let base_message_count = session.messages.len().saturating_add(1);
        let mut emit_progress =
            |progress: crate::services::game_engine::orchestrator::SpeakerTurnProgress| {
                let snapshot = build_progress_snapshot(
                    &session,
                    &runtime_preparation,
                    &progress,
                    turn_index,
                    base_message_count,
                );
                SessionEventEmitter::emit_snapshot_logged(&app, &session_id, &snapshot);
            };
        state
            .services
            .runtime
            .session_orchestrator
            .run_speaker_turns(
                &state.db,
                &state.services.llm_client,
                &state.services.runtime.dialogue_pipeline,
                &state.services.runtime.memory,
                &session_id,
                turn_index,
                &recovery_journal,
                &session,
                &world,
                &characters,
                messages,
                &runtime_preparation.planned_speakers,
                request.content.as_str(),
                &player_media,
                &runtime_preparation.next_scene_name,
                &runtime_preparation.next_location,
                &runtime_preparation.visible_chars,
                Some(NotificationToolRuntime {
                    app: &app,
                    data_dir: &state.data_dir,
                }),
                Some(&mut emit_progress),
            )
            .await?
    };
    let messages = speaker_turn_result.messages.clone();
    if let Some(failure) = speaker_turn_result.failure {
        let overlay = {
            let db = state.db.lock().await;
            let failure_message = state
                .services
                .runtime
                .session_orchestrator
                .record_structured_output_failure(
                    db.conn(),
                    &session_id,
                    turn_index,
                    &request,
                    &failure,
                )?;
            let mut snapshot = session.clone();
            snapshot.messages = messages.clone();
            snapshot.messages.push(failure_message);
            snapshot
        };
        SessionEventEmitter::emit_snapshot_logged(&app, &session_id, &overlay);
        return Ok(overlay);
    }

    // 普通导演模式下,说话人 payload 默认不进入 runtime 解析;事实卡片是唯一例外,
    // 只并 fact_extractions 一个键,不改变其它字段的既有行为。
    let parsed_runtime_with_facts = merge_speaker_fact_extractions(
        &runtime_preparation.parsed_runtime,
        &speaker_turn_result.runtime_payloads,
    );
    let runtime_application = {
        let db = state.db.lock().await;
        crate::services::game_engine::runtime_effects::apply_director_runtime_effects_with_preface(
            db.conn(),
            &state.services.runtime.inventory,
            &state.services.runtime.trigger_engine,
            &state.services.runtime.rule_engine,
            &state.services.runtime.scene_manager,
            &state.services.runtime.state_engine,
            &world,
            &session,
            &characters,
            turn_index,
            request.content.as_str(),
            &parsed_runtime_with_facts,
            &runtime_preparation.pre_runtime_system_messages,
        )?
    };

    let updated = state
        .services
        .runtime
        .session_orchestrator
        .apply_runtime_mutations(
            crate::services::game_engine::orchestrator::RuntimeMutationInput {
                asset_resolver: &state.services.runtime.asset_resolver,
                data_dir: &state.data_dir,
                session: &session,
                messages: &messages,
                world: &world,
                characters: &characters,
                turn_index,
                next_location: &runtime_preparation.next_location,
                next_time_label: &runtime_preparation.next_time_label,
                next_scene_name: &runtime_preparation.next_scene_name,
                current_line: runtime_preparation.current_line.as_deref(),
                next_scene_background_hint: runtime_preparation.next_scene_background_hint.clone(),
                planned_speakers: &runtime_preparation.planned_speakers,
                scene_visible_characters_explicit: runtime_preparation
                    .scene_visible_characters_explicit,
                scene_visible_characters: &runtime_preparation.scene_visible_characters,
                visible_chars: &runtime_preparation.visible_chars,
                runtime_application: &runtime_application,
                image_model: image_model.as_ref(),
                parsed_runtime: &runtime_preparation.parsed_runtime,
            },
        )
        .await;

    {
        let db = state.db.lock().await;
        state.services.runtime.memory.commit_turn_memories(
            db.conn(),
            &recovery_journal,
            &session_id,
            turn_index,
            &runtime_application,
            &updated,
            &session,
            &world,
            &characters,
        )?;
        finalize_turn_snapshot(
            &app,
            &state.data_dir,
            &state.services.runtime.session_orchestrator,
            db.conn(),
            director_service,
            &recovery_journal,
            &session_id,
            turn_index,
            &runtime_application,
            &updated,
            &session,
            &world,
            &characters,
            &runtime_preparation.parsed_runtime,
            &runtime_preparation.planned_speakers,
            &runtime_preparation.scene_visible_characters,
            &director_turn.traces,
            &director_turn.provider,
            &director_turn.model,
            request.content.as_str(),
            director_turn.tool_loop_limit,
        )?;
    }

    Ok(updated)
}

#[tauri::command]
pub async fn switch_player_character(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    request: SwitchCharacterRequest,
) -> Result<SessionSnapshot, String> {
    let _mutation_permit = state.session_mutations.try_acquire(&session_id)?;
    switch_player_character_inner(app, state, session_id, request).await
}

async fn switch_player_character_inner(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    request: SwitchCharacterRequest,
) -> Result<SessionSnapshot, String> {
    let prepared = {
        let db = state.db.lock().await;
        state
            .services
            .runtime
            .session_orchestrator
            .prepare_switch_player_character_context(db.conn(), &session_id, &request)?
    };
    let updated = state
        .services
        .runtime
        .session_orchestrator
        .switch_player_character(
            crate::services::game_engine::orchestrator::SwitchPlayerCharacterInput {
                asset_resolver: &state.services.runtime.asset_resolver,
                data_dir: &state.data_dir,
                session: &prepared.session,
                world: &prepared.world,
                characters: &prepared.characters,
                new_character: &prepared.new_character,
                proposal: request.proposal.as_ref(),
                image_model: prepared.image_model.as_ref(),
            },
        )
        .await?;
    {
        let db = state.db.lock().await;
        state
            .services
            .runtime
            .session_orchestrator
            .writeback_switch_player_character(db.conn(), &updated)?;
    }
    SessionEventEmitter::emit_snapshot_logged(&app, &session_id, &updated);
    Ok(updated)
}

#[tauri::command]
pub async fn resume_last_incomplete_turn(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<SessionSnapshot, String> {
    let _mutation_permit = state.session_mutations.try_acquire(&session_id)?;
    resume_last_incomplete_turn_inner(app, state, session_id).await
}

async fn resume_last_incomplete_turn_inner(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<SessionSnapshot, String> {
    let player_request = {
        let db = state.db.lock().await;
        state
            .services
            .runtime
            .session_orchestrator
            .load_resume_player_request(db.conn(), &session_id)?
    };

    if let Some(player_request) = player_request {
        submit_player_action_inner(app, state, session_id, player_request).await
    } else {
        get_session(state, session_id).await
    }
}

#[tauri::command]
pub async fn retry_failed_llm_step(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    request: RetryFailedLlmStepRequest,
) -> Result<SessionSnapshot, String> {
    let _mutation_permit = state.session_mutations.try_acquire(&session_id)?;
    {
        let db = state.db.lock().await;
        state
            .services
            .runtime
            .session_orchestrator
            .claim_retry_capsule(db.conn(), &session_id, &request.retry_token)?;
    }
    let result =
        match resume_last_incomplete_turn_inner(app, state.clone(), session_id.clone()).await {
            Ok(result) => result,
            Err(err) => {
                // 回合执行失败:把胶囊退回 active,允许用户用同一 token 再试(不双重消费)。
                let db = state.db.lock().await;
                let _ = state
                    .services
                    .runtime
                    .session_orchestrator
                    .release_retry_capsule(db.conn(), &session_id, &request.retry_token);
                return Err(err);
            }
        };
    {
        let db = state.db.lock().await;
        state
            .services
            .runtime
            .session_orchestrator
            .finalize_retry_capsule(db.conn(), &session_id, &request.retry_token)?;
    }
    Ok(result)
}

async fn run_agent_chat_player_action(
    app: &AppHandle,
    state: &State<'_, AppState>,
    session_id: String,
    request: PlayerActionRequest,
    service_config: crate::services::game_engine::service_mode::ServiceRuntimeConfig,
    session: SessionSnapshot,
    world: crate::models::world::WorldDefinition,
    characters: Vec<crate::models::character::CharacterDefinition>,
    turn_index: i32,
    recovery_journal: Vec<serde_json::Value>,
    image_model: Option<crate::models::model_config::ModelConfig>,
    messages: Vec<crate::models::session::ChatMessage>,
) -> Result<SessionSnapshot, String> {
    let player_media = request.content.media_parts();
    let target = state
        .services
        .runtime
        .session_orchestrator
        .prepare_agent_chat_target(&service_config, &session, &characters)?;
    {
        let db = state.db.lock().await;
        state
            .services
            .runtime
            .session_orchestrator
            .ensure_agent_chat_runtime_session(
                db.conn(),
                &session_id,
                turn_index,
                &service_config,
                &target.agent,
            )?;
    }
    let (session, messages) = state
        .services
        .runtime
        .session_orchestrator
        .normalize_agent_chat_player_identity(&session, messages, &target.agent);
    let speaker_turn_result = {
        let base_message_count = session.messages.len().saturating_add(1);
        let mut emit_progress =
            |progress: crate::services::game_engine::orchestrator::SpeakerTurnProgress| {
                let snapshot = build_agent_chat_progress_snapshot(
                    &session,
                    &target,
                    &progress,
                    turn_index,
                    base_message_count,
                );
                SessionEventEmitter::emit_snapshot_logged(app, &session_id, &snapshot);
            };
        crate::services::game_engine::orchestrator::run_agent_chat_speaker_turn(
            &state.services.runtime.session_orchestrator,
            &state.db,
            &state.services.llm_client,
            &state.services.runtime.dialogue_pipeline,
            &state.services.runtime.memory,
            &session_id,
            turn_index,
            &recovery_journal,
            &session,
            &world,
            &characters,
            messages,
            &target,
            request.content.as_str(),
            &player_media,
            Some(NotificationToolRuntime {
                app,
                data_dir: &state.data_dir,
            }),
            Some(&mut emit_progress),
        )
        .await?
    };
    let messages = speaker_turn_result.messages.clone();
    if let Some(failure) = speaker_turn_result.failure {
        let overlay = {
            let db = state.db.lock().await;
            let failure_message = state
                .services
                .runtime
                .session_orchestrator
                .record_structured_output_failure(
                    db.conn(),
                    &session_id,
                    turn_index,
                    &request,
                    &failure,
                )?;
            let mut snapshot = session.clone();
            snapshot.messages = messages.clone();
            snapshot.messages.push(failure_message);
            snapshot
        };
        SessionEventEmitter::emit_snapshot_logged(app, &session_id, &overlay);
        return Ok(overlay);
    }

    let speaker_messages = messages
        .iter()
        .filter(|message| {
            message
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("turn_index"))
                .and_then(|value| value.as_i64())
                == Some(turn_index as i64)
        })
        .cloned()
        .collect::<Vec<_>>();
    let updated = state
        .services
        .runtime
        .session_orchestrator
        .build_agent_chat_updated_session(
            crate::services::game_engine::orchestrator::AgentChatTurnInput {
                service_config: &service_config,
                asset_resolver: &state.services.runtime.asset_resolver,
                data_dir: &state.data_dir,
                session: &session,
                world: &world,
                characters: &characters,
                turn_index,
                messages: messages.clone(),
                speaker_messages: &speaker_messages,
                image_model: image_model.as_ref(),
            },
        )
        .await;
    let agent_runtime_payload =
        merge_agent_chat_runtime_payloads(&speaker_turn_result.runtime_payloads);
    {
        let db = state.db.lock().await;
        let mut runtime_application = if agent_runtime_payload
            .as_object()
            .map(|object| object.is_empty())
            .unwrap_or(true)
        {
            crate::services::game_engine::runtime_effects::DirectorRuntimeApplication::default()
        } else {
            crate::services::game_engine::runtime_effects::apply_director_runtime_effects(
                db.conn(),
                &state.services.runtime.inventory,
                &state.services.runtime.trigger_engine,
                &state.services.runtime.rule_engine,
                &state.services.runtime.scene_manager,
                &state.services.runtime.state_engine,
                &world,
                &session,
                &characters,
                turn_index,
                request.content.as_str(),
                &agent_runtime_payload,
            )?
        };
        runtime_application
            .pending_notifications
            .extend(speaker_turn_result.pending_notifications.clone());
        state.services.runtime.memory.commit_turn_memories(
            db.conn(),
            &recovery_journal,
            &session_id,
            turn_index,
            &runtime_application,
            &updated,
            &session,
            &world,
            &characters,
        )?;
        state
            .services
            .runtime
            .session_orchestrator
            .writeback_agent_chat_turn(
                crate::services::game_engine::orchestrator::AgentChatWritebackInput {
                    conn: db.conn(),
                    recovery_journal: &recovery_journal,
                    session_id: &session_id,
                    turn_index,
                    runtime_application: &runtime_application,
                    updated: &updated,
                    service_config: &service_config,
                    agent: &target.agent,
                },
            )?;
        schedule_pending_notifications(
            app,
            &state.data_dir,
            db.conn(),
            &recovery_journal,
            &session_id,
            turn_index,
            &runtime_application,
            &world,
        )?;
    }
    SessionEventEmitter::emit_snapshot_logged(app, &session_id, &updated);
    Ok(updated)
}

fn merge_agent_chat_runtime_payloads(payloads: &[serde_json::Value]) -> serde_json::Value {
    const ARRAY_KEYS: &[&str] = &[
        "session_attribute_updates",
        "attribute_updates",
        "character_attribute_updates",
        "pending_notifications",
        "memory_entries",
        "memory_events",
        "fact_extractions",
        "tool_calls",
    ];
    const VALUE_KEYS: &[&str] = &[
        "inventory_items",
        "inventory",
        "state_metrics",
        "session_metrics",
        "metrics",
        "state_phase",
        "world_phase",
        "player_stats",
    ];

    let mut merged = serde_json::Map::new();
    for payload in payloads {
        let Some(object) = payload.as_object() else {
            continue;
        };
        for key in ARRAY_KEYS {
            let Some(items) = object.get(*key).and_then(|value| value.as_array()) else {
                continue;
            };
            let entry = merged
                .entry((*key).to_string())
                .or_insert_with(|| serde_json::Value::Array(Vec::new()));
            if let Some(target) = entry.as_array_mut() {
                target.extend(items.iter().cloned());
            }
        }
        for key in VALUE_KEYS {
            if let Some(value) = object.get(*key) {
                if !value.is_null() {
                    merged.insert((*key).to_string(), value.clone());
                }
            }
        }
    }
    // NPC 回复契约用的字段名是 memory_entries，但 runtime 解析读的是 memory_events。
    // 把 memory_entries 的条目并入 memory_events，确保 NPC 写入的记忆能落库。
    if let Some(entries) = merged.get("memory_entries").and_then(|value| value.as_array()).cloned() {
        if !entries.is_empty() {
            let events = merged
                .entry("memory_events".to_string())
                .or_insert_with(|| serde_json::Value::Array(Vec::new()));
            if let Some(target) = events.as_array_mut() {
                target.extend(entries);
            }
        }
    }
    serde_json::Value::Object(merged)
}

/// 把各说话人 payload 里的 fact_extractions 数组并进导演 payload。
/// 普通导演模式下 speaker_turn_result.runtime_payloads 不进入 runtime 解析,
/// 事实卡片需要这条显式通道;parsed_runtime 已有同名字段时追加而非覆盖。
fn merge_speaker_fact_extractions(
    parsed_runtime: &serde_json::Value,
    speaker_payloads: &[serde_json::Value],
) -> serde_json::Value {
    let facts: Vec<serde_json::Value> = speaker_payloads
        .iter()
        .filter_map(|payload| {
            payload
                .get("fact_extractions")
                .and_then(|value| value.as_array())
        })
        .flatten()
        .cloned()
        .collect();
    if facts.is_empty() {
        return parsed_runtime.clone();
    }
    let mut merged = parsed_runtime.clone();
    let Some(root) = merged.as_object_mut() else {
        return parsed_runtime.clone();
    };
    let entry = root
        .entry("fact_extractions".to_string())
        .or_insert_with(|| serde_json::Value::Array(Vec::new()));
    if let Some(target) = entry.as_array_mut() {
        target.extend(facts);
    }
    merged
}

#[tauri::command]
pub async fn get_session_runtime_attributes(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<SessionRuntimeAttributesResponse, String> {
    let db = state.db.lock().await;
    state
        .services
        .runtime
        .session_orchestrator
        .get_session_runtime_attributes(db.conn(), &session_id)
}

/// 本局生成参数的解析结果（第 8 项）：每层各自配了什么 + 最终生效值。
/// 前端编辑界面靠 `session` 层做编辑，靠 `effective_*` 展示「实际用的是多少」。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionGenerationParamsResponse {
    /// 应用设置里的覆盖（空对象表示这层不覆盖）。
    pub app: crate::models::generation_params::GenerationParams,
    /// 世界包 director_config.generation_params 里的覆盖。
    pub world: crate::models::generation_params::GenerationParams,
    /// 本存档自己的覆盖，唯一可在游戏内编辑的一层。
    pub session: crate::models::generation_params::GenerationParams,
    /// 导演调用最终生效的参数（含内置默认 temperature 0.7）。
    pub effective_director: crate::models::generation_params::GenerationParams,
    /// 角色调用最终生效的参数（含内置默认 temperature 0.8）。
    pub effective_character: crate::models::generation_params::GenerationParams,
}

/// 读本局的三级生成参数与最终生效值。
#[tauri::command]
pub async fn get_session_generation_params(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<SessionGenerationParamsResponse, String> {
    let db = state.db.lock().await;
    let conn = db.conn();
    let session = crate::db::repositories::session_repo::SessionRepository::new(conn)
        .get(&session_id)?
        .ok_or_else(|| "Session not found".to_string())?;
    let world = crate::services::game_engine::orchestrator::resolve_world_for_session(conn, &session)?;
    let settings = crate::commands::settings::load_app_settings(conn)?;
    Ok(build_session_generation_params_response(
        &settings, &world, &session,
    ))
}

/// 写本局的会话级生成参数（三级覆盖里优先级最高的一层）。
/// 只动会话层：应用层在设置页改，世界层在世界编辑器改。
#[tauri::command]
pub async fn update_session_generation_params(
    state: State<'_, AppState>,
    session_id: String,
    params: crate::models::generation_params::GenerationParams,
) -> Result<SessionGenerationParamsResponse, String> {
    let db = state.db.lock().await;
    let conn = db.conn();
    let repo = crate::db::repositories::session_repo::SessionRepository::new(conn);
    let mut session = repo
        .get(&session_id)?
        .ok_or_else(|| "Session not found".to_string())?;
    // 入库前夹到合法区间，与应用/世界两层一致。
    session.generation_params = params.sanitized().0;
    repo.upsert(&session)?;
    let world = crate::services::game_engine::orchestrator::resolve_world_for_session(conn, &session)?;
    let settings = crate::commands::settings::load_app_settings(conn)?;
    Ok(build_session_generation_params_response(
        &settings, &world, &session,
    ))
}

fn build_session_generation_params_response(
    settings: &crate::models::settings::AppSettings,
    world: &crate::models::world::WorldDefinition,
    session: &SessionSnapshot,
) -> SessionGenerationParamsResponse {
    use crate::models::generation_params::{
        GENERATION_ROLE_CHARACTER, GENERATION_ROLE_DIRECTOR,
    };
    use crate::services::game_engine::orchestrator::world_generation_params;
    let resolve = |role: &str| {
        // 这里不带模型连接配置，max_tokens 未配就留空（真实回合会用所选模型的上限兜底）。
        crate::models::generation_params::GenerationParams::resolve_for_role(
            role,
            &settings.generation_params,
            world_generation_params(world).as_ref(),
            (!session.generation_params.is_empty()).then_some(&session.generation_params),
        )
    };
    SessionGenerationParamsResponse {
        app: settings.generation_params.clone(),
        world: world_generation_params(world).unwrap_or_default(),
        session: session.generation_params.clone(),
        effective_director: resolve(GENERATION_ROLE_DIRECTOR),
        effective_character: resolve(GENERATION_ROLE_CHARACTER),
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EditSessionMessageRequest {
    pub session_id: String,
    pub message_id: String,
    pub content: String,
}

/// 按稳定消息 ID 编辑一条消息的内容。消息寻址：session_id + message_id。
#[tauri::command]
pub async fn edit_session_message(
    state: State<'_, AppState>,
    request: EditSessionMessageRequest,
) -> Result<bool, String> {
    let db = state.db.lock().await;
    let conn = db.conn();
    let repo = crate::db::repositories::session_repo::SessionRepository::new(conn);
    let Some(mut session) = repo.get(&request.session_id)? else {
        return Ok(false);
    };
    if !session.edit_message_content(&request.message_id, MessageContent::Text(request.content)) {
        return Ok(false);
    }
    repo.upsert(&session)?;
    Ok(true)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AnswerInteractionRequest {
    pub session_id: String,
    pub message_id: String,
    pub interaction_id: String,
    pub answer: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AnswerInteractionResponse {
    pub session: SessionSnapshot,
    pub answer: serde_json::Value,
    /// false 表示该交互此前已被回答，本次返回的是首次结果（幂等，不覆盖）。
    pub newly_answered: bool,
}

/// 玩家回答消息中的交互（第 5 项）。幂等：重复提交返回首次结果，不重复生效。
#[tauri::command]
pub async fn answer_interaction(
    state: State<'_, AppState>,
    request: AnswerInteractionRequest,
) -> Result<AnswerInteractionResponse, String> {
    use crate::models::interaction::{
        validate_interaction_answer, MessageInteraction, INTERACTION_STATUS_ANSWERED,
    };
    let db = state.db.lock().await;
    let conn = db.conn();
    let repo = crate::db::repositories::session_repo::SessionRepository::new(conn);
    let mut session = repo
        .get(&request.session_id)?
        .ok_or_else(|| "会话不存在".to_string())?;
    let message = session
        .messages
        .iter_mut()
        .find(|message| message.message_id == request.message_id)
        .ok_or_else(|| "消息不存在".to_string())?;
    let metadata = message
        .metadata
        .as_mut()
        .ok_or_else(|| "该消息没有交互".to_string())?;
    let mut interaction: MessageInteraction = serde_json::from_value(
        metadata
            .get("interaction")
            .cloned()
            .ok_or_else(|| "该消息没有交互".to_string())?,
    )
    .map_err(|error| format!("交互数据损坏: {error}"))?;
    if interaction.interaction_id != request.interaction_id {
        return Err("交互 id 不匹配".to_string());
    }
    if interaction.status == INTERACTION_STATUS_ANSWERED {
        return Ok(AnswerInteractionResponse {
            session,
            answer: interaction.answer.unwrap_or(serde_json::Value::Null),
            newly_answered: false,
        });
    }
    let answer = validate_interaction_answer(&interaction, &request.answer)?;
    interaction.status = INTERACTION_STATUS_ANSWERED.to_string();
    interaction.answer = Some(answer.clone());
    interaction.answered_at = Some(chrono::Utc::now().to_rfc3339());
    metadata["interaction"] =
        serde_json::to_value(&interaction).map_err(|error| error.to_string())?;
    repo.upsert(&session)?;
    Ok(AnswerInteractionResponse {
        session,
        answer,
        newly_answered: true,
    })
}

/// 重新生成最新回合（第 2 项）：回滚到该回合的回合前状态，
/// 用同一玩家输入重跑标准回合流程。旧回合消息归档在 turn_journal 可追溯。
#[tauri::command]
pub async fn regenerate_last_turn(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<SessionSnapshot, String> {
    let _mutation_permit = state.session_mutations.try_acquire(&session_id)?;
    let (turn_index, player_input) = {
        let db = state.db.lock().await;
        let conn = db.conn();
        let turn_index =
            crate::services::game_engine::orchestrator::writeback::latest_finished_turn(conn, &session_id)?
                .ok_or_else(|| "没有已完成的回合可以重新生成".to_string())?;
        let player_input =
            crate::services::game_engine::orchestrator::writeback::turn_player_input(
                conn,
                &session_id,
                turn_index,
            )?
            .ok_or_else(|| "找不到该回合的玩家输入，无法重新生成".to_string())?;
        (turn_index, player_input)
    };
    submit_player_action_inner(
        app,
        state,
        session_id,
        PlayerActionRequest {
            content: MessageContent::Text(player_input),
            action_mode: PlayerActionMode::Resend,
            resend_from_turn_index: Some(turn_index),
        },
    )
    .await
}

/// 取消 session 中 turn_index >= from_turn_index 回合调度的未触发通知。
pub(crate) fn cancel_notifications_for_turns(
    conn: &rusqlite::Connection,
    app: &AppHandle,
    session_id: &str,
    from_turn_index: i32,
) -> Result<(), String> {
    let repo =
        crate::db::repositories::scheduled_notification_repo::ScheduledNotificationRepository::new(
            conn,
        );
    let notifications = repo.list_for_session(session_id, Some("scheduled"), 500)?;
    for notification in notifications {
        let created_in_turn = notification
            .metadata
            .get("turn_index")
            .and_then(|value| value.as_i64());
        if created_in_turn
            .map(|turn| turn >= from_turn_index as i64)
            .unwrap_or(false)
        {
            let _ = NotificationScheduler::cancel_delivery(app, &notification);
            let _ = repo.cancel(&notification.id, "turn_regenerated");
        }
    }
    Ok(())
}

fn finalize_turn_snapshot(
    app: &AppHandle,
    data_dir: &std::path::Path,
    session_orchestrator: &crate::services::game_engine::orchestrator::SessionOrchestrator,
    conn: &Connection,
    director_service: &crate::services::game_engine::director::WorldDirectorService,
    recovery_journal: &[serde_json::Value],
    session_id: &str,
    turn_index: i32,
    runtime_application: &crate::services::game_engine::runtime_effects::DirectorRuntimeApplication,
    updated: &SessionSnapshot,
    session: &SessionSnapshot,
    world: &crate::models::world::WorldDefinition,
    characters: &[crate::models::character::CharacterDefinition],
    director_runtime: &serde_json::Value,
    planned_speakers: &[String],
    scene_visible_characters: &Option<Vec<String>>,
    director_loop_traces: &[crate::services::game_engine::director::DirectorLoopIterationTrace],
    director_provider: &str,
    director_model: &crate::models::model_config::ModelConfig,
    player_input: &str,
    director_tool_loop_limit: usize,
) -> Result<(), String> {
    session_orchestrator.writeback_turn_snapshot(
        crate::services::game_engine::orchestrator::TurnWritebackInput {
            conn,
            director_service,
            recovery_journal,
            session_id,
            turn_index,
            runtime_application,
            updated,
            session,
            world,
            characters,
            director_runtime,
            planned_speakers,
            scene_visible_characters,
            director_loop_traces,
            director_provider,
            director_model,
            player_input,
            director_tool_loop_limit,
        },
    )?;
    schedule_pending_notifications(
        app,
        data_dir,
        conn,
        recovery_journal,
        session_id,
        turn_index,
        runtime_application,
        world,
    )?;
    SessionEventEmitter::emit_snapshot_logged(app, session_id, updated);
    Ok(())
}

fn schedule_pending_notifications(
    app: &AppHandle,
    data_dir: &std::path::Path,
    conn: &Connection,
    recovery_journal: &[serde_json::Value],
    session_id: &str,
    turn_index: i32,
    runtime_application: &crate::services::game_engine::runtime_effects::DirectorRuntimeApplication,
    world: &crate::models::world::WorldDefinition,
) -> Result<(), String> {
    if runtime_application.pending_notifications.is_empty()
        || crate::services::game_engine::orchestrator::writeback::journal_has_completed_step(
            recovery_journal,
            "notifications_scheduled",
        )
    {
        return Ok(());
    }

    let mut scheduled = Vec::new();
    let mut failed = Vec::new();
    for pending in &runtime_application.pending_notifications {
        let result = NotificationScheduler::schedule_tool_notification(
            conn,
            app,
            data_dir,
            NotificationToolInput {
                session_id,
                world_name: &world.name,
                source: &pending.source,
                title: Some(&pending.title),
                content: &pending.body,
                requested_time: &pending.scheduled_at,
                metadata: serde_json::json!({
                    "tool_call_id": pending.tool_call_id,
                    "requested_time": pending.requested_time,
                    "arguments": pending.arguments,
                    "turn_index": turn_index,
                    "world_id": world.id,
                }),
            },
        );
        match result {
            Ok(notification) => scheduled.push(serde_json::json!({
                "id": notification.id,
                "source": notification.source,
                "scheduled_at": notification.scheduled_at,
                "title": notification.title,
            })),
            Err(error) => failed.push(serde_json::json!({
                "source": pending.source,
                "scheduled_at": pending.scheduled_at,
                "error": error,
            })),
        }
    }

    let scheduled_count = scheduled.len();
    let failed_count = failed.len();
    crate::services::game_engine::orchestrator::writeback::append_turn_journal(
        conn,
        session_id,
        turn_index,
        "notifications_scheduled",
        "completed",
        serde_json::json!({
            "scheduled_count": scheduled_count,
            "failed_count": failed_count,
            "scheduled": scheduled,
            "failed": failed,
        }),
    )?;
    if scheduled_count > 0 {
        crate::services::notifications::sync_session_schedule_attribute(conn, session_id)?;
    }
    Ok(())
}

fn build_progress_snapshot(
    session: &SessionSnapshot,
    runtime_preparation: &crate::services::game_engine::orchestrator::DirectorRuntimePreparation,
    progress: &crate::services::game_engine::orchestrator::SpeakerTurnProgress,
    turn_index: i32,
    base_message_count: usize,
) -> SessionSnapshot {
    let split_index = progress.messages.len().min(base_message_count);
    let mut messages = progress.messages[..split_index].to_vec();
    messages.extend(runtime_preparation.pre_runtime_system_messages.clone());

    let mut speaker_messages = progress.messages[split_index..].to_vec();
    normalize_progress_messages(
        &mut speaker_messages,
        turn_index,
        progress.is_placeholder,
        progress.is_error,
    );
    messages.extend(speaker_messages);

    let current_speaker = messages
        .iter()
        .rev()
        .find(|message| message.role == "agent")
        .and_then(|message| message.speaker.clone())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| progress.speaker_name.clone());

    let mut assets = session.assets.clone();
    assets.background_hint = runtime_preparation.next_scene_background_hint.clone();
    assets.active_speaker_portrait = current_speaker.clone();
    assets.active_speaker_portrait_path = None;
    assets.active_speaker_generation_prompt.clear();

    SessionSnapshot {
        id: session.id.clone(),
        world_name: session.world_name.clone(),
        location: runtime_preparation.next_location.clone(),
        time_label: runtime_preparation.next_time_label.clone(),
        current_speaker,
        current_line: progress
            .narration
            .clone()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| runtime_preparation.current_line.clone())
            .unwrap_or_else(|| session.current_line.clone()),
        player_character_id: session.player_character_id.clone(),
        player_character_name: session.player_character_name.clone(),
        visible_characters: runtime_preparation.visible_chars.clone(),
        messages,
        player_stats: session.player_stats.clone(),
        map_graph_nodes: session.map_graph_nodes.clone(),
        map_graph_edges: session.map_graph_edges.clone(),
        inventory_items: session.inventory_items.clone(),
        system_log: session.system_log.clone(),
        scene: crate::models::session::SceneRuntime {
            scene_id: slugify_progress_scene_id(&runtime_preparation.next_scene_name),
            name: runtime_preparation.next_scene_name.clone(),
            background_hint: runtime_preparation.next_scene_background_hint.clone(),
            temporary_tags: session.scene.temporary_tags.clone(),
            present_characters: build_progress_present_characters(
                &runtime_preparation.visible_chars,
                &session.player_character_name,
            ),
        },
        assets,
        state: session.state.clone(),
        generation_params: session.generation_params.clone(),
    }
}

fn build_director_progress_snapshot(
    session: &SessionSnapshot,
    base_messages: &[crate::models::session::ChatMessage],
    director_trace_message: crate::models::session::ChatMessage,
) -> SessionSnapshot {
    let mut messages = base_messages.to_vec();
    messages.push(director_trace_message);
    let current_speaker = messages
        .iter()
        .rev()
        .find(|message| message.role == "agent")
        .and_then(|message| message.speaker.clone())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| session.current_speaker.clone());

    SessionSnapshot {
        id: session.id.clone(),
        world_name: session.world_name.clone(),
        location: session.location.clone(),
        time_label: session.time_label.clone(),
        current_speaker,
        current_line: session.current_line.clone(),
        player_character_id: session.player_character_id.clone(),
        player_character_name: session.player_character_name.clone(),
        visible_characters: session.visible_characters.clone(),
        messages,
        player_stats: session.player_stats.clone(),
        map_graph_nodes: session.map_graph_nodes.clone(),
        map_graph_edges: session.map_graph_edges.clone(),
        inventory_items: session.inventory_items.clone(),
        system_log: session.system_log.clone(),
        scene: session.scene.clone(),
        assets: session.assets.clone(),
        state: session.state.clone(),
        generation_params: session.generation_params.clone(),
    }
}

fn build_agent_chat_progress_snapshot(
    session: &SessionSnapshot,
    target: &crate::services::game_engine::orchestrator::AgentChatTarget,
    progress: &crate::services::game_engine::orchestrator::SpeakerTurnProgress,
    turn_index: i32,
    base_message_count: usize,
) -> SessionSnapshot {
    let split_index = progress.messages.len().min(base_message_count);
    let mut messages = progress.messages[..split_index].to_vec();

    let mut speaker_messages = progress.messages[split_index..].to_vec();
    normalize_progress_messages(
        &mut speaker_messages,
        turn_index,
        progress.is_placeholder,
        progress.is_error,
    );
    messages.extend(speaker_messages);

    let current_speaker = messages
        .iter()
        .rev()
        .find(|message| message.role == "agent")
        .and_then(|message| message.speaker.clone())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| target.agent.name.clone());

    let mut assets = session.assets.clone();
    assets.active_speaker_portrait = current_speaker.clone();
    assets.active_speaker_portrait_path = None;
    assets.active_speaker_generation_prompt.clear();

    SessionSnapshot {
        id: session.id.clone(),
        world_name: session.world_name.clone(),
        location: target.next_location.clone(),
        time_label: session.time_label.clone(),
        current_speaker,
        current_line: progress
            .narration
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| session.current_line.clone()),
        player_character_id: session.player_character_id.clone(),
        player_character_name: session.player_character_name.clone(),
        visible_characters: target.visible_chars.clone(),
        messages,
        player_stats: session.player_stats.clone(),
        map_graph_nodes: session.map_graph_nodes.clone(),
        map_graph_edges: session.map_graph_edges.clone(),
        inventory_items: session.inventory_items.clone(),
        system_log: session.system_log.clone(),
        scene: crate::models::session::SceneRuntime {
            scene_id: if session.scene.scene_id.trim().is_empty() {
                slugify_progress_scene_id(&target.next_scene_name)
            } else {
                session.scene.scene_id.clone()
            },
            name: target.next_scene_name.clone(),
            background_hint: session.scene.background_hint.clone(),
            temporary_tags: session.scene.temporary_tags.clone(),
            present_characters: build_progress_present_characters(
                &target.visible_chars,
                &session.player_character_name,
            ),
        },
        assets,
        state: session.state.clone(),
        generation_params: session.generation_params.clone(),
    }
}

fn normalize_progress_messages(
    messages: &mut [crate::models::session::ChatMessage],
    turn_index: i32,
    mark_last_as_streaming: bool,
    is_error: bool,
) {
    let last_index = messages.len().saturating_sub(1);
    for (index, message) in messages.iter_mut().enumerate() {
        let mut metadata = message
            .metadata
            .take()
            .unwrap_or_else(|| serde_json::json!({}));
        if !metadata.is_object() {
            metadata = serde_json::json!({});
        }
        if let Some(object) = metadata.as_object_mut() {
            object.insert("turn_index".to_string(), serde_json::json!(turn_index));
            object
                .entry("message_kind".to_string())
                .or_insert_with(|| serde_json::json!("agent_response"));
            if mark_last_as_streaming && index == last_index && !is_error {
                object.insert("streaming".to_string(), serde_json::json!(true));
            } else {
                object.remove("streaming");
            }
        }
        message.metadata = Some(metadata);
    }
}

fn build_progress_present_characters(
    visible_characters: &[String],
    player_character_name: &str,
) -> Vec<String> {
    let mut present = Vec::new();
    for name in visible_characters {
        let trimmed = name.trim();
        if trimmed.is_empty() || present.iter().any(|item| item == trimmed) {
            continue;
        }
        present.push(trimmed.to_string());
    }
    let player_name = player_character_name.trim();
    if !player_name.is_empty() && !present.iter().any(|item| item == player_name) {
        present.push(player_name.to_string());
    }
    present
}

fn slugify_progress_scene_id(value: &str) -> String {
    // L2: 单次扫描折叠连续 '-',避免反复全量 replace。
    let mut normalized = String::with_capacity(value.len());
    let mut last_was_dash = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            normalized.push(character.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            normalized.push('-');
            last_was_dash = true;
        }
    }
    let normalized = normalized.trim_matches('-');
    if normalized.is_empty() {
        "scene-switch".to_string()
    } else {
        normalized.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{merge_agent_chat_runtime_payloads, merge_speaker_fact_extractions};

    #[test]
    fn agent_memory_entries_are_aliased_into_memory_events() {
        let payloads = vec![serde_json::json!({
            "memory_entries": [
                { "content": "记住了玩家的名字", "character_names": ["林黛玉"] }
            ]
        })];
        let merged = merge_agent_chat_runtime_payloads(&payloads);
        // runtime 解析读的是 memory_events，NPC 契约用的是 memory_entries：必须被并入。
        let events = merged
            .get("memory_events")
            .and_then(|value| value.as_array())
            .expect("memory_events present");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["content"], "记住了玩家的名字");
    }

    #[test]
    fn agent_memory_events_and_entries_both_merge() {
        let payloads = vec![
            serde_json::json!({ "memory_events": [{ "content": "A", "character_names": ["甲"] }] }),
            serde_json::json!({ "memory_entries": [{ "content": "B", "character_names": ["乙"] }] }),
        ];
        let merged = merge_agent_chat_runtime_payloads(&payloads);
        let events = merged
            .get("memory_events")
            .and_then(|value| value.as_array())
            .expect("memory_events present");
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn agent_fact_extractions_merge_across_speaker_payloads() {
        let payloads = vec![
            serde_json::json!({ "fact_extractions": [{ "subject": "银钥匙", "predicate": "藏在", "object": "12号柜" }] }),
            serde_json::json!({ "fact_extractions": [{ "subject": "鲍勃", "predicate": "不知道", "object": "位置" }] }),
        ];
        let merged = merge_agent_chat_runtime_payloads(&payloads);
        let facts = merged
            .get("fact_extractions")
            .and_then(|value| value.as_array())
            .expect("fact_extractions present");
        assert_eq!(facts.len(), 2);
    }

    #[test]
    fn speaker_fact_extractions_merge_into_director_payload() {
        let director_payload = serde_json::json!({
            "planned_speakers": ["林黛玉"],
            "fact_extractions": [{ "subject": "导演事实", "predicate": "发生", "object": "雷雨" }]
        });
        let speaker_payloads = vec![
            serde_json::json!({ "fact_extractions": [{ "subject": "银钥匙", "predicate": "藏在", "object": "12号柜" }] }),
            serde_json::json!({ "content": "没有事实的一轮" }),
        ];
        let merged = merge_speaker_fact_extractions(&director_payload, &speaker_payloads);
        let facts = merged
            .get("fact_extractions")
            .and_then(|value| value.as_array())
            .expect("fact_extractions present");
        assert_eq!(facts.len(), 2, "导演已有的事实在前,说话人的追加在后");
        assert_eq!(facts[0]["subject"], "导演事实");
        assert_eq!(facts[1]["subject"], "银钥匙");
        // 其它字段原样保留
        assert_eq!(
            merged.get("planned_speakers"),
            Some(&serde_json::json!(["林黛玉"]))
        );
    }

    #[test]
    fn speaker_fact_extractions_noop_when_nothing_to_merge() {
        let director_payload = serde_json::json!({ "planned_speakers": ["林黛玉"] });
        let merged = merge_speaker_fact_extractions(&director_payload, &[]);
        assert!(merged.get("fact_extractions").is_none());
        // 没有事实时不应新建字段
        assert_eq!(merged, director_payload);
    }
}
