//! H1: 删除世界 / 角色 / 存档时的关联数据显式清理。
//!
//! 背景:`sessions`、`memories`、`saves`、`attribute_values`(owner 为 session/character 的行)、
//! `turn_journal`、`*_traces`、`llm_retry_capsules`、`agent_sessions`、`scheduled_notifications`
//! 等表均未对父表建立外键,删除世界/角色/存档后这些行会成为永久孤儿,污染语义检索、
//! 使库无限膨胀,且 `attribute_values` 的 `UNIQUE(schema_id, owner_type, owner_id)` 会让
//! 新建同 id 实体读到旧残留属性值。这里在应用层显式清理这些关联行。
//!
//! 注意:`memory_embeddings`(FK→memories ON DELETE CASCADE)与
//! `agent_checkpoints`(FK→agent_sessions ON DELETE CASCADE)在 `PRAGMA foreign_keys=ON`
//! 下会随父行删除自动级联,无需手工处理。

use rusqlite::{params, Connection};

/// 清理单个会话产生的全部关联数据(含会话行本身)。
/// 删除顺序:先删子表(memories 触发 embeddings 级联),最后删 sessions / saves。
pub fn purge_session_data(conn: &Connection, session_id: &str) -> Result<(), String> {
    // memories(其 memory_embeddings 经 FK 级联一并删除)
    exec(
        conn,
        "DELETE FROM memories WHERE session_id = ?1",
        params![session_id],
    )?;
    // 会话级属性
    exec(
        conn,
        "DELETE FROM attribute_values WHERE owner_type = 'session' AND owner_id = ?1",
        params![session_id],
    )?;
    // 会话内角色属性,owner_id 形如 "{session_id}:{character_id}"
    exec(
        conn,
        "DELETE FROM attribute_values WHERE owner_type = 'session_character' AND owner_id LIKE ?1 ESCAPE '\\'",
        params![format!("{}:%", escape_like(session_id))],
    )?;
    // 回合日志与各类调用追踪
    exec(
        conn,
        "DELETE FROM turn_journal WHERE session_id = ?1",
        params![session_id],
    )?;
    exec(
        conn,
        "DELETE FROM prompt_call_traces WHERE session_id = ?1",
        params![session_id],
    )?;
    exec(
        conn,
        "DELETE FROM llm_call_traces WHERE session_id = ?1",
        params![session_id],
    )?;
    exec(
        conn,
        "DELETE FROM llm_retry_capsules WHERE session_id = ?1",
        params![session_id],
    )?;
    // agent 会话(其 agent_checkpoints 经 FK 级联一并删除)
    exec(
        conn,
        "DELETE FROM agent_sessions WHERE session_id = ?1",
        params![session_id],
    )?;
    // 该会话的定时通知
    exec(
        conn,
        "DELETE FROM scheduled_notifications WHERE session_id = ?1",
        params![session_id],
    )?;
    // 指向该会话的存档书签
    exec(
        conn,
        "DELETE FROM saves WHERE session_id = ?1",
        params![session_id],
    )?;
    // 会话行本身
    exec(
        conn,
        "DELETE FROM sessions WHERE id = ?1",
        params![session_id],
    )?;
    Ok(())
}

/// 清理某角色在所有会话中残留的属性与记忆(角色行本身由调用方删除)。
pub fn purge_character_data(conn: &Connection, character_id: &str) -> Result<(), String> {
    // 角色级属性
    exec(
        conn,
        "DELETE FROM attribute_values WHERE owner_type = 'character' AND owner_id = ?1",
        params![character_id],
    )?;
    // 各会话内该角色的属性,owner_id 形如 "{session_id}:{character_id}"
    exec(
        conn,
        "DELETE FROM attribute_values WHERE owner_type = 'session_character' AND owner_id LIKE ?1 ESCAPE '\\'",
        params![format!("%:{}", escape_like(character_id))],
    )?;
    // 该角色的记忆(memory_embeddings 经 FK 级联)
    exec(
        conn,
        "DELETE FROM memories WHERE character_id = ?1",
        params![character_id],
    )?;
    Ok(())
}

/// 清理一个世界下、按 world_name 关联的所有会话数据。
/// characters / attribute_values(owner_type='world')由各自路径处理。
pub fn purge_world_sessions(conn: &Connection, world_name: &str) -> Result<(), String> {
    let session_ids = collect_session_ids_by_world_name(conn, world_name)?;
    for session_id in session_ids {
        purge_session_data(conn, &session_id)?;
    }
    Ok(())
}

/// 清理世界级属性(owner_type='world')。
pub fn purge_world_attributes(conn: &Connection, world_id: &str) -> Result<(), String> {
    exec(
        conn,
        "DELETE FROM attribute_values WHERE owner_type = 'world' AND owner_id = ?1",
        params![world_id],
    )?;
    Ok(())
}

/// 清空所有会话衍生数据(删除全部世界时使用)。保留 attribute_schemas、model_configs 等全局配置。
pub fn purge_all_session_scoped_data(conn: &Connection) -> Result<(), String> {
    for sql in [
        "DELETE FROM memories",
        "DELETE FROM attribute_values WHERE owner_type IN ('session', 'session_character', 'character', 'world')",
        "DELETE FROM turn_journal",
        "DELETE FROM prompt_call_traces",
        "DELETE FROM llm_call_traces",
        "DELETE FROM llm_retry_capsules",
        "DELETE FROM agent_sessions",
        "DELETE FROM scheduled_notifications",
        "DELETE FROM saves",
        "DELETE FROM sessions",
    ] {
        exec(conn, sql, params![])?;
    }
    Ok(())
}

fn collect_session_ids_by_world_name(
    conn: &Connection,
    world_name: &str,
) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare("SELECT id FROM sessions WHERE world_name = ?1")
        .map_err(|e| e.to_string())?;
    let ids = stmt
        .query_map(params![world_name], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(ids)
}

fn exec(conn: &Connection, sql: &str, params: &[&dyn rusqlite::types::ToSql]) -> Result<(), String> {
    conn.execute(sql, params).map_err(|e| e.to_string())?;
    Ok(())
}

/// 转义 LIKE 模式中的特殊字符,避免 id 中的 `%`/`_`/`\` 造成误匹配。
fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}
