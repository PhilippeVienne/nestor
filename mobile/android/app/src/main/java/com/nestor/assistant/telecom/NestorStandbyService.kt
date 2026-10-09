package com.nestor.assistant.telecom

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.Uri
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.util.Log
import androidx.core.app.NotificationCompat
import com.nestor.assistant.MainActivity
import com.nestor.assistant.R
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import org.json.JSONObject
import java.util.concurrent.TimeUnit
import kotlin.math.min

/**
 * Canal nestord -> telephone hors appel (etapes F et G de .agent/VISION.md).
 *
 * Service de premier plan qui garde un WebSocket leger vers le daemon
 * (`?client=standby`) : le daemon ne lui envoie que les evenements utiles
 * (alertes, contexte, missions), jamais l'audio. Une alerte devient une
 * notification ; une alerte de reveil (`kind == "wake"`) fait sonner le
 * telephone comme un appel entrant, via l'API Telecom. Reconnexion avec delai
 * croissant (5 s a 5 min). Sans Google : la machine est jointe par le reseau
 * prive (Tailscale), au prix d'une connexion maintenue.
 */
class NestorStandbyService : Service() {

    companion object {
        private const val TAG = "NestorStandby"
        const val CHANNEL_STANDBY = "nestor_standby"
        const val CHANNEL_ALERTS = "nestor_alerts"
        const val NOTIFICATION_ID = 428341
        const val ACTION_START = "com.nestor.assistant.STANDBY_START"
        const val ACTION_STOP = "com.nestor.assistant.STANDBY_STOP"
        const val EXTRA_URL = "serverUrl"
        const val EXTRA_TOKEN = "token"

        @Volatile
        var isRunning: Boolean = false
            private set

        fun start(context: Context, serverUrl: String, token: String) {
            val intent = Intent(context, NestorStandbyService::class.java).apply {
                action = ACTION_START
                putExtra(EXTRA_URL, serverUrl)
                putExtra(EXTRA_TOKEN, token)
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                context.startForegroundService(intent)
            } else {
                context.startService(intent)
            }
        }

        fun stop(context: Context) {
            val intent = Intent(context, NestorStandbyService::class.java).apply { action = ACTION_STOP }
            context.startService(intent)
        }

        /** Ajoute `key=value` a l'URL, sauf si le parametre y figure deja. */
        fun withQuery(url: String, key: String, value: String): String {
            if (value.isEmpty() || Regex("[?&]$key=").containsMatchIn(url)) return url
            val separator = if (url.contains('?')) "&" else "?"
            return "$url$separator$key=${Uri.encode(value)}"
        }
    }

    private val handler = Handler(Looper.getMainLooper())
    private val client = OkHttpClient.Builder()
        .pingInterval(60, TimeUnit.SECONDS)
        .readTimeout(0, TimeUnit.MILLISECONDS)
        .connectTimeout(15, TimeUnit.SECONDS)
        .build()
    private var socket: WebSocket? = null
    private var serverUrl = ""
    private var token = ""
    private var attempts = 0
    @Volatile private var stopping = false
    private var alertSeq = 0

