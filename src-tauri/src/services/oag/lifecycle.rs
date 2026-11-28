use std::collections::HashMap;

use rusqlite::{params, params_from_iter, Connection};

const SESSION_TABLES: &[(&str, &str)] = &[
    ("oag_objects", "session_id = ?1"),
    ("oag_attributes", "session_id = ?1"),
    ("oag_links", "session_id = ?1"),
    ("oag_scenes", "session_id = ?1"),
    (
        "oag_scene_participants",
        "scene_id IN (SELECT id FROM oag_scenes WHERE session_id = ?1)",
    ),
    ("oag_events", "session_id = ?1"),
    (
        "oag_event_objects",
        "event_id IN (SELECT id FROM oag_events WHERE session_id = ?1)",
    ),
    ("oag_turn_audience", "session_id = ?1"),
    ("oag_knowledge", "session_id = ?1"),
    (
        "oag_memory_objects",
        "memory_id IN (SELECT id FROM memories WHERE session_id = ?1)",
    ),
    ("oag_query_traces", "session_id = ?1"),
    ("oag_write_traces", "session_id = ?1"),
];

const DIRECT_SESSION_TABLES: &[&str] = &[
    "oag_objects",
    "oag_attributes",
    "oag_links",
    "oag_scenes",
    "oag_events",
    "oag_turn_audience",
    "oag_knowledge",
    "oag_query_traces",
    "oag_write_traces",
];

pub fn snapshot_session(conn: &Connection, session_id: &str) -> Result<serde_json::Value, String> {
    let mut tables = serde_json::Map::new();
    for (table, condition) in SESSION_TABLES {
        let rows = query_rows_json(
            conn,
            &format!("SELECT * FROM {table} WHERE {condition}"),
            &[&session_id],
        )?;
        tables.insert((*table).to_string(), serde_json::Value::Array(rows));
    }
    Ok(serde_json::json!({ "version": 1, "tables": tables }))
}

