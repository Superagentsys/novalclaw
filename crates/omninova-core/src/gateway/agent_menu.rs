//! Shared data model for the OmniNova Agent menu.
//!
//! Channel adapters render this pure specification in their native format.
//! Feishu renders interactive buttons, while DingTalk currently renders a
//! structured text menu through its existing `sampleText` outbound path.

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
            "msgId",
            "messageId",
        ] {
            assert!(!rendered.contains(forbidden), "leaked {forbidden}");
        }
    }
}
