use std::path::Path;

use rusqlite::{params, Connection};

use crate::services::catalog::world_service::WorldService;

const BUNDLED_MORTAL_WORLD_ZIP: &[u8] =
    include_bytes!("assets/mortal-cultivation-journey-world.zip");
const BUNDLED_MORTAL_WORLD_NAME: &str = "凡人修仙：小石村问道";
const BUNDLED_WORLD_NAMESPACE: &str = "seed.bundled_world";
const BUNDLED_MORTAL_WORLD_KEY: &str = "mortal-cultivation-journey";

/// 包内容指纹：随内置 zip 变化而变化。旧版只写 "true"，没有指纹，
/// 因此老用户升级后会被视为「需要刷新」，从而把旧界面（JSONC 时代）换掉。
fn package_fingerprint(zip: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in zip {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{:x}-{:x}", zip.len(), hash)
}

fn stored_bundled_marker(conn: &Connection, key: &str) -> Result<Option<String>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT value_json FROM scoped_kv
         WHERE owner_type = 'app' AND owner_id = '' AND namespace = ?1 AND key = ?2",
    )?;
    let mut rows = stmt.query(params![BUNDLED_WORLD_NAMESPACE, key])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    let raw: String = row.get(0)?;
    Ok(Some(raw.trim_matches('"').to_string()))
}

fn mark_bundled_world_seeded(
    conn: &Connection,
    key: &str,
    fingerprint: &str,
) -> Result<(), rusqlite::Error> {
    let value = serde_json::Value::String(fingerprint.to_string()).to_string();
    conn.execute(
        "INSERT INTO scoped_kv (owner_type, owner_id, namespace, key, value_json, updated_at)
         VALUES ('app', '', ?1, ?2, ?3, datetime('now'))
         ON CONFLICT(owner_type, owner_id, namespace, key)
         DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
        params![BUNDLED_WORLD_NAMESPACE, key, value],
    )?;
    Ok(())
}

fn world_with_name_exists(conn: &Connection, name: &str) -> Result<bool, rusqlite::Error> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM worlds WHERE TRIM(name) = ?1",
        params![name],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// 手机端首次启动时导入内置的凡人修仙世界包；桌面端仍由用户自行导入。
pub(crate) fn ensure_bundled_worlds(conn: &Connection, data_dir: &Path) -> Result<(), String> {
    if !cfg!(target_os = "android") {
        return Ok(());
    }
    import_bundled_world_once(
        conn,
        data_dir,
        &package_fingerprint(BUNDLED_MORTAL_WORLD_ZIP),
        BUNDLED_MORTAL_WORLD_ZIP,
    )
}

