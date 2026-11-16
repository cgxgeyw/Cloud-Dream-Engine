use crate::models::memory::MemorySummaryRange;
use rusqlite::{params, Connection};

pub struct MemorySummaryRepository<'a> {
    conn: &'a Connection,
}

impl<'a> MemorySummaryRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// 插入一条总结范围。`UNIQUE(session_id, character_id, bucket_index, scene_id)`
    /// 保证同一桶同一场景只生成一次；返回 false 表示已存在（或写入被忽略）。
    pub fn insert(&self, range: &MemorySummaryRange) -> Result<bool, String> {
        let changed = self
            .conn
            .execute(
                "INSERT OR IGNORE INTO memory_summary_ranges (
                    memory_id, world_id, session_id, character_id, bucket_index,
                    bucket_start, bucket_end, scene_id, scene_name, turn_start,
                    turn_end, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    range.memory_id,
                    range.world_id,
                    range.session_id,
                    range.character_id,
                    range.bucket_index,
                    range.bucket_start,
                    range.bucket_end,
                    range.scene_id,
                    range.scene_name,
                    range.turn_start,
                    range.turn_end,
                    range.created_at,
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(changed > 0)
    }

    pub fn list_by_character(
        &self,
        session_id: &str,
        character_id: &str,
    ) -> Result<Vec<MemorySummaryRange>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT memory_id, world_id, session_id, character_id, bucket_index,
                        bucket_start, bucket_end, scene_id, scene_name, turn_start,
                        turn_end, created_at
                 FROM memory_summary_ranges
                 WHERE session_id = ?1 AND character_id = ?2
                 ORDER BY bucket_index ASC, scene_id ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![session_id, character_id], |row| {
                Ok(MemorySummaryRange {
                    memory_id: row.get(0)?,
                    world_id: row.get(1)?,
                    session_id: row.get(2)?,
                    character_id: row.get(3)?,
                    bucket_index: row.get(4)?,
                    bucket_start: row.get(5)?,
                    bucket_end: row.get(6)?,
                    scene_id: row.get(7)?,
                    scene_name: row.get(8)?,
                    turn_start: row.get(9)?,
                    turn_end: row.get(10)?,
                    created_at: row.get(11)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn has_bucket(
        &self,
        session_id: &str,
        character_id: &str,
        bucket_index: i32,
    ) -> Result<bool, String> {
        let exists: i64 = self
            .conn
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM memory_summary_ranges
                    WHERE session_id = ?1 AND character_id = ?2 AND bucket_index = ?3
                )",
                params![session_id, character_id, bucket_index],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        Ok(exists != 0)
    }
}
