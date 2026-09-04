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

/// 工具包导出格式：{"format":"dream-mcp-tools","version":1,"tools":[...]}。
/// 引擎硬编码内置工具（切换场景等）不导出——它们由宿主自带，导出的 id 也
/// 无法被导入方复用。
#[tauri::command]
pub async fn export_mcp_tools(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> Result<String, String> {
    let db = state.db.lock().await;
    let repo = crate::db::repositories::mcp_tool_repo::McpToolRepository::new(db.conn());
    let all = repo.list()?;
    let wanted: std::collections::BTreeSet<&str> =
        ids.iter().map(|item| item.trim()).collect();
    let tools: Vec<McpToolDefinition> = all
        .into_iter()
        .filter(|tool| wanted.contains(tool.id.as_str()))
        .filter(|tool| !is_builtin_mcp_tool_id(&tool.id))
        .collect();
    let payload = serde_json::json!({
        "format": "dream-mcp-tools",
        "version": 1,
        "tools": tools,
    });
    serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McpToolImportSummary {
    pub imported: usize,
    pub updated: usize,
    pub skipped: usize,
}

/// 导入工具包 JSON：按 id upsert（已存在则整行更新，保证世界白名单里的 id 不变）。
/// 引擎硬编码内置工具 id 一律跳过，避免覆盖宿主行为。
#[tauri::command]
pub async fn import_mcp_tools(
    state: State<'_, AppState>,
    json: String,
) -> Result<McpToolImportSummary, String> {
    let payload: serde_json::Value =
        serde_json::from_str(&json).map_err(|error| format!("工具包不是合法 JSON：{error}"))?;
    if payload.get("format").and_then(|value| value.as_str()) != Some("dream-mcp-tools") {
        return Err("不是有效的工具包文件（format 应为 dream-mcp-tools）".to_string());
    }
    if payload.get("version").and_then(|value| value.as_i64()) != Some(1) {
        return Err("不支持的工具包版本（仅支持 version 1）".to_string());
    }
    let tools = payload
        .get("tools")
        .and_then(|value| value.as_array())
        .ok_or_else(|| "工具包缺少 tools 数组".to_string())?
        .clone();

    let db = state.db.lock().await;
    let repo = crate::db::repositories::mcp_tool_repo::McpToolRepository::new(db.conn());
    let existing: std::collections::BTreeSet<String> =
        repo.list()?.into_iter().map(|tool| tool.id).collect();

    let mut summary = McpToolImportSummary {
        imported: 0,
        updated: 0,
        skipped: 0,
    };
    for (index, item) in tools.iter().enumerate() {
        let tool: McpToolDefinition = match serde_json::from_value(item.clone()) {
            Ok(tool) => tool,
            Err(error) => {
                return Err(format!("工具包第 {} 项解析失败：{error}", index + 1));
            }
        };
        if tool.id.trim().is_empty() || tool.name.trim().is_empty() {
            return Err(format!("工具包第 {} 项缺少 id 或 name", index + 1));
        }
        if is_builtin_mcp_tool_id(&tool.id) {
            summary.skipped += 1;
            continue;
        }
        let request = McpToolCreateRequest {
            name: tool.name.clone(),
            description: tool.description.clone(),
            server_name: tool.server_name.clone(),
            tool_name: tool.tool_name.clone(),
            enabled: tool.enabled,
            exposure_policy: tool.exposure_policy.clone(),
            risk_level: tool.risk_level.clone(),
            trigger_keywords: tool.trigger_keywords.clone(),
            input_schema: tool.input_schema.clone(),
            server_id: tool.server_id.clone(),
            impl_kind: tool.impl_kind.clone(),
            impl_config: tool.impl_config.clone(),
        };
        if existing.contains(&tool.id) {
            repo.update(&tool.id, &request)?;
            summary.updated += 1;
        } else {
            repo.insert_with_id(&tool.id, &request)?;
            summary.imported += 1;
        }
    }
    Ok(summary)
}
