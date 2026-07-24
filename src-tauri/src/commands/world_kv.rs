use tauri::State;

use crate::models::world_kv::WorldKvEntry;
use crate::state::AppState;

#[tauri::command]
pub async fn list_world_kv(
    state: State<'_, AppState>,
    world_id: String,
    namespace: String,
) -> Result<Vec<WorldKvEntry>, String> {
    let db = state.db.lock().await;
    let namespace = crate::services::world_storage::require_kv_namespace(
        db.conn(), &world_id, &namespace,
    )?;
    crate::db::repositories::world_kv_repo::WorldKvRepository::new(db.conn())
        .list(&world_id, &namespace)
}

#[tauri::command]
pub async fn get_world_kv(
    state: State<'_, AppState>,
    world_id: String,
    namespace: String,
    key: String,
) -> Result<Option<WorldKvEntry>, String> {
    let db = state.db.lock().await;
    let namespace = crate::services::world_storage::require_kv_namespace(
        db.conn(), &world_id, &namespace,
    )?;
    crate::db::repositories::world_kv_repo::WorldKvRepository::new(db.conn())
        .get(&world_id, &namespace, &key)
}

#[tauri::command]
pub async fn set_world_kv(
    state: State<'_, AppState>,
    world_id: String,
    namespace: String,
    key: String,
    value: serde_json::Value,
) -> Result<WorldKvEntry, String> {
    let db = state.db.lock().await;
    let namespace = crate::services::world_storage::require_kv_namespace(
        db.conn(), &world_id, &namespace,
    )?;
    crate::db::repositories::world_kv_repo::WorldKvRepository::new(db.conn())
        .set(&world_id, &namespace, &key, &value)
}

#[tauri::command]
pub async fn delete_world_kv(
    state: State<'_, AppState>,
    world_id: String,
    namespace: String,
    key: String,
) -> Result<(), String> {
    let db = state.db.lock().await;
    let namespace = crate::services::world_storage::require_kv_namespace(
        db.conn(), &world_id, &namespace,
    )?;
    crate::db::repositories::world_kv_repo::WorldKvRepository::new(db.conn())
        .delete(&world_id, &namespace, &key)
}
