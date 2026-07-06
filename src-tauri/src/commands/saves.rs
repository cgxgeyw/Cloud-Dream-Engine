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
    let conn = db.conn();
    let repo = crate::db::repositories::save_repo::SaveRepository::new(conn);
    // H1: 存档是指向会话的书签;删除存档应连带清理其底层会话的衍生数据,
    // 否则 sessions/memories/attribute_values 等永久残留。
    let session_id = repo.get(&id)?.map(|save| save.session_id);
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    repo.delete(&id)?;
    if let Some(session_id) = session_id {
        crate::db::cleanup::purge_session_data(conn, &session_id)?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn delete_all_saves(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let db = state.db.lock().await;
    let conn = db.conn();
    let repo = crate::db::repositories::save_repo::SaveRepository::new(conn);
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    // H1: 收集所有存档指向的会话,逐个清理其衍生数据,再删除存档本身。
    let session_ids = repo.list_session_ids()?;
    let count = repo.delete_all()?;
    for session_id in session_ids {
        crate::db::cleanup::purge_session_data(conn, &session_id)?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "ok": true, "deleted_count": count }))
}
