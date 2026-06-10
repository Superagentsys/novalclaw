package com.omninova.phoneagent.data

import android.content.Context
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import java.io.File
import java.time.Instant

/**
 * 会话落盘与内存缓存。落盘位置：`filesDir/conversations/<sessionId>.json`。
 */
class ConversationLogStore(private val context: Context) {
    private val json = Json {
        prettyPrint = true
        encodeDefaults = true
        ignoreUnknownKeys = true
    }

    private val _sessions = MutableStateFlow<List<ConversationSessionFile>>(emptyList())
    val sessions: StateFlow<List<ConversationSessionFile>> = _sessions.asStateFlow()

    init {
        loadFromDisk()
    }

    private fun dir(): File {
        val d = File(context.filesDir, "conversations")
        if (!d.exists()) d.mkdirs()
        return d
    }

    private fun fileFor(sessionId: String) = File(dir(), "$sessionId.json")

    fun startSession(sessionId: String, channel: ConversationChannel) {
        val session = ConversationSessionFile(
            sessionId = sessionId,
            channel = channel,
            startedAtUtc = Instant.now().toString(),
            locale = "zh-CN",
            metadata = mutableMapOf("app" to "OmniNovaPhoneAgentAndroid"),
        )
        _sessions.update { it + session }
        persist(session)
    }

    fun appendTurn(sessionId: String, role: String, text: String, isFinal: Boolean) {
        _sessions.update { list ->
            val idx = list.indexOfFirst { it.sessionId == sessionId }
            if (idx < 0) return@update list
            val session = list[idx]
            val turn = ConversationTurn(
                t = Instant.now().toString(),
                role = role,
                text = text,
                confidence = if (isFinal) 1.0 else null,
            )
            if (!isFinal) {
                val lastNonFinal = session.turns.indexOfLast { it.role == role && it.confidence == null }
                if (lastNonFinal >= 0) {
                    session.turns[lastNonFinal] = turn
                } else {
                    session.turns.add(turn)
                }
            } else {
                session.turns.add(turn)
            }
            persist(session)
            list
        }
    }

    fun endSession(sessionId: String) {
        _sessions.update { list ->
            val idx = list.indexOfFirst { it.sessionId == sessionId }
            if (idx < 0) return@update list
            list[idx].endedAtUtc = Instant.now().toString()
            persist(list[idx])
            list
        }
    }

    fun updateMetadata(sessionId: String, entries: Map<String, String>) {
        _sessions.update { list ->
            val idx = list.indexOfFirst { it.sessionId == sessionId }
            if (idx < 0) return@update list
            val m = list[idx].metadata ?: mutableMapOf()
            m.putAll(entries)
            list[idx].metadata = m
            persist(list[idx])
            list
        }
    }

    fun session(sessionId: String): ConversationSessionFile? =
        _sessions.value.firstOrNull { it.sessionId == sessionId }

    private fun persist(session: ConversationSessionFile) {
        runCatching {
            fileFor(session.sessionId).writeText(json.encodeToString(session))
        }
    }

    private fun loadFromDisk() {
        val d = dir()
        val list = d.listFiles()?.filter { it.extension == "json" }?.mapNotNull {
            runCatching { json.decodeFromString<ConversationSessionFile>(it.readText()) }.getOrNull()
        }?.sortedBy { it.startedAtUtc } ?: emptyList()
        _sessions.value = list
    }
}
