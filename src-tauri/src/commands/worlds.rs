use tauri::{AppHandle, State};

use crate::models::world::*;
use crate::services::catalog::world_service::WorldService;
use crate::services::world_package::WorldPackageService;
use crate::state::AppState;

#[tauri::command]
pub async fn list_worlds(state: State<'_, AppState>) -> Result<Vec<WorldDefinition>, String> {
    let db = state.db.lock().await;
    WorldService::new().list_worlds(db.conn())
}

#[tauri::command]
pub async fn get_world(state: State<'_, AppState>, id: String) -> Result<WorldDefinition, String> {
    let db = state.db.lock().await;
    WorldService::new().get_world(db.conn(), &id)
}

#[tauri::command]
pub async fn create_world(
    state: State<'_, AppState>,
    request: WorldCreateRequest,
) -> Result<WorldDefinition, String> {
    let db = state.db.lock().await;
    WorldService::new().create_world(db.conn(), request)
}

#[tauri::command]
pub async fn update_world(
    state: State<'_, AppState>,
    id: String,
    request: WorldUpdateRequest,
) -> Result<WorldDefinition, String> {
    let db = state.db.lock().await;
    WorldService::new().update_world(db.conn(), &id, request)
}

#[tauri::command]
pub async fn delete_world(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let db = state.db.lock().await;
    WorldService::new().delete_world(db.conn(), &id)
}

#[tauri::command]
pub async fn delete_all_worlds(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let db = state.db.lock().await;
    WorldService::new().delete_all_worlds(db.conn())
}

#[tauri::command]
pub async fn duplicate_world(
    state: State<'_, AppState>,
    id: String,
) -> Result<WorldDefinition, String> {
    let db = state.db.lock().await;
    WorldService::new().duplicate_world(db.conn(), &id)
}

#[tauri::command]
pub async fn preview_opening_prompt(
    state: State<'_, AppState>,
    world_id: String,
    params: Option<serde_json::Value>,
) -> Result<WorldOpeningPromptPreviewResponse, String> {
    let db = state.db.lock().await;
    WorldService::new().preview_opening_prompt(db.conn(), &world_id, params)
}

#[tauri::command]
pub async fn export_world_package(
    state: State<'_, AppState>,
    world_id: String,
) -> Result<BinaryFileResponse, String> {
    let db = state.db.lock().await;
    WorldService::new().build_world_package(db.conn(), &state.data_dir, &world_id)
}

#[tauri::command]
pub async fn export_world_package_to_downloads(
    app: AppHandle,
    state: State<'_, AppState>,
    world_id: String,
) -> Result<SavedFileResponse, String> {
    let package = {
        let db = state.db.lock().await;
        WorldService::new().build_world_package(db.conn(), &state.data_dir, &world_id)?
    };
    WorldPackageService::save_package_to_downloads(&app, &state, package).await
}

#[tauri::command]
pub async fn import_world_package(
    state: State<'_, AppState>,
    filename: String,
    data: Vec<u8>,
) -> Result<WorldDefinition, String> {
    let _ = filename;
    let db = state.db.lock().await;
    WorldService::new().import_world_package_archive(db.conn(), &state.data_dir, data)
}

#[tauri::command]
pub async fn import_world_package_from_path(
    state: State<'_, AppState>,
    path: String,
) -> Result<WorldDefinition, String> {
    let data = WorldPackageService::read_package_from_path(&path)?;
    let db = state.db.lock().await;
    WorldService::new().import_world_package_archive(db.conn(), &state.data_dir, data)
}
