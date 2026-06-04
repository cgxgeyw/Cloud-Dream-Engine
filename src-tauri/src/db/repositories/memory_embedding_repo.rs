use chrono::Utc;
use rusqlite::{params, Connection};
use std::collections::HashMap;

pub struct MemoryEmbeddingRepository<'a> {
    conn: &'a Connection,
}

impl<'a> MemoryEmbeddingRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn list_by_model_and_memory_ids(
        &self,
        model_key: &str,
        memory_ids: &[String],
    ) -> Result<HashMap<String, Vec<f32>>, String> {
        if model_key.trim().is_empty() || memory_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let placeholders = (0..memory_ids.len())
            .map(|index| format!("?{}", index + 2))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT memory_id, vector_json FROM memory_embeddings WHERE model_key = ?1 AND memory_id IN ({})",
            placeholders
        );

        let mut params_values: Vec<&dyn rusqlite::types::ToSql> =
            Vec::with_capacity(memory_ids.len() + 1);
        params_values.push(&model_key);
        for memory_id in memory_ids {
            params_values.push(memory_id);
        }

        let mut stmt = self.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params_values.as_slice(), |row| {
                let memory_id: String = row.get(0)?;
                let vector_json: String = row.get(1)?;
                Ok((memory_id, vector_json))
            })
            .map_err(|e| e.to_string())?;

        let mut vectors = HashMap::new();
        for row in rows {
            let (memory_id, vector_json) = row.map_err(|e| e.to_string())?;
            let vector = serde_json::from_str::<Vec<f32>>(&vector_json).unwrap_or_default();
            if !vector.is_empty() {
                vectors.insert(memory_id, vector);
            }
        }
        Ok(vectors)
    }

    pub fn upsert(&self, memory_id: &str, model_key: &str, vector: &[f32]) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO memory_embeddings (memory_id, model_key, vector_json, updated_at) VALUES (?1, ?2, ?3, ?4)",
                params![
                    memory_id.trim(),
                    model_key.trim(),
                    serde_json::to_string(vector).unwrap_or_else(|_| "[]".to_string()),
                    Utc::now().to_rfc3339(),
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