/// 同一个内置包只处理一次；包内容变了要重新导入（同名世界按「升级」处理，
/// 沿用原世界 id，会话/存档不受影响）。用户主动删掉世界后不会再复活。
fn import_bundled_world_once(
    conn: &Connection,
    data_dir: &Path,
    fingerprint: &str,
    zip: &[u8],
) -> Result<(), String> {
    let marker = stored_bundled_marker(conn, BUNDLED_MORTAL_WORLD_KEY).map_err(|e| e.to_string())?;
    if marker.as_deref() == Some(fingerprint) {
        return Ok(());
    }

    let exists =
        world_with_name_exists(conn, BUNDLED_MORTAL_WORLD_NAME).map_err(|e| e.to_string())?;
    if marker.is_some() && !exists {
        // 已播种过，但现在世界不在列表里：用户删掉了它，尊重这个选择。
        return mark_bundled_world_seeded(conn, BUNDLED_MORTAL_WORLD_KEY, fingerprint)
            .map_err(|e| e.to_string());
    }

    let service = WorldService::new();
    let imported = service.unpack_world_package_archive(data_dir, zip.to_vec())?;
    service.persist_world_package(conn, imported)?;
    eprintln!(
        "[db] bundled world '{BUNDLED_MORTAL_WORLD_NAME}' imported/updated ({fingerprint})"
    );
    mark_bundled_world_seeded(conn, BUNDLED_MORTAL_WORLD_KEY, fingerprint)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use rusqlite::params;
    use serde_json::{json, Value};

    fn temp_data_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "dream-engine-bundled-world-{tag}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("temp data dir");
        dir
    }

    fn world_ui_config(conn: &rusqlite::Connection) -> Value {
        let ui_config: String = conn
            .query_row(
                "SELECT ui_theme_config_json FROM worlds WHERE TRIM(name) = ?1",
                [super::BUNDLED_MORTAL_WORLD_NAME],
                |row| row.get(0),
            )
            .expect("world row");
        serde_json::from_str(&ui_config).expect("ui config json")
    }

    fn assert_packaged_ui(conn: &rusqlite::Connection) {
        let ui_config = world_ui_config(conn);
        assert_eq!(
            ui_config.get("runtime_version").and_then(Value::as_u64),
            Some(3)
        );
        for platform in ["desktop", "mobile"] {
            let document = ui_config
                .pointer(&format!("/entries/{platform}/document"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            assert!(
                document.contains("world-name") && document.len() > 4096,
                "{platform} world UI document should be packaged inline"
            );
        }
    }

    #[test]
    fn bundled_mortal_world_imports_once_with_packaged_ui() {
        let conn = rusqlite::Connection::open_in_memory().expect("open db");
        crate::db::schema::create_tables(&conn).expect("create schema");
        let data_dir = temp_data_dir("fresh");

        super::import_bundled_world_once(
            &conn,
            &data_dir,
            "test-fingerprint",
            super::BUNDLED_MORTAL_WORLD_ZIP,
        )
        .expect("import bundled world");

        let character_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM characters WHERE world_id = (SELECT id FROM worlds WHERE TRIM(name) = ?1)",
                [super::BUNDLED_MORTAL_WORLD_NAME],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(character_count, 6, "bundled world keeps its six characters");
        assert_packaged_ui(&conn);

        super::import_bundled_world_once(
            &conn,
            &data_dir,
            "test-fingerprint",
            super::BUNDLED_MORTAL_WORLD_ZIP,
        )
        .expect("repeat import is a no-op");
        let world_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM worlds WHERE TRIM(name) = ?1",
                [super::BUNDLED_MORTAL_WORLD_NAME],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(world_count, 1, "repeated startup must not duplicate the world");

        std::fs::remove_dir_all(&data_dir).ok();
    }

    #[test]
    fn legacy_same_name_world_is_upgraded_in_place() {
        let conn = rusqlite::Connection::open_in_memory().expect("open db");
        crate::db::schema::create_tables(&conn).expect("create schema");
        let data_dir = temp_data_dir("legacy");

        // 旧版本导入的世界：UI 还是 JSONC 时代的字符串，前端只能回退到默认界面。
        conn.execute(
            "INSERT INTO worlds (id, name, ui_theme_config_json) VALUES (?1, ?2, ?3)",
            params![
                "legacy-world-id",
                super::BUNDLED_MORTAL_WORLD_NAME,
                json!({
                    "runtime_version": 2,
                    "desktop_file": "{\"schema_version\":2,\"meta\":{\"name\":\"旧界面\"}}",
                    "entries": {}
                })
                .to_string()
            ],
        )
        .expect("insert legacy world");

        super::import_bundled_world_once(
            &conn,
            &data_dir,
            "test-fingerprint-2",
            super::BUNDLED_MORTAL_WORLD_ZIP,
        )
        .expect("upgrade legacy world");

        let world_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM worlds WHERE TRIM(name) = ?1",
                [super::BUNDLED_MORTAL_WORLD_NAME],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(world_count, 1, "upgrade must reuse the existing world row");
        let world_id: String = conn
            .query_row(
                "SELECT id FROM worlds WHERE TRIM(name) = ?1",
                [super::BUNDLED_MORTAL_WORLD_NAME],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(world_id, "legacy-world-id", "saves stay attached to the same world");
        assert_packaged_ui(&conn);

        std::fs::remove_dir_all(&data_dir).ok();
    }

    #[test]
    fn deleted_bundled_world_is_not_resurrected() {
        let conn = rusqlite::Connection::open_in_memory().expect("open db");
        crate::db::schema::create_tables(&conn).expect("create schema");
        let data_dir = temp_data_dir("deleted");

        super::import_bundled_world_once(
            &conn,
            &data_dir,
            "test-fingerprint-3",
            super::BUNDLED_MORTAL_WORLD_ZIP,
        )
        .expect("first import");
        conn.execute(
            "DELETE FROM worlds WHERE TRIM(name) = ?1",
            [super::BUNDLED_MORTAL_WORLD_NAME],
        )
        .expect("user deletes the world");

        // 包内容变了也不能把用户删掉的世界又塞回来。
        super::import_bundled_world_once(
            &conn,
            &data_dir,
            "test-fingerprint-4",
            super::BUNDLED_MORTAL_WORLD_ZIP,
        )
        .expect("second import");

        let world_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM worlds WHERE TRIM(name) = ?1",
                [super::BUNDLED_MORTAL_WORLD_NAME],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(world_count, 0, "deleted bundled world must stay deleted");

        std::fs::remove_dir_all(&data_dir).ok();
    }
}
