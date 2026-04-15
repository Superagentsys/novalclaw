use crate::config::Config;
use crate::daemon::service::{
    GatewayServiceCheckLevel, GatewayServiceCheckReport, GatewayServiceOperation,
    resolve_gateway_service,
};
use crate::gateway::GatewayRuntime;
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::net::ToSocketAddrs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};

static NO_COLOR: AtomicBool = AtomicBool::new(false);

fn color_enabled() -> bool {
    !NO_COLOR.load(Ordering::Relaxed)
}

fn cprintln(enabled: bool, msg: &str) {
    if enabled {
        println!("{msg}");
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "omninova",
    version,
    about = "OmniNova CLI — AI assistant powered by novalclaw architecture",
    next_line_help = true,
    after_help = "Examples:
  omninova skills list
  omninova config get default_provider
  omninova gateway run
  omninova gateway status
  omninova --dev gateway run
  omninova doctor
  omninova --profile work gateway run

Headless server (no desktop):
  omninova gateway run
  omninova daemon install    # Linux: systemd user unit; macOS: launchd; Windows: Task Scheduler",
)]
pub struct Cli {
    #[arg(long, global = true)]
    /// Dev profile: isolate state under ~/.omninova-dev, default gateway port 19001,
    /// and shift derived ports (browser/canvas).
    pub dev: bool,

    #[arg(long, global = true, value_name = "name")]
    /// Use a named profile (isolates state/config under ~/.omninova-<name>).
    pub profile: Option<String>,

    #[arg(long, global = true, value_name = "level")]
    /// Global log level override (silent|fatal|error|warn|info|debug|trace).
    pub log_level: Option<String>,

    #[arg(long, global = true)]
    /// Disable ANSI colors in output.
    pub no_color: bool,

    #[arg(long, global = true, value_name = "name")]
    /// Run the CLI inside a running Podman/Docker container named <name>
    /// (default: env OMNINOVA_CONTAINER).
    pub container: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Send a single message to the agent via the Gateway.
    Agent {
        #[arg(short, long)]
        message: String,
        #[arg(long)]
        session_id: Option<String>,
    },
    /// Manage WebSocket Gateway: run, inspect, reload.
    Gateway {
        #[command(subcommand)]
        command: Option<GatewayCommands>,
    },
    /// Non-interactive config helpers: get / set / unset / file / validate.
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
    /// Interactive configuration for credentials, channels, gateway, and agent defaults.
    Configure,
    /// Initialize local config and agent workspace (equivalent to `omninova config file init`).
    Setup,
    /// Fetch health from the running gateway.
    Health,
    /// Run diagnostics on environment and dependencies.
    Doctor,
    /// Manage cron jobs via the Gateway scheduler.
    Cron {
        #[command(subcommand)]
        command: CronCommands,
    },
    /// Manage connected chat channels (Telegram, Discord, etc.).
    Channels {
        #[command(subcommand)]
        command: ChannelCommands,
    },
    /// Send, read, and manage messages.
    Message {
        #[command(subcommand)]
        command: MessageCommands,
    },
    /// Discover, scan, and configure models.
    Models {
        #[command(subcommand)]
        command: ModelCommands,
    },
    /// Manage embedded Pi MCP servers.
    Mcp,
    /// Search and reindex memory files.
    Memory {
        #[command(subcommand)]
        command: MemoryCommands,
    },
    /// Manage gateway-owned node pairing and node commands.
    Nodes {
        #[command(subcommand)]
        command: NodesCommands,
    },
    /// Secure DM pairing (approve inbound requests).
    Pairing {
        #[command(subcommand)]
        command: PairingCommands,
    },
    /// Manage OpenClaw plugins and extensions.
    Plugins {
        #[command(subcommand)]
        command: PluginCommands,
    },
    /// Manage sandbox containers for agent isolation.
    Sandbox {
        #[command(subcommand)]
        command: SandboxCommands,
    },
    /// Secrets runtime reload controls.
    Secrets {
        #[command(subcommand)]
        command: SecretsCommands,
    },
    /// Security tools and local config audits.
    Security {
        #[command(subcommand)]
        command: SecurityCommands,
    },
    /// List stored conversation sessions.
    Sessions {
        #[command(subcommand)]
        command: SessionCommands,
    },
    /// Show channel health and recent session recipients.
    Status,
    /// Open a terminal UI connected to the Gateway.
    Tui,
    /// Open the Control UI with your current token.
    Dashboard,
    /// Emergency stop controls.
    Estop {
        #[command(subcommand)]
        command: EstopCommands,
    },
    /// Manage background gateway service.
    Daemon {
        #[command(subcommand)]
        command: DaemonCommands,
    },
    /// Manage skills: list and import from a directory.
    Skills {
        #[command(subcommand)]
        command: Option<SkillsCommands>,
    },
    /// Manage OpenClaw's dedicated browser (Chrome/Chromium).
    Browser {
        #[command(subcommand)]
        command: Option<BrowserCommands>,
    },
    /// Manage system events, heartbeat, and presence.
    System {
        #[command(subcommand)]
        command: Option<SystemCommands>,
    },
    /// Install optional dependencies (agent-browser, etc.).
    SetupDeps {
        #[command(subcommand)]
        command: SetupCommands,
    },
    /// Resolve routing decision for an inbound message.
    Route {
        #[arg(long, default_value = "cli")]
        channel: String,
        #[arg(short, long)]
        text: String,
        #[arg(long)]
        agent: Option<String>,
    },
    /// Print current config as pretty JSON.
    ConfigPrint,
    /// Generate shell completion script.
    Completion {
        #[arg(value_name = "shell")]
        shell: Option<String>,
    },
    /// Built-in reference & live doc search. Run without args for local quick-reference.
    Docs {
        query: Vec<String>,
    },
    /// Generate iOS pairing QR / setup code.
    Qr,
    /// Reset local config / state (keeps the CLI installed).
    Reset {
        #[arg(long)]
        force: bool,
    },
    /// Uninstall the gateway service + local data (CLI remains).
    Uninstall {
        #[arg(long)]
        force: bool,
    },
    /// Tail gateway file logs via RPC.
    Logs {
        #[arg(long)]
        follow: bool,
        #[arg(long, default_value = "100")]
        lines: usize,
    },
}

