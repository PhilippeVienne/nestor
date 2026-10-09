package com.nestor.assistant.telecom

import android.telecom.Connection
import android.telecom.ConnectionRequest
import android.telecom.ConnectionService
import android.telecom.DisconnectCause
import android.telecom.PhoneAccountHandle
import android.util.Log

class NestorConnectionService : ConnectionService() {

    companion object {
        private const val TAG = "NestorConnService"
    }

    override fun onCreateOutgoingConnection(
        connectionManagerPhoneAccount: PhoneAccountHandle?,
        request: ConnectionRequest?
    ): Connection {
        Log.d(TAG, "onCreateOutgoingConnection called")
        val callManager = NestorCallManager.getInstance(applicationContext)
        val connection = NestorConnection(callManager)
        connection.setDialing()
        callManager.registerActiveConnection(connection)
        return connection
    }

    override fun onCreateOutgoingConnectionFailed(
        connectionManagerPhoneAccount: PhoneAccountHandle?,
        request: ConnectionRequest?
    ) {
        Log.e(TAG, "onCreateOutgoingConnectionFailed")
        super.onCreateOutgoingConnectionFailed(connectionManagerPhoneAccount, request)
        NestorCallManager.getInstance(applicationContext).handleConnectionFailed("Failed to create outgoing connection")
    }

    override fun onCreateIncomingConnection(
        connectionManagerPhoneAccount: PhoneAccountHandle?,
        request: ConnectionRequest?
    ): Connection {
        Log.d(TAG, "onCreateIncomingConnection called")
        val callManager = NestorCallManager.getInstance(applicationContext)
        val connection = NestorConnection(callManager)
        connection.setRinging()
        callManager.registerIncomingConnection(connection)
        return connection
    }

    override fun onCreateIncomingConnectionFailed(
        connectionManagerPhoneAccount: PhoneAccountHandle?,
        request: ConnectionRequest?
    ) {
        Log.e(TAG, "onCreateIncomingConnectionFailed")
        super.onCreateIncomingConnectionFailed(connectionManagerPhoneAccount, request)
        NestorCallManager.getInstance(applicationContext).handleConnectionFailed("Failed to create incoming connection")
    }
}
