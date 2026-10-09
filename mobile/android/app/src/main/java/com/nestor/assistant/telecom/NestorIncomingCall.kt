package com.nestor.assistant.telecom

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.media.AudioAttributes
import android.media.RingtoneManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.Person
import com.nestor.assistant.MainActivity
import com.nestor.assistant.R

/**
 * Interface d'appel entrant d'un compte Telecom autogere : le systeme ne l'affiche
 * pas lui-meme, il demande a l'application (`Connection.onShowIncomingCallUi`).
 * Une notification de style appel, avec sonnerie, plein ecran, Répondre / Refuser.
 */
object NestorIncomingCall {
    const val CHANNEL_INCOMING = "nestor_incoming"
    const val NOTIFICATION_ID = 428342
    const val ACTION_ANSWER = "com.nestor.assistant.INCOMING_ANSWER"
    const val ACTION_DECLINE = "com.nestor.assistant.INCOMING_DECLINE"

    fun show(context: Context, text: String) {
        createChannel(context)
        val fullScreen = PendingIntent.getActivity(
            context,
            10,
            Intent(context, MainActivity::class.java).apply { flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP },
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
        val answer = PendingIntent.getBroadcast(
            context,
            11,
            Intent(context, NestorCallActionReceiver::class.java).apply { action = ACTION_ANSWER },
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
        val decline = PendingIntent.getBroadcast(
            context,
            12,
            Intent(context, NestorCallActionReceiver::class.java).apply { action = ACTION_DECLINE },
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
        val nestor = Person.Builder().setName("Nestor").setImportant(true).build()
        val notification = NotificationCompat.Builder(context, CHANNEL_INCOMING)
            .setSmallIcon(R.mipmap.ic_launcher)
            .setContentTitle("Nestor vous appelle")
            .setContentText(text)
            .setStyle(NotificationCompat.CallStyle.forIncomingCall(nestor, decline, answer))
            .setFullScreenIntent(fullScreen, true)
            .setCategory(NotificationCompat.CATEGORY_CALL)
            .setPriority(NotificationCompat.PRIORITY_MAX)
            .setOngoing(true)
            .setAutoCancel(false)
            .build()
        context.getSystemService(NotificationManager::class.java)?.notify(NOTIFICATION_ID, notification)
    }

    fun cancel(context: Context) {
        context.getSystemService(NotificationManager::class.java)?.cancel(NOTIFICATION_ID)
    }

    private fun createChannel(context: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val channel = NotificationChannel(CHANNEL_INCOMING, "Appels entrants de Nestor", NotificationManager.IMPORTANCE_HIGH).apply {
            description = "Réveil et urgences : le téléphone sonne"
            setSound(
                RingtoneManager.getDefaultUri(RingtoneManager.TYPE_RINGTONE),
                AudioAttributes.Builder()
                    .setUsage(AudioAttributes.USAGE_NOTIFICATION_RINGTONE)
                    .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                    .build()
            )
            enableVibration(true)
            vibrationPattern = longArrayOf(0, 800, 600, 800, 600, 800)
        }
        context.getSystemService(NotificationManager::class.java)?.createNotificationChannel(channel)
    }
}

/** Boutons Répondre / Refuser de la notification d'appel entrant. */
class NestorCallActionReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val manager = NestorCallManager.getInstance(context.applicationContext)
        when (intent.action) {
            NestorIncomingCall.ACTION_ANSWER -> manager.answerIncoming()
            NestorIncomingCall.ACTION_DECLINE -> manager.rejectIncoming()
        }
    }
}
