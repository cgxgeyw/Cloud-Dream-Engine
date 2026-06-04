use crate::models::memory::*;
use rusqlite::{params, Connection};
use std::collections::HashSet;

pub struct MemoryRepository<'a> {
    conn: &'a Connection,
}

impl<'a> MemoryRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn list(&self, query: &MemoryQueryParams) -> Result<Vec<MemoryEntry>, String> {
        let mut sql = "SELECT * FROM memories WHERE 1=1".to_string();
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut param_idx = 1;

        if let Some(ref world_id) = query.world_id {
            sql.push_str(&format!(" AND world_id = ?{}", param_idx));
            param_values.push(Box::new(world_id.clone()));
            param_idx += 1;
        }
        if let Some(ref session_id) = query.session_id {
            sql.push_str(&format!(" AND session_id = ?{}", param_idx));
            param_values.push(Box::new(session_id.clone()));
            param_idx += 1;
        }
        if let Some(ref character_id) = query.character_id {
            sql.push_str(&format!(" AND character_id = ?{}", param_idx));
            param_values.push(Box::new(character_id.clone()));
            param_idx += 1;
        }
        if let Some(ref layer) = query.layer {
            sql.push_str(&format!(" AND layer = ?{}", param_idx));
            param_values.push(Box::new(layer.clone()));
        }

        sql.push_str(" ORDER BY created_at DESC");

        if let Some(limit) = query.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }

        let mut stmt = self.conn.prepare(&sql).map_err(|e| e.to_string())?;

        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();

        let memories = stmt
            .query_map(params_refs.as_slice(), |row| {
                Ok(MemoryEntry {
                    id: row.get(0)?,
                    world_id: row.get(1)?,
                    session_id: row.get(2)?,
                    character_id: row.get(3)?,
                    layer: row.get(4)?,
                    content: row.get(5)?,
                    source: row.get(6)?,
                    importance: row.get(7)?,
                    created_at: row.get(8)?,
                    turn_index: row.get(9)?,
                    conversation_id: row.get(10)?,
                    event_id: row.get(11)?,
                    item_id: row.get(12)?,
                    scene_id: row.get(13)?,
                    memory_type: row.get(14)?,
                    speaker: row.get(15)?,
                    role: row.get(16)?,
                    location: row.get(17)?,
                    participants: serde_json::from_str(&row.get::<_, String>(18)?)
                        .unwrap_or_default(),
                    keywords: serde_json::from_str(&row.get::<_, String>(19)?).unwrap_or_default(),
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        Ok(memories)
    }

    pub fn insert(&self, memory: &MemoryEntry) -> Result<(), String> {
        let world_id = memory.world_id.trim().to_string();
        let session_id = memory.session_id.trim().to_string();
        let character_id = memory.character_id.trim().to_string();
        let layer = memory.layer.trim().to_string();
        let content = memory.content.trim().to_string();
        let source = memory.source.trim().to_string();
        let created_at = memory.created_at.trim().to_string();
        let memory_type = memory.memory_type.trim().to_string();
        let speaker = memory
            .speaker
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let role = memory
            .role
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let location = memory
            .location
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let participants = normalize_list(&memory.participants);
        let keywords = normalize_list(&memory.keywords);
        self.conn.execute(
            "INSERT INTO memories (id, world_id, session_id, character_id, layer, content, source, importance, created_at, turn_index, conversation_id, event_id, item_id, scene_id, memory_type, speaker, role, location, participants_json, keywords_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
            params![
                memory.id,
                world_id,
                session_id,
                character_id,
                layer,
                content,
                source,
                memory.importance,
                created_at,
                memory.turn_index,
                memory.conversation_id,
                memory.event_id,
                memory.item_id,
                memory.scene_id,
                memory_type,
                speaker,
                role,
                location,
                serde_json::to_string(&participants).unwrap_or_default(),
                serde_json::to_string(&keywords).unwrap_or_default(),
            ],
        )
        .map_err(|e| e.to_string())?;

        Ok(())
    }
}

fn normalize_list(values: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .filter_map(|value| {
            if seen.insert(value.to_string()) {
                Some(value.to_string())
            } else {
                None
            }
        })
        .collect()
}
