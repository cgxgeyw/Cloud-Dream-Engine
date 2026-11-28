use crate::models::memory::MemoryRelation;
use rusqlite::{params, Connection, Row};

const RELATION_COLUMNS: &str =
    "id, world_id, session_id, subject_entity_id, predicate, object_entity_id,
                    object_text, valid_from_turn, invalid_at_turn, source, confidence, created_at,
                    asserted_by, asserted_by_id, visibility, truth_status, witnesses_json";

pub struct MemoryRelationRepository<'a> {
    conn: &'a Connection,
}

impl<'a> MemoryRelationRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert(&self, relation: &MemoryRelation) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT INTO memory_relations
                 (id, world_id, session_id, subject_entity_id, predicate, object_entity_id,
                  object_text, valid_from_turn, invalid_at_turn, source, confidence, created_at,
                  asserted_by, asserted_by_id, visibility, truth_status, witnesses_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                         ?13, ?14, ?15, ?16, ?17)",
                params![
                    relation.id,
                    relation.world_id,
                    relation.session_id,
                    relation.subject_entity_id,
                    relation.predicate,
                    relation.object_entity_id,
                    relation.object_text,
                    relation.valid_from_turn,
                    relation.invalid_at_turn,
                    relation.source,
                    relation.confidence,
                    relation.created_at,
                    relation.asserted_by,
                    relation.asserted_by_id,
                    relation.visibility,
                    relation.truth_status,
                    serde_json::to_string(&relation.witnesses).map_err(|e| e.to_string())?,
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 会话内当前有效(未失效)的关系,按生效回合倒序。
    pub fn list_active_by_session(&self, session_id: &str) -> Result<Vec<MemoryRelation>, String> {
        self.list_by_session(session_id, true)
    }

    pub fn list_by_session(
        &self,
        session_id: &str,
        active_only: bool,
    ) -> Result<Vec<MemoryRelation>, String> {
        let sql = format!(
            "SELECT {RELATION_COLUMNS} FROM memory_relations
             WHERE session_id = ?1 {}
             ORDER BY valid_from_turn DESC",
            if active_only {
                "AND invalid_at_turn IS NULL"
            } else {
                ""
            }
        );
        let mut stmt = self.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![session_id], Self::row_to_relation)
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn list_visible_by_actor(
        &self,
        session_id: &str,
        active_only: bool,
        actor_id: Option<&str>,
        actor_name: Option<&str>,
    ) -> Result<Vec<MemoryRelation>, String> {
        Ok(self
            .list_by_session(session_id, active_only)?
            .into_iter()
            .filter(|relation| relation_visible_to(relation, actor_id, actor_name))
            .collect())
    }

    /// 查找同一主语+谓语下当前有效的关系(用于矛盾检测)。
    pub fn find_active(
        &self,
        session_id: &str,
        subject_entity_id: &str,
        predicate: &str,
    ) -> Result<Vec<MemoryRelation>, String> {
        let sql = format!(
            "SELECT {RELATION_COLUMNS} FROM memory_relations
             WHERE session_id = ?1 AND subject_entity_id = ?2 AND predicate = ?3
               AND invalid_at_turn IS NULL"
        );
        let mut stmt = self.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(
                params![session_id, subject_entity_id, predicate],
                Self::row_to_relation,
            )
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn find_active_for_actor(
        &self,
        session_id: &str,
        subject_entity_id: &str,
        predicate: &str,
        actor_id: Option<&str>,
        actor_name: Option<&str>,
    ) -> Result<Vec<MemoryRelation>, String> {
        Ok(self
            .find_active(session_id, subject_entity_id, predicate)?
            .into_iter()
            .filter(|relation| relation_visible_to(relation, actor_id, actor_name))
            .collect())
    }

    /// 作废一条关系:标记失效回合,不删除(保留"曾经怎样"的历史)。
    pub fn invalidate(&self, relation_id: &str, turn_index: i32) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE memory_relations SET invalid_at_turn = ?1
                 WHERE id = ?2 AND invalid_at_turn IS NULL",
                params![turn_index, relation_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn row_to_relation(row: &Row<'_>) -> rusqlite::Result<MemoryRelation> {
        let witnesses_json: String = row.get(16)?;
        Ok(MemoryRelation {
            id: row.get(0)?,
            world_id: row.get(1)?,
            session_id: row.get(2)?,
            subject_entity_id: row.get(3)?,
            predicate: row.get(4)?,
            object_entity_id: row.get(5)?,
            object_text: row.get(6)?,
            valid_from_turn: row.get(7)?,
            invalid_at_turn: row.get(8)?,
            source: row.get(9)?,
            confidence: row.get(10)?,
            created_at: row.get(11)?,
            asserted_by: row.get(12)?,
            asserted_by_id: row.get(13)?,
            visibility: row.get(14)?,
            truth_status: row.get(15)?,
            witnesses: serde_json::from_str(&witnesses_json).unwrap_or_default(),
        })
    }
}

fn relation_visible_to(
    relation: &MemoryRelation,
    actor_id: Option<&str>,
    actor_name: Option<&str>,
) -> bool {
    if relation.visibility == "public" {
        return true;
    }
    let is_speaker = actor_id.is_some_and(|id| {
        relation.asserted_by_id.as_deref() == Some(id)
            && !relation
                .asserted_by_id
                .as_deref()
                .unwrap_or_default()
                .is_empty()
    }) || actor_name
        .is_some_and(|name| !name.is_empty() && relation.asserted_by == name);
    match relation.visibility.as_str() {
        "private" => is_speaker,
        "scene" => {
            is_speaker
                || relation.witnesses.iter().any(|witness| {
                    actor_id.is_some_and(|id| witness == id)
                        || actor_name.is_some_and(|name| witness == name)
                })
        }
        _ => false,
    }
}
