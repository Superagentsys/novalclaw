/// High-risk shell commands that are blocked when
/// `autonomy.block_high_risk_commands = true`.
pub const DEFAULT_DANGEROUS_SHELL_COMMANDS: &[&str] = &[
    "rm",
    "dd",
    "mkfs",
    "fdisk",
    "shutdown",
    "reboot",
    "poweroff",
    "halt",
    "sudo",
    "su",
    "killall",
    "launchctl",
];

pub fn is_dangerous_shell_command(cmd: &str) -> bool {
    DEFAULT_DANGEROUS_SHELL_COMMANDS
        .iter()
        .any(|blocked| blocked.eq_ignore_ascii_case(cmd))
}
