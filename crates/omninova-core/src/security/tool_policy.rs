use crate::config::Config;
use crate::security::dangerous_tools::is_dangerous_shell_command;

const MEDIUM_RISK_COMMANDS: &[&str] = &[
    "git",
    "npm",
    "pnpm",
    "yarn",
    "cargo",
    "pip",
    "docker",
];

/// Resolve effective shell command allowlist based on autonomy + security policy.
pub fn resolve_shell_allowlist(config: &Config) -> Vec<String> {
    let mut commands = config.autonomy.allowed_commands.clone();
    if config.autonomy.block_high_risk_commands {
        commands.retain(|cmd| !is_dangerous_shell_command(cmd));
    }
    if config.autonomy.require_approval_for_medium_risk {
        let shell_auto_approved = config
            .autonomy
            .auto_approve
            .iter()
            .any(|t| t.eq_ignore_ascii_case("shell"));
        if !shell_auto_approved {
            commands.retain(|cmd| !is_medium_risk_command(cmd));
        }
    }
    commands.sort();
    commands.dedup();
    commands
}

fn is_medium_risk_command(cmd: &str) -> bool {
    MEDIUM_RISK_COMMANDS
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(cmd))
}
