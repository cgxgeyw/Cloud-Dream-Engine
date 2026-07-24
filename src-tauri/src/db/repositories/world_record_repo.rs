use rusqlite::{params, types::Type, Connection, OptionalExtension};

use crate::models::world_record::{WorldRecord, WorldRecordWriteRequest};

const MAX_RECORD_BYTES: usize = 64 * 1024;
const MAX_WORLD_ID_BYTES: usize = 128;
const MAX_COLLECTION_BYTES: usize = 64;
const MAX_TOP_LEVEL_FIELDS: usize = 128;
const MAX_FIELD_NAME_BYTES: usize = 128;
const MAX_JSON_DEPTH: usize = 16;
const MAX_COLLECTION_RECORDS: i64 = 20_000;
const MAX_COLLECTION_STORAGE_BYTES: i64 = 32 * 1024 * 1024;
const MAX_WORLD_RECORDS: i64 = 100_000;
const MAX_WORLD_STORAGE_BYTES: i64 = 128 * 1024 * 1024;

#[derive(Debug, Default, PartialEq, Eq)]
struct ScopeUsage {
    world_records: i64,
    world_bytes: i64,
    collection_records: i64,
    collection_bytes: i64,
}

pub struct WorldRecordRepository<'a> {
    conn: &'a Connection,
}

