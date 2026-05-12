package com.asolopovas.wtranscriber

import android.Manifest
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Environment
import android.os.PowerManager
import android.provider.Settings
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.annotation.Keep
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat

@Keep
class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    if (BuildConfig.DEBUG || (applicationInfo.flags and android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE) != 0) {
      WebView.setWebContentsDebuggingEnabled(true)
    }
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    runCatching { wtSetActivity(this) }
    requestPostNotificationsIfNeeded()
    requestIgnoreBatteryOptimizationsIfNeeded()
  }

  @Keep
  fun hasAllFilesAccess(): Boolean {
    return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
      Environment.isExternalStorageManager()
    } else {
      true
    }
  }

  @Keep
  fun requestAllFilesAccess() {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) return
    if (Environment.isExternalStorageManager()) return
    val intent = Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION).apply {
      data = Uri.parse("package:$packageName")
      addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    }
    runCatching { startActivity(intent) }.onFailure {
      val fallback = Intent(Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION).apply {
        addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
      }
      runCatching { startActivity(fallback) }
    }
  }

  @Keep
  fun startTranscriptionService(title: String) {
    TranscriptionService.start(applicationContext, title)
  }

  @Keep
  fun stopTranscriptionService() {
    TranscriptionService.stop(applicationContext)
  }

  @Keep
  fun notifyTranscriptionDone(title: String, text: String, success: Boolean) {
    TranscriptionService.notifyDone(applicationContext, title, text, success)
  }

  @Keep
  fun revealPath(path: String): Boolean {
    val target = java.io.File(path)
    if (!target.exists()) return false
    val folder = if (target.isDirectory) target else target.parentFile ?: return false
    val external = Environment.getExternalStorageDirectory().absolutePath
    if (!folder.absolutePath.startsWith(external)) {
      android.util.Log.w("WTranscriber", "revealPath: $folder is not under external storage")
      return false
    }
    val rel = folder.absolutePath.substring(external.length).trimStart('/')
    val docId = if (rel.isEmpty()) "primary:" else "primary:$rel"
    val uri = android.provider.DocumentsContract.buildDocumentUri(
      "com.android.externalstorage.documents",
      docId,
    )
    val intent = Intent(Intent.ACTION_VIEW).apply {
      setDataAndType(uri, android.provider.DocumentsContract.Document.MIME_TYPE_DIR)
      addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
      addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    }
    return try {
      startActivity(intent)
      true
    } catch (e: android.content.ActivityNotFoundException) {
      android.util.Log.w("WTranscriber", "revealPath: no file manager handles documents URI $uri")
      false
    } catch (e: Exception) {
      android.util.Log.w("WTranscriber", "revealPath failed for $path: $e")
      false
    }
  }

  @Keep
  fun shareText(title: String, text: String) {
    val send = Intent(Intent.ACTION_SEND).apply {
      type = "text/plain"
      putExtra(Intent.EXTRA_TEXT, text)
      if (title.isNotEmpty()) {
        putExtra(Intent.EXTRA_SUBJECT, title)
        putExtra(Intent.EXTRA_TITLE, title)
      }
    }
    val chooser = Intent.createChooser(send, if (title.isNotEmpty()) title else null).apply {
      addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    }
    runCatching { applicationContext.startActivity(chooser) }
  }

  private fun requestPostNotificationsIfNeeded() {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
    val granted = ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS) ==
      PackageManager.PERMISSION_GRANTED
    if (granted) return
    ActivityCompat.requestPermissions(this, arrayOf(Manifest.permission.POST_NOTIFICATIONS), REQ_POST_NOTIFICATIONS)
  }

  private fun requestIgnoreBatteryOptimizationsIfNeeded() {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.M) return
    val pm = getSystemService(Context.POWER_SERVICE) as PowerManager
    if (pm.isIgnoringBatteryOptimizations(packageName)) return
    val intent = Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS).apply {
      data = Uri.parse("package:$packageName")
      addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    }
    runCatching { startActivity(intent) }
  }

  private external fun wtSetActivity(activity: MainActivity)

  companion object {
    private const val REQ_POST_NOTIFICATIONS = 4711
  }
}
