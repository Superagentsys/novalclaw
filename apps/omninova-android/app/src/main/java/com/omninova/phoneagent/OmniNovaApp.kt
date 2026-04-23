package com.omninova.phoneagent

import android.app.Application
import android.app.NotificationChannel
import android.app.NotificationManager
import android.os.Build
import com.omninova.phoneagent.call.AgentResponseSynthesizer
import com.omninova.phoneagent.call.KeyInfoExtractor
import com.omninova.phoneagent.call.SpamDetector
import com.omninova.phoneagent.call.SpeechPipeline
import com.omninova.phoneagent.data.ConversationLogStore
import com.omninova.phoneagent.net.GatewayClient
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch

/**
 * 全局依赖容器。可逐步迁移到 Hilt/Koin；当前保持零依赖以便快速编译。
 */
class OmniNovaApp : Application() {

    lateinit var logStore: ConversationLogStore
    lateinit var gateway: GatewayClient
    lateinit var spamDetector: SpamDetector
    lateinit var speech: SpeechPipeline
    lateinit var tts: AgentResponseSynthesizer
    lateinit var extractor: KeyInfoExtractor
    val appScope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    override fun onCreate() {
        super.onCreate()
        INSTANCE = this
        logStore = ConversationLogStore(this)
        gateway = GatewayClient()
        extractor = KeyInfoExtractor()
        spamDetector = SpamDetector(initialJson = assetJson("spam_detection_rules.json"))
        speech = SpeechPipeline(this)
        tts = AgentResponseSynthesizer(this)

        createNotificationChannel()

        appScope.launch {
            gateway.fetchSpamRules()?.let { spamDetector.updateRules(it) }
        }
    }

    private fun createNotificationChannel() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val manager = getSystemService(NotificationManager::class.java) ?: return
        val channel = NotificationChannel(
            CALL_CHANNEL_ID,
            getString(R.string.notif_channel_call),
            NotificationManager.IMPORTANCE_LOW,
        ).apply {
            description = getString(R.string.notif_channel_call_desc)
            setShowBadge(false)
        }
        manager.createNotificationChannel(channel)
    }

    private fun assetJson(name: String): String? = runCatching {
        assets.open(name).bufferedReader().use { it.readText() }
    }.getOrNull()

    companion object {
        const val CALL_CHANNEL_ID = "omninova.call"
        lateinit var INSTANCE: OmniNovaApp
            private set
    }
}
