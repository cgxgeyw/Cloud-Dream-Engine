use crate::models::attribute::{AttributeSchema, AttributeValue, AttributeValueUpsertRequest};
use crate::models::character::CharacterDefinition;
use crate::models::model_config::ModelConfig;
use crate::models::session::*;
use crate::models::world::WorldDefinition;
use crate::services::game_engine::director::{
    DirectorLoopIterationTrace, ParsedDirectorRuntimePayload, WorldDirectorService,
};
use crate::services::game_engine::runtime_effects::DirectorRuntimeApplication;
use chrono::Utc;
use rusqlite::{params, Connection};
use std::collections::HashMap;

use super::run::*;
use super::turn_context::*;

pub(crate) fn build_runtime_updated_session_snapshot(
    input: &RuntimeMutationInput<'_>,
) -> SessionSnapshot {
    let resolved_scene_runtime = input
        .runtime_application
        .scene_runtime
        .clone()
        .unwrap_or_else(|| {
            let explicit_visible = if input.scene_visible_characters_explicit {
                input
                    .scene_visible_characters
                    .as_ref()
                    .cloned()
                    .unwrap_or_else(|| input.visible_chars.to_vec())
            } else {
                input.visible_chars.to_vec()
            };
            SceneRuntime {
                scene_id: slugify_scene_id(input.next_scene_name),
                name: input.next_scene_name.to_string(),
                background_hint: input.next_scene_background_hint.clone(),
                temporary_tags: input.runtime_application.scene_tags.clone(),
                present_characters: build_turn_participants(
                    &explicit_visible,
                    &input.session.player_character_name,
                ),
            }
        });
    let mut messages = input.messages.to_vec();
    messages.extend(
        input
            .runtime_application
            .system_messages
            .iter()
            .filter(|message| should_persist_session_message(message))
            .cloned(),
    );
    let latest_agent_message = messages
        .iter()
        .rev()
        .find(|message| message.role == "agent" && !message.content.trim().is_empty());
    let latest_agent_narration = messages
        .iter()
        .rev()
        .filter(|message| message.role == "agent")
        .filter_map(|message| {
            message
                .metadata
                .as_ref()
                .and_then(|meta| meta.get("narration"))
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
        .next();
    let current_line = latest_agent_narration
        .or_else(|| {
            input
                .current_line
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
        .or_else(|| {
            let previous = input.session.current_line.trim();
            if previous.is_empty() {
                None
            } else {
                Some(previous.to_string())
            }
        })
        .unwrap_or_default();
    SessionSnapshot {
        id: input.session.id.clone(),
        world_name: input.session.world_name.clone(),
        location: input.next_location.to_string(),
        time_label: input.next_time_label.to_string(),
        current_speaker: latest_agent_message
            .and_then(|message| message.speaker.clone())
            .unwrap_or_default(),
        current_line,
        player_character_id: input.session.player_character_id.clone(),
        player_character_name: input.session.player_character_name.clone(),
        visible_characters: input.visible_chars.to_vec(),
        messages,
        player_stats: input
            .runtime_application
            .player_stats
            .clone()
            .unwrap_or_else(|| input.session.player_stats.clone()),
        map_graph_nodes: input.session.map_graph_nodes.clone(),
        map_graph_edges: input.session.map_graph_edges.clone(),
        inventory_items: input
            .runtime_application
            .inventory_items
            .clone()
            .unwrap_or_else(|| input.session.inventory_items.clone()),
        system_log: append_system_log(
            &merge_system_log_lines(
                &merge_system_log_lines(
                    &input.session.system_log,
                    &input.runtime_application.system_log_lines,
                ),
                &input.runtime_application.tool_call_logs,
            ),
            input.turn_index,
            input.next_scene_name,
            input.next_location,
            input.next_time_label,
            input.planned_speakers,
        ),
        scene: SceneRuntime {
            scene_id: resolved_scene_runtime.scene_id,
            name: resolved_scene_runtime.name,
            background_hint: resolved_scene_runtime.background_hint,
            temporary_tags: resolved_scene_runtime.temporary_tags,
            present_characters: resolved_scene_runtime.present_characters,
        },
        assets: input.session.assets.clone(),
        state: SessionState {
            metrics: input.runtime_application.state_metrics.clone(),
            tags: input.runtime_application.state_tags.clone(),
            phase: if input.runtime_application.state_phase.trim().is_empty() {
                input.session.state.phase.clone()
            } else {
                input.runtime_application.state_phase.clone()
            },
        },
    }
}

pub(crate) fn append_runtime_effects_journal(
    conn: &Connection,
    recovery_journal: &[serde_json::Value],
    session_id: &str,
    turn_index: i32,
    runtime_application: &DirectorRuntimeApplication,
    updated: &SessionSnapshot,
) -> Result<(), String> {
    if journal_has_completed_step(recovery_journal, "runtime_effects_applied") {
        return Ok(());
    }
    append_turn_journal(
        conn,
        session_id,
        turn_index,
        "runtime_effects_applied",
        "completed",
        serde_json::json!({
            "state_phase": runtime_application.state_phase,
            "state_tags_count": runtime_application.state_tags.len(),
            "state_metric_keys": runtime_application.state_metrics.keys().cloned().collect::<Vec<_>>(),
            "inventory_items_count": runtime_application
                .inventory_items
                .as_ref()
                .map(|items| items.len())
                .unwrap_or(updated.inventory_items.len()),
            "system_messages_count": runtime_application.system_messages.len(),
            "system_log_lines_count": runtime_application.system_log_lines.len(),
            "tool_call_logs_count": runtime_application.tool_call_logs.len(),
            "session_attribute_updates_count": runtime_application.session_attribute_updates.len(),
            "character_attribute_updates_count": runtime_application.character_attribute_updates.len(),
            "memory_entries_count": runtime_application.memory_entries.len(),
        }),
    )?;
    Ok(())
}

pub(crate) fn persist_director_traces(
    conn: &Connection,
    director_service: &WorldDirectorService,
    session_id: &str,
    turn_index: i32,
    director_loop_traces: &[DirectorLoopIterationTrace],
    world: &WorldDefinition,
    session: &SessionSnapshot,
    characters: &[CharacterDefinition],
    director_provider: &str,
    director_model: &ModelConfig,
    player_input: &str,
    director_tool_loop_limit: usize,
) -> Result<(), String> {
    for trace in director_loop_traces {
        let step = if trace.iteration == director_loop_traces.len() {
            "director_decision".to_string()
        } else {
            format!("director_tool_phase_{}", trace.iteration)
        };
        let prompt_trace = build_director_prompt_trace(
            director_service,
            trace,
            world,
            session,
            characters,
            director_provider,
            director_model,
            player_input,
            director_tool_loop_limit,
        );
        record_prompt_call(
            conn,
            session_id,
            turn_index,
            "director",
            &step,
            "world_director",
            prompt_trace,
        )?;
        record_llm_call(
            conn,
            session_id,
            turn_index,
            &step,
            "world_director",
            trace.request_value.clone(),
            trace.response_value.clone(),
        )?;
    }
    Ok(())
}

pub(crate) fn append_post_update_journals(
    conn: &Connection,
    director_service: &WorldDirectorService,
    recovery_journal: &[serde_json::Value],
    session_id: &str,
    turn_index: i32,
    updated: &SessionSnapshot,
    director_runtime: &serde_json::Value,
    planned_speakers: &[String],
    scene_visible_characters: &Option<Vec<String>>,
    last_director_trace: Option<&DirectorLoopIterationTrace>,
    world: &WorldDefinition,
    session: &SessionSnapshot,
    characters: &[CharacterDefinition],
    director_provider: &str,
    director_model: &ModelConfig,
    player_input: &str,
    director_tool_loop_limit: usize,
) -> Result<(), String> {
    if !journal_has_completed_step(recovery_journal, "director_completed") {
        append_turn_journal(
            conn,
            session_id,
            turn_index,
            "director_completed",
            "completed",
            serde_json::json!({
                "next_location": updated.location,
                "next_scene_name": updated.scene.name,
                "next_time_label": updated.time_label,
                "world_phase": director_runtime
                    .get("world_phase")
                    .cloned()
                    .unwrap_or_else(|| serde_json::Value::String(updated.state.phase.clone())),
                "next_scene_background_hint": director_runtime
                    .get("next_scene_background_hint")
                    .cloned()
                    .unwrap_or_else(|| serde_json::Value::String(updated.scene.background_hint.clone())),
                "background_asset_name": director_runtime
                    .get("background_asset_name")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
                "background_asset_path": director_runtime
                    .get("background_asset_path")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
                "background_generation_prompt": director_runtime
                    .get("background_generation_prompt")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
                "next_scene_tags": director_runtime
                    .get("next_scene_tags")
                    .cloned()
                    .unwrap_or_else(|| serde_json::Value::Array(
                        updated
                            .scene
                            .temporary_tags
                            .iter()
                            .cloned()
                            .map(serde_json::Value::String)
                            .collect()
                    )),
                "planned_speakers": planned_speakers,
                "scene_visible_characters": scene_visible_characters,
                "character_visual_directives": director_runtime
                    .get("character_visual_directives")
                    .cloned()
                    .unwrap_or_else(|| serde_json::Value::Array(vec![])),
                "switch_character_proposal": director_runtime
                    .get("switch_character_proposal")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
                "generated_characters": director_runtime
                    .get("generated_characters")
                    .cloned()
                    .unwrap_or_else(|| serde_json::Value::Array(vec![])),
                "director_runtime": director_runtime.clone(),
                "prompt_trace": last_director_trace.map(|trace| {
                    build_director_prompt_trace(
                        director_service,
                        trace,
                        world,
                        session,
                        characters,
                        director_provider,
                        director_model,
                        player_input,
                        director_tool_loop_limit,
                    )
                }).unwrap_or(serde_json::Value::Null)
            }),
        )?;
    }
    if !journal_has_completed_step(recovery_journal, "scene_applied") {
        append_turn_journal(
            conn,
            session_id,
            turn_index,
            "scene_applied",
            "completed",
            serde_json::json!({
                "scene_id": updated.scene.scene_id,
                "scene_name": updated.scene.name,
                "location": updated.location,
                "time_label": updated.time_label,
                "background_hint": updated.scene.background_hint,
                "scene_tags": updated.scene.temporary_tags,
                "visible_characters": updated.visible_characters,
                "present_characters": updated.scene.present_characters,
                "planned_speakers": planned_speakers,
                "scene_visible_characters": scene_visible_characters,
                "current_speaker": updated.current_speaker,
                "current_line": updated.current_line,
                "state_phase": updated.state.phase,
                "state_tags": updated.state.tags,
                "state_metrics": updated.state.metrics,
                "assets": updated.assets,
            }),
        )?;
    }
    Ok(())
}

pub(crate) fn build_director_trace_message(
    trace: &DirectorLoopIterationTrace,
) -> DirectorTraceMessage {
    let tool_calls = trace
        .parsed
        .get("tool_calls")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    let planned_speakers = trace
        .tool_enriched
        .get("planned_speakers")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect::<Vec<_>>();
    let next_scene_name = trace
        .tool_enriched
        .get("next_scene_name")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let next_location = trace
        .tool_enriched
        .get("next_location")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let world_phase = trace
        .tool_enriched
        .get("world_phase")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let mut trace_lines = Vec::new();
    if !world_phase.is_empty() {
        trace_lines.push(format!("阶段：{world_phase}"));
    }
    if !next_scene_name.is_empty() {
        trace_lines.push(format!("场景：{next_scene_name}"));
    }
    if !next_location.is_empty() {
        trace_lines.push(format!("地点：{next_location}"));
    }
    if !planned_speakers.is_empty() {
        trace_lines.push(format!("发言顺序：{}", planned_speakers.join(" / ")));
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
            trace_lines.push(format!("工具调用：{}", tool_names.join(" / ")));
        }
    }
    let reasoning = trace
        .response_value
        .get("response")
        .and_then(|value| value.get("reasoning"))
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let trace_text = if trace_lines.is_empty() {
        extract_model_response_text(&trace.response_value)
    } else {
        trace_lines.join("\n")
    };
    DirectorTraceMessage {
        trace_text,
        trace_lines,
        reasoning,
    }
}

pub(crate) fn build_director_trace_chat_message(
    trace_message: &DirectorTraceMessage,
    turn_index: i32,
    runtime_payload: &ParsedDirectorRuntimePayload,
    reasoning_expanded: bool,
) -> ChatMessage {
    ChatMessage {
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
            "reasoning_expanded": reasoning_expanded,
            "world_phase": runtime_payload.world_phase,
            "next_scene_name": runtime_payload.next_scene_name,
            "next_location": runtime_payload.next_location,
            "next_time_label": runtime_payload.next_time_label,
            "planned_speakers": runtime_payload.planned_speakers,
        })),
    }
}

