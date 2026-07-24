use tauri::State;

use crate::models::world_record::{WorldRecord, WorldRecordWriteRequest};
use crate::state::AppState;

#[tauri::command]
pub async fn list_world_records(
    state: State<'_, AppState>,
    world_id: String,
    collection: String,
) -> Result<Vec<WorldRecord>, String> {
    let db = state.db.lock().await;
    crate::services::world_storage::require_record_collection(
        db.conn(), &world_id, &collection,
    )?;
    let repository =
        crate::db::repositories::world_record_repo::WorldRecordRepository::new(db.conn());
    repository.list(&world_id, &collection)
}

#[tauri::command]
pub async fn create_world_record(
    state: State<'_, AppState>,
    world_id: String,
    request: WorldRecordWriteRequest,
) -> Result<WorldRecord, String> {
    let db = state.db.lock().await;
    let schema = crate::services::world_storage::require_record_collection(
        db.conn(), &world_id, &request.collection,
    )?;
    crate::services::world_storage::validate_record_against_schema(
        &request.data, schema.as_ref(),
    )?;
    let repository =
        crate::db::repositories::world_record_repo::WorldRecordRepository::new(db.conn());
    repository.create(&world_id, &request)
}

#[tauri::command]
pub async fn update_world_record(
    state: State<'_, AppState>,
    world_id: String,
    id: String,
    request: WorldRecordWriteRequest,
) -> Result<WorldRecord, String> {
    let db = state.db.lock().await;
    let schema = crate::services::world_storage::require_record_collection(
        db.conn(), &world_id, &request.collection,
    )?;
    crate::services::world_storage::validate_record_against_schema(
        &request.data, schema.as_ref(),
    )?;
    let repository =
        crate::db::repositories::world_record_repo::WorldRecordRepository::new(db.conn());
    repository.update(&world_id, &id, &request)
}

#[tauri::command]
pub async fn delete_world_record(
    state: State<'_, AppState>,
    world_id: String,
    collection: String,
    id: String,
) -> Result<(), String> {
    let db = state.db.lock().await;
    crate::services::world_storage::require_record_collection(
        db.conn(), &world_id, &collection,
    )?;
    let repository =
        crate::db::repositories::world_record_repo::WorldRecordRepository::new(db.conn());
    repository.delete(&world_id, &collection, &id)
}
