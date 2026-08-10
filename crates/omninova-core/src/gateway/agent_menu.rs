//! Shared data model for the OmniNova Agent menu.
//!
//! Channel adapters render this pure specification in their native format.
//! Feishu renders interactive buttons. DingTalk advanced cards use the same
//! specification, while the structured-text and legacy ActionCard renderers
//! remain available as compatibility fallbacks.

pub const DINGTALK_MENU_CARD_CALLBACK_PATH: &str = "/api/v1/gateway/dingtalk/card/callback";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentMenuActionStyle {
    Primary,
    Default,
}

impl AgentMenuActionStyle {
    pub fn as_feishu_button_type(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Default => "default",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentMenuAction {
    pub label: &'static str,
    pub action: &'static str,
    pub style: AgentMenuActionStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentMenuPanel {
    pub title: &'static str,
    pub introduction: &'static str,
    pub ordinary_chat_label: &'static str,
    pub primary_actions: &'static [AgentMenuAction],
    pub secondary_actions: &'static [AgentMenuAction],
    pub safety_note: &'static str,
}

const PRIMARY_ACTIONS: &[AgentMenuAction] = &[
    AgentMenuAction {
        label: "桌面监控 30 秒",
        action: "monitor_30s",
        style: AgentMenuActionStyle::Primary,
    },
    AgentMenuAction {
        label: "桌面监控 60 秒",
        action: "monitor_60s",
        style: AgentMenuActionStyle::Primary,
    },
];

const SECONDARY_ACTIONS: &[AgentMenuAction] = &[
    AgentMenuAction {
        label: "Gateway 状态",
        action: "gateway_status",
        style: AgentMenuActionStyle::Default,
    },
    AgentMenuAction {
        label: "最近任务",
        action: "recent_jobs",
        style: AgentMenuActionStyle::Default,
    },
    AgentMenuAction {
        label: "帮助说明",
        action: "help",
        style: AgentMenuActionStyle::Default,
    },
];

/// Return the canonical cross-channel Agent menu definition.
pub fn build_agent_menu_panel() -> AgentMenuPanel {
    AgentMenuPanel {
        title: "OmniNova Agent 功能菜单",
        introduction:
            "请选择要执行的操作。普通聊天可以直接发送文字；工具任务请使用按钮或 slash 命令。",
        ordinary_chat_label: "🟢 普通聊天说明",
        primary_actions: PRIMARY_ACTIONS,
        secondary_actions: SECONDARY_ACTIONS,
        safety_note: "高风险工具不在普通聊天中直接执行。",
    }
}

/// Render the shared menu through DingTalk's existing plain-text outbound.
///
/// DingTalk does not currently expose a stable action callback in this flow,
/// so the Feishu actions are intentionally presented as menu entries rather
/// than pretending to be clickable buttons.
pub fn render_agent_menu_as_dingtalk_text() -> String {
    let panel = build_agent_menu_panel();
    let mut lines = vec![
        panel.title.to_string(),
        String::new(),
        panel
            .ordinary_chat_label
            .trim_start_matches("🟢 ")
            .to_string(),
        "直接发送文字即可与 Agent 对话。".to_string(),
        String::new(),
        "功能项（当前 DingTalk 以结构化文本展示，不是可点击按钮）：".to_string(),
    ];

    for action in panel.primary_actions.iter().chain(panel.secondary_actions) {
        lines.push(format!("- {}", action.label));
    }

    lines.extend([
        String::new(),
        "菜单命令：menu / /menu / 菜单 / panel / /panel / 面板 / help / 帮助".to_string(),
        "状态命令：status / /status / 状态".to_string(),
        String::new(),
        panel.safety_note.to_string(),
    ]);

    lines.join("\n")
}

/// Build the DingTalk `sampleActionCard5` message fields from the shared menu.
///
/// The returned value deliberately contains only `msgKey` and the JSON-string
/// `msgParam`. The worker adds `robotCode` and `openConversationId` immediately
/// before sending, so platform identifiers never enter the reusable renderer.
pub fn build_dingtalk_agent_menu_card_payload(callback_url: &str) -> serde_json::Value {
    let panel = build_agent_menu_panel();
    let actions = panel
        .primary_actions
        .iter()
        .chain(panel.secondary_actions)
        .collect::<Vec<_>>();
    debug_assert_eq!(actions.len(), 5);

    let mut params = serde_json::json!({
        "title": panel.title,
        "text": format!(
            "{}\n\n{}\n\n{}",
            panel.introduction, panel.ordinary_chat_label, panel.safety_note
        ),
    });

    for (index, action) in actions.iter().enumerate() {
        let field_number = index + 1;
        params[format!("actionTitle{field_number}")] = serde_json::json!(action.label);
        params[format!("actionURL{field_number}")] =
            serde_json::json!(dingtalk_action_callback_url(callback_url, action.action));
    }

    serde_json::json!({
        "msgKey": "sampleActionCard5",
        "msgParam": params.to_string(),
    })
}

/// Compatibility alias matching the renderer naming used by the channel
/// adapters. Keeping both names makes the payload contract explicit in tests.
pub fn render_agent_menu_as_dingtalk_card(callback_url: &str) -> serde_json::Value {
    build_dingtalk_agent_menu_card_payload(callback_url)
}

/// Render the shared menu as the standard ActionCard accepted by the
/// sessionWebhook supplied with an inbound DingTalk app-bot message.
pub fn render_agent_menu_as_dingtalk_session_action_card(callback_url: &str) -> serde_json::Value {
    let panel = build_agent_menu_panel();
    let buttons = panel
        .primary_actions
        .iter()
        .chain(panel.secondary_actions)
        .map(|action| {
            serde_json::json!({
                "title": action.label,
                "actionURL": dingtalk_action_callback_url(callback_url, action.action),
            })
        })
        .collect::<Vec<_>>();

    serde_json::json!({
        "msgtype": "actionCard",
        "actionCard": {
            "title": panel.title,
            "text": format!(
                "{}\n\n{}\n直接发送文字即可与 Agent 对话。\n\n{}",
                panel.introduction, panel.ordinary_chat_label, panel.safety_note
            ),
            "btnOrientation": "0",
            "btns": buttons,
        }
    })
}

fn dingtalk_action_callback_url(callback_url: &str, action: &str) -> String {
    let separator = if callback_url.contains('?') { '&' } else { '?' };
    format!(
        "{}{separator}action={action}",
        callback_url.trim_end_matches('/')
    )
}

/// Map DingTalk-specific action aliases onto the canonical action keys shared
/// with Feishu. No platform keeps an unrelated second action vocabulary.
pub fn canonical_agent_menu_action(action: &str) -> Option<&'static str> {
    match action.trim() {
        "monitor_30s" | "desktop_monitor_30" => Some("monitor_30s"),
        "monitor_60s" | "desktop_monitor_60" => Some("monitor_60s"),
        "gateway_status" => Some("gateway_status"),
        "recent_jobs" | "recent_tasks" => Some("recent_jobs"),
        "help" => Some("help"),
        _ => None,
    }
}

/// Extract a DingTalk card action without retaining or logging any sender,
/// conversation, robot, or message identifiers.
pub fn extract_dingtalk_agent_menu_action(payload: &serde_json::Value) -> Option<String> {
    const PATHS: &[&[&str]] = &[
        &["action"],
        &["actionKey"],
        &["actionId"],
        &["params", "action"],
        &["params", "actionKey"],
        &["value", "action"],
        &["value", "actionKey"],
        &["data", "action"],
        &["data", "actionKey"],
        &["cardPrivateData", "action"],
    ];

    PATHS.iter().find_map(|path| {
        path.iter()
            .try_fold(payload, |value, segment| value.get(*segment))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_menu_contains_existing_feishu_items() {
        let panel = build_agent_menu_panel();
        let labels = panel
            .primary_actions
            .iter()
            .chain(panel.secondary_actions)
            .map(|item| item.label)
            .collect::<Vec<_>>();

        assert_eq!(panel.title, "OmniNova Agent 功能菜单");
        assert_eq!(
            labels,
            vec![
                "桌面监控 30 秒",
                "桌面监控 60 秒",
                "Gateway 状态",
                "最近任务",
                "帮助说明",
            ]
        );
        assert!(panel.safety_note.contains("高风险工具"));
    }

    #[test]
    fn dingtalk_menu_is_structured_text_without_sensitive_fields() {
        let rendered = render_agent_menu_as_dingtalk_text();
        for expected in [
            "OmniNova Agent 功能菜单",
            "普通聊天说明",
            "桌面监控 30 秒",
            "桌面监控 60 秒",
            "Gateway 状态",
            "最近任务",
            "帮助说明",
            "高风险工具不在普通聊天中直接执行",
        ] {
            assert!(rendered.contains(expected), "missing {expected:?}");
        }
        for forbidden in [
            "app_secret",
            "access_token",
            "sessionWebhook",
            "robotCode",
            "conversationId",
            "openConversationId",
            "msgId",
            "messageId",
        ] {
            assert!(!rendered.contains(forbidden), "leaked {forbidden}");
        }
    }

    #[test]
    fn dingtalk_card_payload_contains_five_shared_actions_and_string_msg_param() {
        let payload = build_dingtalk_agent_menu_card_payload(
            "https://gateway.example.test/api/v1/gateway/dingtalk/card/callback",
        );
        assert_eq!(payload["msgKey"], "sampleActionCard5");
        let msg_param = payload["msgParam"]
            .as_str()
            .expect("msgParam must be a JSON string");
        let params: serde_json::Value = serde_json::from_str(msg_param).unwrap();

        for (index, expected) in [
            "桌面监控 30 秒",
            "桌面监控 60 秒",
            "Gateway 状态",
            "最近任务",
            "帮助说明",
        ]
        .iter()
        .enumerate()
        {
            let number = index + 1;
            assert_eq!(params[format!("actionTitle{number}")], *expected);
            assert!(params[format!("actionURL{number}")]
                .as_str()
                .is_some_and(|url| url.contains("/dingtalk/card/callback?action=")));
        }
    }

    #[test]
    fn dingtalk_card_payload_contains_no_sensitive_identifiers() {
        let payload = render_agent_menu_as_dingtalk_card(
            "https://gateway.example.test/api/v1/gateway/dingtalk/card/callback",
        );
        let serialized = payload.to_string();
        for forbidden in [
            "app_secret",
            "access_token",
            "sessionWebhook",
            "robotCode",
            "conversationId",
            "openConversationId",
            "msgId",
            "messageId",
        ] {
            assert!(!serialized.contains(forbidden), "leaked {forbidden}");
        }
    }

    #[test]
    fn dingtalk_session_webhook_action_card_contains_five_shared_actions() {
        let payload = render_agent_menu_as_dingtalk_session_action_card(
            "https://gateway.example.test/api/v1/gateway/dingtalk/card/callback",
        );
        assert_eq!(payload["msgtype"], "actionCard");
        assert_eq!(payload["actionCard"]["title"], "OmniNova Agent 功能菜单");
        assert_eq!(payload["actionCard"]["btnOrientation"], "0");
        let buttons = payload["actionCard"]["btns"].as_array().unwrap();
        assert_eq!(buttons.len(), 5);
        for (button, expected) in buttons.iter().zip([
            ("桌面监控 30 秒", "monitor_30s"),
            ("桌面监控 60 秒", "monitor_60s"),
            ("Gateway 状态", "gateway_status"),
            ("最近任务", "recent_jobs"),
            ("帮助说明", "help"),
        ]) {
            assert_eq!(button["title"], expected.0);
            assert!(button["actionURL"]
                .as_str()
                .is_some_and(|url| url.ends_with(&format!("?action={}", expected.1))));
        }

        let serialized = payload.to_string();
        for forbidden in [
            "app_secret",
            "access_token",
            "sessionWebhook",
            "robotCode",
            "conversationId",
            "openConversationId",
            "msgId",
            "messageId",
        ] {
            assert!(!serialized.contains(forbidden), "leaked {forbidden}");
        }
    }

    #[test]
    fn dingtalk_action_extractor_and_aliases_use_canonical_keys() {
        for (payload, expected) in [
            (
                serde_json::json!({"action": "gateway_status"}),
                "gateway_status",
            ),
            (
                serde_json::json!({"params": {"actionKey": "recent_tasks"}}),
                "recent_jobs",
            ),
            (serde_json::json!({"value": {"action": "help"}}), "help"),
            (
                serde_json::json!({"actionId": "desktop_monitor_30"}),
                "monitor_30s",
            ),
            (
                serde_json::json!({"data": {"action": "desktop_monitor_60"}}),
                "monitor_60s",
            ),
        ] {
            let extracted = extract_dingtalk_agent_menu_action(&payload).unwrap();
            assert_eq!(canonical_agent_menu_action(&extracted), Some(expected));
        }
    }
}