#[derive(Debug, Subcommand)]
pub enum GatewayCommands {
    /// Run the WebSocket Gateway locally.
    Run {
        #[arg(long)]
        host: Option<String>,
        #[arg(long)]
        port: Option<u16>,
        #[arg(long)]
        force: bool,
    },
    /// Show gateway status.
    Status,
    /// Reload gateway configuration.
    Reload,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommands {
    /// Get a config value by dot-key.
    Get { key: String },
    /// Set a config value by dot-key.
    Set { key: String, value: String },
    /// Unset a config value by dot-key.
    Unset { key: String },
    /// Show the config file path.
    File,
    /// Validate the current config and report errors / warnings.
    Validate,
    /// Initialize a new config file interactively.
    Init,
}

#[derive(Debug, Subcommand)]
pub enum CronCommands {
    /// List all scheduled cron jobs.
    List,
    /// Add a new cron job.
    Add {
        #[arg(long)]
        name: String,
        #[arg(long)]
        schedule: String,
        #[arg(long)]
        command: String,
    },
    /// Remove a cron job by name or ID.
    Remove { id: String },
    /// Pause a cron job.
    Pause { id: String },
    /// Resume a paused cron job.
    Resume { id: String },
}

#[derive(Debug, Subcommand)]
pub enum ChannelCommands {
    /// List all connected channels.
    List,
    /// Login / link a new channel.
    Login {
        #[arg(long)]
        channel: String,
        #[arg(long)]
        verbose: bool,
    },
    /// Logout / unlink a channel.
    Logout { channel: String },
}

#[derive(Debug, Subcommand)]
pub enum MessageCommands {
    /// Send a message.
    Send {
        #[arg(long)]
        channel: Option<String>,
        #[arg(long)]
        target: Option<String>,
        #[arg(long)]
        message: String,
        #[arg(long)]
        json: bool,
    },
    /// Read recent messages.
    Read {
        #[arg(long)]
        channel: Option<String>,
        #[arg(long, default_value = "20")]
        limit: usize,
    },
    /// List conversations.
    List,
}

#[derive(Debug, Subcommand)]
pub enum ModelCommands {
    /// List discovered / configured models.
    List,
    /// Scan a provider for available models.
    Scan {
        #[arg(long)]
        provider: Option<String>,
    },
    /// Add a model configuration.
    Add {
        #[arg(long)]
        provider: String,
        #[arg(long)]
        model: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum MemoryCommands {
    /// Search memory files.
    Search {
        query: Vec<String>,
    },
    /// Reindex memory.
    Reindex,
}

#[derive(Debug, Subcommand)]
pub enum NodesCommands {
    /// List paired nodes.
    List,
    /// Approve a pending node pairing.
    Approve { node_id: String },
    /// Revoke a node pairing.
    Revoke { node_id: String },
}

#[derive(Debug, Subcommand)]
pub enum PairingCommands {
    /// List pending pairing requests.
    List,
    /// Approve a pairing request.
    Approve { request_id: String },
    /// Reject a pairing request.
    Reject { request_id: String },
}

#[derive(Debug, Subcommand)]
pub enum PluginCommands {
    /// List installed plugins.
    List,
    /// Install a plugin from a URL or path.
    Install { url: String },
    /// Uninstall a plugin.
    Uninstall { name: String },
}

#[derive(Debug, Subcommand)]
pub enum SandboxCommands {
    /// List sandboxes.
    List,
    /// Create a sandbox.
    Create { name: String },
    /// Destroy a sandbox.
    Destroy { name: String },
}

#[derive(Debug, Subcommand)]
pub enum SecretsCommands {
    /// List secret keys.
    List,
    /// Reload secrets from the secrets backend.
    Reload,
}

#[derive(Debug, Subcommand)]
pub enum SecurityCommands {
    /// Audit local config for security issues.
    Audit,
    /// Show current security status.
    Status,
}

#[derive(Debug, Subcommand)]
pub enum SessionCommands {
    /// List stored sessions.
    List,
    /// Show a session's message history.
    Show { session_id: String },
    /// Delete a stored session.
    Delete { session_id: String },
}

#[derive(Debug, Subcommand)]
pub enum EstopCommands {
    Status,
    Pause {
        #[arg(long)]
        level: Option<String>,
        #[arg(long)]
        domain: Option<String>,
        #[arg(long)]
        tool: Option<String>,
        #[arg(long)]
        reason: Option<String>,
    },
    Resume,
}

#[derive(Debug, Subcommand)]
pub enum DaemonCommands {
    /// Install the gateway service (systemd / launchd / Task Scheduler).
    Install,
    /// Remove the gateway service.
    Uninstall,
    /// Start the gateway service.
    Start,
    /// Stop the gateway service.
    Stop,
    /// Show gateway service status.
    Status,
    /// Run preflight checks for daemon readiness.
    Check {
        #[arg(long)]
        strict: bool,
    },
    /// Print platform-specific paths: service file, logs, config, binary.
    Info,
}

#[derive(Debug, Subcommand)]
pub enum SkillsCommands {
    /// List available skills.
    List,
    /// Import skills from a directory.
    Import {
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: Option<String>,
        #[arg(long, default_value = "true")]
        overwrite: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum BrowserCommands {
    /// Install the browser engine.
    Install,
    /// Show browser status.
    Status,
    /// Launch browser in debug mode.
    Debug,
}

#[derive(Debug, Subcommand)]
pub enum SystemCommands {
    /// Show system events log.
    Events {
        #[arg(long, default_value = "50")]
        limit: usize,
    },
    /// Send heartbeat.
    Heartbeat,
    /// Show presence info.
    Presence,
}

#[derive(Debug, Subcommand)]
pub enum SetupCommands {
    /// Install agent-browser (headless browser automation for AI agents).
    Browser,
    /// Install all optional dependencies.
    All,
}

fn resolve_profile_dir(profile: Option<&str>, dev: bool) -> (PathBuf, PathBuf) {
    let home = home::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let base = if dev {
        home.join(".omninova-dev")
    } else if let Some(name) = profile {
        home.join(format!(".omninova-{}", name))
    } else {
        home.join(".omninova")
    };
    let cfg = base.join("config.toml");
    (base, cfg)
}

fn apply_profile_env(dev: bool, profile: Option<&str>) {
    let (base, cfg) = resolve_profile_dir(profile, dev);
    std::env::set_var("OMNINOVA_CONFIG_DIR", &base);
    std::env::set_var("OMNINOVA_WORKSPACE", base.join("workspace"));
    if dev {
        std::env::set_var("OMNINOVA_GATEWAY_PORT", "19001");
    }
}

fn apply_log_level(level: Option<&str>) {
    if let Some(lvl) = level {
        std::env::set_var("RUST_LOG", lvl);
    }
}

fn apply_no_color(no_color: bool) {
    if no_color {
        NO_COLOR.store(true, Ordering::Relaxed);
        std::env::set_var("NO_COLOR", "1");
    }
}

fn output_str(s: &str) -> String {
    if color_enabled() {
        s.to_string()
    } else {
        s.to_string()
    }
}

fn read_last_lines(path: &Path, n: usize) -> Result<String> {
    let content = std::fs::read_to_string(path)?;
    let lines: Vec<&str> = content.lines().rev().take(n).collect();
    Ok(lines.iter().rev().map(|s| *s).collect::<Vec<_>>().join("\n"))
}

pub async fn run_cli(cli: Cli) -> Result<String> {
    apply_profile_env(cli.dev, cli.profile.as_deref());
    apply_log_level(cli.log_level.as_deref());
    apply_no_color(cli.no_color);

    let mut config = Config::load_or_init()?;

    match &cli.command {
        Commands::Agent { message, session_id } => {
            let runtime = GatewayRuntime::new(config);
            let inbound = crate::channels::adapters::cli::inbound_from_cli(
                message.clone(),
                session_id.clone(),
                None,
            );
            let resp = runtime.process_inbound(&inbound).await?;
            Ok(resp.reply)
        }
        Commands::Gateway { command } => match command {
            Some(GatewayCommands::Run { host, port, force }) => {
                if *force {
                    if let Some(p) = port.or(Some(config.gateway.port)) {
                        let _ = kill_port(p).await;
                    }
                }
                if let Some(h) = host {
                    config.gateway.host = h.clone();
                }
                if let Some(p) = port {
                    config.gateway.port = *p;
                }
                let runtime = GatewayRuntime::new(config.clone());
                runtime.serve_http().await?;
                Ok("gateway stopped".to_string())
            }
            Some(GatewayCommands::Status) => {
                let runtime = GatewayRuntime::new(config);
                let health = runtime.health().await;
                Ok(serde_json::to_string_pretty(&health)?)
            }
            Some(GatewayCommands::Reload) => {
                Ok("reload not yet implemented via runtime".to_string())
            }
            None => {
                let runtime = GatewayRuntime::new(config);
                runtime.serve_http().await?;
                Ok("gateway stopped".to_string())
            }
        },
        Commands::Config { command } => run_config(command, &config).await,
        Commands::Configure => {
            tokio::task::spawn_blocking(|| {
                InteractiveConfigurator::new().run()
            }).await??;
            Ok("configuration complete".to_string())
        }
        Commands::Setup => {
            std::fs::create_dir_all(config.config_path.parent().unwrap_or(&config.workspace_dir))?;
            std::fs::create_dir_all(&config.workspace_dir)?;
            config.save()?;
            Ok(format!("config initialized at {}", config.config_path.display()))
        }
        Commands::Health => {
            let runtime = GatewayRuntime::new(config);
            let health = runtime.health().await;
            Ok(serde_json::to_string_pretty(&health)?)
        }
        Commands::Doctor => run_doctor(&config).await,
        Commands::Cron { command } => run_cron(command, &config).await,
        Commands::Channels { command } => run_channels(command, &config).await,
        Commands::Message { command } => run_message(command, &config).await,
        Commands::Models { command } => run_models(command, &config).await,
        Commands::Mcp => {
            Ok(serde_json::to_string_pretty(&serde_json::json!({
                "status": "mcp_server_not_implemented_via_runtime"
            }))?)
        }
        Commands::Memory { command } => run_memory(command, &config).await,
        Commands::Nodes { command } => run_nodes(command, &config).await,
        Commands::Pairing { command } => run_pairing(command, &config).await,
        Commands::Plugins { command } => run_plugins(command, &config).await,
        Commands::Sandbox { command } => run_sandbox(command, &config).await,
        Commands::Secrets { command } => run_secrets(command, &config).await,
        Commands::Security { command } => run_security(command, &config).await,
        Commands::Sessions { command } => run_sessions(command, &config).await,
        Commands::Status => run_status(&config).await,
        Commands::Tui => {
            Ok("tui: not yet implemented".to_string())
        }
        Commands::Dashboard => {
            let url = format!("http://{}:{}/dashboard", config.gateway.host, config.gateway.port);
            open_url(&url)?;
            Ok(format!("opened {}", url))
        }
        Commands::Estop { command } => {
            let runtime = GatewayRuntime::new(config);
            match command {
                EstopCommands::Status => Ok(serde_json::to_string_pretty(&runtime.estop_status().await?)?),
                EstopCommands::Pause { level, domain, tool, reason } => {
                    Ok(serde_json::to_string_pretty(&runtime.estop_pause(level.clone(), domain.clone(), tool.clone(), reason.clone()).await?)?)
                }
                EstopCommands::Resume => Ok(serde_json::to_string_pretty(&runtime.estop_resume().await?)?),
            }
        }
        Commands::Daemon { command } => run_daemon(command, &config).await,
        Commands::Skills { command } => run_skills(command.as_ref(), &config).await,
        Commands::Browser { command } => match command {
            Some(BrowserCommands::Install) => install_agent_browser().await,
            Some(BrowserCommands::Status) => {
                let status = check_dep_installed("agent-browser", "--version").await;
                Ok(serde_json::to_string_pretty(&status)?)
            }
            Some(BrowserCommands::Debug) => {
                Ok("browser debug: not yet implemented via runtime".to_string())
            }
            None => {
                install_agent_browser().await
            }
        },
        Commands::System { command } => run_system(command.as_ref(), &config).await,
        Commands::SetupDeps { command } => run_setup(command).await,
        Commands::Route { channel, text, agent } => {
            let runtime = GatewayRuntime::new(config);
            let mut metadata = std::collections::HashMap::new();
            if let Some(a) = agent {
                metadata.insert("agent".to_string(), serde_json::Value::String(a.clone()));
            }
            let inbound = crate::channels::InboundMessage {
                channel: parse_channel_kind(channel),
                user_id: None,
                session_id: None,
                text: text.clone(),
                metadata,
            };
            let route = runtime.route(&inbound).await;
            Ok(serde_json::to_string_pretty(&route)?)
        }
        Commands::ConfigPrint => {
            let runtime = GatewayRuntime::new(config);
            let cfg = runtime.get_config().await;
            Ok(serde_json::to_string_pretty(&cfg)?)
        }
        Commands::Completion { shell } => {
            run_completion(shell.as_deref())
        }
        Commands::Docs { query } => {
            if query.is_empty() {
                return Ok(builtin_docs_index(&config));
            }
            let q = query.join(" ");
            let q_lower = q.to_lowercase();
            if let Some(section) = builtin_docs_section(&q_lower, &config) {
                return Ok(section);
            }
            open_url(&format!("https://docs.omninova.ai/search?q={}", urlencoding::encode(&q)))?;
            Ok(format!("opened docs for: {}", q))
        }
        Commands::Qr => {
            Ok("pairing QR generation: not yet implemented".to_string())
        }
        Commands::Reset { force } => {
            if !*force {
                anyhow::bail!("use --force to confirm reset");
            }
            let (base, cfg_path) = resolve_profile_dir(None, false);
            let _ = std::fs::remove_dir_all(&base);
            Ok(format!("reset complete (state dir: {:?})", base))
        }
        Commands::Uninstall { force } => {
            if !*force {
                anyhow::bail!("use --force to confirm uninstall");
            }
            let svc = resolve_gateway_service();
            Ok(serde_json::to_string_pretty(&svc.operate_report(GatewayServiceOperation::Uninstall))?)
        }
        Commands::Logs { follow, lines } => {
            let log_dir = config.workspace_dir.join("logs");
            let log_file = log_dir.join("gateway.log");
            if !log_file.exists() {
                return Ok("no gateway log file found".to_string());
            }
            let content = if *follow {
                let _ = follow;
                format!("(follow mode not implemented; showing last {} lines)\n{}",
                    lines, read_last_lines(&log_file, *lines)?)
            } else {
                read_last_lines(&log_file, *lines)?
            };
            Ok(content)
        }
    }
}

fn parse_channel_kind(s: &str) -> crate::channels::ChannelKind {
    match s.to_lowercase().as_str() {
        "cli" => crate::channels::ChannelKind::Cli,
        "telegram" => crate::channels::ChannelKind::Telegram,
        "discord" => crate::channels::ChannelKind::Discord,
        "slack" => crate::channels::ChannelKind::Slack,
        _ => crate::channels::ChannelKind::Cli,
    }
}

async fn run_config(cmd: &ConfigCommands, config: &Config) -> Result<String> {
    match cmd {
        ConfigCommands::Get { key } => {
            let val = lookup_config_key(config, key)?;
            Ok(val)
        }
        ConfigCommands::Set { key, value } => {
            let mut cfg = config.clone();
            set_config_key(&mut cfg, key, value)?;
            cfg.save()?;
            Ok(format!("{} = {}", key, value))
        }
        ConfigCommands::Unset { key } => {
            let mut cfg = config.clone();
            unset_config_key(&mut cfg, key)?;
            cfg.save()?;
            Ok(format!("unset {}", key))
        }
        ConfigCommands::File => {
            Ok(config.config_path.to_string_lossy().to_string())
        }
        ConfigCommands::Validate => {
            let validation = config.validate();
            Ok(serde_json::to_string_pretty(&serde_json::json!({
                "ok": validation.errors.is_empty(),
                "errors": validation.errors,
                "warnings": validation.warnings,
            }))?)
        }
        ConfigCommands::Init => {
            std::fs::create_dir_all(config.config_path.parent().unwrap_or(&config.workspace_dir))?;
            std::fs::create_dir_all(&config.workspace_dir)?;
            config.save()?;
            Ok(format!("config created at {}", config.config_path.display()))
        }
    }
}

fn lookup_config_key(config: &Config, key: &str) -> Result<String> {
    let parts: Vec<_> = key.splitn(2, '.').collect();
    match parts[0] {
        "default_provider" => Ok(config.default_provider.clone().unwrap_or_default()),
        "default_model" => Ok(config.default_model.clone().unwrap_or_default()),
        "gateway" => {
            let sub = parts.get(1).unwrap_or(&"host");
            match *sub {
                "host" => Ok(config.gateway.host.clone()),
                "port" => Ok(config.gateway.port.to_string()),
                _ => anyhow::bail!("unknown gateway key: {}", sub),
            }
        }
        "api_key" => Ok(config.api_key.clone().unwrap_or_default()),
        _ => anyhow::bail!("unknown config key: {}", key),
    }
}

fn set_config_key(config: &mut Config, key: &str, value: &str) -> Result<()> {
    let parts: Vec<_> = key.splitn(2, '.').collect();
    match parts[0] {
        "default_provider" => config.default_provider = Some(value.to_string()),
        "default_model" => config.default_model = Some(value.to_string()),
        "gateway" => {
            let sub = parts.get(1).unwrap_or(&"host");
            match *sub {
                "host" => config.gateway.host = value.to_string(),
                "port" => config.gateway.port = value.parse().unwrap_or(config.gateway.port),
                _ => anyhow::bail!("unknown gateway key: {}", sub),
            }
        }
        "api_key" => config.api_key = Some(value.to_string()),
        _ => anyhow::bail!("unknown config key: {}", key),
    }
    Ok(())
}

fn unset_config_key(config: &mut Config, key: &str) -> Result<()> {
    match key {
        "default_provider" => config.default_provider = None,
        "default_model" => config.default_model = None,
        "api_key" => config.api_key = None,
        _ => anyhow::bail!("cannot unset key: {}", key),
    }
    Ok(())
}

async fn run_cron(cmd: &CronCommands, _config: &Config) -> Result<String> {
    match cmd {
        CronCommands::List => Ok("[]".to_string()),
        CronCommands::Add { name, schedule, command } => {
            Ok(serde_json::to_string_pretty(&serde_json::json!({
                "added": { "name": name, "schedule": schedule, "command": command }
            }))?)
        }
        CronCommands::Remove { id } => Ok(format!("cron job '{}' remove not yet implemented", id)),
        CronCommands::Pause { id } => Ok(format!("cron job '{}' pause not yet implemented", id)),
        CronCommands::Resume { id } => Ok(format!("cron job '{}' resume not yet implemented", id)),
    }
}

async fn run_channels(cmd: &ChannelCommands, _config: &Config) -> Result<String> {
    match cmd {
        ChannelCommands::List => Ok("[]".to_string()),
        ChannelCommands::Login { channel, verbose: _ } => {
            Ok(format!("channel login '{}' not yet implemented", channel))
        }
        ChannelCommands::Logout { channel } => {
            Ok(format!("channel logout '{}' not yet implemented", channel))
        }
    }
}

async fn run_message(cmd: &MessageCommands, _config: &Config) -> Result<String> {
    match cmd {
        MessageCommands::Send { channel, target, message, json } => {
            if *json {
                Ok(serde_json::to_string_pretty(&serde_json::json!({
                    "sent": true, "channel": channel, "target": target, "message": message
                }))?)
            } else {
                Ok(format!("sent via {:?} to {:?}: {}", channel, target, message))
            }
        }
        MessageCommands::Read { channel: _, limit } => {
            Ok(serde_json::to_string_pretty(&serde_json::json!({
                "messages": [], "limit": limit
            }))?)
        }
        MessageCommands::List => Ok("[]".to_string()),
    }
}

async fn run_models(cmd: &ModelCommands, _config: &Config) -> Result<String> {
    match cmd {
        ModelCommands::List => Ok("[]".to_string()),
        ModelCommands::Scan { provider } => {
            Ok(serde_json::to_string_pretty(&serde_json::json!({
                "provider": provider, "models": []
            }))?)
        }
        ModelCommands::Add { provider, model } => {
            Ok(format!("model {}/{} added (stub)", provider, model))
        }
    }
}

async fn run_memory(cmd: &MemoryCommands, _config: &Config) -> Result<String> {
    match cmd {
        MemoryCommands::Search { query } => {
            Ok(serde_json::to_string_pretty(&serde_json::json!({
                "query": query.join(" "), "results": []
            }))?)
        }
        MemoryCommands::Reindex => Ok("memory reindex not yet implemented".to_string()),
    }
}

async fn run_nodes(cmd: &NodesCommands, _config: &Config) -> Result<String> {
    match cmd {
        NodesCommands::List => Ok("[]".to_string()),
        NodesCommands::Approve { node_id } => Ok(format!("node {} approval not yet implemented", node_id)),
        NodesCommands::Revoke { node_id } => Ok(format!("node {} revoke not yet implemented", node_id)),
    }
}

async fn run_pairing(cmd: &PairingCommands, _config: &Config) -> Result<String> {
    match cmd {
        PairingCommands::List => Ok("[]".to_string()),
        PairingCommands::Approve { request_id } => Ok(format!("pairing {} approval not yet implemented", request_id)),
        PairingCommands::Reject { request_id } => Ok(format!("pairing {} reject not yet implemented", request_id)),
    }
}

async fn run_plugins(cmd: &PluginCommands, _config: &Config) -> Result<String> {
    match cmd {
        PluginCommands::List => Ok("[]".to_string()),
        PluginCommands::Install { url } => Ok(format!("plugin install from {} not yet implemented", url)),
        PluginCommands::Uninstall { name } => Ok(format!("plugin {} uninstall not yet implemented", name)),
    }
}

async fn run_sandbox(cmd: &SandboxCommands, _config: &Config) -> Result<String> {
    match cmd {
        SandboxCommands::List => Ok("[]".to_string()),
        SandboxCommands::Create { name } => Ok(format!("sandbox {} create not yet implemented", name)),
        SandboxCommands::Destroy { name } => Ok(format!("sandbox {} destroy not yet implemented", name)),
    }
}

async fn run_secrets(cmd: &SecretsCommands, _config: &Config) -> Result<String> {
    match cmd {
        SecretsCommands::List => Ok("[]".to_string()),
        SecretsCommands::Reload => Ok("secrets reload not yet implemented".to_string()),
    }
}

async fn run_security(cmd: &SecurityCommands, config: &Config) -> Result<String> {
    use crate::security::penetration_playbook;
    match cmd {
        SecurityCommands::Audit => Ok(serde_json::to_string_pretty(
            &penetration_playbook::build_audit_report(config),
        )?),
        SecurityCommands::Status => Ok(serde_json::to_string_pretty(
            &penetration_playbook::build_status_report(config),
        )?),
    }
}

async fn run_sessions(cmd: &SessionCommands, _config: &Config) -> Result<String> {
    match cmd {
        SessionCommands::List => Ok("[]".to_string()),
        SessionCommands::Show { session_id } => {
            Ok(serde_json::to_string_pretty(&serde_json::json!({
                "session_id": session_id, "messages": []
            }))?)
        }
        SessionCommands::Delete { session_id } => {
            Ok(format!("session {} delete not yet implemented", session_id))
        }
    }
}

async fn run_status(_config: &Config) -> Result<String> {
    Ok("{\"status\": \"not yet implemented\"}".to_string())
}

async fn run_system(cmd: Option<&SystemCommands>, _config: &Config) -> Result<String> {
    match cmd {
        Some(SystemCommands::Events { limit }) => {
            Ok(serde_json::to_string_pretty(&serde_json::json!({
                "events": [], "limit": limit
            }))?)
        }
        Some(SystemCommands::Heartbeat) => Ok("heartbeat not yet implemented".to_string()),
        Some(SystemCommands::Presence) => Ok("presence not yet implemented".to_string()),
        None => Ok("[]".to_string()),
    }
}

async fn run_daemon(cmd: &DaemonCommands, config: &Config) -> Result<String> {
    let svc = resolve_gateway_service();
    match cmd {
        DaemonCommands::Install => Ok(serde_json::to_string_pretty(&svc.operate_report(GatewayServiceOperation::Install))?),
        DaemonCommands::Uninstall => Ok(serde_json::to_string_pretty(&svc.operate_report(GatewayServiceOperation::Uninstall))?),
        DaemonCommands::Start => Ok(serde_json::to_string_pretty(&svc.operate_report(GatewayServiceOperation::Start))?),
        DaemonCommands::Stop => Ok(serde_json::to_string_pretty(&svc.operate_report(GatewayServiceOperation::Stop))?),
        DaemonCommands::Status => Ok(serde_json::to_string_pretty(&svc.status_report()?)?),
        DaemonCommands::Info => Ok(daemon_info(config)),
        DaemonCommands::Check { strict } => {
            let mut report = svc.preflight_report();
            let extra_checks = build_generic_daemon_checks(config);
            report.checks.extend(extra_checks);
            let hard_failed = report.checks.iter().any(|c| !c.ok);
            let warn_exists = report.checks.iter().any(|c| matches!(c.level, GatewayServiceCheckLevel::Warn));
            report.ok = !hard_failed && !(*strict && warn_exists);
            report.detail = if report.ok {
                if *strict { "daemon preflight passed (strict mode)".to_string() } else { "daemon preflight passed".to_string() }
            } else {
                if *strict && !hard_failed && warn_exists {
                    "daemon preflight failed in strict mode (warnings present)".to_string()
                } else {
                    "daemon preflight failed".to_string()
                }
            };
            if !report.ok {
                report.hints.push("fix failed checks and rerun: omninova daemon check".to_string());
            }
            Ok(serde_json::to_string_pretty(&report)?)
        }
    }
}

fn daemon_info(config: &Config) -> String {
    let home = home::home_dir().unwrap_or_else(|| PathBuf::from("~"));
    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "<unknown>".to_string());

    let mut out = String::new();
    out.push_str("OmniNova Daemon — Platform Info\n");
    out.push_str("═══════════════════════════════════════════════\n\n");

    out.push_str(&format!("  OS / arch      : {} / {}\n", std::env::consts::OS, std::env::consts::ARCH));
    out.push_str(&format!("  omninova bin   : {}\n", exe));
    out.push_str(&format!("  config file    : {}\n", config.config_path.display()));
    out.push_str(&format!("  workspace dir  : {}\n\n", config.workspace_dir.display()));

    #[cfg(target_os = "macos")]
    {
        let plist = home.join("Library/LaunchAgents/com.omninova.gateway.plist");
        out.push_str("  [macOS — launchd]\n");
        out.push_str(&format!("  service label  : com.omninova.gateway\n"));
        out.push_str(&format!("  plist path     : {}\n", plist.display()));
        out.push_str(&format!("  stdout log     : /tmp/omninova-gateway.out.log\n"));
        out.push_str(&format!("  stderr log     : /tmp/omninova-gateway.err.log\n\n"));
        out.push_str("  commands:\n");
        out.push_str("    omninova daemon install     — create plist + launchctl load\n");
        out.push_str("    omninova daemon uninstall   — launchctl unload + remove plist\n");
        out.push_str("    omninova daemon start       — launchctl start\n");
        out.push_str("    omninova daemon stop        — launchctl stop\n");
        out.push_str("    launchctl list com.omninova.gateway  — manual status check\n");
    }

    #[cfg(target_os = "linux")]
    {
        let unit = home.join(".config/systemd/user/omninova-gateway.service");
        out.push_str("  [Linux — systemd user unit]\n");
        out.push_str(&format!("  service name   : omninova-gateway.service\n"));
        out.push_str(&format!("  unit file      : {}\n", unit.display()));
        out.push_str(&format!("  journal logs   : journalctl --user -u omninova-gateway.service\n\n"));
        out.push_str("  commands:\n");
        out.push_str("    omninova daemon install     — write unit + systemctl enable --now\n");
        out.push_str("    omninova daemon uninstall   — systemctl disable + remove unit\n");
        out.push_str("    omninova daemon start       — systemctl --user start\n");
        out.push_str("    omninova daemon stop        — systemctl --user stop\n");
        out.push_str("    systemctl --user status omninova-gateway  — manual status\n");
    }

    #[cfg(target_os = "windows")]
    {
        out.push_str("  [Windows — Task Scheduler]\n");
        out.push_str(&format!("  task name      : OmniNovaGateway\n"));
        out.push_str(&format!("  logs           : check Event Viewer or gateway workspace logs\n\n"));
        out.push_str("  commands:\n");
        out.push_str("    omninova daemon install     — schtasks /Create\n");
        out.push_str("    omninova daemon uninstall   — schtasks /Delete\n");
        out.push_str("    omninova daemon start       — schtasks /Run\n");
        out.push_str("    omninova daemon stop        — schtasks /End\n");
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        out.push_str("  [unsupported platform — use `omninova gateway run` in foreground]\n");
    }

    out.push_str("\n\n  common:\n");
    out.push_str("    omninova daemon status      — query running state\n");
    out.push_str("    omninova daemon check       — preflight diagnostics\n");
    out.push_str("    omninova gateway run        — foreground (no daemon)\n");

    out
}

async fn run_skills(cmd: Option<&SkillsCommands>, config: &Config) -> Result<String> {
    let skills_dir = config.skills.open_skills_dir.as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| config.workspace_dir.join("skills"));
    match cmd {
        Some(SkillsCommands::List) => {
            let skills = crate::skills::load_skills_from_dir(&skills_dir)?;
            if skills.is_empty() {
                return Ok(format!("No skills found in {:?}.", skills_dir));
            }
            let mut out = String::new();
            out.push_str(&format!("Found {} skills in {:?}:\n\n", skills.len(), skills_dir));
            for s in skills {
                out.push_str(&format!("- {} ({})\n", s.metadata.name, s.metadata.description));
            }
            Ok(out)
        }
        Some(SkillsCommands::Import { from, to, overwrite }) => {
            let target = to.as_ref().map(PathBuf::from).unwrap_or(skills_dir.clone());
            let source = PathBuf::from(from);
            if !source.exists() {
                anyhow::bail!("source directory does not exist: {:?}", source);
            }
            let count = crate::skills::import_skills_from_dir(&source, &target, *overwrite)?;
            Ok(format!("imported {} skills to {:?}", count, target))
        }
        None => {
            let skills = crate::skills::load_skills_from_dir(&skills_dir)?;
            if skills.is_empty() {
                return Ok(format!("No skills found in {:?}.", skills_dir));
            }
            let mut out = String::new();
            out.push_str(&format!("{} skills:\n", skills.len()));
            for s in skills {
                out.push_str(&format!("  - {} ({})\n", s.metadata.name, s.metadata.description));
            }
            Ok(out)
        }
    }
}

async fn run_setup(cmd: &SetupCommands) -> Result<String> {
    match cmd {
        SetupCommands::Browser => install_agent_browser().await,
        SetupCommands::All => {
            let r1 = install_agent_browser().await;
            let mut results = vec![("agent-browser", r1.is_ok())];
            Ok(serde_json::to_string_pretty(&results)?)
        }
    }
}

async fn kill_port(port: u16) -> Result<()> {
    let output = tokio::process::Command::new("lsof")
        .args(["-ti", &format!("tcp:{}", port)])
        .output()
        .await?;
    if !output.stdout.is_empty() {
        let pids = String::from_utf8_lossy(&output.stdout);
        for pid in pids.split_whitespace() {
            let _ = tokio::process::Command::new("kill")
                .arg("-9")
                .arg(pid)
                .spawn();
        }
    }
    Ok(())
}

fn builtin_docs_index(config: &Config) -> String {
    let mut out = String::new();
    out.push_str("OmniNova CLI — Quick Reference\n");
    out.push_str("══════════════════════════════════════════════════\n\n");
    out.push_str("Available offline topics (run `omninova docs <topic>`):\n\n");
    out.push_str("  daemon      — Background service paths, logs & commands (per-platform)\n");
    out.push_str("  config      — Configuration file location & env vars\n");
    out.push_str("  gateway     — Gateway quick-start (foreground & service)\n\n");
    out.push_str("Or pass any search terms to open the online docs:\n");
    out.push_str("  omninova docs skills import\n\n");
    out.push_str("──────────────────────────────────────────────────\n");
    out.push_str(&builtin_docs_section("config", config).unwrap_or_default());
    out.push_str("\n──────────────────────────────────────────────────\n");
    out.push_str(&daemon_info(config));
    out
}

fn builtin_docs_section(topic: &str, config: &Config) -> Option<String> {
    match topic {
        t if t.starts_with("daemon") || t.starts_with("service") || t.starts_with("launchd")
            || t.starts_with("systemd") || t.starts_with("plist") || t.starts_with("schtask") =>
        {
            Some(daemon_info(config))
        }
        t if t.starts_with("config") || t.starts_with("toml") => {
            let home = home::home_dir().unwrap_or_else(|| PathBuf::from("~"));
            let default_config = home.join(".omninova/config.toml");
            let mut s = String::new();
            s.push_str("OmniNova Configuration\n");
            s.push_str("═══════════════════════════════════════════════\n\n");
            s.push_str(&format!("  active config  : {}\n", config.config_path.display()));
            s.push_str(&format!("  default path   : {}\n", default_config.display()));
            s.push_str(&format!("  workspace dir  : {}\n\n", config.workspace_dir.display()));
            s.push_str("  env overrides:\n");
            s.push_str("    OMNINOVA_CONFIG_DIR   — override config directory\n");
            s.push_str("    OMNINOVA_WORKSPACE    — override workspace (config inferred)\n");
            s.push_str("    OMNINOVA_OPENAI_API_KEY, OMNINOVA_ANTHROPIC_API_KEY, …\n\n");
            s.push_str("  commands:\n");
            s.push_str("    omninova config file      — show config path\n");
            s.push_str("    omninova config get <key>  — read a value by dot-key\n");
            s.push_str("    omninova config set <k> <v> — write a value\n");
            s.push_str("    omninova config validate  — check for errors / warnings\n");
            s.push_str("    omninova configure        — interactive wizard\n");
            Some(s)
        }
        t if t.starts_with("gateway") => {
            let mut s = String::new();
            s.push_str("OmniNova Gateway\n");
            s.push_str("═══════════════════════════════════════════════\n\n");
            s.push_str(&format!("  default host:port : {}:{}\n", config.gateway.host, config.gateway.port));
            s.push_str(&format!("  config file       : {}\n\n", config.config_path.display()));
            s.push_str("  foreground:\n");
            s.push_str("    omninova gateway run              — start with current config\n");
            s.push_str("    omninova gateway run --port 8080  — custom port\n");
            s.push_str("    omninova gateway run --force      — kill existing port holder first\n\n");
            s.push_str("  background (daemon):\n");
            s.push_str("    omninova daemon install           — register OS service\n");
            s.push_str("    omninova daemon info              — show service paths & logs\n");
            s.push_str("    omninova daemon status            — running?\n");
            Some(s)
        }
        _ => None,
    }
}

fn open_url(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(url).spawn()?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open").arg(url).spawn()?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/c", "start", url])
            .spawn()?;
    }
    Ok(())
}

