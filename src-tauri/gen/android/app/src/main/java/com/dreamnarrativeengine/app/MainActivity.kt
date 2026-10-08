package com.dreamnarrativeengine.app

import android.os.Bundle
import android.util.Log
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
    // 便于 adb + chrome://inspect 实机调试
    WebView.setWebContentsDebuggingEnabled(true)

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

    // 安卓 WebView 里 env(safe-area-inset-top) 恒为 0；且 enableEdgeToEdge + adjustResize
    // 时软键盘往往不会缩 WebView。这里把 statusBars/displayCutout 与 IME(键盘) inset
    // 换算成 CSS px 注入，前端只按这一份键盘高度避让一次。
    ViewCompat.setOnApplyWindowInsetsListener(webView) { view, insets ->
      val density = view.resources.displayMetrics.density
      // 系统栏避让一律读根窗口原始 inset：分发的 insets 可能被容器祖先（Tauri 容器）
      // 消费掉 statusBars，导致 WebView 收到 top=0、顶部避让整体失效。
      val source = ViewCompat.getRootWindowInsets(view) ?: insets
      val bars = source.getInsets(
        WindowInsetsCompat.Type.statusBars() or WindowInsetsCompat.Type.displayCutout()
      )
      val topCss = bars.top.coerceAtLeast(0) / density
      // 不要只信 isVisible：部分 ROM 在动画/fitInsets 受控时会短暂报 false。
      // 注意：getInsetsIgnoringVisibility(IME) 在部分 ROM 会抛
      // IllegalArgumentException: Unable to query the maximum insets for IME，必须避开。
      // WindowInsetsCompat.getInsets 返回 androidx.core.graphics.Insets。
      var imeVisible = false
      var keyboardPx = 0
      try {
        imeVisible = source.isVisible(WindowInsetsCompat.Type.ime())
        val ime = source.getInsets(WindowInsetsCompat.Type.ime())
        keyboardPx = if (imeVisible) ime.bottom.coerceAtLeast(0) else 0
        Log.d("HlKeyboard", "density=$density topCss=$topCss ime.bottom=${ime.bottom} visible=$imeVisible")
      } catch (e: Exception) {
        Log.w("HlKeyboard", "ime insets failed: ${e.message}")
      }
      val keyboardCss = keyboardPx / density
      // 底部系统栏（手势条/三键导航）也由框架统一预留；键盘弹出时导航栏被覆盖，归零避免双重垫高。
      val navBottom = source.getInsets(WindowInsetsCompat.Type.navigationBars()).bottom.coerceAtLeast(0)
      val bottomCss = (if (imeVisible) 0 else navBottom) / density
      Log.d("HlKeyboard", "inject topCss=$topCss keyboardCss=$keyboardCss bottomCss=$bottomCss")
      webView.evaluateJavascript(
        """
        window.__nativeSafeAreaInsets={top:$topCss,bottom:$bottomCss};
        window.__nativeKeyboardHeight=$keyboardCss;
        window.dispatchEvent(new Event('native-safe-area'));
        window.dispatchEvent(new Event('native-keyboard'));
        """.trimIndent(),
        null,
      )
      insets
    }
    val reinject = Runnable { ViewCompat.requestApplyInsets(webView) }
    webView.postDelayed(reinject, 1500)
    webView.postDelayed(reinject, 4000)
  }
}
