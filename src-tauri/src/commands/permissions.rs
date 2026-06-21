use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldPermissionRequest {
    #[serde(default)]
    pub permissions: Vec<String>,
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

#[cfg(target_os = "android")]
async fn request_world_permissions_impl(
    app: AppHandle,
    request: WorldPermissionRequest,
) -> Result<Vec<WorldPermissionStatus>, String> {
    use std::sync::mpsc;

    let permissions = normalize_permissions(request.permissions);
    let (sender, receiver) = mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = sender.send(request_android_permissions(permissions));
    })
    .map_err(|error| error.to_string())?;
    receiver.recv().map_err(|error| error.to_string())?
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

#[cfg(target_os = "android")]
fn request_android_permissions(
    permissions: Vec<String>,
) -> Result<Vec<WorldPermissionStatus>, String> {
    use jni::objects::{JObject, JString, JValue};

    let vm = crate::workmanager_plugin::android_vm()
        .ok_or_else(|| "Android JavaVM is not initialized; cannot request world permissions".to_string())?;
    let mut env = vm.attach_current_thread().map_err(|error| error.to_string())?;
    let activity_thread = env
        .find_class("android/app/ActivityThread")
        .map_err(|error| error.to_string())?;
    let activity = env
        .call_static_method(
            activity_thread,
            "currentActivityThread",
            "()Landroid/app/ActivityThread;",
            &[],
        )
        .and_then(|value| value.l())
        .map_err(|error| error.to_string())?;
    if activity.is_null() {
        return Err("Android ActivityThread is not available".to_string());
    }
    let activities = env
        .call_method(
            activity,
            "getActivities",
            "()Ljava/util/Map;",
            &[],
        )
        .and_then(|value| value.l())
        .map_err(|error| error.to_string())?;
    let values = env
        .call_method(activities, "values", "()Ljava/util/Collection;", &[])
        .and_then(|value| value.l())
        .map_err(|error| error.to_string())?;
    let iterator = env
        .call_method(values, "iterator", "()Ljava/util/Iterator;", &[])
        .and_then(|value| value.l())
        .map_err(|error| error.to_string())?;
    let mut current_activity = JObject::null();
    while env
        .call_method(&iterator, "hasNext", "()Z", &[])
        .and_then(|value| value.z())
        .map_err(|error| error.to_string())?
    {
        let record = env
            .call_method(&iterator, "next", "()Ljava/lang/Object;", &[])
            .and_then(|value| value.l())
            .map_err(|error| error.to_string())?;
        let paused = env
            .get_field(&record, "paused", "Z")
            .and_then(|value| value.z())
            .unwrap_or(false);
        let candidate = env
            .get_field(&record, "activity", "Landroid/app/Activity;")
            .and_then(|value| value.l())
            .map_err(|error| error.to_string())?;
        if !candidate.is_null() && !paused {
            current_activity = candidate;
            break;
        }
        if current_activity.is_null() && !candidate.is_null() {
            current_activity = candidate;
        }
    }
    if current_activity.is_null() {
        return Err("Android Activity is not available; cannot request world permissions".to_string());
    }

    let permission_names = expand_android_permissions(&permissions);
    let string_class = env
        .find_class("java/lang/String")
        .map_err(|error| error.to_string())?;
    let array = env
        .new_object_array(permission_names.len() as i32, string_class, JObject::null())
        .map_err(|error| error.to_string())?;
    for (index, permission) in permission_names.iter().enumerate() {
        let value: JString = env.new_string(permission).map_err(|error| error.to_string())?;
        env.set_object_array_element(&array, index as i32, value)
            .map_err(|error| error.to_string())?;
    }
    let permission_array = JObject::from(array);
    env.call_method(
        current_activity,
        "requestPermissions",
        "([Ljava/lang/String;I)V",
        &[JValue::Object(&permission_array), JValue::Int(8721)],
    )
    .map_err(|error| error.to_string())?;

    Ok(permissions
        .into_iter()
        .map(|permission| WorldPermissionStatus {
            permission,
            requested: true,
            granted: None,
            error: None,
        })
        .collect())
}

#[cfg(target_os = "android")]
fn expand_android_permissions(permissions: &[String]) -> Vec<&'static str> {
    let mut output = Vec::new();
    for permission in permissions {
        match permission.as_str() {
            "calendar" => {
                output.push("android.permission.READ_CALENDAR");
                output.push("android.permission.WRITE_CALENDAR");
            }
            "microphone" => output.push("android.permission.RECORD_AUDIO"),
            "notifications" => output.push("android.permission.POST_NOTIFICATIONS"),
            _ => {}
        }
    }
    output
}