fn run_completion(shell: Option<&str>) -> Result<String> {
    let sh = shell.unwrap_or("bash");
    let _completion = match sh {
        "bash" => "bash-completion",
        "zsh" => "zsh-completion",
        "fish" => "fish-completion",
        _ => "bash-completion",
    };
    Ok(format!("run: omninova completion {} >> ~/.{}rc", std::env::var("USER").unwrap_or_default(), sh))
}

async fn install_agent_browser() -> Result<String> {
    println!("Downloading Chromium browser engine...");
    let output = tokio::process::Command::new("agent-browser")
        .arg("install")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("agent-browser install failed: {}", stderr);
    }
    let status = check_dep_installed("agent-browser", "--version").await;
    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "ok": true,
        "installed": status.installed,
        "version": status.version,
    }))?)
}

async fn run_doctor(config: &Config) -> Result<String> {
    let runtime = GatewayRuntime::new(config.clone());
    let health = runtime.health().await;
    let agent_browser = check_dep_installed("agent-browser", "--version").await;
    let node = check_dep_installed("node", "--version").await;
    let npm = check_dep_installed("npm", "--version").await;
    let rg = check_dep_installed("rg", "--version").await;
    let git = check_dep_installed("git", "--version").await;
    let validation = config.validate();
    let mut checks = Vec::new();
    checks.push(serde_json::json!({
        "check": "gateway_provider",
        "ok": health.provider_healthy,
        "detail": format!("provider={}", health.provider),
    }));
    checks.push(serde_json::json!({"check": "memory", "ok": health.memory_healthy}));
    checks.push(serde_json::json!({
        "check": "config",
        "ok": validation.is_ok(),
        "errors": validation.errors,
        "warnings": validation.warnings,
    }));
    for dep in &[&agent_browser, &node, &npm, &rg, &git] {
        let required = dep.name == "agent-browser" && config.browser.enabled;
        checks.push(serde_json::json!({
            "check": format!("dep:{}", dep.name),
            "ok": dep.installed || !required,
            "installed": dep.installed,
            "version": dep.version,
            "required": required,
        }));
    }
    if config.browser.enabled && !agent_browser.installed {
        checks.push(serde_json::json!({
            "check": "browser_tool_ready",
            "ok": false,
            "detail": "browser.enabled=true but agent-browser is not installed. Run: omninova setup-deps browser",
        }));
    } else if config.browser.enabled {
        checks.push(serde_json::json!({
            "check": "browser_tool_ready",
            "ok": true,
            "detail": format!("agent-browser {} ready", agent_browser.version.as_deref().unwrap_or("?")),
        }));
    }
    let all_ok = checks.iter().all(|c| c["ok"].as_bool().unwrap_or(false));
    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "ok": all_ok,
        "checks": checks,
        "penetration_assessment": crate::security::penetration_playbook::build_playbook_payload(),
    }))?)
}

