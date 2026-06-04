use tauri::State;

use crate::state::AppState;

use super::common;

pub(crate) async fn get_debug_prompts_impl(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<serde_json::Value, String> {
    let db = state.db.lock().await;

    let session = common::query_session(db.conn(), &session_id)?;
    let world = common::query_world(db.conn(), &session.world_name)?;
    let _characters = common::query_characters(db.conn(), world.as_ref().map(|w| w.id.as_str()))?;

    let prompt_calls = common::query_prompt_calls(db.conn(), &session_id)?;
    let prompt_call_index = common::build_prompt_trace_index(&prompt_calls);
    let llm_calls = common::query_llm_calls(db.conn(), &session_id, &prompt_call_index)?;

    let director_prompt_traces: Vec<serde_json::Value> = prompt_calls
        .iter()
        .filter(|item| {
            item.get("recipient_type").and_then(|value| value.as_str()) == Some("director")
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
            item.get("recipient_type").and_then(|value| value.as_str()) == Some("director")
                && item
                    .get("step")
                    .and_then(|value| value.as_str())
                    .map(|step| {
                        step == "director_tool_phase" || step.starts_with("director_tool_phase_")
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
            item.get("recipient_type").and_then(|value| value.as_str()) == Some("character")
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

    let turn_journal = common::query_turn_journal(db.conn(), &session_id)?;
    let trace_timeline = common::build_trace_timeline(&prompt_calls, &llm_calls, &turn_journal);
    let timeline_tool_chain: Vec<serde_json::Value> = trace_timeline
        .iter()
        .flat_map(|turn| {
            turn.get("events")
                .and_then(|value| value.as_array())
                .cloned()
                .unwrap_or_default()
        })
        .filter(|event| event.get("domain").and_then(|value| value.as_str()) == Some("tool_loop"))
        .collect();

    let tool_chain: Vec<serde_json::Value> = if timeline_tool_chain.is_empty() {
        director_tool_loops
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
            .collect()
    } else {
        timeline_tool_chain
    };

    Ok(serde_json::json!({
        "session": session,
        "prompt_calls": prompt_calls,
        "director_prompt_traces": director_prompt_traces,
        "director_tool_loops": director_tool_loops,
        "character_prompt_traces": character_prompt_traces,
        "tool_chain": tool_chain,
    }))
}
