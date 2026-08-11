package com.omninova.phoneagent.ui

import android.content.Context
import android.content.res.Configuration
import com.omninova.phoneagent.data.AppSettings
import java.util.Locale

/** Applies the saved in-app locale before activity resources are created. */
object AppLocale {
    fun wrap(base: Context): Context {
        val preferences = base.getSharedPreferences(AppSettings.PREFS_NAME, Context.MODE_PRIVATE)
        val languageTag = preferences.getString(
            AppSettings.KEY_LANGUAGE_TAG,
            AppSettings.DEFAULT_LANGUAGE_TAG,
        ) ?: AppSettings.DEFAULT_LANGUAGE_TAG
        return wrap(base, languageTag)
    }

    fun wrap(base: Context, languageTag: String): Context {
        val locale = Locale.forLanguageTag(languageTag)
        Locale.setDefault(locale)
        val configuration = Configuration(base.resources.configuration)
        configuration.setLocale(locale)
        return base.createConfigurationContext(configuration)
    }
}
