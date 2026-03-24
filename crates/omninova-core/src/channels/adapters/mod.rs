pub mod cli;
pub mod discord;
pub mod email;
pub mod platform_webhook;
pub mod slack;
pub mod webhook;

// Re-export main types for convenience
pub use discord::{DiscordApi, DiscordChannel, DiscordChannelFactory, DiscordConfig, DiscordMessage};
pub use email::{
    EmailAddress, EmailAttachment, EmailChannel, EmailChannelFactory, EmailConfig,
    EmailFilter, EmailFilterType, EmailMessage,
};
pub use slack::{SlackApi, SlackChannel, SlackChannelFactory, SlackConfig, SlackMessage};
