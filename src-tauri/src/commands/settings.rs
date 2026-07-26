use crate::models::settings::*;
use crate::state::AppState;
use std::fs;
use tauri::State;

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    let db = state.db.lock().await;
    load_app_settings(db.conn())
}

/// 读应用设置（含第 8 项的应用级生成参数）。命令与引擎侧共用同一实现，避免列清单漂移。
pub fn load_app_settings(conn: &rusqlite::Connection) -> Result<AppSettings, String> {
    let mut stmt = conn
        .prepare(
            "SELECT text_model_provider, default_text_model, image_model_provider, default_image_workflow, embedding_enabled, default_embedding_model, home_background_strategy, export_directory, generation_params_json FROM settings WHERE id = 1",
        )
        .map_err(|e| e.to_string())?;
    let settings = stmt
        .query_row([], |row| {
            Ok(AppSettings {
                text_model_provider: row.get(0)?,
                default_text_model: row.get(1)?,
                image_model_provider: row.get(2)?,
                default_image_workflow: row.get(3)?,
                embedding_enabled: row.get::<_, i64>(4)? != 0,
                default_embedding_model: row.get(5)?,
                home_background_strategy: row.get(6)?,
                export_directory: row.get(7)?,
                generation_params: crate::models::generation_params::GenerationParams::from_json_text(
                    &row.get::<_, String>(8)?,
                )
                .unwrap_or_default(),
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(settings)
}

#[tauri::command]
pub async fn update_settings(
    state: State<'_, AppState>,
    request: AppSettings,
) -> Result<AppSettings, String> {
    // 采样参数入库前夹到合法区间：用户填 5.0 的意思是「更随机」，夹到上限比静默丢弃好。
    let (generation_params, _notes) = request.generation_params.sanitized();
    let request = AppSettings {
        text_model_provider: request.text_model_provider.trim().to_string(),
        default_text_model: request.default_text_model.trim().to_string(),
        image_model_provider: request.image_model_provider.trim().to_string(),
        default_image_workflow: request.default_image_workflow.trim().to_string(),
        embedding_enabled: request.embedding_enabled,
        default_embedding_model: request.default_embedding_model.trim().to_string(),
        home_background_strategy: request.home_background_strategy.trim().to_string(),
        export_directory: request.export_directory.trim().to_string(),
        generation_params,
    };
    let db = state.db.lock().await;
    db.conn().execute(
        "UPDATE settings SET text_model_provider = ?1, default_text_model = ?2, image_model_provider = ?3, default_image_workflow = ?4, embedding_enabled = ?5, default_embedding_model = ?6, home_background_strategy = ?7, export_directory = ?8, generation_params_json = ?9 WHERE id = 1",
        rusqlite::params![
            request.text_model_provider,
            request.default_text_model,
            request.image_model_provider,
            request.default_image_workflow,
            if request.embedding_enabled { 1 } else { 0 },
            request.default_embedding_model,
            request.home_background_strategy,
            request.export_directory,
            serde_json::to_string(&request.generation_params).unwrap_or_else(|_| "{}".to_string()),
        ],
    ).map_err(|e| e.to_string())?;
    Ok(request)
}

#[tauri::command]
pub async fn get_export_directory_suggestion(state: State<'_, AppState>) -> Result<String, String> {
    let directory = state.data_dir.join("exports");
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    Ok(directory.to_string_lossy().to_string())
}
