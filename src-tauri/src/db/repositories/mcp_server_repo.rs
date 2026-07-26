use crate::models::mcp_server::*;
use rusqlite::{params, Connection};

const SELECT_COLUMNS: &str = "id, name, transport, command, args_json, env_json, url, headers_json, auth_token, enabled, timeout_ms, max_result_bytes";

pub struct McpServerRepository<'a> {
    conn: &'a Connection,
}

impl<'a> McpServerRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn list(&self) -> Result<Vec<McpServerConfig>, String> {
        let sql = format!("SELECT {SELECT_COLUMNS} FROM mcp_servers ORDER BY name");
        let mut stmt = self.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], row_to_config)
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn get(&self, id: &str) -> Result<Option<McpServerConfig>, String> {
        let sql = format!("SELECT {SELECT_COLUMNS} FROM mcp_servers WHERE id = ?1");
        let mut stmt = self.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map(params![id], row_to_config)
            .map_err(|e| e.to_string())?;
        match rows.next() {
            Some(row) => row.map(Some).map_err(|e| e.to_string()),
            None => Ok(None),
        }
    }

    pub fn create(&self, req: &McpServerUpsertRequest) -> Result<McpServerConfig, String> {
        let config = normalize(new_id(&req.name), req);
        self.conn
            .execute(
                "INSERT INTO mcp_servers (id, name, transport, command, args_json, env_json, url, headers_json, auth_token, enabled, timeout_ms, max_result_bytes)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                rusqlite::params_from_iter(bind_params(&config)),
            )
            .map_err(|e| e.to_string())?;
        Ok(config)
    }

    pub fn update(
        &self,
        id: &str,
        req: &McpServerUpsertRequest,
    ) -> Result<McpServerConfig, String> {
        let config = normalize(id.to_string(), req);
        let affected = self
            .conn
            .execute(
                "UPDATE mcp_servers SET name = ?2, transport = ?3, command = ?4, args_json = ?5, env_json = ?6, url = ?7, headers_json = ?8, auth_token = ?9, enabled = ?10, timeout_ms = ?11, max_result_bytes = ?12 WHERE id = ?1",
                rusqlite::params_from_iter(bind_params(&config)),
            )
            .map_err(|e| e.to_string())?;
        if affected == 0 {
            return Err(format!("MCP server 不存在：{id}"));
        }
        Ok(config)
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM mcp_servers WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        // 解除工具与该 server 的绑定，避免留下悬空引用。
        self.conn
            .execute(
                "UPDATE mcp_tools SET server_id = '' WHERE server_id = ?1",
                params![id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// 绑定参数使用 owned 值，避免返回引用临时量。
fn bind_params(config: &McpServerConfig) -> Vec<rusqlite::types::Value> {
    use rusqlite::types::Value as SqlValue;
    vec![
        SqlValue::Text(config.id.clone()),
        SqlValue::Text(config.name.clone()),
        SqlValue::Text(config.transport.clone()),
        SqlValue::Text(config.command.clone()),
        SqlValue::Text(serde_json::to_string(&config.args).unwrap_or_else(|_| "[]".to_string())),
        SqlValue::Text(serde_json::to_string(&config.env).unwrap_or_else(|_| "{}".to_string())),
        SqlValue::Text(config.url.clone()),
        SqlValue::Text(serde_json::to_string(&config.headers).unwrap_or_else(|_| "{}".to_string())),
        SqlValue::Text(config.auth_token.clone()),
        SqlValue::Integer(if config.enabled { 1 } else { 0 }),
        SqlValue::Integer(config.timeout_ms),
        SqlValue::Integer(config.max_result_bytes),
    ]
}

fn new_id(name: &str) -> String {
    let slug: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_alphanumeric() { ch } else { '-' })
        .collect();
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        format!("mcp-server-{}", uuid::Uuid::new_v4())
    } else {
        format!("mcp-server-{slug}")
    }
}

/// 归一化：传输方式白名单、映射字段必须是对象、超时与结果上限落进合法区间。
fn normalize(id: String, req: &McpServerUpsertRequest) -> McpServerConfig {
    let transport = match req.transport.trim() {
        TRANSPORT_STDIO => TRANSPORT_STDIO,
        _ => TRANSPORT_HTTP,
    }
    .to_string();
    let as_object = |value: &serde_json::Value| {
        if value.is_object() {
            value.clone()
        } else {
            serde_json::json!({})
        }
    };
    let timeout_ms = if req.timeout_ms <= 0 {
        DEFAULT_MCP_TIMEOUT_MS
    } else {
        req.timeout_ms.clamp(1_000, MAX_MCP_TIMEOUT_MS)
    };
    let max_result_bytes = if req.max_result_bytes <= 0 {
        DEFAULT_MCP_MAX_RESULT_BYTES
    } else {
        req.max_result_bytes.clamp(1_024, MAX_MCP_MAX_RESULT_BYTES)
    };
    McpServerConfig {
        id,
        name: req.name.trim().to_string(),
        transport,
        command: req.command.trim().to_string(),
        args: req
            .args
            .iter()
            .map(|arg| arg.trim().to_string())
            .filter(|arg| !arg.is_empty())
            .collect(),
        env: as_object(&req.env),
        url: req.url.trim().to_string(),
        headers: as_object(&req.headers),
        auth_token: req.auth_token.trim().to_string(),
        enabled: req.enabled,
        timeout_ms,
        max_result_bytes,
    }
}

fn row_to_config(row: &rusqlite::Row<'_>) -> rusqlite::Result<McpServerConfig> {
    let args: String = row.get(4)?;
    let env: String = row.get(5)?;
    let headers: String = row.get(7)?;
    Ok(McpServerConfig {
        id: row.get(0)?,
        name: row.get(1)?,
        transport: row.get(2)?,
        command: row.get(3)?,
        args: serde_json::from_str(&args).unwrap_or_default(),
        env: serde_json::from_str(&env).unwrap_or_else(|_| serde_json::json!({})),
        url: row.get(6)?,
        headers: serde_json::from_str(&headers).unwrap_or_else(|_| serde_json::json!({})),
        auth_token: row.get(8)?,
        enabled: row.get::<_, i64>(9)? != 0,
        timeout_ms: row.get(10)?,
        max_result_bytes: row.get(11)?,
    })
}