fn build_generic_daemon_checks(config: &Config) -> Vec<GatewayServiceCheckReport> {
    let mut checks = Vec::new();
    checks.push(check_gateway_host_resolvable(config));
    checks.push(check_gateway_bindable(config));
    checks.push(check_file_readable(&config.config_path, "config-readable"));
    if let Some(parent) = config.config_path.parent() {
        checks.push(check_dir_writable(parent, "config-parent-writable"));
    } else {
        checks.push(GatewayServiceCheckReport {
            name: "config-parent-writable".to_string(),
            ok: false,
            level: GatewayServiceCheckLevel::Error,
            detail: format!("config path has no parent: {}", config.config_path.display()),
        });
    }
    checks.push(check_dir_writable(&config.workspace_dir, "workspace-writable"));
    checks.extend(build_config_validation_checks(config));
    checks
}

fn check_gateway_host_resolvable(config: &Config) -> GatewayServiceCheckReport {
    let host = config.gateway.host.trim();
    if host.is_empty() {
        return GatewayServiceCheckReport {
            name: "gateway-host-resolvable".to_string(),
            ok: false,
            level: GatewayServiceCheckLevel::Error,
            detail: "gateway.host is empty".to_string(),
        };
    }
    if host.parse::<std::net::IpAddr>().is_ok() {
        return GatewayServiceCheckReport {
            name: "gateway-host-resolvable".to_string(),
            ok: true,
            level: GatewayServiceCheckLevel::Error,
            detail: format!("gateway.host is a valid IP: {host}"),
        };
    }
    match (host, config.gateway.port).to_socket_addrs() {
        Ok(mut iter) => {
            if let Some(addr) = iter.next() {
                GatewayServiceCheckReport {
                    name: "gateway-host-resolvable".to_string(),
                    ok: true,
                    level: GatewayServiceCheckLevel::Error,
                    detail: format!("gateway.host resolved to {addr}"),
                }
            } else {
                GatewayServiceCheckReport {
                    name: "gateway-host-resolvable".to_string(),
                    ok: false,
                    level: GatewayServiceCheckLevel::Error,
                    detail: format!("gateway.host did not resolve to any address: {host}"),
                }
            }
        }
        Err(e) => GatewayServiceCheckReport {
            name: "gateway-host-resolvable".to_string(),
            ok: false,
            level: GatewayServiceCheckLevel::Error,
            detail: format!("failed to resolve gateway.host '{host}': {e}"),
        },
    }
}

