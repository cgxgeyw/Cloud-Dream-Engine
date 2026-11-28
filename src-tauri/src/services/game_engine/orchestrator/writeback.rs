use crate::models::attribute::{AttributeSchema, AttributeValue, AttributeValueUpsertRequest};
use crate::models::session::*;
use crate::models::world::WorldDefinition;
use crate::services::game_engine::runtime_effects::DirectorRuntimeApplication;
use chrono::Utc;
use rusqlite::{params, Connection};
use std::collections::HashMap;

use super::request_building::*;
use super::run::*;
use super::turn_context::*;

pub(crate) use super::journal_trace::*;
pub(crate) use super::session_materialization::*;
pub(crate) use crate::services::oag::lifecycle::{insert_rows, query_rows_json};

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

/// 查一条消息在其创建回合关联的权威状态提议（runtime effects 写回 payload）。
/// 链路：message → metadata.turn_index → turn_journal 中该回合的
/// runtime_effects_applied 步骤。消息无 turn_index 或无对应步骤时返回 None。
/// 供重新生成（第 2 项）回滚状态使用。
#[allow(dead_code)] // 第 2 项（重新生成）使用
pub(crate) fn load_message_runtime_proposals(
    conn: &Connection,
    session_id: &str,
    message_id: &str,
) -> Result<Option<serde_json::Value>, String> {
    let Some(session) =
        crate::db::repositories::session_repo::SessionRepository::new(conn).get(session_id)?
    else {
        return Ok(None);
    };
    let Some(message) = session.find_message(message_id) else {
        return Ok(None);
    };
    let Some(turn_index) = message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("turn_index"))
        .and_then(|value| value.as_i64())
    else {
        return Ok(None);
    };
    let journal = load_turn_journal(conn, session_id, turn_index as i32)?;
    Ok(journal.into_iter().find_map(|entry| {
        if entry.get("step").and_then(|value| value.as_str()) == Some("runtime_effects_applied") {
            entry.get("payload").cloned()
        } else {
            None
        }
    }))
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

pub(crate) fn normalize_provider_name(provider: &str) -> String {
    crate::services::llm::normalize_provider(provider)
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
        .ok_or_else(|| "缺少目标回合的回滚快照".to_string())?;
    let mut restored_session = serde_json::from_value::<SessionSnapshot>(
        snapshot_payload
            .get("session_snapshot")
            .cloned()
            .ok_or_else(|| "缺少会话快照数据".to_string())?,
    )
    .map_err(|e| e.to_string())?;
    // 采样参数是玩家当前偏好，不是回合状态：重新生成不该把它退回旧快照里的值。
    restored_session.generation_params = session.generation_params.clone();
    conn.execute_batch("SAVEPOINT turn_rollback")
        .map_err(|error| error.to_string())?;
    let result = (|| {
        restore_runtime_attribute_values(
            conn,
            &session.id,
            snapshot_payload.get("attribute_values").cloned(),
        )?;
        restore_turn_memory_state(conn, &session.id, turn_index, &snapshot_payload)?;
        for character_id in collect_created_character_ids_from_turns(conn, &session.id, turn_index)?
        {
            crate::db::repositories::character_repo::CharacterRepository::new(conn)
                .delete(&character_id)?;
        }
        delete_turn_traces(conn, &session.id, turn_index)?;
        if let Some(oag_snapshot) = snapshot_payload.get("oag_snapshot") {
            crate::services::oag::lifecycle::restore_session(conn, &session.id, oag_snapshot)?;
        }
        crate::db::repositories::session_repo::SessionRepository::new(conn)
            .upsert(&restored_session)?;
        archive_replayed_turn_messages(conn, session, turn_index)?;
        Ok(restored_session)
    })();
    match result {
        Ok(restored) => match conn.execute_batch("RELEASE turn_rollback") {
            Ok(()) => Ok(restored),
            Err(error) => {
                let _ = conn.execute_batch("ROLLBACK TO turn_rollback; RELEASE turn_rollback");
                Err(error.to_string())
            }
        },
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK TO turn_rollback; RELEASE turn_rollback");
            Err(error)
        }
    }
}

