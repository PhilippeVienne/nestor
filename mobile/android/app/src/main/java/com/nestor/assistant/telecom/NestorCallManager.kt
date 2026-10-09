package com.nestor.assistant.telecom

import android.annotation.SuppressLint
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.Manifest
import android.content.pm.PackageManager
import android.graphics.drawable.Icon
import android.location.Location
import android.location.LocationListener
import android.location.LocationManager
import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioRecord
import android.media.AudioTrack
import android.media.MediaRecorder
import android.media.audiofx.AcousticEchoCanceler
import android.media.audiofx.NoiseSuppressor
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Looper
import android.os.PowerManager
import android.telecom.CallAudioState
import android.telecom.DisconnectCause
import android.telecom.PhoneAccount
import android.telecom.PhoneAccountHandle
import android.telecom.TelecomManager
import android.telecom.VideoProfile
import android.util.Base64
import android.util.Log
import androidx.core.content.ContextCompat
import com.nestor.assistant.R
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString
import okio.ByteString.Companion.toByteString
import org.json.JSONObject
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import kotlin.math.sqrt

class NestorCallManager private constructor(private val context: Context) {

    companion object {
        private const val TAG = "NestorCallManager"
        private const val ACCOUNT_ID = "NestorCallAccount"
        const val SAMPLE_RATE_RECORD = 16000
        const val SAMPLE_RATE_PLAYBACK = 24000

        @SuppressLint("StaticFieldLeak")
        @Volatile
        private var instance: NestorCallManager? = null

        fun getInstance(context: Context): NestorCallManager {
            return instance ?: synchronized(this) {
                instance ?: NestorCallManager(context.applicationContext).also { instance = it }
            }
        }
    }

    interface CallEventListener {
        fun onCallStateChanged(state: String, details: String? = null)
        fun onNestorStateChanged(state: String)
        fun onAudioRouteChanged(route: String, isMuted: Boolean)
        fun onAudioLevels(rms: Float, peak: Float)
        fun onTranscript(role: String, text: String, isPartial: Boolean)
        fun onToolCall(id: String, name: String, status: String)
        fun onBackendStatus(activeBackend: String, isFallback: Boolean, reason: String?)
        // Decision du juge ; `pending` vrai quand une confirmation de l'utilisateur est attendue.
        fun onJudgeVerdict(
            id: Long,
            source: String,
            text: String,
            decision: String,
            score: Int?,
            category: String?,
            rationale: String?,
            pending: Boolean,
            atMs: Long,
        )
        fun onJudgeResolved(id: Long, approved: Boolean)
        // Ecriture d'un connecteur externe en attente d'accord.
        fun onToolApproval(id: Long, server: String, tool: String, arguments: String, atMs: Long)
        fun onToolApprovalResolved(id: Long, approved: Boolean)
        // Interruption vocale detectee par le daemon (`rms` : niveau de la parole, si fourni).
        fun onInterrupt(rms: Float?)
    }

    var eventListener: CallEventListener? = null

    private val telecomManager = context.getSystemService(Context.TELECOM_SERVICE) as TelecomManager
    private val audioManager = context.getSystemService(Context.AUDIO_SERVICE) as AudioManager
    private val powerManager = context.getSystemService(Context.POWER_SERVICE) as PowerManager
    private val locationManager = context.getSystemService(Context.LOCATION_SERVICE) as LocationManager

    // Alimente Config::place_at cote nestord (reconnaissance "Monsieur est chez lui").
    // Pas de demande de permission ici : comme pour RECORD_AUDIO, elle est
    // supposee deja accordee ; sans elle, on se contente de ne rien envoyer.
    private var locationListener: LocationListener? = null

    private var phoneAccountHandle: PhoneAccountHandle? = null
    private var activeConnection: NestorConnection? = null

    private var proximityWakeLock: PowerManager.WakeLock? = null

    // Audio recording & playback
    private var audioRecord: AudioRecord? = null
    @Volatile private var audioTrack: AudioTrack? = null

    // Un segment TTS (une phrase entiere) depasse le tampon de l'AudioTrack : il est ecrit en
    // mode bloquant par un thread dedie, pour ne pas retenir le thread du WebSocket.
    private class PcmChunk(val bytes: ByteArray, val sampleRate: Int)
    private val playbackQueue = LinkedBlockingQueue<PcmChunk>()
    private var playbackThread: Thread? = null
    // Incremente a chaque vidage de la lecture : le segment en cours d'ecriture est abandonne.
    private val playbackGen = AtomicInteger(0)

