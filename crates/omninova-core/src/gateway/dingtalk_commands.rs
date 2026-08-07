//! DingTalk text command router (Phase 2).
//!
//! Commands are short, deterministic phrases that the DingTalk worker
//! handles directly without going through the Agent. Non-command text
//! continues to flow into the Agent exactly like Phase 1.
//!
//! Supported (case-insensitive, after `@bot` mention stripping):
//! - `help`, `帮助`     -> shared Agent menu
//! - `menu`, `菜单`, `panel`, `面板` -> shared Agent menu
//! - `status`, `状态`   -> Status (redacted: no secrets/tokens/IDs)
//! - `ping`             -> Ping
//! - `monitor`          -> explicitly unsupported in Phase 2
//!
//! All command variants also accept a leading `/` (e.g. `/help`).

use crate::config::Config;
use crate::gateway::agent_menu::render_agent_menu_as_dingtalk_text;

/// All commands the DingTalk worker understands in Phase 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DingtalkCommand {
    Help,
    Menu,
    Status,
    Ping,
    /// `monitor` and variants. Always replies with a not-available notice
    /// rather than calling half-baked internals.
    Monitor,
}

impl DingtalkCommand {
    pub fn name(self) -> &'static str {
        match self {
            DingtalkCommand::Help => "help",
            DingtalkCommand::Menu => "menu",
            DingtalkCommand::Status => "status",
            DingtalkCommand::Ping => "ping",
            DingtalkCommand::Monitor => "monitor",
        }
    }
}

/// Strip a leading `@bot` mention from a DingTalk text message so that
/// commands work whether or not the user invoked the bot explicitly.
///
/// DingTalk group messages look like `"@机器人名 hello"` (the bot-name
/// token may contain spaces / Chinese characters). For Phase 2 we strip
/// a single leading `@…` token followed by optional whitespace.
///
/// `raw_text`   : the `text` field from the inbound payload as it is
///                currently set on `InboundMessage.text`.
/// `payload`    : the parsed JSON payload; we look at
///                `text.content` (the real DingTalk shape) if present and
///                fall back to `raw_text`.
pub fn normalize_dingtalk_command_text(
    raw_text: &str,
    payload: Option<&serde_json::Value>,
) -> String {
    // Prefer the real DingTalk `text.content` field if we have the raw
    // payload. Phase 1 already copies the raw_payload into metadata, so
    // tests can pass it through without depending on the optional fix.
    let text_owned;
    let text = if let Some(p) = payload {
        if let Some(content) = p
            .get("text")
            .and_then(|v| v.get("content"))
            .and_then(|v| v.as_str())
        {
            text_owned = content.to_string();
            &text_owned
        } else if let Some(s) = p.get("text").and_then(|v| v.as_str()) {
            text_owned = s.to_string();
            &text_owned
        } else {
            raw_text
        }
    } else {
        raw_text
    };

    strip_bot_mention(text).trim().to_string()
}

/// Strip a leading `@<token> ` from the input. This is deliberately
/// conservative: only one mention is removed, only at the start, and
/// the mention token may include Chinese characters.
///
/// Rules:
/// - If the string does not start with `@`, return it unchanged (after
///   trim).
/// - If the string is exactly `@` or `@<spaces>`, return empty.
/// - If the string is `@<token>` with no whitespace after, return empty
///   (the user typed only the mention).
/// - If the string is `@<token> <rest>` (token followed by whitespace),
///   return `<rest>` trimmed.
pub fn strip_bot_mention(text: &str) -> String {
    let trimmed = text.trim();
    let rest = match trimmed.strip_prefix('@') {
        Some(r) => r,
        None => return trimmed.to_string(),
    };

    // Everything up to (but not including) the first whitespace char is
    // the mention token. If there is no whitespace at all, the user
    // typed only the mention — return empty.
    match rest.find(char::is_whitespace) {
        Some(idx) => rest[idx..].trim().to_string(),
        None => String::new(),
    }
}

