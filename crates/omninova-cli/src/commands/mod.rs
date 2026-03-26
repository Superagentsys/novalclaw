pub mod agents;
pub mod config;
pub mod chat;
pub mod status;
pub mod agent_advanced;
pub mod skills;

use clap::Subcommand;

#[derive(Subcommand)]
pub enum AgentsCommands {
    /// List all agents
    List,
    /// Show agent details
    Show {
        /// Agent ID or name
        id: String,
    },
    /// Create a new agent
    Create {
        /// Agent name
        #[arg(short, long)]
        name: String,
        /// MBTI personality type
        #[arg(short, long)]
        mbti: Option<String>,
    },
    /// Delete an agent
    Delete {
        /// Agent ID
        id: String,
        /// Force deletion without confirmation
        #[arg(short, long)]
        force: bool,
    },
    /// Export agents to file
    Export {
        /// Output file path
        #[arg(short, long)]
        output: String,
        /// Export format (json or yaml)
        #[arg(short, long, default_value = "json")]
        format: String,
        /// Specific agent IDs to export (export all if not specified)
        #[arg(short, long)]
        ids: Option<Vec<String>>,
    },
    /// Import agents from file
    Import {
        /// Input file path
        #[arg(short, long)]
        input: String,
        /// Skip existing agents
        #[arg(short, long)]
        skip_existing: bool,
    },
    /// Batch operations on agents
    Batch {
        /// Operation to perform (enable, disable, delete)
        #[arg(short, long)]
        operation: String,
        /// Agent IDs to operate on
        #[arg(required = true)]
        ids: Vec<String>,
    },
    /// Show agent statistics
    Stats {
        /// Agent ID (show all if not specified)
        #[arg(short, long)]
        id: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Show current configuration
    Show,
    /// Set configuration value
    Set {
        /// Key to set (server_url, default_agent, output_format)
        key: String,
        /// Value to set
        value: String,
    },
    /// Initialize configuration file
    Init,
}

#[derive(Subcommand)]
pub enum SkillsCommands {
    /// List installed skills
    List,
    /// Show skill details
    Show {
        /// Skill name
        name: String,
    },
    /// Install a skill
    Install {
        /// Source path or Git URL
        source: String,
        /// Custom name for the skill
        #[arg(short, long)]
        name: Option<String>,
    },
    /// Uninstall a skill
    Uninstall {
        /// Skill name
        name: String,
        /// Force uninstall without confirmation
        #[arg(short, long)]
        force: bool,
    },
    /// Validate skill configuration
    Validate {
        /// Path to skill directory
        path: String,
    },
    /// Package skill for distribution
    Package {
        /// Path to skill directory
        path: String,
        /// Output file name
        #[arg(short, long)]
        output: Option<String>,
    },
}
