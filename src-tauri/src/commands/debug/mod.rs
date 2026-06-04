pub mod common;
pub mod errors;
pub mod memories;
pub mod prompts;
pub mod timeline;

use std::collections::BTreeMap;

use tauri::State;

use crate::state::AppState;

use common::{
    build_event_chain_v2, build_grouped_memories, build_runtime_attribute_item,
    build_trace_timeline, build_turn_journal_timeline, parse_completed_speaker_step,
};

#[tauri::command]
pub async fn get_debug_timeline(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<serde_json::Value, String> {
    timeline::get_debug_timeline_impl(state, session_id).await
}

#[tauri::command]
pub async fn get_debug_prompts(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<serde_json::Value, String> {
    prompts::get_debug_prompts_impl(state, session_id).await
}

#[tauri::command]
pub async fn get_debug_memories(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<serde_json::Value, String> {
    memories::get_debug_memories_impl(state, session_id).await
}

#[tauri::command]
pub async fn get_debug_errors(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<serde_json::Value, String> {
    errors::get_debug_errors_impl(state, session_id).await
}

#[tauri::command]
pub async fn get_session_debug(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<serde_json::Value, String> {
    let db = state.db.lock().await;

    let session = common::query_session(db.conn(), &session_id)?;
    let world = common::query_world(db.conn(), &session.world_name)?;
    let characters = common::query_characters(db.conn(), world.as_ref().map(|w| w.id.as_str()))?;
    let memories = common::query_memories(
        db.conn(),
        &session_id,
        world.as_ref().map(|w| w.id.as_str()),
    )?;
    let (schema_map, session_attributes, character_attributes) =
        common::query_attributes(db.conn(), &session_id)?;
    let grouped_memories = build_grouped_memories(&memories, &characters);

    let prompt_calls = common::query_prompt_calls(db.conn(), &session_id)?;
    let prompt_call_index = common::build_prompt_trace_index(&prompt_calls);
    let llm_calls = common::query_llm_calls(db.conn(), &session_id, &prompt_call_index)?;

    let agent_sessions = common::query_agent_sessions(db.conn(), &session_id)?;
    let latest_checkpoints = common::query_latest_checkpoints(db.conn(), &session_id)?;
    let turn_journal = common::query_turn_journal(db.conn(), &session_id)?;

    // --- Derived data: prompt traces ---
    let director_prompt_traces: Vec<serde_json::Value> = prompt_calls
        .iter()
        .filter(|item| {
            item.get("recipient_type")
                .and_then(|value| value.as_str())
                == Some("director")
        })
        .map(|item| {
            let prompt_call = item.get("prompt_call").cloned().unwrap_or_default();
            serde_json::json!({
                "turn_index": item.get("turn_index").cloned().unwrap_or(serde_json::Value::Null),
                "step": item.get("step").cloned().unwrap_or(serde_json::Value::Null),
                "stage": item.get("stage").cloned().unwrap_or(serde_json::Value::Null),
                "prompt_trace": prompt_call.clone(),
                "prompt_result": item.get("prompt_result").cloned().unwrap_or(serde_json::Value::Null),
                "tool_loop_messages": item.get("tool_loop_messages").cloned().unwrap_or(serde_json::Value::Array(vec![])),
            })
        })
        .collect();

    let director_tool_loops: Vec<serde_json::Value> = prompt_calls
        .iter()
        .filter(|item| {
            item.get("recipient_type")
                .and_then(|value| value.as_str())
                == Some("director")
                && item
                    .get("step")
                    .and_then(|value| value.as_str())
                    .map(|step| {
                        step == "director_tool_phase"
                            || step.starts_with("director_tool_phase_")
                    })
                    .unwrap_or(false)
        })
        .map(|item| {
            let prompt_trace = item.get("prompt_call").cloned().unwrap_or_default();
            let tool_calls = prompt_trace
                .get("processed_model_return")
                .and_then(|value| value.get("tool_calls"))
                .cloned()
                .unwrap_or_else(|| serde_json::Value::Array(vec![]));
            let tool_results = prompt_trace
                .get("written_result")
                .and_then(|value| value.get("tool_results"))
                .cloned()
                .unwrap_or_else(|| serde_json::Value::Array(vec![]));
            serde_json::json!({
                "turn_index": item.get("turn_index").cloned().unwrap_or(serde_json::Value::Null),
                "step": item.get("step").cloned().unwrap_or(serde_json::Value::Null),
                "stage": item.get("stage").cloned().unwrap_or(serde_json::Value::Null),
                "tool_calls": tool_calls,
                "tool_results": tool_results,
                "prompt_trace": prompt_trace,
                "prompt_result": item.get("prompt_result").cloned().unwrap_or(serde_json::Value::Null),
                "tool_loop_messages": item.get("tool_loop_messages").cloned().unwrap_or(serde_json::Value::Array(vec![])),
            })
        })
        .collect();

    let character_prompt_traces: Vec<serde_json::Value> = prompt_calls
        .iter()
        .filter(|item| {
            item.get("recipient_type")
                .and_then(|value| value.as_str())
                == Some("character")
        })
        .map(|item| {
            let prompt_call = item.get("prompt_call").cloned().unwrap_or_default();
            serde_json::json!({
                "turn_index": item.get("turn_index").cloned().unwrap_or(serde_json::Value::Null),
                "step": item.get("step").cloned().unwrap_or(serde_json::Value::Null),
                "speaker": item.get("recipient_name").cloned().unwrap_or(serde_json::Value::Null),
                "stage": item.get("stage").cloned().unwrap_or(serde_json::Value::Null),
                "prompt_trace": prompt_call.clone(),
                "prompt_result": item.get("prompt_result").cloned().unwrap_or(serde_json::Value::Null),
            })
        })
        .collect();

    let system_prompt_coverage = {
        let director_missing = director_prompt_traces
            .iter()
            .filter(|item| {
                item.get("prompt_trace")
                    .and_then(|trace| trace.get("system_prompt"))
                    .and_then(|value| value.as_str())
                    .map(|s| s.trim().is_empty())
                    .unwrap_or(true)
            })
            .count();
        let character_missing = character_prompt_traces
            .iter()
            .filter(|item| {
                item.get("prompt_trace")
                    .and_then(|trace| trace.get("system_prompt"))
                    .and_then(|value| value.as_str())
                    .map(|s| s.trim().is_empty())
                    .unwrap_or(true)
            })
            .count();
        serde_json::json!({
            "director_missing_count": director_missing,
            "character_missing_count": character_missing,
            "director_ok": director_missing == 0,
            "character_ok": character_missing == 0,
        })
    };

    // --- Recovery state ---
    let latest_turn_index = turn_journal
        .iter()
        .map(|entry| entry.turn_index)
        .max()
        .unwrap_or_default();
    let latest_turn_entries = turn_journal
        .iter()
        .filter(|entry| entry.turn_index == latest_turn_index)
        .collect::<Vec<_>>();
    let completed_steps = latest_turn_entries
        .iter()
        .filter(|entry| entry.status == "completed")
        .map(|entry| entry.step.clone())
        .collect::<Vec<_>>();
    let completed_speaker_steps = latest_turn_entries
        .iter()
        .filter(|entry| entry.status == "completed")
        .filter_map(|entry| parse_completed_speaker_step(&entry.step))
        .collect::<Vec<_>>();
    let has_finished = latest_turn_entries
        .iter()
        .any(|entry| entry.step == "finished" && entry.status == "completed");
    let created_payload = latest_turn_entries
        .iter()
        .find(|entry| entry.step == "created" && entry.status == "completed")
        .map(|entry| entry.payload.clone());
    let recovery_state = serde_json::json!({
        "latest_turn_index": latest_turn_index,
        "has_incomplete_turn": latest_turn_index > 0 && !has_finished,
        "resume_ready": latest_turn_index > 0
            && !has_finished
            && created_payload
                .as_ref()
                .and_then(|payload| payload.get("player_input"))
                .and_then(|value| value.as_str())
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false),
        "completed_steps": completed_steps,
        "completed_speaker_steps": completed_speaker_steps,
        "created_payload": created_payload.unwrap_or(serde_json::Value::Null),
        "last_completed_step": latest_turn_entries
            .iter()
            .filter(|entry| entry.status == "completed")
            .max_by(|a, b| a.created_at.cmp(&b.created_at))
            .map(|entry| entry.step.clone())
            .unwrap_or_default(),
    });

    // --- Latest turn payloads ---
    let latest_director_payload = latest_turn_entries
        .iter()
        .find(|entry| entry.step == "director_completed" && entry.status == "completed")
        .map(|entry| entry.payload.clone())
        .unwrap_or(serde_json::Value::Null);
    let latest_scene_payload = latest_turn_entries
        .iter()
        .find(|entry| entry.step == "scene_applied" && entry.status == "completed")
        .map(|entry| entry.payload.clone())
        .unwrap_or(serde_json::Value::Null);
    let latest_memory_payload = latest_turn_entries
        .iter()
        .find(|entry| entry.step == "memory_committed" && entry.status == "completed")
        .map(|entry| entry.payload.clone())
        .unwrap_or(serde_json::Value::Null);
    let latest_attribute_payload = latest_turn_entries
        .iter()
        .find(|entry| entry.step == "attributes_committed" && entry.status == "completed")
        .map(|entry| entry.payload.clone())
        .unwrap_or(serde_json::Value::Null);
    let latest_switch_payload = latest_turn_entries
        .iter()
        .find(|entry| entry.step == "switch_character_applied" && entry.status == "completed")
        .map(|entry| entry.payload.clone())
        .unwrap_or(serde_json::Value::Null);
    let latest_switch_proposal = latest_director_payload
        .get("switch_character_proposal")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let latest_speaker_plan = latest_director_payload
        .get("planned_speakers")
        .cloned()
        .unwrap_or_else(|| serde_json::Value::Array(vec![]));
    let latest_turn_steps = latest_turn_entries
        .iter()
        .map(|entry| {
            serde_json::json!({
                "step": entry.step,
                "status": entry.status,
                "payload": entry.payload,
                "created_at": entry.created_at,
            })
        })
        .collect::<Vec<_>>();
    let latest_created_characters = latest_turn_entries
        .iter()
        .find(|entry| entry.step == "characters_created" && entry.status == "completed")
        .and_then(|entry| entry.payload.get("character_ids"))
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let turn_journal_timeline = build_turn_journal_timeline(&turn_journal);
    let trace_timeline = build_trace_timeline(&prompt_calls, &llm_calls, &turn_journal);

    let latest_turn_trace = turn_journal_timeline
        .iter()
        .find(|item| {
            item.get("turn_index")
                .and_then(|value| value.as_i64())
                .unwrap_or_default()
                == latest_turn_index as i64
        })
        .cloned()
        .unwrap_or_else(|| serde_json::Value::Null);

    let latest_writeback_events = latest_turn_entries
        .iter()
        .filter(|entry| {
            matches!(
                entry.step.as_str(),
                "runtime_effects_applied"
                    | "attributes_committed"
                    | "memory_committed"
                    | "scene_applied"
                    | "switch_character_applied"
                    | "characters_created"
                    | "finished"
            )
        })
        .map(|entry| {
            serde_json::json!({
                "step": entry.step,
                "status": entry.status,
                "created_at": entry.created_at,
                "payload": entry.payload,
            })
        })
        .collect::<Vec<_>>();

    let latest_tool_loop_summary = director_tool_loops
        .iter()
        .map(|item| {
            let tool_calls = item
                .get("tool_calls")
                .and_then(|value| value.as_array())
                .cloned()
                .unwrap_or_default();
            let tool_results = item
                .get("tool_results")
                .and_then(|value| value.as_array())
                .cloned()
                .unwrap_or_default();
            let tool_call_names = tool_calls
                .iter()
                .filter_map(|call| {
                    call.get("tool_name")
                        .or_else(|| call.get("name"))
                        .and_then(|value| value.as_str())
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                })
                .collect::<Vec<_>>();
            serde_json::json!({
                "turn_index": item.get("turn_index").cloned().unwrap_or(serde_json::Value::Null),
                "step": item.get("step").cloned().unwrap_or(serde_json::Value::Null),
                "tool_call_count": tool_calls.len(),
                "tool_result_count": tool_results.len(),
                "tool_call_names": tool_call_names,
                "tool_loop_message_count": item
                    .get("tool_loop_messages")
                    .and_then(|value| value.as_array())
                    .map(|items| items.len())
                    .unwrap_or(0),
            })
        })
        .collect::<Vec<_>>();

    let latest_memory_entries_preview = latest_memory_payload
        .get("memory_entries")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .take(24)
                .map(|entry| {
                    serde_json::json!({
                        "id": entry.get("id").cloned().unwrap_or(serde_json::Value::Null),
                        "character_id": entry.get("character_id").cloned().unwrap_or(serde_json::Value::Null),
                        "memory_type": entry.get("memory_type").cloned().unwrap_or(serde_json::Value::Null),
                        "importance": entry.get("importance").cloned().unwrap_or(serde_json::Value::Null),
                        "content": entry.get("content").cloned().unwrap_or(serde_json::Value::Null),
                        "scene_id": entry.get("scene_id").cloned().unwrap_or(serde_json::Value::Null),
                        "participants": entry.get("participants").cloned().unwrap_or_else(|| serde_json::Value::Array(vec![])),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let latest_runtime_decision = serde_json::json!({
        "turn_index": latest_turn_index,
        "director_completed": latest_director_payload,
        "scene_applied": latest_scene_payload.clone(),
        "attributes_committed": latest_attribute_payload.clone(),
        "memory_committed": latest_memory_payload.clone(),
        "switch_character_applied": latest_switch_payload.clone(),
        "characters_created": latest_created_characters.clone(),
    });

    let resolved_scene = serde_json::json!({
        "scene_id": latest_scene_payload.get("scene_id").cloned().unwrap_or(serde_json::Value::Null),
        "scene_name": latest_scene_payload.get("scene_name").cloned().unwrap_or(serde_json::Value::Null),
        "location": latest_scene_payload.get("location").cloned().unwrap_or(serde_json::Value::Null),
        "background_hint": latest_scene_payload.get("background_hint").cloned().unwrap_or(serde_json::Value::Null),
        "visible_characters": latest_scene_payload.get("visible_characters").cloned().unwrap_or_else(|| serde_json::Value::Array(vec![])),
        "present_characters": latest_scene_payload.get("present_characters").cloned().unwrap_or_else(|| serde_json::Value::Array(vec![])),
        "state_phase": latest_scene_payload.get("state_phase").cloned().unwrap_or(serde_json::Value::Null),
        "state_tags": latest_scene_payload.get("state_tags").cloned().unwrap_or_else(|| serde_json::Value::Array(vec![])),
        "state_metrics": latest_scene_payload.get("state_metrics").cloned().unwrap_or(serde_json::Value::Null),
    });

    let latest_turn_timeline_events = trace_timeline
        .iter()
        .find(|item| {
            item.get("turn_index")
                .and_then(|value| value.as_i64())
                .unwrap_or_default()
                == latest_turn_index as i64
        })
        .and_then(|item| item.get("events"))
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();

    let latest_memory_timeline_events = latest_turn_timeline_events
        .iter()
        .filter(|event| {
            event.get("domain").and_then(|value| value.as_str()) == Some("memory_commit")
        })
        .cloned()
        .collect::<Vec<_>>();

    let latest_writeback_timeline_events = latest_turn_timeline_events
        .iter()
        .filter(|event| {
            event.get("domain").and_then(|value| value.as_str()) == Some("state_writeback")
        })
        .cloned()
        .collect::<Vec<_>>();

    let timeline_tool_chain = trace_timeline
        .iter()
        .flat_map(|turn| {
            turn.get("events")
                .and_then(|value| value.as_array())
                .cloned()
                .unwrap_or_default()
        })
        .filter(|event| event.get("domain").and_then(|value| value.as_str()) == Some("tool_loop"))
        .collect::<Vec<_>>();

    let memory_commit_trace = serde_json::json!({
        "turn_index": latest_turn_index,
        "events": latest_memory_timeline_events,
        "memory_count": latest_memory_payload
            .get("memory_count")
            .cloned()
            .unwrap_or(serde_json::Value::Null),
        "memory_entries_preview": latest_memory_entries_preview,
    });

    let writeback_event_steps = latest_turn_timeline_events
        .iter()
        .filter_map(|event| {
            let event_type = event
                .get("event_type")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let step = event
                .get("step")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .trim()
                .to_string();
            if event_type == "journal"
                && matches!(
                    step.as_str(),
                    "runtime_effects_applied"
                        | "scene_applied"
                        | "attributes_committed"
                        | "memory_committed"
                        | "switch_character_applied"
                        | "characters_created"
                        | "finished"
                )
            {
                Some(serde_json::json!({
                    "step": step,
                    "created_at": event.get("created_at").cloned().unwrap_or(serde_json::Value::Null),
                    "status": event.get("status").cloned().unwrap_or(serde_json::Value::Null),
                }))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    let state_writeback_trace = serde_json::json!({
        "turn_index": latest_turn_index,
        "events": latest_writeback_timeline_events,
        "scene": latest_scene_payload,
        "attributes": latest_attribute_payload,
        "memory": latest_memory_payload,
        "switch_character": latest_switch_payload,
        "characters_created": latest_created_characters,
        "turn_steps": latest_turn_steps,
        "timeline_events": writeback_event_steps,
        "resolved_scene": resolved_scene,
    });

    let tool_chain: Vec<serde_json::Value> = director_tool_loops
        .iter()
        .map(|item| {
            serde_json::json!({
                "turn_index": item.get("turn_index").cloned().unwrap_or(serde_json::Value::Null),
                "step": item.get("step").cloned().unwrap_or(serde_json::Value::Null),
                "request": item.get("prompt_trace").and_then(|trace| trace.get("request")).cloned().unwrap_or(serde_json::Value::Null),
                "response": item.get("prompt_trace").and_then(|trace| trace.get("response")).cloned().unwrap_or(serde_json::Value::Null),
                "parsed": item.get("prompt_trace").and_then(|trace| trace.get("processed_model_return")).cloned().unwrap_or(serde_json::Value::Null),
                "tool_enriched": item.get("prompt_trace").and_then(|trace| trace.get("written_result")).cloned().unwrap_or(serde_json::Value::Null),
                "tool_calls": item.get("tool_calls").cloned().unwrap_or_else(|| serde_json::Value::Array(vec![])),
                "tool_results": item.get("tool_results").cloned().unwrap_or_else(|| serde_json::Value::Array(vec![])),
                "tool_call_count": item.get("tool_calls").and_then(|value| value.as_array()).map(|items| items.len()).unwrap_or(0),
                "tool_result_count": item.get("tool_results").and_then(|value| value.as_array()).map(|items| items.len()).unwrap_or(0),
            })
        })
        .collect();

    let tool_chain = if timeline_tool_chain.is_empty() {
        tool_chain
    } else {
        timeline_tool_chain
    };

    let available_modules = vec![
        "Session Orchestrator",
        "World Director",
        "Scene Runtime Manager",
        "Trigger Engine",
        "Rule Engine",
        "Inventory Runtime",
        "Speaker Selector",
        "Character Runtime",
        "Dialogue Pipeline",
        "Memory Pipeline",
        "State Engine",
    ];

    let runtime_session_attributes: Vec<serde_json::Value> = session_attributes
        .iter()
        .filter_map(|value| build_runtime_attribute_item(value, &schema_map))
        .collect();

    let mut grouped_runtime_character_attributes: BTreeMap<String, Vec<serde_json::Value>> =
        BTreeMap::new();
    for value in &character_attributes {
        if let Some(item) = build_runtime_attribute_item(value, &schema_map) {
            grouped_runtime_character_attributes
                .entry(value.owner_id.clone())
                .or_default()
                .push(item);
        }
    }
    let runtime_character_attributes = grouped_runtime_character_attributes
        .into_iter()
        .map(|(owner_id, items)| {
            serde_json::json!({
                "owner_type": "session_character",
                "owner_id": owner_id.clone(),
                "owner_label": owner_id.split(':').nth(1).unwrap_or(&owner_id),
                "items": items,
            })
        })
        .collect::<Vec<_>>();

    // speaker_selection_preview — complex computation, reconstruct inline
    let speaker_selection_preview = build_speaker_selection_preview(
        &session,
        &characters,
        &schema_map,
        &session_attributes,
        &character_attributes,
        latest_speaker_plan,
        latest_switch_proposal,
    );

    Ok(serde_json::json!({
        "session": session,
        "runtime_session_attributes": runtime_session_attributes,
        "runtime_character_attributes": runtime_character_attributes,
        "speaker_selection_preview": speaker_selection_preview,
        "memory_groups": grouped_memories,
        "agent_sessions": agent_sessions,
        "latest_checkpoints": latest_checkpoints,
        "turn_journal": turn_journal,
        "recovery_state": recovery_state,
        "director_prompt_traces": director_prompt_traces,
        "director_tool_loops": director_tool_loops,
        "system_prompt_coverage": system_prompt_coverage,
        "character_prompt_traces": character_prompt_traces,
        "llm_calls": llm_calls,
        "prompt_calls": prompt_calls,
        "turn_journal_timeline": turn_journal_timeline,
        "trace_timeline": trace_timeline,
        "latest_turn_trace": latest_turn_trace,
        "latest_writeback_events": latest_writeback_events,
        "latest_tool_loop_summary": latest_tool_loop_summary,
        "memory_commit_trace": memory_commit_trace,
        "latest_runtime_decision": latest_runtime_decision,
        "state_writeback_trace": state_writeback_trace,
        "tool_chain": tool_chain,
        "event_chain": build_event_chain_v2(&session.system_log),
        "available_modules": available_modules,
        "status": "aligned"
    }))
}

// ---------------------------------------------------------------------------
// speaker_selection_preview — kept inline to avoid over-abstracting the
// scoring logic
// ---------------------------------------------------------------------------

fn build_speaker_selection_preview(
    session: &crate::models::session::SessionSnapshot,
    characters: &[crate::models::character::CharacterDefinition],
    schema_map: &std::collections::HashMap<String, crate::models::attribute::AttributeSchema>,
    session_attributes: &[crate::models::attribute::AttributeValue],
    character_attributes: &[crate::models::attribute::AttributeValue],
    latest_speaker_plan: serde_json::Value,
    latest_switch_proposal: serde_json::Value,
) -> serde_json::Value {
    let runtime_session_attrs: Vec<serde_json::Value> = session_attributes
        .iter()
        .filter_map(|value| build_runtime_attribute_item(value, schema_map))
        .collect();

    let mut present_char_ids: Vec<String> = Vec::new();
    if let Some(arr) = latest_speaker_plan.as_array() {
        for item in arr {
            if let Some(s) = item.get("character_id").and_then(|v| v.as_str()) {
                if !s.is_empty() {
                    present_char_ids.push(s.to_string());
                }
            }
        }
    }

    let switch_chars: Vec<String> = latest_switch_proposal
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.get("character_id").and_then(|v| v.as_str()))
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();

    let char_summaries: Vec<serde_json::Value> = characters
        .iter()
        .map(|character| {
            let attrs: Vec<serde_json::Value> = character_attributes
                .iter()
                .filter(|value| value.owner_id == format!("{}:{}", session.id, character.id))
                .filter_map(|value| build_runtime_attribute_item(value, schema_map))
                .collect();

            let mut scores = serde_json::json!({
                "base_weight": 0.0,
                "inactive_penalty": 0.0,
                "override_keywords": [],
                "override_score": 0.0,
                "override_reason": null,
                "switched_out": switch_chars.contains(&character.id),
                "final_score": 0.0,
            });

            if let serde_json::Value::Object(ref mut map) = scores {
                let switched_out = switch_chars.contains(&character.id);
                let inactive_penalty = if switched_out { -1.0 } else { 0.0 };
                let base_weight = 0.0;
                let final_score = (base_weight as f64 + inactive_penalty).max(0.0);
                map.insert(
                    "switched_out".to_string(),
                    serde_json::Value::Bool(switched_out),
                );
                map.insert(
                    "inactive_penalty".to_string(),
                    serde_json::json!(inactive_penalty),
                );
                map.insert("base_weight".to_string(), serde_json::json!(base_weight));
                map.insert("final_score".to_string(), serde_json::json!(final_score));
            }

            serde_json::json!({
                "character_id": character.id,
                "character_name": character.name,
                "is_present": present_char_ids.contains(&character.id),
                "speaker_weight": null,
                "attributes": attrs,
                "scores": scores,
            })
        })
        .collect();

    serde_json::json!({
        "session_attributes": runtime_session_attrs,
        "characters": char_summaries,
        "latest_speaker_plan": latest_speaker_plan,
        "switch_character_proposal": latest_switch_proposal,
        "active_speakers": latest_speaker_plan
            .as_array()
            .map(|arr| arr.iter()
                .filter_map(|item| item.get("character_id").and_then(|v| v.as_str()))
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>())
            .unwrap_or_default(),
    })
}
