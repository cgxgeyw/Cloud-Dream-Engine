use crate::models::world::{
    VerifyWorldPackageUiCompatibilityRequest, WorldUiBundleValidationRequest,
    WorldUiBundleValidationResult, WorldUiCompileRequest, WorldUiCompileResult,
    WorldUiCompatibilityReport, WorldUiDocumentRequest, WorldUiDocumentValidationResult,
};
use crate::services::game_ui::GameUiService;

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
