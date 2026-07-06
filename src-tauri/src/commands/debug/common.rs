use std::collections::{BTreeMap, HashMap};

use rusqlite::params;

use crate::models::attribute::{AttributeSchema, AttributeValue};
use crate::models::character::CharacterDefinition;
use crate::models::memory::{MemoryEntry, MemoryQueryParams};
use crate::models::runtime::{AgentCheckpointRecord, AgentSessionRecord, TurnJournalEntryRecord};
use crate::models::session::SessionSnapshot;
use crate::models::world::WorldDefinition;
// ---------------------------------------------------------------------------
// 共享 DB 查询函数 — 所有子命令 & 聚合命令共用
// ---------------------------------------------------------------------------

pub fn query_session(
    db: &rusqlite::Connection,
    session_id: &str,
) -> Result<SessionSnapshot, String> {
    let session_repo = crate::db::repositories::session_repo::SessionRepository::new(db);
    session_repo
        .get(session_id)?
        .ok_or_else(|| "Session not found".to_string())
}

/// M11: 调试面板应按 id 关联世界,而非按名字(重名/复制世界会加载错误世界的数据)。
/// 会话表只存 world_name,但该会话的 memories 行存有真实 world_id,优先据此精确定位;
/// 仅当会话尚无记忆(如刚创建)时才回退到按名字匹配。
pub fn query_world_for_session(
    db: &rusqlite::Connection,
    session: &SessionSnapshot,
) -> Result<Option<WorldDefinition>, String> {
    let world_repo = crate::db::repositories::world_repo::WorldRepository::new(db);
    if let Some(world_id) = query_session_world_id(db, &session.id)? {
        if let Some(world) = world_repo.get(&world_id)? {
            return Ok(Some(world));
        }
    }
    Ok(world_repo
        .list()?
        .into_iter()
        .find(|item| item.name == session.world_name))
}

fn query_session_world_id(
    db: &rusqlite::Connection,
    session_id: &str,
) -> Result<Option<String>, String> {
    let mut stmt = db
        .prepare("SELECT world_id FROM memories WHERE session_id = ?1 AND world_id <> '' LIMIT 1")
        .map_err(|e| e.to_string())?;
    let world_id = stmt
        .query_row(params![session_id], |row| row.get::<_, String>(0))
        .ok();
    Ok(world_id)
}

pub fn query_characters(
    db: &rusqlite::Connection,
    world_id: Option<&str>,
) -> Result<Vec<CharacterDefinition>, String> {
    let character_repo = crate::db::repositories::character_repo::CharacterRepository::new(db);
    if let Some(id) = world_id {
        character_repo.list_by_world(id)
    } else {
        Ok(Vec::new())
    }
}

pub fn query_memories(
    db: &rusqlite::Connection,
    session_id: &str,
    world_id: Option<&str>,
) -> Result<Vec<MemoryEntry>, String> {
    let memory_repo = crate::db::repositories::memory_repo::MemoryRepository::new(db);
    memory_repo.list(&MemoryQueryParams {
        world_id: world_id.map(|id| id.to_string()),
        session_id: Some(session_id.to_string()),
        character_id: None,
        layer: None,
        limit: Some(200),
    })
}

pub fn query_attributes(
    db: &rusqlite::Connection,
    session_id: &str,
) -> Result<
    (
        HashMap<String, AttributeSchema>,
        Vec<AttributeValue>,
        Vec<AttributeValue>,
    ),
    String,
> {
    let attribute_repo = crate::db::repositories::attribute_repo::AttributeRepository::new(db);
    let schema_map = attribute_repo
        .list_schemas(None)?
        .into_iter()
        .map(|schema| (schema.id.clone(), schema))
        .collect::<HashMap<String, AttributeSchema>>();
    let session_attributes = attribute_repo.list_values(Some("session"), Some(session_id), None)?;
    let character_attributes = attribute_repo
        .list_values(None, None, None)?
        .into_iter()
        .filter(|value| {
            value.owner_type == "session_character"
                && value.owner_id.starts_with(&(session_id.to_string() + ":"))
        })
        .collect::<Vec<_>>();
    Ok((schema_map, session_attributes, character_attributes))
}

