package com.nestor.assistant.telecom

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
import androidx.core.app.NotificationCompat
import com.nestor.assistant.MainActivity
import com.nestor.assistant.R

class NestorCallForegroundService : Service() {

    companion object {
        const val CHANNEL_ID = "nestor_call_channel"
        const val NOTIFICATION_ID = 428340
        const val ACTION_START_CALL = "com.nestor.assistant.START_CALL"
        const val ACTION_STOP_CALL = "com.nestor.assistant.STOP_CALL"
        const val ACTION_HANG_UP = "com.nestor.assistant.HANG_UP"

        fun start(context: Context) {
            val intent = Intent(context, NestorCallForegroundService::class.java).apply {
                action = ACTION_START_CALL
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                context.startForegroundService(intent)
            } else {
                context.startService(intent)
            }
        }

        fun stop(context: Context) {
            val intent = Intent(context, NestorCallForegroundService::class.java).apply {
                action = ACTION_STOP_CALL
            }
            context.startService(intent)
        }
    }

    override fun onCreate() {
        super.onCreate()
        createNotificationChannel()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_STOP_CALL -> {
                stopForeground(STOP_FOREGROUND_REMOVE)
                stopSelf()
                return START_NOT_STICKY
            }
            ACTION_HANG_UP -> {
                NestorCallManager.getInstance(applicationContext).endCall()
                stopForeground(STOP_FOREGROUND_REMOVE)
                stopSelf()
                return START_NOT_STICKY
            }
            else -> {
                val notification = buildCallNotification("Appel en cours avec Nestor", "Microphone actif • Communication en cours")
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                    var serviceType = ServiceInfo.FOREGROUND_SERVICE_TYPE_PHONE_CALL
                    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                        serviceType = serviceType or ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE
                    }
                    startForeground(NOTIFICATION_ID, notification, serviceType)
                } else {
                    startForeground(NOTIFICATION_ID, notification)
                }
            }
        }
        return START_STICKY
    }

    override fun onBind(intent: Intent?): IBinder? = null

    private fun createNotificationChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                "Appels Nestor",
                NotificationManager.IMPORTANCE_HIGH
            ).apply {
                description = "Notifications actives pour les appels vocaux avec Nestor"
                setSound(null, null)
                enableVibration(false)
            }
            val manager = getSystemService(NotificationManager::class.java)
            manager?.createNotificationChannel(channel)
        }
    }

    private fun buildCallNotification(title: String, text: String): Notification {
        val launchIntent = Intent(this, MainActivity::class.java).apply {
            flags = Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP
        }
        val contentPendingIntent = PendingIntent.getActivity(
            this,
            0,
            launchIntent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )

        val hangUpIntent = Intent(this, NestorCallForegroundService::class.java).apply {
            action = ACTION_HANG_UP
        }
        val hangUpPendingIntent = PendingIntent.getService(
            this,
            1,
            hangUpIntent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )

        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle(title)
            .setContentText(text)
            .setSmallIcon(R.mipmap.ic_launcher)
            .setContentIntent(contentPendingIntent)
            .setOngoing(true)
            .setCategory(NotificationCompat.CATEGORY_CALL)
            .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
            .addAction(android.R.drawable.ic_menu_close_clear_cancel, "Raccrocher", hangUpPendingIntent)
            .build()
    }
}
