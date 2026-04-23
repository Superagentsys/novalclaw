package com.omninova.phoneagent.data

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * 与 `skills/phone-call-assistant/conversation_log_schema.json` 对齐。
 */
@Serializable
enum class ConversationChannel {
    @SerialName("voip_callkit") VOIP_CALLKIT,
    @SerialName("in_app_voice") IN_APP_VOICE,
    @SerialName("simulated") SIMULATED,
    @SerialName("cellular_telecom") CELLULAR_TELECOM,
    @SerialName("unknown") UNKNOWN,
}

@Serializable
data class ConversationTurn(
    val t: String,
    val role: String,
    val text: String,
    val confidence: Double? = null,
    @SerialName("raw_note") val rawNote: String? = null,
)

@Serializable
data class ConversationSessionFile(
    @SerialName("schema_version") val schemaVersion: String = "1.0",
    @SerialName("session_id") val sessionId: String,
    val channel: ConversationChannel,
    @SerialName("started_at_utc") val startedAtUtc: String,
    @SerialName("ended_at_utc") var endedAtUtc: String? = null,
    val locale: String? = null,
    val turns: MutableList<ConversationTurn> = mutableListOf(),
    var metadata: MutableMap<String, String>? = null,
)
