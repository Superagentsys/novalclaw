import Foundation

/// 与 OmniNova 网关（HTTP API）通信：发送对话文本、接收 Agent 回复、同步会话记录。
@MainActor @Observable
final class AgentGatewayClient {
    private(set) var isConnected = false
    private var baseURL = "http://127.0.0.1:10809"
    private let session = URLSession.shared
    private let encoder = JSONEncoder()
    private let decoder = JSONDecoder()

    func configure(baseURL: String) {
        var url = baseURL.trimmingCharacters(in: .whitespacesAndNewlines)
        if url.hasSuffix("/") { url.removeLast() }
        self.baseURL = url
    }

    func checkConnection() async {
        guard let url = URL(string: "\(baseURL)/api/health") else {
            isConnected = false
            return
        }
        do {
            let (_, resp) = try await session.data(from: url)
            isConnected = (resp as? HTTPURLResponse)?.statusCode == 200
        } catch {
            isConnected = false
        }
    }

    /// 发送一条消息到网关的 inbound 端点，返回 Agent 回复文本。
    func chat(text: String, sessionId: String, channel: String = "phone_voip") async -> String? {
        guard let url = URL(string: "\(baseURL)/api/inbound") else { return nil }
        var req = URLRequest(url: url)
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")

        let body: [String: Any] = [
            "channel": channel,
            "text": text,
            "session_id": sessionId,
            "user_id": "ios-phone-agent",
            "metadata": ["source": "omninova-ios", "device": UIDevice.current.name]
        ]
        req.httpBody = try? JSONSerialization.data(withJSONObject: body)

        do {
            let (data, _) = try await session.data(for: req)
            if let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
               let reply = json["reply"] as? String {
                return reply
            }
        } catch {
            print("[Gateway] chat error: \(error)")
        }
        return nil
    }

    /// 通话结束后将完整会话 JSON 同步到网关。
    func syncSession(_ session: ConversationSessionFile?) async {
        guard let session else { return }
        guard let url = URL(string: "\(baseURL)/api/webhook") else { return }
        var req = URLRequest(url: url)
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.httpBody = try? encoder.encode([
            "type": "conversation_sync",
            "session_id": session.sessionId,
            "channel": session.channel.rawValue,
            "turns_count": "\(session.turns.count)"
        ])
        _ = try? await self.session.data(for: req)
    }
}
