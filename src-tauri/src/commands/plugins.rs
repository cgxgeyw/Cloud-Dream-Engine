use crate::models::plugin::*;
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn list_plugins(state: State<'_, AppState>) -> Result<Vec<PluginDefinition>, String> {
    let db = state.db.lock().await;
    let repo = crate::db::repositories::plugin_repo::PluginRepository::new(db.conn());
    repo.list()
}
