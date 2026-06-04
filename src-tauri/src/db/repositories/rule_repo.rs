use rusqlite::{params, Connection};

use crate::models::rule::*;

pub struct RuleRepository<'a> {
    conn: &'a Connection,
}

impl<'a> RuleRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn list(&self, scope: Option<&str>) -> Result<Vec<RuleDefinition>, String> {
        let mut sql = "SELECT * FROM rules".to_string();
        if scope.is_some() {
            sql.push_str(" WHERE scope = ?1");
        }
        sql.push_str(" ORDER BY priority DESC, name");
        let mut stmt = self.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let row_to_rule = |row: &rusqlite::Row| -> rusqlite::Result<RuleDefinition> {
            Ok(RuleDefinition {
                id: row.get(0)?,
                scope: row.get(1)?,
                name: row.get(2)?,
                enabled: row.get::<_, i32>(3)? != 0,
                priority: row.get(4)?,
                description: row.get(5)?,
                condition: serde_json::from_str(&row.get::<_, String>(6)?).unwrap_or_default(),
                effects: serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or_default(),
            })
        };
        let rows = match scope {
            Some(scope_value) => stmt.query_map(params![scope_value], row_to_rule),
            None => stmt.query_map([], row_to_rule),
        }
        .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn get(&self, id: &str) -> Result<Option<RuleDefinition>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM rules WHERE id = ?1")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map(params![id], |row| {
                Ok(RuleDefinition {
                    id: row.get(0)?,
                    scope: row.get(1)?,
                    name: row.get(2)?,
                    enabled: row.get::<_, i32>(3)? != 0,
                    priority: row.get(4)?,
                    description: row.get(5)?,
                    condition: serde_json::from_str(&row.get::<_, String>(6)?).unwrap_or_default(),
                    effects: serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or_default(),
                })
            })
            .map_err(|e| e.to_string())?;
        rows.next().transpose().map_err(|e| e.to_string())
    }

    pub fn create(&self, request: &RuleCreateRequest) -> Result<RuleDefinition, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let scope = request.scope.trim().to_string();
        let name = request.name.trim().to_string();
        let description = request.description.trim().to_string();
        self.conn.execute(
            "INSERT INTO rules (id, scope, name, enabled, priority, description, condition_json, effects_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                scope,
                name,
                if request.enabled { 1 } else { 0 },
                request.priority,
                description,
                serde_json::to_string(&request.condition).unwrap_or_default(),
                serde_json::to_string(&request.effects).unwrap_or_default(),
            ],
        ).map_err(|e| e.to_string())?;
        self.get(&id)?
            .ok_or_else(|| "Failed to create rule".to_string())
    }

    pub fn update(&self, id: &str, request: &RuleCreateRequest) -> Result<RuleDefinition, String> {
        let scope = request.scope.trim().to_string();
        let name = request.name.trim().to_string();
        let description = request.description.trim().to_string();
        self.conn.execute(
            "UPDATE rules SET scope = ?1, name = ?2, enabled = ?3, priority = ?4, description = ?5, condition_json = ?6, effects_json = ?7 WHERE id = ?8",
            params![
                scope,
                name,
                if request.enabled { 1 } else { 0 },
                request.priority,
                description,
                serde_json::to_string(&request.condition).unwrap_or_default(),
                serde_json::to_string(&request.effects).unwrap_or_default(),
                id,
            ],
        ).map_err(|e| e.to_string())?;
        self.get(id)?.ok_or_else(|| "Rule not found".to_string())
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM rules WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
