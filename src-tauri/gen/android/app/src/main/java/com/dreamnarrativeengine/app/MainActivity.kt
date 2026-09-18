package com.dreamnarrativeengine.app

import android.os.Bundle
import android.view.MotionEvent
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

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

    // 安卓 WebView 里 env(safe-area-inset-top) 恒为 0，JS 只能保守猜测 88px，全面屏上留白过高。
    // 这里把真实的 statusBars/displayCutout inset 换算成 CSS px 注入 __nativeSafeAreaInsets，
    // 并派发 native-safe-area 事件让前端重新测量；返回 insets 原样，不改变 edge-to-edge 行为。
    // 首次 inset 分发可能早于页面资源加载完，延迟补发两次保证 JS 端能拿到初值。
    ViewCompat.setOnApplyWindowInsetsListener(webView) { view, insets ->
      val bars = insets.getInsets(
        WindowInsetsCompat.Type.statusBars() or WindowInsetsCompat.Type.displayCutout()
      )
      val topCss = bars.top.coerceAtLeast(0) / view.resources.displayMetrics.density
      webView.evaluateJavascript(
        "window.__nativeSafeAreaInsets={top:$topCss};window.dispatchEvent(new Event('native-safe-area'))",
        null,
      )
      insets
    }
    val reinject = Runnable { ViewCompat.requestApplyInsets(webView) }
    webView.postDelayed(reinject, 1500)
    webView.postDelayed(reinject, 4000)
  }
}
