import AVFoundation
import Foundation
import Speech

/// App 内语音 → 文本（需麦克风与语音识别权限）。**不**访问运营商通话线路。
///
/// 仅以 `@Observable` 暴露给 SwiftUI；语音识别 SDK 的回调天然在后台线程派发，
/// 因此类不标记 `@MainActor`，由调用方（或我们内部的 `Task @MainActor`）
/// 负责在写入可观察状态时跃迁到主线程。
@Observable
final class SpeechPipeline: NSObject, @unchecked Sendable {
    var authorizationStatus: SFSpeechRecognizerAuthorizationStatus = .notDetermined
    var lastTranscript: String = ""
    var isListening: Bool = false

    private let audioEngine = AVAudioEngine()
    private var recognitionRequest: SFSpeechAudioBufferRecognitionRequest?
    private var recognitionTask: SFSpeechRecognitionTask?
    private let speechRecognizer: SFSpeechRecognizer?

    override init() {
        self.speechRecognizer = SFSpeechRecognizer(locale: Locale(identifier: "zh-CN"))
        super.init()
    }

    func refreshAuthorization() {
        authorizationStatus = SFSpeechRecognizer.authorizationStatus()
    }

    func requestAuthorization() async {
        await withCheckedContinuation { (cont: CheckedContinuation<Void, Never>) in
            SFSpeechRecognizer.requestAuthorization { [weak self] _ in
                // Re-bind `self` to a non-optional strong reference *before*
                // hopping to MainActor. Capturing the outer weak `self`
                // (a `var`) directly inside a Task body is rejected under
                // Swift 5.9 strict concurrency because the Task closure is
                // concurrently-executing and may only capture `let`s.
                guard let self else {
                    Task { @MainActor in cont.resume() }
                    return
                }
                Task { @MainActor in
                    self.refreshAuthorization()
                    cont.resume()
                }
            }
        }
    }

    /// 开始从麦克风采集并实时转写；结果通过 `onPartial` / `onFinal` 回调。
    /// 回调统一会被调度到 `@MainActor` 上执行，调用方可以在闭包内直接访问
    /// `ConversationLogStore` 等可观察对象（这些对象不再使用 `@MainActor`
    /// 类级别隔离，仅依赖 `@Observable` 的观察通知）。
    func startListening(
        onPartial: @escaping (String) -> Void,
        onFinal: @escaping (String) -> Void
    ) throws {
        guard let recognizer = speechRecognizer, recognizer.isAvailable else {
            throw NSError(domain: "SpeechPipeline", code: 1, userInfo: [NSLocalizedDescriptionKey: "Speech recognizer unavailable"])
        }
        recognitionTask?.cancel()
        recognitionTask = nil

        let session = AVAudioSession.sharedInstance()
        try session.setCategory(.record, mode: .measurement, options: [.duckOthers])
        try session.setActive(true, options: .notifyOthersOnDeactivation)

        recognitionRequest = SFSpeechAudioBufferRecognitionRequest()
        guard let recognitionRequest else { return }
        recognitionRequest.shouldReportPartialResults = true

        let inputNode = audioEngine.inputNode
        let format = inputNode.outputFormat(forBus: 0)
        inputNode.installTap(onBus: 0, bufferSize: 1024, format: format) { buffer, _ in
            recognitionRequest.append(buffer)
        }

        audioEngine.prepare()
        try audioEngine.start()
        isListening = true

        recognitionTask = recognizer.recognitionTask(with: recognitionRequest) { result, error in
            if let result {
                let text = result.bestTranscription.formattedString
                Task { @MainActor in
                    self.lastTranscript = text
                }
                if result.isFinal {
                    Task { @MainActor in onFinal(text) }
                } else {
                    Task { @MainActor in onPartial(text) }
                }
            }
            if error != nil {
                Task { @MainActor in
                    self.stopListening()
                }
            }
        }
    }

    func stopListening() {
        audioEngine.stop()
        audioEngine.inputNode.removeTap(onBus: 0)
        recognitionRequest?.endAudio()
        recognitionRequest = nil
        recognitionTask?.cancel()
        recognitionTask = nil
        isListening = false
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }
}
