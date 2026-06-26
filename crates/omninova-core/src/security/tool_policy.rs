use crate::config::Config;
use crate::security::dangerous_tools::is_dangerous_shell_command;
use crate::security::sandbox::path_hits_forbidden;
use serde::{Deserialize, Serialize};

const MEDIUM_RISK_COMMANDS: &[&str] = &[
    "git", "npm", "pnpm", "yarn", "cargo", "pip", "docker",
];

const HIGH_RISK_TOOLS: &[&str] = &[
    "shell",
    "file_write",
    "file_edit",
    "git_operations",
    "browser",
    "http_request",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolPolicyDecision {
    Allow,
    Deny { reason: String },
    RequireApproval { reason: String },
}

/// Resolve effective shell command allowlist based on autonomy + security policy.
pub fn resolve_shell_allowlist(config: &Config) -> Vec<String> {
    let mut commands = config.autonomy.allowed_commands.clone();
    if config.autonomy.block_high_risk_commands {
        commands.retain(|cmd| !is_dangerous_shell_command(cmd));
    }
    if config.autonomy.require_approval_for_medium_risk {
        let shell_auto_approved = is_tool_auto_approved(config, "shell");
        if !shell_auto_approved {
            commands.retain(|cmd| !is_medium_risk_command(cmd));
        }
    }
    commands.sort();
    commands.dedup();
    commands
}

pub fn is_tool_auto_approved(config: &Config, tool_name: &str) -> bool {
    config
        .autonomy
        .auto_approve
        .iter()
        .any(|t| t.eq_ignore_ascii_case(tool_name))
        || config
            .approvals
            .auto_approve
            .iter()
            .any(|t| t.eq_ignore_ascii_case(tool_name))
}

pub fn is_tool_denied(config: &Config, tool_name: &str) -> bool {
    if config
        .security
        .tool_policy
        .denied_tools
        .iter()
        .any(|t| t.eq_ignore_ascii_case(tool_name))
    {
        return true;
    }
    if config
        .commands
        .forbidden
        .iter()
        .any(|t| t.eq_ignore_ascii_case(tool_name))
    {
        return true;
    }
    false
}

pub fn is_tool_globally_allowed(config: &Config, tool_name: &str) -> bool {
    if is_tool_denied(config, tool_name) {
        return false;
    }
    let allowlist = &config.security.tool_policy.allowed_tools;
    if !allowlist.is_empty() {
        return allowlist
            .iter()
            .any(|t| t.eq_ignore_ascii_case(tool_name));
    }
    if !config.commands.allowed.is_empty() {
        return config
            .commands
            .allowed
            .iter()
            .any(|t| t.eq_ignore_ascii_case(tool_name));
    }
    true
}

pub fn evaluate_tool_call(
    config: &Config,
    tool_name: &str,
    arguments: &serde_json::Value,
) -> ToolPolicyDecision {
    if !config.security.tool_policy.enabled {
        return ToolPolicyDecision::Allow;
    }

    if !is_tool_globally_allowed(config, tool_name) {
        return ToolPolicyDecision::Deny {
            reason: format!("tool '{tool_name}' is not allowed by security policy"),
        };
    }

    if tool_name == "shell" {
        if let Some(cmd) = arguments.get("command").and_then(|v| v.as_str()) {
            let first = cmd.split_whitespace().next().unwrap_or("");
            if config.autonomy.block_high_risk_commands && is_dangerous_shell_command(first) {
                return ToolPolicyDecision::Deny {
                    reason: format!("shell command '{first}' is blocked as high-risk"),
                };
            }
            let allowlist = resolve_shell_allowlist(config);
            if !allowlist.iter().any(|c| c == first) {
                return ToolPolicyDecision::Deny {
                    reason: format!("shell command '{first}' is not in allowlist"),
                };
            }
        }
    }

    if matches!(tool_name, "file_read" | "file_write" | "file_edit") {
        if let Some(path) = arguments.get("path").and_then(|v| v.as_str()) {
            if let Some(reason) = path_hits_forbidden(config, path) {
                return ToolPolicyDecision::Deny { reason };
            }
        }
    }

    if is_tool_auto_approved(config, tool_name) {
        return ToolPolicyDecision::Allow;
    }

    if config.approvals.enabled {
        if config
            .approvals
            .require_approval
            .iter()
            .any(|t| t.eq_ignore_ascii_case(tool_name))
        {
            return ToolPolicyDecision::RequireApproval {
                reason: format!("tool '{tool_name}' requires explicit approval"),
            };
        }
    }

    match config.autonomy.level.as_str() {
        "autonomous" => ToolPolicyDecision::Allow,
        "semi" => {
            if is_high_risk_tool(tool_name) {
                ToolPolicyDecision::RequireApproval {
                    reason: format!(
                        "tool '{tool_name}' is high-risk under semi-autonomous policy"
                    ),
                }
            } else {
                ToolPolicyDecision::Allow
            }
        }
        _ => ToolPolicyDecision::RequireApproval {
            reason: format!(
                "tool '{tool_name}' requires approval under supervised autonomy"
            ),
        },
    }
}

fn is_medium_risk_command(cmd: &str) -> bool {
    MEDIUM_RISK_COMMANDS
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(cmd))
}

fn is_high_risk_tool(tool_name: &str) -> bool {
    HIGH_RISK_TOOLS
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(tool_name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn supervised_shell_requires_approval() {
        let mut config = Config::default();
        config.security.tool_policy.enabled = true;
        config.autonomy.level = "supervised".into();
        config.autonomy.auto_approve = vec!["file_read".into()];
        let decision = evaluate_tool_call(
            &config,
            "shell",
            &serde_json::json!({"command": "ls"}),
        );
        assert!(matches!(decision, ToolPolicyDecision::RequireApproval { .. }));
    }

    #[test]
    fn dangerous_shell_is_denied() {
        let mut config = Config::default();
        config.security.tool_policy.enabled = true;
        config.autonomy.block_high_risk_commands = true;
        let decision = evaluate_tool_call(
            &config,
            "shell",
            &serde_json::json!({"command": "rm -rf /"}),
        );
        assert!(matches!(decision, ToolPolicyDecision::Deny { .. }));
    }
}
