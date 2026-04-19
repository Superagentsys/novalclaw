import SwiftUI

struct ContentView: View {
    @Environment(CallManager.self) private var callManager
    @Environment(SpeechPipeline.self) private var speech
    @Environment(ConversationLogStore.self) private var logStore
    @Environment(AgentGatewayClient.self) private var gateway
    @Environment(AgentResponseSynthesizer.self) private var synthesizer

    @State private var gatewayURL = "http://192.168.1.100:10809"
    @State private var showSettings = false

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                statusBanner
                sessionList
            }
            .navigationTitle("OmniNova 通话助手")
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button { showSettings.toggle() } label: {
                        Image(systemName: "gearshape")
                    }
                }
                ToolbarItem(placement: .topBarLeading) {
                    Button { startSimulatedCall() } label: {
                        Label("模拟来电", systemImage: "phone.arrow.down.left")
                    }
                }
            }
            .sheet(isPresented: $showSettings) { settingsSheet }
        }
    }

    private var statusBanner: some View {
        HStack(spacing: 12) {
            Circle()
                .fill(gateway.isConnected ? Color.green : Color.orange)
                .frame(width: 10, height: 10)
            Text(gateway.isConnected ? "网关已连接" : "网关未连接")
                .font(.caption)
            Spacer()
            if speech.isListening {
                Label("转写中", systemImage: "waveform")
                    .font(.caption)
                    .foregroundStyle(.blue)
            }
            if callManager.hasActiveCall {
                Label("通话中", systemImage: "phone.fill")
                    .font(.caption)
                    .foregroundStyle(.green)
            }
        }
        .padding(.horizontal)
        .padding(.vertical, 8)
        .background(.ultraThinMaterial)
    }

    private var sessionList: some View {
        List {
            ForEach(logStore.sessions.reversed()) { session in
                NavigationLink {
                    SessionDetailView(session: session)
                } label: {
                    VStack(alignment: .leading, spacing: 4) {
                        HStack {
                            Text(session.channel.rawValue)
                                .font(.caption2)
                                .padding(.horizontal, 6).padding(.vertical, 2)
                                .background(.blue.opacity(0.12))
                                .clipShape(Capsule())
                            Spacer()
                            Text(session.startedAtUtc.prefix(19))
                                .font(.caption2).foregroundStyle(.secondary)
                        }
                        Text("\(session.turns.count) 轮对话")
                            .font(.subheadline)
                        if let last = session.turns.last {
                            Text("\(last.role): \(last.text)")
                                .font(.caption)
                                .lineLimit(1)
                                .foregroundStyle(.secondary)
                        }
                    }
                    .padding(.vertical, 4)
                }
            }
        }
        .listStyle(.plain)
        .overlay {
            if logStore.sessions.isEmpty {
                ContentUnavailableView(
                    "暂无对话记录",
                    systemImage: "phone.badge.waveform",
                    description: Text("来电或模拟通话后将自动记录")
                )
            }
        }
    }

    private var settingsSheet: some View {
        NavigationStack {
            Form {
                Section("OmniNova 网关") {
                    TextField("地址", text: $gatewayURL)
                        .keyboardType(.URL)
                        .autocorrectionDisabled()
                    Button("连接") {
                        gateway.configure(baseURL: gatewayURL)
                        Task { await gateway.checkConnection() }
                    }
                }
                Section("权限") {
                    HStack {
                        Text("语音识别")
                        Spacer()
                        Text(speech.authorizationStatus == .authorized ? "已授权" : "未授权")
                            .foregroundStyle(.secondary)
                    }
                    Button("请求麦克风与语音权限") {
                        Task { await speech.requestAuthorization() }
                    }
                }
                Section("关于") {
                    Text("OmniNova Phone Agent v0.1.0")
                    Text("技能：phone-call-assistant")
                        .font(.caption).foregroundStyle(.secondary)
                }
            }
            .navigationTitle("设置")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("完成") { showSettings = false }
                }
            }
        }
    }

    private func startSimulatedCall() {
        let sessionId = UUID().uuidString.lowercased()
        logStore.startSession(sessionId: sessionId, channel: .simulated)
        do {
            try speech.startListening(
                onPartial: { partial in
                    logStore.appendTurn(sessionId: sessionId, role: "caller", text: partial, isFinal: false)
                },
                onFinal: { transcript in
                    logStore.appendTurn(sessionId: sessionId, role: "caller", text: transcript, isFinal: true)
                    Task {
                        guard let reply = await gateway.chat(text: transcript, sessionId: sessionId) else { return }
                        logStore.appendTurn(sessionId: sessionId, role: "agent", text: reply, isFinal: true)
                        synthesizer.speak(reply)
                    }
                }
            )
        } catch {
            print("[SimCall] start failed: \(error)")
        }
    }
}

struct SessionDetailView: View {
    let session: ConversationSessionFile

    var body: some View {
        List {
            ForEach(session.turns) { turn in
                VStack(alignment: turn.role == "agent" ? .trailing : .leading, spacing: 4) {
                    Text(turn.role == "agent" ? "🤖 Agent" : "📞 来电者")
                        .font(.caption2).foregroundStyle(.secondary)
                    Text(turn.text)
                        .padding(10)
                        .background(turn.role == "agent" ? Color.blue.opacity(0.1) : Color.gray.opacity(0.1))
                        .clipShape(RoundedRectangle(cornerRadius: 12))
                    Text(turn.t.suffix(8))
                        .font(.caption2).foregroundStyle(.tertiary)
                }
                .frame(maxWidth: .infinity, alignment: turn.role == "agent" ? .trailing : .leading)
                .listRowSeparator(.hidden)
            }
        }
        .listStyle(.plain)
        .navigationTitle("会话 \(session.sessionId.prefix(8))…")
        .navigationBarTitleDisplayMode(.inline)
    }
}
