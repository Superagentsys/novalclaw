package com.omninova.phoneagent.ui

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.PhoneCallback
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import com.omninova.phoneagent.OmniNovaApp
import com.omninova.phoneagent.data.ConversationChannel
import com.omninova.phoneagent.data.ConversationSessionFile
import kotlinx.coroutines.launch
import java.util.UUID

class MainActivity : ComponentActivity() {

    private val requestPermissions = registerForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions()
    ) { /* no-op */ }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val app = application as OmniNovaApp
        setContent {
            MaterialTheme(colorScheme = lightColorScheme()) {
                OmniNovaScreen(
                    app = app,
                    onRequestPermissions = ::ensurePermissions,
                )
            }
        }
    }

    private fun ensurePermissions() {
        val perms = mutableListOf(
            Manifest.permission.RECORD_AUDIO,
            Manifest.permission.READ_PHONE_STATE,
            Manifest.permission.READ_CONTACTS,
        )
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            perms += Manifest.permission.ANSWER_PHONE_CALLS
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            perms += Manifest.permission.POST_NOTIFICATIONS
        }
        val missing = perms.filter {
            ContextCompat.checkSelfPermission(this, it) != PackageManager.PERMISSION_GRANTED
        }
        if (missing.isNotEmpty()) requestPermissions.launch(missing.toTypedArray())
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun OmniNovaScreen(
    app: OmniNovaApp,
    onRequestPermissions: () -> Unit,
) {
    val scope = rememberCoroutineScope()
    val connected by app.gateway.connected.collectAsState()
    val baseUrl by app.gateway.baseUrl.collectAsState()
    val isListening by app.speech.isListening.collectAsState()
    val sessions by app.logStore.sessions.collectAsState()

    var showSettings by remember { mutableStateOf(false) }
    var urlInput by remember { mutableStateOf(baseUrl) }
    var autoAnswer by remember { mutableStateOf(true) }
    var spamScreening by remember { mutableStateOf(true) }

    LaunchedEffect(Unit) { app.gateway.checkConnection() }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("OmniNova 通话助手") },
                actions = {
                    IconButton(onClick = { showSettings = true }) {
                        Icon(Icons.Filled.Settings, contentDescription = "设置")
                    }
                },
                navigationIcon = {
                    IconButton(onClick = {
                        startSimulatedCall(app, scope)
                    }) {
                        Icon(Icons.Filled.PhoneCallback, contentDescription = "模拟来电")
                    }
                }
            )
        }
    ) { padding ->
        Column(
            modifier = Modifier
                .padding(padding)
                .fillMaxSize()
        ) {
            StatusBanner(connected = connected, isListening = isListening)
            if (sessions.isEmpty()) {
                EmptyState()
            } else {
                SessionList(sessions = sessions)
            }
        }
    }

    if (showSettings) {
        SettingsSheet(
            urlInput = urlInput,
            onUrlChange = { urlInput = it },
            autoAnswer = autoAnswer,
            onAutoAnswerChange = { autoAnswer = it },
            spamScreening = spamScreening,
            onSpamScreeningChange = { spamScreening = it },
            onConnect = {
                app.gateway.configure(urlInput)
                scope.launch { app.gateway.checkConnection() }
            },
            onRequestPermissions = onRequestPermissions,
            onDismiss = { showSettings = false },
        )
    }
}

@Composable
private fun StatusBanner(connected: Boolean, isListening: Boolean) {
    Surface(tonalElevation = 2.dp, modifier = Modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Box(
                modifier = Modifier
                    .size(10.dp)
                    .padding(end = 4.dp)
            ) {
                Surface(
                    color = if (connected) Color(0xFF19C37D) else Color(0xFFFF9F1C),
                    shape = MaterialTheme.shapes.small,
                    modifier = Modifier.fillMaxSize()
                ) {}
            }
            Spacer(Modifier.width(8.dp))
            Text(
                text = if (connected) "网关已连接" else "网关未连接",
                style = MaterialTheme.typography.labelMedium
            )
            Spacer(Modifier.weight(1f))
            if (isListening) {
                AssistChip(onClick = {}, label = { Text("转写中") })
            }
        }
    }
}

