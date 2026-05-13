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
    val underExternal = folder.absolutePath.startsWith(external)
    val rel = if (underExternal) folder.absolutePath.substring(external.length).trimStart('/') else ""
    val docId = if (rel.isEmpty()) "primary:" else "primary:$rel"
    val docUri = android.provider.DocumentsContract.buildDocumentUri(
      "com.android.externalstorage.documents",
      docId,
    )
    val attempts = mutableListOf<Intent>()
    if (underExternal) {
      attempts += Intent(Intent.ACTION_VIEW).apply {
        setDataAndType(docUri, android.provider.DocumentsContract.Document.MIME_TYPE_DIR)
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
      }
      attempts += Intent.createChooser(
        Intent(Intent.ACTION_VIEW).apply {
          setDataAndType(docUri, android.provider.DocumentsContract.Document.MIME_TYPE_DIR)
          addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        },
        "Open folder",
      ).apply { addFlags(Intent.FLAG_ACTIVITY_NEW_TASK) }
      attempts += Intent("com.sec.android.app.myfiles.PICK_DATA").apply {
        setPackage("com.sec.android.app.myfiles")
        putExtra("CONTENT_TYPE", "*/*")
        putExtra("FOLDERPATH", folder.absolutePath)
        addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
      }
      attempts += Intent(Intent.ACTION_VIEW).apply {
        setPackage("com.google.android.apps.nbu.files")
        setDataAndType(Uri.parse("file://${folder.absolutePath}"), "resource/folder")
        addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
      }
    }
    for (intent in attempts) {
      try {
        startActivity(intent)
        return true
      } catch (e: Exception) {
        android.util.Log.w("WTranscriber", "revealPath: ${intent.action}/${intent.`package`} failed: $e")
      }
    }
    return try {
      val cm = getSystemService(Context.CLIPBOARD_SERVICE) as android.content.ClipboardManager
      cm.setPrimaryClip(android.content.ClipData.newPlainText("WTranscriber path", folder.absolutePath))
      android.widget.Toast.makeText(
        this,
        "No file manager available. Path copied: ${folder.absolutePath}",
        android.widget.Toast.LENGTH_LONG,
      ).show()
      true
    } catch (e: Exception) {
      android.util.Log.w("WTranscriber", "revealPath: clipboard fallback failed: $e")
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
