package com.omninova.phoneagent.ui

import android.Manifest
import android.app.Activity
import android.app.role.RoleManager
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.PhoneCallback
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import com.omninova.phoneagent.OmniNovaApp
import com.omninova.phoneagent.R
import com.omninova.phoneagent.data.ConversationChannel
import com.omninova.phoneagent.data.ConversationSessionFile
import java.util.UUID

class MainActivity : ComponentActivity() {
    private var pendingAfterPermissions: (() -> Unit)? = null

    private val requestPermissions = registerForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions(),
    ) { results ->
        val microphoneGranted = results[Manifest.permission.RECORD_AUDIO] == true ||
            ContextCompat.checkSelfPermission(this, Manifest.permission.RECORD_AUDIO) ==
            PackageManager.PERMISSION_GRANTED
        val action = pendingAfterPermissions
        pendingAfterPermissions = null
        if (microphoneGranted) {
            action?.invoke()
        } else {
            Toast.makeText(this, getString(R.string.permission_microphone_required), Toast.LENGTH_LONG).show()
        }
    }

    private val requestScreeningRole = registerForActivityResult(
        ActivityResultContracts.StartActivityForResult(),
    ) { result ->
        val message = if (result.resultCode == Activity.RESULT_OK) {
            getString(R.string.screening_enabled)
        } else {
            getString(R.string.screening_not_enabled)
        }
        Toast.makeText(this, message, Toast.LENGTH_LONG).show()
    }

    override fun attachBaseContext(newBase: Context) {
        super.attachBaseContext(AppLocale.wrap(newBase))
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val app = application as OmniNovaApp
        setContent {
            MaterialTheme(colorScheme = lightColorScheme()) {
                OmniNovaScreen(
                    app = app,
                    onRequestPermissions = { ensurePermissions() },
                    onStartVoiceTest = { ensurePermissions { startSimulatedCall(app) } },
                    onRequestScreeningRole = ::requestCallScreeningRole,
                    onLanguageChange = { tag ->
                        app.settings.setLanguageTag(tag)
                        recreate()
                    },
                )
            }
        }
    }

    private fun ensurePermissions(afterGranted: (() -> Unit)? = null) {
        val permissions = mutableListOf(
            Manifest.permission.RECORD_AUDIO,
            Manifest.permission.READ_PHONE_STATE,
            Manifest.permission.READ_CONTACTS,
        )
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            permissions += Manifest.permission.ANSWER_PHONE_CALLS
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            permissions += Manifest.permission.POST_NOTIFICATIONS
        }
        val missing = permissions.filter {
            ContextCompat.checkSelfPermission(this, it) != PackageManager.PERMISSION_GRANTED
        }
        if (missing.isEmpty()) {
            afterGranted?.invoke()
        } else {
            pendingAfterPermissions = afterGranted
            requestPermissions.launch(missing.toTypedArray())
        }
    }

    private fun requestCallScreeningRole() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) {
            Toast.makeText(this, getString(R.string.screening_android_10), Toast.LENGTH_LONG).show()
            return
        }
        val roleManager = getSystemService(RoleManager::class.java)
        if (!roleManager.isRoleAvailable(RoleManager.ROLE_CALL_SCREENING)) {
            Toast.makeText(this, getString(R.string.screening_not_supported), Toast.LENGTH_LONG).show()
            return
        }
        if (roleManager.isRoleHeld(RoleManager.ROLE_CALL_SCREENING)) {
            Toast.makeText(this, getString(R.string.screening_already_enabled), Toast.LENGTH_SHORT).show()
            return
        }
        requestScreeningRole.launch(roleManager.createRequestRoleIntent(RoleManager.ROLE_CALL_SCREENING))
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun OmniNovaScreen(
    app: OmniNovaApp,
    onRequestPermissions: () -> Unit,
    onStartVoiceTest: () -> Unit,
    onRequestScreeningRole: () -> Unit,
    onLanguageChange: (String) -> Unit,
) {
    val isListening by app.speech.isListening.collectAsState()
    val speechError by app.speech.lastError.collectAsState()
    val sessions by app.logStore.sessions.collectAsState()
    val autoAnswer by app.settings.autoAnswerEnabled.collectAsState()
    val spamScreening by app.settings.spamScreeningEnabled.collectAsState()
    val languageTag by app.settings.languageTag.collectAsState()
    var showSettings by remember { mutableStateOf(false) }
    var textInput by rememberSaveable { mutableStateOf("") }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.app_name)) },
                actions = {
                    IconButton(onClick = { showSettings = true }) {
                        Icon(Icons.Filled.Settings, contentDescription = stringResource(R.string.content_settings))
                    }
                },
                navigationIcon = {
                    IconButton(onClick = onStartVoiceTest) {
                        Icon(
                            Icons.Filled.PhoneCallback,
                            contentDescription = stringResource(R.string.content_start_voice_test),
                        )
                    }
                },
            )
        },
    ) { padding ->
        Column(
            modifier = Modifier
                .padding(padding)
                .fillMaxSize(),
        ) {
            StatusBanner(isListening = isListening, speechError = speechError)
            LocalTextTest(
                value = textInput,
                onValueChange = { textInput = it },
                onSubmit = {
                    submitLocalText(app, textInput)
                    textInput = ""
                },
            )
            if (sessions.isEmpty()) {
                EmptyState(onStartVoiceTest = onStartVoiceTest, modifier = Modifier.weight(1f))
            } else {
                SessionList(sessions = sessions, modifier = Modifier.weight(1f))
            }
        }
    }

    if (showSettings) {
        SettingsSheet(
            autoAnswer = autoAnswer,
            onAutoAnswerChange = app.settings::setAutoAnswerEnabled,
            spamScreening = spamScreening,
            onSpamScreeningChange = app.settings::setSpamScreeningEnabled,
            onRequestPermissions = onRequestPermissions,
            onRequestScreeningRole = onRequestScreeningRole,
            languageTag = languageTag,
            onLanguageChange = onLanguageChange,
            onDismiss = { showSettings = false },
        )
    }
}

