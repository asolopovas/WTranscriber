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