impl<'a> WorldRecordRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn list(&self, world_id: &str, collection: &str) -> Result<Vec<WorldRecord>, String> {
        let world_id = validate_world_id(world_id)?;
        self.require_world(&world_id)?;
        let collection = validate_collection(collection)?;
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, world_id, collection, data_json, created_at, updated_at
                 FROM world_records
                 WHERE world_id = ?1 AND collection = ?2
                 ORDER BY updated_at DESC, id DESC",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(params![world_id, collection], row_to_world_record)
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())
    }

    pub fn create(
        &self,
        world_id: &str,
        request: &WorldRecordWriteRequest,
    ) -> Result<WorldRecord, String> {
        let world_id = validate_world_id(world_id)?;
        self.require_world(&world_id)?;
        let collection = validate_collection(&request.collection)?;
        let data_json = validate_data(&request.data)?;
        let usage = self.scope_usage(&world_id, &collection)?;
        validate_create_quota(&usage, data_json.len() as i64)?;
        let id = uuid::Uuid::new_v4().to_string();
        self.conn
            .execute(
                "INSERT INTO world_records (
                    id, world_id, collection, data_json, created_at, updated_at
                 ) VALUES (
                    ?1, ?2, ?3, ?4,
                    strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                    strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 )",
                params![id, world_id, collection, data_json],
            )
            .map_err(|error| error.to_string())?;
        self.get(&world_id, &collection, &id)?
            .ok_or_else(|| "World record was not created".to_string())
    }

    pub fn update(
        &self,
        world_id: &str,
        id: &str,
        request: &WorldRecordWriteRequest,
    ) -> Result<WorldRecord, String> {
        let world_id = validate_world_id(world_id)?;
        self.require_world(&world_id)?;
        let id = validate_record_id(id)?;
        let collection = validate_collection(&request.collection)?;
        let data_json = validate_data(&request.data)?;
        let (_, existing_bytes) = self
            .get_with_size(&world_id, &collection, &id)?
            .ok_or_else(|| "World record not found".to_string())?;
        let usage = self.scope_usage(&world_id, &collection)?;
        validate_update_quota(&usage, existing_bytes, data_json.len() as i64)?;
        let affected = self
            .conn
            .execute(
                "UPDATE world_records
                 SET data_json = ?1,
                     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = ?2 AND world_id = ?3 AND collection = ?4",
                params![data_json, id, world_id, collection],
            )
            .map_err(|error| error.to_string())?;
        if affected == 0 {
            return Err("World record not found".to_string());
        }
        self.get(&world_id, &collection, &id)?
            .ok_or_else(|| "World record not found".to_string())
    }

    pub fn delete(&self, world_id: &str, collection: &str, id: &str) -> Result<(), String> {
        let world_id = validate_world_id(world_id)?;
        self.require_world(&world_id)?;
        let collection = validate_collection(collection)?;
        let id = validate_record_id(id)?;
        let affected = self
            .conn
            .execute(
                "DELETE FROM world_records
                 WHERE id = ?1 AND world_id = ?2 AND collection = ?3",
                params![id, world_id, collection],
            )
            .map_err(|error| error.to_string())?;
        if affected == 0 {
            return Err("World record not found".to_string());
        }
        Ok(())
    }

    fn require_world(&self, world_id: &str) -> Result<(), String> {
        let exists = self
            .conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM worlds WHERE id = ?1)",
                params![world_id],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|error| error.to_string())?;
        if !exists {
            return Err("World not found".to_string());
        }
        Ok(())
    }

    fn scope_usage(&self, world_id: &str, collection: &str) -> Result<ScopeUsage, String> {
        self.conn
            .query_row(
                "SELECT
                    COUNT(*),
                    COALESCE(SUM(length(CAST(data_json AS BLOB))), 0),
                    COALESCE(SUM(CASE WHEN collection = ?2 THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(
                        CASE WHEN collection = ?2
                             THEN length(CAST(data_json AS BLOB))
                             ELSE 0
                        END
                    ), 0)
                 FROM world_records
                 WHERE world_id = ?1",
                params![world_id, collection],
                |row| {
                    Ok(ScopeUsage {
                        world_records: row.get(0)?,
                        world_bytes: row.get(1)?,
                        collection_records: row.get(2)?,
                        collection_bytes: row.get(3)?,
                    })
                },
            )
            .map_err(|error| error.to_string())
    }

    fn get(
        &self,
        world_id: &str,
        collection: &str,
        id: &str,
    ) -> Result<Option<WorldRecord>, String> {
        self.conn
            .query_row(
                "SELECT id, world_id, collection, data_json, created_at, updated_at
                 FROM world_records
                 WHERE id = ?1 AND world_id = ?2 AND collection = ?3",
                params![id, world_id, collection],
                row_to_world_record,
            )
            .optional()
            .map_err(|error| error.to_string())
    }

    fn get_with_size(
        &self,
        world_id: &str,
        collection: &str,
        id: &str,
    ) -> Result<Option<(WorldRecord, i64)>, String> {
        self.conn
            .query_row(
                "SELECT id, world_id, collection, data_json, created_at, updated_at,
                        length(CAST(data_json AS BLOB))
                 FROM world_records
                 WHERE id = ?1 AND world_id = ?2 AND collection = ?3",
                params![id, world_id, collection],
                |row| Ok((row_to_world_record(row)?, row.get(6)?)),
            )
            .optional()
            .map_err(|error| error.to_string())
    }
}

fn row_to_world_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorldRecord> {
    let data_json: String = row.get(3)?;
    let data = serde_json::from_str(&data_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(3, Type::Text, Box::new(error))
    })?;
    Ok(WorldRecord {
        id: row.get(0)?,
        world_id: row.get(1)?,
        collection: row.get(2)?,
        data,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

fn validate_world_id(value: &str) -> Result<String, String> {
    if value.is_empty()
        || value.len() > MAX_WORLD_ID_BYTES
        || value.trim() != value
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        return Err(
            "World id must use 1-128 ASCII letters, numbers, dots, dashes, or underscores"
                .to_string(),
        );
    }
    Ok(value.to_string())
}

fn validate_record_id(value: &str) -> Result<String, String> {
    uuid::Uuid::parse_str(value).map_err(|_| "World record id must be a valid UUID".to_string())?;
    Ok(value.to_string())
}

fn validate_collection(value: &str) -> Result<String, String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty()
        || normalized.len() > MAX_COLLECTION_BYTES
        || !normalized.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        return Err(
            format!(
                "World record collection must use 1-{} ASCII letters, numbers, dots, dashes, or underscores",
                MAX_COLLECTION_BYTES
            ),
        );
    }
    Ok(normalized)
}

