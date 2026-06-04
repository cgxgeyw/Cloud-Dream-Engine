use crate::models::model_config::*;
use crate::services::assets::image_gen::{ImageGenerator, ImageRequest};
use crate::services::llm::client::{ChatMessage, ChatRequest};
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn list_models(
    state: State<'_, AppState>,
    model_type: Option<String>,
) -> Result<Vec<ModelConfig>, String> {
    let db = state.db.lock().await;
    let repo = crate::db::repositories::model_repo::ModelRepository::new(db.conn());
    repo.list(model_type.as_deref())
}

#[tauri::command]
pub async fn get_model(state: State<'_, AppState>, id: String) -> Result<ModelConfig, String> {
    let db = state.db.lock().await;
    let repo = crate::db::repositories::model_repo::ModelRepository::new(db.conn());
    repo.get(&id)?.ok_or_else(|| "Model not found".to_string())
}

#[tauri::command]
pub async fn create_model(
    state: State<'_, AppState>,
    request: ModelConfigCreateRequest,
) -> Result<ModelConfig, String> {
    let db = state.db.lock().await;
    let repo = crate::db::repositories::model_repo::ModelRepository::new(db.conn());
    repo.create(&request)
}

#[tauri::command]
pub async fn update_model(
    state: State<'_, AppState>,
    id: String,
    request: ModelConfigUpdateRequest,
) -> Result<ModelConfig, String> {
    let db = state.db.lock().await;
    let repo = crate::db::repositories::model_repo::ModelRepository::new(db.conn());
    repo.update(&id, &request)
}

#[tauri::command]
pub async fn delete_model(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let db = state.db.lock().await;
    let repo = crate::db::repositories::model_repo::ModelRepository::new(db.conn());
    repo.delete(&id)
}

