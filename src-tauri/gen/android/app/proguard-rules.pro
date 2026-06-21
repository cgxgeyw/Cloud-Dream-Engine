# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.
#
# For more details, see
#   http://developer.android.com/guide/developing/tools/proguard.html

# If your project uses WebView with JS, uncomment the following
# and specify the fully qualified class name to the JavaScript interface
# class:
#-keepclassmembers class fqcn.of.javascript.interface.for.webview {
#   public *;
#}

# Uncomment this to preserve the line number information for
# debugging stack traces.
#-keepattributes SourceFile,LineNumberTable

# If you keep the line number information, uncomment this to
# hide the original source file name.
#-renamesourcefileattribute SourceFile

# Tauri/Wry 在 Android 启动期通过 JNI 按方法名调用这些桥接方法。
# release 混淆后若重命名 Kotlin 生成的访问器，会在 onActivityCreate 阶段直接崩溃。
-keep class com.dreamnarrativeengine.app.WryActivity {
    public <init>(...);
    public int getId();
    public void setId(int);
    public boolean getHandleBackNavigation();
    public void onWebViewCreate(android.webkit.WebView);
    public void setWebView(com.dreamnarrativeengine.app.RustWebView);
    public java.lang.String getVersion();
    protected void onCreate(android.os.Bundle);
    public void onWindowFocusChanged(boolean);
    protected void onSaveInstanceState(android.os.Bundle);
    protected void onPause();
    protected void onResume();
    protected void onDestroy();
    public void onLowMemory();
    protected void onNewIntent(android.content.Intent);
    public java.lang.Class getAppClass(java.lang.String);
    public int startActivity(java.lang.Class);
}

# Called from Rust/JNI by class and method name.
-keep class com.dreamnarrativeengine.app.ScheduledNotificationReceiver { *; }
-keep class com.dreamnarrativeengine.app.ScheduledNotificationReceiver$Companion { *; }