/// Normalize the input text for command matching: case-fold the ASCII
/// portion (Chinese characters are unaffected), trim, and drop a single
/// leading `/`. We do **not** mutate the user's text for echoing —
/// `parse_dingtalk_command` operates on this normalized view only.
pub fn to_normalized_for_match(text: &str) -> String {
    let mut s = text.trim().to_string();
    if let Some(rest) = s.strip_prefix('/') {
        s = rest.trim().to_string();
    }
    // ASCII case-fold; non-ASCII stays as-is.
    s.to_ascii_lowercase()
}

/// Try to interpret `normalized` text as a DingTalk command. Returns
/// `None` for anything we do not recognize — that means "let the Agent
/// handle it."
pub fn parse_dingtalk_command(normalized: &str) -> Option<DingtalkCommand> {
    match normalized {
        "help" | "帮助" | "?" => Some(DingtalkCommand::Help),
        "menu" | "菜单" | "panel" | "面板" => Some(DingtalkCommand::Menu),
        "status" | "状态" => Some(DingtalkCommand::Status),
        "ping" | "pong" => Some(DingtalkCommand::Ping),
        "monitor" => Some(DingtalkCommand::Monitor),
        _ => None,
    }
}

/// Reply text for `/help`. Public so tests can assert on the exact
/// wording and so the integration path doesn't depend on a hidden
/// format.
pub fn build_dingtalk_help_text() -> String {
    render_agent_menu_as_dingtalk_text()
}

/// Reply text for `/menu`. The menu mirrors the help list so users on
/// either entry point see the same set of commands.
pub fn build_dingtalk_menu_text() -> String {
    render_agent_menu_as_dingtalk_text()
}

/// Inputs needed to render status without leaking secrets.
#[derive(Debug, Clone, Copy)]
pub struct DingtalkStatusInputs<'a> {
    pub config: &'a Config,
    /// Whether the async DingTalk worker has been initialized.
    pub worker_initialized: bool,
    /// Current backlog of the async job queue (0 when worker is off).
    pub queue_len: usize,
}

/// Render the `/status` reply. **Must not** contain any of:
/// app_secret, app_key, robot_code, access_token, sessionWebhook,
/// senderStaffId, conversationId, messageId, msgId.
///
/// Allowed tokens: `enabled`, `initialized`, `configured`, `present`,
/// `count`, `true`, `false`, `<number>`.
pub fn build_dingtalk_status_text(inputs: DingtalkStatusInputs<'_>) -> String {
    let cfg = inputs.config;
    let dt = &cfg.gateway.dingtalk;

    let gateway_enabled = dt.enabled;
    let legacy_channel_enabled = cfg
        .channels_config
        .dingtalk
        .as_ref()
        .map(|e| e.enabled)
        .unwrap_or(false);

    let app_key_present = !dt.app_key.trim().is_empty();
    let app_secret_present = !dt.app_secret.trim().is_empty();
    let robot_code_present = !dt.robot_code.trim().is_empty();

    let status = if gateway_enabled {
        "enabled"
    } else {
        "disabled"
    };

    format!(
        "DingTalk bot status\n\
         \n\
         gateway.dingtalk.enabled: {}\n\
         channels.dingtalk.enabled: {}\n\
         app_key: {}\n\
         app_secret: {}\n\
         robot_code: {}\n\
         worker_initialized: {}\n\
         queue_count: {}\n\
         outbound_mode: {}\n\
         redact_sensitive_logs: {}\n\
         \n\
         (no secrets, tokens, ids, or webhook URLs are shown)",
        status,
        if legacy_channel_enabled {
            "true"
        } else {
            "false"
        },
        if app_key_present { "present" } else { "absent" },
        if app_secret_present {
            "present"
        } else {
            "absent"
        },
        if robot_code_present {
            "present"
        } else {
            "absent"
        },
        if inputs.worker_initialized {
            "true"
        } else {
            "false"
        },
        inputs.queue_len,
        dt.outbound_mode,
        if dt.redact_sensitive_logs {
            "true"
        } else {
            "false"
        },
    )
}

/// Reply text for `/ping`.
pub fn build_dingtalk_ping_text() -> String {
    "pong".to_string()
}

/// Reply text for `/monitor`. Phase 2 deliberately returns a
/// not-available notice rather than half-implementing the feature.
pub fn build_dingtalk_monitor_text() -> String {
    "DingTalk monitor is not available in this phase.".to_string()
}

