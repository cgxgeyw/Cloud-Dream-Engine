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

  override fun load(webView: WebView) {
    notificationManager = NotificationManager(activity.applicationContext)
    permissionManager = PermissionManager(this)
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

  @PermissionCallback
  fun worldPermissionsCallback(invoke: Invoke) {
    permissionManager.resolveWorldPermissions(invoke)
  }
}