    override fun onCreate() {
        super.onCreate()
        createChannels()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) {
            stopping = true
            isRunning = false
            handler.removeCallbacksAndMessages(null)
            socket?.close(1000, "veille arretee")
            socket = null
            stopForeground(STOP_FOREGROUND_REMOVE)
            stopSelf()
            return START_NOT_STICKY
        }
        serverUrl = intent?.getStringExtra(EXTRA_URL) ?: serverUrl
        token = intent?.getStringExtra(EXTRA_TOKEN) ?: token
        stopping = false
        isRunning = true
        val notification = buildStatusNotification("Connexion à Nestor…")
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            startForeground(NOTIFICATION_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
        if (socket == null) connect()
        return START_STICKY
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onDestroy() {
        stopping = true
        isRunning = false
        handler.removeCallbacksAndMessages(null)
        socket?.cancel()
        socket = null
        super.onDestroy()
    }

    private fun standbyUrl(): String = withQuery(withQuery(serverUrl, "client", "standby"), "token", token)

    private fun callUrl(): String = withQuery(withQuery(serverUrl, "client", "mobile"), "token", token)

    private fun connect() {
        if (stopping || serverUrl.isEmpty()) return
        val request = Request.Builder().url(standbyUrl()).build()
        socket = client.newWebSocket(request, object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                if (webSocket !== socket) return
                attempts = 0
                Log.d(TAG, "veille connectee")
                updateStatus("Joignable par Nestor")
            }

            override fun onMessage(webSocket: WebSocket, text: String) {
                if (webSocket !== socket) return
                handleMessage(text)
            }

            override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
                webSocket.close(1000, null)
            }

            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                if (webSocket !== socket) return
                scheduleReconnect("connexion fermée")
            }

            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                if (webSocket !== socket) return
                Log.w(TAG, "veille : echec de connexion", t)
                // Un 200 a la place du 101 : c'est la page publique de Cloudflare qui a repondu,
                // donc le telephone est hors du tailnet.
                scheduleReconnect(
                    when (response?.code) {
                        null -> "daemon injoignable (Tailscale ?)"
                        200 -> "hors du tailnet : activez Tailscale"
                        else -> "refusé (HTTP ${response.code})"
                    }
                )
            }
        })
    }

    private fun scheduleReconnect(reason: String) {
        socket = null
        if (stopping) return
        attempts += 1
        val delayMs = min(5_000L shl (attempts - 1).coerceAtMost(10), 300_000L)
        updateStatus("$reason · nouvel essai dans ${delayMs / 1000} s")
        handler.postDelayed({ if (!stopping) connect() }, delayMs)
    }

    private fun handleMessage(text: String) {
        try {
            val obj = JSONObject(text)
            if (obj.optString("type") != "alert") return
            val kind = obj.optString("kind")
            val body = obj.optString("text")
            if (kind == "wake") {
                NestorCallManager.getInstance(applicationContext).ringIncoming(callUrl(), "Réveil : $body")
            } else {
                notifyAlert(kind, body)
            }
        } catch (e: Exception) {
            Log.w(TAG, "message de veille illisible", e)
        }
    }

    private fun notifyAlert(kind: String, body: String) {
        val title = when (kind) {
            "departure" -> "Il est temps de partir"
            "event_imminent" -> "Rendez-vous imminent"
            "mission_stalled" -> "Mission sans nouvelles"
            "session_fallback" -> "Nestor en mode réduit"
            "quota" -> "Quota Claude"
            "todo_due" -> "Tâches à relancer"
            else -> "Nestor"
        }
        val launch = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java).apply { flags = Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP },
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
        val notification = NotificationCompat.Builder(this, CHANNEL_ALERTS)
            .setSmallIcon(R.mipmap.ic_launcher)
            .setContentTitle(title)
            .setContentText(body)
            .setStyle(NotificationCompat.BigTextStyle().bigText(body))
            .setContentIntent(launch)
            .setAutoCancel(true)
            .setCategory(NotificationCompat.CATEGORY_REMINDER)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .build()
        alertSeq = (alertSeq + 1) % 1000
        getSystemService(NotificationManager::class.java)?.notify(500_000 + alertSeq, notification)
    }

    private fun updateStatus(text: String) {
        getSystemService(NotificationManager::class.java)?.notify(NOTIFICATION_ID, buildStatusNotification(text))
    }

    private fun buildStatusNotification(text: String): Notification {
        val launch = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java).apply { flags = Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP },
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
        val stop = PendingIntent.getService(
            this,
            2,
            Intent(this, NestorStandbyService::class.java).apply { action = ACTION_STOP },
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
        return NotificationCompat.Builder(this, CHANNEL_STANDBY)
            .setSmallIcon(R.mipmap.ic_launcher)
            .setContentTitle("Nestor en veille")
            .setContentText(text)
            .setContentIntent(launch)
            .setOngoing(true)
            .setSilent(true)
            .setCategory(NotificationCompat.CATEGORY_SERVICE)
            .addAction(android.R.drawable.ic_menu_close_clear_cancel, "Arrêter", stop)
            .build()
    }

    private fun createChannels() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val manager = getSystemService(NotificationManager::class.java) ?: return
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL_STANDBY, "Nestor en veille", NotificationManager.IMPORTANCE_LOW).apply {
                description = "Connexion maintenue avec le daemon hors appel"
                setSound(null, null)
                enableVibration(false)
            }
        )
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL_ALERTS, "Alertes de Nestor", NotificationManager.IMPORTANCE_HIGH).apply {
                description = "Départ à temps, rendez-vous, missions, quota"
            }
        )
    }
}