pub fn query_prompt_calls(
    db: &rusqlite::Connection,
    session_id: &str,
) -> Result<Vec<serde_json::Value>, String> {
    let mut stmt = db
        .prepare("SELECT turn_index, step, recipient_type, recipient_name, prompt_call_json, created_at FROM prompt_call_traces WHERE session_id = ?1 ORDER BY turn_index, created_at, id")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![session_id], |row| {
            let prompt_call_json: String = row.get(4)?;
            let prompt_call = serde_json::from_str::<serde_json::Value>(&prompt_call_json)
                .unwrap_or_default();
            Ok(serde_json::json!({
                "turn_index": row.get::<_, i32>(0)?,
                "step": row.get::<_, String>(1)?,
                "recipient_type": row.get::<_, String>(2)?,
                "recipient_name": row.get::<_, String>(3)?,
                "created_at": row.get::<_, String>(5)?,
                "stage": prompt_call.get("stage").cloned().unwrap_or(serde_json::Value::Null),
                "prompt_call": prompt_call.clone(),
                "prompt_result": {
                    "raw_model_return": prompt_call.get("raw_model_return").cloned().unwrap_or(serde_json::Value::Null),
                    "return_processing": prompt_call.get("return_processing").cloned().unwrap_or(serde_json::Value::Null),
                    "processed_model_return": prompt_call.get("processed_model_return").cloned().unwrap_or(serde_json::Value::Null),
                    "written_result": prompt_call.get("written_result").cloned().unwrap_or(serde_json::Value::Null),
                },
                "tool_loop_messages": prompt_call
                    .get("raw_debug")
                    .and_then(|value| value.get("tool_loop_messages"))
                    .cloned()
                    .unwrap_or_else(|| serde_json::Value::Array(vec![])),
            }))
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn query_agent_sessions(
    db: &rusqlite::Connection,
    session_id: &str,
) -> Result<Vec<AgentSessionRecord>, String> {
    let mut stmt = db
        .prepare("SELECT id, session_id, agent_type, status, connection_state, scene_presence_state, character_id, character_name, checkpoint_id, last_active_turn, last_ack_message_index, prompt_version, runtime_key, initialized_at, created_at, updated_at FROM agent_sessions WHERE session_id = ?1 ORDER BY agent_type, character_name, created_at")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![session_id], |row| {
            Ok(AgentSessionRecord {
                id: row.get(0)?,
                session_id: row.get(1)?,
                agent_type: row.get(2)?,
                status: row.get(3)?,
                connection_state: row.get(4)?,
                scene_presence_state: row.get(5)?,
                character_id: row.get(6)?,
                character_name: row.get(7)?,
                checkpoint_id: row.get(8)?,
                last_active_turn: row.get(9)?,
                last_ack_message_index: row.get(10)?,
                prompt_version: row.get(11)?,
                runtime_key: row.get(12)?,
                initialized_at: row.get(13)?,
                created_at: row.get(14)?,
                updated_at: row.get(15)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn query_latest_checkpoints(
    db: &rusqlite::Connection,
    session_id: &str,
) -> Result<Vec<AgentCheckpointRecord>, String> {
    let mut stmt = db
        .prepare("SELECT c.id, c.agent_session_id, c.turn_index, c.checkpoint_type, c.payload_json, c.created_at
                  FROM agent_checkpoints c
                  INNER JOIN agent_sessions s ON s.id = c.agent_session_id
                  WHERE s.session_id = ?1 AND c.id = s.checkpoint_id
                  ORDER BY c.created_at DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![session_id], |row| {
            Ok(AgentCheckpointRecord {
                id: row.get(0)?,
                agent_session_id: row.get(1)?,
                turn_index: row.get(2)?,
                checkpoint_type: row.get(3)?,
                payload: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
                created_at: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn query_turn_journal(
    db: &rusqlite::Connection,
    session_id: &str,
) -> Result<Vec<TurnJournalEntryRecord>, String> {
    let mut stmt = db
        .prepare("SELECT id, session_id, turn_index, step, status, payload_json, created_at FROM turn_journal WHERE session_id = ?1 ORDER BY turn_index DESC, created_at DESC LIMIT 120")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![session_id], |row| {
            Ok(TurnJournalEntryRecord {
                id: row.get(0)?,
                session_id: row.get(1)?,
                turn_index: row.get(2)?,
                step: row.get(3)?,
                status: row.get(4)?,
                payload: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default(),
                created_at: row.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn query_llm_calls(
    db: &rusqlite::Connection,
    session_id: &str,
    prompt_call_index: &HashMap<String, &serde_json::Value>,
) -> Result<Vec<serde_json::Value>, String> {
    let mut stmt = db
        .prepare("SELECT turn_index, step, speaker, provider, model_id, status, latency_ms, input_payload_json, output_payload_json, created_at FROM llm_call_traces WHERE session_id = ?1 ORDER BY turn_index, created_at, id")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![session_id], |row| {
            let turn_index = row.get::<_, i32>(0)?;
            let step = row.get::<_, String>(1)?;
            let speaker = row.get::<_, String>(2)?;
            let input_json: String = row.get(7)?;
            let output_json: String = row.get(8)?;
            let input_payload =
                serde_json::from_str::<serde_json::Value>(&input_json).unwrap_or_default();
            let output_payload =
                serde_json::from_str::<serde_json::Value>(&output_json).unwrap_or_default();
            let fallback_provider: String = row.get(3)?;
            let fallback_model_id: String = row.get(4)?;
            let fallback_status: String = row.get(5)?;
            let fallback_latency_ms: i64 = row.get(6)?;
            let provider = extract_trace_string(&output_payload, "provider")
                .or_else(|| extract_trace_string(&input_payload, "provider"))
                .unwrap_or(fallback_provider);
            let model_id = extract_trace_string(&output_payload, "model_id")
                .or_else(|| extract_trace_string(&input_payload, "model_id"))
                .unwrap_or(fallback_model_id);
            let status = extract_trace_string(&output_payload, "status").unwrap_or(fallback_status);
            let latency_ms =
                extract_trace_i64(&output_payload, "latency_ms").unwrap_or(fallback_latency_ms);
            let request_payload = input_payload
                .get("request")
                .cloned()
                .unwrap_or_else(|| input_payload.clone());
            let response_payload = output_payload
                .get("response")
                .cloned()
                .unwrap_or_else(|| output_payload.clone());
            let prompt_trace = prompt_call_index
                .get(&build_trace_lookup_key(turn_index, &step, &speaker))
                .copied();
            let recipient_type = prompt_trace
                .and_then(|item| item.get("recipient_type"))
                .cloned()
                .unwrap_or_else(|| {
                    serde_json::Value::String(if speaker == "world_director" {
                        "director".to_string()
                    } else {
                        "character".to_string()
                    })
                });
            let stage = prompt_trace
                .and_then(|item| item.get("stage"))
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let prompt_result = prompt_trace
                .and_then(|item| item.get("prompt_result"))
                .cloned()
                .unwrap_or_default();
            let parsed_payload = prompt_result
                .get("processed_model_return")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let written_result = prompt_result
                .get("written_result")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let raw_model_return = prompt_result
                .get("raw_model_return")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let error_payload = output_payload
                .get("error")
                .cloned()
                .or_else(|| parsed_payload.get("error").cloned())
                .unwrap_or(serde_json::Value::Null);
            let tool_calls = parsed_payload
                .get("tool_calls")
                .cloned()
                .unwrap_or_else(|| serde_json::Value::Array(vec![]));
            let tool_results = written_result
                .get("tool_results")
                .cloned()
                .unwrap_or_else(|| serde_json::Value::Array(vec![]));
            let tool_loop_messages = prompt_trace
                .and_then(|item| item.get("tool_loop_messages"))
                .cloned()
                .unwrap_or_else(|| serde_json::Value::Array(vec![]));
            Ok(serde_json::json!({
                "turn_index": turn_index,
                "step": step,
                "speaker": speaker,
                "created_at": row.get::<_, String>(9)?,
                "recipient_type": recipient_type,
                "stage": stage,
                "provider": provider,
                "model": model_id.clone(),
                "model_id": model_id,
                "status": status,
                "latency_ms": latency_ms,
                "request": request_payload.clone(),
                "response": response_payload.clone(),
                "parsed": parsed_payload.clone(),
                "written_result": written_result,
                "raw_model_return": raw_model_return,
                "error": error_payload,
                "tool_calls": tool_calls,
                "tool_results": tool_results,
                "tool_loop_messages": tool_loop_messages,
                "input_payload": request_payload,
                "output_payload": response_payload,
                "raw_input_payload": input_payload,
                "raw_output_payload": output_payload,
            }))
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// 共享辅助函数
// ---------------------------------------------------------------------------

pub fn build_prompt_trace_index<'a>(
    prompt_calls: &'a [serde_json::Value],
) -> HashMap<String, &'a serde_json::Value> {
    let mut index = HashMap::new();
    for item in prompt_calls {
        let turn_index = item
            .get("turn_index")
            .and_then(|value| value.as_i64())
            .unwrap_or_default() as i32;
        let step = item
            .get("step")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let recipient_name = item
            .get("recipient_name")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        index.insert(
            build_trace_lookup_key(turn_index, step, recipient_name),
            item,
        );
    }
    index
}

pub fn build_trace_lookup_key(turn_index: i32, step: &str, recipient_name: &str) -> String {
    format!("{turn_index}\u{001f}{step}\u{001f}{recipient_name}")
}

pub fn classify_trace_domain(step: &str, event_type: &str) -> &'static str {
    match step {
        "memory_committed" => "memory_commit",
        "runtime_effects_applied"
        | "scene_applied"
        | "attributes_committed"
        | "switch_character_applied"
        | "characters_created"
        | "finished" => "state_writeback",
        "director_completed" | "director_decision" => "director_runtime",
        value if value == "director_tool_phase" || value.starts_with("director_tool_phase_") => {
            "tool_loop"
        }
        "character_response" => "character_llm",
        _ if event_type == "llm_call" => "llm_call",
        _ => "turn_flow",
    }
}

pub fn parse_completed_speaker_step(step: &str) -> Option<i32> {
    if !step.starts_with("speaker_") || !step.ends_with("_completed") {
        return None;
    }
    let numeric = &step["speaker_".len()..step.len() - "_completed".len()];
    numeric.parse::<i32>().ok()
}

pub fn build_runtime_attribute_item(
    value: &AttributeValue,
    schema_map: &HashMap<String, AttributeSchema>,
) -> Option<serde_json::Value> {
    let schema = schema_map.get(&value.schema_id)?;
    Some(serde_json::json!({
        "schema_id": schema.id,
        "key": schema.key,
        "label": schema.label,
        "value_type": schema.value_type,
        "value": value.value,
        "source": value.source,
        "display_policy": schema.display_policy,
        "influence_policy": schema.influence_policy,
    }))
}

pub fn extract_trace_string(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|item| item.as_str())
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

pub fn extract_trace_i64(value: &serde_json::Value, key: &str) -> Option<i64> {
    value.get(key).and_then(|item| item.as_i64())
}

pub fn build_event_chain_v2(system_log: &[String]) -> Vec<String> {
    system_log
        .iter()
        .map(|entry| {
            if entry.contains("涓栫晫涓绘帶") {
                format!("World Director -> {entry}")
            } else if entry.contains("TriggerEngine") || entry.contains("瑙﹀彂") {
                format!("Trigger Engine -> {entry}")
            } else if entry.contains("RuleEngine") || entry.contains("瑙勫垯") {
                format!("Rule Engine -> {entry}")
            } else if entry.contains("InventoryRuntime") || entry.contains("鑳屽寘") {
                format!("Inventory Runtime -> {entry}")
            } else if entry.contains("SpeakerSelector") || entry.contains("鍙戣█鎺掑簭") {
                format!("Speaker Selector -> {entry}")
            } else if entry.contains("CharacterRuntime") || entry.contains("DialoguePipeline") {
                format!("Character Runtime -> {entry}")
            } else if entry.contains("StateEngine") {
                format!("State Engine -> {entry}")
            } else {
                entry.clone()
            }
        })
        .collect()
}

pub fn build_turn_journal_timeline(
    turn_journal: &[TurnJournalEntryRecord],
) -> Vec<serde_json::Value> {
    let mut by_turn: BTreeMap<i32, Vec<&TurnJournalEntryRecord>> = BTreeMap::new();
    for entry in turn_journal {
        by_turn.entry(entry.turn_index).or_default().push(entry);
    }

    by_turn
        .into_iter()
        .map(|(turn_index, entries)| {
            let mut entry_payloads = BTreeMap::<String, serde_json::Value>::new();
            let mut completed_steps = Vec::new();
            let mut completed_payloads = Vec::new();
            for entry in entries {
                let payload = entry.payload.clone();
                if entry.status == "completed" {
                    completed_steps.push(entry.step.clone());
                    completed_payloads.push(serde_json::json!({
                        "step": entry.step,
                        "payload": payload.clone(),
                        "created_at": entry.created_at,
                    }));
                }
                entry_payloads.insert(
                    entry.step.clone(),
                    serde_json::json!({
                        "status": entry.status,
                        "payload": payload,
                        "created_at": entry.created_at,
                    }),
                );
            }
            serde_json::json!({
                "turn_index": turn_index,
                "completed_steps": completed_steps,
                "completed_payloads": completed_payloads,
                "entries": entry_payloads,
            })
        })
        .collect()
}

pub fn build_trace_timeline(
    prompt_calls: &[serde_json::Value],
    llm_calls: &[serde_json::Value],
    turn_journal: &[TurnJournalEntryRecord],
) -> Vec<serde_json::Value> {
    let mut turns = BTreeMap::<i32, BTreeMap<String, serde_json::Value>>::new();
    let mut turn_events = BTreeMap::<i32, Vec<serde_json::Value>>::new();

    for entry in turn_journal {
        turns
            .entry(entry.turn_index)
            .or_default()
            .entry(entry.step.clone())
            .or_insert_with(|| {
                serde_json::json!({
                    "turn_index": entry.turn_index,
                    "step": entry.step,
                    "journal": [],
                    "prompt_calls": [],
                    "llm_calls": [],
                })
            });
        if let Some(step_entry) = turns
            .get_mut(&entry.turn_index)
            .and_then(|map| map.get_mut(&entry.step))
        {
            if let Some(array) = step_entry
                .get_mut("journal")
                .and_then(|value| value.as_array_mut())
            {
                array.push(serde_json::json!({
                    "status": entry.status,
                    "payload": entry.payload,
                    "created_at": entry.created_at,
                }));
            }
        }
        turn_events
            .entry(entry.turn_index)
            .or_default()
            .push(serde_json::json!({
                "created_at": entry.created_at,
                "step": entry.step,
                "event_type": "journal",
                "domain": classify_trace_domain(entry.step.as_str(), "journal"),
                "status": entry.status,
                "payload": entry.payload,
            }));
    }

    for item in prompt_calls {
        let turn_index = item
            .get("turn_index")
            .and_then(|value| value.as_i64())
            .unwrap_or_default() as i32;
        let step = item
            .get("step")
            .and_then(|value| value.as_str())
            .unwrap_or("prompt")
            .to_string();
        let bucket = turns
            .entry(turn_index)
            .or_default()
            .entry(step.clone())
            .or_insert_with(|| {
                serde_json::json!({
                    "turn_index": turn_index,
                    "step": step,
                    "journal": [],
                    "prompt_calls": [],
                    "llm_calls": [],
                })
            });
        if let Some(array) = bucket
            .get_mut("prompt_calls")
            .and_then(|value| value.as_array_mut())
        {
            array.push(item.clone());
        }
        turn_events
            .entry(turn_index)
            .or_default()
            .push(serde_json::json!({
                "created_at": item.get("created_at").cloned().unwrap_or(serde_json::Value::Null),
                "step": step,
                "event_type": "prompt_call",
                "domain": classify_trace_domain(
                    item.get("step").and_then(|value| value.as_str()).unwrap_or_default(),
                    "prompt_call",
                ),
                "recipient_type": item.get("recipient_type").cloned().unwrap_or(serde_json::Value::Null),
                "recipient_name": item.get("recipient_name").cloned().unwrap_or(serde_json::Value::Null),
                "stage": item.get("stage").cloned().unwrap_or(serde_json::Value::Null),
                "prompt_result": item.get("prompt_result").cloned().unwrap_or(serde_json::Value::Null),
                "tool_calls": item
                    .get("prompt_result")
                    .and_then(|value| value.get("processed_model_return"))
                    .and_then(|value| value.get("tool_calls"))
                    .cloned()
                    .unwrap_or_else(|| serde_json::Value::Array(vec![])),
                "tool_results": item
                    .get("prompt_result")
                    .and_then(|value| value.get("written_result"))
                    .and_then(|value| value.get("tool_results"))
                    .cloned()
                    .unwrap_or_else(|| serde_json::Value::Array(vec![])),
                "tool_loop_messages": item.get("tool_loop_messages").cloned().unwrap_or_else(|| serde_json::Value::Array(vec![])),
            }));
    }

    for item in llm_calls {
        let turn_index = item
            .get("turn_index")
            .and_then(|value| value.as_i64())
            .unwrap_or_default() as i32;
        let step = item
            .get("step")
            .and_then(|value| value.as_str())
            .unwrap_or("llm")
            .to_string();
        let bucket = turns
            .entry(turn_index)
            .or_default()
            .entry(step.clone())
            .or_insert_with(|| {
                serde_json::json!({
                    "turn_index": turn_index,
                    "step": step,
                    "journal": [],
                    "prompt_calls": [],
                    "llm_calls": [],
                })
            });
        if let Some(array) = bucket
            .get_mut("llm_calls")
            .and_then(|value| value.as_array_mut())
        {
            array.push(item.clone());
        }
        turn_events
            .entry(turn_index)
            .or_default()
            .push(serde_json::json!({
                "created_at": item.get("created_at").cloned().unwrap_or(serde_json::Value::Null),
                "step": step,
                "event_type": "llm_call",
                "domain": classify_trace_domain(
                    item.get("step").and_then(|value| value.as_str()).unwrap_or_default(),
                    "llm_call",
                ),
                "recipient_type": item.get("recipient_type").cloned().unwrap_or(serde_json::Value::Null),
                "stage": item.get("stage").cloned().unwrap_or(serde_json::Value::Null),
                "speaker": item.get("speaker").cloned().unwrap_or(serde_json::Value::Null),
                "provider": item.get("provider").cloned().unwrap_or(serde_json::Value::Null),
                "model": item.get("model").cloned().unwrap_or(serde_json::Value::Null),
                "model_id": item.get("model_id").cloned().unwrap_or(serde_json::Value::Null),
                "status": item.get("status").cloned().unwrap_or(serde_json::Value::Null),
                "latency_ms": item.get("latency_ms").cloned().unwrap_or(serde_json::Value::Null),
                "request": item.get("request").cloned().unwrap_or(serde_json::Value::Null),
                "response": item.get("response").cloned().unwrap_or(serde_json::Value::Null),
                "parsed": item.get("parsed").cloned().unwrap_or(serde_json::Value::Null),
                "written_result": item.get("written_result").cloned().unwrap_or(serde_json::Value::Null),
                "raw_model_return": item.get("raw_model_return").cloned().unwrap_or(serde_json::Value::Null),
                "error": item.get("error").cloned().unwrap_or(serde_json::Value::Null),
                "tool_calls": item.get("tool_calls").cloned().unwrap_or_else(|| serde_json::Value::Array(vec![])),
                "tool_results": item.get("tool_results").cloned().unwrap_or_else(|| serde_json::Value::Array(vec![])),
                "tool_loop_messages": item.get("tool_loop_messages").cloned().unwrap_or_else(|| serde_json::Value::Array(vec![])),
            }));
    }

    turns
        .into_iter()
        .map(|(turn_index, steps)| {
            let mut events = turn_events.remove(&turn_index).unwrap_or_default();
            events.sort_by(|left, right| {
                let left_ts = left
                    .get("created_at")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default();
                let right_ts = right
                    .get("created_at")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default();
                left_ts.cmp(right_ts)
            });
            serde_json::json!({
                "turn_index": turn_index,
                "steps": steps.into_values().collect::<Vec<_>>(),
                "events": events,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 内存分组辅助函数（复用于 memories.rs 和聚合命令）
// ---------------------------------------------------------------------------

pub fn build_grouped_memories(
    memories: &[MemoryEntry],
    characters: &[CharacterDefinition],
) -> Vec<serde_json::Value> {
    let mut memory_groups: BTreeMap<String, Vec<serde_json::Value>> = BTreeMap::new();
    for memory in memories {
        memory_groups
            .entry(memory.character_id.clone())
            .or_default()
            .push(serde_json::to_value(memory).unwrap_or_default());
    }

    memory_groups
        .into_iter()
        .map(|(character_id, entries)| {
            let character_name = characters
                .iter()
                .find(|item| item.id == character_id)
                .map(|item| item.name.clone())
                .unwrap_or_else(|| character_id.clone());
            serde_json::json!({
                "character_id": character_id,
                "character_name": character_name,
                "memories": entries,
            })
        })
        .collect()
}
