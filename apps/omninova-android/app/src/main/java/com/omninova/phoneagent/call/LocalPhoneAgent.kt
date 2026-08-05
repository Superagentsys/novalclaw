package com.omninova.phoneagent.call

/**
 * A deterministic, fully on-device call assistant. It never opens a network
 * connection and does not require a desktop OmniNova gateway or an API key.
 *
 * This is deliberately a small rule-based agent rather than pretending that a
 * bundled LLM exists. A future on-device model can replace this class without
 * changing the call, speech, or conversation-log flow.
 */
class LocalPhoneAgent {
    fun reply(transcript: String, languageTag: String): String {
        val normalized = transcript.lowercase()
        val intent = when {
            normalized.containsAny(FRAUD_KEYWORDS) -> Intent.FRAUD
            normalized.containsAny(DELIVERY_KEYWORDS) -> Intent.DELIVERY
            normalized.containsAny(APPOINTMENT_KEYWORDS) -> Intent.APPOINTMENT
            normalized.containsAny(SALES_KEYWORDS) -> Intent.SALES
            normalized.containsAny(SUPPORT_KEYWORDS) -> Intent.SUPPORT
            else -> Intent.GENERAL
        }
        return response(intent, languageTag)
    }

    private fun response(intent: Intent, languageTag: String): String = when (languageTag) {
        "en" -> when (intent) {
            Intent.FRAUD -> "For security, I cannot share verification codes, passwords, or make transfers. Please use the official channel."
            Intent.DELIVERY -> "Thank you. Please leave the delivery details and a contact method; they will be reviewed on this phone."
            Intent.APPOINTMENT -> "Thank you. Please leave the proposed time and purpose, and the owner will confirm later."
            Intent.SALES -> "Thank you for the information. Please leave the key details; unsolicited offers are not accepted during this call."
            Intent.SUPPORT -> "Your request has been recorded locally. Please state the order or issue details."
            Intent.GENERAL -> "This is the local phone assistant. Your message has been recorded; please state your name and reason for calling."
        }
        "zh-TW" -> when (intent) {
            Intent.FRAUD -> "為保障安全，我無法提供驗證碼、密碼或進行轉帳。請透過官方管道聯絡。"
            Intent.DELIVERY -> "謝謝，請留下配送內容與聯絡方式，資料會儲存在本機供後續查看。"
            Intent.APPOINTMENT -> "謝謝，請留下預約時間與事項，機主稍後會確認。"
            Intent.SALES -> "謝謝您的資訊，請留下重點內容；本通話不接受即時推銷。"
            Intent.SUPPORT -> "您的需求已在手機本機記錄，請說明訂單或問題詳情。"
            Intent.GENERAL -> "這裡是本機通話助理，訊息已記錄。請留下姓名與來電事由。"
        }
        else -> when (intent) {
            Intent.FRAUD -> "为保障安全，我无法提供验证码、密码或进行转账。请通过官方渠道联系。"
            Intent.DELIVERY -> "谢谢，请留下配送内容和联系方式，信息会保存在本机供稍后查看。"
            Intent.APPOINTMENT -> "谢谢，请留下预约时间和事项，机主稍后会确认。"
            Intent.SALES -> "谢谢您的信息，请留下重点内容；本通话不接受即时推销。"
            Intent.SUPPORT -> "您的需求已在手机本机记录，请说明订单或问题详情。"
            Intent.GENERAL -> "这里是本机通话助手，信息已记录。请留下姓名和来电事由。"
        }
    }

    private enum class Intent { FRAUD, DELIVERY, APPOINTMENT, SALES, SUPPORT, GENERAL }

    private fun String.containsAny(keywords: Set<String>): Boolean = keywords.any(::contains)

    private companion object {
        val FRAUD_KEYWORDS = setOf(
            "验证码", "转账", "安全账户", "公安", "冻结", "退款", "刷单",
            "verification code", "transfer", "password", "bank account",
        )
        val DELIVERY_KEYWORDS = setOf("快递", "外卖", "包裹", "签收", "delivery", "package")
        val APPOINTMENT_KEYWORDS = setOf("预约", "会议", "面谈", "上门", "appointment", "meeting")
        val SALES_KEYWORDS = setOf("推广", "优惠", "促销", "广告", "sales", "promotion")
        val SUPPORT_KEYWORDS = setOf("客服", "售后", "工单", "投诉", "support", "service")
    }
}
