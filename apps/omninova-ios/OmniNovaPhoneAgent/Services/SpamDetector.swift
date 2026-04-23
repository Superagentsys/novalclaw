import Foundation

/// 骚扰电话识别器，规则来源于 `skills/phone-call-assistant/spam_detection_rules.json`。
///
/// 可在 App 启动时预置内置规则，并从网关通过 `AgentGatewayClient.fetchSpamRules()` 动态覆盖。
@MainActor @Observable
final class SpamDetector {
    enum Severity: String, Codable {
        case block, silence, allow
    }

    struct Decision: Codable, Equatable {
        let severity: Severity
        let matchedRuleId: String?
        let reason: String?

        static let allow = Decision(severity: .allow, matchedRuleId: nil, reason: nil)
    }

    struct Rule: Codable {
        let id: String
        let severity: Severity
        let reason: String
        let match: Match
    }

    struct Match: Codable {
        let type: String
        let values: [String]?
        let windowSeconds: Int?
        let threshold: Int?
        let scope: String?
        let startLocal: String?
        let endLocal: String?
        let appliesTo: String?

        enum CodingKeys: String, CodingKey {
            case type, values, threshold, scope
            case windowSeconds = "window_seconds"
            case startLocal = "start_local"
            case endLocal = "end_local"
            case appliesTo = "applies_to"
        }
    }

    struct RulesFile: Codable {
        let schemaVersion: String?
        let rules: [Rule]
        let numberBlocklist: [String]?
        let numberAllowlist: [String]?

        enum CodingKeys: String, CodingKey {
            case schemaVersion = "schema_version"
            case rules
            case numberBlocklist = "number_blocklist"
            case numberAllowlist = "number_allowlist"
        }
    }

    private(set) var rulesFile: RulesFile = RulesFile(
        schemaVersion: "1.0",
        rules: [],
        numberBlocklist: [],
        numberAllowlist: []
    )

    private var recentCalls: [String: [Date]] = [:]

    init(bundledJSON: Data? = nil) {
        if let bundledJSON, let rf = try? JSONDecoder().decode(RulesFile.self, from: bundledJSON) {
            self.rulesFile = rf
        } else if let data = Self.loadBundledDefault() {
            if let rf = try? JSONDecoder().decode(RulesFile.self, from: data) {
                self.rulesFile = rf
            }
        }
    }

    func updateRules(from data: Data) {
        guard let rf = try? JSONDecoder().decode(RulesFile.self, from: data) else { return }
        self.rulesFile = rf
    }

    /// 评估一通来电。`firstUtterance` 可在前几秒 ASR 拿到开场白后补充调用。
    func evaluate(
        phoneNumber: String?,
        isInContacts: Bool,
        firstUtterance: String?,
        now: Date = Date()
    ) -> Decision {
        if let phone = phoneNumber, rulesFile.numberAllowlist?.contains(phone) == true {
            return .allow
        }
        if let phone = phoneNumber, rulesFile.numberBlocklist?.contains(phone) == true {
            return Decision(severity: .block, matchedRuleId: "blocklist", reason: "硬黑名单")
        }

        if let phone = phoneNumber {
            recentCalls[phone, default: []].append(now)
            recentCalls[phone] = recentCalls[phone]?.filter { now.timeIntervalSince($0) < 24 * 3600 }
        }

        for rule in rulesFile.rules {
            if matches(rule: rule, phone: phoneNumber, isInContacts: isInContacts,
                       utterance: firstUtterance, now: now) {
                return Decision(severity: rule.severity, matchedRuleId: rule.id, reason: rule.reason)
            }
        }
        return .allow
    }

    private func matches(
        rule: Rule,
        phone: String?,
        isInContacts: Bool,
        utterance: String?,
        now: Date
    ) -> Bool {
        switch rule.match.type {
        case "prefix":
            guard let phone, let values = rule.match.values else { return false }
            return values.contains { phone.hasPrefix($0) }
        case "keyword":
            guard let utterance = utterance?.lowercased(), let values = rule.match.values else {
                return false
            }
            return values.contains { utterance.contains($0.lowercased()) }
        case "rate_limit":
            guard let phone,
                  let window = rule.match.windowSeconds,
                  let threshold = rule.match.threshold else { return false }
            if rule.match.scope == "not_in_contacts" && isInContacts { return false }
            let history = recentCalls[phone] ?? []
            let recent = history.filter { now.timeIntervalSince($0) < TimeInterval(window) }
            return recent.count >= threshold
        case "time_window":
            guard let start = rule.match.startLocal,
                  let end = rule.match.endLocal else { return false }
            if rule.match.scope == "not_in_contacts" && isInContacts { return false }
            let cal = Calendar.current
            let comps = cal.dateComponents([.hour, .minute], from: now)
            let minutes = (comps.hour ?? 0) * 60 + (comps.minute ?? 0)
            let startMin = Self.parseMinutes(start)
            let endMin = Self.parseMinutes(end)
            if startMin > endMin {
                return minutes >= startMin || minutes <= endMin
            } else {
                return minutes >= startMin && minutes <= endMin
            }
        default:
            return false
        }
    }

    private static func parseMinutes(_ hhmm: String) -> Int {
        let parts = hhmm.split(separator: ":")
        guard parts.count == 2, let h = Int(parts[0]), let m = Int(parts[1]) else { return 0 }
        return h * 60 + m
    }

    private static func loadBundledDefault() -> Data? {
        guard let url = Bundle.main.url(forResource: "spam_detection_rules", withExtension: "json") else {
            return nil
        }
        return try? Data(contentsOf: url)
    }
}