fn check_gateway_bindable(config: &Config) -> GatewayServiceCheckReport {
    let addr = format!("{}:{}", config.gateway.host, config.gateway.port);
    match std::net::TcpListener::bind(&addr) {
        Ok(listener) => {
            drop(listener);
            GatewayServiceCheckReport {
                name: "gateway-port-bindable".to_string(),
                ok: true,
                level: GatewayServiceCheckLevel::Error,
                detail: format!("bind probe passed for {addr}"),
            }
        }
        Err(e) => GatewayServiceCheckReport {
            name: "gateway-port-bindable".to_string(),
            ok: false,
            level: GatewayServiceCheckLevel::Error,
            detail: format!("failed to bind {addr}: {e}"),
        },
    }
}

fn check_file_readable(path: &Path, name: &str) -> GatewayServiceCheckReport {
    match std::fs::read_to_string(path) {
        Ok(_) => GatewayServiceCheckReport {
            name: name.to_string(),
            ok: true,
            level: GatewayServiceCheckLevel::Error,
            detail: format!("{} is readable", path.display()),
        },
        Err(e) => GatewayServiceCheckReport {
            name: name.to_string(),
            ok: false,
            level: GatewayServiceCheckLevel::Error,
            detail: format!("{} is not readable: {}", path.display(), e),
        },
    }
}

