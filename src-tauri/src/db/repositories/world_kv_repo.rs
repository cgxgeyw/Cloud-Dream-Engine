use rusqlite::{params, types::Type, Connection, OptionalExtension};

use crate::models::world_kv::WorldKvEntry;

const MAX_KEY_BYTES: usize = 128;
const MAX_VALUE_BYTES: usize = 64 * 1024;
const MAX_OWNER_BYTES: i64 = 8 * 1024 * 1024;
const MAX_OWNER_ENTRIES: i64 = 10_000;

/// 三级作用域 KV 仓储：world（世界，跨存档共享）/ session（世界存档）/
/// character（存档内角色，owner_id 形如 "{session_id}:{character_id}"）。
/// 配额按属主独立计算。
pub struct WorldKvRepository<'a> {
    conn: &'a Connection,
}

impl<'a> WorldKvRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn list(
        &self,
        owner_type: &str,
        owner_id: &str,
        namespace: &str,
    ) -> Result<Vec<WorldKvEntry>, String> {
        let mut stmt = self.conn.prepare(
            "SELECT owner_type, owner_id, namespace, key, value_json, updated_at
             FROM scoped_kv WHERE owner_type = ?1 AND owner_id = ?2 AND namespace = ?3 ORDER BY key",
        ).map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(params![owner_type, owner_id, namespace], row_to_entry)
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|error| error.to_string())
    }

    pub fn get(
        &self,
        owner_type: &str,
        owner_id: &str,
        namespace: &str,
        key: &str,
    ) -> Result<Option<WorldKvEntry>, String> {
        let key = validate_key(key)?;
        self.conn.query_row(
            "SELECT owner_type, owner_id, namespace, key, value_json, updated_at
             FROM scoped_kv WHERE owner_type = ?1 AND owner_id = ?2 AND namespace = ?3 AND key = ?4",
            params![owner_type, owner_id, namespace, key],
            row_to_entry,
        ).optional().map_err(|error| error.to_string())
    }

    pub fn set(
        &self,
        owner_type: &str,
        owner_id: &str,
        namespace: &str,
        key: &str,
        value: &serde_json::Value,
    ) -> Result<WorldKvEntry, String> {
        let key = validate_key(key)?;
        let encoded = serde_json::to_string(value).map_err(|error| error.to_string())?;
        if encoded.len() > MAX_VALUE_BYTES {
            return Err(format!("KV value exceeds the {MAX_VALUE_BYTES} byte limit"));
        }
        let (entries, bytes, existing_bytes): (i64, i64, i64) = self.conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(length(CAST(value_json AS BLOB))), 0),
                    COALESCE(MAX(CASE WHEN namespace = ?3 AND key = ?4
                                      THEN length(CAST(value_json AS BLOB)) ELSE 0 END), 0)
             FROM scoped_kv WHERE owner_type = ?1 AND owner_id = ?2",
            params![owner_type, owner_id, namespace, key],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).map_err(|error| error.to_string())?;
        if existing_bytes == 0 && entries >= MAX_OWNER_ENTRIES {
            return Err(format!("KV storage reached the {MAX_OWNER_ENTRIES} entry limit"));
        }
        if bytes.saturating_sub(existing_bytes).saturating_add(encoded.len() as i64) > MAX_OWNER_BYTES {
            return Err(format!("KV storage would exceed the {MAX_OWNER_BYTES} byte limit"));
        }
        self.conn.execute(
            "INSERT INTO scoped_kv (owner_type, owner_id, namespace, key, value_json, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
             ON CONFLICT(owner_type, owner_id, namespace, key) DO UPDATE SET
                value_json = excluded.value_json,
                updated_at = excluded.updated_at",
            params![owner_type, owner_id, namespace, key, encoded],
        ).map_err(|error| error.to_string())?;
        self.get(owner_type, owner_id, namespace, &key)?
            .ok_or_else(|| "KV entry was not saved".to_string())
    }

    pub fn delete(
        &self,
        owner_type: &str,
        owner_id: &str,
        namespace: &str,
        key: &str,
    ) -> Result<(), String> {
        let key = validate_key(key)?;
        self.conn.execute(
            "DELETE FROM scoped_kv WHERE owner_type = ?1 AND owner_id = ?2 AND namespace = ?3 AND key = ?4",
            params![owner_type, owner_id, namespace, key],
        ).map_err(|error| error.to_string())?;
        Ok(())
    }
}

fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorldKvEntry> {
    let raw: String = row.get(4)?;
    let value = serde_json::from_str(&raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(4, Type::Text, Box::new(error))
    })?;
    Ok(WorldKvEntry {
        owner_type: row.get(0)?,
        owner_id: row.get(1)?,
        namespace: row.get(2)?,
        key: row.get(3)?,
        value,
        updated_at: row.get(5)?,
    })
}

fn validate_key(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > MAX_KEY_BYTES || value.chars().any(char::is_control) {
        return Err(format!(
            "KV key must be non-empty, contain no control characters, and use at most {MAX_KEY_BYTES} bytes"
        ));
    }
    Ok(value.to_string())
}
