package com.asolopovas.wtranscriber

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import androidx.annotation.Keep
import androidx.core.app.NotificationCompat

@Keep
class TranscriptionService : Service() {
  private var wakeLock: PowerManager.WakeLock? = null

  override fun onBind(intent: Intent?): IBinder? = null

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    val title = intent?.getStringExtra(EXTRA_TITLE) ?: getString(R.string.transcription_running)
    ensureChannel(this)
    val notif = buildOngoing(title)
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
      startForeground(ONGOING_ID, notif, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
    } else {
      startForeground(ONGOING_ID, notif)
    }
    if (wakeLock == null) {
      val pm = getSystemService(Context.POWER_SERVICE) as PowerManager
      wakeLock = pm.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "wtranscriber:transcribe").also {
        it.setReferenceCounted(false)
        it.acquire(MAX_WAKE_MS)
      }
    }
    return START_NOT_STICKY
  }

  override fun onDestroy() {
    runCatching { wakeLock?.takeIf { it.isHeld }?.release() }
    wakeLock = null
    super.onDestroy()
  }

  private fun buildOngoing(title: String): Notification {
    val pi = launchPendingIntent(this)
    return NotificationCompat.Builder(this, CHANNEL_ID)
      .setSmallIcon(android.R.drawable.stat_notify_sync)
      .setContentTitle(title)
      .setContentText(getString(R.string.transcription_running_text))
      .setOngoing(true)
      .setPriority(NotificationCompat.PRIORITY_LOW)
      .setCategory(NotificationCompat.CATEGORY_PROGRESS)
      .setContentIntent(pi)
      .setForegroundServiceBehavior(NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE)
      .build()
  }

  companion object {
    private const val CHANNEL_ID = "transcription"
    private const val ONGOING_ID = 1001
    private const val DONE_ID_BASE = 2000
    private const val EXTRA_TITLE = "title"
    private const val MAX_WAKE_MS = 12L * 60L * 60L * 1000L

    fun start(ctx: Context, title: String) {
      val i = Intent(ctx, TranscriptionService::class.java).putExtra(EXTRA_TITLE, title)
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
        ctx.startForegroundService(i)
      } else {
        ctx.startService(i)
      }
    }

    fun stop(ctx: Context) {
      ctx.stopService(Intent(ctx, TranscriptionService::class.java))
    }

    fun notifyDone(ctx: Context, title: String, text: String, success: Boolean) {
      ensureChannel(ctx)
      val pi = launchPendingIntent(ctx)
      val icon = if (success) {
        android.R.drawable.stat_sys_download_done
      } else {
        android.R.drawable.stat_notify_error
      }
      val notif = NotificationCompat.Builder(ctx, CHANNEL_ID)
        .setSmallIcon(icon)
        .setContentTitle(title)
        .setContentText(text)
        .setStyle(NotificationCompat.BigTextStyle().bigText(text))
        .setAutoCancel(true)
        .setPriority(NotificationCompat.PRIORITY_DEFAULT)
        .setCategory(NotificationCompat.CATEGORY_STATUS)
        .setContentIntent(pi)
        .build()
      val mgr = ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
      mgr.notify(DONE_ID_BASE + (System.currentTimeMillis() and 0xffff).toInt(), notif)
    }

    private fun ensureChannel(ctx: Context) {
      if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
      val mgr = ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
      if (mgr.getNotificationChannel(CHANNEL_ID) != null) return
      val ch = NotificationChannel(
        CHANNEL_ID,
        ctx.getString(R.string.transcription_channel),
        NotificationManager.IMPORTANCE_LOW,
      ).apply {
        description = ctx.getString(R.string.transcription_channel_desc)
        setShowBadge(false)
      }
      mgr.createNotificationChannel(ch)
    }

    private fun launchPendingIntent(ctx: Context): PendingIntent {
      val launch = ctx.packageManager.getLaunchIntentForPackage(ctx.packageName)
        ?: Intent(ctx, MainActivity::class.java)
      launch.addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP)
      val flags = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
      } else {
        PendingIntent.FLAG_UPDATE_CURRENT
      }
      return PendingIntent.getActivity(ctx, 0, launch, flags)
    }
  }
}
