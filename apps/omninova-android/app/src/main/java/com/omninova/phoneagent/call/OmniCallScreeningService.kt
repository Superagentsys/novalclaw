package com.omninova.phoneagent.call

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Intent
import android.os.Build
import android.telecom.Call
import android.telecom.CallScreeningService
import androidx.core.app.NotificationCompat
import com.omninova.phoneagent.OmniNovaApp
import com.omninova.phoneagent.R
import com.omninova.phoneagent.ui.MainActivity

/**
 * Android 9+ `CallScreeningService`：在振铃前决策 allow / silence / reject。
 * 需要被系统或用户设为默认 screener。
 */
class OmniCallScreeningService : CallScreeningService() {

    override fun onScreenCall(callDetails: Call.Details) {
        val app = applicationContext as OmniNovaApp
        val phone = callDetails.handle?.schemeSpecificPart
        val decision = app.spamDetector.evaluate(
            phoneNumber = phone,
            isInContacts = false,
            firstUtterance = null,
        )
        val response = CallResponse.Builder()
        when (decision.severity) {
            SpamDetector.Severity.block -> {
                response.setDisallowCall(true)
                response.setRejectCall(true)
                response.setSkipCallLog(false)
                response.setSkipNotification(false)
                notifyBlocked(phone, decision)
            }
            SpamDetector.Severity.silence -> {
                response.setSilenceCall(true)
            }
            SpamDetector.Severity.allow -> { /* 默认即 allow */ }
        }
        respondToCall(callDetails, response.build())
    }

    private fun notifyBlocked(phone: String?, decision: SpamDetector.Decision) {
        val nm = getSystemService(NotificationManager::class.java) ?: return
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            nm.createNotificationChannel(
                NotificationChannel(
                    OmniNovaApp.CALL_CHANNEL_ID,
                    getString(R.string.notif_channel_call),
                    NotificationManager.IMPORTANCE_LOW,
                )
            )
        }
        val intent = PendingIntent.getActivity(
            this, 0, Intent(this, MainActivity::class.java),
            PendingIntent.FLAG_IMMUTABLE,
        )
        val notif = NotificationCompat.Builder(this, OmniNovaApp.CALL_CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(getString(R.string.notif_title_screening))
            .setContentText(buildString {
                append(phone ?: "未知号码")
                decision.reason?.let { append(" · $it") }
            })
            .setContentIntent(intent)
            .setAutoCancel(true)
            .build()
        nm.notify((phone?.hashCode() ?: 0) or 0x40000000, notif)
    }
}
