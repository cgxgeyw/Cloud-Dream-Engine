use std::path::PathBuf;
use tauri::Manager;

mod commands;
mod db;
mod events;
mod models;
mod services;
mod state;
mod workmanager_plugin;

use db::Database;
use services::backend::BackendServices;
use state::{AppState, SessionMutationCoordinator};

fn get_data_dir(app: &tauri::App) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let data_dir = get_data_dir(app)?;
            let db = Database::new(&data_dir)?;
            let services = BackendServices::new(data_dir.clone());
            let notification_app = app.handle().clone();
            let notification_data_dir = data_dir.clone();
            app.manage(AppState {
                db: tokio::sync::Mutex::new(db),
                services,
                data_dir,
                session_mutations: SessionMutationCoordinator::default(),
            });
            tauri::async_runtime::spawn(async move {
                let _ = crate::services::notifications::NotificationScheduler::restore_pending(
                    &notification_app,
                    &notification_data_dir,
                )
                .await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // 世界
            commands::worlds::list_worlds,
            commands::worlds::get_world,
            commands::worlds::create_world,
            commands::worlds::create_world_with_ai,
            commands::worlds::update_world,
            commands::worlds::delete_world,
            commands::worlds::delete_all_worlds,
            commands::worlds::duplicate_world,
            commands::worlds::preview_opening_prompt,
            commands::worlds::export_world_package,
            commands::worlds::export_world_package_to_downloads,
            commands::worlds::import_world_package,
            commands::worlds::import_world_package_from_path,
            commands::game_ui::validate_world_ui_document,
            commands::game_ui::validate_world_ui_bundle,
            commands::game_ui::compile_world_ui_document,
            commands::game_ui::verify_world_package_ui_compatibility,
            // 角色
            commands::characters::list_world_characters,
            commands::characters::list_all_characters,
            commands::characters::get_character,
            commands::characters::create_world_character,
            commands::characters::update_world_character,
            commands::characters::delete_world_character,
            commands::characters::export_character_template,
            commands::characters::create_character_in_world,
            commands::characters::import_character_template,
            // 会话
            commands::sessions::get_session,
            commands::sessions::create_session,
            commands::sessions::submit_player_action,
            commands::sessions::retry_failed_llm_step,
            commands::sessions::switch_player_character,
            commands::sessions::resume_last_incomplete_turn,
            commands::sessions::get_session_runtime_attributes,
            // 存档
            commands::saves::list_saves,
            commands::saves::branch_save,
            commands::saves::delete_save,
            commands::saves::delete_all_saves,
            // 记忆
            commands::memories::list_memories,
            // 属性
            commands::attributes::list_attribute_schemas,
            commands::attributes::create_attribute_schema,
            commands::attributes::update_attribute_schema,
            commands::attributes::delete_attribute_schema,
            commands::attributes::list_attribute_values,
            commands::attributes::upsert_attribute_value,
            // 模型
            commands::models::list_models,
            commands::models::get_model,
            commands::models::create_model,
            commands::models::update_model,
            commands::models::delete_model,
            commands::models::set_default_model,
            commands::models::test_model,
            commands::models::test_image_model,
            commands::models::discover_models,
            commands::models::get_builtin_embedding_model_status,
            commands::models::download_builtin_embedding_model,
            // 设置
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::settings::get_export_directory_suggestion,
            // 调试
            commands::debug::get_session_debug,
            commands::debug::get_debug_timeline,
            commands::debug::get_debug_prompts,
            commands::debug::get_debug_memories,
            commands::debug::get_debug_errors,
            // 插件
            commands::plugins::list_plugins,
            // MCP 工具
            commands::mcp_tools::list_mcp_tools,
            commands::mcp_tools::create_mcp_tool,
            commands::mcp_tools::update_mcp_tool,
            commands::mcp_tools::delete_mcp_tool,
            commands::rules::list_rules,
            commands::rules::get_rule,
            commands::rules::create_rule,
            commands::rules::update_rule,
            commands::rules::delete_rule,
            // 上传
            commands::uploads::upload_file,
            commands::uploads::get_asset_base_dir,
            commands::uploads::delete_asset,
            commands::permissions::request_world_permissions,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Dream Narrative Engine");
}