/// One-stop helper used by both the async worker path and the sync
/// fallback: parse the command and return the reply text, or
/// `None` if the message is not a command (must be forwarded to the
/// agent unchanged).
///
/// `raw_text` is the `InboundMessage.text` value as set by Phase 1.
/// `payload` is the parsed JSON body of the callback (the helper
/// prefers the real `text.content` field when present). `inputs`
/// supplies the live Config + worker state for `/status` rendering.
pub fn evaluate_dingtalk_command(
    raw_text: &str,
    payload: Option<&serde_json::Value>,
    inputs: DingtalkStatusInputs<'_>,
) -> Option<(DingtalkCommand, String)> {
    let text_for_match = normalize_dingtalk_command_text(raw_text, payload);
    let normalized = to_normalized_for_match(&text_for_match);
    let cmd = parse_dingtalk_command(&normalized)?;
    let reply = match cmd {
        DingtalkCommand::Help => build_dingtalk_help_text(),
        DingtalkCommand::Menu => build_dingtalk_menu_text(),
        DingtalkCommand::Status => build_dingtalk_status_text(inputs),
        DingtalkCommand::Ping => build_dingtalk_ping_text(),
        DingtalkCommand::Monitor => build_dingtalk_monitor_text(),
    };
    Some((cmd, reply))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn empty_inputs<'a>(cfg: &'a Config) -> DingtalkStatusInputs<'a> {
        DingtalkStatusInputs {
            config: cfg,
            worker_initialized: false,
            queue_len: 0,
        }
    }

    #[test]
    fn strip_bot_mention_basic() {
        assert_eq!(strip_bot_mention("@bot help"), "help");
        assert_eq!(strip_bot_mention("@机器人菜单"), "");
        assert_eq!(strip_bot_mention("   @bot   menu   "), "menu");
    }

    #[test]
    fn strip_bot_mention_no_mention() {
        assert_eq!(strip_bot_mention("hello"), "hello");
        assert_eq!(strip_bot_mention("  no mention  "), "no mention");
    }

    #[test]
    fn to_normalized_for_match_lowercases_and_strips_slash() {
        assert_eq!(to_normalized_for_match("HELP"), "help");
        assert_eq!(to_normalized_for_match("/help"), "help");
        assert_eq!(to_normalized_for_match("  /Ping  "), "ping");
    }

    #[test]
    fn parse_dingtalk_command_matches_known_aliases() {
        assert_eq!(parse_dingtalk_command("help"), Some(DingtalkCommand::Help));
        assert_eq!(parse_dingtalk_command("菜单"), Some(DingtalkCommand::Menu));
        assert_eq!(parse_dingtalk_command("panel"), Some(DingtalkCommand::Menu));
        assert_eq!(parse_dingtalk_command("面板"), Some(DingtalkCommand::Menu));
        assert_eq!(
            parse_dingtalk_command("status"),
            Some(DingtalkCommand::Status)
        );
        assert_eq!(
            parse_dingtalk_command("状态"),
            Some(DingtalkCommand::Status)
        );
        assert_eq!(parse_dingtalk_command("ping"), Some(DingtalkCommand::Ping));
        assert_eq!(
            parse_dingtalk_command("monitor"),
            Some(DingtalkCommand::Monitor)
        );
        assert_eq!(parse_dingtalk_command("anything else"), None);
    }

    #[test]
    fn command_normalization_handles_mentions_spaces_and_newlines() {
        let payload = serde_json::json!({
            "text": { "content": "  @OmniNova\n  /STATUS  \r\n" }
        });
        let normalized = normalize_dingtalk_command_text("ignored", Some(&payload));
        assert_eq!(to_normalized_for_match(&normalized), "status");

        let chinese = serde_json::json!({
            "text": { "content": "@机器人\n帮助" }
        });
        let normalized = normalize_dingtalk_command_text("ignored", Some(&chinese));
        assert_eq!(
            parse_dingtalk_command(&to_normalized_for_match(&normalized)),
            Some(DingtalkCommand::Help)
        );

        assert_eq!(
            parse_dingtalk_command(&to_normalized_for_match(" \n /ping \r\n")),
            Some(DingtalkCommand::Ping)
        );
    }
}