pub(crate) fn materialize_completed_speaker_messages(
    recovery_journal: &[serde_json::Value],
    turn_index: i32,
) -> Vec<ChatMessage> {
    let mut steps = recovery_journal
        .iter()
        .filter_map(|entry| {
            let step = entry.get("step").and_then(|value| value.as_str())?;
            let status = entry.get("status").and_then(|value| value.as_str())?;
            if status != "completed"
                || !step.starts_with("speaker_")
                || !step.ends_with("_completed")
            {
                return None;
            }
            let payload = entry.get("payload")?;
            let llm_output = payload.get("llm_output")?;
            let speaker = llm_output
                .get("speaker")
                .and_then(|value| value.as_str())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())?;
            let content = llm_output
                .get("content")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string())
                .unwrap_or_default();
            let order = step
                .trim_start_matches("speaker_")
                .trim_end_matches("_completed")
                .parse::<i32>()
                .unwrap_or_default();
            Some((order, speaker, content, llm_output.clone()))
        })
        .collect::<Vec<_>>();
    steps.sort_by(|left, right| left.0.cmp(&right.0));
    steps.into_iter()
        .map(|(_, speaker, content, llm_output)| ChatMessage {
            role: "agent".to_string(),
            content: MessageContent::Text(content),
            speaker: Some(speaker),
            metadata: Some(serde_json::json!({
                "turn_index": turn_index,
                "message_kind": "agent_response",
                "recovered": true,
                "intent": llm_output.get("intent").cloned().unwrap_or(serde_json::Value::Null),
                "emotion": llm_output.get("emotion").cloned().unwrap_or(serde_json::Value::Null),
                "narration": llm_output.get("narration").cloned().unwrap_or(serde_json::Value::Null),
                "raw_response": llm_output.get("raw_content").cloned().unwrap_or(serde_json::Value::Null),
            })),
        })
        .collect()
}

