use rusqlite::Connection;

fn ensure_column(
    conn: &Connection,
    table_name: &str,
    column_name: &str,
    column_definition: &str,
) -> Result<(), rusqlite::Error> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table_name})"))?;
    let existing_columns = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for column in existing_columns {
        if column? == column_name {
            return Ok(());
        }
    }

    conn.execute(
        &format!("ALTER TABLE {table_name} ADD COLUMN {column_name} {column_definition}"),
        [],
    )?;
    Ok(())
}

pub(crate) fn run(conn: &Connection) -> Result<(), rusqlite::Error> {
    ensure_column(
        conn,
        "characters",
        "system_prompt_template",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        conn,
        "characters",
        "response_contract_prompt",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        conn,
        "characters",
        "narration_prompt",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        conn,
        "settings",
        "embedding_enabled",
        "INTEGER NOT NULL DEFAULT 1",
    )?;
    ensure_column(
        conn,
        "settings",
        "default_embedding_model",
        "TEXT NOT NULL DEFAULT 'BAAI/bge-small-zh-v1.5'",
    )?;
    ensure_column(
        conn,
        "model_configs",
        "max_tokens",
        "INTEGER NOT NULL DEFAULT 1200",
    )?;
    ensure_column(
        conn,
        "model_configs",
        "streaming_enabled",
        "INTEGER NOT NULL DEFAULT 1",
    )?;
    ensure_column(
        conn,
        "llm_call_traces",
        "provider",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        conn,
        "llm_call_traces",
        "model_id",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        conn,
        "llm_call_traces",
        "status",
        "TEXT NOT NULL DEFAULT 'completed'",
    )?;
    ensure_column(
        conn,
        "llm_call_traces",
        "latency_ms",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(conn, "saves", "turn_index", "INTEGER NOT NULL DEFAULT 0")?;
    conn.execute(
        "UPDATE settings SET home_background_strategy = '' WHERE home_background_strategy = 'static'",
        [],
    )?;
    Ok(())
}
