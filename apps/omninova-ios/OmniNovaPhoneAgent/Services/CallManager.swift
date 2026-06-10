import AVFoundation
import CallKit
import Foundation

/// CallKit + VoIP 来电管理。
///
/// - `reportIncomingCall` 向系统报告 VoIP 来电（需 PushKit + VoIP 证书触发，此处提供 API）。
/// - 系统展示原生来电 UI 后，`CXAnswerCallAction` 自动执行接听（若 `autoAnswer` 开启）。
/// - **蜂窝电话无法用 CallKit 自动接听**——仅限 VoIP 来电。
///
/// 类本身仅以 `@Observable` 提供 SwiftUI 观察能力，不再标记 `@MainActor`：
/// 这样 `@State` 默认值能在合成的 memberwise init 中直接构造它，同时
/// CallKit 自后台线程派发的代理回调也能就地处理，必要时再 `Task @MainActor`
/// 跃迁到主线程更新可观察状态。
@Observable
final class CallManager: NSObject, @unchecked Sendable {
    private(set) var hasActiveCall = false
    private(set) var activeCallUUID: UUID?

    var autoAnswer = true

    private let provider: CXProvider
    private let callController = CXCallController()
    private var onCallAnswered: (@MainActor (UUID) -> Void)?
    private var onCallEnded: (@MainActor (UUID) -> Void)?

    override init() {
        // In iOS 14+ `init(localizedName:)` is deprecated and `localizedName`
        // is read-only; CXProviderConfiguration reads the user-visible name
        // from the bundle's `CFBundleDisplayName` (set in Info.plist to
        // "OmniNova 通话助手") automatically.
        let config = CXProviderConfiguration()
        config.supportsVideo = false
        config.maximumCallsPerCallGroup = 1
        config.supportedHandleTypes = [.generic, .phoneNumber]
        self.provider = CXProvider(configuration: config)
        super.init()
        provider.setDelegate(self, queue: nil)
    }

    /// 回调被标注为 `@MainActor`，便于调用方（`@MainActor` 隔离的 SwiftUI
    /// App/View）直接在闭包内同步访问主线程隔离的成员函数。
    func configure(
        onCallAnswered: @escaping @MainActor (UUID) -> Void,
        onCallEnded: @escaping @MainActor (UUID) -> Void
    ) {
        self.onCallAnswered = onCallAnswered
        self.onCallEnded = onCallEnded
    }

    /// 向系统报告一通 VoIP 来电。通常由 PushKit 推送触发。
    /// 若 `autoAnswer == true`，报告完成后立即发起 `CXAnswerCallAction` 自动接听。
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
        activeCallUUID = uuid
        if autoAnswer {
            await autoAnswerCall(uuid: uuid)
        }
    }

    /// 通过 CXCallController 主动发起接听动作（仅 VoIP 来电可接管）。
    func autoAnswerCall(uuid: UUID) async {
        let action = CXAnswerCallAction(call: uuid)
        let transaction = CXTransaction(action: action)
        await withCheckedContinuation { (cont: CheckedContinuation<Void, Never>) in
            callController.request(transaction) { error in
                if let error {
                    print("[CallManager] auto answer failed: \(error)")
                }
                cont.resume()
            }
        }
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