    // Fin de lecture estimee de la voix de Nestor : le micro est envoye en continu jusque-la
    // (le daemon annule l'echo avec son signal de reference, il lui faut un flux sans trous).
    @Volatile private var playbackEndMs = 0L
    @Volatile private var gateBypassUntilMs = 0L
    private var echoCanceler: AcousticEchoCanceler? = null
    private var noiseSuppressor: NoiseSuppressor? = null

    private val isRecording = AtomicBoolean(false)
    private val isPlaying = AtomicBoolean(false)
    private var recordThread: Thread? = null

    @Volatile var isMicMuted: Boolean = false
    @Volatile var isCallActive: Boolean = false
    @Volatile var currentServerUrl: String = "ws://10.0.2.2:8340/ws"

    // WebSocket
    private var okHttpClient: OkHttpClient = OkHttpClient.Builder()
        .readTimeout(0, TimeUnit.MILLISECONDS)
        .connectTimeout(10, TimeUnit.SECONDS)
        .build()
    private var webSocket: WebSocket? = null

    init {
        initPhoneAccount()
        initProximityLock()
    }

    private fun initPhoneAccount() {
        try {
            val componentName = ComponentName(context, NestorConnectionService::class.java)
            phoneAccountHandle = PhoneAccountHandle(componentName, ACCOUNT_ID)

            val builder = PhoneAccount.builder(phoneAccountHandle, "Nestor")
                .setCapabilities(PhoneAccount.CAPABILITY_SELF_MANAGED)
                .setHighlightColor(0xFF38BDF8.toInt())
                .setShortDescription("Assistant IA Nestor")
                .addSupportedUriScheme(PhoneAccount.SCHEME_TEL)

            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                builder.setIcon(Icon.createWithResource(context, R.mipmap.ic_launcher))
            }

            telecomManager.registerPhoneAccount(builder.build())
            Log.d(TAG, "PhoneAccount registered successfully")
        } catch (e: Exception) {
            Log.e(TAG, "Error registering PhoneAccount", e)
        }
    }

    private fun initProximityLock() {
        try {
            if (powerManager.isWakeLockLevelSupported(PowerManager.PROXIMITY_SCREEN_OFF_WAKE_LOCK)) {
                proximityWakeLock = powerManager.newWakeLock(
                    PowerManager.PROXIMITY_SCREEN_OFF_WAKE_LOCK,
                    "Nestor:ProximityLock"
                )
            }
        } catch (e: Exception) {
            Log.w(TAG, "Proximity wake lock not supported", e)
        }
    }

    fun startCall(serverUrl: String) {
        if (isCallActive) {
            Log.w(TAG, "Call is already active")
            return
        }

        currentServerUrl = serverUrl
        eventListener?.onCallStateChanged("CONNECTING", "Initialisation de l'appel...")

        // Register phone account if needed
        initPhoneAccount()

        try {
            val uri = Uri.fromParts("tel", "nestor", null)
            val extras = Bundle().apply {
                putParcelable(TelecomManager.EXTRA_PHONE_ACCOUNT_HANDLE, phoneAccountHandle)
                putInt(TelecomManager.EXTRA_START_CALL_WITH_VIDEO_STATE, VideoProfile.STATE_AUDIO_ONLY)
            }

            telecomManager.placeCall(uri, extras)
            Log.d(TAG, "TelecomManager.placeCall dispatched")
        } catch (e: SecurityException) {
            Log.e(TAG, "Permission denied for placeCall, falling back to direct call", e)
            startDirectCallSession()
        } catch (e: Exception) {
            Log.e(TAG, "Error placing call, falling back to direct call", e)
            startDirectCallSession()
        }
    }

    fun registerActiveConnection(connection: NestorConnection) {
        Log.d(TAG, "registerActiveConnection")
        activeConnection = connection
        connection.onConnected()
        startCallSession()
    }

    // ------------------------------------------------------------ appel entrant
    // Declenche par NestorStandbyService (alerte de reveil) : le telephone sonne via
    // l'API Telecom ; la session audio ne demarre qu'a la reponse.

    @Volatile private var incomingText: String = "Nestor vous appelle"

    fun ringIncoming(callUrl: String, text: String) {
        if (isCallActive || activeConnection != null) {
            Log.w(TAG, "appel entrant ignore : un appel est deja en cours")
            return
        }
        currentServerUrl = callUrl
        incomingText = text
        initPhoneAccount()
        try {
            val extras = Bundle().apply {
                putParcelable(TelecomManager.EXTRA_PHONE_ACCOUNT_HANDLE, phoneAccountHandle)
            }
            telecomManager.addNewIncomingCall(phoneAccountHandle, extras)
            Log.d(TAG, "appel entrant signale a Telecom")
        } catch (e: Exception) {
            // Sans Telecom (compte refuse), on sonne quand meme : la reponse lance l'appel.
            Log.e(TAG, "addNewIncomingCall impossible, sonnerie directe", e)
            showIncomingUi()
        }
    }

    /** Connexion entrante creee par Telecom : elle sonne, rien d'autre tant qu'on ne repond pas. */
    fun registerIncomingConnection(connection: NestorConnection) {
        Log.d(TAG, "registerIncomingConnection")
        activeConnection = connection
    }

    fun showIncomingUi() {
        NestorIncomingCall.show(context, incomingText)
    }

    fun answerIncoming() {
        Log.d(TAG, "appel entrant accepte")
        NestorIncomingCall.cancel(context)
        activeConnection?.onConnected()
        if (!isCallActive) startCallSession()
        try {
            context.startActivity(
                Intent(context, com.nestor.assistant.MainActivity::class.java)
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
            )
        } catch (e: Exception) {
            Log.w(TAG, "impossible d'amener l'application au premier plan", e)
        }
    }

    fun rejectIncoming() {
        Log.d(TAG, "appel entrant refuse")
        NestorIncomingCall.cancel(context)
        activeConnection?.let {
            it.setDisconnected(DisconnectCause(DisconnectCause.REJECTED, "Appel refuse"))
            it.destroy()
        }
        activeConnection = null
    }

    private fun startDirectCallSession() {
        Log.d(TAG, "startDirectCallSession (fallback without system telecom handle)")
        startCallSession()
    }

    private fun startCallSession() {
        isCallActive = true
        NestorCallForegroundService.start(context)

        setupAudioHardware()
        connectWebSocket()
        startLocationUpdates()

        eventListener?.onCallStateChanged("ACTIVE", "Connecté à Nestor")
    }

    fun endCall() {
        Log.d(TAG, "endCall called")
        isCallActive = false

        stopLocationUpdates()
        stopAudioEngines()
        disconnectWebSocket()

        activeConnection?.let {
            it.onDisconnect()
            activeConnection = null
        }

        releaseProximityLock()
        resetAudioMode()
        NestorCallForegroundService.stop(context)

        eventListener?.onCallStateChanged("ENDED", "Appel terminé")
    }

    fun handleConnectionDisconnected() {
        Log.d(TAG, "handleConnectionDisconnected")
        if (isCallActive) {
            endCall()
        }
    }

    fun handleConnectionFailed(reason: String) {
        Log.e(TAG, "handleConnectionFailed: $reason")
        eventListener?.onCallStateChanged("ERROR", reason)
        endCall()
    }

    fun handleAudioStateChanged(state: CallAudioState) {
        val routeStr = when (state.route) {
            CallAudioState.ROUTE_SPEAKER -> "SPEAKER"
            CallAudioState.ROUTE_BLUETOOTH -> "BLUETOOTH"
            CallAudioState.ROUTE_WIRED_HEADSET -> "WIRED_HEADSET"
            CallAudioState.ROUTE_EARPIECE -> "EARPIECE"
            else -> "UNKNOWN"
        }

        // Proximity sensor management: turn screen off when at earpiece
        if (state.route == CallAudioState.ROUTE_EARPIECE && !state.isMuted) {
            acquireProximityLock()
        } else {
            releaseProximityLock()
        }

        eventListener?.onAudioRouteChanged(routeStr, state.isMuted)
    }

    fun setSpeakerphoneOn(on: Boolean) {
        try {
            audioManager.isSpeakerphoneOn = on
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                activeConnection?.setAudioRoute(
                    if (on) CallAudioState.ROUTE_SPEAKER else CallAudioState.ROUTE_EARPIECE
                )
            }
            if (on) {
                releaseProximityLock()
            } else {
                acquireProximityLock()
            }
            eventListener?.onAudioRouteChanged(if (on) "SPEAKER" else "EARPIECE", isMicMuted)
        } catch (e: Exception) {
            Log.e(TAG, "Error setting speakerphone", e)
        }
    }

    fun muteMic(muted: Boolean) {
        isMicMuted = muted
        activeConnection?.let {
            // connection mute if supported
        }
        val currentRoute = if (audioManager.isSpeakerphoneOn) "SPEAKER" else "EARPIECE"
        eventListener?.onAudioRouteChanged(currentRoute, isMicMuted)
    }

    fun pauseAudio() {
        isMicMuted = true
    }

    fun resumeAudio() {
        isMicMuted = false
    }

    private fun acquireProximityLock() {
        try {
            if (proximityWakeLock != null && !proximityWakeLock!!.isHeld) {
                proximityWakeLock?.acquire()
            }
        } catch (e: Exception) {
            Log.w(TAG, "Failed to acquire proximity lock", e)
        }
    }

    private fun releaseProximityLock() {
        try {
            if (proximityWakeLock != null && proximityWakeLock!!.isHeld) {
                proximityWakeLock?.release()
            }
        } catch (e: Exception) {
            Log.w(TAG, "Failed to release proximity lock", e)
        }
    }

    // --- Audio Hardware Setup & Recording/Playback Engines ---

    private fun setupAudioHardware() {
        try {
            audioManager.mode = AudioManager.MODE_IN_COMMUNICATION

            // Setup AudioTrack for TTS playback (Mono PCM, recreated if the daemon announces another rate)
            audioTrack = createAudioTrack(SAMPLE_RATE_PLAYBACK)
            isPlaying.set(true)
            playbackQueue.clear()
            playbackThread = Thread({ playbackLoop() }, "Nestor-AudioTrack-Thread").apply { start() }

            if (ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO)
                != PackageManager.PERMISSION_GRANTED
            ) {
                Log.e(TAG, "Permission micro non accordee, capture audio impossible")
                return
            }

            // Setup AudioRecord for 16kHz Mono Mic capture
            val minRecBufferSize = AudioRecord.getMinBufferSize(
                SAMPLE_RATE_RECORD,
                AudioFormat.CHANNEL_IN_MONO,
                AudioFormat.ENCODING_PCM_16BIT
            )

            audioRecord = AudioRecord(
                MediaRecorder.AudioSource.VOICE_COMMUNICATION,
                SAMPLE_RATE_RECORD,
                AudioFormat.CHANNEL_IN_MONO,
                AudioFormat.ENCODING_PCM_16BIT,
                minRecBufferSize * 2
            )

            val sessionId = audioRecord?.audioSessionId ?: 0
            if (sessionId != 0) {
                if (AcousticEchoCanceler.isAvailable()) {
                    echoCanceler = AcousticEchoCanceler.create(sessionId)?.apply {
                        enabled = true
                        Log.d(TAG, "AcousticEchoCanceler enabled")
                    }
                }
                if (NoiseSuppressor.isAvailable()) {
                    noiseSuppressor = NoiseSuppressor.create(sessionId)?.apply {
                        enabled = true
                        Log.d(TAG, "NoiseSuppressor enabled")
                    }
                }
            }

            audioRecord?.startRecording()
            isRecording.set(true)

            // Start background capture thread
            recordThread = Thread({ recordLoop() }, "Nestor-AudioRecord-Thread").apply {
                priority = Thread.MAX_PRIORITY
                start()
            }

            Log.d(TAG, "Audio hardware initialized (In-Communication mode, AEC active)")
        } catch (e: Exception) {
            Log.e(TAG, "Error setting up audio hardware", e)
        }
    }

    private fun createAudioTrack(sampleRate: Int): AudioTrack {
        val minTrackBufferSize = AudioTrack.getMinBufferSize(
            sampleRate,
            AudioFormat.CHANNEL_OUT_MONO,
            AudioFormat.ENCODING_PCM_16BIT
        )

        val audioAttributes = AudioAttributes.Builder()
            .setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION)
            .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
            .build()

        val audioFormat = AudioFormat.Builder()
            .setSampleRate(sampleRate)
            .setChannelMask(AudioFormat.CHANNEL_OUT_MONO)
            .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
            .build()

        return AudioTrack.Builder()
            .setAudioAttributes(audioAttributes)
            .setAudioFormat(audioFormat)
            .setBufferSizeInBytes(minTrackBufferSize * 4)
            .setTransferMode(AudioTrack.MODE_STREAM)
            .build()
            .apply { play() }
    }

    private fun playbackLoop() {
        while (isPlaying.get()) {
            val chunk = try {
                playbackQueue.take()
            } catch (e: InterruptedException) {
                break
            }
            try {
                val gen = playbackGen.get()
                var track = audioTrack ?: continue
                // La frequence depend de la voix chargee par le daemon (22 050 Hz pour Piper).
                if (track.sampleRate != chunk.sampleRate) {
                    track.stop()
                    track.release()
                    track = createAudioTrack(chunk.sampleRate)
                    audioTrack = track
                    Log.d(TAG, "AudioTrack recreated at ${chunk.sampleRate} Hz")
                }
                // Ecriture par tranches de ~100 ms, pour qu'une interruption prenne effet aussitot.
                val sliceBytes = chunk.sampleRate / 10 * 2
                var offset = 0
                while (offset < chunk.bytes.size && isPlaying.get() && gen == playbackGen.get()) {
                    val written = track.write(chunk.bytes, offset, minOf(sliceBytes, chunk.bytes.size - offset))
                    if (written <= 0) break
                    offset += written
                }
            } catch (e: Exception) {
                Log.e(TAG, "Error playing audio bytes", e)
            }
        }
    }

    /** Vide la lecture en cours et les segments en attente (barge-in, interruption vocale). */
    private fun flushPlayback() {
        playbackEndMs = 0L
        gateBypassUntilMs = 0L
        playbackGen.incrementAndGet()
        playbackQueue.clear()
        audioTrack?.pause()
        audioTrack?.flush()
        audioTrack?.play()
    }

    private fun recordLoop() {
        val buffer = ShortArray(800) // 50ms at 16kHz = 800 samples
        val byteBuffer = ByteArray(1600)

        // Optimisation batterie : VAD locale basee sur l'energie RMS.
        // Pendant les phases de silence (90%+ du temps), aucune trame n'est envoyee sur le WebSocket,
        // ce qui permet a la puce radio (Wi-Fi / 4G / 5G) d'entrer en mode veille (DRX/low-power).
        val ENERGY_VAD_THRESHOLD = 0.012f
        val PREROLL_CHUNKS = 6 // 6 x 50ms = 300ms de pre-roll pour capturer l'attaque ("H" de "Hey")
        val HANGOVER_CHUNKS = 32 // 32 x 50ms = 1600ms : doit depasser le silence maximal cote nestord (turn.max_silence_ms 1400ms), sinon l'enonce n'est jamais cloture
        val prerollQueue = java.util.ArrayDeque<ByteArray>(PREROLL_CHUNKS)
        var inVoice = false
        var hangoverRemaining = 0

        while (isRecording.get() && isCallActive) {
            val readCount = audioRecord?.read(buffer, 0, buffer.size) ?: 0
            if (readCount > 0) {
                var sumSquares = 0.0
                var peak = 0.0f

                for (i in 0 until readCount) {
                    val sample = buffer[i]
                    val sampleFloat = sample.toFloat() / 32768.0f
                    sumSquares += (sampleFloat * sampleFloat)
                    val abs = kotlin.math.abs(sampleFloat)
                    if (abs > peak) peak = abs

                    // Convert to little-endian bytes
                    byteBuffer[i * 2] = (sample.toInt() and 0xFF).toByte()
                    byteBuffer[i * 2 + 1] = ((sample.toInt() shr 8) and 0xFF).toByte()
                }

                val rms = sqrt(sumSquares / readCount).toFloat()
                eventListener?.onAudioLevels(rms, peak)

                if (!isMicMuted && webSocket != null) {
                    val currentFrame = byteBuffer.copyOf(readCount * 2)

                    if (rms >= ENERGY_VAD_THRESHOLD || System.currentTimeMillis() < gateBypassUntilMs) {
                        if (!inVoice) {
                            inVoice = true
                            // Fin du silence : envoyer le pre-roll complet d'abord
                            while (!prerollQueue.isEmpty()) {
                                val pre = prerollQueue.pollFirst()
                                if (pre != null) {
                                    webSocket?.send(pre.toByteString())
                                }
                            }
                        }
                        hangoverRemaining = HANGOVER_CHUNKS
                        webSocket?.send(currentFrame.toByteString())
                    } else if (inVoice) {
                        if (hangoverRemaining > 0) {
                            hangoverRemaining--
                            webSocket?.send(currentFrame.toByteString())
                        } else {
                            inVoice = false
                            if (prerollQueue.size >= PREROLL_CHUNKS) {
                                prerollQueue.pollFirst()
                            }
                            prerollQueue.addLast(currentFrame)
                        }
                    } else {
                        // En silence : aucune transmission reseau -> economie batterie drastique !
                        if (prerollQueue.size >= PREROLL_CHUNKS) {
                            prerollQueue.pollFirst()
                        }
                        prerollQueue.addLast(currentFrame)
                    }
                }
            }
        }
    }

    private fun stopAudioEngines() {
        try {
            isRecording.set(false)
            isPlaying.set(false)

            recordThread?.interrupt()
            recordThread = null

            playbackThread?.interrupt()
            playbackThread = null
            playbackQueue.clear()

            audioRecord?.stop()
            audioRecord?.release()
            audioRecord = null

            echoCanceler?.release()
            echoCanceler = null
            noiseSuppressor?.release()
            noiseSuppressor = null

            audioTrack?.stop()
            audioTrack?.release()
            audioTrack = null
        } catch (e: Exception) {
            Log.e(TAG, "Error stopping audio engines", e)
        }
    }

    private fun resetAudioMode() {
        try {
            audioManager.mode = AudioManager.MODE_NORMAL
            audioManager.isSpeakerphoneOn = false
        } catch (e: Exception) {
            Log.e(TAG, "Error resetting audio mode", e)
        }
    }

    fun bargeIn() {
        try {
            // Instantly clear AudioTrack playback buffer
            flushPlayback()

            // Send barge_in JSON to daemon
            val json = JSONObject().apply {
                put("type", "barge_in")
            }
            webSocket?.send(json.toString())
            Log.d(TAG, "Barge-in dispatched")
        } catch (e: Exception) {
            Log.e(TAG, "Error during barge-in", e)
        }
    }

    fun sendTextMessage(text: String) {
        try {
            val json = JSONObject().apply {
                put("type", "send_text")
                put("content", text)
            }
            webSocket?.send(json.toString())
            eventListener?.onTranscript("user", text, false)
        } catch (e: Exception) {
            Log.e(TAG, "Error sending text message", e)
        }
    }

    fun setBackend(backend: String) {
        try {
            val json = JSONObject().apply {
                put("type", "set_backend")
                put("backend", backend)
            }
            webSocket?.send(json.toString())
            Log.d(TAG, "Set backend dispatched: $backend")
        } catch (e: Exception) {
            Log.e(TAG, "Error sending set_backend", e)
        }
    }

    /** Repond a une confirmation demandee par le juge. Retourne faux si rien n'a pu etre envoye. */
    fun resolveJudgement(id: Long, approve: Boolean): Boolean =
        sendResolution("resolve_judgement", id, approve)

    /** Repond a une demande d'accord pour une ecriture d'un connecteur externe. */
    fun resolveToolApproval(id: Long, approve: Boolean): Boolean =
        sendResolution("resolve_tool_approval", id, approve)

    private fun sendResolution(type: String, id: Long, approve: Boolean): Boolean {
        return try {
            val json = JSONObject().apply {
                put("type", type)
                put("id", id)
                put("approve", approve)
            }
            val sent = webSocket?.send(json.toString()) ?: false
            Log.d(TAG, "$type id=$id approve=$approve sent=$sent")
            sent
        } catch (e: Exception) {
            Log.e(TAG, "Error sending $type", e)
            false
        }
    }

    // --- Localisation (reconnaissance de lieu cote nestord) ---

    private fun hasLocationPermission(): Boolean {
        return ContextCompat.checkSelfPermission(
            context, Manifest.permission.ACCESS_COARSE_LOCATION
        ) == PackageManager.PERMISSION_GRANTED
    }

    private fun startLocationUpdates() {
        if (!hasLocationPermission()) {
            Log.w(TAG, "Permission de localisation non accordee, position non envoyee")
            return
        }

        val listener = LocationListener { location -> sendLocation(location) }
        locationListener = listener

        try {
            val provider = when {
                locationManager.isProviderEnabled(LocationManager.NETWORK_PROVIDER) -> LocationManager.NETWORK_PROVIDER
                locationManager.isProviderEnabled(LocationManager.GPS_PROVIDER) -> LocationManager.GPS_PROVIDER
                else -> null
            }
            if (provider == null) {
                Log.w(TAG, "Aucun fournisseur de localisation disponible")
                return
            }

            // Cadence large : la reconnaissance de lieu n'a pas besoin de temps reel.
            locationManager.requestLocationUpdates(provider, 60_000L, 50f, listener, Looper.getMainLooper())
            locationManager.getLastKnownLocation(provider)?.let { sendLocation(it) }
        } catch (e: SecurityException) {
            Log.e(TAG, "Permission de localisation refusee au moment de l'appel", e)
        } catch (e: Exception) {
            Log.e(TAG, "Erreur lors du demarrage de la localisation", e)
        }
    }

    private fun stopLocationUpdates() {
        locationListener?.let {
            try {
                locationManager.removeUpdates(it)
            } catch (e: Exception) {
                Log.w(TAG, "Erreur lors de l'arret de la localisation", e)
            }
        }
        locationListener = null
    }

    private fun sendLocation(location: Location) {
        try {
            val json = JSONObject().apply {
                put("type", "location")
                put("lat", location.latitude)
                put("lon", location.longitude)
            }
            webSocket?.send(json.toString())
        } catch (e: Exception) {
            Log.e(TAG, "Erreur lors de l'envoi de la position", e)
        }
    }

    // --- WebSocket Connection ---

    private fun connectWebSocket() {
        try {
            val request = Request.Builder().url(currentServerUrl).build()
            webSocket = okHttpClient.newWebSocket(request, object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) {
                    Log.d(TAG, "WebSocket connected to $currentServerUrl")
                    // Announce remote audio client
                    val hello = JSONObject().apply {
                        put("type", "client_hello")
                        put("client", "nestor-android-call")
                        put("audio_input", "stream_16k_pcm")
                        put("audio_output", "stream_pcm")
                    }
                    webSocket.send(hello.toString())
                }

                override fun onMessage(webSocket: WebSocket, text: String) {
                    handleJsonMessage(text)
                }

                override fun onMessage(webSocket: WebSocket, bytes: ByteString) {
                    // Raw incoming PCM audio chunk from Kokoro / daemon
                    playAudioBytes(bytes.toByteArray())
                }

                override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
                    Log.d(TAG, "WebSocket closing: $code $reason")
                    // Sans cet accuse de reception, OkHttp n'appelle jamais onClosed.
                    webSocket.close(1000, null)
                }

                override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                    Log.d(TAG, "WebSocket closed: $code $reason")
                    // Un socket deja remplace ou ferme par endCall() ne doit pas couper l'appel courant.
                    if (webSocket !== this@NestorCallManager.webSocket) return
                    if (isCallActive) {
                        endCall()
                    }
                }

                override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                    Log.e(TAG, "WebSocket failure", t)
                    if (webSocket !== this@NestorCallManager.webSocket) return
                    // L'appel est termine : sans cela il resterait actif sans serveur, et
                    // startCall() refuserait toute nouvelle tentative.
                    handleConnectionFailed(
                        if (response != null) "Connexion refusée par le serveur (HTTP ${response.code})"
                        else "Connexion perdue avec le serveur"
                    )
                }
            })
        } catch (e: Exception) {
            Log.e(TAG, "Error opening WebSocket", e)
            eventListener?.onCallStateChanged("ERROR", "Impossible de joindre le serveur: ${e.message}")
        }
    }

    private fun disconnectWebSocket() {
        try {
            webSocket?.close(1000, "Call ended")
            webSocket = null
        } catch (e: Exception) {
            Log.e(TAG, "Error closing WebSocket", e)
        }
    }

    private fun handleJsonMessage(text: String) {
        try {
            val obj = JSONObject(text)
            val type = obj.optString("type")

            when (type) {
                "state" -> {
                    val state = if (obj.has("status")) obj.getString("status") else obj.optString("state", "idle")
                    eventListener?.onNestorStateChanged(state)
                }
                "wake_state" -> {
                    val active = obj.optBoolean("active", false)
                    val state = if (active) "listening" else "idle"
                    eventListener?.onNestorStateChanged(state)
                }
                "audio_levels" -> {
                    val rms = obj.optDouble("rms", 0.0).toFloat()
                    val peak = obj.optDouble("peak", 0.0).toFloat()
                    eventListener?.onAudioLevels(rms, peak)
                }
                "transcript" -> {
                    val role = obj.optString("role", "assistant")
                    val transcriptText = obj.optString("text", "")
                    // Le daemon envoie le texte cumule de la reponse, avec `is_final` faux tant qu'elle s'ecrit.
                    val isPartial = !obj.optBoolean("is_final", true)
                    eventListener?.onTranscript(role, transcriptText, isPartial)
                }
                "tool_call" -> {
                    val id = obj.optString("id", "")
                    val name = obj.optString("name", "")
                    val status = obj.optString("status", "running")
                    eventListener?.onToolCall(id, name, status)
                }
                "interrupt" -> {
                    // Interruption vocale detectee par le daemon : on vide la lecture en cours.
                    flushPlayback()
                    val rms = obj.optDouble("rms", Double.NaN)
                    eventListener?.onInterrupt(if (rms.isNaN()) null else rms.toFloat())
                }
                "judge_verdict" -> {
                    eventListener?.onJudgeVerdict(
                        id = obj.optLong("id"),
                        source = obj.optString("source", "message"),
                        text = obj.optString("text", ""),
                        decision = obj.optString("decision", "confirm"),
                        score = if (obj.has("score") && !obj.isNull("score")) obj.optInt("score") else null,
                        category = optStringOrNull(obj, "category"),
                        rationale = optStringOrNull(obj, "rationale"),
                        pending = obj.optBoolean("pending", false),
                        atMs = obj.optLong("at_ms", System.currentTimeMillis()),
                    )
                }
                "judge_resolved" -> {
                    eventListener?.onJudgeResolved(obj.optLong("id"), obj.optBoolean("approved", false))
                }
                "tool_approval" -> {
                    eventListener?.onToolApproval(
                        id = obj.optLong("id"),
                        server = obj.optString("server", ""),
                        tool = obj.optString("tool", ""),
                        arguments = obj.optString("arguments", ""),
                        atMs = obj.optLong("at_ms", System.currentTimeMillis()),
                    )
                }
                "tool_approval_resolved" -> {
                    eventListener?.onToolApprovalResolved(obj.optLong("id"), obj.optBoolean("approved", false))
                }
                "audio_chunk" -> {
                    // Base64 encoded audio
                    val b64 = obj.optString("data", "")
                    if (b64.isNotEmpty()) {
                        val pcmBytes = Base64.decode(b64, Base64.DEFAULT)
                        val rate = obj.optInt("sample_rate", 22050).coerceAtLeast(1)
                        val chunkMs = pcmBytes.size / 2 * 1000L / rate
                        playbackEndMs = maxOf(playbackEndMs, System.currentTimeMillis()) + chunkMs
                        gateBypassUntilMs = playbackEndMs + 1500
                        playAudioBytes(pcmBytes, rate)
                    }
                }
                "backend_status" -> {
                    val activeBackend = obj.optString("active_backend", "claude")
                    val isFallback = obj.optBoolean("is_fallback", false)
                    val reason = if (obj.has("reason") && !obj.isNull("reason")) obj.getString("reason") else null
                    eventListener?.onBackendStatus(activeBackend, isFallback, reason)
                }
            }
        } catch (e: Exception) {
            Log.e(TAG, "Error parsing WebSocket message", e)
        }
    }

    private fun optStringOrNull(obj: JSONObject, key: String): String? =
        if (obj.has(key) && !obj.isNull(key)) obj.optString(key) else null

    private fun playAudioBytes(bytes: ByteArray, sampleRate: Int = SAMPLE_RATE_PLAYBACK) {
        if (!isPlaying.get() || audioTrack == null) return
        playbackQueue.offer(PcmChunk(bytes, sampleRate))
    }
}
