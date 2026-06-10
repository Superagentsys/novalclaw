pub mod agent;
pub mod acp;
pub mod channels;
pub mod cli;
pub mod config;
pub mod cron;
pub mod daemon;
pub mod gateway;
pub mod memory;
pub mod observability;
pub mod providers;
pub mod routing;
pub mod security;
pub mod skills;
pub mod tools;
pub mod util;

pub use agent::Agent;
pub use config::{AgentConfig, Config};
pub use cron::{CronScheduler, CronStore};
pub use memory::backend::{InMemoryMemory, MockMemory};
pub use providers::{MockProvider, OpenAiProvider};

pub fn init() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    Ok(())
}
