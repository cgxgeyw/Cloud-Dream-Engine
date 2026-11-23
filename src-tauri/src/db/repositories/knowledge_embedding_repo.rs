use chrono::Utc;
use rusqlite::{params, Connection};
use std::collections::HashMap;

pub struct KnowledgeEmbeddingRepository<'a> {
    conn: &'a Connection,
}

impl<'a> KnowledgeEmbeddingRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// 读取某批文档已缓存的切片向量，键为 (document_id, chunk_index)。
    pub fn list_by_documents(
        &self,
        model_key: &str,
        document_ids: &[String],
    ) -> Result<HashMap<(String, usize), Vec<f32>>, String> {
        if model_key.trim().is_empty() || document_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let placeholders = (0..document_ids.len())
            .map(|index| format!("?{}", index + 2))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT document_id, chunk_index, vector_json FROM knowledge_chunk_embeddings WHERE model_key = ?1 AND document_id IN ({placeholders})"
        );
        let mut values: Vec<&dyn rusqlite::types::ToSql> =
            Vec::with_capacity(document_ids.len() + 1);
        values.push(&model_key);
        for document_id in document_ids {
            values.push(document_id);
        }
        let mut stmt = self.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(values.as_slice(), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let mut vectors = HashMap::new();
        for row in rows {
            let (document_id, chunk_index, vector_json) = row.map_err(|e| e.to_string())?;
            let vector = serde_json::from_str::<Vec<f32>>(&vector_json).unwrap_or_default();
            if !vector.is_empty() {
                vectors.insert((document_id, chunk_index.max(0) as usize), vector);
            }
        }
        Ok(vectors)
    }

    pub fn upsert(
        &self,
        document_id: &str,
        chunk_index: usize,
        model_key: &str,
        vector: &[f32],
    ) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO knowledge_chunk_embeddings (document_id, chunk_index, model_key, vector_json, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    document_id,
                    chunk_index as i64,
                    model_key.trim(),
                    serde_json::to_string(vector).unwrap_or_else(|_| "[]".to_string()),
                    Utc::now().to_rfc3339(),
                ],
            )
            .map_err(|e| e.to_string())
            .map(|_| ())
    }

    /// 文档正文或分块设置变化后清掉旧切片向量，避免索引错位。
    pub fn clear_document(&self, document_id: &str) -> Result<(), String> {
        self.conn
            .execute(
                "DELETE FROM knowledge_chunk_embeddings WHERE document_id = ?1",
                params![document_id],
            )
            .map_err(|e| e.to_string())
            .map(|_| ())
    }

    /// 知识库分块设置变化后，整库切片索引都会变，全部作废。
    pub fn clear_base(&self, knowledge_base_id: &str) -> Result<(), String> {
        self.conn
            .execute(
                "DELETE FROM knowledge_chunk_embeddings WHERE document_id IN (SELECT id FROM knowledge_documents WHERE knowledge_base_id = ?1)",
                params![knowledge_base_id],
            )
            .map_err(|e| e.to_string())
            .map(|_| ())
    }
}
