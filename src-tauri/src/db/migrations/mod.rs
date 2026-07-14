use rusqlite::{params, Connection};
use serde_json::Value;

use crate::db::seeds::feihualing_world::SEED_WORLD_POETRY_ID;
use crate::db::seeds::schedule_assistant_world::SEED_WORLD_SCHEDULE_ASSISTANT_ID;

const MIGRATION_LEGACY_COLUMNS: i64 = 1;
const MIGRATION_HOME_BACKGROUND_STRATEGY: i64 = 2;
const MIGRATION_BUILTIN_DESKTOP_UI_REPAIR: i64 = 3;
const MIGRATION_SCHEDULE_ATTRIBUTE_REPAIR: i64 = 4;
const MIGRATION_EMBEDDING_MODEL_NAME_REPAIR: i64 = 5;
const CURRENT_SCHEMA_VERSION: i64 = MIGRATION_EMBEDDING_MODEL_NAME_REPAIR;

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
    let mut stmt = conn.prepare(
        "SELECT id, ui_theme_config_json
         FROM worlds
         WHERE id IN (?1, ?2)",
    )?;
    let rows = stmt.query_map(
        params![SEED_WORLD_POETRY_ID, SEED_WORLD_SCHEDULE_ASSISTANT_ID],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )?;
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
    // M4: 仅修复仍处于损坏状态(value_type 不是 'list')的行,带守卫避免每次启动无条件
    // 覆盖,从而不会把用户对该 schema 的修改静默回滚(与 repair_garbled_embedding_model_name
    // 的 `name != …` 守卫思路一致)。
    conn.execute(
        "UPDATE attribute_schemas
         SET value_type = 'list',
             default_value_json = '[]'
         WHERE id = 'attr-schedule-assistant-notifications'
           AND value_type <> 'list'",
        [],
    )?;
    Ok(())
}

fn add_legacy_columns(conn: &Connection) -> Result<(), rusqlite::Error> {
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
        "characters",
        "avatar_asset",
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
    ensure_column(
        conn,
        "mcp_tools",
        "input_schema_json",
        "TEXT NOT NULL DEFAULT '{\"type\":\"object\",\"properties\":{}}'",
    )?;
    Ok(())
}

fn repair_home_background_strategy(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE settings SET home_background_strategy = '' WHERE home_background_strategy = 'static'",
        [],
    )?;
    Ok(())
}

fn repair_garbled_embedding_model_name(conn: &Connection) -> Result<(), rusqlite::Error> {
    // The builtin-local embedding model was seeded with a corrupted UTF-8 name.
    // Match by id and provider to be safe.
    conn.execute(
        "UPDATE model_configs
         SET name = '内置本地 Embedding / bge-small-zh-v1.5'
         WHERE id = 'model-seed-bge-small-embedding'
           AND provider = 'builtin-local'
           AND name != '内置本地 Embedding / bge-small-zh-v1.5'",
        [],
    )?;
    Ok(())
}

fn schema_version(conn: &Connection) -> Result<i64, rusqlite::Error> {
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
}

fn set_schema_version(conn: &Connection, version: i64) -> Result<(), rusqlite::Error> {
    conn.pragma_update(None, "user_version", version)
}

