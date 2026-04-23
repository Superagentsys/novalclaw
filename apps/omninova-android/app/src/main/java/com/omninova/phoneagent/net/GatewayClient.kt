package com.omninova.phoneagent.net

import com.omninova.phoneagent.data.ConversationSessionFile
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.withContext
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import java.util.concurrent.TimeUnit

/**
 * OmniNova 网关客户端。
 * 与 iOS `AgentGatewayClient` 同形。
 */
class GatewayClient(baseUrl: String = "http://127.0.0.1:10809") {
    private val json = Json { ignoreUnknownKeys = true; encodeDefaults = true }
    private val client = OkHttpClient.Builder()
        .connectTimeout(10, TimeUnit.SECONDS)
        .readTimeout(60, TimeUnit.SECONDS)
        .callTimeout(90, TimeUnit.SECONDS)
        .build()

    private val _baseUrl = MutableStateFlow(normalize(baseUrl))
    val baseUrl: StateFlow<String> = _baseUrl.asStateFlow()

    private val _connected = MutableStateFlow(false)
    val connected: StateFlow<Boolean> = _connected.asStateFlow()

    fun configure(url: String) {
        _baseUrl.value = normalize(url)
    }

    private fun normalize(url: String): String = url.trim().trimEnd('/')

    suspend fun checkConnection(): Boolean = withContext(Dispatchers.IO) {
        val ok = runCatching {
            val req = Request.Builder().url("${_baseUrl.value}/api/health").build()
            client.newCall(req).execute().use { it.isSuccessful }
        }.getOrDefault(false)
        _connected.value = ok
        ok
    }

    /** 发送对话到网关，返回 Agent 回复文本。 */
    suspend fun chat(
        text: String,
        sessionId: String,
        channel: String = "cellular_telecom",
    ): String? = withContext(Dispatchers.IO) {
        runCatching {
            val payload: JsonObject = buildJsonObject {
                put("channel", channel)
                put("text", text)
                put("session_id", sessionId)
                put("user_id", "android-phone-agent")
            }
            val req = Request.Builder()
                .url("${_baseUrl.value}/api/inbound")
                .post(
                    json.encodeToString(JsonObject.serializer(), payload)
                        .toRequestBody("application/json".toMediaType())
                )
                .build()
            client.newCall(req).execute().use { resp ->
                val body = resp.body?.string() ?: return@use null
                val obj = json.parseToJsonElement(body).let { it as? JsonObject } ?: return@use null
                (obj["reply"]?.toString()?.trim('"'))
            }
        }.getOrNull()
    }

    suspend fun syncSession(session: ConversationSessionFile): Boolean = withContext(Dispatchers.IO) {
        runCatching {
            val body = json.encodeToString(session)
            val wrapped = """{"type":"conversation_sync","session":$body}"""
            val req = Request.Builder()
                .url("${_baseUrl.value}/api/webhook")
                .header("X-OmniNova-Event", "conversation_sync")
                .post(wrapped.toRequestBody("application/json".toMediaType()))
                .build()
            client.newCall(req).execute().use { it.isSuccessful }
        }.getOrDefault(false)
    }

    suspend fun extractKeyInfo(sessionId: String): String? = withContext(Dispatchers.IO) {
        runCatching {
            val payload = """{"session_id":"$sessionId"}"""
            val req = Request.Builder()
                .url("${_baseUrl.value}/api/skill/phone-call-assistant/extract")
                .post(payload.toRequestBody("application/json".toMediaType()))
                .build()
            client.newCall(req).execute().use { it.body?.string() }
        }.getOrNull()
    }

    suspend fun fetchSpamRules(): String? = withContext(Dispatchers.IO) {
        runCatching {
            val req = Request.Builder()
                .url("${_baseUrl.value}/api/skill/phone-call-assistant/rules")
                .build()
            client.newCall(req).execute().use {
                if (it.isSuccessful) it.body?.string() else null
            }
        }.getOrNull()
    }
}
