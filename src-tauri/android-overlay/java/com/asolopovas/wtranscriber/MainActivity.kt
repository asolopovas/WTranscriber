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
import android.provider.MediaStore
import android.provider.OpenableColumns
import android.provider.Settings
import android.webkit.WebView
import android.widget.Toast
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
    handleSharedAudio(intent)
    runCatching { wtSetActivity(this) }
    requestPostNotificationsIfNeeded()
    requestIgnoreBatteryOptimizationsIfNeeded()
  }

  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    setIntent(intent)
    handleSharedAudio(intent)
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
  fun displayNameForUri(uriText: String): String {
    val uri = Uri.parse(uriText)
    if (uri.scheme == "content") {
      runCatching {
        contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
          if (cursor.moveToFirst()) {
            val idx = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
            if (idx >= 0) return cursor.getString(idx) ?: ""
          }
        }
      }
    }
    return uri.lastPathSegment ?: ""
  }

  @Keep
  fun copyUriToFile(uriText: String, destPath: String): Boolean {
    val uri = Uri.parse(uriText)
    val dest = java.io.File(destPath)
    dest.parentFile?.mkdirs()
    return try {
      contentResolver.openInputStream(uri)?.use { input ->
        dest.outputStream().use { output ->
          input.copyTo(output, 1024 * 1024)
        }
      } ?: return false
      true
    } catch (e: Exception) {
      android.util.Log.e("WTranscriber", "copyUriToFile failed: $uriText -> $destPath", e)
      dest.delete()
      false
    }
  }

  private fun handleSharedAudio(intent: Intent?) {
    if (intent == null) return
    val uris = when (intent.action) {
      Intent.ACTION_SEND -> listOfNotNull(streamUri(intent))
      Intent.ACTION_SEND_MULTIPLE -> streamUris(intent)
      else -> emptyList()
    }
    if (uris.isEmpty()) return
    var copied = 0
    for (uri in uris) {
      if (copySharedAudio(uri)) copied += 1
    }
    if (copied > 0) {
      Toast.makeText(this, "Added $copied audio file${if (copied == 1) "" else "s"}", Toast.LENGTH_SHORT).show()
    }
  }

  @Suppress("DEPRECATION")
  private fun streamUri(intent: Intent): Uri? {
    return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
      intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java)
    } else {
      intent.getParcelableExtra(Intent.EXTRA_STREAM)
    }
  }

  @Suppress("DEPRECATION")
  private fun streamUris(intent: Intent): List<Uri> {
    return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
      intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM, Uri::class.java) ?: emptyList()
    } else {
      intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM) ?: emptyList()
    }
  }

  private fun copySharedAudio(uri: Uri): Boolean {
    val name = safeSharedName(displayNameForUri(uri.toString()).ifBlank { uri.lastPathSegment ?: "shared-audio" })
    val dir = java.io.File(Environment.getExternalStorageDirectory(), "WTranscriber/transcripts")
    dir.mkdirs()
    val dest = uniqueFile(dir, name)
    return try {
      contentResolver.openInputStream(uri)?.use { input ->
        dest.outputStream().use { output ->
          input.copyTo(output, 1024 * 1024)
        }
      } ?: return false
      sharedModifiedMs(uri)?.let { dest.setLastModified(it) }
      true
    } catch (e: Exception) {
      android.util.Log.e("WTranscriber", "copy shared audio failed: $uri -> ${dest.absolutePath}", e)
      dest.delete()
      false
    }
  }

  private fun sharedModifiedMs(uri: Uri): Long? {
    val columns = arrayOf(MediaStore.MediaColumns.DATE_MODIFIED, MediaStore.MediaColumns.DATE_ADDED)
    runCatching {
      contentResolver.query(uri, columns, null, null, null)?.use { cursor ->
        if (!cursor.moveToFirst()) return null
        for (column in columns) {
          val idx = cursor.getColumnIndex(column)
          if (idx >= 0) {
            val seconds = cursor.getLong(idx)
            if (seconds > 0) return seconds * 1000
          }
        }
      }
    }
    return null
  }

  private fun safeSharedName(name: String): String {
    val cleaned = name.substringAfterLast('/').replace(Regex("[\\\\/:*?\"<>|]"), "_").trim()
    val withExt = if (cleaned.contains('.')) cleaned else "$cleaned.opus"
    return withExt.ifBlank { "shared-audio.opus" }
  }

  private fun uniqueFile(dir: java.io.File, name: String): java.io.File {
    val dot = name.lastIndexOf('.')
    val base = if (dot > 0) name.substring(0, dot) else name
    val ext = if (dot > 0) name.substring(dot) else ""
    var candidate = java.io.File(dir, name)
    var i = 1
    while (candidate.exists()) {
      candidate = java.io.File(dir, "$base-$i$ext")
      i += 1
    }
    return candidate
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
