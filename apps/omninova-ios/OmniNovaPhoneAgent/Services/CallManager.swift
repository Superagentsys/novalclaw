import CallKit
import Foundation

/// CallKit + VoIP 来电管理。
///
/// - `reportIncomingCall` 向系统报告 VoIP 来电（需 PushKit + VoIP 证书触发，此处提供 API）。
/// - 系统展示原生来电 UI 后，`CXAnswerCallAction` 自动执行接听（若 `autoAnswer` 开启）。
/// - **蜂窝电话无法用 CallKit 自动接听**——仅限 VoIP 来电。
@MainActor @Observable
final class CallManager: NSObject {
    private(set) var hasActiveCall = false
    private(set) var activeCallUUID: UUID?

    var autoAnswer = true

    private let provider: CXProvider
    private let callController = CXCallController()
    private var onCallAnswered: ((UUID) -> Void)?
    private var onCallEnded: ((UUID) -> Void)?

    override init() {
        let config = CXProviderConfiguration()
        config.localizedName = "OmniNova"
        config.supportsVideo = false
        config.maximumCallsPerCallGroup = 1
        config.supportedHandleTypes = [.generic, .phoneNumber]
        self.provider = CXProvider(configuration: config)
        super.init()
        provider.setDelegate(self, queue: nil)
    }

    func configure(
        onCallAnswered: @escaping (UUID) -> Void,
        onCallEnded: @escaping (UUID) -> Void
    ) {
        self.onCallAnswered = onCallAnswered
        self.onCallEnded = onCallEnded
    }

    /// 向系统报告一通 VoIP 来电。通常由 PushKit 推送触发。
    func reportIncomingCall(
        uuid: UUID = UUID(),
        handle: String = "OmniNova Agent",
        hasVideo: Bool = false
    ) async throws {
        let update = CXCallUpdate()
        update.remoteHandle = CXHandle(type: .generic, value: handle)
        update.hasVideo = hasVideo
        update.localizedCallerName = handle
        try await provider.reportNewIncomingCall(with: uuid, update: update)
    }

    /// 手动结束当前通话。
    func endCall() {
        guard let uuid = activeCallUUID else { return }
        let action = CXEndCallAction(call: uuid)
        callController.request(CXTransaction(action: action)) { error in
            if let error { print("[CallManager] end call error: \(error)") }
        }
    }
}

extension CallManager: CXProviderDelegate {
    nonisolated func providerDidReset(_ provider: CXProvider) {
        Task { @MainActor in
            hasActiveCall = false
            activeCallUUID = nil
        }
    }

    nonisolated func provider(_ provider: CXProvider, perform action: CXAnswerCallAction) {
        Task { @MainActor in
            hasActiveCall = true
            activeCallUUID = action.callUUID
            onCallAnswered?(action.callUUID)
        }
        action.fulfill()
    }

    nonisolated func provider(_ provider: CXProvider, perform action: CXEndCallAction) {
        Task { @MainActor in
            let uuid = action.callUUID
            hasActiveCall = false
            activeCallUUID = nil
            onCallEnded?(uuid)
        }
        action.fulfill()
    }

    nonisolated func provider(_ provider: CXProvider, perform action: CXStartCallAction) {
        action.fulfill()
    }

    nonisolated func provider(_ provider: CXProvider, didActivate audioSession: AVAudioSession) {
        // 音频会话激活：此处可切换到 playAndRecord 模式
    }

    nonisolated func provider(_ provider: CXProvider, didDeactivate audioSession: AVAudioSession) {}
}
