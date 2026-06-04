use tauri::State;

use crate::models::rule::*;
use crate::state::AppState;

#[tauri::command]
pub async fn list_rules(
    state: State<'_, AppState>,
    scope: Option<String>,
) -> Result<Vec<RuleDefinition>, String> {
    let db = state.db.lock().await;
    let repo = crate::db::repositories::rule_repo::RuleRepository::new(db.conn());
    repo.list(scope.as_deref())
}

#[tauri::command]
pub async fn get_rule(state: State<'_, AppState>, id: String) -> Result<RuleDefinition, String> {
    let db = state.db.lock().await;
    let repo = crate::db::repositories::rule_repo::RuleRepository::new(db.conn());
    repo.get(&id)?.ok_or_else(|| "Rule not found".to_string())
}

#[tauri::command]
pub async fn create_rule(
    state: State<'_, AppState>,
    request: RuleCreateRequest,
) -> Result<RuleDefinition, String> {
    let db = state.db.lock().await;
    let repo = crate::db::repositories::rule_repo::RuleRepository::new(db.conn());
    repo.create(&request)
}

#[tauri::command]
pub async fn update_rule(
    state: State<'_, AppState>,
    id: String,
    request: RuleCreateRequest,
) -> Result<RuleDefinition, String> {
    let db = state.db.lock().await;
    let repo = crate::db::repositories::rule_repo::RuleRepository::new(db.conn());
    repo.update(&id, &request)
}

#[tauri::command]
pub async fn delete_rule(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let db = state.db.lock().await;
    let repo = crate::db::repositories::rule_repo::RuleRepository::new(db.conn());
    repo.delete(&id)
}
