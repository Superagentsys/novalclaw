package com.omninova.phoneagent.call

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import java.time.LocalTime

/**
 * 骚扰识别，规则与 `skills/phone-call-assistant/spam_detection_rules.json` 对齐。
 */
class SpamDetector(initialJson: String? = null) {

    @Serializable
    enum class Severity { block, silence, allow }

    @Serializable
    data class Match(
        val type: String,
        val values: List<String>? = null,
        @SerialName("window_seconds") val windowSeconds: Int? = null,
        val threshold: Int? = null,
        val scope: String? = null,
        @SerialName("start_local") val startLocal: String? = null,
        @SerialName("end_local") val endLocal: String? = null,
        @SerialName("applies_to") val appliesTo: String? = null,
    )

    @Serializable
    data class Rule(
        val id: String,
        val severity: Severity,
        val reason: String,
        val match: Match,
    )

    @Serializable
    data class RulesFile(
        @SerialName("schema_version") val schemaVersion: String = "1.0",
        val rules: List<Rule> = emptyList(),
        @SerialName("number_blocklist") val numberBlocklist: List<String> = emptyList(),
        @SerialName("number_allowlist") val numberAllowlist: List<String> = emptyList(),
    )

    data class Decision(
        val severity: Severity,
        val matchedRuleId: String? = null,
        val reason: String? = null,
    ) {
        companion object {
            val ALLOW = Decision(Severity.allow)
        }
    }

    private val parser = Json { ignoreUnknownKeys = true }

    @Volatile
    var rulesFile: RulesFile = initialJson?.let {
        runCatching { parser.decodeFromString(RulesFile.serializer(), it) }.getOrNull()
    } ?: RulesFile()
        private set

    private val recent = HashMap<String, ArrayDeque<Long>>()

    fun updateRules(jsonText: String) {
        runCatching { parser.decodeFromString(RulesFile.serializer(), jsonText) }
            .onSuccess { rulesFile = it }
    }

    fun evaluate(
        phoneNumber: String?,
        isInContacts: Boolean,
        firstUtterance: String?,
        nowMs: Long = System.currentTimeMillis(),
        now: LocalTime = LocalTime.now(),
    ): Decision {
        if (phoneNumber != null) {
            if (rulesFile.numberAllowlist.contains(phoneNumber)) return Decision.ALLOW
            if (rulesFile.numberBlocklist.contains(phoneNumber)) {
                return Decision(Severity.block, "blocklist", "硬黑名单")
            }
            val history = recent.getOrPut(phoneNumber) { ArrayDeque() }
            history.addLast(nowMs)
            while (history.isNotEmpty() && nowMs - history.first() > 86_400_000L) {
                history.removeFirst()
            }
        }

        for (rule in rulesFile.rules) {
            if (matches(rule, phoneNumber, isInContacts, firstUtterance, nowMs, now)) {
                return Decision(rule.severity, rule.id, rule.reason)
            }
        }
        return Decision.ALLOW
    }

    private fun matches(
        rule: Rule,
        phone: String?,
        isInContacts: Boolean,
        utterance: String?,
        nowMs: Long,
        now: LocalTime,
    ): Boolean {
        return when (rule.match.type) {
            "prefix" -> {
                val p = phone ?: return false
                rule.match.values?.any { p.startsWith(it) } ?: false
            }
            "keyword" -> {
                val u = utterance?.lowercase() ?: return false
                rule.match.values?.any { u.contains(it.lowercase()) } ?: false
            }
            "rate_limit" -> {
                val p = phone ?: return false
                val window = rule.match.windowSeconds ?: return false
                val threshold = rule.match.threshold ?: return false
                if (rule.match.scope == "not_in_contacts" && isInContacts) return false
                val history = recent[p] ?: return false
                val cutoff = nowMs - window * 1000L
                history.count { it >= cutoff } >= threshold
            }
            "time_window" -> {
                val start = rule.match.startLocal ?: return false
                val end = rule.match.endLocal ?: return false
                if (rule.match.scope == "not_in_contacts" && isInContacts) return false
                val s = LocalTime.parse(start)
                val e = LocalTime.parse(end)
                if (s.isAfter(e)) !now.isBefore(s) || !now.isAfter(e)
                else !now.isBefore(s) && !now.isAfter(e)
            }
            else -> false
        }
    }
}