pub(crate) fn run(conn: &Connection) -> Result<(), rusqlite::Error> {
    if schema_version(conn)? >= CURRENT_SCHEMA_VERSION {
        return Ok(());
    }

    let tx = conn.unchecked_transaction()?;
    let mut version = schema_version(&tx)?;

    if version < MIGRATION_LEGACY_COLUMNS {
        add_legacy_columns(&tx)?;
        version = MIGRATION_LEGACY_COLUMNS;
        set_schema_version(&tx, version)?;
    }
    if version < MIGRATION_HOME_BACKGROUND_STRATEGY {
        repair_home_background_strategy(&tx)?;
        version = MIGRATION_HOME_BACKGROUND_STRATEGY;
        set_schema_version(&tx, version)?;
    }
    if version < MIGRATION_BUILTIN_DESKTOP_UI_REPAIR {
        repair_desktop_ui_question_marks(&tx)?;
        version = MIGRATION_BUILTIN_DESKTOP_UI_REPAIR;
        set_schema_version(&tx, version)?;
    }
    if version < MIGRATION_SCHEDULE_ATTRIBUTE_REPAIR {
        repair_schedule_status_attribute_schema(&tx)?;
        version = MIGRATION_SCHEDULE_ATTRIBUTE_REPAIR;
        set_schema_version(&tx, version)?;
    }
    if version < MIGRATION_EMBEDDING_MODEL_NAME_REPAIR {
        repair_garbled_embedding_model_name(&tx)?;
        set_schema_version(&tx, MIGRATION_EMBEDDING_MODEL_NAME_REPAIR)?;
    }

    tx.commit()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_legacy_schema(conn: &Connection) {
        conn.execute_batch(
            "
            CREATE TABLE characters (id TEXT PRIMARY KEY);
            CREATE TABLE settings (
                id INTEGER PRIMARY KEY,
                home_background_strategy TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE model_configs (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                provider TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE llm_call_traces (id TEXT PRIMARY KEY);
            CREATE TABLE saves (id TEXT PRIMARY KEY);
            CREATE TABLE mcp_tools (id TEXT PRIMARY KEY);
            CREATE TABLE worlds (
                id TEXT PRIMARY KEY,
                ui_theme_config_json TEXT NOT NULL DEFAULT '{}'
            );
            CREATE TABLE attribute_schemas (
                id TEXT PRIMARY KEY,
                value_type TEXT NOT NULL DEFAULT 'text',
                default_value_json TEXT NOT NULL DEFAULT 'null'
            );
            ",
        )
        .expect("create legacy schema");
    }

    fn column_exists(conn: &Connection, table_name: &str, column_name: &str) -> bool {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({table_name})"))
            .expect("prepare table info");
        let mut columns = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .expect("query table info");
        columns.any(|column| column.expect("column name") == column_name)
    }

    fn desktop_file(conn: &Connection, world_id: &str) -> String {
        let raw: String = conn
            .query_row(
                "SELECT ui_theme_config_json FROM worlds WHERE id = ?1",
                params![world_id],
                |row| row.get(0),
            )
            .expect("world config");
        serde_json::from_str::<Value>(&raw)
            .expect("valid world config")
            .get("desktop_file")
            .and_then(Value::as_str)
            .expect("desktop file")
            .to_string()
    }

    #[test]
    fn versioned_migrations_upgrade_legacy_database_once() {
        let conn = Connection::open_in_memory().expect("open database");
        create_legacy_schema(&conn);

        let damaged_desktop = ".game-scene-center::before {\\n  content: \\\"????\\\";";
        let damaged_config = serde_json::json!({ "desktop_file": damaged_desktop }).to_string();
        conn.execute(
            "INSERT INTO settings (id, home_background_strategy) VALUES (1, 'static')",
            [],
        )
        .expect("insert settings");
        conn.execute(
            "INSERT INTO model_configs (id, name, provider)
             VALUES ('model-seed-bge-small-embedding', 'garbled', 'builtin-local')",
            [],
        )
        .expect("insert model");
        conn.execute(
            "INSERT INTO attribute_schemas (id, value_type, default_value_json)
             VALUES ('attr-schedule-assistant-notifications', 'text', 'null')",
            [],
        )
        .expect("insert attribute schema");
        conn.execute(
            "INSERT INTO worlds (id, ui_theme_config_json) VALUES (?1, ?2)",
            params![SEED_WORLD_POETRY_ID, damaged_config],
        )
        .expect("insert seed world");
        conn.execute(
            "INSERT INTO worlds (id, ui_theme_config_json) VALUES ('user-world', ?1)",
            params![damaged_config],
        )
        .expect("insert user world");

        run(&conn).expect("run migrations");

        assert_eq!(
            schema_version(&conn).expect("schema version"),
            CURRENT_SCHEMA_VERSION
        );
        assert!(column_exists(&conn, "characters", "system_prompt_template"));
        assert!(column_exists(&conn, "settings", "embedding_enabled"));
        assert!(column_exists(&conn, "mcp_tools", "input_schema_json"));
        assert_eq!(
            conn.query_row(
                "SELECT home_background_strategy FROM settings WHERE id = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .expect("background strategy"),
            ""
        );
        assert_eq!(
            conn.query_row(
                "SELECT value_type FROM attribute_schemas
                 WHERE id = 'attr-schedule-assistant-notifications'",
                [],
                |row| row.get::<_, String>(0),
            )
            .expect("attribute type"),
            "list"
        );
        assert_eq!(
            conn.query_row(
                "SELECT name FROM model_configs
                 WHERE id = 'model-seed-bge-small-embedding'",
                [],
                |row| row.get::<_, String>(0),
            )
            .expect("model name"),
            "内置本地 Embedding / bge-small-zh-v1.5"
        );
        assert!(desktop_file(&conn, SEED_WORLD_POETRY_ID).contains("\\\\5f53"));
        assert_eq!(desktop_file(&conn, "user-world"), damaged_desktop);

        conn.execute(
            "UPDATE settings SET home_background_strategy = 'static' WHERE id = 1",
            [],
        )
        .expect("change migrated data");
        run(&conn).expect("rerun migrations");
        assert_eq!(
            conn.query_row(
                "SELECT home_background_strategy FROM settings WHERE id = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .expect("background strategy after rerun"),
            "static"
        );
    }

    #[test]
    fn migration_failure_rolls_back_schema_and_version() {
        let conn = Connection::open_in_memory().expect("open database");
        conn.execute("CREATE TABLE characters (id TEXT PRIMARY KEY)", [])
            .expect("create partial legacy schema");

        let error = run(&conn).expect_err("missing tables should fail migration");

        assert!(error.to_string().contains("settings"));
        assert!(!column_exists(
            &conn,
            "characters",
            "system_prompt_template"
        ));
        assert_eq!(schema_version(&conn).expect("schema version"), 0);
    }
}
