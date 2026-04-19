import AVFoundation
import Foundation
import Speech

/// App 内语音 → 文本（需麦克风与语音识别权限）。**不**访问运营商通话线路。
@MainActor
final class SpeechPipeline: NSObject, ObservableObject {
    @Published var authorizationStatus: SFSpeechRecognizerAuthorizationStatus = .notDetermined
    @Published var lastTranscript: String = ""
    @Published var isListening: Bool = false

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
                Task { @MainActor in
                    self?.refreshAuthorization()
                    cont.resume()
                }
            }
        }
    }

    /// 开始从麦克风采集并实时转写；结果通过 `onPartial` / `onFinal` 回调。
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
                    onFinal(text)
                } else {
                    onPartial(text)
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
