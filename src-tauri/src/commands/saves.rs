use crate::models::save::*;
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn list_saves(state: State<'_, AppState>) -> Result<Vec<SaveSummary>, String> {
    let db = state.db.lock().await;
    let repo = crate::db::repositories::save_repo::SaveRepository::new(db.conn());
    repo.list()
}

#[tauri::command]
pub async fn branch_save(state: State<'_, AppState>, id: String) -> Result<SaveSummary, String> {
    let db = state.db.lock().await;
    let save_repo = crate::db::repositories::save_repo::SaveRepository::new(db.conn());
    let session_repo = crate::db::repositories::session_repo::SessionRepository::new(db.conn());
    let save = save_repo
        .get(&id)?
        .ok_or_else(|| "Save not found".to_string())?;
    let session = session_repo
        .get(&save.session_id)?
        .ok_or_else(|| "Session not found".to_string())?;
    save_repo.branch_save(&id, &session)
}

#[tauri::command]
pub async fn delete_save(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let db = state.db.lock().await;
    let repo = crate::db::repositories::save_repo::SaveRepository::new(db.conn());
    repo.delete(&id)
}

#[tauri::command]
pub async fn delete_all_saves(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let db = state.db.lock().await;
    let repo = crate::db::repositories::save_repo::SaveRepository::new(db.conn());
    let count = repo.delete_all()?;
    Ok(serde_json::json!({ "ok": true, "deleted_count": count }))
}
