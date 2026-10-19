use crate::models::world::{
    VerifyWorldPackageUiCompatibilityRequest, WorldUiBundleValidationRequest,
    WorldUiBundleValidationResult, WorldUiCompileRequest, WorldUiCompileResult,
    WorldUiCompatibilityReport, WorldUiDocumentRequest, WorldUiDocumentValidationResult,
};
use crate::services::game_ui::GameUiService;

/// Unified world event endpoint. UI actions, end-turn commands and agent
/// actions use the same serialized event contract.
#[tauri::command]
pub async fn dispatch_world_event(
    state: tauri::State<'_, crate::state::AppState>,
    world_id: String,
    session_id: String,
    event: crate::services::workflow::WorldEvent,
) -> Result<crate::services::workflow::WorldEventResult, String> {
    let _permit = state.session_mutations.try_acquire(&session_id)?;
    let db = state.db.lock().await;
    let world = crate::services::catalog::world_service::WorldService::new().get_world(db.conn(), &world_id)?;
    crate::services::world_logic::dispatch_event(
        db.conn(),
        &world,
        &session_id,
        event,
    )
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
