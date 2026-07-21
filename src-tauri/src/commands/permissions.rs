use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldPermissionRequest {
    #[serde(default)]
    pub permissions: Vec<String>,
    /// 为 true 时阻塞等待用户在系统弹窗中做出选择，granted 返回真实结果；
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

#[cfg(target_os = "android")]
async fn request_world_permissions_impl(
    _app: AppHandle,
    request: WorldPermissionRequest,
) -> Result<Vec<WorldPermissionStatus>, String> {
    let permissions = normalize_permissions(request.permissions);
    if permissions.is_empty() {
        return Ok(Vec::new());
    }
    let wait = request.wait;
    tauri::async_runtime::spawn_blocking(move || request_android_permissions(permissions, wait))
        .await
        .map_err(|error| error.to_string())?
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

fn normalize_permissions(values: Vec<String>) -> Vec<String> {    let mut output = Vec::new();
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
    wait: bool,
) -> Result<Vec<WorldPermissionStatus>, String> {
    // 串行化权限请求：系统授权弹窗同一时间只能处理一个，并发请求会互相覆盖结果。
    static PERMISSION_REQUEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = PERMISSION_REQUEST_LOCK
        .lock()
        .map_err(|error| error.to_string())?;

    let permission_names = expand_android_permissions(&permissions);
    if permission_names.is_empty() {
        return Ok(permissions
            .into_iter()
            .map(|permission| WorldPermissionStatus {
                permission,
                requested: false,
                granted: None,
                error: None,
            })
            .collect());
    }

    let vm = crate::workmanager_plugin::android_vm()
        .ok_or_else(|| "Android JavaVM is not initialized; cannot request world permissions".to_string())?;
    // 当前是 spawn_blocking 的工作线程（非主线程），attach/detach 安全。
    let mut env = vm.attach_current_thread().map_err(|error| error.to_string())?;

    // 原生线程上 FindClass 只能看到框架类；应用类要用 JNI_OnLoad 缓存的全局引用。
    let main_activity_class = crate::workmanager_plugin::main_activity_class()?;

    let granted = match request_permissions_via_main_activity(&mut env, main_activity_class, &permission_names, wait) {
        Ok(granted) => granted,
        Err(jni::errors::Error::JavaException) => {
            let detail = describe_pending_java_exception(&mut env);
            return Err(format!("Java exception during world permission request: {detail}"));
        }
        Err(error) => return Err(error.to_string()),
    };

    Ok(permissions
        .into_iter()
        .map(|permission| WorldPermissionStatus {
            permission,
            requested: true,
            granted: if wait { Some(granted) } else { None },
            error: None,
        })
        .collect())
}

/// 通过 MainActivity 的静态方法走正规 ActivityCompat.requestPermissions 流程，
/// 替代原先对 ActivityThread 隐藏 API 的反射（在 Android 14+ 上会被拦截并引发崩溃）。
#[cfg(target_os = "android")]
fn request_permissions_via_main_activity(
    env: &mut jni::JNIEnv,
    main_activity_class: &jni::objects::GlobalRef,
    permission_names: &[&str],
    wait: bool,
) -> Result<bool, jni::errors::Error> {
    use jni::objects::{JObject, JString, JValue};

    let string_class = env.find_class("java/lang/String")?;
    let array = env.new_object_array(permission_names.len() as i32, string_class, JObject::null())?;
    for (index, permission) in permission_names.iter().enumerate() {
        let value: JString = env.new_string(permission)?;
        env.set_object_array_element(&array, index as i32, value)?;
    }
    let permission_array = JObject::from(array);
    Ok(env
        .call_static_method(
            main_activity_class,
            "requestWorldPermissions",
            "([Ljava/lang/String;Z)Z",
            &[JValue::Object(&permission_array), JValue::Bool(wait as u8)],
        )?
        .z()?)
}

/// 取出并清理当前挂起的 Java 异常，返回其 toString 文本，
/// 避免带着未处理异常返回 JVM 导致进程直接崩溃。
#[cfg(target_os = "android")]
fn describe_pending_java_exception(env: &mut jni::JNIEnv) -> String {
    let throwable = match env.exception_occurred() {
        Ok(throwable) => throwable,
        Err(_) => return "unknown Java exception".to_string(),
    };
    let _ = env.exception_clear();
    env.call_method(&throwable, "toString", "()Ljava/lang/String;", &[])
        .and_then(|value| value.l())
        .map_err(|error| error.to_string())
        .and_then(|object| {
            let jstring = jni::objects::JString::from(object);
            env.get_string(&jstring)
                .map(|value| value.to_string_lossy().into_owned())
                .map_err(|error| error.to_string())
        })
        .unwrap_or_else(|_| "unknown Java exception".to_string())
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
