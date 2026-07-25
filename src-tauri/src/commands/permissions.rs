use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldPermissionRequest {
    #[serde(default)]
    pub permissions: Vec<String>,
    /// 为 true 时异步等待用户在系统弹窗中做出选择，granted 返回真实结果；
    /// 为 false（默认）时仅触发弹窗，立即返回 granted = null。
    #[serde(default)]
    pub wait: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldPermissionStatus {
    pub permission: String,
    pub requested: bool,
    pub granted: Option<bool>,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn request_world_permissions(
    _state: State<'_, AppState>,
    app: AppHandle,
    request: WorldPermissionRequest,
) -> Result<Vec<WorldPermissionStatus>, String> {
    request_world_permissions_impl(app, request).await
}

/// 安卓上权限申请只经 Kotlin NativeBridge 中间件：系统弹窗结果经回调异步返回，
/// 不再阻塞线程等待（旧实现为 JNI + CompletableFuture 60 秒超时）。
#[cfg(target_os = "android")]
async fn request_world_permissions_impl(
    app: AppHandle,
    request: WorldPermissionRequest,
) -> Result<Vec<WorldPermissionStatus>, String> {
    let permissions = normalize_permissions(request.permissions);
    if permissions.is_empty() {
        return Ok(Vec::new());
    }
    let wait = request.wait;
    use tauri::Manager;
    let bridge = app.state::<tauri_plugin_native_bridge::NativeBridge<tauri::Wry>>();
    let granted = bridge.request_permissions(permissions.clone(), wait).await?;
    Ok(permissions
        .into_iter()
        .map(|permission| WorldPermissionStatus {
            permission,
            requested: true,
            granted: if wait { granted } else { None },
            error: None,
        })
        .collect())
}

#[cfg(not(target_os = "android"))]
async fn request_world_permissions_impl(
    _app: AppHandle,
    request: WorldPermissionRequest,
) -> Result<Vec<WorldPermissionStatus>, String> {
    Ok(normalize_permissions(request.permissions)
        .into_iter()
        .map(|permission| WorldPermissionStatus {
            permission,
            requested: false,
            granted: None,
            error: None,
        })
        .collect())
}

fn normalize_permissions(values: Vec<String>) -> Vec<String> {
    let mut output = Vec::new();
    for value in values {
        let normalized = match value.trim() {
            "calendar" | "android.permission.READ_CALENDAR" | "android.permission.WRITE_CALENDAR" => {
                "calendar"
            }
            "microphone" | "mic" | "android.permission.RECORD_AUDIO" => "microphone",
            "notifications" | "notification" | "android.permission.POST_NOTIFICATIONS" => {
                "notifications"
            }
            _ => continue,
        };
        if !output.iter().any(|item| item == normalized) {
            output.push(normalized.to_string());
        }
    }
    output
}
