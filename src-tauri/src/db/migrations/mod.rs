use rusqlite::{params, Connection};
use serde_json::Value;

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

fn repair_desktop_ui_question_marks(conn: &Connection) -> Result<(), rusqlite::Error> {
    let mut stmt = conn.prepare("SELECT id, ui_theme_config_json FROM worlds")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let world_configs = rows.collect::<Result<Vec<_>, _>>()?;

    for (world_id, raw_config) in world_configs {
        let Ok(mut config) = serde_json::from_str::<Value>(&raw_config) else {
            continue;
        };

        let Some(desktop_file_value) = config.get_mut("desktop_file") else {
            continue;
        };
        let Some(desktop_file) = desktop_file_value.as_str() else {
            continue;
        };

        let repaired = desktop_file
            .replace(
                "\"minmax(0, 1fr)\",\n        \"88px\",\n        \"420px\"",
                "\"minmax(0, 1fr)\",\n        \"156px\",\n        \"420px\"",
            )
            .replace(
                ".game-scene-center::before {\\n  content: \\\"????\\\";",
                ".game-scene-center::before {\\n  content: \\\"\\\\5f53\\\\524d\\\\573a\\\\666f\\\";",
            )
            .replace(
                ".game-input-area::before {\\n  content: \\\"????\\\";",
                ".game-input-area::before {\\n  content: \\\"\\\\884c\\\\52a8\\\\8f93\\\\5165\\\";",
            )
            .replace(
                ".schedule-d-header .game-title-group {\\n  position: relative;\\n  min-width: 0;\\n  padding-left: 68px;\\n}",
                ".schedule-d-header .game-title-group {\\n  position: relative;\\n  min-width: 0;\\n  padding-left: 68px;\\n  transform: translateY(4px);\\n}",
            )
            .replace(
                ".schedule-d-header .game-title-group::after {\\n  content: \\\"?????????\\\";",
                ".schedule-d-header .game-title-group::after {\\n  content: \\\"\\\\667a\\\\80fd\\\\65e5\\\\7a0b\\\\4e0e\\\\63d0\\\\9192\\\\52a9\\\\624b\\\";",
            )
            .replace(
                ".schedule-d-side .game-status::before {\\n  content: \\\"????\\\\A?????????\\\";",
                ".schedule-d-side .game-status::before {\\n  content: \\\"\\\\72b6\\\\6001\\\\9762\\\\677f\\\\A\\\\5f85\\\\529e\\\\4e8b\\\\9879\\\\4e0e\\\\63d0\\\\9192\\\";",
            )
            .replace(
                "\"show_image_button\": false,\n                \"show_audio_button\": false",
                "\"show_image_button\": false,\n                \"show_audio_button\": true",
            )
            .replace(
                ".schedule-d-actions .game-back-btn,\\n&.game-root .schedule-d-actions .game-quick-btn {\\n  min-width: 40px;\\n  height: 40px;\\n  padding: 0 12px;\\n  border-radius: 14px;\\n  border: 1px solid #e5e5e5;\\n  background: rgba(245,245,245,0.72);\\n  color: #737373;\\n}",
                ".schedule-d-actions .game-back-btn,\\n&.game-root .schedule-d-actions .game-quick-btn {\\n  min-width: 40px;\\n  height: 40px;\\n  padding: 0 12px;\\n  border-radius: 14px;\\n  border: 1px solid #e5e5e5;\\n  background: rgba(245,245,245,0.72);\\n  color: #737373;\\n}\\n&.game-root .schedule-d-actions .game-back-btn {\\n  width: 40px;\\n  min-width: 40px;\\n  padding: 0;\\n}\\n&.game-root .schedule-d-actions .game-quick-btn {\\n  min-width: 72px;\\n  padding: 0 18px;\\n  white-space: nowrap;\\n}",
            );

        if repaired == desktop_file {
            continue;
        }

        *desktop_file_value = Value::String(repaired);
        let repaired_config = serde_json::to_string(&config).unwrap_or(raw_config);
        conn.execute(
            "UPDATE worlds SET ui_theme_config_json = ?1 WHERE id = ?2",
            params![repaired_config, world_id],
        )?;
    }

    Ok(())
}

fn repair_schedule_status_attribute_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE attribute_schemas
         SET value_type = 'list',
             default_value_json = '[]'
         WHERE id = 'attr-schedule-assistant-notifications'",
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
    repair_desktop_ui_question_marks(conn)?;
    repair_schedule_status_attribute_schema(conn)?;
    Ok(())
}
