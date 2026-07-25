package com.dreamnarrativeengine.app

import android.os.Bundle
import android.view.MotionEvent
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
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
}
