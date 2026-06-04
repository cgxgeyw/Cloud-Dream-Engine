use crate::models::mcp_tool::*;
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn list_mcp_tools(state: State<'_, AppState>) -> Result<Vec<McpToolDefinition>, String> {
    let db = state.db.lock().await;
    let repo = crate::db::repositories::mcp_tool_repo::McpToolRepository::new(db.conn());
    repo.list()
}

#[tauri::command]
pub async fn create_mcp_tool(
    state: State<'_, AppState>,
    request: McpToolCreateRequest,
) -> Result<McpToolDefinition, String> {
    let db = state.db.lock().await;
    let repo = crate::db::repositories::mcp_tool_repo::McpToolRepository::new(db.conn());
    repo.create(&request)
}

#[tauri::command]
pub async fn update_mcp_tool(
    state: State<'_, AppState>,
    id: String,
    request: McpToolCreateRequest,
) -> Result<McpToolDefinition, String> {
    let db = state.db.lock().await;
    let repo = crate::db::repositories::mcp_tool_repo::McpToolRepository::new(db.conn());
    repo.update(&id, &request)
}

#[tauri::command]
pub async fn delete_mcp_tool(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let db = state.db.lock().await;
    let repo = crate::db::repositories::mcp_tool_repo::McpToolRepository::new(db.conn());
    repo.delete(&id)
}
