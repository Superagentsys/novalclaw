package com.omninova.phoneagent.call

import android.content.Intent
import android.telecom.Call
import android.telecom.InCallService
import android.telecom.VideoProfile
import com.omninova.phoneagent.OmniNovaApp
import com.omninova.phoneagent.data.ConversationChannel
import java.util.UUID

/**
 * 作为默认拨号器时才会收到 Call。自动接听需要 `ANSWER_PHONE_CALLS` 权限 + 默认 dialer。
 * 不能用于拦截他人拨号器的来电。
 */
class OmniInCallService : InCallService() {

    var autoAnswerEnabled: Boolean = true

    private val callCallback = object : Call.Callback() {
        override fun onStateChanged(call: Call, state: Int) {
            super.onStateChanged(call, state)
            if (state == Call.STATE_ACTIVE) {
                val sessionId = call.details.telecomCallId ?: UUID.randomUUID().toString()
                startForegroundAgent(sessionId)
            } else if (state == Call.STATE_DISCONNECTED) {
                stopForegroundAgent()
            }
        }
    }

    override fun onCallAdded(call: Call) {
        super.onCallAdded(call)
        call.registerCallback(callCallback)
        if (autoAnswerEnabled && call.state == Call.STATE_RINGING) {
            call.answer(VideoProfile.STATE_AUDIO_ONLY)
        }
    }

    override fun onCallRemoved(call: Call) {
        call.unregisterCallback(callCallback)
        super.onCallRemoved(call)
        stopForegroundAgent()
    }

    private fun startForegroundAgent(sessionId: String) {
        val app = applicationContext as OmniNovaApp
        app.logStore.startSession(sessionId, ConversationChannel.CELLULAR_TELECOM)
        val intent = Intent(this, CallAgentForegroundService::class.java).apply {
            action = CallAgentForegroundService.ACTION_START
            putExtra(CallAgentForegroundService.EXTRA_SESSION_ID, sessionId)
        }
        startForegroundService(intent)
    }

    private fun stopForegroundAgent() {
        val intent = Intent(this, CallAgentForegroundService::class.java).apply {
            action = CallAgentForegroundService.ACTION_STOP
        }
        startService(intent)
    }
}
