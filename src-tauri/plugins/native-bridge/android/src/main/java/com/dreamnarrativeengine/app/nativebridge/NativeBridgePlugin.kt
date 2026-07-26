package com.dreamnarrativeengine.app.nativebridge

import android.Manifest
import android.app.Activity
import android.webkit.WebView
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.Permission
import app.tauri.annotation.PermissionCallback
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

@InvokeArg
class ScheduleNotificationArgs {
  lateinit var notificationId: String
  lateinit var title: String
  lateinit var body: String
  lateinit var channelId: String
  var triggerAtMs: Long = 0
}

@InvokeArg
class CancelNotificationArgs {
  lateinit var notificationId: String
}

@InvokeArg
class RequestWorldPermissionsArgs {
  var permissions: List<String>? = null
  var wait: Boolean = false
}

@InvokeArg
class ReadFileArgs {
  lateinit var path: String
}

@InvokeArg
class WriteFileArgs {
  lateinit var path: String
  lateinit var dataBase64: String
}

@InvokeArg
class ShareFileArgs {
  lateinit var path: String
}

/**
 * 安卓平台能力唯一入口。Rust 业务代码经 tauri 移动插件通道调用这里的 @Command，
 * 不再直接写 JNI；世界包/前端拿不到 Activity、Context 等原生对象。
 */
@TauriPlugin(
  permissions = [
    Permission(
      alias = "calendar",
      strings = [Manifest.permission.READ_CALENDAR, Manifest.permission.WRITE_CALENDAR]
    ),
    Permission(
      alias = "microphone",
      strings = [Manifest.permission.RECORD_AUDIO]
    ),
    Permission(
      alias = "notifications",
      strings = [Manifest.permission.POST_NOTIFICATIONS]
    )
  ]
)
class NativeBridgePlugin(private val activity: Activity) : Plugin(activity) {
  private lateinit var notificationManager: NotificationManager
  private lateinit var permissionManager: PermissionManager
  private lateinit var fileManager: FileManager

  override fun load(webView: WebView) {
    notificationManager = NotificationManager(activity.applicationContext)
    permissionManager = PermissionManager(this, activity)
    fileManager = FileManager(activity.applicationContext)
    permissionManager.requestPostNotificationsOnLaunch()
  }

  @Command
  fun scheduleNotification(invoke: Invoke) {
    val args = invoke.parseArgs(ScheduleNotificationArgs::class.java)
    invoke.resolve(
      notificationManager.scheduleReminder(
        args.notificationId,
        args.title,
        args.body,
        args.channelId,
        args.triggerAtMs
      )
    )
  }

  @Command
  fun cancelNotification(invoke: Invoke) {
    val args = invoke.parseArgs(CancelNotificationArgs::class.java)
    invoke.resolve(notificationManager.cancelReminder(args.notificationId))
  }

  @Command
  fun requestWorldPermissions(invoke: Invoke) {
    val args = invoke.parseArgs(RequestWorldPermissionsArgs::class.java)
    permissionManager.requestWorldPermissions(args.permissions.orEmpty(), args.wait, invoke)
  }

  // ---- 第 12 项：世界包文件能力（路径由 Rust 拼好并做穿越防护） ----

  @Command
  fun readFile(invoke: Invoke) {
    val args = invoke.parseArgs(ReadFileArgs::class.java)
    invoke.resolve(fileManager.readFile(args.path))
  }

  @Command
  fun writeFile(invoke: Invoke) {
    val args = invoke.parseArgs(WriteFileArgs::class.java)
    invoke.resolve(fileManager.writeFile(args.path, args.dataBase64))
  }

  @Command
  fun shareFile(invoke: Invoke) {
    val args = invoke.parseArgs(ShareFileArgs::class.java)
    invoke.resolve(fileManager.shareFile(args.path))
  }

  @PermissionCallback
  fun worldPermissionsCallback(invoke: Invoke) {
    permissionManager.resolveWorldPermissions(invoke)
  }
}
