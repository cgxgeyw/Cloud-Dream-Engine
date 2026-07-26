use tauri::State;

use crate::models::world_kv::{KV_OWNER_CHARACTER, KV_OWNER_SESSION, KV_OWNER_WORLD, WorldKvEntry};
use crate::state::AppState;

/// 作用域参数（全部可选，缺省为世界级，保持旧调用兼容）：
/// - world：owner = world_id（跨存档共享）
/// - session：owner = session_id（世界存档，存档间互不可见）
/// - character：owner = "{session_id}:{character_id}"（存档内角色）
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct KvScope {
    pub scope: Option<String>,
    pub session_id: Option<String>,
    pub character_id: Option<String>,
}

fn resolve_owner(world_id: &str, scope: &KvScope) -> Result<(String, String), String> {
    match scope.scope.as_deref().map(str::trim) {
        None | Some("") | Some(KV_OWNER_WORLD) => {
            Ok((KV_OWNER_WORLD.to_string(), world_id.to_string()))
        }
        Some(KV_OWNER_SESSION) => {
            let session_id = scope
                .session_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "session 作用域需要 session_id".to_string())?;
            Ok((KV_OWNER_SESSION.to_string(), session_id.to_string()))
        }
        Some(KV_OWNER_CHARACTER) => {
            let session_id = scope
                .session_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "character 作用域需要 session_id".to_string())?;
            let character_id = scope
                .character_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "character 作用域需要 character_id".to_string())?;
            Ok((
                KV_OWNER_CHARACTER.to_string(),
                format!("{session_id}:{character_id}"),
            ))
        }
        Some(other) => Err(format!("未知的变量作用域: {other}")),
    }
}

#[tauri::command]
pub async fn list_world_kv(
    state: State<'_, AppState>,
    world_id: String,
    namespace: String,
    scope: Option<KvScope>,
) -> Result<Vec<WorldKvEntry>, String> {
    let db = state.db.lock().await;
    let namespace = crate::services::world_storage::require_kv_namespace(
        db.conn(), &world_id, &namespace,
    )?;
    let (owner_type, owner_id) = resolve_owner(&world_id, &scope.unwrap_or_default())?;
    crate::db::repositories::world_kv_repo::WorldKvRepository::new(db.conn())
        .list(&owner_type, &owner_id, &namespace)
}

#[tauri::command]
pub async fn get_world_kv(
    state: State<'_, AppState>,
    world_id: String,
    namespace: String,
    key: String,
    scope: Option<KvScope>,
) -> Result<Option<WorldKvEntry>, String> {
    let db = state.db.lock().await;
    let namespace = crate::services::world_storage::require_kv_namespace(
        db.conn(), &world_id, &namespace,
    )?;
    let (owner_type, owner_id) = resolve_owner(&world_id, &scope.unwrap_or_default())?;
    crate::db::repositories::world_kv_repo::WorldKvRepository::new(db.conn())
        .get(&owner_type, &owner_id, &namespace, &key)
}

#[tauri::command]
pub async fn set_world_kv(
    state: State<'_, AppState>,
    world_id: String,
    namespace: String,
    key: String,
    value: serde_json::Value,
    scope: Option<KvScope>,
) -> Result<WorldKvEntry, String> {
    let db = state.db.lock().await;
    let namespace = crate::services::world_storage::require_kv_namespace(
        db.conn(), &world_id, &namespace,
    )?;
    let (owner_type, owner_id) = resolve_owner(&world_id, &scope.unwrap_or_default())?;
    crate::db::repositories::world_kv_repo::WorldKvRepository::new(db.conn())
        .set(&owner_type, &owner_id, &namespace, &key, &value)
}

#[tauri::command]
pub async fn delete_world_kv(
    state: State<'_, AppState>,
    world_id: String,
    namespace: String,
    key: String,
    scope: Option<KvScope>,
) -> Result<(), String> {
    let db = state.db.lock().await;
    let namespace = crate::services::world_storage::require_kv_namespace(
        db.conn(), &world_id, &namespace,
    )?;
    let (owner_type, owner_id) = resolve_owner(&world_id, &scope.unwrap_or_default())?;
    crate::db::repositories::world_kv_repo::WorldKvRepository::new(db.conn())
        .delete(&owner_type, &owner_id, &namespace, &key)
}
