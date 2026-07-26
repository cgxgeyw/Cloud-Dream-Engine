use crate::models::mcp_server::*;
use crate::state::AppState;
use tauri::State;

/// 连通性测试 / 工具发现的返回结果。
#[derive(Debug, serde::Serialize)]
pub struct McpServerProbeResult {
    pub ok: bool,
    pub error: Option<String>,
    /// server 声明的工具（tools/list 原样返回），供配置页选择。
    pub tools: Vec<serde_json::Value>,
    /// 本平台是否支持该 server 的传输方式。
    pub platform_supported: bool,
}

#[tauri::command]
pub async fn list_mcp_servers(state: State<'_, AppState>) -> Result<Vec<McpServerConfig>, String> {
    let db = state.db.lock().await;
    crate::db::repositories::mcp_server_repo::McpServerRepository::new(db.conn()).list()
}

#[tauri::command]
pub async fn create_mcp_server(
    state: State<'_, AppState>,
    request: McpServerUpsertRequest,
) -> Result<McpServerConfig, String> {
    let db = state.db.lock().await;
    crate::db::repositories::mcp_server_repo::McpServerRepository::new(db.conn()).create(&request)
}

#[tauri::command]
pub async fn update_mcp_server(
    state: State<'_, AppState>,
    id: String,
    request: McpServerUpsertRequest,
) -> Result<McpServerConfig, String> {
    let db = state.db.lock().await;
    crate::db::repositories::mcp_server_repo::McpServerRepository::new(db.conn())
        .update(&id, &request)
}

#[tauri::command]
pub async fn delete_mcp_server(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let db = state.db.lock().await;
    crate::db::repositories::mcp_server_repo::McpServerRepository::new(db.conn()).delete(&id)
}

/// 测试连接并列出工具。stdio 在安卓上直接返回「本平台不支持」，不做无谓的尝试。
#[tauri::command]
pub async fn probe_mcp_server(
    state: State<'_, AppState>,
    id: String,
) -> Result<McpServerProbeResult, String> {
    let config = {
        let db = state.db.lock().await;
        crate::db::repositories::mcp_server_repo::McpServerRepository::new(db.conn()).get(&id)?
    };
    let Some(config) = config else {
        return Err(format!("MCP server 不存在：{id}"));
    };
    let platform_supported = transport_supported_on_platform(&config.transport);
    if !platform_supported {
        return Ok(McpServerProbeResult {
            ok: false,
            error: Some(transport_unsupported_reason(&config.transport)),
            tools: Vec::new(),
            platform_supported,
        });
    }
    match crate::services::mcp::client::list_server_tools(&config).await {
        Ok(tools) => Ok(McpServerProbeResult {
            ok: true,
            error: None,
            tools,
            platform_supported,
        }),
        Err(error) => Ok(McpServerProbeResult {
            ok: false,
            error: Some(error),
            tools: Vec::new(),
            platform_supported,
        }),
    }
}
