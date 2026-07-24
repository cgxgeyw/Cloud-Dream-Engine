use rusqlite::{params, types::Type, Connection, OptionalExtension};

use crate::models::world_kv::WorldKvEntry;

const MAX_KEY_BYTES: usize = 128;
const MAX_VALUE_BYTES: usize = 64 * 1024;
const MAX_WORLD_BYTES: i64 = 8 * 1024 * 1024;
const MAX_WORLD_ENTRIES: i64 = 10_000;

pub struct WorldKvRepository<'a> {
    conn: &'a Connection,
}

impl<'a> WorldKvRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn list(&self, world_id: &str, namespace: &str) -> Result<Vec<WorldKvEntry>, String> {
        let mut stmt = self.conn.prepare(
            "SELECT world_id, namespace, key, value_json, updated_at
             FROM world_kv WHERE world_id = ?1 AND namespace = ?2 ORDER BY key",
        ).map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(params![world_id, namespace], row_to_entry)
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|error| error.to_string())
    }

    pub fn get(
        &self,
        world_id: &str,
        namespace: &str,
        key: &str,
    ) -> Result<Option<WorldKvEntry>, String> {
        let key = validate_key(key)?;
        self.conn.query_row(
            "SELECT world_id, namespace, key, value_json, updated_at
             FROM world_kv WHERE world_id = ?1 AND namespace = ?2 AND key = ?3",
            params![world_id, namespace, key],
            row_to_entry,
        ).optional().map_err(|error| error.to_string())
    }

    pub fn set(
        &self,
        world_id: &str,
        namespace: &str,
        key: &str,
        value: &serde_json::Value,
    ) -> Result<WorldKvEntry, String> {
        let key = validate_key(key)?;
        let encoded = serde_json::to_string(value).map_err(|error| error.to_string())?;
        if encoded.len() > MAX_VALUE_BYTES {
            return Err(format!("World KV value exceeds the {MAX_VALUE_BYTES} byte limit"));
        }
        let (entries, bytes, existing_bytes): (i64, i64, i64) = self.conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(length(CAST(value_json AS BLOB))), 0),
                    COALESCE(MAX(CASE WHEN namespace = ?2 AND key = ?3
                                      THEN length(CAST(value_json AS BLOB)) ELSE 0 END), 0)
             FROM world_kv WHERE world_id = ?1",
            params![world_id, namespace, key],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).map_err(|error| error.to_string())?;
        if existing_bytes == 0 && entries >= MAX_WORLD_ENTRIES {
            return Err(format!("World KV storage reached the {MAX_WORLD_ENTRIES} entry limit"));
        }
        if bytes.saturating_sub(existing_bytes).saturating_add(encoded.len() as i64) > MAX_WORLD_BYTES {
            return Err(format!("World KV storage would exceed the {MAX_WORLD_BYTES} byte limit"));
        }
        self.conn.execute(
            "INSERT INTO world_kv (world_id, namespace, key, value_json, updated_at)
             VALUES (?1, ?2, ?3, ?4, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
             ON CONFLICT(world_id, namespace, key) DO UPDATE SET
                value_json = excluded.value_json,
                updated_at = excluded.updated_at",
            params![world_id, namespace, key, encoded],
        ).map_err(|error| error.to_string())?;
        self.get(world_id, namespace, &key)?
            .ok_or_else(|| "World KV entry was not saved".to_string())
    }

    pub fn delete(&self, world_id: &str, namespace: &str, key: &str) -> Result<(), String> {
        let key = validate_key(key)?;
        self.conn.execute(
            "DELETE FROM world_kv WHERE world_id = ?1 AND namespace = ?2 AND key = ?3",
            params![world_id, namespace, key],
        ).map_err(|error| error.to_string())?;
        Ok(())
    }
}

fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorldKvEntry> {
    let raw: String = row.get(3)?;
    let value = serde_json::from_str(&raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(3, Type::Text, Box::new(error))
    })?;
    Ok(WorldKvEntry {
        world_id: row.get(0)?,
        namespace: row.get(1)?,
        key: row.get(2)?,
        value,
        updated_at: row.get(4)?,
    })
}

fn validate_key(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > MAX_KEY_BYTES || value.chars().any(char::is_control) {
        return Err(format!(
            "World KV key must be non-empty, contain no control characters, and use at most {MAX_KEY_BYTES} bytes"
        ));
    }
    Ok(value.to_string())
}
