//! 第 12 项：世界包平台能力命令。
//! 世界包（logic.js，经世界框架 iframe → 宿主 → 本命令）只能按
//! 「manifest 声明 + 用户在设置里允许」调用目录内的平台能力；
//! world_id 由宿主注入，世界包无法伪造其它世界的身份。

use serde::Deserialize;
use serde_json::Value;
use tauri::{AppHandle, State};

use crate::db::repositories::world_repo::WorldRepository;
use crate::services::platform_features::{self, WorldFeatureGrantStatus};
use crate::state::AppState;

#[derive(Debug, Clone, Deserialize)]
pub struct InvokeWorldPlatformFeatureRequest {
    pub world_id: String,
    pub feature: String,
    #[serde(default)]
    pub params: Value,
}

#[tauri::command]
pub async fn invoke_world_platform_feature(
    app: AppHandle,
    state: State<'_, AppState>,
    request: InvokeWorldPlatformFeatureRequest,
) -> Result<Value, String> {
    let data_dir = state.data_dir.clone();
    let db = state.db.lock().await;
    let world = WorldRepository::new(db.conn())
        .get(&request.world_id)?
        .ok_or_else(|| "not_declared: 世界不存在或已删除。".to_string())?;
    platform_features::invoke(
        db.conn(),
        &data_dir,
        &app,
        &world,
        &request.feature,
        &request.params,
    )
}

#[tauri::command]
pub async fn list_world_feature_grants(
    state: State<'_, AppState>,
    world_id: String,
) -> Result<Vec<WorldFeatureGrantStatus>, String> {
    let db = state.db.lock().await;
    let world = WorldRepository::new(db.conn())
        .get(&world_id)?
        .ok_or_else(|| "世界不存在或已删除。".to_string())?;
    platform_features::list_grant_status(db.conn(), &world)
}

#[tauri::command]
pub async fn set_world_feature_grant(
    state: State<'_, AppState>,
    world_id: String,
    feature: String,
    granted: bool,
) -> Result<(), String> {
    let db = state.db.lock().await;
    platform_features::set_grant(db.conn(), &world_id, &feature, granted)
}
