use crate::db::{oag_schema, repositories::oag_repo::OagRepository};
use crate::models::oag::{OagLegacyIdMap, OagObject, OAG_SCHEMA_VERSION};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::{HashMap, HashSet};

pub fn apply(conn: &Connection) -> Result<(), String> {
    let tx = conn.unchecked_transaction().map_err(db_err)?;
    oag_schema::create(&tx).map_err(db_err)?;
    ensure_column(
        &tx,
        "oag_object_types",
        "actor",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        &tx,
        "oag_object_types",
        "platform_role",
        "TEXT NOT NULL DEFAULT ''",
    )?;

    let version = schema_version(&tx)?;
    if version > OAG_SCHEMA_VERSION {
        return Err(format!(
            "OAG schema version {} is newer than supported version {}",
            version, OAG_SCHEMA_VERSION
        ));
    }
    if version == 0 {
        migrate_legacy(&tx)?;
        tx.execute(
            "INSERT INTO oag_schema_meta (id, schema_version, applied_at)
             VALUES (1, ?1, ?2)
             ON CONFLICT(id) DO UPDATE SET
               schema_version = excluded.schema_version,
               applied_at = excluded.applied_at",
            params![OAG_SCHEMA_VERSION, Utc::now().to_rfc3339()],
        )
        .map_err(db_err)?;
    }
    tx.commit().map_err(db_err)
}

pub fn migrate_legacy(conn: &Connection) -> Result<(), String> {
    migrate_legacy_objects(conn)?;
    migrate_legacy_relations(conn)?;
    migrate_legacy_attributes(conn)
}

fn schema_version(conn: &Connection) -> Result<i64, String> {
    let version: Option<i64> = conn
        .query_row(
            "SELECT schema_version FROM oag_schema_meta WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_err)?;
    Ok(version.unwrap_or(0))
}

fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), String> {
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = ?2",
            params![table, column],
            |row| row.get(0),
        )
        .map_err(db_err)?;
    if exists == 0 {
        conn.execute_batch(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {definition};"
        ))
        .map_err(db_err)?;
    }
    Ok(())
}

fn migrate_legacy_objects(conn: &Connection) -> Result<(), String> {
    let repo = OagRepository::new(conn);
    let mut stmt = conn
        .prepare(
            "SELECT id, world_id, session_id, name, name_normalized, entity_type,
                    aliases_json, first_seen_turn, last_seen_turn, created_at
             FROM memory_entities ORDER BY world_id, session_id, id",
        )
        .map_err(db_err)?;
    let entities = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, i32>(7)?,
                row.get::<_, i32>(8)?,
                row.get::<_, String>(9)?,
            ))
        })
        .map_err(db_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_err)?;
    drop(stmt);

    for (id, world_id, session_id, name, normalized, entity_type, aliases, first, last, created) in
        entities
    {
        if let Some(target) = repo.legacy_target("memory_entities", &id)? {
            if repo.object(&target)?.is_some() {
                continue;
            }
        }
        let object_type = entity_type.trim();
        if object_type.is_empty() || repo.object_type(&world_id, object_type)?.is_none() {
            continue;
        }
        let canonical_key = format!("{}:legacy:{}", object_type, id);
        let object_id = format!(
            "obj:{}:session:{}:{}:legacy:{}",
            world_id, session_id, object_type, id
        );
        let object = OagObject {
            id: object_id.clone(),
            world_id: world_id.clone(),
            session_id: session_id.clone(),
            object_type: object_type.to_string(),
            canonical_key,
            name,
            name_normalized: normalized,
            aliases: serde_json::from_str(&aliases).unwrap_or_default(),
            description: String::new(),
            source: "legacy".to_string(),
            visibility: "scene".to_string(),
            asserted_by_id: None,
            source_event_id: None,
            truth_status: "asserted".to_string(),
            created_turn: first,
            updated_turn: last,
            retired_at: String::new(),
            created_at: created.clone(),
            updated_at: created,
        };
        let stored = repo.upsert_object(&object)?;
        repo.record_legacy_map(&OagLegacyIdMap {
            source_table: "memory_entities".to_string(),
            source_id: id,
            world_id,
            session_id,
            target_table: "oag_objects".to_string(),
            target_id: stored.id,
        })?;
    }
    Ok(())
}

