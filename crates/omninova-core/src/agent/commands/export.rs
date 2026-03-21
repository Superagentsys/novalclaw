//! Export Command - Export session history
//!
//! [Source: Story 4.10 - 指令执行框架]

use async_trait::async_trait;

use crate::agent::command::{Command, CommandContext, CommandError, CommandResult};

/// Export command - exports session history
pub struct ExportCommand;

#[async_trait]
impl Command for ExportCommand {
    fn name(&self) -> &str {
        "export"
    }

    fn description(&self) -> &str {
        "导出会话历史"
    }

    fn usage(&self) -> &str {
        "/export [format]"
    }

    async fn execute(
        &self,
        args: Vec<String>,
        _context: CommandContext,
        _registry: &crate::agent::command::CommandRegistry,
    ) -> Result<CommandResult, CommandError> {
        // Parse format argument (default to json)
        let format = args.first().map(|s| s.as_str()).unwrap_or("json");

        match format {
            "json" | "markdown" | "md" | "txt" | "text" => {
                // The actual export is handled by the frontend
                // This command returns a special result that the frontend interprets
                let normalized_format = if format == "md" { "markdown" } else if format == "txt" || format == "text" { "text" } else { format };

                Ok(CommandResult::success_with_data(
                    format!("会话历史将导出为 {} 格式", normalized_format),
                    serde_json::json!({
                        "action": "export_session",
                        "format": normalized_format
                    }),
                ))
            }
            _ => {
                Err(CommandError::InvalidArguments(format!(
                    "不支持的导出格式: {}。支持的格式: json, markdown, text",
                    format
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_command_info() {
        let cmd = ExportCommand;
        assert_eq!(cmd.name(), "export");
        assert!(cmd.usage().contains("[format]"));
        assert!(!cmd.description().is_empty());
    }
}