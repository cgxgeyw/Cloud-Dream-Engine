use crate::models::plugin::*;
use rusqlite::Connection;

pub struct PluginRepository<'a> {
    conn: &'a Connection,
}

impl<'a> PluginRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn list(&self) -> Result<Vec<PluginDefinition>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM plugins ORDER BY name")
            .map_err(|e| e.to_string())?;

        let plugins = stmt
            .query_map([], |row| {
                Ok(PluginDefinition {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    enabled: row.get::<_, i32>(2)? != 0,
                    description: row.get(3)?,
                    hooks: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        Ok(plugins)
    }
}
