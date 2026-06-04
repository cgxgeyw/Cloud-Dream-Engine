use crate::models::mcp_tool::*;
use rusqlite::{params, Connection};
use std::collections::BTreeSet;

pub struct McpToolRepository<'a> {
    conn: &'a Connection,
}

impl<'a> McpToolRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn list(&self) -> Result<Vec<McpToolDefinition>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM mcp_tools ORDER BY name")
            .map_err(|e| e.to_string())?;

        let tools = stmt
            .query_map([], |row| {
                Ok(McpToolDefinition {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    server_name: row.get(3)?,
                    tool_name: row.get(4)?,
                    enabled: row.get::<_, i32>(5)? != 0,
                    exposure_policy: serde_json::from_str(&row.get::<_, String>(6)?)
                        .unwrap_or_default(),
                    risk_level: row.get(7)?,
                    trigger_keywords: serde_json::from_str(&row.get::<_, String>(8)?)
                        .unwrap_or_default(),
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        Ok(tools)
    }

    pub fn create(&self, req: &McpToolCreateRequest) -> Result<McpToolDefinition, String> {
        let base_id = normalize_tool_id(&req.name);
        let mut id = base_id.clone();
        while self.exists(&id)? {
            id = format!(
                "{}-{}",
                base_id,
                &uuid::Uuid::new_v4().simple().to_string()[..4]
            );
        }
        let exposure_policy = normalize_exposure_policy(req.exposure_policy.clone());
        let risk_level = normalize_risk_level(&req.risk_level);
        let trigger_keywords = normalize_keywords(&req.trigger_keywords);
        self.conn.execute(
            "INSERT INTO mcp_tools (id, name, description, server_name, tool_name, enabled, exposure_policy_json, risk_level, trigger_keywords_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                id,
                req.name.trim(),
                req.description.trim(),
                req.server_name.trim(),
                req.tool_name.trim(),
                if req.enabled { 1 } else { 0 },
                serde_json::to_string(&exposure_policy).unwrap_or_default(),
                risk_level,
                serde_json::to_string(&trigger_keywords).unwrap_or_default(),
            ],
        )
        .map_err(|e| e.to_string())?;

        Ok(McpToolDefinition {
            id,
            name: req.name.trim().to_string(),
            description: req.description.trim().to_string(),
            server_name: req.server_name.trim().to_string(),
            tool_name: req.tool_name.trim().to_string(),
            enabled: req.enabled,
            exposure_policy,
            risk_level: risk_level.to_string(),
            trigger_keywords,
        })
    }

    pub fn update(
        &self,
        id: &str,
        req: &McpToolCreateRequest,
    ) -> Result<McpToolDefinition, String> {
        let exposure_policy = normalize_exposure_policy(req.exposure_policy.clone());
        let risk_level = normalize_risk_level(&req.risk_level);
        let trigger_keywords = normalize_keywords(&req.trigger_keywords);
        self.conn.execute(
            "UPDATE mcp_tools SET name = ?1, description = ?2, server_name = ?3, tool_name = ?4, enabled = ?5, exposure_policy_json = ?6, risk_level = ?7, trigger_keywords_json = ?8 WHERE id = ?9",
            params![
                req.name.trim(),
                req.description.trim(),
                req.server_name.trim(),
                req.tool_name.trim(),
                if req.enabled { 1 } else { 0 },
                serde_json::to_string(&exposure_policy).unwrap_or_default(),
                risk_level,
                serde_json::to_string(&trigger_keywords).unwrap_or_default(),
                id,
            ],
        )
        .map_err(|e| e.to_string())?;

        let mut stmt = self
            .conn
            .prepare("SELECT * FROM mcp_tools WHERE id = ?1")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map(params![id], |row| {
                Ok(McpToolDefinition {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    server_name: row.get(3)?,
                    tool_name: row.get(4)?,
                    enabled: row.get::<_, i32>(5)? != 0,
                    exposure_policy: serde_json::from_str(&row.get::<_, String>(6)?)
                        .unwrap_or_default(),
                    risk_level: row.get(7)?,
                    trigger_keywords: serde_json::from_str(&row.get::<_, String>(8)?)
                        .unwrap_or_default(),
                })
            })
            .map_err(|e| e.to_string())?;

        match rows.next() {
            Some(row) => Ok(row.map_err(|e| e.to_string())?),
            None => Err("MCP tool not found".to_string()),
        }
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM mcp_tools WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn exists(&self, id: &str) -> Result<bool, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT 1 FROM mcp_tools WHERE id = ?1 LIMIT 1")
            .map_err(|e| e.to_string())?;
        let exists = stmt
            .query_row(params![id], |_| Ok(()))
            .map(|_| true)
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(false),
                other => Err(other.to_string()),
            })?;
        Ok(exists)
    }
}

fn normalize_tool_id(name: &str) -> String {
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
        format!(
            "mcp-tool-{}",
            &uuid::Uuid::new_v4().simple().to_string()[..8]
        )
    } else {
        format!("mcp-tool-{slug}")
    }
}

fn normalize_exposure_policy(value: serde_json::Value) -> serde_json::Value {
    let text = if let Some(text) = value.as_str() {
        text.trim().to_ascii_lowercase()
    } else if let Some(mode) = value.get("mode").and_then(|item| item.as_str()) {
        mode.trim().to_ascii_lowercase()
    } else {
        "on-demand".to_string()
    };
    match text.as_str() {
        "on-demand" | "manual-only" | "disabled" => serde_json::Value::String(text),
        _ => serde_json::Value::String("on-demand".to_string()),
    }
}

fn normalize_risk_level(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "low" => "low",
        "medium" => "medium",
        "high" => "high",
        _ => "low",
    }
}

fn normalize_keywords(values: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .iter()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .filter_map(|item| {
            if seen.insert(item.to_string()) {
                Some(item.to_string())
            } else {
                None
            }
        })
        .collect()
}
