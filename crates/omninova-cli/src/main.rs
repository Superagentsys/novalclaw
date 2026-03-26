use clap::{Parser, Subcommand};
use anyhow::Result;
use tracing::{info, debug};

mod api;
mod commands;
mod config;

use config::Config;
use commands::{AgentsCommands, ConfigCommands, SkillsCommands};


#[derive(Parser)]
#[command(
    name = "omninova",
    about = "OmniNova Claw CLI - Manage AI agents from the command line",
    version,
    author
)]
#[command(propagate_version = true)]
struct Cli {
    /// Path to configuration file
    #[arg(short, long, global = true)]
    config: Option<String>,

    /// Output format (text or json)
    #[arg(short, long, global = true, default_value = "text")]
    format: OutputFormat,

    /// Server URL (overrides config)
    #[arg(short, long, global = true)]
    server: Option<String>,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Clone, Debug, Default, clap::ValueEnum)]
enum OutputFormat {
    #[default]
    Text,
    Json,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage AI agents
    #[command(subcommand)]
    Agents(AgentsCommands),

    /// Manage configuration
    #[command(subcommand)]
    Config(ConfigCommands),

    /// Manage skills
    #[command(subcommand)]
    Skills(SkillsCommands),

    /// Quick chat with an agent
    Chat {
        /// Agent ID or name
        agent: String,
        /// Message to send
        message: Vec<String>,
    },

    /// Show system status
    Status,

    /// List all agents (shortcut for `agents list`)
    #[command(alias = "ls")]
    List,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(
            if cli.verbose {
                "debug"
            } else {
                "info"
            }
        )
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .finish();
    
    tracing::subscriber::set_global_default(subscriber)?;

    debug!("Starting OmniNova CLI");
    debug!("Output format: {:?}", cli.format);

    // Load configuration
    let config = Config::load(cli.config.as_deref())?;
    debug!("Configuration loaded successfully");

    // Override server URL if provided
    let server_url = cli.server.unwrap_or_else(|| config.server_url.clone());
    info!("Using server: {}", server_url);

    // Create API client
    let client = api::Client::new(&server_url)?;

    // Execute command
    match cli.command {
        Commands::Agents(cmd) => {
            commands::agents::execute(cmd, &client, cli.format).await?;
        }
        Commands::Config(cmd) => {
            commands::config::execute(cmd, &config).await?;
        }
        Commands::Chat { agent, message } => {
            let msg = message.join(" ");
            commands::chat::execute(&agent, &msg, &client, cli.format).await?;
        }
        Commands::Status => {
            commands::status::execute(&client, cli.format).await?;
        }
        Commands::List => {
            commands::agents::execute(
                AgentsCommands::List,
                &client,
                cli.format
            ).await?;
        }
        Commands::Skills(cmd) => {
            commands::skills::execute(cmd, cli.format).await?;
        }
    }

    Ok(())
}
