use crate::models::world::{
    VerifyWorldPackageUiCompatibilityRequest, WorldUiBundleValidationRequest,
    WorldUiBundleValidationResult, WorldUiCompatibilityReport, WorldUiCompileRequest,
    WorldUiCompileResult, WorldUiDocumentRequest, WorldUiDocumentValidationResult,
};
use crate::services::game_ui::GameUiService;

/// Unified world event endpoint. UI actions, end-turn commands and agent
/// actions use the same serialized event contract.
#[tauri::command]
pub async fn dispatch_world_event(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::state::AppState>,
    world_id: String,
    session_id: String,
    event: crate::services::workflow::WorldEvent,
) -> Result<crate::services::workflow::WorldEventResult, String> {
    let _permit = state.session_mutations.try_acquire(&session_id)?;
    let event_result = {
        let db = state.db.lock().await;
        let world = crate::services::catalog::world_service::WorldService::new()
            .get_world(db.conn(), &world_id)?;
        crate::services::world_logic::dispatch_event(db.conn(), &world, &session_id, event)?
    };

    if event_result.transition.next_actor.as_deref() == Some("agent")
        && event_result.transition.turn_ended
        && !event_result.transition.game_over
    {
        let trigger_message = {
            let db = state.db.lock().await;
            let world = crate::services::catalog::world_service::WorldService::new()
                .get_world(db.conn(), &world_id)?;
            world.ui_theme_config["logic"]["agent_turn"]["message"]
                .as_str()
                .unwrap_or("请按当前回合规则完成你的行动。")
                .to_string()
        };
        let snapshot = crate::commands::sessions::run_workflow_agent_action(
            &app,
            &state,
            session_id,
            trigger_message,
        )
        .await?;
        let mut completed = event_result;
        completed.snapshot = Some(snapshot);
        return Ok(completed);
    }

    Ok(event_result)
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