/// 归档 turn_index >= from_turn_index 的会话消息到 journal（turn_regenerated 步骤）。
fn archive_replayed_turn_messages(
    conn: &Connection,
    session: &SessionSnapshot,
    from_turn_index: i32,
) -> Result<(), String> {
    let archived: Vec<_> = session
        .messages
        .iter()
        .filter(|message| {
            message
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("turn_index"))
                .and_then(|value| value.as_i64())
                .map(|turn| turn >= from_turn_index as i64)
                .unwrap_or(false)
        })
        .cloned()
        .collect();
    if archived.is_empty() {
        return Ok(());
    }
    append_turn_journal(
        conn,
        &session.id,
        from_turn_index,
        "turn_regenerated",
        "completed",
        serde_json::json!({ "archived_messages": archived }),
    )
}

/// 回滚记忆状态：删除 from_turn_index 起新增的记忆（含 embedding），
/// 并按快照恢复总结区间与记忆实体/关系。旧快照缺少对应节时保留原有兼容行为。
fn restore_turn_memory_state(
    conn: &Connection,
    session_id: &str,
    from_turn_index: i32,
    snapshot_payload: &serde_json::Value,
) -> Result<(), String> {
    let summary_ranges = snapshot_payload
        .get("memory_summary_ranges")
        .and_then(|value| value.as_array());
    if let Some(ranges) = summary_ranges {
        let snapshot_memory_ids: std::collections::HashSet<&str> = ranges
            .iter()
            .filter_map(|range| range.get("memory_id").and_then(|value| value.as_str()))
            .collect();
        let mut stmt = conn
            .prepare("SELECT id FROM memories WHERE session_id = ?1 AND memory_type = 'summary'")
            .map_err(|e| e.to_string())?;
        let summary_memory_ids = stmt
            .query_map(params![session_id], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        drop(stmt);
        for memory_id in summary_memory_ids {
            // 快照后才生成的旧桶总结，turn_index 可能小于本轮。
            if snapshot_memory_ids.contains(memory_id.as_str()) {
                continue;
            }
            conn.execute(
                "DELETE FROM memory_embeddings WHERE memory_id = ?1",
                params![memory_id],
            )
            .map_err(|e| e.to_string())?;
            conn.execute("DELETE FROM memories WHERE id = ?1", params![memory_id])
                .map_err(|e| e.to_string())?;
        }
        conn.execute(
            "DELETE FROM memory_summary_ranges WHERE session_id = ?1",
            params![session_id],
        )
        .map_err(|e| e.to_string())?;
    }
    conn.execute(
        "DELETE FROM memory_embeddings WHERE memory_id IN (SELECT id FROM memories WHERE session_id = ?1 AND turn_index >= ?2)",
        params![session_id, from_turn_index],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM memories WHERE session_id = ?1 AND turn_index >= ?2",
        params![session_id, from_turn_index],
    )
    .map_err(|e| e.to_string())?;
    let Some(entities) = snapshot_payload
        .get("memory_entities")
        .and_then(|value| value.as_array())
    else {
        if let Some(ranges) = summary_ranges {
            insert_rows(conn, "memory_summary_ranges", ranges)?;
        }
        return Ok(());
    };
    let empty: Vec<serde_json::Value> = Vec::new();
    let relations = snapshot_payload
        .get("memory_relations")
        .and_then(|value| value.as_array())
        .unwrap_or(&empty);
    conn.execute(
        "DELETE FROM memory_relations WHERE session_id = ?1",
        params![session_id],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM memory_entities WHERE session_id = ?1",
        params![session_id],
    )
    .map_err(|e| e.to_string())?;
    if let Some(ranges) = summary_ranges {
        insert_rows(conn, "memory_summary_ranges", ranges)?;
    }
    insert_rows(conn, "memory_entities", entities)?;
    insert_rows(conn, "memory_relations", relations)?;
    Ok(())
}

/// 最新已完成回合（journal 有 finished 步骤）的 turn_index；没有则 None。
pub(crate) fn latest_finished_turn(
    conn: &Connection,
    session_id: &str,
) -> Result<Option<i32>, String> {
    let turn_index: i32 = conn
        .query_row(
            "SELECT COALESCE(MAX(turn_index), 0) FROM turn_journal WHERE session_id = ?1",
            params![session_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if turn_index <= 0 {
        return Ok(None);
    }
    let journal = load_turn_journal(conn, session_id, turn_index)?;
    if !journal_has_completed_step(&journal, "finished") {
        return Ok(None);
    }
    Ok(Some(turn_index))
}

/// 某回合的玩家输入（journal created 步骤的 payload）。
pub(crate) fn turn_player_input(
    conn: &Connection,
    session_id: &str,
    turn_index: i32,
) -> Result<Option<String>, String> {
    let journal = load_turn_journal(conn, session_id, turn_index)?;
    Ok(journal.iter().find_map(|entry| {
        if entry.get("step").and_then(|v| v.as_str()) == Some("created") {
            entry
                .get("payload")
                .and_then(|payload| payload.get("player_input"))
                .and_then(|v| v.as_str())
                .map(str::to_string)
        } else {
            None
        }
    }))
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
        description: schema.description.clone(),
        value_type: schema.value_type.clone(),
        enum_options: schema.enum_options.clone(),
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
        session.map_graph_nodes = update_current_map_graph_nodes(
            &session.map_graph_nodes,
            &session.scene.name,
            &session.location,
        );
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
                input.world.director_image_generation_enabled(),
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
            .ok_or_else(|| "会话不存在".to_string())?;
        let world = resolve_world_for_session(conn, &session)?;
        let characters = crate::db::repositories::character_repo::CharacterRepository::new(conn)
            .list_by_world(&world.id)?;
        let new_character = characters
            .iter()
            .find(|character| character.id == request.player_character_id)
            .ok_or_else(|| "角色不存在".to_string())?
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
        crate::services::assets::resolver::register_generated_session_assets(conn, updated)?;
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
        crate::services::assets::resolver::register_generated_session_assets(
            input.conn,
            input.updated,
        )?;
        write_oag_turn(
            input.conn,
            &input.world,
            input.session,
            input.updated,
            input.characters,
            input.turn_index,
            input.runtime_application,
        );
        append_runtime_effects_journal(
            input.conn,
            input.recovery_journal,
            input.session_id,
            input.turn_index,
            input.runtime_application,
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

fn write_oag_turn(
    conn: &Connection,
    world: &WorldDefinition,
    previous: &SessionSnapshot,
    updated: &SessionSnapshot,
    characters: &[crate::models::character::CharacterDefinition],
    turn_index: i32,
    runtime_application: &DirectorRuntimeApplication,
) {
    if !crate::services::game_engine::memory::resolve_oag_enabled(world) {
        return;
    }
    let already_written = load_turn_journal(conn, &updated.id, turn_index)
        .map(|journal| journal_has_completed_step(&journal, "oag_turn_written"))
        .unwrap_or(false);
    if already_written {
        return;
    }
    let oag_result = (|| {
        let ontology = crate::services::oag::canon::load_active_ontology(conn, &world.id)?
            .ok_or_else(|| "当前世界尚未导入 OAG 本体".to_string())?;
        crate::services::oag::turn_writer::write_turn(
            &crate::services::oag::turn_writer::OagTurnWriterInput {
                conn,
                world,
                previous,
                updated,
                characters,
                turn_index,
                runtime_application,
                resolved_ontology: &ontology,
            },
        )
    })();
    let (status, detail) = match oag_result {
        Ok(()) => ("completed", serde_json::json!({})),
        Err(error) => ("failed", serde_json::json!({ "error": error })),
    };
    let _ = crate::services::oag::turn_writer::record_trace(
        conn,
        &world.id,
        &updated.id,
        turn_index,
        "turn_write",
        status,
        detail.clone(),
    );
    let _ = append_turn_journal(
        conn,
        &updated.id,
        turn_index,
        "oag_turn_written",
        status,
        detail,
    );
}

#[cfg(test)]
mod oag_writeback_tests {
    use super::*;
    use crate::db::{oag_schema, schema};
    use crate::models::session::{AssetSelection, SceneRuntime, SessionState};

    fn fixtures() -> (Connection, WorldDefinition, SessionSnapshot) {
        let conn = Connection::open_in_memory().unwrap();
        schema::create_tables(&conn).unwrap();
        let world = WorldDefinition {
            id: "world".to_string(),
            name: "World".to_string(),
            genre: String::new(),
            background_prompt: String::new(),
            opening_scene: String::new(),
            summary: String::new(),
            time_system: String::new(),
            map_nodes: serde_json::Value::Null,
            time_config: serde_json::Value::Null,
            director_config: serde_json::json!({ "memory_mode": "oag" }),
            ui_theme_config: serde_json::Value::Null,
            director_system_prompt_base: String::new(),
            director_runtime_system_prompt: String::new(),
            opening_messages: Vec::new(),
            opening_character_ids: Vec::new(),
            player_character_id: None,
        };
        let session = SessionSnapshot {
            id: "session".to_string(),
            world_name: "World".to_string(),
            location: "Dock".to_string(),
            time_label: "Night".to_string(),
            current_speaker: "Player".to_string(),
            current_line: String::new(),
            player_character_id: "player".to_string(),
            player_character_name: "Player".to_string(),
            visible_characters: Vec::new(),
            messages: Vec::new(),
            player_stats: Vec::new(),
            map_graph_nodes: Vec::new(),
            map_graph_edges: Vec::new(),
            system_log: Vec::new(),
            scene: SceneRuntime::default(),
            assets: AssetSelection::default(),
            state: SessionState::default(),
            generation_params: Default::default(),
        };
        (conn, world, session)
    }

    #[test]
    fn disabled_mode_does_not_attempt_oag_write() {
        let (conn, mut world, session) = fixtures();
        world.director_config = serde_json::json!({ "memory_mode": "embedding" });
        write_oag_turn(
            &conn,
            &world,
            &session,
            &session,
            &[],
            1,
            &DirectorRuntimeApplication::default(),
        );
        let journals: i64 = conn
            .query_row("SELECT COUNT(*) FROM turn_journal", [], |row| row.get(0))
            .unwrap();
        assert_eq!(journals, 0);
    }

    #[test]
    fn failure_is_traced_without_blocking_and_completed_turn_is_skipped() {
        let (conn, world, session) = fixtures();
        oag_schema::create(&conn).unwrap();
        for _ in 0..2 {
            write_oag_turn(
                &conn,
                &world,
                &session,
                &session,
                &[],
                1,
                &DirectorRuntimeApplication::default(),
            );
        }
        let failed_traces: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM oag_write_traces WHERE status = 'failed'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(failed_traces, 2);
        let failed_journals: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM turn_journal WHERE step = 'oag_turn_written' AND status = 'failed'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(failed_journals, 2);
        conn.execute(
            "UPDATE turn_journal SET status = 'completed' WHERE step = 'oag_turn_written'",
            [],
        )
        .unwrap();
        write_oag_turn(
            &conn,
            &world,
            &session,
            &session,
            &[],
            1,
            &DirectorRuntimeApplication::default(),
        );
        let traces: i64 = conn
            .query_row("SELECT COUNT(*) FROM oag_write_traces", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(traces, 2);
    }
}

#[cfg(test)]
mod rollback_tests {
    //! 第 2 项（重新生成/重发）的回滚验收：snapshot_created + rollback_session_to_turn
    //! 必须让 属性/记忆/事实关系/会话消息 回到"从没生成过被覆盖回合"的状态，
    //! 被覆盖消息归档可追溯，连续回滚不漂移。
    use super::*;
    use crate::db::schema;
    use crate::models::session::{
        AssetSelection, ChatMessage, MessageContent, SceneRuntime, SessionState,
    };

    fn sample_session(id: &str) -> SessionSnapshot {
        SessionSnapshot {
            id: id.to_string(),
            world_name: "World".to_string(),
            location: "Dock".to_string(),
            time_label: "Night".to_string(),
            current_speaker: "Alice".to_string(),
            current_line: "line".to_string(),
            player_character_id: "char-player".to_string(),
            player_character_name: "Player".to_string(),
            visible_characters: vec!["Alice".to_string()],
            messages: vec![],
            player_stats: vec![],
            map_graph_nodes: vec![],
            map_graph_edges: vec![],
            system_log: vec![],
            scene: SceneRuntime::default(),
            assets: AssetSelection::default(),
            state: SessionState::default(),
            generation_params: Default::default(),
        }
    }

    fn message(turn_index: i64, text: &str) -> ChatMessage {
        ChatMessage::new(
            "agent",
            MessageContent::Text(text.to_string()),
            Some("Alice".to_string()),
        )
        .with_metadata(serde_json::json!({ "turn_index": turn_index }))
    }

    fn setup() -> (Connection, SessionSnapshot) {
        let conn = Connection::open_in_memory().expect("open sqlite");
        schema::create_tables(&conn).expect("create schema");
        crate::db::oag_schema::create(&conn).expect("create OAG schema");
        conn.pragma_update(None, "foreign_keys", true)
            .expect("enable foreign keys");
        let session = sample_session("sess-1");
        crate::db::repositories::session_repo::SessionRepository::new(&conn)
            .upsert(&session)
            .expect("upsert");
        (conn, session)
    }

    fn insert_entity(conn: &Connection, id: &str, mention_count: i64) {
        conn.execute(
            "INSERT INTO memory_entities (id, world_id, session_id, name, name_normalized, entity_type, aliases_json, mention_count, first_seen_turn, last_seen_turn, created_at) VALUES (?1, 'world-1', 'sess-1', ?2, ?3, '', '[]', ?4, 1, 1, 'now')",
            params![id, id, id.to_lowercase(), mention_count],
        )
        .expect("insert entity");
    }

    fn insert_relation(conn: &Connection, id: &str, subject: &str, invalid_at: Option<i64>) {
        conn.execute(
            "INSERT INTO memory_relations (id, world_id, session_id, subject_entity_id, predicate, object_entity_id, object_text, valid_from_turn, invalid_at_turn, source, confidence, created_at) VALUES (?1, 'world-1', 'sess-1', ?2, '持有', NULL, '银钥匙', 1, ?3, 'llm_extraction', 0.9, 'now')",
            params![id, subject, invalid_at],
        )
        .expect("insert relation");
    }

    fn insert_memory(conn: &Connection, id: &str, turn_index: i64) {
        conn.execute(
            "INSERT INTO memories (id, world_id, session_id, character_id, content, turn_index) VALUES (?1, 'world-1', 'sess-1', 'char-a', ?2, ?3)",
            params![id, format!("记忆-{id}"), turn_index],
        )
        .expect("insert memory");
    }

    fn insert_summary(conn: &Connection, id: &str, turn_index: i64, scene_id: &str) {
        conn.execute(
            "INSERT INTO memories (id, world_id, session_id, character_id, content, turn_index, memory_type, scene_id) VALUES (?1, 'world-1', 'sess-1', 'char-a', ?2, ?3, 'summary', ?4)",
            params![id, format!("总结-{id}"), turn_index, scene_id],
        )
        .expect("insert summary memory");
        conn.execute(
            "INSERT INTO memory_summary_ranges (memory_id, world_id, session_id, character_id, bucket_index, bucket_start, bucket_end, scene_id, scene_name, turn_start, turn_end, created_at) VALUES (?1, 'world-1', 'sess-1', 'char-a', 0, 1, 10, ?2, ?2, 1, 10, 'now')",
            params![id, scene_id],
        )
        .expect("insert summary range");
    }

    fn upsert_attribute(conn: &Connection, id: &str, owner: &str, value: &str) {
        conn.execute(
            "INSERT INTO attribute_schemas (id, scope, key, label) VALUES (?1, 'session', ?2, ?3) ON CONFLICT(id) DO NOTHING",
            params![format!("schema-{id}"), id, id],
        )
        .expect("insert schema");
        conn.execute(
            "INSERT INTO attribute_values (id, schema_id, owner_type, owner_id, value_json) VALUES (?1, ?2, 'session', ?3, ?4) ON CONFLICT(schema_id, owner_type, owner_id) DO UPDATE SET value_json = excluded.value_json",
            params![
                format!("value-{id}"),
                format!("schema-{id}"),
                owner,
                format!("\"{value}\"")
            ],
        )
        .expect("upsert attribute");
    }

    fn scalar<T: rusqlite::types::FromSql>(
        conn: &Connection,
        sql: &str,
        p: &[&dyn rusqlite::ToSql],
    ) -> T {
        conn.query_row(sql, p, |row| row.get(0)).expect("scalar")
    }

    /// 按生产快照格式写入 snapshot_created（与 prepare_turn_context 一致）。
    fn write_snapshot(conn: &Connection, session: &SessionSnapshot, turn_index: i32) {
        append_turn_journal(
            conn,
            &session.id,
            turn_index,
            "snapshot_created",
            "completed",
            serde_json::json!({
                "session_snapshot": session,
                "attribute_values": collect_runtime_attribute_values(conn, &session.id).unwrap(),
                "memory_entities": query_rows_json(
                    conn,
                    "SELECT * FROM memory_entities WHERE session_id = ?1",
                    &[&session.id],
                ).unwrap(),
                "memory_relations": query_rows_json(
                    conn,
                    "SELECT * FROM memory_relations WHERE session_id = ?1",
                    &[&session.id],
                ).unwrap(),
                "memory_summary_ranges": query_rows_json(
                    conn,
                    "SELECT * FROM memory_summary_ranges WHERE session_id = ?1",
                    &[&session.id],
                ).unwrap(),
                "oag_snapshot": crate::services::oag::lifecycle::snapshot_session(conn, &session.id).unwrap(),
            }),
        )
        .expect("write snapshot");
    }

    #[test]
    fn rollback_restores_attributes_memory_graph_and_archives_messages() {
        let (conn, mut session) = setup();
        // 回合前基线
        upsert_attribute(&conn, "gold", "sess-1", "100");
        insert_entity(&conn, "银钥匙", 1);
        insert_relation(&conn, "rel-1", "银钥匙", None);
        insert_memory(&conn, "mem-old", 3);
        session.messages = vec![message(4, "旧回合消息")];
        write_snapshot(&conn, &session, 5);

        // 模拟回合 5 的全部变更
        upsert_attribute(&conn, "gold", "sess-1", "42");
        insert_entity(&conn, "新角色", 1);
        conn.execute(
            "UPDATE memory_entities SET mention_count = 7 WHERE id = '银钥匙'",
            [],
        )
        .expect("bump mentions");
        conn.execute(
            "UPDATE memory_relations SET invalid_at_turn = 5 WHERE id = 'rel-1'",
            [],
        )
        .expect("invalidate relation");
        insert_relation(&conn, "rel-2", "新角色", None);
        insert_memory(&conn, "mem-new", 5);
        session.messages.push(message(5, "要被覆盖的回复"));
        session.location = "Warehouse".to_string();

        let restored = rollback_session_to_turn(&conn, &session, 5).expect("rollback");

        // 会话快照内容还原
        assert_eq!(restored.location, "Dock");
        assert_eq!(restored.messages.len(), 1);
        assert_eq!(restored.messages[0].content.as_str(), "旧回合消息");
        // 属性回到 100
        let gold: String = scalar(
            &conn,
            "SELECT value_json FROM attribute_values WHERE owner_id = 'sess-1' AND schema_id = 'schema-gold'",
            &[],
        );
        assert_eq!(gold, "\"100\"");
        // 实体：新角色消失，银钥匙 mention_count 还原为 1
        let entity_count: i64 = scalar(
            &conn,
            "SELECT COUNT(*) FROM memory_entities WHERE session_id = 'sess-1'",
            &[],
        );
        assert_eq!(entity_count, 1);
        let mentions: i64 = scalar(
            &conn,
            "SELECT mention_count FROM memory_entities WHERE id = '银钥匙'",
            &[],
        );
        assert_eq!(mentions, 1);
        // 关系：rel-2 消失，rel-1 恢复为未作废
        let rel_count: i64 = scalar(
            &conn,
            "SELECT COUNT(*) FROM memory_relations WHERE session_id = 'sess-1'",
            &[],
        );
        assert_eq!(rel_count, 1);
        let invalid_at: Option<i64> = scalar(
            &conn,
            "SELECT invalid_at_turn FROM memory_relations WHERE id = 'rel-1'",
            &[],
        );
        assert_eq!(invalid_at, None);
        // 记忆：回合 >= 5 的被删，旧记忆保留
        let mem_ids: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT id FROM memories WHERE session_id = 'sess-1' ORDER BY id")
                .unwrap();
            stmt.query_map([], |row| row.get(0))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert_eq!(mem_ids, vec!["mem-old".to_string()]);
        // 旧回复归档可追溯
        let journal = load_turn_journal(&conn, "sess-1", 5).expect("journal");
        let archived_step = journal
            .iter()
            .find(|entry| entry.get("step").and_then(|v| v.as_str()) == Some("turn_regenerated"))
            .expect("archived step");
        let archived_messages = archived_step
            .get("payload")
            .and_then(|p| p.get("archived_messages"))
            .and_then(|v| v.as_array())
            .expect("archived messages");
        assert_eq!(archived_messages.len(), 1);
        assert_eq!(
            archived_messages[0].get("content").and_then(|v| v.as_str()),
            Some("要被覆盖的回复")
        );
    }

    #[test]
    fn rollback_deletes_memories_from_all_replayed_turns() {
        let (conn, session) = setup();
        insert_memory(&conn, "mem-t3", 3);
        write_snapshot(&conn, &session, 3);
        insert_memory(&conn, "mem-t4", 4);
        insert_memory(&conn, "mem-t5", 5);

        rollback_session_to_turn(&conn, &session, 3).expect("rollback");
        let remaining: i64 = scalar(
            &conn,
            "SELECT COUNT(*) FROM memories WHERE session_id = 'sess-1'",
            &[],
        );
        assert_eq!(remaining, 0, "回合 >= 3 的记忆都应删除");
    }

    #[test]
    fn rollback_restores_summary_ranges_and_fact_provenance() {
        let (conn, session) = setup();
        insert_summary(&conn, "summary-before", 10, "scene-1");
        insert_memory(&conn, "raw-before", 3);
        insert_entity(&conn, "银钥匙", 1);
        conn.execute(
            "INSERT INTO memory_relations (id, world_id, session_id, subject_entity_id, predicate, object_text, valid_from_turn, created_at, asserted_by, asserted_by_id, visibility, truth_status, witnesses_json) VALUES ('rel-before', 'world-1', 'sess-1', '银钥匙', '藏在', '柜子', 3, 'now', 'Alice', 'char-a', 'private', 'asserted', '[\"Alice\",\"Player\"]')",
            [],
        )
        .expect("insert private fact");
        write_snapshot(&conn, &session, 11);

        insert_summary(&conn, "summary-old-bucket-after-snapshot", 10, "scene-2");
        insert_summary(&conn, "summary-current-turn", 11, "scene-3");
        insert_memory(&conn, "raw-after", 11);
        conn.execute("UPDATE memory_relations SET invalid_at_turn = 11, visibility = 'public', truth_status = 'confirmed' WHERE id = 'rel-before'", [])
            .expect("mutate old fact");
        insert_entity(&conn, "新钥匙", 1);
        insert_relation(&conn, "rel-after", "新钥匙", None);

        rollback_session_to_turn(&conn, &session, 11).expect("rollback");
        let mut stmt = conn
            .prepare("SELECT id FROM memories WHERE session_id = 'sess-1' ORDER BY id")
            .unwrap();
        let memory_ids: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(memory_ids, vec!["raw-before", "summary-before"]);
        let ranges: Vec<(String, String)> = {
            let mut stmt = conn
                .prepare("SELECT memory_id, scene_id FROM memory_summary_ranges WHERE session_id = 'sess-1'")
                .unwrap();
            stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert_eq!(
            ranges,
            vec![("summary-before".to_string(), "scene-1".to_string())]
        );
        let fact: (Option<i64>, String, String, String, Option<String>, String) = conn
            .query_row(
                "SELECT invalid_at_turn, asserted_by, asserted_by_id, visibility, truth_status, witnesses_json FROM memory_relations WHERE id = 'rel-before'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
            )
            .expect("restored fact");
        assert_eq!(
            fact,
            (
                None,
                "Alice".to_string(),
                "char-a".to_string(),
                "private".to_string(),
                Some("asserted".to_string()),
                "[\"Alice\",\"Player\"]".to_string()
            )
        );
        let relation_count: i64 = scalar(
            &conn,
            "SELECT COUNT(*) FROM memory_relations WHERE session_id = 'sess-1'",
            &[],
        );
        assert_eq!(relation_count, 1);
        let integrity: String = scalar(&conn, "PRAGMA integrity_check", &[]);
        assert_eq!(integrity, "ok");
    }

    #[test]
    fn repeated_rollbacks_do_not_drift() {
        let (conn, session) = setup();
        upsert_attribute(&conn, "gold", "sess-1", "100");
        write_snapshot(&conn, &session, 5);

        // 生产形态：每次重放都会删掉旧 journal（含旧快照）并写入新快照，
        // 连续重生成 = 回滚 → 新快照 → 变更 → 再回滚，状态不得漂移。
        for value in ["42", "7", "999"] {
            upsert_attribute(&conn, "gold", "sess-1", value);
            rollback_session_to_turn(&conn, &session, 5).expect("rollback");
            let gold: String = scalar(
                &conn,
                "SELECT value_json FROM attribute_values WHERE owner_id = 'sess-1' AND schema_id = 'schema-gold'",
                &[],
            );
            assert_eq!(gold, "\"100\"", "第 {value} 次回滚后属性漂移");
            write_snapshot(&conn, &session, 5);
        }
    }

    fn insert_oag_baseline(conn: &Connection) {
        conn.execute_batch(
            "INSERT INTO oag_objects (id, world_id, session_id, object_type, canonical_key, name, name_normalized, created_turn, updated_turn)
             VALUES ('oag-baseline', 'world-1', 'sess-1', 'person', 'person-baseline', 'Alice', 'alice', 1, 1),
                    ('oag-canon', 'world-1', '', 'person', 'person-canon', 'Canon', 'canon', 0, 0);
             INSERT INTO oag_attributes (id, world_id, session_id, object_id, key, value_json, valid_from_turn, valid_to_turn)
             VALUES ('oag-attr', 'world-1', 'sess-1', 'oag-baseline', 'health', '10', 1, 3);",
        )
        .expect("insert OAG baseline");
    }

    #[test]
    fn rollback_restores_oag_intervals_and_preserves_canon() {
        let (conn, mut session) = setup();
        insert_oag_baseline(&conn);
        write_snapshot(&conn, &session, 5);
        conn.execute(
            "UPDATE oag_attributes SET valid_to_turn = NULL WHERE id = 'oag-attr'",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO oag_objects (id, world_id, session_id, object_type, canonical_key, name, name_normalized, created_turn, updated_turn)
             VALUES ('oag-future', 'world-1', 'sess-1', 'person', 'person-future', 'Future', 'future', 5, 5)",
            [],
        )
        .unwrap();
        session.location = "Warehouse".to_string();
        crate::db::repositories::session_repo::SessionRepository::new(&conn)
            .upsert(&session)
            .unwrap();

        let restored = rollback_session_to_turn(&conn, &session, 5).unwrap();
        assert_eq!(restored.location, "Dock");
        let valid_to: Option<i64> = scalar(
            &conn,
            "SELECT valid_to_turn FROM oag_attributes WHERE id = 'oag-attr'",
            &[],
        );
        assert_eq!(valid_to, Some(3));
        let future: i64 = scalar(
            &conn,
            "SELECT COUNT(*) FROM oag_objects WHERE id = 'oag-future'",
            &[],
        );
        assert_eq!(future, 0);
        let canon: i64 = scalar(
            &conn,
            "SELECT COUNT(*) FROM oag_objects WHERE id = 'oag-canon'",
            &[],
        );
        assert_eq!(canon, 1);
        let persisted_location: String = scalar(
            &conn,
            "SELECT location FROM sessions WHERE id = 'sess-1'",
            &[],
        );
        assert_eq!(persisted_location, "Dock");
    }

    #[test]
    fn failed_oag_restore_rolls_back_session_and_memory_changes() {
        let (conn, mut session) = setup();
        insert_oag_baseline(&conn);
        write_snapshot(&conn, &session, 5);
        conn.execute(
            "UPDATE oag_attributes SET valid_to_turn = NULL WHERE id = 'oag-attr'",
            [],
        )
        .unwrap();
        insert_memory(&conn, "mem-after", 5);
        session.location = "Warehouse".to_string();
        crate::db::repositories::session_repo::SessionRepository::new(&conn)
            .upsert(&session)
            .unwrap();
        let payload: String = scalar(
            &conn,
            "SELECT payload_json FROM turn_journal WHERE step = 'snapshot_created'",
            &[],
        );
        let mut payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
        payload["oag_snapshot"]["version"] = serde_json::json!(99);
        conn.execute(
            "UPDATE turn_journal SET payload_json = ?1 WHERE step = 'snapshot_created'",
            [payload.to_string()],
        )
        .unwrap();

        let error = rollback_session_to_turn(&conn, &session, 5).expect_err("should fail");
        assert!(error.contains("快照版本"), "unexpected: {error}");
        let valid_to: Option<i64> = scalar(
            &conn,
            "SELECT valid_to_turn FROM oag_attributes WHERE id = 'oag-attr'",
            &[],
        );
        assert_eq!(valid_to, None);
        let memory: i64 = scalar(
            &conn,
            "SELECT COUNT(*) FROM memories WHERE id = 'mem-after'",
            &[],
        );
        assert_eq!(memory, 1);
        let persisted_location: String = scalar(
            &conn,
            "SELECT location FROM sessions WHERE id = 'sess-1'",
            &[],
        );
        assert_eq!(persisted_location, "Warehouse");
    }

    #[test]
    fn rollback_without_snapshot_is_a_clear_error() {
        let (conn, session) = setup();
        let error = rollback_session_to_turn(&conn, &session, 9).expect_err("should fail");
        assert!(error.contains("回滚快照"), "unexpected: {error}");
    }
}