pub fn copy_session(
    conn: &Connection,
    source_session_id: &str,
    target_session_id: &str,
    memory_ids: &HashMap<String, String>,
) -> Result<HashMap<String, String>, String> {
    let snapshot = snapshot_session(conn, source_session_id)?;
    let tables = snapshot_tables(&snapshot)?;
    if snapshot_session(conn, target_session_id)?
        .get("tables")
        .and_then(|value| value.as_object())
        .is_some_and(|tables| {
            tables
                .values()
                .any(|rows| !rows.as_array().is_some_and(Vec::is_empty))
        })
    {
        return Err("分支目标会话已存在 OAG 数据".to_string());
    }

    let mut id_map = HashMap::new();
    for (table, rows) in &tables {
        for row in rows.iter() {
            let Some(old_id) = row.get("id").and_then(|value| value.as_str()) else {
                continue;
            };
            let new_id = uuid::Uuid::new_v5(
                &uuid::Uuid::NAMESPACE_URL,
                format!(
                    "dream-oag-branch:{source_session_id}:{target_session_id}:{table}:{old_id}"
                )
                .as_bytes(),
            )
            .to_string();
            id_map.insert(old_id.to_string(), new_id);
        }
    }

    conn.execute_batch("SAVEPOINT oag_branch_copy")
        .map_err(|error| error.to_string())?;
    let result = (|| {
        conn.execute_batch("PRAGMA defer_foreign_keys = ON")
            .map_err(|error| error.to_string())?;
        for (table, _) in SESSION_TABLES {
            let rows = tables
                .get(table)
                .copied()
                .ok_or_else(|| format!("OAG 快照缺少表：{table}"))?;
            let mut copied = Vec::with_capacity(rows.len());
            for row in rows {
                let mut value = row.clone();
                remap_row(
                    table,
                    &mut value,
                    &id_map,
                    memory_ids,
                    source_session_id,
                    target_session_id,
                )?;
                copied.push(value);
            }
            insert_rows(conn, table, &copied)?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => match conn.execute_batch("RELEASE oag_branch_copy") {
            Ok(()) => Ok(id_map),
            Err(error) => {
                let _ = conn.execute_batch("ROLLBACK TO oag_branch_copy; RELEASE oag_branch_copy");
                Err(error.to_string())
            }
        },
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK TO oag_branch_copy; RELEASE oag_branch_copy");
            Err(error)
        }
    }
}

pub fn remap_branch_value(
    value: &mut serde_json::Value,
    id_map: &HashMap<String, String>,
    source_session_id: &str,
    target_session_id: &str,
) {
    match value {
        serde_json::Value::String(text) => {
            if let Some(target) = id_map.get(text) {
                *text = target.clone();
            } else if text == source_session_id {
                *text = target_session_id.to_string();
            } else if let Some(suffix) = text.strip_prefix(&format!("{source_session_id}:")) {
                *text = format!("{target_session_id}:{suffix}");
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                remap_branch_value(item, id_map, source_session_id, target_session_id);
            }
        }
        serde_json::Value::Object(items) => {
            for item in items.values_mut() {
                remap_branch_value(item, id_map, source_session_id, target_session_id);
            }
        }
        _ => {}
    }
}

pub fn restore_session(
    conn: &Connection,
    session_id: &str,
    snapshot: &serde_json::Value,
) -> Result<(), String> {
    let tables = snapshot_tables(snapshot)?;
    for table in DIRECT_SESSION_TABLES {
        for row in tables[table] {
            if row.get("session_id").and_then(|value| value.as_str()) != Some(session_id) {
                return Err(format!("OAG 快照包含非目标会话数据：{table}"));
            }
        }
    }
    conn.execute_batch("SAVEPOINT oag_restore")
        .map_err(|error| error.to_string())?;
    let result = (|| {
        conn.execute_batch("PRAGMA defer_foreign_keys = ON")
            .map_err(|error| error.to_string())?;
        purge_session(conn, session_id)?;
        for (table, _) in SESSION_TABLES {
            insert_rows(conn, table, &tables[table])?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => match conn.execute_batch("RELEASE oag_restore") {
            Ok(()) => Ok(()),
            Err(error) => {
                let _ = conn.execute_batch("ROLLBACK TO oag_restore; RELEASE oag_restore");
                Err(error.to_string())
            }
        },
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK TO oag_restore; RELEASE oag_restore");
            Err(error)
        }
    }
}

pub fn session_has_data(conn: &Connection, session_id: &str) -> Result<bool, String> {
    let snapshot = snapshot_session(conn, session_id)?;
    Ok(snapshot_tables(&snapshot)?
        .values()
        .any(|rows| !rows.is_empty()))
}

pub fn purge_session(conn: &Connection, session_id: &str) -> Result<(), String> {
    for (table, condition) in SESSION_TABLES.iter().rev() {
        conn.execute(
            &format!("DELETE FROM {table} WHERE {condition}"),
            params![session_id],
        )
        .map_err(|error| format!("清理 OAG 会话数据失败 {table}: {error}"))?;
    }
    Ok(())
}

pub fn purge_world(conn: &Connection, world_id: &str) -> Result<(), String> {
    let statements = [
        "DELETE FROM oag_write_traces WHERE world_id = ?1",
        "DELETE FROM oag_query_traces WHERE world_id = ?1",
        "DELETE FROM oag_memory_objects WHERE memory_id IN (SELECT id FROM memories WHERE world_id = ?1)",
        "DELETE FROM oag_knowledge WHERE world_id = ?1",
        "DELETE FROM oag_turn_audience WHERE world_id = ?1",
        "DELETE FROM oag_event_objects WHERE event_id IN (SELECT id FROM oag_events WHERE world_id = ?1)",
        "DELETE FROM oag_events WHERE world_id = ?1",
        "DELETE FROM oag_scene_participants WHERE scene_id IN (SELECT id FROM oag_scenes WHERE world_id = ?1)",
        "DELETE FROM oag_scenes WHERE world_id = ?1",
        "DELETE FROM oag_links WHERE world_id = ?1",
        "DELETE FROM oag_attributes WHERE world_id = ?1",
        "DELETE FROM oag_objects WHERE world_id = ?1",
        "DELETE FROM oag_legacy_id_map WHERE world_id = ?1",
        "DELETE FROM oag_link_types WHERE world_id = ?1",
        "DELETE FROM oag_object_types WHERE world_id = ?1",
    ];
    for sql in statements {
        conn.execute(sql, params![world_id])
            .map_err(|error| format!("清理 OAG 世界数据失败：{error}"))?;
    }
    Ok(())
}

pub fn purge_all(conn: &Connection) -> Result<(), String> {
    for (table, _) in SESSION_TABLES.iter().rev() {
        conn.execute(&format!("DELETE FROM {table}"), [])
            .map_err(|error| format!("清空 OAG 数据失败 {table}: {error}"))?;
    }
    for table in ["oag_legacy_id_map", "oag_link_types", "oag_object_types"] {
        conn.execute(&format!("DELETE FROM {table}"), [])
            .map_err(|error| format!("清空 OAG 数据失败 {table}: {error}"))?;
    }
    Ok(())
}

pub fn purge_character(conn: &Connection, character_id: &str) -> Result<(), String> {
    let ids: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT id FROM oag_objects WHERE canonical_key = ?1")
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(
                params![format!("runtime-character:{character_id}")],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
    };
    if ids.is_empty() {
        return Ok(());
    }
    let placeholders = std::iter::repeat_n("?", ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let statements = [
        format!("DELETE FROM oag_knowledge WHERE target_kind = 'object' AND target_id IN ({placeholders})"),
        format!("DELETE FROM oag_query_traces WHERE actor_id IN ({placeholders})"),
        format!("DELETE FROM oag_objects WHERE id IN ({placeholders})"),
    ];
    for sql in statements {
        conn.execute(&sql, params_from_iter(ids.iter()))
            .map_err(|error| format!("清理 OAG 角色数据失败：{error}"))?;
    }
    Ok(())
}

fn snapshot_tables(
    snapshot: &serde_json::Value,
) -> Result<HashMap<&str, &Vec<serde_json::Value>>, String> {
    if snapshot.get("version").and_then(|value| value.as_i64()) != Some(1) {
        return Err("OAG 快照版本不受支持".to_string());
    }
    let tables = snapshot
        .get("tables")
        .and_then(|value| value.as_object())
        .ok_or_else(|| "OAG 快照缺少表数据".to_string())?;
    SESSION_TABLES
        .iter()
        .map(|(table, _)| {
            let rows = tables
                .get(*table)
                .and_then(|value| value.as_array())
                .ok_or_else(|| format!("OAG 快照缺少表：{table}"))?;
            Ok((*table, rows))
        })
        .collect()
}

fn remap_row(
    table: &str,
    row: &mut serde_json::Value,
    id_map: &HashMap<String, String>,
    memory_ids: &HashMap<String, String>,
    source_session_id: &str,
    target_session_id: &str,
) -> Result<(), String> {
    let references: &[&str] = match table {
        "oag_objects" => &["asserted_by_id", "source_event_id"],
        "oag_attributes" => &[
            "object_id",
            "asserted_by_id",
            "source_event_id",
            "supersedes_id",
        ],
        "oag_links" => &[
            "from_object_id",
            "to_object_id",
            "asserted_by_id",
            "source_event_id",
            "supersedes_id",
        ],
        "oag_scenes" => &["location_object_id"],
        "oag_scene_participants" => &["scene_id", "object_id"],
        "oag_events" => &["scene_id"],
        "oag_event_objects" => &["event_id", "object_id"],
        "oag_turn_audience" => &["scene_id", "actor_id"],
        "oag_knowledge" => &["actor_id", "source_actor_id", "source_event_id"],
        "oag_memory_objects" => &["object_id"],
        "oag_query_traces" => &["actor_id"],
        _ => &[],
    };
    let object = row
        .as_object_mut()
        .ok_or_else(|| format!("OAG 分支行不是对象：{table}"))?;
    for field in ["id", "session_id"]
        .into_iter()
        .chain(references.iter().copied())
    {
        if let Some(value) = object.get_mut(field).and_then(|value| value.as_str()) {
            let target = if field == "session_id" {
                Some(target_session_id.to_string())
            } else {
                id_map.get(value).cloned()
            };
            if let Some(target) = target {
                object.insert((*field).to_string(), serde_json::Value::String(target));
            }
        }
    }
    if table == "oag_memory_objects" {
        let value = object
            .get("memory_id")
            .and_then(|value| value.as_str())
            .ok_or_else(|| "OAG 记忆关联缺少 memory_id".to_string())?;
        let target = memory_ids
            .get(value)
            .ok_or_else(|| format!("OAG 记忆关联找不到分支记忆：{value}"))?;
        object.insert(
            "memory_id".to_string(),
            serde_json::Value::String(target.clone()),
        );
    }
    if table == "oag_knowledge" {
        let kind = object
            .get("target_kind")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();
        if let Some(value) = object.get_mut("target_id").and_then(|value| value.as_str()) {
            if let Some(target) = id_map.get(value) {
                object.insert(
                    "target_id".to_string(),
                    serde_json::Value::String(target.clone()),
                );
            } else if matches!(kind.as_str(), "object" | "event" | "attribute" | "link") {
                return Err(format!("OAG 知识目标找不到分支对象：{value}"));
            }
        }
    }
    if table == "oag_events" {
        let dedupe = object
            .get("dedupe_key")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();
        let event_id = object
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();
        object.insert(
            "dedupe_key".to_string(),
            serde_json::Value::String(if dedupe.is_empty() {
                event_id
            } else {
                id_map.get(&dedupe).cloned().unwrap_or(dedupe)
            }),
        );
    }
    for field in ["query_json", "result_json", "detail_json"] {
        if let Some(serde_json::Value::String(value)) = object.get_mut(field) {
            let mut parsed = serde_json::from_str(value)
                .map_err(|error| format!("OAG 分支 JSON 无效 {table}.{field}: {error}"))?;
            let mut all_ids = id_map.clone();
            all_ids.extend(
                memory_ids
                    .iter()
                    .map(|(old, new)| (old.clone(), new.clone())),
            );
            remap_branch_value(&mut parsed, &all_ids, source_session_id, target_session_id);
            *value = parsed.to_string();
        }
    }
    Ok(())
}

pub(crate) fn query_rows_json(
    conn: &Connection,
    sql: &str,
    sql_params: &[&dyn rusqlite::ToSql],
) -> Result<Vec<serde_json::Value>, String> {
    let mut stmt = conn.prepare(sql).map_err(|error| error.to_string())?;
    let columns = stmt
        .column_names()
        .iter()
        .map(|name| name.to_string())
        .collect::<Vec<_>>();
    let rows = stmt
        .query_map(sql_params, |row| {
            let mut object = serde_json::Map::new();
            for (index, column) in columns.iter().enumerate() {
                let value = match row.get_ref(index)? {
                    rusqlite::types::ValueRef::Null => serde_json::Value::Null,
                    rusqlite::types::ValueRef::Integer(value) => serde_json::json!(value),
                    rusqlite::types::ValueRef::Real(value) => serde_json::json!(value),
                    rusqlite::types::ValueRef::Text(value) => {
                        serde_json::Value::String(String::from_utf8_lossy(value).into_owned())
                    }
                    rusqlite::types::ValueRef::Blob(value) => {
                        use base64::Engine as _;
                        serde_json::Value::String(
                            base64::engine::general_purpose::STANDARD.encode(value),
                        )
                    }
                };
                object.insert(column.clone(), value);
            }
            Ok(serde_json::Value::Object(object))
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub(crate) fn insert_rows(
    conn: &Connection,
    table: &str,
    rows: &[serde_json::Value],
) -> Result<(), String> {
    for row in rows {
        let object = row
            .as_object()
            .ok_or_else(|| format!("快照行不是对象：{table}"))?;
        let columns = object.keys().map(String::as_str).collect::<Vec<_>>();
        let placeholders = (1..=columns.len())
            .map(|index| format!("?{index}"))
            .collect::<Vec<_>>();
        let values = columns
            .iter()
            .map(|column| json_to_sql_value(&object[*column]))
            .collect::<Vec<_>>();
        conn.execute(
            &format!(
                "INSERT INTO {table} ({}) VALUES ({})",
                columns.join(", "),
                placeholders.join(", ")
            ),
            params_from_iter(values.iter()),
        )
        .map_err(|error| format!("快照写入失败 {table}: {error}"))?;
    }
    Ok(())
}

fn json_to_sql_value(value: &serde_json::Value) -> rusqlite::types::Value {
    match value {
        serde_json::Value::Null => rusqlite::types::Value::Null,
        serde_json::Value::Bool(value) => rusqlite::types::Value::Integer(*value as i64),
        serde_json::Value::Number(value) => value.as_i64().map_or_else(
            || rusqlite::types::Value::Real(value.as_f64().unwrap_or_default()),
            rusqlite::types::Value::Integer,
        ),
        serde_json::Value::String(value) => rusqlite::types::Value::Text(value.clone()),
        other => rusqlite::types::Value::Text(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{oag_schema, schema};

    fn database() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        schema::create_tables(&conn).unwrap();
        oag_schema::create(&conn).unwrap();
        conn
    }

    fn insert_fixture(conn: &Connection, session: &str) {
        conn.execute(
            "INSERT INTO oag_objects (id, world_id, session_id, object_type, canonical_key, name, name_normalized, created_turn, updated_turn)
             VALUES (?1, 'world', ?2, 'person', 'runtime-character:alice', 'Alice', 'alice', 1, 1)",
            params![format!("object-{session}"), session],
        ).unwrap();
        conn.execute(
            "INSERT INTO oag_objects (id, world_id, session_id, object_type, canonical_key, name, name_normalized, created_turn, updated_turn)
             VALUES ('canon', 'world', '', 'person', 'canon', 'Canon', 'canon', 0, 0)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO oag_events (id, world_id, session_id, turn_index, dedupe_key)
             VALUES (?1, 'world', ?2, 1, ?1)",
            params![format!("event-{session}"), session],
        )
        .unwrap();
        conn.execute(
            "UPDATE oag_objects SET source_event_id = ?1 WHERE id = ?2",
            params![format!("event-{session}"), format!("object-{session}")],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO oag_attributes (id, world_id, session_id, object_id, key, value_json, valid_from_turn, valid_to_turn)
             VALUES (?1, 'world', ?2, ?3, 'health', '10', 1, 3)",
            params![format!("attribute-{session}"), session, format!("object-{session}")],
        ).unwrap();
        conn.execute(
            "INSERT INTO memories (id, world_id, session_id, character_id, layer, content, source, importance, created_at, turn_index)
             VALUES (?1, 'world', ?2, 'alice', 'long_term', 'memory', 'test', 1, 'now', 1)",
            params![format!("memory-{session}"), session],
        ).unwrap();
        conn.execute(
            "INSERT INTO oag_memory_objects (memory_id, object_id, role) VALUES (?1, ?2, 'mentioned')",
            params![format!("memory-{session}"), format!("object-{session}")],
        ).unwrap();
        conn.execute(
            "INSERT INTO oag_knowledge (id, world_id, session_id, actor_id, target_kind, target_id, source_event_id, learned_turn)
             VALUES (?1, 'world', ?2, ?3, 'event', ?4, ?4, 1)",
            params![format!("knowledge-{session}"), session, format!("object-{session}"), format!("event-{session}")],
        ).unwrap();
    }

    #[test]
    fn branch_remaps_graph_and_memory_references_without_copying_canon() {
        let conn = database();
        insert_fixture(&conn, "source");
        let memory_ids =
            HashMap::from([("memory-source".to_string(), "memory-target".to_string())]);
        conn.execute(
            "INSERT INTO memories (id, world_id, session_id, character_id, layer, content, source, importance, created_at, turn_index)
             VALUES ('memory-target', 'world', 'target', 'alice', 'long_term', 'memory', 'test', 1, 'now', 1)",
            [],
        ).unwrap();
        let id_map = copy_session(&conn, "source", "target", &memory_ids).unwrap();
        assert_ne!(id_map["object-source"], "object-source");
        let object_id: String = conn
            .query_row(
                "SELECT source_event_id FROM oag_objects WHERE session_id = 'target'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(object_id, id_map["event-source"]);
        let memory_id: String = conn
            .query_row(
                "SELECT memory_id FROM oag_memory_objects WHERE object_id = ?1",
                [&id_map["object-source"]],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(memory_id, "memory-target");
        let canon_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM oag_objects WHERE id = 'canon' AND session_id = ''",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(canon_count, 1);
        let target_canon_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM oag_objects WHERE session_id = 'target' AND id = 'canon'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(target_canon_count, 0);
    }

    #[test]
    fn restore_reopens_closed_intervals_and_preserves_canon() {
        let conn = database();
        insert_fixture(&conn, "source");
        let snapshot = snapshot_session(&conn, "source").unwrap();
        conn.execute(
            "UPDATE oag_attributes SET valid_to_turn = NULL WHERE session_id = 'source'",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO oag_events (id, world_id, session_id, turn_index, dedupe_key) VALUES ('future', 'world', 'source', 9, 'future')", []).unwrap();
        restore_session(&conn, "source", &snapshot).unwrap();
        let end: Option<i64> = conn
            .query_row(
                "SELECT valid_to_turn FROM oag_attributes WHERE session_id = 'source'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(end, Some(3));
        let future: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM oag_events WHERE id = 'future'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(future, 0);
        let canon: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM oag_objects WHERE id = 'canon'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(canon, 1);
    }

    #[test]
    fn purge_session_world_and_all_leave_no_orphans() {
        let conn = database();
        insert_fixture(&conn, "source");
        purge_character(&conn, "alice").unwrap();
        assert_eq!(count(&conn, "oag_objects WHERE id = 'object-source'"), 0);
        assert!(session_has_data(&conn, "source").unwrap());
        purge_session(&conn, "source").unwrap();
        assert!(!session_has_data(&conn, "source").unwrap());
        purge_world(&conn, "world").unwrap();
        assert_eq!(count(&conn, "oag_objects"), 0);
        insert_fixture(&conn, "other");
        purge_all(&conn).unwrap();
        assert_eq!(count(&conn, "oag_objects"), 0);
        assert_eq!(count(&conn, "oag_memory_objects"), 0);
    }

    fn count(conn: &Connection, query: &str) -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {query}"), [], |row| {
            row.get(0)
        })
        .unwrap()
    }
}
