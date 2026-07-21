package com.dreamnarrativeengine.app

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.view.MotionEvent
import android.webkit.WebView
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.enableEdgeToEdge
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import java.util.concurrent.CompletableFuture
import java.util.concurrent.TimeUnit

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

  override fun onWebViewCreate(webView: WebView) {
    super.onWebViewCreate(webView)
    // realme/ColorOS 的系统长按拖拽助手会在长按时认领手势，导致 WebView 的 JS 收不到抬起事件。
    // 但系统 View 层一定能收到事件：按下时禁止父级拦截，并把 DOWN/MOVE/UP/CANCEL 经
    // evaluateJavascript 转发给 JS 侧的 __nativeTouchEnd/__nativeTouchMove，支撑按住说话与上滑取消。
    var downY = 0f
    var lastReportedDy = 0f
    webView.setOnTouchListener { v, event ->
      when (event.action) {
        MotionEvent.ACTION_DOWN -> {
          downY = event.y
          lastReportedDy = 0f
          v.parent?.requestDisallowInterceptTouchEvent(true)
        }
        MotionEvent.ACTION_MOVE -> {
          val dy = event.y - downY
          if (Math.abs(dy - lastReportedDy) >= 24f) {
            lastReportedDy = dy
            webView.evaluateJavascript("window.__nativeTouchMove && window.__nativeTouchMove($dy)", null)
          }
        }
        MotionEvent.ACTION_UP -> webView.evaluateJavascript("window.__nativeTouchEnd && window.__nativeTouchEnd(true)", null)
        MotionEvent.ACTION_CANCEL -> webView.evaluateJavascript("window.__nativeTouchEnd && window.__nativeTouchEnd(false)", null)
      }
      false
    }
  }

  override fun onResume() {
    super.onResume()
    currentActivity = this
  }

  override fun onPause() {
    if (currentActivity === this) {
      currentActivity = null
    }
    super.onPause()
  }

  override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<String>, grantResults: IntArray) {
    super.onRequestPermissionsResult(requestCode, permissions, grantResults)
    if (requestCode == WORLD_PERMISSION_REQUEST_CODE) {
      val granted = grantResults.isNotEmpty() && grantResults.all { it == PackageManager.PERMISSION_GRANTED }
      pendingWorldPermissionResult?.complete(granted)
    }
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

  companion object {
    private const val WORLD_PERMISSION_REQUEST_CODE = 8721
    private const val WORLD_PERMISSION_TIMEOUT_SECONDS = 60L

    @Volatile
    private var currentActivity: MainActivity? = null

    @Volatile
    private var pendingWorldPermissionResult: CompletableFuture<Boolean>? = null

    /**
     * 供 Rust 经 JNI 调用的安卓运行时权限申请入口。
     * wait = true 时阻塞调用线程直至用户做出选择（最长 60 秒），返回是否全部授权；
     * wait = false 时仅弹出系统授权框并立即返回 true，授权结果无人消费、直接丢弃。
     */
    @JvmStatic
    fun requestWorldPermissions(permissions: Array<String>, wait: Boolean): Boolean {
      val activity = currentActivity ?: return false
      if (wait) {
        val result = CompletableFuture<Boolean>()
        pendingWorldPermissionResult = result
        activity.runOnUiThread {
          ActivityCompat.requestPermissions(activity, permissions, WORLD_PERMISSION_REQUEST_CODE)
        }
        return try {
          result.get(WORLD_PERMISSION_TIMEOUT_SECONDS, TimeUnit.SECONDS)
        } catch (error: Exception) {
          false
        } finally {
          pendingWorldPermissionResult = null
        }
      }
      activity.runOnUiThread {
        ActivityCompat.requestPermissions(activity, permissions, WORLD_PERMISSION_REQUEST_CODE)
      }
      return true
    }
  }
}
