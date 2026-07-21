#[cfg(target_os = "android")]
use std::sync::OnceLock;

#[cfg(target_os = "android")]
use jni::{
    objects::{GlobalRef, JObject, JString, JValue},
    sys::{jint, JNI_VERSION_1_6},
    JavaVM,
};

#[cfg(target_os = "android")]
fn android_log(message: &str) {
    let sanitized = message.replace('\0', " ");
    let tag = std::ffi::CString::new("CloudDreamNotifyRust");
    let text = std::ffi::CString::new(sanitized);
    if let (Ok(tag), Ok(text)) = (tag, text) {
        unsafe {
            __android_log_write(4, tag.as_ptr(), text.as_ptr());
        }
    }
}

#[cfg(target_os = "android")]
extern "C" {
    fn __android_log_write(
        priority: std::ffi::c_int,
        tag: *const std::ffi::c_char,
        text: *const std::ffi::c_char,
    ) -> std::ffi::c_int;
}

#[cfg(target_os = "android")]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AndroidScheduleResult {
    pub ok: bool,
    #[serde(default)]
    pub notification_id: Option<String>,
    #[serde(default)]
    pub package_name: Option<String>,
    #[serde(default)]
    pub trigger_at_ms: Option<i64>,
    #[serde(default)]
    pub request_code: Option<i32>,
    #[serde(default)]
    pub channel_id: Option<String>,
    #[serde(default)]
    pub calendar_event_created: Option<bool>,
    #[serde(default)]
    pub calendar_event_id: Option<i64>,
    #[serde(default)]
    pub calendar_id: Option<i64>,
    #[serde(default)]
    pub calendar_reminder_minutes: Option<i32>,
    #[serde(default)]
    pub manufacturer: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub sdk_int: Option<i32>,
    #[serde(default)]
    pub battery_optimization_settings_action: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[cfg(target_os = "android")]
static ANDROID_JVM: OnceLock<JavaVM> = OnceLock::new();
#[cfg(target_os = "android")]
static SCHEDULED_NOTIFICATION_CLASS: OnceLock<GlobalRef> = OnceLock::new();
#[cfg(target_os = "android")]
static MAIN_ACTIVITY_CLASS: OnceLock<GlobalRef> = OnceLock::new();

#[cfg(target_os = "android")]
pub fn android_vm() -> Option<&'static JavaVM> {
    ANDROID_JVM.get()
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn JNI_OnLoad(vm: JavaVM, _reserved: *mut std::ffi::c_void) -> jint {
    if let Ok(mut env) = vm.get_env() {
        if let Ok(class) = env.find_class("com/dreamnarrativeengine/app/ScheduledNotificationReceiver") {
            if let Ok(global_class) = env.new_global_ref(class) {
                let _ = SCHEDULED_NOTIFICATION_CLASS.set(global_class);
                android_log("JNI_OnLoad initialized ScheduledNotificationReceiver class");
            }
        } else {
            let _ = env.exception_clear();
            android_log("JNI_OnLoad could not find ScheduledNotificationReceiver class");
        }
        if let Ok(class) = env.find_class("com/dreamnarrativeengine/app/MainActivity") {
            if let Ok(global_class) = env.new_global_ref(class) {
                let _ = MAIN_ACTIVITY_CLASS.set(global_class);
                android_log("JNI_OnLoad initialized MainActivity class");
            }
        } else {
            let _ = env.exception_clear();
            android_log("JNI_OnLoad could not find MainActivity class");
        }
    }
    let _ = ANDROID_JVM.set(vm);
    android_log("JNI_OnLoad initialized Android JavaVM");
    JNI_VERSION_1_6
}

