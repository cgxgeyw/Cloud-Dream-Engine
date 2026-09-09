//! Android 平台能力统一中间件（Kotlin NativeBridge）的 Rust 侧。
//!
//! 所有安卓系统交互（通知调度、运行时权限等）只能经此插件进出：
//! Rust 业务代码不直接写 JNI，前端/世界包拿不到安卓原生对象。
//! 桌面平台只提供返回 `unsupported` 的同名接口，保证跨平台代码可编译、行为可预期。

use serde::{Deserialize, Serialize};
use tauri::{
    plugin::{Builder, TauriPlugin},
    Manager, Runtime,
};

/// 调度一条行程提醒（当前实现为写入系统日历事件，语义与旧 JNI 链路一致）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleReminderArgs {
    pub notification_id: String,
    pub title: String,
    pub body: String,
    pub channel_id: String,
    pub trigger_at_ms: i64,
}

/// Kotlin 侧返回的结构化结果，字段名保持 snake_case。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AndroidScheduleResult {
    pub ok: bool,
    pub notification_id: Option<String>,
    pub package_name: Option<String>,
    pub trigger_at_ms: Option<i64>,
    pub request_code: Option<i32>,
    pub channel_id: Option<String>,
    pub calendar_event_created: Option<bool>,
    pub calendar_event_id: Option<i64>,
    pub calendar_id: Option<i64>,
    pub calendar_reminder_minutes: Option<i32>,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub sdk_int: Option<i32>,
    pub error: Option<String>,
}

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CancelReminderArgs {
    notification_id: String,
}

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RequestWorldPermissionsArgs {
    permissions: Vec<String>,
    wait: bool,
}

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RequestWorldPermissionsResult {
    granted: Option<bool>,
}

// ---- 第 12 项：世界包文件能力 ----

/// Kotlin 文件 action 的统一返回形态（ok=false 时 error 带 `code: 说明`）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AndroidFileResult {
    pub ok: bool,
    pub size: Option<usize>,
    pub data_base64: Option<String>,
    pub error: Option<String>,
}

/// file.read 的结果（已解出 size 与 base64 内容）。
pub struct FileReadOutput {
    pub size: usize,
    pub data_base64: String,
}

/// file.write 的结果。
pub struct FileWriteOutput {
    pub size: usize,
}

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct FilePathArgs {
    path: String,
}

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WriteFileArgs {
    path: String,
    data_base64: String,
}

#[cfg(not(target_os = "android"))]
const UNSUPPORTED: &str = "该平台能力仅在安卓可用（unsupported）";

/// 托管在 AppState 中的中间件句柄。安卓上持有 Kotlin 插件引用；桌面为空壳。
pub struct NativeBridge<R: Runtime> {
    #[cfg(target_os = "android")]
    handle: tauri::plugin::PluginHandle<R>,
    #[cfg(not(target_os = "android"))]
    _marker: std::marker::PhantomData<fn() -> R>,
}

impl<R: Runtime> NativeBridge<R> {
    /// 调度行程提醒。同步阻塞等待 Kotlin 返回，与旧 JNI 调用语义一致，
    /// 调用方（通知服务）无需改为异步。
    pub fn schedule_reminder(&self, args: ScheduleReminderArgs) -> Result<AndroidScheduleResult, String> {
        #[cfg(target_os = "android")]
        {
            let result: AndroidScheduleResult = self
                .handle
                .run_mobile_plugin("scheduleNotification", args)
                .map_err(|error| error.to_string())?;
            if result.ok {
                Ok(result)
            } else {
                Err(result
                    .error
                    .unwrap_or_else(|| "Android 行程提醒创建失败".to_string()))
            }
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = args;
            Err(UNSUPPORTED.to_string())
        }
    }

