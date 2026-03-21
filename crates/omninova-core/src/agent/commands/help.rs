//! Help Command - Display available commands
//!
//! [Source: Story 4.10 - 指令执行框架]

use async_trait::async_trait;

use crate::agent::command::{Command, CommandContext, CommandError, CommandResult, CommandInfo};

/// Help command - displays available commands
pub struct HelpCommand;

#[async_trait]
impl Command for HelpCommand {
    fn name(&self) -> &str {
        "help"
    }

    fn description(&self) -> &str {
        "显示可用指令列表"
    }

    fn usage(&self) -> &str {
        "/help"
    }

    async fn execute(
        &self,
        _args: Vec<String>,
        _context: CommandContext,
        registry: &crate::agent::command::CommandRegistry,
    ) -> Result<CommandResult, CommandError> {
        let commands: Vec<CommandInfo> = registry.list_info();

        // Build a formatted message
        let mut message = String::from("📋 **可用指令**\n\n");
        for cmd in &commands {
            message.push_str(&format!("- **{}** - {}\n", cmd.usage, cmd.description));
        }
        message.push_str("\n💡 输入指令名称以获取更多帮助。");

        Ok(CommandResult::success_with_data(
            message,
            serde_json::to_value(&commands).unwrap_or_default(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_help_command_info() {
        let cmd = HelpCommand;
        assert_eq!(cmd.name(), "help");
        assert_eq!(cmd.usage(), "/help");
        assert!(!cmd.description().is_empty());
    }
}