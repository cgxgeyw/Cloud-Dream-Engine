use crate::models::world::{
    VerifyWorldPackageUiCompatibilityRequest, WorldUiBundleValidationRequest,
    WorldUiBundleValidationResult, WorldUiCompileRequest, WorldUiCompileResult,
    WorldUiCompatibilityReport, WorldUiDocumentRequest, WorldUiDocumentValidationResult,
};
use crate::services::game_ui::GameUiService;

#[tauri::command]
pub async fn invoke_world_logic(
    state: tauri::State<'_, crate::state::AppState>, world_id: String,
    session_id: String, handler: String, input: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let _permit = state.session_mutations.try_acquire(&session_id)?;
    let db = state.db.lock().await;
    let world = crate::services::catalog::world_service::WorldService::new().get_world(db.conn(), &world_id)?;
    crate::services::world_logic::invoke(db.conn(), &world, &session_id, &handler, input, "player")
}

#[tauri::command]
pub async fn validate_world_ui_document(
    request: WorldUiDocumentRequest,
) -> Result<WorldUiDocumentValidationResult, String> {
    Ok(GameUiService::new().validate_world_ui_document(request))
}

#[tauri::command]
pub async fn validate_world_ui_bundle(
    request: WorldUiBundleValidationRequest,
) -> Result<WorldUiBundleValidationResult, String> {
    Ok(GameUiService::new().validate_world_ui_bundle(request))
}

#[tauri::command]
pub async fn compile_world_ui_document(
    request: WorldUiCompileRequest,
) -> Result<WorldUiCompileResult, String> {
    Ok(GameUiService::new().compile_world_ui_document(request))
}

#[tauri::command]
pub async fn verify_world_package_ui_compatibility(
    request: VerifyWorldPackageUiCompatibilityRequest,
) -> Result<WorldUiCompatibilityReport, String> {
    Ok(GameUiService::new().verify_world_package_ui_compatibility(request))
}