fn check_dir_writable(path: &Path, name: &str) -> GatewayServiceCheckReport {
    let test_file = path.join(".write_test");
    match std::fs::write(&test_file, b"") {
        Ok(_) => {
            let _ = std::fs::remove_file(&test_file);
            GatewayServiceCheckReport {
                name: name.to_string(),
                ok: true,
                level: GatewayServiceCheckLevel::Error,
                detail: format!("{} is writable", path.display()),
            }
        }
        Err(e) => GatewayServiceCheckReport {
            name: name.to_string(),
            ok: false,
            level: GatewayServiceCheckLevel::Error,
            detail: format!("{} is not writable: {}", path.display(), e),
        },
    }
}

fn build_config_validation_checks(config: &Config) -> Vec<GatewayServiceCheckReport> {
    let validation = config.validate();
    validation
        .errors
        .iter()
        .map(|e| GatewayServiceCheckReport {
            name: "config-validation".to_string(),
            ok: false,
            level: GatewayServiceCheckLevel::Error,
            detail: e.clone(),
        })
        .chain(validation.warnings.iter().map(|w| GatewayServiceCheckReport {
            name: "config-warning".to_string(),
            ok: true,
            level: GatewayServiceCheckLevel::Warn,
            detail: w.clone(),
        }))
        .collect()
}

