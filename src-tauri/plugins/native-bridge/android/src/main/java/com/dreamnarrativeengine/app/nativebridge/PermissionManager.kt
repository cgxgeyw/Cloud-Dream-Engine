package com.dreamnarrativeengine.app.nativebridge

import android.Manifest
import android.os.Build
import app.tauri.PermissionState
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.PluginManager
import org.json.JSONObject

/**
 * 运行时权限的非阻塞申请流程。取代旧 MainActivity 中"静态 currentActivity +
 * CompletableFuture 阻塞 60 秒"的实现：系统弹窗结果经 ActivityResult 回调返回，
 * Rust 侧异步等待，任何线程都不会被卡住。
 */
class PermissionManager(private val plugin: NativeBridgePlugin) {

  /** 应用启动时申请通知权限（Android 13+），结果无需消费。 */
  fun requestPostNotificationsOnLaunch() {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) {
      return
    }
    if (plugin.getPermissionState("notifications") == PermissionState.GRANTED) {
      return
    }
    PluginManager.requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS)) { }
  }

  /**
   * 世界包权限申请入口。wait = true 时挂起 invoke 直到用户在系统弹窗中做出选择，
   * 结果由 NativeBridgePlugin.worldPermissionsCallback 返回；wait = false 时仅弹窗
   * 并立即返回 granted = null（结果不消费，与旧行为一致）。
   */
  fun requestWorldPermissions(aliases: List<String>, wait: Boolean, invoke: Invoke) {
    if (aliases.isEmpty()) {
      invoke.resolve(JSObject().put("granted", JSONObject.NULL))
      return
    }
    if (wait) {
      plugin.requestPermissionForAliases(aliases.toTypedArray(), invoke, "worldPermissionsCallback")
      return
    }
    val strings = aliases.flatMap { aliasStrings(it) }.toTypedArray()
    if (strings.isNotEmpty()) {
      PluginManager.requestPermissions(strings) { }
    }
    invoke.resolve(JSObject().put("granted", JSONObject.NULL))
  }

  /** 权限弹窗回调：按最初申请的别名逐个查询当前授权状态，全部授予才算 granted。 */
  fun resolveWorldPermissions(invoke: Invoke) {
    val args = invoke.parseArgs(RequestWorldPermissionsArgs::class.java)
    val aliases = args.permissions.orEmpty()
    val granted = aliases.isNotEmpty() && aliases.all { alias ->
      plugin.getPermissionState(alias) == PermissionState.GRANTED
    }
    invoke.resolve(JSObject().put("granted", granted))
  }

  private fun aliasStrings(alias: String): List<String> = when (alias) {
    "calendar" -> listOf(
      Manifest.permission.READ_CALENDAR,
      Manifest.permission.WRITE_CALENDAR
    )
    "microphone" -> listOf(Manifest.permission.RECORD_AUDIO)
    "notifications" -> listOf(Manifest.permission.POST_NOTIFICATIONS)
    else -> emptyList()
  }
}
