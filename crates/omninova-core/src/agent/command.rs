//! Command System for Chat Interface
//!
//! This module provides a command execution framework for user-facing commands
//! that can be invoked from the chat interface (e.g., /help, /clear, /export).
//!
//! Commands are distinguished from regular chat messages by the "/" prefix.
//!
//! [Source: Story 4.10 - 指令执行框架]

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur during command execution
#[derive(Debug, Error)]
pub enum CommandError {
    /// The requested command was not found
    #[error("未知指令: {0}")]
    NotFound(String),

    /// Invalid arguments provided to the command
    #[error("无效参数: {0}")]
    InvalidArguments(String),

    /// Command execution failed
    #[error("执行失败: {0}")]
    ExecutionFailed(String),

    /// No session context available
    #[error("无活动会话")]
    NoActiveSession,
}

// ============================================================================
// Result Types
// ============================================================================

/// Result of a command execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResult {
    /// Whether the command executed successfully
    pub success: bool,
    /// Human-readable message to display to the user
    pub message: String,
    /// Optional structured data returned by the command
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    /// List of available commands (populated for /help or unknown command errors)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_commands: Option<Vec<CommandInfo>>,
}

impl CommandResult {
    /// Create a successful result with a message
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            data: None,
            available_commands: None,
        }
    }

    /// Create a successful result with data
    pub fn success_with_data(message: impl Into<String>, data: serde_json::Value) -> Self {
        Self {
            success: true,
            message: message.into(),
            data: Some(data),
            available_commands: None,
        }
    }

    /// Create an error result with a message
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            data: None,
            available_commands: None,
        }
    }

    /// Create an error result with available commands (for unknown command)
    pub fn error_with_suggestions(
        message: impl Into<String>,
        available_commands: Vec<CommandInfo>,
    ) -> Self {
        Self {
            success: false,
            message: message.into(),
            data: None,
            available_commands: Some(available_commands),
        }
    }
}

/// Information about a command for help display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandInfo {
    /// Command name (without / prefix)
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// Usage example
    pub usage: String,
}

// ============================================================================
// Context Types
// ============================================================================

/// Context provided to command execution
#[derive(Debug, Clone)]
pub struct CommandContext {
    /// Current session ID
    pub session_id: i64,
    /// Current agent ID
    pub agent_id: i64,
}

// ============================================================================
// Command Trait
// ============================================================================

/// Core command trait — implement for any user-facing command
///
/// Commands are invoked from the chat interface with a "/" prefix.
///
/// # Example
///
/// ```rust
/// use omninova_core::agent::{Command, CommandContext, CommandError, CommandInfo, CommandResult, CommandRegistry};
///
/// struct HelpCommand;
///
/// #[async_trait::async_trait]
/// impl Command for HelpCommand {
///     fn name(&self) -> &str { "help" }
///     fn description(&self) -> &str { "显示可用指令列表" }
///     fn usage(&self) -> &str { "/help" }
///
///     async fn execute(
///         &self,
///         _args: Vec<String>,
///         _context: CommandContext,
///         registry: &CommandRegistry,
///     ) -> Result<CommandResult, CommandError> {
///         let commands: Vec<CommandInfo> = registry
///             .list()
///             .iter()
///             .map(|cmd| CommandInfo {
///                 name: cmd.name().to_string(),
///                 description: cmd.description().to_string(),
///                 usage: cmd.usage().to_string(),
///             })
///             .collect();
///
///         Ok(CommandResult::success_with_data(
///             "可用指令列表",
///             serde_json::to_value(&commands).unwrap(),
///         ))
///     }
/// }
/// ```
#[async_trait]
pub trait Command: Send + Sync {
    /// Command name (without / prefix, e.g., "help" for /help)
    fn name(&self) -> &str;

    /// Human-readable description
    fn description(&self) -> &str;

    /// Usage example (e.g., "/help" or "/export [format]")
    fn usage(&self) -> &str;

    /// Execute the command with given arguments
    ///
    /// # Arguments
    ///
    /// * `args` - Command arguments (after the command name)
    /// * `context` - Current session/agent context
    /// * `registry` - Reference to the command registry (for /help command)
    ///
    /// # Returns
    ///
    /// A `CommandResult` with success status and message/data
    async fn execute(
        &self,
        args: Vec<String>,
        context: CommandContext,
        registry: &CommandRegistry,
    ) -> Result<CommandResult, CommandError>;

    /// Get command info for help display
    fn info(&self) -> CommandInfo {
        CommandInfo {
            name: self.name().to_string(),
            description: self.description().to_string(),
            usage: self.usage().to_string(),
        }
    }
}

// ============================================================================
// Command Registry
// ============================================================================

/// Registry for managing available commands
pub struct CommandRegistry {
    commands: HashMap<String, Box<dyn Command>>,
}

