import Foundation

/// 管理对话会话的内存缓存与 JSON 文件持久化。
/// 文件写入 `Documents/conversations/<sessionId>.json`，格式与
/// `skills/phone-call-assistant/conversation_log_schema.json` 对齐。
@MainActor @Observable
final class ConversationLogStore {
    private(set) var sessions: [ConversationSessionFile] = []
    private let encoder: JSONEncoder = {
        let e = JSONEncoder()
        e.outputFormatting = [.prettyPrinted, .sortedKeys]
        return e
    }()

    init() { loadFromDisk() }

    func startSession(sessionId: String, channel: ConversationChannel) {
        let file = ConversationSessionFile(
            schemaVersion: ConversationSessionFile.schemaVersionValue,
            sessionId: sessionId,
            channel: channel,
            startedAtUtc: Self.iso8601Now(),
            endedAtUtc: nil,
            locale: Locale.current.identifier,
            turns: [],
            metadata: ["app": "OmniNovaPhoneAgent"]
        )
        sessions.append(file)
        persist(file)
    }

    func appendTurn(sessionId: String, role: String, text: String, isFinal: Bool) {
        guard let idx = sessions.firstIndex(where: { $0.sessionId == sessionId }) else { return }
        if !isFinal {
            if let lastIdx = sessions[idx].turns.lastIndex(where: { $0.role == role && $0.confidence == nil }) {
                sessions[idx].turns[lastIdx] = ConversationTurn(
                    t: Self.iso8601Now(), role: role, text: text, confidence: nil, rawNote: nil
                )
                return
            }
        }
        let turn = ConversationTurn(
            t: Self.iso8601Now(), role: role, text: text,
            confidence: isFinal ? 1.0 : nil, rawNote: nil
        )
        sessions[idx].turns.append(turn)
        persist(sessions[idx])
    }

    func endSession(sessionId: String) {
        guard let idx = sessions.firstIndex(where: { $0.sessionId == sessionId }) else { return }
        sessions[idx].endedAtUtc = Self.iso8601Now()
        persist(sessions[idx])
    }

    func session(for id: String) -> ConversationSessionFile? {
        sessions.first(where: { $0.sessionId == id })
    }

    private func persist(_ session: ConversationSessionFile) {
        let dir = Self.conversationsDir()
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let url = dir.appendingPathComponent("\(session.sessionId).json")
        if let data = try? encoder.encode(session) {
            try? data.write(to: url, options: .atomic)
        }
    }

    private func loadFromDisk() {
        let dir = Self.conversationsDir()
        guard let files = try? FileManager.default.contentsOfDirectory(at: dir, includingPropertiesForKeys: nil) else { return }
        let decoder = JSONDecoder()
        for url in files where url.pathExtension == "json" {
            guard let data = try? Data(contentsOf: url),
                  let session = try? decoder.decode(ConversationSessionFile.self, from: data) else { continue }
            sessions.append(session)
        }
        sessions.sort { $0.startedAtUtc < $1.startedAtUtc }
    }

    private static func conversationsDir() -> URL {
        FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("conversations", isDirectory: true)
    }

    private static func iso8601Now() -> String {
        ISO8601DateFormatter().string(from: Date())
    }
}
