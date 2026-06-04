use tauri::State;

use crate::state::AppState;

use super::common;

pub(crate) async fn get_debug_timeline_impl(
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
    let grouped_memories = common::build_grouped_memories(&memories, &characters);

    let prompt_calls = common::query_prompt_calls(db.conn(), &session_id)?;
    let prompt_call_index = common::build_prompt_trace_index(&prompt_calls);
    let llm_calls = common::query_llm_calls(db.conn(), &session_id, &prompt_call_index)?;

    let turn_journal = common::query_turn_journal(db.conn(), &session_id)?;

    let turn_journal_timeline = common::build_turn_journal_timeline(&turn_journal);
    let trace_timeline = common::build_trace_timeline(&prompt_calls, &llm_calls, &turn_journal);

    let latest_turn_index = turn_journal
        .iter()
        .map(|entry| entry.turn_index)
        .max()
        .unwrap_or_default();

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

    let _latest_writeback_timeline_events = latest_turn_timeline_events
        .iter()
        .filter(|event| {
            event.get("domain").and_then(|value| value.as_str()) == Some("state_writeback")
        })
        .cloned()
        .collect::<Vec<_>>();

    let memory_commit_trace = {
        let latest_turn_entries: Vec<_> = turn_journal
            .iter()
            .filter(|entry| entry.turn_index == latest_turn_index)
            .collect();
        let latest_memory_payload = latest_turn_entries
            .iter()
            .find(|entry| entry.step == "memory_committed" && entry.status == "completed")
            .map(|entry| entry.payload.clone())
            .unwrap_or(serde_json::Value::Null);
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
        serde_json::json!({
            "turn_index": latest_turn_index,
            "events": latest_memory_timeline_events,
            "memory_count": latest_memory_payload
                .get("memory_count")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
            "memory_entries_preview": latest_memory_entries_preview,
        })
    };

    Ok(serde_json::json!({
        "session": session,
        "turn_journal": turn_journal,
        "llm_calls": llm_calls,
        "prompt_calls": prompt_calls,
        "turn_journal_timeline": turn_journal_timeline,
        "trace_timeline": trace_timeline,
        "latest_turn_trace": latest_turn_trace,
        "memory_commit_trace": memory_commit_trace,
        "grouped_memories": grouped_memories,
    }))
}
