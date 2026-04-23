package com.omninova.phoneagent.call

import com.omninova.phoneagent.data.ConversationSessionFile
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * 通话关键信息抽取。Schema 对齐 `skills/phone-call-assistant/key_info_extraction_schema.json`。
 */
class KeyInfoExtractor {
    @Serializable
    data class CallerIdentity(
        val name: String? = null,
        val organization: String? = null,
        val role: String? = null,
        val verified: Boolean = false,
    )

    @Serializable
    data class ContactInfo(
        val phone: String? = null,
        @SerialName("phone_redacted") val phoneRedacted: String? = null,
        val email: String? = null,
        val address: String? = null,
    )

    @Serializable
    data class KeyEntity(val type: String, val value: String, val context: String? = null)

    @Serializable
    data class ExtractResult(
        @SerialName("schema_version") val schemaVersion: String = "1.0",
        @SerialName("call_intent") val callIntent: String,
        val confidence: Double,
        @SerialName("caller_identity") val callerIdentity: CallerIdentity? = null,
        @SerialName("contact_info") val contactInfo: ContactInfo? = null,
        @SerialName("key_entities") val keyEntities: List<KeyEntity> = emptyList(),
        val summary: String? = null,
        val sentiment: String = "unknown",
    )

    private val intentKeywords = mapOf(
        "delivery" to listOf("快递", "外卖", "包裹", "签收"),
        "sales" to listOf("推销", "优惠", "活动", "促销"),
        "marketing" to listOf("广告", "公众号", "关注", "扫码"),
        "fraud_suspected" to listOf("公安局", "洗钱", "安全账户", "冻结"),
        "customer_service" to listOf("客服", "售后", "投诉", "工单"),
        "appointment" to listOf("预约", "面谈", "会议", "上门"),
        "recruitment" to listOf("招聘", "简历", "面试", "岗位"),
        "business_inquiry" to listOf("合作", "咨询", "报价", "方案"),
    )

    private val phoneRegex = Regex("1[3-9]\\d{9}")
    private val emailRegex = Regex("[A-Za-z0-9._%+\\-]+@[A-Za-z0-9.\\-]+\\.[A-Za-z]{2,}")
    private val nameRegex = Regex("(?:我是|本人)([\\p{IsHan}A-Za-z]{2,6})")
    private val orgRegex = Regex("([\\p{IsHan}A-Za-z0-9]{2,20}(?:公司|集团|科技|银行|大学))")

    fun extract(session: ConversationSessionFile): ExtractResult {
        val transcript = session.turns.filter { it.role == "caller" }
            .joinToString("\n") { it.text }
        val (intent, confidence) = detectIntent(transcript)
        val phone = phoneRegex.find(transcript)?.value
        val email = emailRegex.find(transcript)?.value
        val name = nameRegex.find(transcript)?.groupValues?.getOrNull(1)
        val org = orgRegex.find(transcript)?.groupValues?.getOrNull(1)

        return ExtractResult(
            callIntent = intent,
            confidence = confidence,
            callerIdentity = if (name != null || org != null)
                CallerIdentity(name = name, organization = org) else null,
            contactInfo = if (phone != null || email != null)
                ContactInfo(phone = phone, phoneRedacted = phone?.let(::redact), email = email)
            else null,
            keyEntities = emptyList(),
            summary = transcript.take(80).ifBlank { null }?.let { "[$intent] $it" },
            sentiment = detectSentiment(transcript),
        )
    }

    private fun detectIntent(text: String): Pair<String, Double> {
        var best = "unknown" to 0
        for ((intent, keywords) in intentKeywords) {
            val hits = keywords.count { text.contains(it) }
            if (hits > best.second) best = intent to hits
        }
        val confidence = minOf(1.0, best.second * 0.35 + (if (best.second > 0) 0.3 else 0.0))
        return best.first to confidence
    }

    private fun detectSentiment(text: String): String {
        val neg = listOf("生气", "投诉", "不满", "愤怒", "欺骗").count { text.contains(it) }
        val pos = listOf("谢谢", "感谢", "满意", "不错").count { text.contains(it) }
        return when {
            neg > pos && neg > 0 -> "negative"
            pos > neg && pos > 0 -> "positive"
            text.isBlank() -> "unknown"
            else -> "neutral"
        }
    }

    private fun redact(phone: String): String =
        if (phone.length >= 11) "${phone.take(3)}****${phone.takeLast(4)}" else phone
}