#[cfg(target_os = "android")]
pub fn schedule_notification_with_result(
    _app: &tauri::AppHandle,
    notification_id: &str,
    title: &str,
    body: &str,
    channel_id: &str,
    delay_ms: i64,
) -> Result<AndroidScheduleResult, String> {
    let notification_id_for_log = notification_id.to_string();
    android_log(&format!(
        "schedule_notification_with_result enter id={} delay_ms={} channel={}",
        notification_id_for_log, delay_ms, channel_id
    ));
    let vm = ANDROID_JVM
        .get()
        .ok_or_else(|| "Android JavaVM is not initialized; scheduled notifications are unavailable".to_string())?;
    let mut env = vm.attach_current_thread().map_err(|error| error.to_string())?;
    let class = scheduler_class()?;
    let notification_id = JObject::from(
        env.new_string(notification_id)
            .map_err(|error| error.to_string())?,
    );
    let title = JObject::from(env.new_string(title).map_err(|error| error.to_string())?);
    let body = JObject::from(env.new_string(body).map_err(|error| error.to_string())?);
    let channel_id = JObject::from(
        env.new_string(channel_id)
            .map_err(|error| error.to_string())?,
    );
    let result = call_static_json(
        &mut env,
        class,
        "scheduleNotificationFromRust",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;J)Ljava/lang/String;",
        &[
            JValue::Object(&notification_id),
            JValue::Object(&title),
            JValue::Object(&body),
            JValue::Object(&channel_id),
            JValue::Long(delay_ms),
        ],
    )?;
    android_log(&format!(
        "schedule_notification_with_result result id={} ok={} trigger_at_ms={:?} calendar_event_created={:?} calendar_event_id={:?} error={:?}",
        notification_id_for_log, result.ok, result.trigger_at_ms, result.calendar_event_created, result.calendar_event_id, result.error
    ));
    if result.ok {
        Ok(result)
    } else {
        Err(result
            .error
            .unwrap_or_else(|| "Android calendar reminder was not created".to_string()))
    }
}

#[cfg(target_os = "android")]
pub fn cancel_notification(
    _app: &tauri::AppHandle,
    notification_id: &str,
) -> Result<(), String> {
    let vm = ANDROID_JVM
        .get()
        .ok_or_else(|| "Android JavaVM is not initialized; calendar reminders are unavailable".to_string())?;
    let mut env = vm.attach_current_thread().map_err(|error| error.to_string())?;
    let class = scheduler_class()?;
    let notification_id = JObject::from(
        env.new_string(notification_id)
            .map_err(|error| error.to_string())?,
    );
    let result = call_static_json(
        &mut env,
        class,
        "cancelNotificationFromRust",
        "(Ljava/lang/String;)Ljava/lang/String;",
        &[JValue::Object(&notification_id)],
    )?;
    if result.ok {
        Ok(())
    } else {
        Err(result
            .error
            .unwrap_or_else(|| "Android calendar reminder was not canceled".to_string()))
    }
}

#[cfg(target_os = "android")]
fn scheduler_class() -> Result<&'static GlobalRef, String> {
    SCHEDULED_NOTIFICATION_CLASS.get().ok_or_else(|| {
        "Android calendar reminder class was not initialized".to_string()
    })
}

#[cfg(target_os = "android")]
pub fn main_activity_class() -> Result<&'static GlobalRef, String> {
    MAIN_ACTIVITY_CLASS
        .get()
        .ok_or_else(|| "Android MainActivity class was not initialized".to_string())
}

#[cfg(target_os = "android")]
fn call_static_json(
    env: &mut jni::JNIEnv<'_>,
    class: &GlobalRef,
    name: &str,
    signature: &str,
    args: &[JValue<'_, '_>],
) -> Result<AndroidScheduleResult, String> {
    match env.call_static_method(class, name, signature, args) {
        Ok(value) => {
            let object = value.l().map_err(|error| error.to_string())?;
            let string = JString::from(object);
            let raw: String = env
                .get_string(&string)
                .map_err(|error| error.to_string())?
                .into();
            serde_json::from_str(&raw)
                .map_err(|error| format!("Android calendar reminder returned invalid JSON: {error}; payload={raw}"))
        }
        Err(error) => {
            let _ = env.exception_clear();
            Err(format!("Android calendar reminder JNI call failed: {error}"))
        }
    }
}

#[cfg(not(target_os = "android"))]
#[allow(dead_code)]
pub fn schedule_notification_with_result(
    _app: &tauri::AppHandle,
    _notification_id: &str,
    _title: &str,
    _body: &str,
    _channel_id: &str,
    _delay_ms: i64,
) -> Result<serde_json::Value, String> {
    Err("WorkManager scheduling is only available on Android".to_string())
}

#[cfg(not(target_os = "android"))]
#[allow(dead_code)]
pub fn cancel_notification(
    _app: &tauri::AppHandle,
    _notification_id: &str,
) -> Result<(), String> {
    Err("WorkManager scheduling is only available on Android".to_string())
}
