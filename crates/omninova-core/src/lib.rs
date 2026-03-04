pub mod agent;
pub mod config;
pub mod memory;
pub mod observability;
pub mod providers;
pub mod tools;
pub mod util;

pub use agent::Agent;
pub use config::{AgentConfig, Config};
pub use memory::backend::MockMemory;
pub use providers::{MockProvider, OpenAiProvider};

pub fn init() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    Ok(())
}
