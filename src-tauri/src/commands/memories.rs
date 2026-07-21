use crate::models::memory::*;
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn list_memories(
    state: State<'_, AppState>,
    world_id: Option<String>,
    session_id: Option<String>,
    character_id: Option<String>,
    layer: Option<String>,
    limit: Option<i32>,
) -> Result<Vec<MemoryEntry>, String> {
    let db = state.db.lock().await;
    let repo = crate::db::repositories::memory_repo::MemoryRepository::new(db.conn());
    repo.list(&MemoryQueryParams {
        world_id,
        session_id,
        character_id,
        layer,
        limit,
    })
}

#[tauri::command]
pub async fn list_memory_entities(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<MemoryEntity>, String> {
    let db = state.db.lock().await;
    let repo =
        crate::db::repositories::memory_entity_repo::MemoryEntityRepository::new(db.conn());
    repo.list_by_session(&session_id)
}

#[tauri::command]
pub async fn list_memory_relations(
    state: State<'_, AppState>,
    session_id: String,
    active_only: Option<bool>,
) -> Result<Vec<MemoryRelation>, String> {
    let db = state.db.lock().await;
    let repo =
        crate::db::repositories::memory_relation_repo::MemoryRelationRepository::new(db.conn());
    if active_only.unwrap_or(false) {
        repo.list_active_by_session(&session_id)
    } else {
        repo.list_by_session(&session_id, false)
    }
}
