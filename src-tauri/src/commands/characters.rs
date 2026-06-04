use tauri::State;

use crate::models::character::*;
use crate::services::catalog::character_service::CharacterService;
use crate::state::AppState;

#[tauri::command]
pub async fn list_world_characters(
    state: State<'_, AppState>,
    world_id: String,
) -> Result<Vec<CharacterDefinition>, String> {
    let db = state.db.lock().await;
    CharacterService::new().list_world_characters(db.conn(), &world_id)
}

#[tauri::command]
pub async fn list_all_characters(
    state: State<'_, AppState>,
) -> Result<Vec<CharacterDefinition>, String> {
    let db = state.db.lock().await;
    CharacterService::new().list_all_characters(db.conn())
}

#[tauri::command]
pub async fn get_character(
    state: State<'_, AppState>,
    id: String,
) -> Result<CharacterDefinition, String> {
    let db = state.db.lock().await;
    CharacterService::new().get_character(db.conn(), &id)
}

#[tauri::command]
pub async fn create_world_character(
    state: State<'_, AppState>,
    world_id: String,
    request: CharacterCreateRequest,
) -> Result<CharacterDefinition, String> {
    let db = state.db.lock().await;
    CharacterService::new().create_world_character(db.conn(), &world_id, request)
}

#[tauri::command]
pub async fn update_world_character(
    state: State<'_, AppState>,
    id: String,
    request: CharacterUpdateRequest,
) -> Result<CharacterDefinition, String> {
    let db = state.db.lock().await;
    CharacterService::new().update_world_character(db.conn(), &id, request)
}

#[tauri::command]
pub async fn delete_world_character(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let db = state.db.lock().await;
    CharacterService::new().delete_world_character(db.conn(), &id)
}

#[tauri::command]
pub async fn export_character_template(
    state: State<'_, AppState>,
    world_id: String,
    character_id: String,
) -> Result<CharacterTemplateExport, String> {
    let db = state.db.lock().await;
    CharacterService::new().export_character_template(db.conn(), &world_id, &character_id)
}

#[tauri::command]
pub async fn create_character_in_world(
    state: State<'_, AppState>,
    world_id: String,
    character_id: String,
    request: CharacterImportRequest,
) -> Result<CharacterDefinition, String> {
    let db = state.db.lock().await;
    CharacterService::new().create_character_in_world(db.conn(), &world_id, &character_id, request)
}

#[tauri::command]
pub async fn import_character_template(
    state: State<'_, AppState>,
    world_id: String,
    request: CharacterTemplateImportRequest,
) -> Result<CharacterDefinition, String> {
    let db = state.db.lock().await;
    CharacterService::new().import_character_template(db.conn(), &world_id, request)
}