@Composable
private fun EmptyState() {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(24.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        Text("暂无对话记录", fontWeight = FontWeight.SemiBold, fontSize = 18.sp)
        Spacer(Modifier.height(8.dp))
        Text("来电或模拟通话后将自动记录",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable
private fun SessionList(sessions: List<ConversationSessionFile>) {
    LazyColumn(modifier = Modifier.fillMaxSize()) {
        items(sessions.reversed()) { s ->
            ListItem(
                headlineContent = { Text("${s.turns.size} 轮对话") },
                supportingContent = {
                    val last = s.turns.lastOrNull()
                    Text(
                        text = if (last != null) "${last.role}: ${last.text}"
                        else "（无内容）",
                        maxLines = 1
                    )
                },
                overlineContent = {
                    Text("${s.channel.name} · ${s.startedAtUtc.take(19)}")
                }
            )
            HorizontalDivider()
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SettingsSheet(
    urlInput: String,
    onUrlChange: (String) -> Unit,
    autoAnswer: Boolean,
    onAutoAnswerChange: (Boolean) -> Unit,
    spamScreening: Boolean,
    onSpamScreeningChange: (Boolean) -> Unit,
    onConnect: () -> Unit,
    onRequestPermissions: () -> Unit,
    onDismiss: () -> Unit,
) {
    ModalBottomSheet(onDismissRequest = onDismiss) {
        Column(modifier = Modifier.padding(16.dp)) {
            Text("设置", fontWeight = FontWeight.Bold, fontSize = 20.sp)
            Spacer(Modifier.height(16.dp))
            OutlinedTextField(
                value = urlInput,
                onValueChange = onUrlChange,
                label = { Text("OmniNova 网关地址") },
                modifier = Modifier.fillMaxWidth()
            )
            Spacer(Modifier.height(8.dp))
            Button(onClick = onConnect, modifier = Modifier.fillMaxWidth()) {
                Text("连接")
            }
            Spacer(Modifier.height(16.dp))
            SwitchRow(
                label = "自动接听 VoIP 通话",
                checked = autoAnswer,
                onCheckedChange = onAutoAnswerChange,
            )
            SwitchRow(
                label = "启用骚扰识别",
                checked = spamScreening,
                onCheckedChange = onSpamScreeningChange,
            )
            Spacer(Modifier.height(12.dp))
            OutlinedButton(onClick = onRequestPermissions, modifier = Modifier.fillMaxWidth()) {
                Text("请求通话与麦克风权限")
            }
            Spacer(Modifier.height(24.dp))
            Text("OmniNova Phone Agent v0.1.0",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

@Composable
private fun SwitchRow(label: String, checked: Boolean, onCheckedChange: (Boolean) -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Text(label, modifier = Modifier.weight(1f))
        Switch(checked = checked, onCheckedChange = onCheckedChange)
    }
}

private fun startSimulatedCall(
    app: OmniNovaApp,
    scope: kotlinx.coroutines.CoroutineScope,
) {
    val sessionId = UUID.randomUUID().toString()
    app.logStore.startSession(sessionId, ConversationChannel.SIMULATED)
    app.speech.start(
        onPartial = { t ->
            app.logStore.appendTurn(sessionId, "caller", t, isFinal = false)
        },
        onFinal = { transcript ->
            app.logStore.appendTurn(sessionId, "caller", transcript, isFinal = true)
            scope.launch {
                val reply = app.gateway.chat(
                    text = transcript,
                    sessionId = sessionId,
                    channel = "simulated",
                ) ?: "（网关未连接，示例回复）好的，已记录您的请求。"
                app.logStore.appendTurn(sessionId, "agent", reply, isFinal = true)
                app.tts.speak(reply)
            }
        }
    )
}
