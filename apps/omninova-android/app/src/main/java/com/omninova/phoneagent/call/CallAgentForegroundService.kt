package com.omninova.phoneagent.call

import android.app.Notification
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import com.omninova.phoneagent.OmniNovaApp
import com.omninova.phoneagent.R
import com.omninova.phoneagent.ui.MainActivity
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

/** Runs phone speech, the local agent, and TTS while a call is active. */
class CallAgentForegroundService : Service() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private var sessionId: String? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_START -> {
                sessionId = intent.getStringExtra(EXTRA_SESSION_ID)
                startForegroundWithNotification()
                sessionId?.let(::startDialogLoop)
            }
            ACTION_STOP -> stopSelfSafely()
        }
        return START_NOT_STICKY
    }

    private fun startForegroundWithNotification() {
        val openApp = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java),
            PendingIntent.FLAG_IMMUTABLE,
        )
        val notification: Notification = NotificationCompat.Builder(this, OmniNovaApp.CALL_CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(getString(R.string.notif_title_in_call))
            .setContentText(getString(R.string.notif_text_in_call))
            .setContentIntent(openApp)
            .setOngoing(true)
            .build()

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            startForeground(
                NOTIF_ID,
                notification,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_PHONE_CALL or
                    ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE,
            )
        } else {
            startForeground(NOTIF_ID, notification)
        }
    }

    private fun startDialogLoop(sessionId: String) {
        val app = applicationContext as OmniNovaApp
        var hasConversationContent = false
        app.speech.start(
            onPartial = { partial ->
                hasConversationContent = true
                app.logStore.appendTurn(sessionId, "caller", partial, isFinal = false)
            },
            onFinal = { transcript ->
                hasConversationContent = true
                app.logStore.appendTurn(sessionId, "caller", transcript, isFinal = true)
                scope.launch {
                    val reply = app.localAgent.reply(transcript, app.settings.languageTag.value)
                    app.logStore.appendTurn(sessionId, "agent", reply, isFinal = true)
                    app.tts.speak(reply)
                }
            },
            onError = { message ->
                if (hasConversationContent) {
                    app.logStore.updateMetadata(sessionId, mapOf("speech_error" to message))
                } else {
                    app.logStore.discardSession(sessionId)
                }
            },
        )
    }

    private fun stopSelfSafely() {
        val app = applicationContext as OmniNovaApp
        app.speech.stop()
        app.tts.stop()
        sessionId?.let { id ->
            app.logStore.endSession(id)
            scope.launch {
                app.logStore.session(id)?.let { session ->
                    val result = app.extractor.extract(session)
                    app.logStore.updateMetadata(
                        id,
                        mapOf(
                            "processing" to "on_device",
                            "extracted_intent" to result.callIntent,
                            "extracted_summary" to (result.summary ?: ""),
                            "extracted_sentiment" to result.sentiment,
                        ),
                    )
                }
            }
        }
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    companion object {
        const val ACTION_START = "com.omninova.phoneagent.action.START"
        const val ACTION_STOP = "com.omninova.phoneagent.action.STOP"
        const val EXTRA_SESSION_ID = "session_id"
        const val NOTIF_ID = 0x4F4D4E00
    }
}
