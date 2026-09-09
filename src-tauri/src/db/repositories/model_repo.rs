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
        let mut sql = "SELECT id, name, model_type, provider, model_id, base_url, api_key, max_tokens, streaming_enabled, is_default, input_modalities FROM model_configs".to_string();
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
                input_modalities: parse_input_modalities(&row.get::<_, String>(10)?),
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
            .prepare("SELECT id, name, model_type, provider, model_id, base_url, api_key, max_tokens, streaming_enabled, is_default, input_modalities FROM model_configs WHERE id = ?1")
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
                    input_modalities: parse_input_modalities(&row.get::<_, String>(10)?),
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
            "INSERT INTO model_configs (id, name, model_type, provider, model_id, base_url, api_key, max_tokens, streaming_enabled, is_default, input_modalities) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
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
                format_input_modalities(&req.input_modalities),
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
        // 掩码回传值表示「用户未改密钥」→ 保留库中真实 key；空字符串表示显式清空。
        let api_key = match req.api_key.as_deref() {
            None => existing.api_key,
            Some(value) if is_masked_api_key(value) => existing.api_key,
            Some(value) => value.trim().to_string(),
        };
        let max_tokens = normalize_max_tokens(req.max_tokens.unwrap_or(existing.max_tokens));
        let streaming_enabled = req.streaming_enabled.unwrap_or(existing.streaming_enabled);
        let input_modalities = normalize_input_modalities(
            req.input_modalities
                .clone()
                .unwrap_or(existing.input_modalities),
        );
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
            input_modalities,
        };

        // L6: 主 UPDATE 不直接写 is_default=1,否则在 set_default 清理同类默认之前
        // 会出现"多个默认"的中间态。这里先写 0,默认标志统一交给 set_default 落定。
        self.conn.execute(
            "UPDATE model_configs SET name = ?1, model_type = ?2, provider = ?3, model_id = ?4, base_url = ?5, api_key = ?6, max_tokens = ?7, streaming_enabled = ?8, is_default = ?9, input_modalities = ?10 WHERE id = ?11",
            params![
                updated.name,
                updated.model_type,
                updated.provider,
                updated.model_id,
                updated.base_url,
                updated.api_key,
                updated.max_tokens,
                if updated.streaming_enabled { 1 } else { 0 },
                0,
                format_input_modalities(&updated.input_modalities),
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
        // L6: 清空同类默认 + 置位本行,放进事务内原子完成,避免中间态出现 0 或多个默认。
        let tx = self.conn.unchecked_transaction().map_err(|e| e.to_string())?;
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
        tx.commit().map_err(|e| e.to_string())?;
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

/// input_modalities 只保留已知模态（image/audio），去重后按字典序返回。
pub(crate) fn normalize_input_modalities(values: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    for value in values {
        let normalized = value.trim().to_ascii_lowercase();
        if matches!(normalized.as_str(), "image" | "audio") {
            seen.insert(normalized);
        }
    }
    seen.into_iter().collect()
}

/// input_modalities 以 JSON 数组文本存库。
fn format_input_modalities(values: &[String]) -> String {
    serde_json::to_string(&normalize_input_modalities(values.to_vec()))
        .unwrap_or_else(|_| "[]".to_string())
}

fn parse_input_modalities(raw: &str) -> Vec<String> {
    let parsed: Vec<String> = serde_json::from_str(raw).unwrap_or_default();
    normalize_input_modalities(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_request(modalities: &[&str]) -> ModelConfigCreateRequest {
        ModelConfigCreateRequest {
            name: "多模态模型".to_string(),
            model_type: "text".to_string(),
            provider: "openai".to_string(),
            model_id: "gpt-test".to_string(),
            base_url: "http://localhost".to_string(),
            api_key: String::new(),
            max_tokens: 1200,
            streaming_enabled: true,
            is_default: false,
            input_modalities: modalities.iter().map(|value| value.to_string()).collect(),
        }
    }

    #[test]
    fn input_modalities_survive_a_db_round_trip() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        crate::db::schema::create_tables(&conn).expect("create schema");
        let repo = ModelRepository::new(&conn);

        // 默认（未声明任何模态）→ 空数组
        let plain = repo.create(&create_request(&[])).expect("create model");
        assert!(plain.input_modalities.is_empty());

        // 声明后写读一致；未知模态被过滤、去重
        let declared = repo
            .create(&create_request(&["image", "audio", "image", "hologram"]))
            .expect("create model");
        assert_eq!(declared.input_modalities, vec!["audio", "image"]);
        let fetched = repo.get(&declared.id).expect("get").expect("exists");
        assert_eq!(fetched.input_modalities, vec!["audio", "image"]);

        // update 只改模态，不动其它字段
        let updated = repo
            .update(
                &declared.id,
                &ModelConfigUpdateRequest {
                    input_modalities: Some(vec!["image".to_string()]),
                    ..Default::default()
                },
            )
            .expect("update");
        assert_eq!(updated.input_modalities, vec!["image"]);
        assert_eq!(updated.model_id, "gpt-test");
    }
}