pub(crate) fn should_persist_session_message(message: &ChatMessage) -> bool {
    if message.role != "system" {
        return true;
    }
    let action_type = message
        .metadata
        .as_ref()
        .and_then(|meta| meta.get("action_type"))
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim();
    matches!(
        action_type,
        "director_trace" | "switch_character" | "character_created"
    )
}

pub(crate) fn extract_model_response_text(response_value: &serde_json::Value) -> String {
    response_value
        .get("response")
        .and_then(|value| value.get("content"))
        .and_then(|value| value.as_str())
        .or_else(|| {
            response_value
                .get("response")
                .and_then(|value| value.get("reasoning"))
                .and_then(|value| value.as_str())
        })
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_default()
}

pub(crate) fn append_finished_journal(
    conn: &Connection,
    recovery_journal: &[serde_json::Value],
    session_id: &str,
    turn_index: i32,
    updated: &SessionSnapshot,
) -> Result<(), String> {
    if journal_has_completed_step(recovery_journal, "finished") {
        return Ok(());
    }
    append_turn_journal(
        conn,
        session_id,
        turn_index,
        "finished",
        "completed",
        serde_json::json!({
            "current_speaker": updated.current_speaker,
            "current_line": updated.current_line,
            "message_count": updated.messages.len(),
        }),
    )?;
    Ok(())
}

