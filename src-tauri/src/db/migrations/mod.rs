use rusqlite::{params, Connection};
use serde_json::Value;

use crate::db::seeds::feihualing_world::SEED_WORLD_POETRY_ID;
use crate::db::seeds::schedule_assistant_world::SEED_WORLD_SCHEDULE_ASSISTANT_ID;

const MIGRATION_LEGACY_COLUMNS: i64 = 1;
const MIGRATION_HOME_BACKGROUND_STRATEGY: i64 = 2;
const MIGRATION_BUILTIN_DESKTOP_UI_REPAIR: i64 = 3;
const MIGRATION_SCHEDULE_ATTRIBUTE_REPAIR: i64 = 4;
const MIGRATION_EMBEDDING_MODEL_NAME_REPAIR: i64 = 5;
const MIGRATION_MEMORY_LAYER_DEDUP: i64 = 6;
const CURRENT_SCHEMA_VERSION: i64 = MIGRATION_MEMORY_LAYER_DEDUP;

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

fn table_exists(conn: &Connection, table_name: &str) -> Result<bool, rusqlite::Error> {
    let count = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        params![table_name],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(count > 0)
}

fn dedup_memory_layer_copies(conn: &Connection) -> Result<(), rusqlite::Error> {
    // M6 之前的对话记忆会把同一段内容按 working/short_term/archive 三层各写一份
    // (仅 id 与 layer 不同)。改为单层写入 + 召回时推算有效层之后,这些副本只剩冗余,
    // 且同一文本会以多条 id 不同的记录重复进入召回结果。
    // 这里按 (world, session, character, turn, content, source, speaker) 分组,
    // 仅当组内存在多个不同 layer 时删除多余副本,优先保留 working 那条。
    // 缺表(迁移测试的遗留夹具)时直接跳过。
    if !table_exists(conn, "memories")? {
        return Ok(());
    }
    let mut stmt = conn.prepare(
        "SELECT id FROM (
           SELECT m.id,
             ROW_NUMBER() OVER (
               PARTITION BY m.world_id, m.session_id, m.character_id,
                            m.turn_index, m.content, m.source, m.speaker
               ORDER BY CASE m.layer
                          WHEN 'working' THEN 0
                          WHEN 'short_term' THEN 1
                          WHEN 'archive' THEN 2
                          ELSE 3 END, m.id
             ) AS rn
           FROM memories m
           JOIN (
             SELECT world_id, session_id, character_id, turn_index, content, source, speaker
             FROM memories
             GROUP BY world_id, session_id, character_id, turn_index, content, source, speaker
             HAVING COUNT(DISTINCT layer) > 1
           ) dup
             ON m.world_id = dup.world_id
            AND m.session_id = dup.session_id
            AND m.character_id = dup.character_id
            AND m.turn_index = dup.turn_index
            AND m.content = dup.content
            AND m.source = dup.source
            AND m.speaker IS dup.speaker
         ) WHERE rn > 1",
    )?;
    let duplicate_ids = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let embeddings_table_exists = table_exists(conn, "memory_embeddings")?;
    for id in duplicate_ids {
        if embeddings_table_exists {
            // 不赌 PRAGMA foreign_keys 是否开启,显式清理向量缓存。
            conn.execute(
                "DELETE FROM memory_embeddings WHERE memory_id = ?1",
                params![id],
            )?;
        }
        conn.execute("DELETE FROM memories WHERE id = ?1", params![id])?;
    }
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
    if version < MIGRATION_MEMORY_LAYER_DEDUP {
        dedup_memory_layer_copies(&tx)?;
        set_schema_version(&tx, MIGRATION_MEMORY_LAYER_DEDUP)?;
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
            CREATE TABLE memories (
                id TEXT PRIMARY KEY,
                world_id TEXT NOT NULL DEFAULT '',
                session_id TEXT NOT NULL DEFAULT '',
                character_id TEXT NOT NULL DEFAULT '',
                layer TEXT NOT NULL DEFAULT 'working',
                content TEXT NOT NULL DEFAULT '',
                source TEXT NOT NULL DEFAULT '',
                turn_index INTEGER NOT NULL DEFAULT 0,
                speaker TEXT
            );
            CREATE TABLE memory_embeddings (
                memory_id TEXT NOT NULL,
                model_key TEXT NOT NULL,
                vector_json TEXT NOT NULL DEFAULT '[]',
                PRIMARY KEY (memory_id, model_key)
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

    fn insert_legacy_memory(
        conn: &Connection,
        id: &str,
        layer: &str,
        content: &str,
        turn_index: i64,
    ) {
        conn.execute(
            "INSERT INTO memories (id, world_id, session_id, character_id, layer, content, source, turn_index, speaker)
             VALUES (?1, 'world-1', 'sess-1', 'char-a', ?2, ?3, 'speaker_response', ?4, 'Alice')",
            params![id, layer, content, turn_index],
        )
        .expect("insert legacy memory");
    }

    fn memory_count(conn: &Connection) -> i64 {
        conn.query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))
            .expect("count memories")
    }

    #[test]
    fn memory_layer_dedup_removes_triplicates_once() {
        let conn = Connection::open_in_memory().expect("open database");
        create_legacy_schema(&conn);

        // 三层副本组:同一内容 working/short_term/archive 各一份。
        insert_legacy_memory(&conn, "m-work", "working", "钥匙在12号柜", 3);
        insert_legacy_memory(&conn, "m-short", "short_term", "钥匙在12号柜", 3);
        insert_legacy_memory(&conn, "m-arch", "archive", "钥匙在12号柜", 3);
        // 单行记忆:不层叠,不应被动。
        insert_legacy_memory(&conn, "m-solo", "working", "另一条独立记忆", 4);
        for id in ["m-work", "m-short", "m-arch"] {
            conn.execute(
                "INSERT INTO memory_embeddings (memory_id, model_key, vector_json)
                 VALUES (?1, 'model-1', '[0.1, 0.2]')",
                params![id],
            )
            .expect("insert embedding");
        }

        dedup_memory_layer_copies(&conn).expect("dedup memories");

        assert_eq!(memory_count(&conn), 2);
        let remaining: Vec<(String, String)> = conn
            .prepare("SELECT id, layer FROM memories ORDER BY id")
            .expect("prepare select")
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("query memories")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect memories");
        assert_eq!(
            remaining,
            vec![
                ("m-solo".to_string(), "working".to_string()),
                ("m-work".to_string(), "working".to_string()),
            ],
            "三副本组应只保留 working 那条"
        );
        let embedding_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM memory_embeddings", [], |row| {
                row.get(0)
            })
            .expect("count embeddings");
        assert_eq!(
            embedding_count, 1,
            "被删副本的向量缓存应一并清理,只留保留行的"
        );

        // 幂等:再跑一次不应再删任何东西。
        dedup_memory_layer_copies(&conn).expect("rerun dedup");
        assert_eq!(memory_count(&conn), 2);
    }

    #[test]
    fn memory_layer_dedup_skips_missing_table() {
        let conn = Connection::open_in_memory().expect("open database");
        conn.execute("CREATE TABLE characters (id TEXT PRIMARY KEY)", [])
            .expect("create partial schema");
        dedup_memory_layer_copies(&conn).expect("missing memories table should be fine");
    }

    #[test]
    fn migration_failure_rolls_back_schema_and_version() {        let conn = Connection::open_in_memory().expect("open database");
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
