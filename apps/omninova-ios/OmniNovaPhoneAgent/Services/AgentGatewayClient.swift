import Foundation
#if canImport(UIKit)
import UIKit
#endif

private let gatewayURLDefaultsKey = "omninova.gateway.url"

/// 与 OmniNova 网关（HTTP API）通信：发送对话文本、接收 Agent 回复、同步会话记录。
///
/// 仅以 `@Observable` 暴露给 SwiftUI；HTTP 调用本就跨线程，类级别不再标
/// `@MainActor`，让 `@State` 默认值能够在合成 init 中直接构造。
@Observable
final class AgentGatewayClient: @unchecked Sendable {
    private(set) var isConnected = false
    private var baseURL = AgentGatewayClient.loadSavedBaseURL() ?? ""
    private let session = URLSession.shared
    private let encoder: JSONEncoder = {
        let e = JSONEncoder()
        e.outputFormatting = [.sortedKeys]
        return e
    }()
    private let decoder = JSONDecoder()

    init() {}

    static func loadSavedBaseURL() -> String? {
        let value = UserDefaults.standard.string(forKey: gatewayURLDefaultsKey)?
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard let value, !value.isEmpty else { return nil }
        return value
    }

    static func saveBaseURL(_ url: String) {
        UserDefaults.standard.set(url, forKey: gatewayURLDefaultsKey)
    }

    private var deviceName: String {
        #if canImport(UIKit)
        return UIDevice.current.name
        #else
        return ProcessInfo.processInfo.hostName
        #endif
    }

    func configure(baseURL: String) {
        var url = baseURL.trimmingCharacters(in: .whitespacesAndNewlines)
        if url.hasSuffix("/") { url.removeLast() }
        self.baseURL = url
        if !url.isEmpty {
            Self.saveBaseURL(url)
        }
    }

    func checkConnection() async {
        guard !baseURL.isEmpty else {
            isConnected = false
            return
        }
        // 优先 /health，兼容旧版 /api/health
        for path in ["/health", "/api/health"] {
            guard let url = URL(string: "\(baseURL)\(path)") else { continue }
            do {
                let (_, resp) = try await session.data(from: url)
                if (resp as? HTTPURLResponse)?.statusCode == 200 {
                    isConnected = true
                    return
                }
            } catch {
                continue
            }
        }
        isConnected = false
    }

    /// 发送一条消息到网关 inbound 端点，返回 Agent 回复文本。
    func chat(text: String, sessionId: String, channel: String = "phone_voip") async -> String? {
        guard !baseURL.isEmpty else { return nil }
        guard let url = URL(string: "\(baseURL)/api/inbound") else { return nil }
        var req = URLRequest(url: url)
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.timeoutInterval = 120

        let body: [String: Any] = [
            "channel": channel,
            "text": text,
            "session_id": sessionId,
            "user_id": "ios-phone-agent",
            "metadata": ["source": "omninova-ios", "device": deviceName]
        ]
        req.httpBody = try? JSONSerialization.data(withJSONObject: body)

        do {
            let (data, resp) = try await session.data(for: req)
            guard (resp as? HTTPURLResponse)?.statusCode == 200 else {
                print("[Gateway] chat HTTP \((resp as? HTTPURLResponse)?.statusCode ?? -1)")
                return nil
            }
            if let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
               let reply = json["reply"] as? String, !reply.isEmpty {
                return reply
            }
        } catch {
            print("[Gateway] chat error: \(error)")
        }
        return nil
    }

    /// 通话结束后将完整会话 JSON 同步到网关。
    func syncSession(_ session: ConversationSessionFile?) async {
        guard let session, !baseURL.isEmpty else { return }
        guard let url = URL(string: "\(baseURL)/api/webhook") else { return }
        var req = URLRequest(url: url)
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.setValue("conversation_sync", forHTTPHeaderField: "X-OmniNova-Event")
        req.httpBody = try? encoder.encode(SyncEnvelope(
            type: "conversation_sync",
            session: session
        ))
        _ = try? await self.session.data(for: req)
    }

    /// 触发网关侧关键信息抽取（端侧已抽取，网关仅 ack）。
    func extractKeyInfo(sessionId: String) async -> [String: Any]? {
        guard !baseURL.isEmpty else { return nil }
        guard let url = URL(string: "\(baseURL)/api/skill/phone-call-assistant/extract") else {
            return nil
        }
        var req = URLRequest(url: url)
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.httpBody = try? JSONSerialization.data(withJSONObject: [
            "session_id": sessionId
        ])
        do {
            let (data, _) = try await session.data(for: req)
            return try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        } catch {
            return nil
        }
    }

    /// 从网关拉取最新骚扰识别规则。
    func fetchSpamRules() async -> Data? {
        guard !baseURL.isEmpty else { return nil }
        guard let url = URL(string: "\(baseURL)/api/skill/phone-call-assistant/rules") else {
            return nil
        }
        do {
            let (data, resp) = try await session.data(from: url)
            guard (resp as? HTTPURLResponse)?.statusCode == 200 else { return nil }
            return data
        } catch {
            return nil
        }
    }
}

private struct SyncEnvelope: Codable {
    let type: String
    let session: ConversationSessionFile
}
