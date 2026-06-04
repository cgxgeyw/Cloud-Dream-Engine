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
