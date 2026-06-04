use crate::models::model_config::*;
use rusqlite::{params, Connection};

pub struct ModelRepository<'a> {
    conn: &'a Connection,
}

impl<'a> ModelRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn list(&self, model_type: Option<&str>) -> Result<Vec<ModelConfig>, String> {
        let mut sql = "SELECT id, name, model_type, provider, model_id, base_url, api_key, max_tokens, streaming_enabled, is_default FROM model_configs".to_string();
        if model_type.is_some() {
            sql.push_str(" WHERE model_type = ?1");
        }
        sql.push_str(" ORDER BY model_type, name");

        let mut stmt = self.conn.prepare(&sql).map_err(|e| e.to_string())?;

        let row_to_model = |row: &rusqlite::Row| -> rusqlite::Result<ModelConfig> {
            Ok(ModelConfig {
                id: row.get(0)?,
                name: row.get(1)?,
                model_type: row.get(2)?,
                provider: row.get(3)?,
                model_id: row.get(4)?,
                base_url: row.get(5)?,
                api_key: row.get(6)?,
                max_tokens: row.get(7)?,
                streaming_enabled: row.get::<_, i32>(8)? != 0,
                is_default: row.get::<_, i32>(9)? != 0,
            })
        };

        let mut models = Vec::new();
        match model_type {
            Some(mt) => {
                let rows = stmt
                    .query_map(params![mt], row_to_model)
                    .map_err(|e| e.to_string())?;
                for row in rows {
                    models.push(row.map_err(|e| e.to_string())?);
                }
            }
            None => {
                let rows = stmt
                    .query_map([], row_to_model)
                    .map_err(|e| e.to_string())?;
                for row in rows {
                    models.push(row.map_err(|e| e.to_string())?);
                }
            }
        };

        Ok(models)
    }

    pub fn get(&self, id: &str) -> Result<Option<ModelConfig>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, model_type, provider, model_id, base_url, api_key, max_tokens, streaming_enabled, is_default FROM model_configs WHERE id = ?1")
            .map_err(|e| e.to_string())?;

        let mut rows = stmt
            .query_map(params![id], |row| {
                Ok(ModelConfig {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    model_type: row.get(2)?,
                    provider: row.get(3)?,
                    model_id: row.get(4)?,
                    base_url: row.get(5)?,
                    api_key: row.get(6)?,
                    max_tokens: row.get(7)?,
                    streaming_enabled: row.get::<_, i32>(8)? != 0,
                    is_default: row.get::<_, i32>(9)? != 0,
                })
            })
            .map_err(|e| e.to_string())?;

        match rows.next() {
            Some(row) => Ok(Some(row.map_err(|e| e.to_string())?)),
            None => Ok(None),
        }
    }

    pub fn create(&self, req: &ModelConfigCreateRequest) -> Result<ModelConfig, String> {
        let base_id = normalize_model_id(&req.name);
        let mut id = base_id.clone();
        while self.exists(&id)? {
            id = format!(
                "{}-{}",
                base_id,
                &uuid::Uuid::new_v4().simple().to_string()[..4]
            );
        }
        let model_type = normalize_model_type(&req.model_type);
        let name = req.name.trim().to_string();
        let provider = req.provider.trim().to_string();
        let model_id = req.model_id.trim().to_string();
        let base_url = req.base_url.trim().to_string();
        let api_key = req.api_key.trim().to_string();
        let max_tokens = normalize_max_tokens(req.max_tokens);
        let streaming_enabled = req.streaming_enabled;
        let is_default = req.is_default;
        if is_default {
            self.conn
                .execute(
                    "UPDATE model_configs SET is_default = 0 WHERE model_type = ?1",
                    params![model_type.as_str()],
                )
                .map_err(|e| e.to_string())?;
        }
        self.conn.execute(
            "INSERT INTO model_configs (id, name, model_type, provider, model_id, base_url, api_key, max_tokens, streaming_enabled, is_default) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                id,
                name,
                model_type,
                provider,
                model_id,
                base_url,
                api_key,
                max_tokens,
                if streaming_enabled { 1 } else { 0 },
                if is_default { 1 } else { 0 },
            ],
        )
        .map_err(|e| e.to_string())?;

        self.get(&id)?
            .ok_or_else(|| "Failed to create model".to_string())
    }

    pub fn update(&self, id: &str, req: &ModelConfigUpdateRequest) -> Result<ModelConfig, String> {
        let existing = self.get(id)?.ok_or_else(|| "Model not found".to_string())?;
        let name = req.name.clone().unwrap_or(existing.name).trim().to_string();
        let model_type = req
            .model_type
            .clone()
            .map(|value| normalize_model_type(&value))
            .unwrap_or(existing.model_type);
        let provider = req
            .provider
            .clone()
            .unwrap_or(existing.provider)
            .trim()
            .to_string();
        let model_id = req
            .model_id
            .clone()
            .unwrap_or(existing.model_id)
            .trim()
            .to_string();
        let base_url = req
            .base_url
            .clone()
            .unwrap_or(existing.base_url)
            .trim()
            .to_string();
        let api_key = req
            .api_key
            .clone()
            .unwrap_or(existing.api_key)
            .trim()
            .to_string();
        let max_tokens = normalize_max_tokens(req.max_tokens.unwrap_or(existing.max_tokens));
        let streaming_enabled = req.streaming_enabled.unwrap_or(existing.streaming_enabled);
        let updated = ModelConfig {
            id: existing.id.clone(),
            name,
            model_type,
            provider,
            model_id,
            base_url,
            api_key,
            max_tokens,
            streaming_enabled,
            is_default: req.is_default.unwrap_or(existing.is_default),
        };

        self.conn.execute(
            "UPDATE model_configs SET name = ?1, model_type = ?2, provider = ?3, model_id = ?4, base_url = ?5, api_key = ?6, max_tokens = ?7, streaming_enabled = ?8, is_default = ?9 WHERE id = ?10",
            params![
                updated.name,
                updated.model_type,
                updated.provider,
                updated.model_id,
                updated.base_url,
                updated.api_key,
                updated.max_tokens,
                if updated.streaming_enabled { 1 } else { 0 },
                if updated.is_default { 1 } else { 0 },
                id,
            ],
        )
        .map_err(|e| e.to_string())?;

        if updated.is_default {
            self.set_default(id)?;
        }

        self.get(id)?
            .ok_or_else(|| "Failed to update model".to_string())
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM model_configs WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn set_default(&self, id: &str) -> Result<(), String> {
        let model = self.get(id)?.ok_or_else(|| "Model not found".to_string())?;
        self.conn
            .execute(
                "UPDATE model_configs SET is_default = 0 WHERE model_type = ?1",
                params![model.model_type],
            )
            .map_err(|e| e.to_string())?;
        self.conn
            .execute(
                "UPDATE model_configs SET is_default = 1 WHERE id = ?1",
                params![id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn exists(&self, id: &str) -> Result<bool, String> {
        for table in ["model_configs", "worlds", "characters"] {
            let mut stmt = self
                .conn
                .prepare(&format!("SELECT 1 FROM {table} WHERE id = ?1 LIMIT 1"))
                .map_err(|e| e.to_string())?;
            let exists = stmt
                .query_row(params![id], |_| Ok(()))
                .map(|_| true)
                .or_else(|error| match error {
                    rusqlite::Error::QueryReturnedNoRows => Ok(false),
                    other => Err(other.to_string()),
                })?;
            if exists {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

fn normalize_model_id(name: &str) -> String {
    let slug = name
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        format!("model-{}", &uuid::Uuid::new_v4().simple().to_string()[..8])
    } else {
        format!("model-{slug}")
    }
}

fn normalize_model_type(model_type: &str) -> String {
    match model_type.trim().to_ascii_lowercase().as_str() {
        "text" => "text".to_string(),
        "image" => "image".to_string(),
        "embedding" => "embedding".to_string(),
        _ => "text".to_string(),
    }
}

fn normalize_max_tokens(max_tokens: i32) -> i32 {
    if max_tokens <= 0 {
        1200
    } else {
        max_tokens.clamp(1, 32768)
    }
}