    /// 取消行程提醒。幂等：不存在的提醒视为成功（与旧行为一致，由 Kotlin 保证）。
    pub fn cancel_reminder(&self, notification_id: &str) -> Result<(), String> {
        #[cfg(target_os = "android")]
        {
            let result: AndroidScheduleResult = self
                .handle
                .run_mobile_plugin(
                    "cancelNotification",
                    CancelReminderArgs {
                        notification_id: notification_id.to_string(),
                    },
                )
                .map_err(|error| error.to_string())?;
            if result.ok {
                Ok(())
            } else {
                Err(result
                    .error
                    .unwrap_or_else(|| "Android 行程提醒取消失败".to_string()))
            }
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = notification_id;
            Err(UNSUPPORTED.to_string())
        }
    }

    /// 申请世界包运行时权限（calendar / microphone / notifications 别名）。
    /// 异步等待用户在系统弹窗中的选择，不再阻塞线程或设置 60 秒超时。
    /// wait = false 时仅弹窗并立即返回 None（结果不消费，与旧行为一致）。
    pub async fn request_permissions(
        &self,
        permissions: Vec<String>,
        wait: bool,
    ) -> Result<Option<bool>, String> {
        #[cfg(target_os = "android")]
        {
            let result: RequestWorldPermissionsResult = self
                .handle
                .run_mobile_plugin_async(
                    "requestWorldPermissions",
                    RequestWorldPermissionsArgs { permissions, wait },
                )
                .await
                .map_err(|error| error.to_string())?;
            Ok(result.granted)
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = (permissions, wait);
            Err(UNSUPPORTED.to_string())
        }
    }

    /// file.read：读取世界目录内文件的绝对路径（Rust 侧已做穿越防护）。
    pub fn read_file(&self, path: &str) -> Result<FileReadOutput, String> {
        #[cfg(target_os = "android")]
        {
            let result = self.run_file_command(
                "readFile",
                FilePathArgs {
                    path: path.to_string(),
                },
            )?;
            Ok(FileReadOutput {
                size: result.size.unwrap_or(0),
                data_base64: result.data_base64.unwrap_or_default(),
            })
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = path;
            Err(UNSUPPORTED.to_string())
        }
    }

    /// file.write：把 base64 内容写入世界目录内的绝对路径（自动建父目录）。
    pub fn write_file(&self, path: &str, data_base64: &str) -> Result<FileWriteOutput, String> {
        #[cfg(target_os = "android")]
        {
            let result = self.run_file_command(
                "writeFile",
                WriteFileArgs {
                    path: path.to_string(),
                    data_base64: data_base64.to_string(),
                },
            )?;
            Ok(FileWriteOutput {
                size: result.size.unwrap_or(0),
            })
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = (path, data_base64);
            Err(UNSUPPORTED.to_string())
        }
    }

    /// file.share：经系统分享面板发送世界目录内的文件。
    pub fn share_file(&self, path: &str) -> Result<(), String> {
        #[cfg(target_os = "android")]
        {
            self.run_file_command(
                "shareFile",
                FilePathArgs {
                    path: path.to_string(),
                },
            )?;
            Ok(())
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = path;
            Err(UNSUPPORTED.to_string())
        }
    }

    #[cfg(target_os = "android")]
    fn run_file_command(
        &self,
        command: &str,
        args: impl serde::Serialize,
    ) -> Result<AndroidFileResult, String> {
        let result: AndroidFileResult = self
            .handle
            .run_mobile_plugin(command, args)
            .map_err(|error| error.to_string())?;
        if result.ok {
            Ok(result)
        } else {
            Err(result
                .error
                .unwrap_or_else(|| "io: 安卓文件操作失败".to_string()))
        }
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("native-bridge")
        .setup(|app, api| {
            #[cfg(not(target_os = "android"))]
            let _ = api;
            #[cfg(target_os = "android")]
            {
                let handle = api.register_android_plugin(
                    "com.dreamnarrativeengine.app.nativebridge",
                    "NativeBridgePlugin",
                )?;
                app.manage(NativeBridge { handle });
            }
            #[cfg(not(target_os = "android"))]
            {
                app.manage(NativeBridge::<R> {
                    _marker: std::marker::PhantomData,
                });
            }
            Ok(())
        })
        .build()
}