fn validate_data(value: &serde_json::Value) -> Result<String, String> {
    let Some(fields) = value.as_object() else {
        return Err("World record data must be a JSON object".to_string());
    };
    if fields.len() > MAX_TOP_LEVEL_FIELDS {
        return Err(format!(
            "World record data exceeds the {} top-level field limit",
            MAX_TOP_LEVEL_FIELDS
        ));
    }
    for field in fields.keys() {
        if field.trim().is_empty()
            || field.len() > MAX_FIELD_NAME_BYTES
            || field.chars().any(char::is_control)
        {
            return Err(format!(
                "World record field names must be non-empty, contain no control characters, and use at most {} bytes",
                MAX_FIELD_NAME_BYTES
            ));
        }
    }
    validate_json_depth(value, 1)?;
    let encoded = serde_json::to_string(value).map_err(|error| error.to_string())?;
    if encoded.len() > MAX_RECORD_BYTES {
        return Err(format!(
            "World record data exceeds the {} byte limit",
            MAX_RECORD_BYTES
        ));
    }
    Ok(encoded)
}

fn validate_json_depth(value: &serde_json::Value, depth: usize) -> Result<(), String> {
    if depth > MAX_JSON_DEPTH {
        return Err(format!(
            "World record data exceeds the JSON nesting depth limit of {}",
            MAX_JSON_DEPTH
        ));
    }
    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                validate_json_depth(item, depth + 1)?;
            }
        }
        serde_json::Value::Object(fields) => {
            for item in fields.values() {
                validate_json_depth(item, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_create_quota(usage: &ScopeUsage, incoming_bytes: i64) -> Result<(), String> {
    if usage.collection_records >= MAX_COLLECTION_RECORDS {
        return Err(format!(
            "World record collection reached the {} record limit",
            MAX_COLLECTION_RECORDS
        ));
    }
    if usage.world_records >= MAX_WORLD_RECORDS {
        return Err(format!(
            "World record storage reached the {} record limit",
            MAX_WORLD_RECORDS
        ));
    }
    validate_storage_quota(usage, 0, incoming_bytes)
}

fn validate_update_quota(
    usage: &ScopeUsage,
    existing_bytes: i64,
    replacement_bytes: i64,
) -> Result<(), String> {
    if replacement_bytes <= existing_bytes {
        return Ok(());
    }
    validate_storage_quota(usage, existing_bytes, replacement_bytes)
}

fn validate_storage_quota(
    usage: &ScopeUsage,
    replaced_bytes: i64,
    incoming_bytes: i64,
) -> Result<(), String> {
    let collection_bytes = usage
        .collection_bytes
        .saturating_sub(replaced_bytes)
        .saturating_add(incoming_bytes);
    if collection_bytes > MAX_COLLECTION_STORAGE_BYTES {
        return Err(format!(
            "World record collection would exceed the {} byte storage limit",
            MAX_COLLECTION_STORAGE_BYTES
        ));
    }
    let world_bytes = usage
        .world_bytes
        .saturating_sub(replaced_bytes)
        .saturating_add(incoming_bytes);
    if world_bytes > MAX_WORLD_STORAGE_BYTES {
        return Err(format!(
            "World record storage would exceed the {} byte limit",
            MAX_WORLD_STORAGE_BYTES
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(collection: &str, data: serde_json::Value) -> WorldRecordWriteRequest {
        WorldRecordWriteRequest {
            collection: collection.to_string(),
            data,
        }
    }

    fn connection() -> Connection {
        let connection = Connection::open_in_memory().expect("open database");
        connection
            .execute_batch(
                "PRAGMA foreign_keys=ON;
                 CREATE TABLE worlds (id TEXT PRIMARY KEY);
                 CREATE TABLE world_records (
                    id TEXT PRIMARY KEY,
                    world_id TEXT NOT NULL,
                    collection TEXT NOT NULL,
                    data_json TEXT NOT NULL DEFAULT '{}',
                    created_at TEXT NOT NULL DEFAULT '',
                    updated_at TEXT NOT NULL DEFAULT '',
                    FOREIGN KEY (world_id) REFERENCES worlds(id) ON DELETE CASCADE
                 );",
            )
            .expect("create schema");
        connection
            .execute(
                "INSERT INTO worlds (id) VALUES ('world-a'), ('world-b')",
                [],
            )
            .expect("insert worlds");
        connection
    }

    #[test]
    fn records_are_scoped_to_their_world() {
        let connection = connection();
        let repository = WorldRecordRepository::new(&connection);
        let request = request("ledger.entries", serde_json::json!({"amount": 12.5}));
        repository
            .create("world-a", &request)
            .expect("create record");
        assert_eq!(
            repository.list("world-a", "ledger.entries").unwrap().len(),
            1
        );
        assert!(repository
            .list("world-b", "ledger.entries")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn update_and_delete_reject_a_different_world() {
        let connection = connection();
        let repository = WorldRecordRepository::new(&connection);
        let request = request("ledger.entries", serde_json::json!({"amount": 12.5}));
        let record = repository
            .create("world-a", &request)
            .expect("create record");
        assert_eq!(
            repository
                .update("world-b", &record.id, &request)
                .unwrap_err(),
            "World record not found"
        );
        assert_eq!(
            repository
                .delete("world-b", "ledger.entries", &record.id)
                .unwrap_err(),
            "World record not found"
        );
    }

    #[test]
    fn records_are_scoped_to_their_collection() {
        let connection = connection();
        let repository = WorldRecordRepository::new(&connection);
        let entry_request = request(" Ledger.Entries ", serde_json::json!({"amount": 12.5}));
        let record = repository
            .create("world-a", &entry_request)
            .expect("create record");

        assert_eq!(record.collection, "ledger.entries");
        assert_eq!(
            repository.list("world-a", "LEDGER.ENTRIES").unwrap().len(),
            1
        );
        assert!(repository
            .list("world-a", "ledger.notes")
            .unwrap()
            .is_empty());
        assert_eq!(
            repository
                .update(
                    "world-a",
                    &record.id,
                    &request("ledger.notes", serde_json::json!({"note": "moved"})),
                )
                .unwrap_err(),
            "World record not found"
        );
        assert_eq!(
            repository
                .delete("world-a", "ledger.notes", &record.id)
                .unwrap_err(),
            "World record not found"
        );
    }

    #[test]
    fn missing_world_and_invalid_record_ids_are_rejected() {
        let connection = connection();
        let repository = WorldRecordRepository::new(&connection);
        assert_eq!(
            repository
                .list("missing-world", "ledger.entries")
                .unwrap_err(),
            "World not found"
        );
        assert!(repository
            .list(" invalid-world ", "ledger.entries")
            .unwrap_err()
            .starts_with("World id must use"));

        let request = request("ledger.entries", serde_json::json!({"amount": 1}));
        assert_eq!(
            repository
                .update("world-a", "not-a-uuid", &request)
                .unwrap_err(),
            "World record id must be a valid UUID"
        );
        assert_eq!(
            repository
                .delete("world-a", "ledger.entries", "not-a-uuid")
                .unwrap_err(),
            "World record id must be a valid UUID"
        );
    }

    #[test]
    fn collection_and_json_fields_are_validated() {
        let connection = connection();
        let repository = WorldRecordRepository::new(&connection);
        assert!(repository
            .create("world-a", &request("ledger/entries", serde_json::json!({})))
            .unwrap_err()
            .starts_with("World record collection must use"));
        assert_eq!(
            repository
                .create(
                    "world-a",
                    &request("ledger.entries", serde_json::json!([1, 2, 3])),
                )
                .unwrap_err(),
            "World record data must be a JSON object"
        );
        assert!(repository
            .create(
                "world-a",
                &request("ledger.entries", serde_json::json!({"\n": "invalid"})),
            )
            .unwrap_err()
            .starts_with("World record field names must be"));

        let oversized = "x".repeat(MAX_RECORD_BYTES);
        assert!(repository
            .create(
                "world-a",
                &request("ledger.entries", serde_json::json!({"note": oversized})),
            )
            .unwrap_err()
            .contains("byte limit"));

        let mut deeply_nested = serde_json::Value::Null;
        for _ in 0..MAX_JSON_DEPTH {
            deeply_nested = serde_json::json!({"nested": deeply_nested});
        }
        assert!(repository
            .create("world-a", &request("ledger.entries", deeply_nested))
            .unwrap_err()
            .contains("nesting depth"));
    }

    #[test]
    fn create_quotas_cover_collection_and_world_limits() {
        let mut usage = ScopeUsage {
            collection_records: MAX_COLLECTION_RECORDS,
            ..ScopeUsage::default()
        };
        assert!(validate_create_quota(&usage, 1)
            .unwrap_err()
            .contains("collection reached"));

        usage = ScopeUsage {
            world_records: MAX_WORLD_RECORDS,
            ..ScopeUsage::default()
        };
        assert!(validate_create_quota(&usage, 1)
            .unwrap_err()
            .contains("storage reached"));

        usage = ScopeUsage {
            collection_bytes: MAX_COLLECTION_STORAGE_BYTES,
            ..ScopeUsage::default()
        };
        assert!(validate_create_quota(&usage, 1)
            .unwrap_err()
            .contains("collection would exceed"));

        usage = ScopeUsage {
            world_bytes: MAX_WORLD_STORAGE_BYTES,
            ..ScopeUsage::default()
        };
        assert!(validate_create_quota(&usage, 1)
            .unwrap_err()
            .contains("storage would exceed"));
    }

    #[test]
    fn updates_may_shrink_records_that_are_already_over_quota() {
        let usage = ScopeUsage {
            world_bytes: MAX_WORLD_STORAGE_BYTES + 10,
            collection_bytes: MAX_COLLECTION_STORAGE_BYTES + 10,
            ..ScopeUsage::default()
        };
        validate_update_quota(&usage, 100, 90).expect("shrinking update");
        assert!(validate_update_quota(&usage, 100, 101).is_err());
    }

    #[test]
    fn scope_usage_counts_encoded_utf8_bytes() {
        let connection = connection();
        let repository = WorldRecordRepository::new(&connection);
        let data = serde_json::json!({"note": "一笔账"});
        repository
            .create("world-a", &request("ledger.entries", data.clone()))
            .expect("create record");

        let usage = repository
            .scope_usage("world-a", "ledger.entries")
            .expect("scope usage");
        assert_eq!(usage.world_records, 1);
        assert_eq!(usage.collection_records, 1);
        assert_eq!(
            usage.collection_bytes,
            serde_json::to_string(&data).unwrap().len() as i64
        );
        assert_eq!(usage.world_bytes, usage.collection_bytes);
    }

    #[test]
    fn deleting_a_world_cascades_its_records() {
        let connection = connection();
        let repository = WorldRecordRepository::new(&connection);
        repository
            .create(
                "world-a",
                &request("ledger.entries", serde_json::json!({"amount": 12.5})),
            )
            .expect("create record");
        connection
            .execute("DELETE FROM worlds WHERE id = 'world-a'", [])
            .expect("delete world");
        let remaining: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM world_records WHERE world_id = 'world-a'",
                [],
                |row| row.get(0),
            )
            .expect("count records");
        assert_eq!(remaining, 0);
    }

    #[test]
    fn malformed_stored_json_is_reported_instead_of_silently_replaced() {
        let connection = connection();
        connection
            .execute(
                "INSERT INTO world_records (
                    id, world_id, collection, data_json, created_at, updated_at
                 ) VALUES (?1, 'world-a', 'ledger.entries', '{broken', '', '')",
                params![uuid::Uuid::new_v4().to_string()],
            )
            .expect("insert malformed row");
        let repository = WorldRecordRepository::new(&connection);
        assert!(repository.list("world-a", "ledger.entries").is_err());
    }
}
