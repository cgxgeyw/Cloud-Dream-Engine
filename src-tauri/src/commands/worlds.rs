use tauri::{AppHandle, Emitter, State};

use crate::models::world::*;
use crate::services::catalog::world_service::WorldService;
use crate::services::world_builder::AiWorldBuilderService;
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
pub async fn create_world_with_ai(
    app: AppHandle,
    state: State<'_, AppState>,
    request: AiWorldCreateRequest,
) -> Result<AiWorldCreateResponse, String> {
    let model = {
        let db = state.db.lock().await;
        let repo = crate::db::repositories::model_repo::ModelRepository::new(db.conn());
        let models = repo.list(Some("text"))?;
        models
            .iter()
            .find(|model| model.is_default)
            .cloned()
            .or_else(|| models.first().cloned())
            .ok_or_else(|| {
                "No text model configured. Please add a text model in Settings first.".to_string()
            })?
    };

    // Emit live progress (accumulated character count) so the UI can show the
    // model is actively generating.
    let mut emit_progress = |received: usize| {
        let _ = app.emit(
            "ai_world_create:progress",
            serde_json::json!({ "received_chars": received }),
        );
    };

    let draft = AiWorldBuilderService::generate_draft(
        &state.services.llm_client,
        &model,
        request.clone(),
        Some(&mut emit_progress),
    )
    .await?;

    let db = state.db.lock().await;
    // M12: 取模型→释放锁→长流式 LLM 调用→重新取锁,期间 model_configs 可能被改/删。
    // 持久化前用最新值校验该模型仍存在,避免写出指向已删除模型的悬空关联。
    let model = {
        let repo = crate::db::repositories::model_repo::ModelRepository::new(db.conn());
        repo.list(Some("text"))?
            .into_iter()
            .find(|item| item.id == model.id)
            .ok_or_else(|| {
                "The selected text model was changed or removed during generation. Please retry."
                    .to_string()
            })?
    };
    AiWorldBuilderService::persist_world(db.conn(), &model, &request, draft)
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
    // L1: 记录来源文件名以便排查导入问题(此前被直接丢弃)。
    #[cfg(debug_assertions)]
    eprintln!("[worlds] importing world package from upload: {filename}");
    #[cfg(not(debug_assertions))]
    let _ = &filename;
    let service = WorldService::new();
    // H8: 先在锁外解压并把资产写盘(可能是数 MB 的磁盘 IO),再仅为 DB 持久化短暂持锁,
    // 避免持全局 DB 锁期间阻塞 tokio 线程做解压/磁盘 IO 而让整个 UI 停顿数秒。
    let imported = service.unpack_world_package_archive(&state.data_dir, data)?;
    let db = state.db.lock().await;
    service.persist_world_package(db.conn(), imported)
}

#[tauri::command]
pub async fn import_world_package_from_path(
    state: State<'_, AppState>,
    path: String,
) -> Result<WorldDefinition, String> {
    let data = WorldPackageService::read_package_from_path(&path)?;
    let service = WorldService::new();
    // H8: 同上,解压/落盘在锁外完成。
    let imported = service.unpack_world_package_archive(&state.data_dir, data)?;
    let db = state.db.lock().await;
    service.persist_world_package(db.conn(), imported)
}
