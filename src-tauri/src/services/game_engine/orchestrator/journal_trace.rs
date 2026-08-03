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

use super::run::*;
use super::writeback::{append_turn_journal, journal_has_completed_step};

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
            "pending_notifications_count": runtime_application.pending_notifications.len(),
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
            "reasoning_expanded": reasoning_expanded,
            "world_phase": runtime_payload.world_phase,
            "next_scene_name": runtime_payload.next_scene_name,
            "next_location": runtime_payload.next_location,
            "next_time_label": runtime_payload.next_time_label,
            "planned_speakers": runtime_payload.planned_speakers,
        })),
    }
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
        // 请求上带的就是本回合三级覆盖解析后的参数，直接复用，不重算。
        &trace.request.generation,
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