pub(crate) fn build_director_prompt_trace(
    director_service: &WorldDirectorService,
    trace: &DirectorLoopIterationTrace,
    world: &WorldDefinition,
    session: &SessionSnapshot,
    characters: &[CharacterDefinition],
    provider: &str,
    model: &ModelConfig,
    player_input: &str,
    loop_limit: usize,
) -> serde_json::Value {
    let stage = director_service.resolve_runtime_stage_label(world, &trace.request.messages);
    director_service.build_prompt_trace(
        &trace.request.messages,
        &trace.request_value,
        &trace.response_value,
        &trace.parsed,
        &trace.tool_enriched,
        trace.iteration,
        world,
        session,
        characters,
        provider,
        model,
        player_input,
        loop_limit,
        &stage,
    )
}

pub(crate) fn record_prompt_call(
    conn: &Connection,
    session_id: &str,
    turn_index: i32,
    recipient_type: &str,
    step: &str,
    recipient_name: &str,
    prompt_call: serde_json::Value,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO prompt_call_traces (id, session_id, turn_index, step, recipient_type, recipient_name, prompt_call_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            uuid::Uuid::new_v4().to_string(),
            session_id,
            turn_index,
            step,
            recipient_type,
            recipient_name,
            serde_json::to_string(&prompt_call).map_err(|e| e.to_string())?,
            Utc::now().to_rfc3339(),
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) fn extract_trace_string(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|item| item.as_str())
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

pub(crate) fn extract_trace_i64(value: &serde_json::Value, key: &str) -> Option<i64> {
    value.get(key).and_then(|item| item.as_i64())
}

pub(crate) fn record_llm_call(
    conn: &Connection,
    session_id: &str,
    turn_index: i32,
    step: &str,
    speaker: &str,
    input_payload: serde_json::Value,
    output_payload: serde_json::Value,
) -> Result<(), String> {
    let provider = extract_trace_string(&output_payload, "provider")
        .or_else(|| extract_trace_string(&input_payload, "provider"))
        .unwrap_or_default();
    let model_id = extract_trace_string(&output_payload, "model_id")
        .or_else(|| extract_trace_string(&input_payload, "model_id"))
        .unwrap_or_default();
    let status =
        extract_trace_string(&output_payload, "status").unwrap_or_else(|| "completed".to_string());
    let latency_ms = extract_trace_i64(&output_payload, "latency_ms").unwrap_or_default();
    conn.execute(
        "INSERT INTO llm_call_traces (id, session_id, turn_index, step, speaker, provider, model_id, status, latency_ms, input_payload_json, output_payload_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            uuid::Uuid::new_v4().to_string(),
            session_id,
            turn_index,
            step,
            speaker,
            provider,
            model_id,
            status,
            latency_ms,
            serde_json::to_string(&input_payload).map_err(|e| e.to_string())?,
            serde_json::to_string(&output_payload).map_err(|e| e.to_string())?,
            Utc::now().to_rfc3339(),
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) fn record_agent_checkpoint(
    conn: &Connection,
    session_id: &str,
    runtime_key: &str,
    turn_index: i32,
    checkpoint_type: &str,
    payload: serde_json::Value,
) -> Result<(), String> {
    let mut stmt = conn
        .prepare("SELECT id FROM agent_sessions WHERE session_id = ?1 AND runtime_key = ?2 LIMIT 1")
        .map_err(|e| e.to_string())?;
    let mut rows = stmt
        .query_map(params![session_id, runtime_key], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|e| e.to_string())?;
    let Some(agent_session_id) = rows.next().transpose().map_err(|e| e.to_string())? else {
        return Ok(());
    };
    let checkpoint_id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO agent_checkpoints (id, agent_session_id, turn_index, checkpoint_type, payload_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            checkpoint_id,
            agent_session_id,
            turn_index,
            checkpoint_type,
            serde_json::to_string(&payload).map_err(|e| e.to_string())?,
            Utc::now().to_rfc3339(),
        ],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE agent_sessions SET checkpoint_id = ?1, updated_at = ?2, last_active_turn = ?3 WHERE id = ?4",
        params![checkpoint_id, Utc::now().to_rfc3339(), turn_index, agent_session_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) fn has_agent_checkpoint(
    conn: &Connection,
    session_id: &str,
    runtime_key: &str,
    turn_index: i32,
    checkpoint_type: &str,
) -> Result<bool, String> {
    let mut stmt = conn
        .prepare(
            "SELECT 1
             FROM agent_checkpoints checkpoints
             INNER JOIN agent_sessions sessions ON sessions.id = checkpoints.agent_session_id
             WHERE sessions.session_id = ?1
               AND sessions.runtime_key = ?2
               AND checkpoints.turn_index = ?3
               AND checkpoints.checkpoint_type = ?4
             LIMIT 1",
        )
        .map_err(|e| e.to_string())?;
    let mut rows = stmt
        .query(params![
            session_id,
            runtime_key,
            turn_index,
            checkpoint_type
        ])
        .map_err(|e| e.to_string())?;
    Ok(rows.next().map_err(|e| e.to_string())?.is_some())
}

pub(crate) fn merge_system_log_lines(current: &[String], additions: &[String]) -> Vec<String> {
    let mut merged = current.to_vec();
    for line in additions {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            merged.push(trimmed.to_string());
        }
    }
    merged
}

pub(crate) fn append_system_log(
    current: &[String],
    turn_index: i32,
    scene_name: &str,
    location: &str,
    time_label: &str,
    visible_characters: &[String],
) -> Vec<String> {
    let mut next = current.to_vec();
    if !visible_characters.is_empty() {
        next.push(format!("Speaker order: {}", visible_characters.join(" / ")));
    }
    next.push(format!(
        "Turn {}: scene={}, location={}, time={}, visible={}",
        turn_index,
        scene_name,
        location,
        time_label,
        visible_characters.join(" / ")
    ));
    next
}

pub(crate) fn append_turn_journal(
    conn: &Connection,
    session_id: &str,
    turn_index: i32,
    step: &str,
    status: &str,
    payload: serde_json::Value,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO turn_journal (id, session_id, turn_index, step, status, payload_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            uuid::Uuid::new_v4().to_string(),
            session_id,
            turn_index,
            step,
            status,
            serde_json::to_string(&payload).map_err(|e| e.to_string())?,
            Utc::now().to_rfc3339(),
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) fn load_latest_turn_index(conn: &Connection, session_id: &str) -> Result<i32, String> {
    let mut stmt = conn
        .prepare("SELECT COALESCE(MAX(turn_index), 0) FROM turn_journal WHERE session_id = ?1")
        .map_err(|e| e.to_string())?;
    stmt.query_row(params![session_id], |row| row.get(0))
        .map_err(|e| e.to_string())
}

pub(crate) fn load_turn_journal(
    conn: &Connection,
    session_id: &str,
    turn_index: i32,
) -> Result<Vec<serde_json::Value>, String> {
    let mut stmt = conn
        .prepare("SELECT step, status, payload_json, created_at FROM turn_journal WHERE session_id = ?1 AND turn_index = ?2 ORDER BY created_at, id")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![session_id, turn_index], |row| {
            let payload_json: String = row.get(2)?;
            let payload =
                serde_json::from_str::<serde_json::Value>(&payload_json).unwrap_or_default();
            Ok(serde_json::json!({
                "step": row.get::<_, String>(0)?,
                "status": row.get::<_, String>(1)?,
                "payload": payload,
                "created_at": row.get::<_, String>(3)?,
            }))
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub(crate) fn journal_has_completed_step(journal: &[serde_json::Value], step: &str) -> bool {
    journal.iter().any(|entry| {
        entry.get("step").and_then(|value| value.as_str()) == Some(step)
            && entry.get("status").and_then(|value| value.as_str()) == Some("completed")
    })
}

pub(crate) fn unique_strings(values: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::<String>::new();
    let mut result = Vec::new();
    for value in values {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() || !seen.insert(trimmed.clone()) {
            continue;
        }
        result.push(trimmed);
    }
    result
}

pub(crate) fn slugify_scene_id(value: &str) -> String {
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

pub(crate) fn merge_visible_characters(
    existing: &[String],
    additions: Vec<String>,
    player_character_name: &str,
) -> Vec<String> {
    let mut merged = existing.to_vec();
    for name in additions {
        if name != player_character_name && !merged.contains(&name) {
            merged.push(name);
        }
    }
    merged
}

pub(crate) fn build_turn_participants(
    visible_character_names: &[String],
    player_character_name: &str,
) -> Vec<String> {
    let mut names = unique_strings(
        visible_character_names
            .iter()
            .cloned()
            .chain(std::iter::once(player_character_name.to_string()))
            .collect::<Vec<_>>(),
    );
    if !names.iter().any(|name| name == player_character_name) {
        names.push(player_character_name.to_string());
    }
    names
}

pub(crate) fn normalize_provider_name(provider: &str) -> String {
    match provider.trim().to_ascii_lowercase().as_str() {
        "anthropic" | "claude" | "claude / anthropic" => "anthropic".to_string(),
        "openai" | "openai-compatible" | "openai compatible" | "ollama" | "lm studio"
        | "lmstudio" => "openai".to_string(),
        _ => "openai".to_string(),
    }
}

pub(crate) fn world_allows_mcp_tool(world: &WorldDefinition, tool_id: &str) -> bool {
    world
        .director_config
        .get("allowed_mcp_tool_ids")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(|item| item.trim())
                .any(|item| item == tool_id)
        })
        .unwrap_or(false)
}

pub(crate) fn collect_runtime_attribute_values(
    conn: &Connection,
    session_id: &str,
) -> Result<Vec<AttributeValue>, String> {
    let repo = crate::db::repositories::attribute_repo::AttributeRepository::new(conn);
    let mut values = repo.list_values(Some("session"), Some(session_id), None)?;
    values.extend(
        repo.list_values(Some("session_character"), None, None)?
            .into_iter()
            .filter(|value| value.owner_id.starts_with(&(session_id.to_string() + ":"))),
    );
    Ok(values)
}

pub(crate) fn restore_runtime_attribute_values(
    conn: &Connection,
    session_id: &str,
    payload: Option<serde_json::Value>,
) -> Result<(), String> {
    conn.execute(
        "DELETE FROM attribute_values WHERE owner_type = 'session' AND owner_id = ?1",
        params![session_id],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM attribute_values WHERE owner_type = 'session_character' AND owner_id LIKE ?1",
        params![format!("{session_id}:%")],
    )
    .map_err(|e| e.to_string())?;
    let Some(payload) = payload else {
        return Ok(());
    };
    let values =
        serde_json::from_value::<Vec<AttributeValue>>(payload).map_err(|e| e.to_string())?;
    let repo = crate::db::repositories::attribute_repo::AttributeRepository::new(conn);
    for value in values {
        repo.upsert_value(&AttributeValueUpsertRequest {
            schema_id: value.schema_id,
            owner_type: value.owner_type,
            owner_id: value.owner_id,
            value: value.value,
            source: value.source,
        })?;
    }
    Ok(())
}

pub(crate) fn load_turn_snapshot_payload(
    conn: &Connection,
    session_id: &str,
    turn_index: i32,
) -> Result<Option<serde_json::Value>, String> {
    let mut stmt = conn
        .prepare("SELECT payload_json FROM turn_journal WHERE session_id = ?1 AND turn_index = ?2 AND step = 'snapshot_created' AND status = 'completed' ORDER BY created_at DESC LIMIT 1")
        .map_err(|e| e.to_string())?;
    let mut rows = stmt
        .query_map(params![session_id, turn_index], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|e| e.to_string())?;
    match rows.next() {
        Some(row) => Ok(Some(
            serde_json::from_str::<serde_json::Value>(&row.map_err(|e| e.to_string())?)
                .unwrap_or_default(),
        )),
        None => Ok(None),
    }
}

pub(crate) fn delete_turn_traces(
    conn: &Connection,
    session_id: &str,
    turn_index: i32,
) -> Result<(), String> {
    conn.execute(
        "DELETE FROM prompt_call_traces WHERE session_id = ?1 AND turn_index >= ?2",
        params![session_id, turn_index],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM llm_call_traces WHERE session_id = ?1 AND turn_index >= ?2",
        params![session_id, turn_index],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM turn_journal WHERE session_id = ?1 AND turn_index >= ?2",
        params![session_id, turn_index],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM memories WHERE session_id = ?1 AND turn_index >= ?2",
        params![session_id, turn_index],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM agent_checkpoints WHERE agent_session_id IN (SELECT id FROM agent_sessions WHERE session_id = ?1) AND turn_index >= ?2",
        params![session_id, turn_index],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) fn collect_created_character_ids_from_turns(
    conn: &Connection,
    session_id: &str,
    from_turn_index: i32,
) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare("SELECT payload_json FROM turn_journal WHERE session_id = ?1 AND turn_index >= ?2 AND step = 'characters_created' AND status = 'completed' ORDER BY turn_index, created_at")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![session_id, from_turn_index], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|e| e.to_string())?;
    let mut character_ids = Vec::new();
    for row in rows {
        let payload = serde_json::from_str::<serde_json::Value>(&row.map_err(|e| e.to_string())?)
            .unwrap_or_default();
        if let Some(items) = payload
            .get("character_ids")
            .and_then(|value| value.as_array())
        {
            for item in items {
                if let Some(value) = item.as_str() {
                    let value = value.trim().to_string();
                    if !value.is_empty() && !character_ids.contains(&value) {
                        character_ids.push(value);
                    }
                }
            }
        }
    }
    Ok(character_ids)
}

pub(crate) fn rollback_session_to_turn(
    conn: &Connection,
    session: &SessionSnapshot,
    turn_index: i32,
) -> Result<SessionSnapshot, String> {
    let snapshot_payload = load_turn_snapshot_payload(conn, &session.id, turn_index)?
        .ok_or_else(|| "Missing rollback snapshot for requested turn".to_string())?;
    let restored_session = serde_json::from_value::<SessionSnapshot>(
        snapshot_payload
            .get("session_snapshot")
            .cloned()
            .ok_or_else(|| "Missing session snapshot payload".to_string())?,
    )
    .map_err(|e| e.to_string())?;
    restore_runtime_attribute_values(
        conn,
        &session.id,
        snapshot_payload.get("attribute_values").cloned(),
    )?;
    for character_id in collect_created_character_ids_from_turns(conn, &session.id, turn_index)? {
        crate::db::repositories::character_repo::CharacterRepository::new(conn)
            .delete(&character_id)?;
    }
    delete_turn_traces(conn, &session.id, turn_index)?;
    Ok(restored_session)
}

pub(crate) fn ensure_agent_session(
    conn: &Connection,
    session_id: &str,
    agent_type: &str,
    runtime_key: &str,
    character_id: Option<&str>,
    character_name: Option<&str>,
    scene_presence_state: &str,
    turn_index: i32,
) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    let mut stmt = conn
        .prepare("SELECT id, initialized_at, created_at, checkpoint_id FROM agent_sessions WHERE session_id = ?1 AND runtime_key = ?2 LIMIT 1")
        .map_err(|e| e.to_string())?;
    let mut rows = stmt
        .query_map(params![session_id, runtime_key], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let existing = rows.next().transpose().map_err(|e| e.to_string())?;
    let (id, initialized_at, created_at, checkpoint_id) = existing.unwrap_or((
        uuid::Uuid::new_v4().to_string(),
        Some(now.clone()),
        now.clone(),
        None,
    ));
    conn.execute(
        "INSERT OR REPLACE INTO agent_sessions (id, session_id, agent_type, status, connection_state, scene_presence_state, character_id, character_name, checkpoint_id, last_active_turn, last_ack_message_index, prompt_version, runtime_key, initialized_at, created_at, updated_at) VALUES (?1, ?2, ?3, 'active', 'connected', ?4, ?5, ?6, ?7, ?8, 0, 'v1', ?9, ?10, ?11, ?12)",
        params![
            id,
            session_id,
            agent_type,
            scene_presence_state,
            character_id,
            character_name,
            checkpoint_id,
            turn_index,
            runtime_key,
            initialized_at,
            created_at,
            now,
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) fn completed_speaker_steps_from_journal(journal: &[serde_json::Value]) -> Vec<i32> {
    let mut completed = Vec::new();
    for entry in journal {
        let step = entry
            .get("step")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let status = entry
            .get("status")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        if status != "completed" || !step.starts_with("speaker_") || !step.ends_with("_completed") {
            continue;
        }
        let numeric = &step["speaker_".len()..step.len() - "_completed".len()];
        if let Ok(index) = numeric.parse::<i32>() {
            if !completed.contains(&index) {
                completed.push(index);
            }
        }
    }
    completed
}

pub(crate) fn recovered_director_payload_to_result(
    payload: &serde_json::Value,
) -> serde_json::Value {
    if let Some(runtime) = payload.get("director_runtime").cloned() {
        return runtime;
    }
    serde_json::json!({
        "world_phase": payload.get("world_phase").cloned().unwrap_or_else(|| serde_json::Value::String("opening".to_string())),
        "next_location": payload.get("next_location").cloned().unwrap_or(serde_json::Value::Null),
        "next_scene_name": payload.get("next_scene_name").cloned().unwrap_or(serde_json::Value::Null),
        "next_scene_background_hint": payload.get("next_scene_background_hint").cloned().unwrap_or(serde_json::Value::Null),
        "background_asset_name": payload.get("background_asset_name").cloned().unwrap_or(serde_json::Value::Null),
        "background_asset_path": payload.get("background_asset_path").cloned().unwrap_or(serde_json::Value::Null),
        "background_generation_prompt": payload.get("background_generation_prompt").cloned().unwrap_or(serde_json::Value::Null),
        "next_scene_tags": payload.get("next_scene_tags").cloned().unwrap_or_else(|| serde_json::Value::Array(vec![])),
        "next_time_label": payload.get("next_time_label").cloned().unwrap_or(serde_json::Value::Null),
        "scene_visible_characters": payload.get("scene_visible_characters").cloned().unwrap_or_else(|| serde_json::Value::Array(vec![])),
        "planned_speakers": payload.get("planned_speakers").cloned().unwrap_or_else(|| serde_json::Value::Array(vec![])),
        "character_visual_directives": payload.get("character_visual_directives").cloned().unwrap_or_else(|| serde_json::Value::Array(vec![])),
        "switch_character_proposal": payload.get("switch_character_proposal").cloned().unwrap_or(serde_json::Value::Null),
        "generated_characters": payload.get("generated_characters").cloned().unwrap_or_else(|| serde_json::Value::Array(vec![])),
    })
}

pub(crate) fn journal_payload(
    journal: &[serde_json::Value],
    step: &str,
) -> Option<serde_json::Value> {
    journal
        .iter()
        .find(|entry| {
            entry.get("step").and_then(|value| value.as_str()) == Some(step)
                && entry.get("status").and_then(|value| value.as_str()) == Some("completed")
        })
        .and_then(|entry| entry.get("payload"))
        .cloned()
}

pub(crate) fn build_runtime_attribute_item(
    value: &AttributeValue,
    schema_map: &HashMap<String, AttributeSchema>,
) -> Option<RuntimeAttributeItem> {
    let schema = schema_map.get(&value.schema_id)?;
    Some(RuntimeAttributeItem {
        schema_id: schema.id.clone(),
        key: schema.key.clone(),
        label: schema.label.clone(),
        value_type: schema.value_type.clone(),
        value: value.value.clone(),
        source: value.source.clone(),
        display_policy: serde_json::to_value(&schema.display_policy).unwrap_or_default(),
        influence_policy: serde_json::to_value(&schema.influence_policy).unwrap_or_default(),
    })
}

impl SessionOrchestrator {
    pub async fn switch_player_character(
        &self,
        input: SwitchPlayerCharacterInput<'_>,
    ) -> Result<SessionSnapshot, String> {
        if input.session.player_character_id == input.new_character.id {
            return Ok(input.session.clone());
        }

        let mut session = input.session.clone();
        let previous_player_name = session.player_character_name.clone();
        let proposal_present = input.proposal.is_some();
        let proposal = input.proposal.cloned().unwrap_or(SwitchCharacterProposal {
            target_character_name: None,
            reason: None,
            location: None,
            scene_name: None,
            scene_background_hint: None,
            scene_tags: vec![],
            visible_characters: vec![],
        });
        let location_override = proposal
            .location
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let scene_name_override = proposal
            .scene_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let scene_background_hint_override = proposal
            .scene_background_hint
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let scene_tags_override = proposal
            .scene_tags
            .iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>();
        let visible_override = proposal
            .visible_characters
            .iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty() && *item != input.new_character.name)
            .collect::<Vec<_>>();

        let next_location = location_override.unwrap_or_else(|| session.location.clone());
        let next_scene_name = scene_name_override
            .clone()
            .or_else(|| {
                if next_location.trim().is_empty() {
                    None
                } else {
                    Some(next_location.clone())
                }
            })
            .unwrap_or_else(|| session.scene.name.clone());
        let next_scene_id = slugify_scene_id(&next_scene_name);
        let next_background_hint =
            scene_background_hint_override.unwrap_or_else(|| session.scene.background_hint.clone());
        let next_scene_tags = if proposal_present {
            unique_strings(scene_tags_override)
        } else {
            session.scene.temporary_tags.clone()
        };

        let mut next_visible_characters = if proposal_present {
            unique_strings(visible_override)
        } else {
            session.visible_characters.clone()
        };
        next_visible_characters.retain(|item| item != &input.new_character.name);
        if !proposal_present
            && previous_player_name != input.new_character.name
            && !previous_player_name.trim().is_empty()
            && !next_visible_characters.contains(&previous_player_name)
        {
            next_visible_characters.push(previous_player_name.clone());
        }
        next_visible_characters = unique_strings(next_visible_characters);

        session.player_character_id = input.new_character.id.clone();
        session.player_character_name = input.new_character.name.clone();
        session.location = next_location.clone();
        session.current_speaker = input.new_character.name.clone();
        session.current_line = if next_location.trim().is_empty() {
            format!("{} joined", input.new_character.name)
        } else {
            format!("{} joined at {}", input.new_character.name, next_location)
        };
        session.visible_characters = next_visible_characters.clone();
        session.system_log.push(session.current_line.clone());
        session.scene = SceneRuntime {
            scene_id: next_scene_id,
            name: next_scene_name,
            background_hint: next_background_hint,
            temporary_tags: next_scene_tags,
            present_characters: build_turn_participants(
                &next_visible_characters,
                &session.player_character_name,
            ),
        };
        session.assets = input
            .asset_resolver
            .resolve(
                input.data_dir,
                &session,
                &session.scene,
                &session.current_speaker,
                Some(input.world),
                input.characters,
                input.image_model,
                None,
                world_allows_mcp_tool(input.world, "mcp-tool-image-generation"),
            )
            .await;
        Ok(session)
    }

    pub fn prepare_switch_player_character_context(
        &self,
        conn: &Connection,
        session_id: &str,
        request: &SwitchCharacterRequest,
    ) -> Result<SwitchPlayerCharacterContext, String> {
        let session = crate::db::repositories::session_repo::SessionRepository::new(conn)
            .get(session_id)?
            .ok_or_else(|| "Session not found".to_string())?;
        let world = resolve_world_for_session(conn, &session)?;
        let characters = crate::db::repositories::character_repo::CharacterRepository::new(conn)
            .list_by_world(&world.id)?;
        let new_character = characters
            .iter()
            .find(|character| character.id == request.player_character_id)
            .ok_or_else(|| "Character not found".to_string())?
            .clone();
        let settings = resolve_settings(conn)?;
        let image_model = resolve_default_image_model(conn, &settings)?;
        Ok(SwitchPlayerCharacterContext {
            session,
            world,
            characters,
            new_character,
            image_model,
        })
    }

    pub fn writeback_switch_player_character(
        &self,
        conn: &Connection,
        updated: &SessionSnapshot,
    ) -> Result<(), String> {
        let latest_turn_index = load_latest_turn_index(conn, &updated.id)?;
        let recovery_journal = load_turn_journal(conn, &updated.id, latest_turn_index)?;
        let repo = crate::db::repositories::session_repo::SessionRepository::new(conn);
        repo.upsert(updated)?;
        if !journal_has_completed_step(&recovery_journal, "switch_character_applied") {
            append_turn_journal(
                conn,
                &updated.id,
                latest_turn_index,
                "switch_character_applied",
                "completed",
                serde_json::json!({
                    "player_character_id": updated.player_character_id,
                    "player_character_name": updated.player_character_name,
                    "location": updated.location,
                    "scene_id": updated.scene.scene_id,
                    "scene_name": updated.scene.name,
                    "visible_characters": updated.visible_characters,
                }),
            )?;
        }
        Ok(())
    }

    pub async fn apply_runtime_mutations(
        &self,
        input: RuntimeMutationInput<'_>,
    ) -> SessionSnapshot {
        let updated = build_runtime_updated_session_snapshot(&input);
        let resolved_assets = self
            .resolve_runtime_assets(
                input.asset_resolver,
                input.data_dir,
                &updated,
                input.world,
                input.characters,
                input.image_model,
                input.parsed_runtime,
            )
            .await;
        SessionSnapshot {
            assets: resolved_assets,
            ..updated
        }
    }

    pub fn writeback_turn_snapshot(&self, input: TurnWritebackInput<'_>) -> Result<(), String> {
        let repo = crate::db::repositories::session_repo::SessionRepository::new(input.conn);
        repo.upsert(input.updated)?;
        append_runtime_effects_journal(
            input.conn,
            input.recovery_journal,
            input.session_id,
            input.turn_index,
            input.runtime_application,
            input.updated,
        )?;
        append_post_update_journals(
            input.conn,
            input.director_service,
            input.recovery_journal,
            input.session_id,
            input.turn_index,
            input.updated,
            input.director_runtime,
            input.planned_speakers,
            input.scene_visible_characters,
            input.director_loop_traces.last(),
            input.world,
            input.session,
            input.characters,
            input.director_provider,
            input.director_model,
            input.player_input,
            input.director_tool_loop_limit,
        )?;
        persist_director_traces(
            input.conn,
            input.director_service,
            input.session_id,
            input.turn_index,
            input.director_loop_traces,
            input.world,
            input.session,
            input.characters,
            input.director_provider,
            input.director_model,
            input.player_input,
            input.director_tool_loop_limit,
        )?;
        append_finished_journal(
            input.conn,
            input.recovery_journal,
            input.session_id,
            input.turn_index,
            input.updated,
        )?;
        if !has_agent_checkpoint(
            input.conn,
            input.session_id,
            "director",
            input.turn_index,
            "turn_state",
        )? {
            record_agent_checkpoint(
                input.conn,
                input.session_id,
                "director",
                input.turn_index,
                "turn_state",
                serde_json::json!({
                    "session_snapshot": input.updated.clone(),
                    "phase": "finished",
                }),
            )?;
        }
        Ok(())
    }
}