impl CommandRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
        }
    }

    /// Create a new registry with default commands registered
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register_defaults();
        registry
    }

    /// Register a command
    pub fn register(&mut self, command: Box<dyn Command>) {
        self.commands.insert(command.name().to_lowercase(), command);
    }

    /// Register default built-in commands
    pub fn register_defaults(&mut self) {
        use super::commands::{ClearCommand, ExportCommand, HelpCommand};

        self.register(Box::new(HelpCommand));
        self.register(Box::new(ClearCommand));
        self.register(Box::new(ExportCommand));
    }

    /// Get a command by name (case-insensitive)
    pub fn get(&self, name: &str) -> Option<&Box<dyn Command>> {
        self.commands.get(&name.to_lowercase())
    }

    /// Check if a command exists
    pub fn has(&self, name: &str) -> bool {
        self.commands.contains_key(&name.to_lowercase())
    }

    /// List all registered commands
    pub fn list(&self) -> Vec<&Box<dyn Command>> {
        let mut commands: Vec<_> = self.commands.values().collect();
        commands.sort_by(|a, b| a.name().cmp(b.name()));
        commands
    }

    /// Get command info for all commands
    pub fn list_info(&self) -> Vec<CommandInfo> {
        self.list().iter().map(|cmd| cmd.info()).collect()
    }

    /// Execute a command by name
    pub async fn execute(
        &self,
        name: &str,
        args: Vec<String>,
        context: CommandContext,
    ) -> Result<CommandResult, CommandError> {
        match self.get(name) {
            Some(command) => command.execute(args, context, self).await,
            None => Err(CommandError::NotFound(name.to_string())),
        }
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

// ============================================================================
// Command Parsing
// ============================================================================

/// Parsed command from user input
#[derive(Debug, Clone)]
pub struct ParsedCommand {
    /// Command name (without / prefix)
    pub name: String,
    /// Command arguments
    pub args: Vec<String>,
}

/// Parse a user message to check if it's a command
///
/// Returns `Some(ParsedCommand)` if the message starts with "/",
/// otherwise returns `None`.
///
/// # Examples
///
/// ```
/// use omninova_core::agent::command::parse_command;
///
/// let parsed = parse_command("/help").unwrap();
/// assert_eq!(parsed.name, "help");
/// assert!(parsed.args.is_empty());
///
/// let parsed = parse_command("/export json").unwrap();
/// assert_eq!(parsed.name, "export");
/// assert_eq!(parsed.args, vec!["json"]);
///
/// let parsed = parse_command("Hello, how are you?");
/// assert!(parsed.is_none());
/// ```
pub fn parse_command(input: &str) -> Option<ParsedCommand> {
    let trimmed = input.trim();

    // Must start with /
    if !trimmed.starts_with('/') {
        return None;
    }

    // Split into parts (remove the / prefix)
    let parts: Vec<&str> = trimmed[1..].split_whitespace().collect();

    // Must have at least a command name
    if parts.is_empty() {
        return None;
    }

    // Extract command name and args
    let name = parts[0].to_lowercase();
    let args = parts[1..].iter().map(|s| s.to_string()).collect();

    Some(ParsedCommand { name, args })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_command_simple() {
        let parsed = parse_command("/help").unwrap();
        assert_eq!(parsed.name, "help");
        assert!(parsed.args.is_empty());
    }

    #[test]
    fn test_parse_command_with_args() {
        let parsed = parse_command("/export json").unwrap();
        assert_eq!(parsed.name, "export");
        assert_eq!(parsed.args, vec!["json"]);
    }

    #[test]
    fn test_parse_command_multiple_args() {
        let parsed = parse_command("/command arg1 arg2 arg3").unwrap();
        assert_eq!(parsed.name, "command");
        assert_eq!(parsed.args, vec!["arg1", "arg2", "arg3"]);
    }

    #[test]
    fn test_parse_command_case_insensitive() {
        let parsed = parse_command("/HELP").unwrap();
        assert_eq!(parsed.name, "help");

        let parsed = parse_command("/ExPoRt").unwrap();
        assert_eq!(parsed.name, "export");
    }

    #[test]
    fn test_parse_command_with_whitespace() {
        let parsed = parse_command("  /help  ").unwrap();
        assert_eq!(parsed.name, "help");
        assert!(parsed.args.is_empty());
    }

    #[test]
    fn test_parse_command_not_a_command() {
        assert!(parse_command("Hello world").is_none());
        assert!(parse_command("This is /not a command").is_none());
    }

    #[test]
    fn test_parse_command_empty_after_slash() {
        // Just a slash with nothing after it
        // This would split to empty vec, so returns None
        assert!(parse_command("/").is_none());
    }

    #[test]
    fn test_parse_command_only_whitespace_after_slash() {
        assert!(parse_command("/   ").is_none());
    }

    #[test]
    fn test_command_result_success() {
        let result = CommandResult::success("Operation completed");
        assert!(result.success);
        assert_eq!(result.message, "Operation completed");
        assert!(result.data.is_none());
        assert!(result.available_commands.is_none());
    }

    #[test]
    fn test_command_result_error() {
        let result = CommandResult::error("Something went wrong");
        assert!(!result.success);
        assert_eq!(result.message, "Something went wrong");
    }

    #[test]
    fn test_command_error_display() {
        let err = CommandError::NotFound("xyz".to_string());
        assert!(err.to_string().contains("xyz"));

        let err = CommandError::InvalidArguments("missing arg".to_string());
        assert!(err.to_string().contains("missing arg"));
    }

    #[test]
    fn test_command_registry_new() {
        let registry = CommandRegistry::new();
        assert!(!registry.has("help"));
    }

    #[test]
    fn test_command_registry_with_defaults() {
        let registry = CommandRegistry::with_defaults();
        assert!(registry.has("help"));
        assert!(registry.has("clear"));
        assert!(registry.has("export"));
    }

    #[test]
    fn test_command_registry_list() {
        let registry = CommandRegistry::with_defaults();
        let commands = registry.list();
        assert!(!commands.is_empty());
    }

    #[test]
    fn test_command_info() {
        let info = CommandInfo {
            name: "test".to_string(),
            description: "Test command".to_string(),
            usage: "/test".to_string(),
        };
        assert_eq!(info.name, "test");
        assert_eq!(info.description, "Test command");
        assert_eq!(info.usage, "/test");
    }
}