#[tauri::command]
pub async fn set_default_model(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let db = state.db.lock().await;
    let repo = crate::db::repositories::model_repo::ModelRepository::new(db.conn());
    let model = repo
        .get(&id)?
        .ok_or_else(|| "Model not found".to_string())?;
    repo.set_default(&id)?;
    if model.model_type == "text" || model.model_type == "image" {
        let settings = {
            let mut stmt = db
                .conn()
                .prepare(
                    "SELECT text_model_provider, default_text_model, image_model_provider, default_image_workflow, embedding_enabled, default_embedding_model, home_background_strategy, export_directory FROM settings WHERE id = 1",
                )
                .map_err(|e| e.to_string())?;
            stmt.query_row([], |row| {
                Ok(crate::models::settings::AppSettings {
                    text_model_provider: row.get(0)?,
                    default_text_model: row.get(1)?,
                    image_model_provider: row.get(2)?,
                    default_image_workflow: row.get(3)?,
                    embedding_enabled: row.get::<_, i64>(4)? != 0,
                    default_embedding_model: row.get(5)?,
                    home_background_strategy: row.get(6)?,
                    export_directory: row.get(7)?,
                })
            })
            .map_err(|e| e.to_string())?
        };
        if model.model_type == "text" {
            db.conn().execute(
                "UPDATE settings SET text_model_provider = ?1, default_text_model = ?2 WHERE id = 1",
                rusqlite::params![
                    if model.provider.trim().is_empty() {
                        settings.text_model_provider
                    } else {
                        model.provider.trim().to_string()
                    },
                    model.model_id.trim(),
                ],
            ).map_err(|e| e.to_string())?;
        } else {
            db.conn().execute(
                "UPDATE settings SET image_model_provider = ?1, default_image_workflow = ?2 WHERE id = 1",
                rusqlite::params![
                    if model.provider.trim().is_empty() {
                        settings.image_model_provider
                    } else {
                        model.provider.trim().to_string()
                    },
                    model.model_id.trim(),
                ],
            ).map_err(|e| e.to_string())?;
        }
    } else if model.model_type == "embedding" {
        db.conn()
            .execute(
                "UPDATE settings SET default_embedding_model = ?1 WHERE id = 1",
                rusqlite::params![model.model_id.trim()],
            )
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn test_model(
    state: State<'_, AppState>,
    id: String,
) -> Result<ModelTestResponse, String> {
    let model = {
        let db = state.db.lock().await;
        let repo = crate::db::repositories::model_repo::ModelRepository::new(db.conn());
        repo.get(&id)?
            .ok_or_else(|| "Model not found".to_string())?
    };

    let test_request = ChatRequest {
        model: model.model_id.clone(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: serde_json::Value::String("Say 'hello' in one word.".to_string()),
            reasoning_content: None,
            speaker: None,
            tool_call_id: None,
            tool_calls: None,
            metadata: None,
        }],
        temperature: Some(0.7),
        max_tokens: Some(10),
        stream: Some(false),
        json_mode: None,
        response_schema: None,
        tools: None,
        tool_choice: None,
        native_tool_calling: None,
    };

    let result = state
        .services
        .llm_client
        .chat_completion(
            &model.provider,
            &model.base_url,
            &model.api_key,
            &test_request,
        )
        .await;

    match result {
        Ok(response) => Ok(ModelTestResponse {
            ok: true,
            detail: format!("Model responded: {}", response.content),
            debug_lines: vec![
                format!("Provider: {}", model.provider),
                format!("Model: {}", model.model_id),
                format!("Response: {}", response.content),
            ],
        }),
        Err(e) => Ok(ModelTestResponse {
            ok: false,
            detail: e.clone(),
            debug_lines: vec![
                format!("Provider: {}", model.provider),
                format!("Model: {}", model.model_id),
                format!("Error: {}", e),
            ],
        }),
    }
}

#[tauri::command]
pub async fn test_image_model(
    state: State<'_, AppState>,
    id: String,
    request: ImageModelTestRequest,
) -> Result<ImageModelTestResponse, String> {
    let model = {
        let db = state.db.lock().await;
        let repo = crate::db::repositories::model_repo::ModelRepository::new(db.conn());
        repo.get(&id)?
            .ok_or_else(|| "Model not found".to_string())?
    };

    if model.model_type != "image" {
        return Ok(ImageModelTestResponse {
            ok: false,
            detail: "Only image models support image test generation.".to_string(),
            debug_lines: vec![format!(
                "ModelImageTest unsupported_model_type type={}",
                model.model_type
            )],
            asset_path: None,
            image_url: None,
            seed: None,
        });
    }

    let prompt = request.prompt.trim();
    if prompt.is_empty() {
        return Ok(ImageModelTestResponse {
            ok: false,
            detail: "Prompt is required.".to_string(),
            debug_lines: vec!["ModelImageTest empty_prompt".to_string()],
            asset_path: None,
            image_url: None,
            seed: None,
        });
    }

    let (width, height) = if model
        .provider
        .trim()
        .to_ascii_lowercase()
        .contains("automatic1111")
    {
        (960, 540)
    } else {
        (1536, 1024)
    };

    let generator = ImageGenerator::new();
    let result = generator
        .generate(
            &model.provider,
            &model.base_url,
            &model.api_key,
            Some(&model.model_id),
            &ImageRequest {
                prompt: prompt.to_string(),
                negative_prompt: None,
                width: Some(width),
                height: Some(height),
                steps: None,
                cfg_scale: None,
                seed: None,
            },
        )
        .await;

    match result {
        Ok(image) => {
            let filename = format!(
                "image-test-{}.{}",
                uuid::Uuid::new_v4().simple(),
                image.format.trim().trim_start_matches('.')
            );
            let saved = crate::commands::uploads::save_asset_file(
                &state.data_dir,
                &filename,
                &image.image_data,
            )?;
            let asset_path = saved
                .get("asset_path")
                .and_then(|value| value.as_str())
                .map(str::to_string);
            let image_url = saved
                .get("url")
                .and_then(|value| value.as_str())
                .map(str::to_string);

            Ok(ImageModelTestResponse {
                ok: true,
                detail: format!("Image generated by {}.", model.model_id),
                debug_lines: vec![
                    format!("Provider: {}", model.provider),
                    format!("Model: {}", model.model_id),
                    format!("Prompt: {}", prompt),
                ],
                asset_path,
                image_url,
                seed: image.seed,
            })
        }
        Err(e) => Ok(ImageModelTestResponse {
            ok: false,
            detail: e.clone(),
            debug_lines: vec![
                format!("Provider: {}", model.provider),
                format!("Model: {}", model.model_id),
                format!("Prompt: {}", prompt),
                format!("Error: {}", e),
            ],
            asset_path: None,
            image_url: None,
            seed: None,
        }),
    }
}

#[tauri::command]
pub async fn discover_models(
    state: State<'_, AppState>,
    provider: String,
    base_url: String,
    api_key: String,
) -> Result<ModelDiscoverResponse, String> {
    let request = ModelDiscoverRequest {
        provider,
        base_url,
        api_key,
    };
    let mut debug_lines = Vec::new();
    debug_lines.push(format!("Discovering models at {}", request.base_url));

    match state
        .services
        .llm_client
        .discover_models(&request.provider, &request.base_url, &request.api_key)
        .await
    {
        Ok(models) => {
            let model_ids: Vec<String> = models.iter().map(|m| m.id.clone()).collect();
            debug_lines.push(format!("Found {} models", models.len()));
            Ok(ModelDiscoverResponse {
                ok: true,
                detail: format!("Found {} models", models.len()),
                model_ids,
                debug_lines,
            })
        }
        Err(e) => {
            debug_lines.push(format!("Error: {}", e));
            Ok(ModelDiscoverResponse {
                ok: false,
                detail: e,
                model_ids: vec![],
                debug_lines,
            })
        }
    }
}

#[tauri::command]
pub async fn get_builtin_embedding_model_status(
    state: State<'_, AppState>,
    model_id: Option<String>,
) -> Result<EmbeddingModelStatus, String> {
    state
        .services
        .runtime
        .memory
        .get_builtin_model_status(model_id.as_deref())
}

#[tauri::command]
pub async fn download_builtin_embedding_model(
    state: State<'_, AppState>,
    model_id: Option<String>,
) -> Result<EmbeddingModelStatus, String> {
    state
        .services
        .runtime
        .memory
        .download_builtin_model(model_id.as_deref())
}
