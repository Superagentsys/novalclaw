import Foundation
import NaturalLanguage

/// 从通话转写文本中抽取关键信息。
/// Schema 参考 `skills/phone-call-assistant/key_info_extraction_schema.json`。
///
/// 客户端实现轻量规则 + NL 框架；复杂归一化由网关端大模型完成。
struct KeyInfoExtractor {
    struct CallerIdentity: Codable {
        var name: String?
        var organization: String?
        var role: String?
        var verified: Bool = false
    }

    struct ContactInfo: Codable {
        var phone: String?
        var phoneRedacted: String?
        var email: String?
        var address: String?

        enum CodingKeys: String, CodingKey {
            case phone
            case phoneRedacted = "phone_redacted"
            case email, address
        }
    }

    struct KeyEntity: Codable {
        let type: String
        let value: String
        var context: String?
    }

    struct ExtractResult: Codable {
        var schemaVersion: String = "1.0"
        var callIntent: String
        var confidence: Double
        var callerIdentity: CallerIdentity?
        var contactInfo: ContactInfo?
        var keyEntities: [KeyEntity] = []
        var summary: String?
        var sentiment: String = "unknown"

        enum CodingKeys: String, CodingKey {
            case schemaVersion = "schema_version"
            case callIntent = "call_intent"
            case confidence
            case callerIdentity = "caller_identity"
            case contactInfo = "contact_info"
            case keyEntities = "key_entities"
            case summary, sentiment
        }
    }

    private static let intentKeywords: [(intent: String, keywords: [String])] = [
        ("delivery", ["快递", "外卖", "包裹", "签收"]),
        ("sales", ["推销", "优惠", "活动", "促销"]),
        ("marketing", ["广告", "公众号", "关注", "扫码"]),
        ("fraud_suspected", ["公安局", "洗钱", "安全账户", "冻结"]),
        ("customer_service", ["客服", "售后", "投诉", "工单"]),
        ("appointment", ["预约", "面谈", "会议", "上门"]),
        ("recruitment", ["招聘", "简历", "面试", "岗位"]),
        ("business_inquiry", ["合作", "咨询", "报价", "方案"])
    ]

    func extract(from session: ConversationSessionFile) -> ExtractResult {
        let callerTurns = session.turns.filter { $0.role == "caller" }
        let transcript = callerTurns.map { $0.text }.joined(separator: "\n")

        let (intent, confidence) = detectIntent(transcript: transcript)
        let entities = extractEntities(from: transcript)
        let contact = extractContact(from: transcript)
        let identity = extractIdentity(from: transcript)
        let summary = buildSummary(transcript: transcript, intent: intent)

        return ExtractResult(
            callIntent: intent,
            confidence: confidence,
            callerIdentity: identity,
            contactInfo: contact,
            keyEntities: entities,
            summary: summary,
            sentiment: detectSentiment(transcript: transcript)
        )
    }

    private func detectIntent(transcript: String) -> (String, Double) {
        var best: (intent: String, hits: Int) = ("unknown", 0)
        for pair in Self.intentKeywords {
            let hits = pair.keywords.reduce(0) { $0 + (transcript.contains($1) ? 1 : 0) }
            if hits > best.hits {
                best = (pair.intent, hits)
            }
        }
        let confidence = min(1.0, Double(best.hits) * 0.35 + (best.hits > 0 ? 0.3 : 0))
        return (best.intent, confidence)
    }

    private func extractEntities(from text: String) -> [KeyEntity] {
        var entities: [KeyEntity] = []
        let tagger = NLTagger(tagSchemes: [.nameType])
        tagger.string = text
        let options: NLTagger.Options = [.omitWhitespace, .omitPunctuation, .joinNames]
        let tags: [NLTag] = [.personalName, .placeName, .organizationName]
        tagger.enumerateTags(
            in: text.startIndex..<text.endIndex,
            unit: .word,
            scheme: .nameType,
            options: options
        ) { tag, range in
            if let tag, tags.contains(tag) {
                let value = String(text[range])
                let type: String
                switch tag {
                case .personalName: type = "person"
                case .placeName: type = "location"
                case .organizationName: type = "organization"
                default: type = "other"
                }
                entities.append(KeyEntity(type: type, value: value))
            }
            return true
        }
        return entities
    }

    private func extractContact(from text: String) -> ContactInfo? {
        var info = ContactInfo()
        if let phone = firstMatch(in: text, pattern: #"1[3-9]\d{9}"#) {
            info.phone = phone
            info.phoneRedacted = redactPhone(phone)
        }
        if let email = firstMatch(in: text, pattern: #"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}"#) {
            info.email = email
        }
        if info.phone == nil && info.email == nil { return nil }
        return info
    }

    private func extractIdentity(from text: String) -> CallerIdentity? {
        var identity = CallerIdentity()
        // 简单启发：「我是 XXX」「XXX 公司」
        if let name = firstMatch(in: text, pattern: #"(?:我是|本人)([\p{Han}A-Za-z]{2,6})"#, groupIndex: 1) {
            identity.name = name
        }
        if let org = firstMatch(in: text, pattern: #"([\p{Han}A-Za-z0-9]{2,20}(?:公司|集团|科技|银行|大学))"#, groupIndex: 1) {
            identity.organization = org
        }
        if identity.name == nil && identity.organization == nil { return nil }
        return identity
    }

    private func detectSentiment(transcript: String) -> String {
        let negatives = ["生气", "投诉", "不满", "愤怒", "欺骗"]
        let positives = ["谢谢", "感谢", "满意", "不错"]
        let neg = negatives.reduce(0) { $0 + (transcript.contains($1) ? 1 : 0) }
        let pos = positives.reduce(0) { $0 + (transcript.contains($1) ? 1 : 0) }
        if neg > pos && neg > 0 { return "negative" }
        if pos > neg && pos > 0 { return "positive" }
        if transcript.isEmpty { return "unknown" }
        return "neutral"
    }

    private func buildSummary(transcript: String, intent: String) -> String? {
        let trimmed = transcript
            .components(separatedBy: .newlines)
            .joined(separator: " ")
            .prefix(80)
        if trimmed.isEmpty { return nil }
        return "[\(intent)] \(trimmed)"
    }

    private func firstMatch(in text: String, pattern: String, groupIndex: Int = 0) -> String? {
        guard let regex = try? NSRegularExpression(pattern: pattern) else { return nil }
        let range = NSRange(text.startIndex..., in: text)
        guard let match = regex.firstMatch(in: text, range: range),
              let r = Range(match.range(at: groupIndex), in: text) else { return nil }
        return String(text[r])
    }

    private func redactPhone(_ phone: String) -> String {
        guard phone.count >= 11 else { return phone }
        let prefix = phone.prefix(3)
        let suffix = phone.suffix(4)
        return "\(prefix)****\(suffix)"
    }
}