fn migrate_legacy_relations(conn: &Connection) -> Result<(), String> {
    let repo = OagRepository::new(conn);
    let mut stmt = conn
        .prepare(
            "SELECT id, world_id, session_id, subject_entity_id, predicate, object_entity_id,
                    object_text, valid_from_turn, invalid_at_turn, source, confidence,
                    asserted_by, asserted_by_id, visibility, truth_status, witnesses_json
             FROM memory_relations ORDER BY world_id, session_id, valid_from_turn, id",
        )
        .map_err(db_err)?;
    let relations = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, i32>(7)?,
                row.get::<_, Option<i32>>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, f64>(10)?,
                row.get::<_, String>(11)?,
                row.get::<_, Option<String>>(12)?,
                row.get::<_, String>(13)?,
                row.get::<_, String>(14)?,
                row.get::<_, String>(15)?,
            ))
        })
        .map_err(db_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_err)?;
    drop(stmt);

    for relation in relations {
        let (
            id,
            world_id,
            session_id,
            subject_id,
            predicate,
            object_id,
            object_text,
            valid_from,
            valid_to,
            source,
            confidence,
            asserted_by,
            asserted_by_id,
            visibility,
            truth_status,
            witnesses_json,
        ) = relation;
        let Some(subject_id) = mapped_object(&repo, "memory_entities", &subject_id)? else {
            continue;
        };
        let link_type = if repo.link_type(&world_id, &predicate)?.is_some() {
            predicate.clone()
        } else if repo.link_type(&world_id, "related_to")?.is_some() {
            "related_to".to_string()
        } else {
            continue;
        };
        let to_object_id = match object_id.as_deref() {
            Some(id) => mapped_object(&repo, "memory_entities", id)?,
            None => None,
        };
        if object_id.is_some() && to_object_id.is_none() {
            continue;
        }
        if object_id.is_none() && object_text.trim().is_empty() {
            continue;
        }
        let actor_id = asserted_by_id
            .as_deref()
            .and_then(|id| {
                resolve_actor(conn, &world_id, &session_id, id)
                    .ok()
                    .flatten()
            })
            .or_else(|| {
                resolve_actor(conn, &world_id, &session_id, &asserted_by)
                    .ok()
                    .flatten()
            });
        if visibility == "private" && actor_id.is_none() {
            continue;
        }
        let link_id = format!("oag-link:{}:{}:legacy:{}", world_id, session_id, id);
        let original_predicate = if link_type != predicate {
            predicate
        } else {
            String::new()
        };
        conn.execute(
            "INSERT OR IGNORE INTO oag_links
             (id, world_id, session_id, from_object_id, link_type, to_object_id, to_value,
              original_predicate, attributes_json, valid_from_turn, valid_to_turn, source,
              confidence, asserted_by_id, source_event_id, visibility, truth_status, inferred,
              supersedes_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, '{}', ?9, ?10, ?11, ?12, ?13, NULL,
                     ?14, ?15, 0, NULL, ?16)",
            params![
                link_id,
                world_id,
                session_id,
                subject_id,
                link_type,
                to_object_id,
                if object_id.is_none() {
                    object_text
                } else {
                    String::new()
                },
                original_predicate,
                valid_from,
                valid_to,
                source,
                confidence,
                actor_id,
                visibility,
                truth_status,
                Utc::now().to_rfc3339(),
            ],
        )
        .map_err(db_err)?;
        repo.record_legacy_map(&OagLegacyIdMap {
            source_table: "memory_relations".to_string(),
            source_id: id,
            world_id: world_id.clone(),
            session_id: session_id.clone(),
            target_table: "oag_links".to_string(),
            target_id: link_id.clone(),
        })?;
        migrate_relation_knowledge(
            conn,
            &world_id,
            &session_id,
            &link_id,
            &subject_id,
            to_object_id.as_deref(),
            &visibility,
            actor_id.as_deref(),
            &witnesses_json,
            valid_from,
            confidence,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn migrate_relation_knowledge(
    conn: &Connection,
    world_id: &str,
    session_id: &str,
    link_id: &str,
    subject_id: &str,
    object_id: Option<&str>,
    visibility: &str,
    asserted_by: Option<&str>,
    witnesses_json: &str,
    learned_turn: i32,
    confidence: f64,
) -> Result<(), String> {
    if visibility == "public" {
        return Ok(());
    }
    let witnesses: Vec<String> = serde_json::from_str(witnesses_json).unwrap_or_default();
    let mut actors = Vec::new();
    if let Some(actor) = asserted_by {
        actors.push(actor.to_string());
    }
    if visibility == "scene" {
        actors.extend(witnesses);
    }
    let mut resolved = HashSet::new();
    for actor in actors {
        let Some(actor_id) = resolve_actor(conn, world_id, session_id, &actor)? else {
            return Ok(());
        };
        resolved.insert(actor_id);
    }
    for actor_id in resolved {
        insert_knowledge(
            conn,
            world_id,
            session_id,
            &actor_id,
            "link",
            link_id,
            asserted_by,
            learned_turn,
            confidence,
        )?;
        insert_knowledge(
            conn,
            world_id,
            session_id,
            &actor_id,
            "object",
            subject_id,
            asserted_by,
            learned_turn,
            confidence,
        )?;
        if let Some(object_id) = object_id {
            insert_knowledge(
                conn,
                world_id,
                session_id,
                &actor_id,
                "object",
                object_id,
                asserted_by,
                learned_turn,
                confidence,
            )?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn insert_knowledge(
    conn: &Connection,
    world_id: &str,
    session_id: &str,
    actor_id: &str,
    target_kind: &str,
    target_id: &str,
    source_actor_id: Option<&str>,
    learned_turn: i32,
    confidence: f64,
) -> Result<(), String> {
    let id = format!(
        "oag-knowledge:{}:{}:{}:{}:{}",
        world_id, session_id, actor_id, target_kind, target_id
    );
    conn.execute(
        "INSERT OR IGNORE INTO oag_knowledge
         (id, world_id, session_id, actor_id, target_kind, target_id, knowledge_type,
          access_kind, source_event_id, source_actor_id, learned_turn, confidence, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'witnessed', 'content', NULL, ?7, ?8, ?9, ?10)",
        params![
            id,
            world_id,
            session_id,
            actor_id,
            target_kind,
            target_id,
            source_actor_id,
            learned_turn,
            confidence,
            Utc::now().to_rfc3339(),
        ],
    )
    .map_err(db_err)?;
    Ok(())
}

fn migrate_legacy_attributes(conn: &Connection) -> Result<(), String> {
    let session_worlds = session_worlds(conn)?;
    let mut stmt = conn
        .prepare(
            "SELECT v.id, s.world_id, s.key, s.value_type, v.owner_type, v.owner_id,
                    v.value_json, v.source
             FROM attribute_values v
             JOIN attribute_schemas s ON s.id = v.schema_id
             ORDER BY v.id",
        )
        .map_err(db_err)?;
    let values = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
            ))
        })
        .map_err(db_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_err)?;
    drop(stmt);

    for (id, schema_world, key, value_type, owner_type, owner_id, value_json, source) in values {
        let (world_id, session_id, character_id) = match owner_type.as_str() {
            "character" => {
                let world_id = match schema_world.clone() {
                    Some(world_id) => Some(world_id),
                    None => character_world(conn, &owner_id)?,
                };
                let Some(world_id) = world_id else {
                    continue;
                };
                (world_id, String::new(), owner_id)
            }
            "session_character" => {
                let Some((session_id, character_id)) = owner_id.rsplit_once(':') else {
                    continue;
                };
                let world_id = match schema_world.clone() {
                    Some(world_id) => Some(world_id),
                    None => session_worlds.get(session_id).cloned(),
                };
                let Some(world_id) = world_id else {
                    continue;
                };
                (world_id, session_id.to_string(), character_id.to_string())
            }
            _ => continue,
        };
        let Some(object_id) =
            mapped_object(&OagRepository::new(conn), "characters", &character_id)?
        else {
            continue;
        };
        let Some(object) = OagRepository::new(conn).object(&object_id)? else {
            continue;
        };
        let Some(object_type) =
            OagRepository::new(conn).object_type(&world_id, &object.object_type)?
        else {
            continue;
        };
        let Some(definition) = object_type
            .attributes
            .into_iter()
            .find(|item| item.key == key)
        else {
            continue;
        };
        let turn = if session_id.is_empty() {
            0
        } else {
            conn.query_row(
                "SELECT COALESCE(MAX(turn_index), 0) FROM turn_journal WHERE session_id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .map_err(db_err)?
        };
        let value: serde_json::Value =
            serde_json::from_str(&value_json).unwrap_or(serde_json::Value::Null);
        let normalized_type = if value_type == "text" {
            definition.value_type
        } else {
            value_type
        };
        conn.execute(
            "INSERT OR IGNORE INTO oag_attributes
             (id, world_id, session_id, object_id, key, value_json, value_type,
              valid_from_turn, valid_to_turn, source, confidence, asserted_by_id,
              source_event_id, visibility, truth_status, supersedes_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, ?9, 1.0, NULL, NULL,
                     'scene', 'asserted', NULL, ?10)",
            params![
                format!("oag-attribute:legacy:{}", id),
                world_id,
                session_id,
                object_id,
                key,
                serde_json::to_string(&value).map_err(|error| error.to_string())?,
                normalized_type,
                turn,
                source,
                Utc::now().to_rfc3339(),
            ],
        )
        .map_err(db_err)?;
    }
    Ok(())
}

fn session_worlds(conn: &Connection) -> Result<HashMap<String, String>, String> {
    let mut stmt = conn
        .prepare("SELECT DISTINCT session_id, world_id FROM memory_entities ORDER BY session_id, world_id")
        .map_err(db_err)?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(db_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_err)?;
    let mut candidates: HashMap<String, Vec<String>> = HashMap::new();
    for (session_id, world_id) in rows {
        candidates.entry(session_id).or_default().push(world_id);
    }
    Ok(candidates
        .into_iter()
        .filter_map(|(session_id, worlds)| {
            if worlds.len() == 1 {
                Some((session_id, worlds[0].clone()))
            } else {
                None
            }
        })
        .collect())
}

fn character_world(conn: &Connection, character_id: &str) -> Result<Option<String>, String> {
    conn.query_row(
        "SELECT world_id FROM characters WHERE id = ?1",
        params![character_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(db_err)
}

fn mapped_object(
    repo: &OagRepository<'_>,
    source_table: &str,
    source_id: &str,
) -> Result<Option<String>, String> {
    let Some(target) = repo.legacy_target(source_table, source_id)? else {
        return Ok(None);
    };
    if repo.object(&target)?.is_some() {
        Ok(Some(target))
    } else {
        Ok(None)
    }
}

fn resolve_actor(
    conn: &Connection,
    world_id: &str,
    session_id: &str,
    reference: &str,
) -> Result<Option<String>, String> {
    let reference = reference.trim();
    if reference.is_empty() {
        return Ok(None);
    }
    let repo = OagRepository::new(conn);
    if let Some(id) = mapped_object(&repo, "memory_entities", reference)? {
        return actor_object(&repo, world_id, session_id, &id);
    }
    if let Some(id) = mapped_object(&repo, "characters", reference)? {
        return actor_object(&repo, world_id, session_id, &id);
    }
    for scope in [session_id, ""] {
        if let Some(object) = repo.object_by_canonical_key(world_id, scope, reference)? {
            return actor_object(&repo, world_id, session_id, &object.id);
        }
    }
    if let Some(object) = repo.object(reference)? {
        if object.world_id == world_id
            && (object.session_id == session_id || object.session_id.is_empty())
        {
            return actor_object(&repo, world_id, session_id, &object.id);
        }
    }
    let normalized = crate::services::game_engine::memory::normalize_memory_text(reference);
    let mut stmt = conn
        .prepare(
            "SELECT id, name, name_normalized, aliases_json
             FROM oag_objects
             WHERE world_id = ?1 AND (session_id = ?2 OR session_id = '') AND retired_at = ''",
        )
        .map_err(db_err)?;
    let rows = stmt
        .query_map(params![world_id, session_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(db_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_err)?;
    let mut matches = Vec::new();
    for (id, name, name_normalized, aliases_json) in rows {
        let aliases: Vec<String> = serde_json::from_str(&aliases_json).unwrap_or_default();
        let matched = crate::services::game_engine::memory::normalize_memory_text(&name)
            == normalized
            || name_normalized == normalized
            || aliases.iter().any(|alias| {
                crate::services::game_engine::memory::normalize_memory_text(alias) == normalized
            });
        if matched {
            matches.push(id);
        }
    }
    if matches.len() == 1 {
        actor_object(&repo, world_id, session_id, &matches[0])
    } else {
        Ok(None)
    }
}

fn actor_object(
    repo: &OagRepository<'_>,
    world_id: &str,
    session_id: &str,
    object_id: &str,
) -> Result<Option<String>, String> {
    let Some(object) = repo.object(object_id)? else {
        return Ok(None);
    };
    if object.world_id != world_id
        || (object.session_id != session_id && !object.session_id.is_empty())
        || !object.retired_at.is_empty()
    {
        return Ok(None);
    }
    let is_actor = repo.is_actor_type(&world_id, &object.object_type)?;
    Ok(is_actor.then_some(object.id))
}

fn db_err(error: rusqlite::Error) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::oag::{OagLinkTypeDefinition, OagObjectTypeDefinition};

    fn legacy_schema(conn: &Connection) {
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE memory_entities (
               id TEXT PRIMARY KEY, world_id TEXT NOT NULL, session_id TEXT NOT NULL,
               name TEXT NOT NULL, name_normalized TEXT NOT NULL, entity_type TEXT NOT NULL,
               aliases_json TEXT NOT NULL DEFAULT '[]', first_seen_turn INTEGER NOT NULL DEFAULT 0,
               last_seen_turn INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL DEFAULT ''
             );
             CREATE TABLE memory_relations (
               id TEXT PRIMARY KEY, world_id TEXT NOT NULL, session_id TEXT NOT NULL,
               subject_entity_id TEXT NOT NULL, predicate TEXT NOT NULL,
               object_entity_id TEXT, object_text TEXT NOT NULL DEFAULT '',
               valid_from_turn INTEGER NOT NULL DEFAULT 0, invalid_at_turn INTEGER,
               source TEXT NOT NULL DEFAULT 'llm_extraction', confidence REAL NOT NULL DEFAULT 0.7,
               asserted_by TEXT NOT NULL DEFAULT '', asserted_by_id TEXT,
               visibility TEXT NOT NULL DEFAULT 'scene', truth_status TEXT NOT NULL DEFAULT 'asserted',
               witnesses_json TEXT NOT NULL DEFAULT '[]'
             );
             CREATE TABLE attribute_schemas (
               id TEXT PRIMARY KEY, world_id TEXT, key TEXT NOT NULL, value_type TEXT NOT NULL
             );
             CREATE TABLE attribute_values (
               id TEXT PRIMARY KEY, schema_id TEXT NOT NULL, owner_type TEXT NOT NULL,
               owner_id TEXT NOT NULL, value_json TEXT NOT NULL DEFAULT 'null',
               source TEXT NOT NULL DEFAULT 'system'
             );
             CREATE TABLE characters (id TEXT PRIMARY KEY, world_id TEXT NOT NULL);
             CREATE TABLE turn_journal (
               session_id TEXT NOT NULL, turn_index INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE memories (id TEXT PRIMARY KEY);",
        )
        .unwrap();
    }

    #[test]
    fn schema_is_idempotent_and_has_frozen_shape() {
        let conn = Connection::open_in_memory().unwrap();
        legacy_schema(&conn);
        apply(&conn).unwrap();
        apply(&conn).unwrap();

        let tables: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table' AND name LIKE 'oag_%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let indexes: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'index' AND name LIKE 'idx_oag_%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tables, oag_schema::OAG_TABLE_COUNT as i64);
        assert_eq!(indexes, oag_schema::OAG_INDEX_COUNT as i64);
        assert_eq!(schema_version(&conn).unwrap(), OAG_SCHEMA_VERSION);
        assert!(!OagRepository::new(&conn)
            .is_actor_type("missing", "missing")
            .unwrap());
        let violations: i64 = conn
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(violations, 0);
    }

    #[test]
    fn legacy_entities_relations_and_knowledge_migrate_deterministically() {
        let conn = Connection::open_in_memory().unwrap();
        legacy_schema(&conn);
        apply(&conn).unwrap();
        let repo = OagRepository::new(&conn);
        repo.upsert_object_type(
            "world-1",
            &OagObjectTypeDefinition {
                key: "suspect".to_string(),
                label: "嫌疑人".to_string(),
                parent: None,
                actor: true,
                platform_role: String::new(),
                attributes: Vec::new(),
            },
            false,
        )
        .unwrap();
        repo.upsert_link_type(
            "world-1",
            &OagLinkTypeDefinition {
                key: "knows".to_string(),
                label: "认识".to_string(),
                domain: vec!["suspect".to_string()],
                range: vec!["suspect".to_string()],
                functional: false,
                symmetric: false,
                transitive: false,
                temporal: true,
                inverse: None,
            },
            false,
        )
        .unwrap();
        conn.execute_batch(
            "INSERT INTO memory_entities
               (id, world_id, session_id, name, name_normalized, entity_type,
                first_seen_turn, last_seen_turn)
             VALUES ('entity-a', 'world-1', 'session-1', '甲', '甲', 'suspect', 1, 4),
                    ('entity-b', 'world-1', 'session-1', '乙', '乙', 'suspect', 2, 4);
             INSERT INTO memory_relations
               (id, world_id, session_id, subject_entity_id, predicate, object_entity_id,
                valid_from_turn, asserted_by, asserted_by_id, visibility, witnesses_json)
             VALUES ('relation-1', 'world-1', 'session-1', 'entity-a', 'knows', 'entity-b',
                     3, '甲', 'entity-a', 'scene', '[\"乙\"]');",
        )
        .unwrap();

        migrate_legacy(&conn).unwrap();
        migrate_legacy(&conn).unwrap();

        let objects: i64 = conn
            .query_row("SELECT COUNT(*) FROM oag_objects", [], |row| row.get(0))
            .unwrap();
        let links: i64 = conn
            .query_row("SELECT COUNT(*) FROM oag_links", [], |row| row.get(0))
            .unwrap();
        let knowledge: i64 = conn
            .query_row("SELECT COUNT(*) FROM oag_knowledge", [], |row| row.get(0))
            .unwrap();
        let mappings: i64 = conn
            .query_row("SELECT COUNT(*) FROM oag_legacy_id_map", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!((objects, links, knowledge, mappings), (2, 1, 6, 3));
        let target = repo
            .legacy_target("memory_entities", "entity-a")
            .unwrap()
            .unwrap();
        let object = repo.object(&target).unwrap().unwrap();
        assert_eq!(object.visibility, "scene");
        assert_eq!(object.name, "甲");
    }

    #[test]
    fn undeclared_legacy_types_stay_in_legacy_storage() {
        let conn = Connection::open_in_memory().unwrap();
        legacy_schema(&conn);
        apply(&conn).unwrap();
        conn.execute(
            "INSERT INTO memory_entities
             (id, world_id, session_id, name, name_normalized, entity_type)
             VALUES ('entity-x', 'world-1', 'session-1', '未知', '未知', 'invented')",
            [],
        )
        .unwrap();
        migrate_legacy(&conn).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM oag_objects", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
        assert!(repo(&conn)
            .legacy_target("memory_entities", "entity-x")
            .unwrap()
            .is_none());
    }

    #[test]
    fn newer_schema_is_rejected_without_downgrade() {
        let conn = Connection::open_in_memory().unwrap();
        legacy_schema(&conn);
        apply(&conn).unwrap();
        conn.execute(
            "UPDATE oag_schema_meta SET schema_version = 999 WHERE id = 1",
            [],
        )
        .unwrap();
        assert!(apply(&conn).is_err());
        assert_eq!(schema_version(&conn).unwrap(), 999);
    }

    fn repo(conn: &Connection) -> OagRepository<'_> {
        OagRepository::new(conn)
    }
}