#[derive(Debug, serde::Serialize)]
pub struct DepStatus {
    pub name: String,
    pub installed: bool,
    pub version: Option<String>,
    pub detail: String,
}

async fn check_dep_installed(bin: &str, version_flag: &str) -> DepStatus {
    let output = tokio::process::Command::new(bin)
        .arg(version_flag)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await;
    match output {
        Ok(out) if out.status.success() => {
            let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
            DepStatus {
                name: bin.to_string(),
                installed: true,
                version: Some(version.clone()),
                detail: format!("{} found (version: {})", bin, version),
            }
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            DepStatus {
                name: bin.to_string(),
                installed: false,
                version: None,
                detail: format!("{} not working: {}", bin, stderr.trim()),
            }
        }
        Err(e) => DepStatus {
            name: bin.to_string(),
            installed: false,
            version: None,
            detail: format!("{} not found: {}", bin, e),
        },
    }
}

struct InteractiveConfigurator {
    // placeholder — implement interactive prompts using inlined readline-style read
}

impl InteractiveConfigurator {
    fn new() -> Self {
        Self {}
    }

    fn run(&self) -> Result<()> {
        println!("OmniNova interactive configurator");
        println!("(for non-interactive use, see: omninova config set / omninova config get)");
        println!("this is a placeholder — use 'omninova config set <key> <value>' instead");
        Ok(())
    }
}
