import AVFoundation
import Foundation

/// 将 Agent 回复文本通过系统 TTS 播放给来电者。
@MainActor @Observable
final class AgentResponseSynthesizer: NSObject {
    private(set) var isSpeaking = false
    private let synth = AVSpeechSynthesizer()
    private var voiceIdentifier: String?

    override init() {
        super.init()
        synth.delegate = self
        selectBestVoice()
    }

    func speak(_ text: String) {
        synth.stopSpeaking(at: .immediate)
        let utterance = AVSpeechUtterance(string: text)
        utterance.rate = AVSpeechUtteranceDefaultSpeechRate
        utterance.pitchMultiplier = 1.05
        utterance.preUtteranceDelay = 0.1
        if let id = voiceIdentifier, let voice = AVSpeechSynthesisVoice(identifier: id) {
            utterance.voice = voice
        } else {
            utterance.voice = AVSpeechSynthesisVoice(language: "zh-CN")
        }
        configureAudioSession()
        synth.speak(utterance)
        isSpeaking = true
    }

    func stop() {
        synth.stopSpeaking(at: .immediate)
        isSpeaking = false
    }

    private func selectBestVoice() {
        let preferred = AVSpeechSynthesisVoice.speechVoices()
            .filter { $0.language.hasPrefix("zh") }
            .sorted { ($0.quality.rawValue, $0.name) > ($1.quality.rawValue, $1.name) }
        voiceIdentifier = preferred.first?.identifier
    }

    private func configureAudioSession() {
        let session = AVAudioSession.sharedInstance()
        try? session.setCategory(.playAndRecord, mode: .voiceChat, options: [.defaultToSpeaker, .allowBluetooth])
        try? session.setActive(true)
    }
}

extension AgentResponseSynthesizer: AVSpeechSynthesizerDelegate {
    nonisolated func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didFinish utterance: AVSpeechUtterance) {
        Task { @MainActor in isSpeaking = false }
    }
    nonisolated func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didCancel utterance: AVSpeechUtterance) {
        Task { @MainActor in isSpeaking = false }
    }
}
