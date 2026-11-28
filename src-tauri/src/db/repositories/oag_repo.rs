use crate::models::oag::{
    OagAttribute, OagKnowledge, OagLegacyIdMap, OagLink, OagLinkType, OagLinkTypeDefinition,
    OagObject, OagObjectType, OagObjectTypeDefinition,
};
use rusqlite::{params, types::Type, Connection, OptionalExtension};

pub struct OagRepository<'a> {
    conn: &'a Connection,
}

impl<'a> OagRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn upsert_object_type(
        &self,
        world_id: &str,
        definition: &OagObjectTypeDefinition,
        builtin: bool,
    ) -> Result<OagObjectType, String> {
        let id = format!("oag-type:{}:{}", world_id, definition.key);
        let attributes =
            serde_json::to_string(&definition.attributes).map_err(|error| error.to_string())?;
        self.conn
            .execute(
                "INSERT INTO oag_object_types
                 (id, world_id, key, label, parent_key, actor, platform_role, attributes_json, builtin)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(world_id, key) DO UPDATE SET
                   label = excluded.label,
                   parent_key = excluded.parent_key,
                   actor = excluded.actor,
                   platform_role = excluded.platform_role,
                   attributes_json = excluded.attributes_json,
                   builtin = excluded.builtin",
                params![
                    id,
                    world_id,
                    definition.key,
                    definition.label,
                    definition.parent,
                    definition.actor,
                    definition.platform_role,
                    attributes,
                    builtin,
                ],
            )
            .map_err(|error| error.to_string())?;
        self.object_type(world_id, &definition.key)?
            .ok_or_else(|| format!("对象类型写入后无法读取：{}", definition.key))
    }

    pub fn object_type(&self, world_id: &str, key: &str) -> Result<Option<OagObjectType>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, world_id, key, label, parent_key, actor, platform_role,
                        attributes_json, builtin
                 FROM oag_object_types WHERE world_id = ?1 AND key = ?2",
            )
            .map_err(|error| error.to_string())?;
        let mut rows = stmt
            .query_map(params![world_id, key], |row| {
                Ok(OagObjectType {
                    id: row.get(0)?,
                    world_id: row.get(1)?,
                    key: row.get(2)?,
                    label: row.get(3)?,
                    parent_key: row.get(4)?,
                    actor: row.get::<_, i64>(5)? != 0,
                    platform_role: row.get(6)?,
                    attributes: serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or_default(),
                    builtin: row.get::<_, i64>(8)? != 0,
                })
            })
            .map_err(|error| error.to_string())?;
        rows.next().map(|row| row.map_err(err)).transpose()
    }

    pub fn list_object_types(&self, world_id: &str) -> Result<Vec<OagObjectType>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, world_id, key, label, parent_key, actor, platform_role,
                        attributes_json, builtin
                 FROM oag_object_types WHERE world_id = ?1 ORDER BY key",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(params![world_id], |row| {
                Ok(OagObjectType {
                    id: row.get(0)?,
                    world_id: row.get(1)?,
                    key: row.get(2)?,
                    label: row.get(3)?,
                    parent_key: row.get(4)?,
                    actor: row.get::<_, i64>(5)? != 0,
                    platform_role: row.get(6)?,
                    attributes: serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or_default(),
                    builtin: row.get::<_, i64>(8)? != 0,
                })
            })
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(err)
    }

    pub fn upsert_link_type(
        &self,
        world_id: &str,
        definition: &OagLinkTypeDefinition,
        builtin: bool,
    ) -> Result<OagLinkType, String> {
        let id = format!("oag-link-type:{}:{}", world_id, definition.key);
        let domain =
            serde_json::to_string(&definition.domain).map_err(|error| error.to_string())?;
        let range = serde_json::to_string(&definition.range).map_err(|error| error.to_string())?;
        self.conn
            .execute(
                "INSERT INTO oag_link_types
                 (id, world_id, key, label, domain_json, range_json, functional, symmetric,
                  transitive, temporal, inverse, builtin)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                 ON CONFLICT(world_id, key) DO UPDATE SET
                   label = excluded.label,
                   domain_json = excluded.domain_json,
                   range_json = excluded.range_json,
                   functional = excluded.functional,
                   symmetric = excluded.symmetric,
                   transitive = excluded.transitive,
                   temporal = excluded.temporal,
                   inverse = excluded.inverse,
                   builtin = excluded.builtin",
                params![
                    id,
                    world_id,
                    definition.key,
                    definition.label,
                    domain,
                    range,
                    definition.functional,
                    definition.symmetric,
                    definition.transitive,
                    definition.temporal,
                    definition.inverse,
                    builtin,
                ],
            )
            .map_err(|error| error.to_string())?;
        self.link_type(world_id, &definition.key)?
            .ok_or_else(|| format!("关系类型写入后无法读取：{}", definition.key))
    }

    pub fn link_type(&self, world_id: &str, key: &str) -> Result<Option<OagLinkType>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, world_id, key, label, domain_json, range_json, functional,
                        symmetric, transitive, temporal, inverse, builtin
                 FROM oag_link_types WHERE world_id = ?1 AND key = ?2",
            )
            .map_err(|error| error.to_string())?;
        let mut rows = stmt
            .query_map(params![world_id, key], |row| {
                Ok(OagLinkType {
                    id: row.get(0)?,
                    world_id: row.get(1)?,
                    key: row.get(2)?,
                    label: row.get(3)?,
                    domain: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
                    range: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default(),
                    functional: row.get::<_, i64>(6)? != 0,
                    symmetric: row.get::<_, i64>(7)? != 0,
                    transitive: row.get::<_, i64>(8)? != 0,
                    temporal: row.get::<_, i64>(9)? != 0,
                    inverse: row.get(10)?,
                    builtin: row.get::<_, i64>(11)? != 0,
                })
            })
            .map_err(|error| error.to_string())?;
        rows.next().map(|row| row.map_err(err)).transpose()
    }

    pub fn list_link_types(&self, world_id: &str) -> Result<Vec<OagLinkType>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, world_id, key, label, domain_json, range_json, functional,
                        symmetric, transitive, temporal, inverse, builtin
                 FROM oag_link_types WHERE world_id = ?1 ORDER BY key",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(params![world_id], |row| {
                Ok(OagLinkType {
                    id: row.get(0)?,
                    world_id: row.get(1)?,
                    key: row.get(2)?,
                    label: row.get(3)?,
                    domain: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
                    range: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default(),
                    functional: row.get::<_, i64>(6)? != 0,
                    symmetric: row.get::<_, i64>(7)? != 0,
                    transitive: row.get::<_, i64>(8)? != 0,
                    temporal: row.get::<_, i64>(9)? != 0,
                    inverse: row.get(10)?,
                    builtin: row.get::<_, i64>(11)? != 0,
                })
            })
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(err)
    }

    pub fn upsert_object(&self, object: &OagObject) -> Result<OagObject, String> {
        let aliases = serde_json::to_string(&object.aliases).map_err(|error| error.to_string())?;
        self.conn
            .execute(
                "INSERT INTO oag_objects
                 (id, world_id, session_id, object_type, canonical_key, name, name_normalized,
                  aliases_json, description, source, visibility, asserted_by_id, source_event_id,
                  truth_status, created_turn, updated_turn, retired_at, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                        ?16, ?17, ?18, ?19)
                 ON CONFLICT(world_id, session_id, canonical_key) DO UPDATE SET
                   object_type = excluded.object_type,
                   name = excluded.name,
                   name_normalized = excluded.name_normalized,
                   aliases_json = excluded.aliases_json,
                   description = excluded.description,
                   source = excluded.source,
                   visibility = excluded.visibility,
                   asserted_by_id = excluded.asserted_by_id,
                   source_event_id = excluded.source_event_id,
                   truth_status = excluded.truth_status,
                   created_turn = MIN(oag_objects.created_turn, excluded.created_turn),
                   updated_turn = MAX(oag_objects.updated_turn, excluded.updated_turn),
                   retired_at = excluded.retired_at,
                   created_at = CASE WHEN oag_objects.created_at = '' THEN excluded.created_at
                                     ELSE oag_objects.created_at END,
                   updated_at = excluded.updated_at",
                params![
                    object.id,
                    object.world_id,
                    object.session_id,
                    object.object_type,
                    object.canonical_key,
                    object.name,
                    object.name_normalized,
                    aliases,
                    object.description,
                    object.source,
                    object.visibility,
                    object.asserted_by_id,
                    object.source_event_id,
                    object.truth_status,
                    object.created_turn,
                    object.updated_turn,
                    object.retired_at,
                    object.created_at,
                    object.updated_at,
                ],
            )
            .map_err(|error| error.to_string())?;
        self.object_by_canonical_key(&object.world_id, &object.session_id, &object.canonical_key)?
            .ok_or_else(|| format!("对象写入后无法读取：{}", object.canonical_key))
    }

    pub fn object(&self, id: &str) -> Result<Option<OagObject>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, world_id, session_id, object_type, canonical_key, name,
                        name_normalized, aliases_json, description, source, visibility,
                        asserted_by_id, source_event_id, truth_status, created_turn,
                        updated_turn, retired_at, created_at, updated_at
                 FROM oag_objects WHERE id = ?1",
            )
            .map_err(|error| error.to_string())?;
        let mut rows = stmt
            .query_map(params![id], map_object)
            .map_err(|error| error.to_string())?;
        rows.next().map(|row| row.map_err(err)).transpose()
    }

    pub fn object_by_canonical_key(
        &self,
        world_id: &str,
        session_id: &str,
        canonical_key: &str,
    ) -> Result<Option<OagObject>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, world_id, session_id, object_type, canonical_key, name,
                        name_normalized, aliases_json, description, source, visibility,
                        asserted_by_id, source_event_id, truth_status, created_turn,
                        updated_turn, retired_at, created_at, updated_at
                 FROM oag_objects
                 WHERE world_id = ?1 AND session_id = ?2 AND canonical_key = ?3",
            )
            .map_err(|error| error.to_string())?;
        let mut rows = stmt
            .query_map(params![world_id, session_id, canonical_key], map_object)
            .map_err(|error| error.to_string())?;
        rows.next().map(|row| row.map_err(err)).transpose()
    }

    pub fn replace_tbox(
        &self,
        world_id: &str,
        object_types: &[OagObjectTypeDefinition],
        link_types: &[OagLinkTypeDefinition],
        builtin: bool,
    ) -> Result<(), String> {
        self.conn
            .execute(
                "DELETE FROM oag_object_types WHERE world_id = ?1",
                params![world_id],
            )
            .map_err(|error| error.to_string())?;
        self.conn
            .execute(
                "DELETE FROM oag_link_types WHERE world_id = ?1",
                params![world_id],
            )
            .map_err(|error| error.to_string())?;
        for definition in object_types {
            self.upsert_object_type(world_id, definition, builtin)?;
        }
        for definition in link_types {
            self.upsert_link_type(world_id, definition, builtin)?;
        }
        Ok(())
    }

    pub fn replace_canon_attributes(&self, object_id: &str) -> Result<(), String> {
        self.conn
            .execute(
                "DELETE FROM oag_attributes
                 WHERE object_id = ?1 AND session_id = '' AND source = 'author'",
                params![object_id],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn insert_canon_attribute(&self, attribute: &OagAttribute) -> Result<(), String> {
        let value = serde_json::to_string(&attribute.value).map_err(|error| error.to_string())?;
        self.conn
            .execute(
                "INSERT INTO oag_attributes
                 (id, world_id, session_id, object_id, key, value_json, value_type,
                  valid_from_turn, valid_to_turn, source, confidence, asserted_by_id,
                  source_event_id, visibility, truth_status, supersedes_id, created_at)
                 VALUES (?1, ?2, '', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                params![
                    attribute.id,
                    attribute.world_id,
                    attribute.object_id,
                    attribute.key,
                    value,
                    attribute.value_type,
                    attribute.valid_from_turn,
                    attribute.valid_to_turn,
                    attribute.source,
                    attribute.confidence,
                    attribute.asserted_by_id,
                    attribute.source_event_id,
                    attribute.visibility,
                    attribute.truth_status,
                    attribute.supersedes_id,
                    attribute.created_at,
                ],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn replace_canon_links(&self, world_id: &str, links: &[OagLink]) -> Result<(), String> {
        self.conn
            .execute(
                "DELETE FROM oag_links
                 WHERE world_id = ?1 AND session_id = '' AND source = 'author'",
                params![world_id],
            )
            .map_err(|error| error.to_string())?;
        for link in links {
            self.conn
                .execute(
                    "INSERT INTO oag_links
                     (id, world_id, session_id, from_object_id, link_type, to_object_id,
                      to_value, original_predicate, attributes_json, valid_from_turn,
                      valid_to_turn, source, confidence, asserted_by_id, source_event_id,
                      visibility, truth_status, inferred, supersedes_id, created_at)
                     VALUES (?1, ?2, '', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                             ?14, ?15, ?16, ?17, ?18, ?19)",
                    params![
                        link.id,
                        link.world_id,
                        link.from_object_id,
                        link.link_type,
                        link.to_object_id,
                        link.to_value,
                        link.original_predicate,
                        serde_json::to_string(&link.attributes)
                            .map_err(|error| error.to_string())?,
                        link.valid_from_turn,
                        link.valid_to_turn,
                        link.source,
                        link.confidence,
                        link.asserted_by_id,
                        link.source_event_id,
                        link.visibility,
                        link.truth_status,
                        link.inferred,
                        link.supersedes_id,
                        link.created_at,
                    ],
                )
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub fn retire_canon_objects(
        &self,
        world_id: &str,
        retained_ids: &[String],
        retired_at: &str,
    ) -> Result<(), String> {
        self.conn
            .execute_batch(
                "CREATE TEMP TABLE IF NOT EXISTS oag_canon_retained_ids (id TEXT PRIMARY KEY);
                 DELETE FROM oag_canon_retained_ids;",
            )
            .map_err(|error| error.to_string())?;
        for chunk in retained_ids.chunks(500) {
            let placeholders = std::iter::repeat("(?)")
                .take(chunk.len())
                .collect::<Vec<_>>()
                .join(",");
            self.conn
                .execute(
                    &format!("INSERT INTO oag_canon_retained_ids (id) VALUES {placeholders}"),
                    rusqlite::params_from_iter(chunk),
                )
                .map_err(|error| error.to_string())?;
        }
        self.conn
            .execute(
                "UPDATE oag_objects SET retired_at = ?1, updated_at = ?1
                 WHERE world_id = ?2 AND session_id = '' AND source = 'author'
                   AND retired_at = ''
                   AND NOT EXISTS (
                     SELECT 1 FROM oag_canon_retained_ids WHERE id = oag_objects.id
                   )",
                params![retired_at, world_id],
            )
            .map_err(|error| error.to_string())?;
        self.conn
            .execute("DROP TABLE oag_canon_retained_ids", [])
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn list_canon_objects(&self, world_id: &str) -> Result<Vec<OagObject>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, world_id, session_id, object_type, canonical_key, name,
                        name_normalized, aliases_json, description, source, visibility,
                        asserted_by_id, source_event_id, truth_status, created_turn,
                        updated_turn, retired_at, created_at, updated_at
                 FROM oag_objects
                 WHERE world_id = ?1 AND session_id = '' AND retired_at = ''
                 ORDER BY canonical_key",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(params![world_id], map_object)
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(err)
    }

    pub fn list_canon_attributes(&self, world_id: &str) -> Result<Vec<OagAttribute>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, world_id, session_id, object_id, key, value_json, value_type,
                        valid_from_turn, valid_to_turn, source, confidence, asserted_by_id,
                        source_event_id, visibility, truth_status, supersedes_id, created_at
                 FROM oag_attributes
                 WHERE world_id = ?1 AND session_id = ''
                 ORDER BY object_id, key",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(params![world_id], |row| {
                Ok(OagAttribute {
                    id: row.get(0)?,
                    world_id: row.get(1)?,
                    session_id: row.get(2)?,
                    object_id: row.get(3)?,
                    key: row.get(4)?,
                    value: serde_json::from_str(&row.get::<_, String>(5)?).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(5, Type::Text, Box::new(error))
                    })?,
                    value_type: row.get(6)?,
                    valid_from_turn: row.get(7)?,
                    valid_to_turn: row.get(8)?,
                    source: row.get(9)?,
                    confidence: row.get(10)?,
                    asserted_by_id: row.get(11)?,
                    source_event_id: row.get(12)?,
                    visibility: row.get(13)?,
                    truth_status: row.get(14)?,
                    supersedes_id: row.get(15)?,
                    created_at: row.get(16)?,
                })
            })
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(err)
    }

    pub fn list_canon_links(&self, world_id: &str) -> Result<Vec<OagLink>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, world_id, session_id, from_object_id, link_type, to_object_id,
                        to_value, original_predicate, attributes_json, valid_from_turn,
                        valid_to_turn, source, confidence, asserted_by_id, source_event_id,
                        visibility, truth_status, inferred, supersedes_id, created_at
                 FROM oag_links
                 WHERE world_id = ?1 AND session_id = ''
                 ORDER BY from_object_id, link_type, to_object_id, to_value",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(params![world_id], |row| {
                Ok(OagLink {
                    id: row.get(0)?,
                    world_id: row.get(1)?,
                    session_id: row.get(2)?,
                    from_object_id: row.get(3)?,
                    link_type: row.get(4)?,
                    to_object_id: row.get(5)?,
                    to_value: row.get(6)?,
                    original_predicate: row.get(7)?,
                    attributes: serde_json::from_str(&row.get::<_, String>(8)?).map_err(
                        |error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                8,
                                Type::Text,
                                Box::new(error),
                            )
                        },
                    )?,
                    valid_from_turn: row.get(9)?,
                    valid_to_turn: row.get(10)?,
                    source: row.get(11)?,
                    confidence: row.get(12)?,
                    asserted_by_id: row.get(13)?,
                    source_event_id: row.get(14)?,
                    visibility: row.get(15)?,
                    truth_status: row.get(16)?,
                    inferred: row.get::<_, i64>(17)? != 0,
                    supersedes_id: row.get(18)?,
                    created_at: row.get(19)?,
                })
            })
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(err)
    }

    pub fn list_objects_scope(
        &self,
        world_id: &str,
        session_id: &str,
    ) -> Result<Vec<OagObject>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, world_id, session_id, object_type, canonical_key, name,
                        name_normalized, aliases_json, description, source, visibility,
                        asserted_by_id, source_event_id, truth_status, created_turn,
                        updated_turn, retired_at, created_at, updated_at
                 FROM oag_objects
                 WHERE world_id = ?1 AND (session_id = '' OR session_id = ?2)
                 ORDER BY created_turn, id",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(params![world_id, session_id], map_object)
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(err)
    }

    pub fn list_attributes_scope(
        &self,
        world_id: &str,
        session_id: &str,
    ) -> Result<Vec<OagAttribute>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, world_id, session_id, object_id, key, value_json, value_type,
                        valid_from_turn, valid_to_turn, source, confidence, asserted_by_id,
                        source_event_id, visibility, truth_status, supersedes_id, created_at
                 FROM oag_attributes
                 WHERE world_id = ?1 AND (session_id = '' OR session_id = ?2)
                 ORDER BY object_id, key, valid_from_turn, id",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(params![world_id, session_id], map_attribute)
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(err)
    }

    pub fn list_links_scope(
        &self,
        world_id: &str,
        session_id: &str,
    ) -> Result<Vec<OagLink>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, world_id, session_id, from_object_id, link_type, to_object_id,
                        to_value, original_predicate, attributes_json, valid_from_turn,
                        valid_to_turn, source, confidence, asserted_by_id, source_event_id,
                        visibility, truth_status, inferred, supersedes_id, created_at
                 FROM oag_links
                 WHERE world_id = ?1 AND (session_id = '' OR session_id = ?2)
                 ORDER BY from_object_id, link_type, to_object_id, to_value, id",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(params![world_id, session_id], map_link)
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(err)
    }

    pub fn list_events_scope(
        &self,
        world_id: &str,
        session_id: &str,
    ) -> Result<Vec<crate::models::oag::OagEvent>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, world_id, session_id, turn_index, ordinal, scene_id, event_type,
                        title, summary, details_json, source_message_id, dedupe_key,
                        visibility, importance, truth_status, created_at
                 FROM oag_events
                 WHERE world_id = ?1 AND (session_id = '' OR session_id = ?2)
                 ORDER BY turn_index, ordinal, id",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(params![world_id, session_id], map_event)
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(err)
    }

    pub fn list_knowledge_scope(
        &self,
        world_id: &str,
        session_id: &str,
    ) -> Result<Vec<OagKnowledge>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, world_id, session_id, actor_id, target_kind, target_id,
                        knowledge_type, access_kind, source_event_id, source_actor_id,
                        learned_turn, confidence, created_at
                 FROM oag_knowledge
                 WHERE world_id = ?1 AND (session_id = '' OR session_id = ?2)
                 ORDER BY learned_turn, actor_id, target_kind, target_id, id",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(params![world_id, session_id], |row| {
                Ok(OagKnowledge {
                    id: row.get(0)?,
                    world_id: row.get(1)?,
                    session_id: row.get(2)?,
                    actor_id: row.get(3)?,
                    target_kind: row.get(4)?,
                    target_id: row.get(5)?,
                    knowledge_type: row.get(6)?,
                    access_kind: row.get(7)?,
                    source_event_id: row.get(8)?,
                    source_actor_id: row.get(9)?,
                    learned_turn: row.get(10)?,
                    confidence: row.get(11)?,
                    created_at: row.get(12)?,
                })
            })
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(err)
    }

    pub fn list_event_objects_scope(
        &self,
        world_id: &str,
        session_id: &str,
    ) -> Result<Vec<crate::models::oag::OagEventObject>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT eo.event_id, eo.object_id, eo.role
                 FROM oag_event_objects eo
                 JOIN oag_events e ON e.id = eo.event_id
                 WHERE e.world_id = ?1 AND (e.session_id = '' OR e.session_id = ?2)
                 ORDER BY eo.event_id, eo.object_id, eo.role",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(params![world_id, session_id], |row| {
                Ok(crate::models::oag::OagEventObject {
                    event_id: row.get(0)?,
                    object_id: row.get(1)?,
                    role: row.get(2)?,
                })
            })
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(err)
    }

    pub fn list_scenes_scope(
        &self,
        world_id: &str,
        session_id: &str,
    ) -> Result<Vec<crate::models::oag::OagScene>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, world_id, session_id, runtime_scene_id, name, location_object_id,
                        background_hint, valid_from_turn, valid_to_turn, created_at
                 FROM oag_scenes
                 WHERE world_id = ?1 AND (session_id = '' OR session_id = ?2)
                 ORDER BY valid_from_turn, id",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(params![world_id, session_id], |row| {
                Ok(crate::models::oag::OagScene {
                    id: row.get(0)?,
                    world_id: row.get(1)?,
                    session_id: row.get(2)?,
                    runtime_scene_id: row.get(3)?,
                    name: row.get(4)?,
                    location_object_id: row.get(5)?,
                    background_hint: row.get(6)?,
                    valid_from_turn: row.get(7)?,
                    valid_to_turn: row.get(8)?,
                    created_at: row.get(9)?,
                })
            })
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(err)
    }

    pub fn list_scene_participants_scope(
        &self,
        world_id: &str,
        session_id: &str,
    ) -> Result<Vec<crate::models::oag::OagSceneParticipant>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT sp.id, sp.scene_id, sp.object_id, sp.role, sp.presence_mode,
                        sp.from_turn, sp.to_turn
                 FROM oag_scene_participants sp
                 JOIN oag_scenes s ON s.id = sp.scene_id
                 WHERE s.world_id = ?1 AND (s.session_id = '' OR s.session_id = ?2)
                 ORDER BY sp.scene_id, sp.object_id, sp.from_turn, sp.id",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(params![world_id, session_id], |row| {
                Ok(crate::models::oag::OagSceneParticipant {
                    id: row.get(0)?,
                    scene_id: row.get(1)?,
                    object_id: row.get(2)?,
                    role: row.get(3)?,
                    presence_mode: row.get(4)?,
                    from_turn: row.get(5)?,
                    to_turn: row.get(6)?,
                })
            })
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(err)
    }

    pub fn is_actor_type(&self, world_id: &str, key: &str) -> Result<bool, String> {
        Ok(self
            .conn
            .query_row(
                "SELECT actor FROM oag_object_types WHERE world_id = ?1 AND key = ?2",
                params![world_id, key],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .is_some_and(|value| value != 0))
    }

    pub fn record_legacy_map(&self, mapping: &OagLegacyIdMap) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT INTO oag_legacy_id_map
                 (source_table, source_id, world_id, session_id, target_table, target_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(source_table, source_id) DO UPDATE SET
                   world_id = excluded.world_id,
                   session_id = excluded.session_id,
                   target_table = excluded.target_table,
                   target_id = excluded.target_id",
                params![
                    mapping.source_table,
                    mapping.source_id,
                    mapping.world_id,
                    mapping.session_id,
                    mapping.target_table,
                    mapping.target_id,
                ],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn legacy_target(
        &self,
        source_table: &str,
        source_id: &str,
    ) -> Result<Option<String>, String> {
        self.conn
            .query_row(
                "SELECT target_id FROM oag_legacy_id_map
                 WHERE source_table = ?1 AND source_id = ?2",
                params![source_table, source_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(err)
    }
}

fn map_object(row: &rusqlite::Row<'_>) -> rusqlite::Result<OagObject> {
    Ok(OagObject {
        id: row.get(0)?,
        world_id: row.get(1)?,
        session_id: row.get(2)?,
        object_type: row.get(3)?,
        canonical_key: row.get(4)?,
        name: row.get(5)?,
        name_normalized: row.get(6)?,
        aliases: serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or_default(),
        description: row.get(8)?,
        source: row.get(9)?,
        visibility: row.get(10)?,
        asserted_by_id: row.get(11)?,
        source_event_id: row.get(12)?,
        truth_status: row.get(13)?,
        created_turn: row.get(14)?,
        updated_turn: row.get(15)?,
        retired_at: row.get(16)?,
        created_at: row.get(17)?,
        updated_at: row.get(18)?,
    })
}

fn map_attribute(row: &rusqlite::Row<'_>) -> rusqlite::Result<OagAttribute> {
    Ok(OagAttribute {
        id: row.get(0)?,
        world_id: row.get(1)?,
        session_id: row.get(2)?,
        object_id: row.get(3)?,
        key: row.get(4)?,
        value: serde_json::from_str(&row.get::<_, String>(5)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(5, Type::Text, Box::new(error))
        })?,
        value_type: row.get(6)?,
        valid_from_turn: row.get(7)?,
        valid_to_turn: row.get(8)?,
        source: row.get(9)?,
        confidence: row.get(10)?,
        asserted_by_id: row.get(11)?,
        source_event_id: row.get(12)?,
        visibility: row.get(13)?,
        truth_status: row.get(14)?,
        supersedes_id: row.get(15)?,
        created_at: row.get(16)?,
    })
}

fn map_link(row: &rusqlite::Row<'_>) -> rusqlite::Result<OagLink> {
    Ok(OagLink {
        id: row.get(0)?,
        world_id: row.get(1)?,
        session_id: row.get(2)?,
        from_object_id: row.get(3)?,
        link_type: row.get(4)?,
        to_object_id: row.get(5)?,
        to_value: row.get(6)?,
        original_predicate: row.get(7)?,
        attributes: serde_json::from_str(&row.get::<_, String>(8)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(8, Type::Text, Box::new(error))
        })?,
        valid_from_turn: row.get(9)?,
        valid_to_turn: row.get(10)?,
        source: row.get(11)?,
        confidence: row.get(12)?,
        asserted_by_id: row.get(13)?,
        source_event_id: row.get(14)?,
        visibility: row.get(15)?,
        truth_status: row.get(16)?,
        inferred: row.get::<_, i64>(17)? != 0,
        supersedes_id: row.get(18)?,
        created_at: row.get(19)?,
    })
}

fn map_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<crate::models::oag::OagEvent> {
    Ok(crate::models::oag::OagEvent {
        id: row.get(0)?,
        world_id: row.get(1)?,
        session_id: row.get(2)?,
        turn_index: row.get(3)?,
        ordinal: row.get(4)?,
        scene_id: row.get(5)?,
        event_type: row.get(6)?,
        title: row.get(7)?,
        summary: row.get(8)?,
        details: serde_json::from_str(&row.get::<_, String>(9)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(9, Type::Text, Box::new(error))
        })?,
        source_message_id: row.get(10)?,
        dedupe_key: row.get(11)?,
        visibility: row.get(12)?,
        importance: row.get(13)?,
        truth_status: row.get(14)?,
        created_at: row.get(15)?,
    })
}

fn err(error: rusqlite::Error) -> String {
    error.to_string()
}
