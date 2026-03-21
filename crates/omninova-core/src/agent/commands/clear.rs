//! Clear Command - Clear session messages
//!
//! [Source: Story 4.10 - 指令执行框架]

use async_trait::async_trait;

use crate::agent::command::{Command, CommandContext, CommandError, CommandResult};

/// Clear command - clears the current session messages
pub struct ClearCommand;

#[async_trait]
impl Command for ClearCommand {
    fn name(&self) -> &str {
        "clear"
    }

    fn description(&self) -> &str {
        "清除当前会话消息"
    }

    fn usage(&self) -> &str {
        "/clear"
    }

    async fn execute(
        &self,
        _args: Vec<String>,
        _context: CommandContext,
        _registry: &crate::agent::command::CommandRegistry,
    ) -> Result<CommandResult, CommandError> {
        // The actual clearing is handled by the frontend
        // This command returns a special result that the frontend interprets
        Ok(CommandResult::success_with_data(
            "会话消息已清除",
            serde_json::json!({
                "action": "clear_messages"
            }),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clear_command_info() {
        let cmd = ClearCommand;
        assert_eq!(cmd.name(), "clear");
        assert_eq!(cmd.usage(), "/clear");
        assert!(!cmd.description().is_empty());
    }
}