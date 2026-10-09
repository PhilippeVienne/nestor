package com.nestor.assistant.telecom

import android.net.Uri
import android.os.Build
import android.telecom.CallAudioState
import android.telecom.Connection
import android.telecom.DisconnectCause
import android.telecom.TelecomManager
import android.util.Log

class NestorConnection(
    private val callManager: NestorCallManager
) : Connection() {

    companion object {
        private const val TAG = "NestorConnection"
    }

    init {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            connectionProperties = PROPERTY_SELF_MANAGED
        }
        audioModeIsVoip = true
        setCallerDisplayName("Nestor", TelecomManager.PRESENTATION_ALLOWED)
        setAddress(Uri.parse("tel:nestor"), TelecomManager.PRESENTATION_ALLOWED)
        setInitializing()
    }

    fun onConnected() {
        Log.d(TAG, "NestorConnection active")
        setActive()
    }

    override fun onCallAudioStateChanged(state: CallAudioState?) {
        super.onCallAudioStateChanged(state)
        if (state == null) return
        Log.d(TAG, "onCallAudioStateChanged: route=${state.route}, isMuted=${state.isMuted}")
        callManager.handleAudioStateChanged(state)
    }

    override fun onDisconnect() {
        Log.d(TAG, "onDisconnect requested by user or system")
        setDisconnected(DisconnectCause(DisconnectCause.LOCAL, "User ended call"))
        destroy()
        callManager.handleConnectionDisconnected()
    }

    override fun onAbort() {
        Log.d(TAG, "onAbort")
        setDisconnected(DisconnectCause(DisconnectCause.CANCELED, "Call aborted"))
        destroy()
        callManager.handleConnectionDisconnected()
    }

    override fun onHold() {
        Log.d(TAG, "onHold")
        setOnHold()
        callManager.pauseAudio()
    }

    override fun onUnhold() {
        Log.d(TAG, "onUnhold")
        setActive()
        callManager.resumeAudio()
    }

    override fun onSilence() {
        Log.d(TAG, "onSilence")
        callManager.muteMic(true)
    }
}