@Composable
private fun StatusBanner(isListening: Boolean, speechError: String?) {
    Surface(tonalElevation = 2.dp, modifier = Modifier.fillMaxWidth()) {
        Column(modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Surface(
                    color = Color(0xFF19C37D),
                    shape = MaterialTheme.shapes.small,
                    modifier = Modifier.size(10.dp),
                ) {}
                Spacer(Modifier.width(8.dp))
                Text(
                    text = stringResource(R.string.status_local_ready),
                    style = MaterialTheme.typography.labelMedium,
                )
                Spacer(Modifier.weight(1f))
                if (isListening) {
                    AssistChip(onClick = {}, label = { Text(stringResource(R.string.listening)) })
                }
            }
            Text(
                stringResource(R.string.status_local_hint),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            speechError?.let { error ->
                Spacer(Modifier.height(4.dp))
                Text(error, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error)
            }
        }
    }
}

@Composable
private fun LocalTextTest(value: String, onValueChange: (String) -> Unit, onSubmit: () -> Unit) {
    Column(modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp)) {
        OutlinedTextField(
            value = value,
            onValueChange = onValueChange,
            label = { Text(stringResource(R.string.local_text_label)) },
            supportingText = { Text(stringResource(R.string.local_text_hint)) },
            modifier = Modifier.fillMaxWidth(),
            minLines = 1,
            maxLines = 3,
        )
        Spacer(Modifier.height(8.dp))
        Button(onClick = onSubmit, enabled = value.isNotBlank(), modifier = Modifier.fillMaxWidth()) {
            Text(stringResource(R.string.local_text_submit))
        }
    }
}

