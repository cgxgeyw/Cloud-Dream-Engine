package com.dreamnarrativeengine.app

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.enableEdgeToEdge
import androidx.core.content.ContextCompat

class MainActivity : TauriActivity() {
  private val requestPostNotifications =
    registerForActivityResult(ActivityResultContracts.RequestPermission()) { }

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    // 行程提醒写入系统日历需要 appContext,否则 Rust 经 JNI 调用时会因 context 未就绪而失败。
    ScheduledNotificationReceiver.initialize(applicationContext)
    requestNotificationPermissionIfNeeded()
  }

  private fun requestNotificationPermissionIfNeeded() {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) {
      return
    }

    val granted = ContextCompat.checkSelfPermission(
      this,
      Manifest.permission.POST_NOTIFICATIONS
    ) == PackageManager.PERMISSION_GRANTED
    if (!granted) {
      requestPostNotifications.launch(Manifest.permission.POST_NOTIFICATIONS)
    }
  }
}
