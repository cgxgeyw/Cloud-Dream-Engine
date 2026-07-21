use crate::models::memory::MemoryEntity;
use chrono::Utc;
use rusqlite::{params, Connection};

pub struct MemoryEntityRepository<'a> {
    conn: &'a Connection,
}

impl<'a> MemoryEntityRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// 按 (session_id, name_normalized) 查找实体;命中则 bump 提及计数与最后出现回合,
    /// 未命中则插入新实体。surface_form 是本次出现的原始写法,normalized 是其归一化形式。
    pub fn upsert(
        &self,
        world_id: &str,
        session_id: &str,
        surface_form: &str,
        normalized: &str,
        entity_type: &str,
        turn_index: i32,
    ) -> Result<MemoryEntity, String> {
        if let Some(mut entity) = self.find_by_normalized(session_id, normalized)? {
            self.conn
                .execute(
                    "UPDATE memory_entities
                     SET mention_count = mention_count + 1,
                         last_seen_turn = MAX(last_seen_turn, ?1)
                     WHERE id = ?2",
                    params![turn_index, entity.id],
                )
                .map_err(|e| e.to_string())?;
            entity.mention_count += 1;
            entity.last_seen_turn = entity.last_seen_turn.max(turn_index);
            return Ok(entity);
        }

        let entity = MemoryEntity {
            id: format!("ent-{}", uuid::Uuid::new_v4().simple()),
            world_id: world_id.to_string(),
            session_id: session_id.to_string(),
            name: surface_form.trim().to_string(),
            name_normalized: normalized.to_string(),
            entity_type: entity_type.trim().to_string(),
            aliases: Vec::new(),
            mention_count: 1,
            first_seen_turn: turn_index,
            last_seen_turn: turn_index,
            created_at: Utc::now().to_rfc3339(),
        };
        self.conn
            .execute(
                "INSERT INTO memory_entities
                 (id, world_id, session_id, name, name_normalized, entity_type, aliases_json,
                  mention_count, first_seen_turn, last_seen_turn, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    entity.id,
                    entity.world_id,
                    entity.session_id,
                    entity.name,
                    entity.name_normalized,
                    entity.entity_type,
                    "[]",
                    entity.mention_count,
                    entity.first_seen_turn,
                    entity.last_seen_turn,
                    entity.created_at,
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(entity)
    }

    /// 按归一化名或别名查找会话内实体。
    pub fn find_by_normalized(
        &self,
        session_id: &str,
        normalized: &str,
    ) -> Result<Option<MemoryEntity>, String> {
        let entities = self.list_by_session(session_id)?;
        Ok(entities.into_iter().find(|entity| {
            entity.name_normalized == normalized
                || entity.aliases.iter().any(|alias| alias == normalized)
        }))
    }

    pub fn list_by_session(&self, session_id: &str) -> Result<Vec<MemoryEntity>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, world_id, session_id, name, name_normalized, entity_type,
                        aliases_json, mention_count, first_seen_turn, last_seen_turn, created_at
                 FROM memory_entities WHERE session_id = ?1
                 ORDER BY mention_count DESC, last_seen_turn DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![session_id], |row| {
                Ok(MemoryEntity {
                    id: row.get(0)?,
                    world_id: row.get(1)?,
                    session_id: row.get(2)?,
                    name: row.get(3)?,
                    name_normalized: row.get(4)?,
                    entity_type: row.get(5)?,
                    aliases: serde_json::from_str(&row.get::<_, String>(6)?).unwrap_or_default(),
                    mention_count: row.get(7)?,
                    first_seen_turn: row.get(8)?,
                    last_seen_turn: row.get(9)?,
                    created_at: row.get(10)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }
}
