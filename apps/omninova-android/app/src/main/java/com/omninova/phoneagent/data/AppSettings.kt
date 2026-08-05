package com.omninova.phoneagent.data

import android.content.Context
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * Settings kept on the phone. SharedPreferences is intentionally used here so
 * Android services created outside the activity can read the same values.
 */
class AppSettings(context: Context) {
    private val preferences = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)

    private val _autoAnswerEnabled = MutableStateFlow(
        preferences.getBoolean(KEY_AUTO_ANSWER, false),
    )
    val autoAnswerEnabled: StateFlow<Boolean> = _autoAnswerEnabled.asStateFlow()

    private val _spamScreeningEnabled = MutableStateFlow(
        preferences.getBoolean(KEY_SPAM_SCREENING, true),
    )
    val spamScreeningEnabled: StateFlow<Boolean> = _spamScreeningEnabled.asStateFlow()

    private val _languageTag = MutableStateFlow(
        preferences.getString(KEY_LANGUAGE_TAG, DEFAULT_LANGUAGE_TAG) ?: DEFAULT_LANGUAGE_TAG,
    )
    val languageTag: StateFlow<String> = _languageTag.asStateFlow()

    fun setAutoAnswerEnabled(value: Boolean) {
        preferences.edit().putBoolean(KEY_AUTO_ANSWER, value).apply()
        _autoAnswerEnabled.value = value
    }

    fun setSpamScreeningEnabled(value: Boolean) {
        preferences.edit().putBoolean(KEY_SPAM_SCREENING, value).apply()
        _spamScreeningEnabled.value = value
    }

    fun setLanguageTag(value: String) {
        val supported = value.takeIf { it in SUPPORTED_LANGUAGE_TAGS } ?: DEFAULT_LANGUAGE_TAG
        preferences.edit().putString(KEY_LANGUAGE_TAG, supported).apply()
        _languageTag.value = supported
    }

    companion object {
        const val PREFS_NAME = "omninova_settings"
        const val KEY_LANGUAGE_TAG = "language_tag"
        const val DEFAULT_LANGUAGE_TAG = "zh-CN"
        val SUPPORTED_LANGUAGE_TAGS = setOf("zh-CN", "zh-TW", "en")

        private const val KEY_AUTO_ANSWER = "auto_answer_enabled"
        private const val KEY_SPAM_SCREENING = "spam_screening_enabled"
    }
}
