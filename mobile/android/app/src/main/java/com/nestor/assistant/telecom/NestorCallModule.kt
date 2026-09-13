package com.nestor.assistant.telecom

import android.Manifest
import android.content.pm.PackageManager
import com.facebook.react.bridge.Arguments
import com.facebook.react.bridge.Promise
import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.bridge.ReactContextBaseJavaModule
import com.facebook.react.bridge.ReactMethod
import com.facebook.react.bridge.ReadableMap
import com.facebook.react.bridge.WritableMap
import com.facebook.react.modules.core.DeviceEventManagerModule
import com.facebook.react.modules.core.PermissionAwareActivity
import com.facebook.react.modules.core.PermissionListener

class NestorCallModule(private val reactContext: ReactApplicationContext) :
    ReactContextBaseJavaModule(reactContext),
    NestorCallManager.CallEventListener {

    companion object {
        const val MODULE_NAME = "NestorCallModule"
        private const val PERMISSIONS_REQUEST_CODE = 4242
    }

    private val callManager = NestorCallManager.getInstance(reactContext)

    init {
        callManager.eventListener = this
    }

    override fun getName(): String = MODULE_NAME

    /**
     * Demande RECORD_AUDIO (indispensable a l'appel) et la localisation
     * (reconnaissance de lieu, best-effort) en une seule fois. Resout avec
     * `true` si RECORD_AUDIO est accorde ; la localisation est facultative,
     * un refus n'empeche pas l'appel de demarrer.
     */
    @ReactMethod
    fun requestPermissions(promise: Promise) {
        val activity = reactApplicationContext.getCurrentActivity()
        if (activity !is PermissionAwareActivity) {
            promise.reject("NO_ACTIVITY", "Activite indisponible pour demander les permissions")
            return
        }

        val permissions = arrayOf(
            Manifest.permission.RECORD_AUDIO,
            Manifest.permission.ACCESS_COARSE_LOCATION,
            Manifest.permission.ACCESS_FINE_LOCATION,
        )

        val alreadyGranted = permissions.all {
            reactContext.checkSelfPermission(it) == PackageManager.PERMISSION_GRANTED
        }
        if (alreadyGranted) {
            promise.resolve(true)
            return
        }

        val listener = PermissionListener { requestCode, _, grantResults ->
            if (requestCode != PERMISSIONS_REQUEST_CODE) return@PermissionListener false
            val micGranted = grantResults.isNotEmpty() &&
                grantResults[0] == PackageManager.PERMISSION_GRANTED
            promise.resolve(micGranted)
            true
        }

        activity.requestPermissions(permissions, PERMISSIONS_REQUEST_CODE, listener)
    }

    @ReactMethod
    fun startCall(options: ReadableMap?, promise: Promise) {
        try {
            val serverUrl = if (options != null && options.hasKey("serverUrl")) {
                options.getString("serverUrl") ?: "ws://10.0.2.2:8340/ws"
            } else {
                "ws://10.0.2.2:8340/ws"
            }
            callManager.startCall(serverUrl)
            promise.resolve(true)
        } catch (e: Exception) {
            promise.reject("START_CALL_ERROR", e.message, e)
        }
    }

    @ReactMethod
    fun endCall(promise: Promise) {
        try {
            callManager.endCall()
            promise.resolve(true)
        } catch (e: Exception) {
            promise.reject("END_CALL_ERROR", e.message, e)
        }
    }

    @ReactMethod
    fun setMuted(muted: Boolean, promise: Promise) {
        try {
            callManager.muteMic(muted)
            promise.resolve(muted)
        } catch (e: Exception) {
            promise.reject("MUTE_ERROR", e.message, e)
        }
    }

    @ReactMethod
    fun setSpeakerphoneOn(speakerOn: Boolean, promise: Promise) {
        try {
            callManager.setSpeakerphoneOn(speakerOn)
            promise.resolve(speakerOn)
        } catch (e: Exception) {
            promise.reject("SPEAKER_ERROR", e.message, e)
        }
    }

    @ReactMethod
    fun bargeIn(promise: Promise) {
        try {
            callManager.bargeIn()
            promise.resolve(true)
        } catch (e: Exception) {
            promise.reject("BARGE_IN_ERROR", e.message, e)
        }
    }

    @ReactMethod
    fun sendTextMessage(text: String, promise: Promise) {
        try {
            callManager.sendTextMessage(text)
            promise.resolve(true)
        } catch (e: Exception) {
            promise.reject("TEXT_MESSAGE_ERROR", e.message, e)
        }
    }

    @ReactMethod
    fun setBackend(backend: String, promise: Promise) {
        try {
            callManager.setBackend(backend)
            promise.resolve(true)
        } catch (e: Exception) {
            promise.reject("SET_BACKEND_ERROR", e.message, e)
        }
    }

    @ReactMethod
    fun getCallState(promise: Promise) {
        try {
            val map = Arguments.createMap().apply {
                putBoolean("isCallActive", callManager.isCallActive)
                putBoolean("isMicMuted", callManager.isMicMuted)
                putString("serverUrl", callManager.currentServerUrl)
            }
            promise.resolve(map)
        } catch (e: Exception) {
            promise.reject("GET_STATE_ERROR", e.message, e)
        }
    }

    // --- CallEventListener implementation forwarding to React Native ---

    override fun onCallStateChanged(state: String, details: String?) {
        val params = Arguments.createMap().apply {
            putString("state", state)
            if (details != null) putString("details", details)
        }
        sendEvent("onCallStateChanged", params)
    }

    override fun onNestorStateChanged(state: String) {
        val params = Arguments.createMap().apply {
            putString("state", state)
        }
        sendEvent("onNestorStateChanged", params)
    }

    override fun onAudioRouteChanged(route: String, isMuted: Boolean) {
        val params = Arguments.createMap().apply {
            putString("route", route)
            putBoolean("isMuted", isMuted)
        }
        sendEvent("onAudioRouteChanged", params)
    }

    override fun onAudioLevels(rms: Float, peak: Float) {
        val params = Arguments.createMap().apply {
            putDouble("rms", rms.toDouble())
            putDouble("peak", peak.toDouble())
        }
        sendEvent("onAudioLevels", params)
    }

    override fun onTranscript(role: String, text: String, isPartial: Boolean) {
        val params = Arguments.createMap().apply {
            putString("role", role)
            putString("text", text)
            putBoolean("isPartial", isPartial)
        }
        sendEvent("onTranscript", params)
    }

    override fun onToolCall(id: String, name: String, status: String) {
        val params = Arguments.createMap().apply {
            putString("id", id)
            putString("name", name)
            putString("status", status)
        }
        sendEvent("onToolCall", params)
    }

    override fun onBackendStatus(activeBackend: String, isFallback: Boolean, reason: String?) {
        val params = Arguments.createMap().apply {
            putString("active_backend", activeBackend)
            putBoolean("is_fallback", isFallback)
            if (reason != null) putString("reason", reason)
        }
        sendEvent("onBackendStatusChanged", params)
    }

    private fun sendEvent(eventName: String, params: WritableMap?) {
        if (reactContext.hasActiveReactInstance()) {
            reactContext
                .getJSModule(DeviceEventManagerModule.RCTDeviceEventEmitter::class.java)
                .emit(eventName, params)
        }
    }

    // Required for React Native new architecture and event listeners
    @ReactMethod
    fun addListener(eventName: String) {}

    @ReactMethod
    fun removeListeners(count: Double) {}
}
