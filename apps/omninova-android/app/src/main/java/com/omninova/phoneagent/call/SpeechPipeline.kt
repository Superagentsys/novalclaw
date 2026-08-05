package com.omninova.phoneagent.call

import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.speech.RecognitionListener
import android.speech.RecognizerIntent
import android.speech.SpeechRecognizer
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/** Android system speech-recognition wrapper with visible, actionable errors. */
class SpeechPipeline(private val context: Context) {
    fun interface OnPartial { fun invoke(text: String) }
    fun interface OnFinal { fun invoke(text: String) }
    fun interface OnError { fun invoke(message: String) }

    private var recognizer: SpeechRecognizer? = null
    private var active = false

    private val _isListening = MutableStateFlow(false)
    val isListening: StateFlow<Boolean> = _isListening.asStateFlow()

    private val _lastTranscript = MutableStateFlow("")
    val lastTranscript: StateFlow<String> = _lastTranscript.asStateFlow()

    private val _lastError = MutableStateFlow<String?>(null)
    val lastError: StateFlow<String?> = _lastError.asStateFlow()

    fun isAvailable(): Boolean = SpeechRecognizer.isRecognitionAvailable(context)

    /** Returns false if Android cannot start a recognizer at all. */
    fun start(
        onPartial: OnPartial,
        onFinal: OnFinal,
        onError: OnError = OnError {},
    ): Boolean {
        stop()
        _lastError.value = null
        if (!isAvailable()) {
            reportError("此手机没有可用的系统语音识别服务。请在系统设置中启用语音输入，或使用文字测试。", onError)
            return false
        }

        return runCatching {
            active = true
            val created = SpeechRecognizer.createSpeechRecognizer(context)
            created.setRecognitionListener(object : RecognitionListener {
                    override fun onReadyForSpeech(params: Bundle?) {
                        _isListening.value = true
                    }

                    override fun onBeginningOfSpeech() = Unit
                    override fun onRmsChanged(rmsdB: Float) = Unit
                    override fun onBufferReceived(buffer: ByteArray?) = Unit
                    override fun onEndOfSpeech() = Unit

                    override fun onError(error: Int) {
                        if (!active) return
                        active = false
                        _isListening.value = false
                        reportError(recognitionErrorMessage(error), onError)
                        releaseRecognizer()
                    }

                    override fun onResults(results: Bundle?) {
                        if (!active) return
                        active = false
                        val text = results?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)
                            ?.firstOrNull().orEmpty()
                        if (text.isNotBlank()) {
                            _lastTranscript.value = text
                            onFinal.invoke(text)
                        } else {
                            reportError("没有识别到语音，请重试或使用文字测试。", onError)
                        }
                        _isListening.value = false
                        releaseRecognizer()
                    }

                    override fun onPartialResults(partialResults: Bundle?) {
                        val text = partialResults
                            ?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)
                            ?.firstOrNull().orEmpty()
                        if (active && text.isNotBlank()) {
                            _lastTranscript.value = text
                            onPartial.invoke(text)
                        }
                    }

                    override fun onEvent(eventType: Int, params: Bundle?) = Unit
            })

            val intent = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).apply {
                putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
                putExtra(RecognizerIntent.EXTRA_LANGUAGE, "zh-CN")
                putExtra(RecognizerIntent.EXTRA_PARTIAL_RESULTS, true)
                putExtra(RecognizerIntent.EXTRA_CALLING_PACKAGE, context.packageName)
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
                    putExtra(RecognizerIntent.EXTRA_REQUEST_WORD_CONFIDENCE, true)
                }
            }
            recognizer = created
            created.startListening(intent)
            true
        }.getOrElse { error ->
            active = false
            _isListening.value = false
            reportError(error.message ?: "无法启动系统语音识别，请使用文字测试。", onError)
            releaseRecognizer()
            false
        }
    }

    fun stop() {
        active = false
        runCatching {
            recognizer?.stopListening()
            recognizer?.destroy()
        }
        recognizer = null
        _isListening.value = false
    }

    private fun releaseRecognizer() {
        runCatching { recognizer?.destroy() }
        recognizer = null
    }

    private fun reportError(message: String, onError: OnError) {
        _lastError.value = message
        onError.invoke(message)
    }

    private fun recognitionErrorMessage(error: Int): String = when (error) {
        SpeechRecognizer.ERROR_INSUFFICIENT_PERMISSIONS -> "未获得麦克风权限。请在系统设置中允许麦克风权限。"
        SpeechRecognizer.ERROR_NETWORK, SpeechRecognizer.ERROR_NETWORK_TIMEOUT ->
            "系统语音识别网络不可用。请检查手机网络，或使用文字测试。"
        SpeechRecognizer.ERROR_NO_MATCH, SpeechRecognizer.ERROR_SPEECH_TIMEOUT ->
            "没有识别到语音，请靠近麦克风后重试，或使用文字测试。"
        SpeechRecognizer.ERROR_RECOGNIZER_BUSY -> "系统语音识别正在被其他应用使用，请稍后重试。"
        SpeechRecognizer.ERROR_SERVER_DISCONNECTED ->
            "系统语音识别服务已断开（code 11）。请在手机设置中启用或重启语音识别服务，然后重试；也可使用文字测试。"
        else -> "系统语音识别失败（code $error），请重试或使用文字测试。"
    }
}
