use rusqlite::Connection;
use tauri::{AppHandle, State};

use crate::events::session_events::SessionEventEmitter;
use crate::models::session::*;
use crate::state::AppState;

#[tauri::command]
pub async fn get_session(
    state: State<'_, AppState>,
    id: String,
) -> Result<SessionSnapshot, String> {
    let prepared = {
        let db = state.db.lock().await;
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
        state
            .services
            .runtime
            .session_orchestrator
            .persist_session_snapshot(db.conn(), &updated)?;
        if let Some(overlay) = state
            .services
            .runtime
            .session_orchestrator
            .build_incomplete_turn_overlay(db.conn(), &updated)?
        {
            return Ok(overlay);
        }
    }
    Ok(updated)
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

    let director_service = &state.services.runtime.world_director;
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
            let _ = SessionEventEmitter::emit_snapshot(&app, &session_id, &snapshot);
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
            let _ = SessionEventEmitter::emit_snapshot(&app, &session_id, &overlay);
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
        let _ = SessionEventEmitter::emit_snapshot(&app, &session_id, &snapshot);
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
                let _ = SessionEventEmitter::emit_snapshot(&app, &session_id, &snapshot);
            };
        let db = state.db.lock().await;
        state
            .services
            .runtime
            .session_orchestrator
            .run_speaker_turns(
                db,
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
                &runtime_preparation.next_scene_name,
                &runtime_preparation.next_location,
                &runtime_preparation.visible_chars,
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
        let _ = SessionEventEmitter::emit_snapshot(&app, &session_id, &overlay);
        return Ok(overlay);
    }

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
            &runtime_preparation.parsed_runtime,
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
    let _ = SessionEventEmitter::emit_snapshot(&app, &session_id, &updated);
    Ok(updated)
}

#[tauri::command]
pub async fn resume_last_incomplete_turn(
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
        submit_player_action(app, state, session_id, player_request).await
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
    {
        let db = state.db.lock().await;
        state
            .services
            .runtime
            .session_orchestrator
            .verify_retry_capsule(db.conn(), &session_id, &request.retry_token)?;
    }
    let result = resume_last_incomplete_turn(app, state.clone(), session_id.clone()).await?;
    {
        let db = state.db.lock().await;
        state
            .services
            .runtime
            .session_orchestrator
            .consume_retry_capsule(db.conn(), &session_id, &request.retry_token)?;
    }
    Ok(result)
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

fn finalize_turn_snapshot(
    app: &AppHandle,
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
    let _ = SessionEventEmitter::emit_snapshot(app, session_id, updated);
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
    let mut normalized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    while normalized.contains("--") {
        normalized = normalized.replace("--", "-");
    }
    let normalized = normalized.trim_matches('-').to_string();
    if normalized.is_empty() {
        "scene-switch".to_string()
    } else {
        normalized
    }
}