@Composable
private fun EmptyState(onStartVoiceTest: () -> Unit, modifier: Modifier = Modifier) {
    Column(
        modifier = modifier
            .fillMaxWidth()
            .padding(24.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(stringResource(R.string.no_conversations), fontWeight = FontWeight.SemiBold, fontSize = 18.sp)
        Spacer(Modifier.height(8.dp))
        Text(
            stringResource(R.string.no_conversations_hint),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(Modifier.height(16.dp))
        Button(onClick = onStartVoiceTest) {
            Text(stringResource(R.string.start_voice_test))
        }
    }
}

@Composable
private fun SessionList(sessions: List<ConversationSessionFile>, modifier: Modifier = Modifier) {
    LazyColumn(modifier = modifier.fillMaxWidth()) {
        items(sessions.reversed(), key = { it.sessionId }) { session ->
            ListItem(
                headlineContent = { Text(stringResource(R.string.conversation_turns, session.turns.size)) },
                supportingContent = {
                    val last = session.turns.lastOrNull()
                    Text(
                        text = if (last != null) "${last.role}: ${last.text}"
                        else stringResource(R.string.no_content),
                        maxLines = 1,
                    )
                },
                overlineContent = { Text("${session.channel.name} · ${session.startedAtUtc.take(19)}") },
            )
            HorizontalDivider()
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SettingsSheet(
    autoAnswer: Boolean,
    onAutoAnswerChange: (Boolean) -> Unit,
    spamScreening: Boolean,
    onSpamScreeningChange: (Boolean) -> Unit,
    onRequestPermissions: () -> Unit,
    onRequestScreeningRole: () -> Unit,
    languageTag: String,
    onLanguageChange: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    ModalBottomSheet(onDismissRequest = onDismiss) {
        Column(modifier = Modifier.padding(16.dp)) {
            Text(stringResource(R.string.settings_title), fontWeight = FontWeight.Bold, fontSize = 20.sp)
            Spacer(Modifier.height(16.dp))
            Text(stringResource(R.string.language_title), fontWeight = FontWeight.SemiBold)
            Spacer(Modifier.height(8.dp))
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                LanguageButton(
                    label = stringResource(R.string.language_simplified),
                    selected = languageTag == "zh-CN",
                    onClick = { onLanguageChange("zh-CN") },
                )
                LanguageButton(
                    label = stringResource(R.string.language_traditional),
                    selected = languageTag == "zh-TW",
                    onClick = { onLanguageChange("zh-TW") },
                )
                LanguageButton(
                    label = stringResource(R.string.language_english),
                    selected = languageTag == "en",
                    onClick = { onLanguageChange("en") },
                )
            }
            Spacer(Modifier.height(16.dp))
            Text(stringResource(R.string.local_mode_title), fontWeight = FontWeight.SemiBold)
            Text(
                stringResource(R.string.local_mode_description),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Spacer(Modifier.height(16.dp))
            SwitchRow(
                label = stringResource(R.string.auto_answer),
                checked = autoAnswer,
                onCheckedChange = onAutoAnswerChange,
            )
            Text(
                stringResource(R.string.auto_answer_hint),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Spacer(Modifier.height(8.dp))
            SwitchRow(
                label = stringResource(R.string.spam_screening),
                checked = spamScreening,
                onCheckedChange = onSpamScreeningChange,
            )
            OutlinedButton(onClick = onRequestScreeningRole, modifier = Modifier.fillMaxWidth()) {
                Text(stringResource(R.string.enable_screening))
            }
            Spacer(Modifier.height(12.dp))
            OutlinedButton(onClick = onRequestPermissions, modifier = Modifier.fillMaxWidth()) {
                Text(stringResource(R.string.grant_permissions))
            }
            Spacer(Modifier.height(24.dp))
            Text(
                stringResource(R.string.version),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun RowScope.LanguageButton(label: String, selected: Boolean, onClick: () -> Unit) {
    OutlinedButton(onClick = onClick, enabled = !selected, modifier = Modifier.weight(1f)) {
        Text(label, maxLines = 1, style = MaterialTheme.typography.labelSmall)
    }
}

@Composable
private fun SwitchRow(label: String, checked: Boolean, onCheckedChange: (Boolean) -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, modifier = Modifier.weight(1f))
        Switch(checked = checked, onCheckedChange = onCheckedChange)
    }
}

private fun submitLocalText(app: OmniNovaApp, text: String) {
    val sessionId = UUID.randomUUID().toString()
    app.logStore.startSession(sessionId, ConversationChannel.IN_APP_VOICE)
    app.logStore.appendTurn(sessionId, "caller", text.trim(), isFinal = true)
    val reply = app.localAgent.reply(text, app.settings.languageTag.value)
    app.logStore.appendTurn(sessionId, "agent", reply, isFinal = true)
    app.logStore.updateMetadata(sessionId, mapOf("processing" to "on_device"))
    app.logStore.endSession(sessionId)
    app.tts.speak(reply)
}

private fun startSimulatedCall(app: OmniNovaApp) {
    var sessionId: String? = null
    fun ensureSession(): String {
        return sessionId ?: UUID.randomUUID().toString().also { id ->
            sessionId = id
            app.logStore.startSession(id, ConversationChannel.SIMULATED)
        }
    }

    app.speech.start(
        onPartial = { text ->
            app.logStore.appendTurn(ensureSession(), "caller", text, isFinal = false)
        },
        onFinal = { transcript ->
            val id = ensureSession()
            app.logStore.appendTurn(id, "caller", transcript, isFinal = true)
            val reply = app.localAgent.reply(transcript, app.settings.languageTag.value)
            app.logStore.appendTurn(id, "agent", reply, isFinal = true)
            app.logStore.updateMetadata(id, mapOf("processing" to "on_device"))
            app.logStore.endSession(id)
            app.tts.speak(reply)
        },
        onError = {
            sessionId?.let { id ->
                if (app.logStore.session(id)?.turns.isNullOrEmpty()) app.logStore.discardSession(id)
            }
        },
    )
}
