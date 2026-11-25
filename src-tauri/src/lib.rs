use std::path::PathBuf;
use tauri::Manager;

mod commands;
mod db;
mod events;
mod models;
mod services;
mod state;

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
    if crate::services::notifications::run_scheduled_notification_cli() {
        return;
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_native_bridge::init())
        .setup(|app| {
            let data_dir = get_data_dir(app)?;
            // 随包分发的内置向量模型先就位，记忆召回与知识库检索才能用上本地推理。
            // 首次启动需要把 ~92MB 权重从资源目录搬到数据目录，放到后台线程避免卡住界面；
            // 模型本身是懒加载的，即使首回合抢在拷贝完成前也只会退回关键词检索。
            {
                let handle = app.handle().clone();
                let install_dir = data_dir.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    crate::services::embedding_install::ensure_bundled_embedding_model(
                        &handle,
                        &install_dir,
                    );
                });
            }
            let db = Database::new(&data_dir)?;
            let services = BackendServices::new(data_dir.clone());
            let notification_app = app.handle().clone();
            let notification_data_dir = data_dir.clone();
            app.manage(AppState {
                db: tokio::sync::Mutex::new(db),
                services,
                data_dir,
                session_mutations: SessionMutationCoordinator::default(),
                knowledge_rebuild: crate::state::KnowledgeRebuildTracker::default(),
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
            commands::worlds::get_default_director_response_contract,
            commands::worlds::get_default_memory_summary_prompts,
            commands::worlds::export_world_package,
            commands::worlds::export_world_package_to_downloads,
            commands::worlds::import_world_package,
            commands::worlds::inspect_world_package,
            commands::worlds::import_world_package_from_path,
            commands::world_records::list_world_records,
            commands::world_records::create_world_record,
            commands::world_records::update_world_record,
            commands::world_records::delete_world_record,
            commands::world_kv::list_world_kv,
            commands::world_kv::get_world_kv,
            commands::world_kv::set_world_kv,
            commands::world_kv::delete_world_kv,
            commands::platform_features::invoke_world_platform_feature,
            commands::platform_features::list_world_feature_grants,
            commands::platform_features::set_world_feature_grant,
            commands::game_ui::validate_world_ui_document,
            commands::game_ui::dispatch_world_event,
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
            commands::sessions::edit_session_message,
            commands::sessions::answer_interaction,
            commands::sessions::get_session_generation_params,
            commands::sessions::update_session_generation_params,
            commands::sessions::regenerate_last_turn,
            // 存档
            commands::saves::list_saves,
            commands::saves::branch_save,
            commands::saves::delete_save,
            commands::saves::delete_all_saves,
            // 记忆
            commands::memories::list_memories,
            commands::memories::list_memory_entities,
            commands::memories::list_memory_relations,
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
            commands::mcp_servers::list_mcp_servers,
            commands::mcp_servers::create_mcp_server,
            commands::mcp_servers::update_mcp_server,
            commands::mcp_servers::delete_mcp_server,
            commands::mcp_servers::probe_mcp_server,
            commands::mcp_tools::list_mcp_tools,
            commands::mcp_tools::create_mcp_tool,
            commands::mcp_tools::update_mcp_tool,
            commands::mcp_tools::delete_mcp_tool,
            commands::mcp_tools::export_mcp_tools,
            commands::mcp_tools::import_mcp_tools,
            // 上传
            commands::uploads::upload_file,
            commands::uploads::get_asset_base_dir,
            commands::knowledge::list_knowledge_bases,
            commands::knowledge::create_knowledge_base,
            commands::knowledge::delete_knowledge_base,
            commands::knowledge::list_knowledge_documents,
            commands::knowledge::add_knowledge_document,
            commands::knowledge::delete_knowledge_document,
            commands::knowledge::update_knowledge_chunking,
            commands::knowledge::update_knowledge_document,
            commands::knowledge::rebuild_knowledge_embeddings,
            commands::knowledge::get_knowledge_rebuild_status,
            commands::knowledge::get_knowledge_rebuild_for_base,
            commands::knowledge::list_knowledge_rebuild_jobs,
            commands::knowledge::list_world_knowledge_candidates,
            commands::knowledge::list_world_knowledge_scope,
            commands::stickers::list_sticker_packs,
            commands::stickers::create_sticker_pack,
            commands::stickers::update_sticker_pack,
            commands::stickers::delete_sticker_pack,
            commands::stickers::list_stickers,
            commands::stickers::create_sticker,
            commands::stickers::update_sticker,
            commands::stickers::delete_sticker,
            commands::stickers::reorder_stickers,
            commands::stickers::get_sticker_world_bindings,
            commands::stickers::set_sticker_world_binding,
            commands::uploads::delete_asset,
            commands::permissions::request_world_permissions,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Dream Narrative Engine");
}
