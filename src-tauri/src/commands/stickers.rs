use tauri::State;

use crate::models::sticker::{
    StickerItem, StickerPack, StickerPackCreateRequest, StickerPackUpdateRequest,
};
use crate::services::sticker_library;
use crate::state::AppState;

#[tauri::command]
pub async fn list_sticker_packs(state: State<'_, AppState>) -> Result<Vec<StickerPack>, String> {
    let db = state.db.lock().await;
    sticker_library::migrate_all_legacy_stickers(db.conn())?;
    sticker_library::list_packs(db.conn())
}

#[tauri::command]
pub async fn create_sticker_pack(
    state: State<'_, AppState>,
    request: StickerPackCreateRequest,
) -> Result<StickerPack, String> {
    let db = state.db.lock().await;
    sticker_library::create_pack(
        db.conn(),
        &request.name,
        &request.description,
        request.world_id.as_deref(),
    )
}

#[tauri::command]
pub async fn update_sticker_pack(
    state: State<'_, AppState>,
    id: String,
    request: StickerPackUpdateRequest,
) -> Result<StickerPack, String> {
    let db = state.db.lock().await;
    sticker_library::update_pack(
        db.conn(),
        &id,
        &request.name,
        &request.description,
        request.enabled,
    )
}

#[tauri::command]
pub async fn delete_sticker_pack(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let db = state.db.lock().await;
    sticker_library::delete_pack(db.conn(), &id)
}

#[tauri::command]
pub async fn list_stickers(
    state: State<'_, AppState>,
    pack_id: String,
) -> Result<Vec<StickerItem>, String> {
    let db = state.db.lock().await;
    sticker_library::list_stickers(db.conn(), &pack_id)
}

#[tauri::command]
pub async fn create_sticker(
    state: State<'_, AppState>,
    pack_id: String,
    key: String,
    label: String,
    description: String,
    asset_path: String,
) -> Result<StickerItem, String> {
    let db = state.db.lock().await;
    sticker_library::create_sticker(
        db.conn(),
        &state.data_dir,
        &pack_id,
        &key,
        &label,
        &description,
        &asset_path,
    )
}

#[tauri::command]
pub async fn update_sticker(
    state: State<'_, AppState>,
    id: String,
    key: String,
    label: String,
    description: String,
    asset_path: String,
    enabled: bool,
) -> Result<StickerItem, String> {
    let db = state.db.lock().await;
    sticker_library::update_sticker(
        db.conn(),
        &state.data_dir,
        &id,
        &key,
        &label,
        &description,
        &asset_path,
        enabled,
    )
}

#[tauri::command]
pub async fn delete_sticker(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let db = state.db.lock().await;
    sticker_library::delete_sticker(db.conn(), &id)
}

#[tauri::command]
pub async fn reorder_stickers(
    state: State<'_, AppState>,
    pack_id: String,
    sticker_ids: Vec<String>,
) -> Result<(), String> {
    let db = state.db.lock().await;
    sticker_library::reorder_stickers(db.conn(), &pack_id, &sticker_ids)
}

#[tauri::command]
pub async fn get_sticker_world_bindings(
    state: State<'_, AppState>,
    pack_id: String,
) -> Result<Vec<String>, String> {
    let db = state.db.lock().await;
    sticker_library::get_world_bindings(db.conn(), &pack_id)
}

#[tauri::command]
pub async fn set_sticker_world_binding(
    state: State<'_, AppState>,
    pack_id: String,
    world_id: String,
    enabled: bool,
) -> Result<(), String> {
    let db = state.db.lock().await;
    sticker_library::set_world_binding(db.conn(), &pack_id, &world_id, enabled)
}
