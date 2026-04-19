import SwiftUI

@main
struct OmniNovaPhoneAgentApp: App {
    @State private var callManager = CallManager()
    @State private var speechPipeline = SpeechPipeline()
    @State private var logStore = ConversationLogStore()
    @State private var gatewayClient = AgentGatewayClient()
    @State private var synthesizer = AgentResponseSynthesizer()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environment(callManager)
                .environment(speechPipeline)
                .environment(logStore)
                .environment(gatewayClient)
                .environment(synthesizer)
                .task {
                    callManager.configure(
                        onCallAnswered: { uuid in
                            handleCallAnswered(callUUID: uuid)
                        },
                        onCallEnded: { uuid in
                            handleCallEnded(callUUID: uuid)
                        }
                    )
                }
        }
    }

    private func handleCallAnswered(callUUID: UUID) {
        let sessionId = callUUID.uuidString.lowercased()
        logStore.startSession(sessionId: sessionId, channel: .voipCallkit)
        do {
            try speechPipeline.startListening(
                onPartial: { partial in
                    logStore.appendTurn(
                        sessionId: sessionId, role: "caller",
                        text: partial, isFinal: false
                    )
                },
                onFinal: { transcript in
                    logStore.appendTurn(
                        sessionId: sessionId, role: "caller",
                        text: transcript, isFinal: true
                    )
                    Task {
                        await sendToAgentAndSpeak(sessionId: sessionId, callerText: transcript)
                    }
                }
            )
        } catch {
            print("[PhoneAgent] speech start failed: \(error)")
        }
    }

    private func handleCallEnded(callUUID: UUID) {
        let sessionId = callUUID.uuidString.lowercased()
        speechPipeline.stopListening()
        synthesizer.stop()
        logStore.endSession(sessionId: sessionId)
        Task { await gatewayClient.syncSession(logStore.session(for: sessionId)) }
    }

    private func sendToAgentAndSpeak(sessionId: String, callerText: String) async {
        guard let reply = await gatewayClient.chat(text: callerText, sessionId: sessionId) else { return }
        logStore.appendTurn(sessionId: sessionId, role: "agent", text: reply, isFinal: true)
        synthesizer.speak(reply)
    }
}
