use tauri::State;

use crate::state::AppState;

use super::common;

pub(crate) async fn get_debug_errors_impl(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<serde_json::Value, String> {
    let db = state.db.lock().await;

    let session = common::query_session(db.conn(), &session_id)?;

    let prompt_calls = common::query_prompt_calls(db.conn(), &session_id)?;
    let prompt_call_index = common::build_prompt_trace_index(&prompt_calls);
    let llm_calls = common::query_llm_calls(db.conn(), &session_id, &prompt_call_index)?;

    let turn_journal = common::query_turn_journal(db.conn(), &session_id)?;

    // Extract failed LLM calls
    let failed_llm_calls: Vec<serde_json::Value> = llm_calls
        .iter()
        .filter(|item| {
            item.get("status")
                .and_then(|value| value.as_str())
                .map(|status| status != "completed" && status != "success")
                .unwrap_or(false)
        })
        .map(|item| {
            serde_json::json!({
                "turn_index": item.get("turn_index").cloned().unwrap_or(serde_json::Value::Null),
                "step": item.get("step").cloned().unwrap_or(serde_json::Value::Null),
                "speaker": item.get("speaker").cloned().unwrap_or(serde_json::Value::Null),
                "recipient_type": item.get("recipient_type").cloned().unwrap_or(serde_json::Value::Null),
                "stage": item.get("stage").cloned().unwrap_or(serde_json::Value::Null),
                "provider": item.get("provider").cloned().unwrap_or(serde_json::Value::Null),
                "model": item.get("model").cloned().unwrap_or(serde_json::Value::Null),
                "status": item.get("status").cloned().unwrap_or(serde_json::Value::Null),
                "latency_ms": item.get("latency_ms").cloned().unwrap_or(serde_json::Value::Null),
                "error": item.get("error").cloned().unwrap_or(serde_json::Value::Null),
                "created_at": item.get("created_at").cloned().unwrap_or(serde_json::Value::Null),
            })
        })
        .collect();

    // Extract failed prompt calls (where prompt_result contains errors)
    let failed_prompt_calls: Vec<serde_json::Value> = prompt_calls
        .iter()
        .filter(|item| {
            let prompt_result = item.get("prompt_result").cloned().unwrap_or_default();
            let has_error = prompt_result
                .get("processed_model_return")
                .and_then(|value| value.get("error"))
                .is_some()
                || prompt_result
                    .get("written_result")
                    .and_then(|value| value.get("error"))
                    .is_some();
            has_error
        })
        .map(|item| {
            let prompt_result = item.get("prompt_result").cloned().unwrap_or_default();
            serde_json::json!({
                "turn_index": item.get("turn_index").cloned().unwrap_or(serde_json::Value::Null),
                "step": item.get("step").cloned().unwrap_or(serde_json::Value::Null),
                "recipient_type": item.get("recipient_type").cloned().unwrap_or(serde_json::Value::Null),
                "recipient_name": item.get("recipient_name").cloned().unwrap_or(serde_json::Value::Null),
                "stage": item.get("stage").cloned().unwrap_or(serde_json::Value::Null),
                "created_at": item.get("created_at").cloned().unwrap_or(serde_json::Value::Null),
                "error": prompt_result
                    .get("processed_model_return")
                    .and_then(|value| value.get("error"))
                    .cloned()
                    .or_else(|| prompt_result.get("written_result").and_then(|value| value.get("error")).cloned())
                    .unwrap_or(serde_json::Value::Null),
            })
        })
        .collect();

    // Extract failed turn journal entries
    let failed_turn_entries: Vec<serde_json::Value> = turn_journal
        .iter()
        .filter(|entry| entry.status != "completed" && entry.status != "success")
        .map(|entry| {
            serde_json::json!({
                "turn_index": entry.turn_index,
                "step": entry.step.clone(),
                "status": entry.status.clone(),
                "payload": entry.payload.clone(),
                "created_at": entry.created_at.clone(),
            })
        })
        .collect();

    // Error statistics
    let error_stats = serde_json::json!({
        "total_llm_calls": llm_calls.len(),
        "failed_llm_calls": failed_llm_calls.len(),
        "total_prompt_calls": prompt_calls.len(),
        "failed_prompt_calls": failed_prompt_calls.len(),
        "total_turn_entries": turn_journal.len(),
        "failed_turn_entries": failed_turn_entries.len(),
        "llm_failure_rate": if llm_calls.is_empty() { 0.0 } else { failed_llm_calls.len() as f64 / llm_calls.len() as f64 * 100.0 },
        "prompt_failure_rate": if prompt_calls.is_empty() { 0.0 } else { failed_prompt_calls.len() as f64 / prompt_calls.len() as f64 * 100.0 },
        "turn_failure_rate": if turn_journal.is_empty() { 0.0 } else { failed_turn_entries.len() as f64 / turn_journal.len() as f64 * 100.0 },
    });

    Ok(serde_json::json!({
        "session": session,
        "error_stats": error_stats,
        "failed_llm_calls": failed_llm_calls,
        "failed_prompt_calls": failed_prompt_calls,
        "failed_turn_entries": failed_turn_entries,
    }))
}
