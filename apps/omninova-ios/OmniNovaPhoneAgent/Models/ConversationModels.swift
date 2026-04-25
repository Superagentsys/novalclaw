import Foundation

/// 与 `skills/phone-call-assistant/conversation_log_schema.json` 对齐。
enum ConversationChannel: String, Codable {
    case voipCallkit = "voip_callkit"
    case inAppVoice = "in_app_voice"
    case simulated = "simulated"
    case unknown = "unknown"
}

struct ConversationTurn: Codable, Identifiable {
    var id: String { "\(t)-\(role)" }
    let t: String
    let role: String
    let text: String
    var confidence: Double?
    var rawNote: String?

    enum CodingKeys: String, CodingKey {
        case t, role, text, confidence
        case rawNote = "raw_note"
    }
}

struct ConversationSessionFile: Codable, Identifiable {
    var id: String { sessionId }
    let schemaVersion: String
    let sessionId: String
    let channel: ConversationChannel
    let startedAtUtc: String
    var endedAtUtc: String?
    let locale: String?
    var turns: [ConversationTurn]
    var metadata: [String: String]?

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case sessionId = "session_id"
        case channel
        case startedAtUtc = "started_at_utc"
        case endedAtUtc = "ended_at_utc"
        case locale
        case turns
        case metadata
    }

    static let schemaVersionValue = "1.0"
}
