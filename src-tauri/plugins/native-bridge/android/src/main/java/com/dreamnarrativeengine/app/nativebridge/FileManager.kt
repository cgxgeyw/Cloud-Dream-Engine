package com.dreamnarrativeengine.app.nativebridge

import android.content.Context
import android.content.Intent
import android.util.Base64
import androidx.core.content.FileProvider
import app.tauri.plugin.JSObject
import java.io.File

/**
 * 世界包平台能力（第 12 项，第一批文件能力）的安卓实现。
 * 只做纯 IO 与系统调起：路径由 Rust 侧拼好并做过穿越防护，
 * 本类不解释路径语义、不做授权判断（授权在 Rust/DB 层）。
 */
class FileManager(private val context: Context) {

  fun readFile(path: String): JSObject {
    return try {
      val file = File(path)
      if (!file.exists() || !file.isFile) {
        return errorResult("io: 文件不存在。")
      }
      val bytes = file.readBytes()
      JSObject()
        .put("ok", true)
        .put("size", bytes.size)
        .put("data_base64", Base64.encodeToString(bytes, Base64.NO_WRAP))
    } catch (error: Exception) {
      errorResult("io: 读取失败：${error.message}")
    }
  }

  fun writeFile(path: String, dataBase64: String): JSObject {
    return try {
      val bytes = Base64.decode(dataBase64, Base64.DEFAULT)
      val file = File(path)
      file.parentFile?.mkdirs()
      file.writeBytes(bytes)
      JSObject()
        .put("ok", true)
        .put("size", bytes.size)
    } catch (error: Exception) {
      errorResult("io: 写入失败：${error.message}")
    }
  }

  fun shareFile(path: String): JSObject {
    return try {
      val file = File(path)
      if (!file.exists() || !file.isFile) {
        return errorResult("io: 文件不存在。")
      }
      val authority = context.packageName + ".fileprovider"
      val uri = FileProvider.getUriForFile(context, authority, file)
      val intent = Intent(Intent.ACTION_SEND).apply {
        type = guessMimeType(file.name)
        putExtra(Intent.EXTRA_STREAM, uri)
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
      }
      val chooser = Intent.createChooser(intent, "分享文件")
      chooser.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
      context.startActivity(chooser)
      JSObject().put("ok", true)
    } catch (error: Exception) {
      errorResult("io: 分享失败：${error.message}")
    }
  }

  /**
   * 把 APK assets 下的整个目录复制到应用私有目录。
   * 内置向量模型（model.safetensors 等）必须落到真实文件路径才能被 mmap 加载，
   * 而 assets 只能以流读取，因此首次启动（或应用升级）后解包一次。
   * 用 `.asset-version` 记录已解包的应用版本，命中就直接跳过。
   */
  fun copyAssetDir(assetPath: String, targetDir: String): JSObject {
    return try {
      val assets = context.assets
      val root = File(targetDir)
      val version = appVersionName()
      val marker = File(root, ".asset-version")
      if (marker.isFile && marker.readText().trim() == version) {
        return JSObject().put("ok", true).put("copied", 0).put("skipped", 1)
      }

      var copied = 0
      val queue = ArrayDeque<Pair<String, File>>()
      queue.addLast(assetPath.trimEnd('/') to root)
      while (queue.isNotEmpty()) {
        val (assetDir, dir) = queue.removeFirst()
        dir.mkdirs()
        for (name in assets.list(assetDir).orEmpty()) {
          val childAsset = "$assetDir/$name"
          val childFile = File(dir, name)
          if (assets.list(childAsset).orEmpty().isEmpty()) {
            childFile.parentFile?.mkdirs()
            assets.open(childAsset).use { input ->
              childFile.outputStream().use { output -> input.copyTo(output) }
            }
            copied++
          } else {
            queue.addLast(childAsset to childFile)
          }
        }
      }
      marker.writeText(version)
      JSObject().put("ok", true).put("copied", copied).put("skipped", 0)
    } catch (error: Exception) {
      errorResult("io: 复制内置资源失败：${error.message}")
    }
  }

  private fun appVersionName(): String = runCatching {
    val info = context.packageManager.getPackageInfo(context.packageName, 0)
    info.versionName ?: ""
  }.getOrDefault("")

  private fun guessMimeType(name: String): String {
    val ext = name.substringAfterLast('.', "").lowercase()
    return when (ext) {
      "txt", "md", "log", "json", "csv" -> "text/plain"
      "png" -> "image/png"
      "jpg", "jpeg" -> "image/jpeg"
      "gif" -> "image/gif"
      "webp" -> "image/webp"
      "mp3" -> "audio/mpeg"
      "wav" -> "audio/wav"
      "mp4" -> "video/mp4"
      "pdf" -> "application/pdf"
      "zip" -> "application/zip"
      else -> "application/octet-stream"
    }
  }

  private fun errorResult(message: String): JSObject {
    return JSObject()
      .put("ok", false)
      .put("error", message)
  }
}
