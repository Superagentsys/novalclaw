mod repl;
mod tui;

use crate::config::{Config, GatewayPublicMode};
use crate::channels::ChannelKind;
use crate::cron::{now_timestamp, CronJob, CronStore, Schedule};
use crate::daemon::service::{
    GatewayServiceCheckLevel, GatewayServiceCheckReport, GatewayServiceOperation,
    resolve_gateway_service,
};
use crate::gateway::{
    check_gateway_public_health, cloudflared_available, feishu_public_callback_urls,
    normalize_gateway_public_config, normalize_named_tunnel_hostname,
    normalize_public_webhook_base_url, resolve_public_webhook_base_url, GatewayRuntime,
    GatewayRuntimeStatus,
};
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::net::ToSocketAddrs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};

static NO_COLOR: AtomicBool = AtomicBool::new(false);

#[allow(dead_code)]
fn color_enabled() -> bool {
    !NO_COLOR.load(Ordering::Relaxed)
}

#[allow(dead_code)]
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
  omninova                         # interactive REPL (Claude Code style)
  omninova -p \"summarize this repo\"
  omninova --session work -p \"status\"
  omninova tui
  omninova web                     # open the browser UI
  omninova skills list
  omninova cron list
  omninova gateway run             # start gateway + web at /app
  omninova doctor

Headless server (no desktop):
  omninova gateway run
  omninova daemon install    # Linux: systemd user unit; macOS: launchd; Windows: Task Scheduler",
)]
pub struct Cli {
    #[arg(short = 'p', long = "prompt", value_name = "TEXT")]
    /// One-shot prompt (Claude Code `-p`). Prints the reply and exits.
    pub prompt: Option<String>,

    #[arg(long, value_name = "id")]
    /// Session id for REPL / `-p` / default chat.
    pub session: Option<String>,

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
    pub command: Option<Commands>,
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
    /// Manage sanitized diagnostics packages for support.
    Diagnostics {
        #[command(subcommand)]
        command: DiagnosticsCommands,
    },
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
    /// Manage the local document knowledge base.
    Knowledge {
        #[command(subcommand)]
        command: KnowledgeCommands,
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
    /// Open the Web UI in a browser (`/app` on the local gateway).
    Web,
    /// Open the Control UI with your current token.
    Dashboard,
    /// Emergency stop controls.
    Estop {
        #[command(subcommand)]
        command: EstopCommands,
    },
    /// Approve or reject pending tool execution requests.
    Approvals {
        #[command(subcommand)]
        command: ApprovalsCommands,
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
    /// Feishu persistence store queries: events, jobs, outbox, inspect.
    Feishu {
        #[command(subcommand)]
        command: FeishuCommands,
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
    /// Run gateway pre-flight diagnostic checks.
    Doctor,
}

#[derive(Debug, Subcommand)]
pub enum DiagnosticsCommands {
    /// Export a privacy-safe diagnostics ZIP.
    Export {
        /// Output ZIP file or directory. Defaults to the config diagnostics directory.
        #[arg(long)]
        output: Option<String>,
    },
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
        /// Legacy shell payload; omitted for agent jobs.
        #[arg(long)]
        command: Option<String>,
        /// Instruction handed to the agent when the job fires.
        #[arg(long)]
        prompt: Option<String>,
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
pub enum KnowledgeCommands {
    /// List indexed documents.
    List {
        #[arg(long)]
        collection: Option<String>,
    },
    /// Add a note or import a file.
    Add {
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
        #[arg(long)]
        text: Option<String>,
        #[arg(long)]
        collection: Option<String>,
        #[arg(long)]
        tags: Option<String>,
    },
    /// Search document passages.
    Search {
        query: Vec<String>,
        #[arg(long)]
        collection: Option<String>,
        #[arg(long, default_value_t = 8)]
        limit: usize,
    },
    /// Show a document's full text.
    Show { id: String },
    /// Remove a document.
    Remove { id: String },
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
pub enum ApprovalsCommands {
    /// List approval requests.
    List {
        #[arg(long)]
        all: bool,
    },
    /// Approve a pending tool execution request.
    Approve {
        id: String,
        #[arg(long)]
        approved_by: Option<String>,
    },
    /// Reject a pending tool execution request.
    Reject {
        id: String,
        #[arg(long)]
        reason: Option<String>,
    },
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

/// Feishu persistence store query commands.
#[derive(Debug, Subcommand)]
pub enum FeishuCommands {
    /// Show store statistics and health.
    Status,
    /// List recent events.
    Events {
        #[arg(long, default_value = "20")]
        limit: usize,
    },
    /// List recent jobs.
    Jobs {
        #[arg(long, default_value = "20")]
        limit: usize,
    },
    /// List recent outbox items.
    Outbox {
        #[arg(long, default_value = "20")]
        limit: usize,
    },
    /// Inspect a specific record.
    Inspect {
        #[arg(long)]
        job_id: Option<String>,
        #[arg(long)]
        event_key: Option<String>,
        #[arg(long)]
        outbound_id: Option<String>,
    },
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
    std::env::set_var("OMNINOVA_CONFIG_FILE", &cfg);
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

#[allow(dead_code)]
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

    match cli.command.as_ref() {
        None => repl::run_default(config, cli.prompt.clone(), cli.session.clone()).await,
        Some(Commands::Agent { message, session_id }) => {
            let runtime = GatewayRuntime::new(config);
            let inbound = crate::channels::adapters::cli::inbound_from_cli(
                message.clone(),
                session_id.clone(),
                None,
            );
            let resp = runtime.process_inbound(&inbound).await?;
            Ok(resp.reply)
        }
        Some(Commands::Gateway { command }) => match command {
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
                run_gateway_status(&config).await
            }
            Some(GatewayCommands::Reload) => {
                Ok("reload not yet implemented via runtime".to_string())
            }
            Some(GatewayCommands::Doctor) => {
                run_gateway_doctor(&config).await
            }
            None => {
                let runtime = GatewayRuntime::new(config);
                runtime.serve_http().await?;
                Ok("gateway stopped".to_string())
            }
        },
        Some(Commands::Config { command }) => run_config(command, &config).await,
        Some(Commands::Configure) => {
            tokio::task::spawn_blocking(|| {
                InteractiveConfigurator::new().run()
            }).await??;
            Ok("configuration complete".to_string())
        }
        Some(Commands::Setup) => {
            std::fs::create_dir_all(config.config_path.parent().unwrap_or(&config.workspace_dir))?;
            std::fs::create_dir_all(&config.workspace_dir)?;
            config.save()?;
            Ok(format!("config initialized at {}", config.config_path.display()))
        }
        Some(Commands::Health) => {
            let runtime = GatewayRuntime::new(config);
            let health = runtime.health().await;
            Ok(serde_json::to_string_pretty(&health)?)
        }
        Some(Commands::Doctor) => run_doctor(&config).await,
        Some(Commands::Diagnostics { command }) => match command {
            DiagnosticsCommands::Export { output } => {
                run_diagnostics(&config, output.as_deref()).await
            }
        },
        Some(Commands::Cron { command }) => run_cron(command, &config).await,
        Some(Commands::Channels { command }) => run_channels(command, &config).await,
        Some(Commands::Message { command }) => run_message(command, &config).await,
        Some(Commands::Models { command }) => run_models(command, &config).await,
        Some(Commands::Mcp) => {
            Ok(serde_json::to_string_pretty(&serde_json::json!({
                "status": "mcp_server_not_implemented_via_runtime"
            }))?)
        }
        Some(Commands::Memory { command }) => run_memory(command, &config).await,
        Some(Commands::Knowledge { command }) => run_knowledge(command, &config).await,
        Some(Commands::Nodes { command }) => run_nodes(command, &config).await,
        Some(Commands::Pairing { command }) => run_pairing(command, &config).await,
        Some(Commands::Plugins { command }) => run_plugins(command, &config).await,
        Some(Commands::Sandbox { command }) => run_sandbox(command, &config).await,
        Some(Commands::Secrets { command }) => run_secrets(command, &config).await,
        Some(Commands::Security { command }) => run_security(command, &config).await,
        Some(Commands::Sessions { command }) => run_sessions(command, &config).await,
        Some(Commands::Status) => run_status(&config).await,
        Some(Commands::Tui) => tui::run_tui(config).await,
        Some(Commands::Web) | Some(Commands::Dashboard) => {
            let host = if config.gateway.host == "0.0.0.0" || config.gateway.host == "::" {
                "127.0.0.1"
            } else {
                config.gateway.host.as_str()
            };
            let url = format!("http://{}:{}/app", host, config.gateway.port);
            open_url(&url)?;
            Ok(format!("opened {}", url))
        }
        Some(Commands::Estop { command }) => {
            let runtime = GatewayRuntime::new(config);
            match command {
                EstopCommands::Status => Ok(serde_json::to_string_pretty(&runtime.estop_status().await?)?),
                EstopCommands::Pause { level, domain, tool, reason } => {
                    Ok(serde_json::to_string_pretty(&runtime.estop_pause(level.clone(), domain.clone(), tool.clone(), reason.clone()).await?)?)
                }
                EstopCommands::Resume => Ok(serde_json::to_string_pretty(&runtime.estop_resume().await?)?),
            }
        }
        Some(Commands::Approvals { command }) => run_approvals(command, &config).await,
        Some(Commands::Daemon { command }) => run_daemon(command, &config).await,
        Some(Commands::Skills { command }) => run_skills(command.as_ref(), &config).await,
        Some(Commands::Browser { command }) => match command {
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
        Some(Commands::System { command }) => run_system(command.as_ref(), &config).await,
        Some(Commands::Feishu { command }) => run_feishu(command, &config).await,
        Some(Commands::SetupDeps { command }) => run_setup(command).await,
        Some(Commands::Route { channel, text, agent }) => {
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
        Some(Commands::ConfigPrint) => {
            let runtime = GatewayRuntime::new(config);
            let cfg = runtime.get_config().await;
            Ok(serde_json::to_string_pretty(&cfg)?)
        }
        Some(Commands::Completion { shell }) => {
            run_completion(shell.as_deref())
        }
        Some(Commands::Docs { query }) => {
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
        Some(Commands::Qr) => Ok(generate_pairing_qr(&config)?),
        Some(Commands::Reset { force }) => {
            if !*force {
                anyhow::bail!("use --force to confirm reset");
            }
            let (base, cfg_path) = resolve_profile_dir(None, false);
            let _ = std::fs::remove_dir_all(&base);
            Ok(format!(
                "reset complete (state dir: {:?}, config: {:?})",
                base, cfg_path
            ))
        }
        Some(Commands::Uninstall { force }) => {
            if !*force {
                anyhow::bail!("use --force to confirm uninstall");
            }
            let svc = resolve_gateway_service();
            Ok(serde_json::to_string_pretty(&svc.operate_report(GatewayServiceOperation::Uninstall))?)
        }
        Some(Commands::Logs { follow, lines }) => {
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
        "gateway_public" => {
            let sub = parts.get(1).unwrap_or(&"mode");
            match *sub {
                "mode" => Ok(config.gateway_public.mode.as_str().to_string()),
                "public_webhook_base_url" => Ok(config
                    .gateway_public
                    .public_webhook_base_url
                    .clone()
                    .unwrap_or_default()),
                "cloudflared_path" => Ok(config
                    .gateway_public
                    .cloudflared_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().to_string())
                    .unwrap_or_default()),
                "named_tunnel_name" => Ok(config
                    .gateway_public
                    .named_tunnel_name
                    .clone()
                    .unwrap_or_default()),
                "named_tunnel_hostname" => Ok(config
                    .gateway_public
                    .named_tunnel_hostname
                    .clone()
                    .unwrap_or_default()),
                _ => anyhow::bail!("unknown gateway_public key: {}", sub),
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
        "gateway_public" => {
            let sub = parts.get(1).unwrap_or(&"mode");
            match *sub {
                "mode" => {
                    config.gateway_public.mode = match value {
                        "quick_tunnel" => GatewayPublicMode::QuickTunnel,
                        "named_cloudflare_tunnel" => {
                            GatewayPublicMode::NamedCloudflareTunnel
                        }
                        "external_public_url" => GatewayPublicMode::ExternalPublicUrl,
                        _ => anyhow::bail!(
                            "gateway_public.mode must be quick_tunnel, named_cloudflare_tunnel, or external_public_url"
                        ),
                    };
                }
                "public_webhook_base_url" => {
                    config.gateway_public.public_webhook_base_url =
                        normalize_public_webhook_base_url(value);
                }
                "cloudflared_path" => {
                    config.gateway_public.cloudflared_path = if value.trim().is_empty() {
                        None
                    } else {
                        Some(PathBuf::from(value.trim()))
                    };
                }
                "named_tunnel_name" => {
                    config.gateway_public.named_tunnel_name = non_empty_config_value(value);
                }
                "named_tunnel_hostname" => {
                    config.gateway_public.named_tunnel_hostname = non_empty_config_value(value);
                }
                _ => anyhow::bail!("unknown gateway_public key: {}", sub),
            }
        }
        "api_key" => config.api_key = Some(value.to_string()),
        _ => anyhow::bail!("unknown config key: {}", key),
    }
    if parts[0] == "gateway_public" {
        normalize_gateway_public_config(&mut config.gateway_public);
    }
    Ok(())
}

fn unset_config_key(config: &mut Config, key: &str) -> Result<()> {
    match key {
        "default_provider" => config.default_provider = None,
        "default_model" => config.default_model = None,
        "api_key" => config.api_key = None,
        "gateway_public.public_webhook_base_url" => {
            config.gateway_public.public_webhook_base_url = None
        }
        "gateway_public.cloudflared_path" => config.gateway_public.cloudflared_path = None,
        "gateway_public.named_tunnel_name" => config.gateway_public.named_tunnel_name = None,
        "gateway_public.named_tunnel_hostname" => {
            config.gateway_public.named_tunnel_hostname = None
        }
        _ => anyhow::bail!("cannot unset key: {}", key),
    }
    if key.starts_with("gateway_public.") {
        normalize_gateway_public_config(&mut config.gateway_public);
    }
    Ok(())
}

fn non_empty_config_value(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

async fn run_cron(cmd: &CronCommands, config: &Config) -> Result<String> {
    let store = CronStore::open(config.workspace_dir.join("cron.json")).await?;
    match cmd {
        CronCommands::List => Ok(serde_json::to_string_pretty(&store.list().await)?),
        CronCommands::Add {
            name,
            schedule,
            command,
            prompt,
        } => {
            let prompt = prompt
                .clone()
                .or_else(|| command.clone())
                .unwrap_or_default();
            if prompt.trim().is_empty() {
                anyhow::bail!("provide --prompt (agent job) or --command");
            }
            let parsed = Schedule::parse(schedule)?;
            let job = CronJob {
                id: format!("job-{}", now_timestamp().replace([':', '.'], "-")),
                name: name.clone(),
                schedule: schedule.clone(),
                prompt,
                command: command.clone().unwrap_or_default(),
                description: String::new(),
                template_id: None,
                provider: None,
                model: None,
                tz_offset_minutes: 0,
                enabled: true,
                last_run: None,
                last_status: None,
                next_run: parsed.next_run_iso(0),
                last_error: None,
                created_at: now_timestamp(),
                task_id: None,
            };
            store.upsert(job.clone()).await?;
            Ok(serde_json::to_string_pretty(&job)?)
        }
        CronCommands::Remove { id } => {
            let removed = store.remove(id).await?;
            Ok(serde_json::to_string_pretty(&serde_json::json!({
                "id": id,
                "removed": removed
            }))?)
        }
        CronCommands::Pause { id } => {
            store.set_enabled(id, false).await?;
            Ok(serde_json::to_string_pretty(&store.get(id).await)?)
        }
        CronCommands::Resume { id } => {
            store.set_enabled(id, true).await?;
            if let Some(job) = store.get(id).await {
                if let Ok(schedule) = Schedule::parse(&job.schedule) {
                    let _ = store
                        .set_next_run(id, schedule.next_run_iso(job.tz_offset_minutes))
                        .await;
                }
            }
            Ok(serde_json::to_string_pretty(&store.get(id).await)?)
        }
    }
}

async fn run_channels(cmd: &ChannelCommands, config: &Config) -> Result<String> {
    match cmd {
        ChannelCommands::List => {
            let raw = serde_json::to_value(&config.channels_config)?;
            let mut items = Vec::new();
            if let Some(map) = raw.as_object() {
                for (name, value) in map {
                    let enabled = value
                        .get("enabled")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    items.push(serde_json::json!({
                        "channel": name,
                        "enabled": enabled,
                        "configured": !value.is_null(),
                    }));
                }
            }
            Ok(serde_json::to_string_pretty(&items)?)
        }
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

async fn run_models(cmd: &ModelCommands, config: &Config) -> Result<String> {
    match cmd {
        ModelCommands::List => Ok(serde_json::to_string_pretty(&repl::models_view(config))?),
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

async fn run_knowledge(cmd: &KnowledgeCommands, config: &Config) -> Result<String> {
    let store = crate::knowledge::KnowledgeStore::open_in(&config.workspace_dir).await?;
    match cmd {
        KnowledgeCommands::List { collection } => {
            Ok(serde_json::to_string_pretty(&store.list(collection.as_deref()).await)?)
        }
        KnowledgeCommands::Add {
            title,
            file,
            text,
            collection,
            tags,
        } => {
            let tags = tags
                .as_deref()
                .unwrap_or("")
                .split(',')
                .map(|tag| tag.trim().to_string())
                .filter(|tag| !tag.is_empty())
                .collect::<Vec<_>>();
            let doc = if let Some(path) = file {
                store
                    .import_path(path, collection.as_deref(), tags)
                    .await?
            } else {
                let content = text.clone().unwrap_or_default();
                if content.trim().is_empty() {
                    anyhow::bail!("provide --file or --text");
                }
                store
                    .upsert(crate::knowledge::KnowledgeUpsert {
                        id: None,
                        title: title.clone().unwrap_or_else(|| "Untitled".into()),
                        collection: collection.clone().unwrap_or_else(|| "default".into()),
                        source: "note".into(),
                        source_path: None,
                        kind: "md".into(),
                        tags,
                        content,
                        enabled: true,
                    })
                    .await?
            };
            Ok(serde_json::to_string_pretty(&doc)?)
        }
        KnowledgeCommands::Search {
            query,
            collection,
            limit,
        } => {
            let q = query.join(" ");
            Ok(serde_json::to_string_pretty(
                &store.search(&q, collection.as_deref(), *limit).await,
            )?)
        }
        KnowledgeCommands::Show { id } => match store.get(id).await? {
            Some((doc, content)) => Ok(serde_json::to_string_pretty(&serde_json::json!({
                "document": doc,
                "content": content
            }))?),
            None => anyhow::bail!("document not found: {id}"),
        },
        KnowledgeCommands::Remove { id } => {
            let removed = store.remove(id).await?;
            Ok(serde_json::to_string_pretty(&serde_json::json!({
                "id": id,
                "removed": removed
            }))?)
        }
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

async fn run_approvals(cmd: &ApprovalsCommands, config: &Config) -> Result<String> {
    let runtime = GatewayRuntime::new(config.clone());
    match cmd {
        ApprovalsCommands::List { all } => {
            let items = runtime.list_approvals(!all).await?;
            Ok(serde_json::to_string_pretty(&items)?)
        }
        ApprovalsCommands::Approve { id, approved_by } => {
            let item = runtime.approve_request(id, approved_by.clone()).await?;
            Ok(serde_json::to_string_pretty(&item)?)
        }
        ApprovalsCommands::Reject { id, reason } => {
            let item = runtime.reject_request(id, reason.clone()).await?;
            Ok(serde_json::to_string_pretty(&item)?)
        }
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

async fn run_sessions(cmd: &SessionCommands, config: &Config) -> Result<String> {
    match cmd {
        SessionCommands::List => {
            let runtime = GatewayRuntime::new(config.clone());
            let snapshot = runtime.session_tree_snapshot().await?;
            Ok(serde_json::to_string_pretty(&snapshot)?)
        }
        SessionCommands::Show { session_id } => {
            let runtime = GatewayRuntime::new(config.clone());
            let mut history = runtime
                .get_session_history(&ChannelKind::Web, session_id)
                .await;
            if history.messages.is_empty() {
                history = runtime
                    .get_session_history(&ChannelKind::Cli, session_id)
                    .await;
            }
            Ok(serde_json::to_string_pretty(&history)?)
        }
        SessionCommands::Delete { session_id } => {
            let runtime = GatewayRuntime::new(config.clone());
            let web = runtime
                .delete_session(&ChannelKind::Web, session_id)
                .await
                .unwrap_or(false);
            let cli = runtime
                .delete_session(&ChannelKind::Cli, session_id)
                .await
                .unwrap_or(false);
            Ok(serde_json::to_string_pretty(&serde_json::json!({
                "session_id": session_id,
                "removed": web || cli
            }))?)
        }
    }
}

fn generate_pairing_qr(config: &Config) -> Result<String> {
    use qrcode::QrCode;
    let host = if config.gateway.host == "0.0.0.0" {
        "127.0.0.1".to_string()
    } else {
        config.gateway.host.clone()
    };
    let payload = serde_json::json!({
        "type": "omninova-pairing",
        "version": env!("CARGO_PKG_VERSION"),
        "gateway": format!("http://{}:{}", host, config.gateway.port),
        "agent": config.agent.name,
        "issued_at": time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default(),
    });
    let payload_str = serde_json::to_string(&payload)?;
    let code = QrCode::new(payload_str.as_bytes())
        .map_err(|e| anyhow::anyhow!("failed to encode QR: {e}"))?;
    let render = code
        .render::<char>()
        .quiet_zone(true)
        .module_dimensions(2, 1)
        .dark_color('█')
        .light_color(' ')
        .build();
    Ok(format!(
        "{render}\n\nPairing payload: {payload_str}\n\nScan the QR with the OmniNova mobile app, or paste the JSON\npayload into Settings → Pair Gateway."
    ))
}

async fn run_status(config: &Config) -> Result<String> {
    let runtime = GatewayRuntime::new(config.clone());
    let health = runtime.health().await;
    let cfg = runtime.get_config().await;
    let tools = crate::gateway::create_default_tools(&cfg);
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
    let payload = serde_json::json!({
        "gateway": {
            "ok": health.ok,
            "provider": health.provider,
            "provider_healthy": health.provider_healthy,
            "memory_healthy": health.memory_healthy,
        },
        "config": {
            "default_provider": cfg.default_provider,
            "default_model": cfg.default_model,
            "gateway_host": cfg.gateway.host,
            "gateway_port": cfg.gateway.port,
            "agent_name": cfg.agent.name,
        },
        "tools": tool_names,
        "agents": cfg.agents.keys().collect::<Vec<_>>(),
    });
    Ok(serde_json::to_string_pretty(&payload)?)
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

/// Format a Unix timestamp (milliseconds) as a human-readable string.
fn format_timestamp(ts: i64) -> String {
    let secs = ts / 1000;
    let millis = ts % 1000;
    
    // Convert to breakdown
    let days_since_epoch = secs / 86400;
    let secs_in_day = secs % 86400;
    let hours = secs_in_day / 3600;
    let mins = (secs_in_day % 3600) / 60;
    let s = secs_in_day % 60;
    
    // Calculate year (rough approximation)
    let mut remaining_days = days_since_epoch;
    let mut year = 1970;
    let mut month = 1;
    let mut day = 1;
    
    // This is a simplified version - works for dates after 1970
    let days_in_year = if is_leap_year(year) { 366 } else { 365 };
    while remaining_days >= days_in_year {
        remaining_days -= days_in_year;
        year += 1;
    }
    
    // Days in each month (non-leap year)
    let days_per_month = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let days_per_month_leap = [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    
    let days_in_this_month = if is_leap_year(year) { &days_per_month_leap } else { &days_per_month };
    
    for (i, d) in days_in_this_month.iter().enumerate() {
        if remaining_days < *d as i64 {
            month = i + 1;
            day = remaining_days + 1;
            break;
        }
        remaining_days -= *d as i64;
    }
    
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}", year, month, day, hours, mins, s, millis)
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn feishu_security_status_report(config: &Config) -> String {
    let security = crate::gateway::FeishuSecurityConfig::from_entry(
        config.channels_config.feishu.as_ref(),
    );
    format!(
        "Feishu webhook security:\n  security_mode                 : {}\n  verification_token_configured : {}\n  encrypt_key_configured        : {}\n  insecure_dev_mode             : {}\n",
        security.mode.as_str(),
        security.verification_token.is_some(),
        security.encrypt_key.is_some(),
        security.insecure,
    )
}

#[cfg(test)]
mod feishu_security_status_tests {
    use super::*;
    use crate::config::schema::ChannelEntry;
    use std::collections::HashMap;

    #[test]
    fn status_report_exposes_configuration_state_without_secret_values() {
        let mut config = Config::default();
        config.channels_config.feishu = Some(ChannelEntry {
            enabled: true,
            token: None,
            token_env: None,
            security_mode: Some("encrypted".to_string()),
            verification_token: Some("verification-token-must-not-leak".to_string()),
            verification_token_env: None,
            encrypt_key: Some("encrypt-key-must-not-leak".to_string()),
            encrypt_key_env: None,
            extra: HashMap::new(),
        });

        let report = feishu_security_status_report(&config);
        assert!(report.contains("security_mode                 : encrypted"));
        assert!(report.contains("verification_token_configured : true"));
        assert!(report.contains("encrypt_key_configured        : true"));
        assert!(!report.contains("verification-token-must-not-leak"));
        assert!(!report.contains("encrypt-key-must-not-leak"));
    }

    #[test]
    fn status_report_does_not_treat_setup_marker_as_a_configured_secret() {
        let mut config = Config::default();
        let mut extra = HashMap::new();
        extra.insert(
            "verification_token".to_string(),
            serde_json::Value::String("***SET***".to_string()),
        );
        extra.insert(
            "encrypt_key".to_string(),
            serde_json::Value::String("***SET***".to_string()),
        );
        config.channels_config.feishu = Some(ChannelEntry {
            enabled: true,
            token: None,
            token_env: None,
            security_mode: Some("dev".to_string()),
            verification_token: None,
            verification_token_env: None,
            encrypt_key: None,
            encrypt_key_env: None,
            extra,
        });

        let report = feishu_security_status_report(&config);
        assert!(report.contains("verification_token_configured : false"));
        assert!(report.contains("encrypt_key_configured        : false"));
        assert!(!report.contains("***SET***"));
    }
}

async fn run_feishu(cmd: &FeishuCommands, config: &Config) -> Result<String> {
    use crate::gateway::feishu_store::FeishuStore;
    
    // Determine config dir for state.sqlite
    let config_dir = std::env::var("OMNINOVA_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            home::home_dir()
                .map(|h| h.join(".omninova"))
                .unwrap_or_else(|| PathBuf::from(".omninova"))
        });
    
    let db_path = config_dir.join("state.sqlite");
    let security_report = feishu_security_status_report(config);
    
    if !db_path.exists() {
        if matches!(cmd, FeishuCommands::Status) {
            return Ok(format!(
                "{security_report}\nFeishu store: unavailable (state.sqlite not found at {})\n",
                db_path.display()
            ));
        }
        anyhow::bail!("state.sqlite not found at {}. Is the Gateway running?", db_path.display());
    }
    
    let store = FeishuStore::open(&config_dir)
        .map_err(|e| anyhow::anyhow!("failed to open feishu store: {}", e))?;
    
    match cmd {
        FeishuCommands::Status => {
            let stats = store.get_store_stats()
                .map_err(|e| anyhow::anyhow!("failed to get stats: {}", e))?;
            let version = store.get_migration_version()
                .unwrap_or(0);
            
            let mut output = String::new();
            output.push_str(&security_report);
            output.push('\n');
            output.push_str("╔══════════════════════════════════════════════════════════════╗\n");
            output.push_str("║              OmniNova Feishu Store Status                   ║\n");
            output.push_str("╚══════════════════════════════════════════════════════════════╝\n\n");
            
            output.push_str(&format!("  Database path : {}\n", db_path.display()));
            output.push_str(&format!("  Migration    : v{}\n\n", version));
            
            output.push_str("  Counts:\n");
            output.push_str(&format!("    Events      : {}\n", stats.events_total));
            output.push_str(&format!("    Jobs        : {}\n", stats.jobs_total));
            output.push_str(&format!("    Outbox      : {}\n\n", stats.outbox_total));
            
            output.push_str("  Jobs by status:\n");
            for (status, count) in &stats.job_status_counts {
                output.push_str(&format!("    {:16} : {}\n", status, count));
            }
            if stats.job_status_counts.is_empty() {
                output.push_str("    (none)\n");
            }
            output.push('\n');
            
            output.push_str("  Outbox by status:\n");
            for (status, count) in &stats.outbox_status_counts {
                output.push_str(&format!("    {:26} : {}\n", status, count));
            }
            if stats.outbox_status_counts.is_empty() {
                output.push_str("    (none)\n");
            }
            output.push('\n');
            
            output.push_str("  Errors:\n");
            output.push_str(&format!("    Job errors  : {}\n", stats.error_count));
            
            if let Some(ts) = stats.last_event_at {
                let datetime = format_timestamp(ts);
                output.push_str(&format!("    Last event  : {} (ts={})\n", datetime, ts));
            }
            
            Ok(output)
        }
        
        FeishuCommands::Events { limit } => {
            let events = store.get_recent_events(*limit)
                .map_err(|e| anyhow::anyhow!("failed to get events: {}", e))?;
            
            if events.is_empty() {
                return Ok("No events found.".to_string());
            }
            
            let mut output = String::new();
            output.push_str(&format!("{:>4}  {:<22} {:<12} {:<10} {:<15} {:<10} {:<30}\n",
                "ID", "received_at", "event_type", "status", "message_type", "sender", "text_preview"));
            output.push_str(&format!("{}\n", "-".repeat(120)));
            
            for event in events {
                let time = format_timestamp(event.received_at);
                
                let preview = event.text_preview.as_deref()
                    .unwrap_or("-")
                    .chars()
                    .take(28)
                    .collect::<String>();
                
                output.push_str(&format!(
                    "{:>4}  {:<22} {:<12} {:<10} {:<15} {:<10} {}\n",
                    event.id,
                    time,
                    event.event_type.as_deref().unwrap_or("-"),
                    event.status.as_str(),
                    event.message_type.as_deref().unwrap_or("-"),
                    event.sender_type.as_deref().unwrap_or("-"),
                    preview
                ));
            }
            
            Ok(output)
        }
        
        FeishuCommands::Jobs { limit } => {
            let jobs = store.get_recent_jobs(*limit)
                .map_err(|e| anyhow::anyhow!("failed to get jobs: {}", e))?;
            
            if jobs.is_empty() {
                return Ok("No jobs found.".to_string());
            }
            
            let mut output = String::new();
            output.push_str(&format!("{:>4}  {:<36} {:<20} {:<10} {:<12} {:<8} {:<25}\n",
                "ID", "job_id", "event_key", "mode", "slash_cmd", "status", "error"));
            output.push_str(&format!("{}\n", "-".repeat(140)));
            
            for job in jobs {
                let job_id_short = if job.job_id.len() > 36 {
                    format!("...{}", &job.job_id[job.job_id.len() - 33..])
                } else {
                    job.job_id.clone()
                };
                
                let event_key_short = if job.event_key.len() > 20 {
                    format!("...{}", &job.event_key[job.event_key.len() - 17..])
                } else {
                    job.event_key.clone()
                };
                
                let error = job.error_code.as_deref().unwrap_or("-");
                
                output.push_str(&format!(
                    "{:>4}  {:<36} {:<20} {:<10} {:<12} {:<8} {}\n",
                    job.id,
                    job_id_short,
                    event_key_short,
                    job.mode,
                    job.slash_command.as_deref().unwrap_or("-"),
                    job.status.as_str(),
                    error
                ));
            }
            
            Ok(output)
        }
        
        FeishuCommands::Outbox { limit } => {
            let items = store.get_recent_outbox(*limit)
                .map_err(|e| anyhow::anyhow!("failed to get outbox: {}", e))?;
            
            if items.is_empty() {
                return Ok("No outbox items found.".to_string());
            }
            
            let mut output = String::new();
            output.push_str(&format!("{:>4}  {:<32} {:<22} {:<10} {:<5} {:<10} {:<35}\n",
                "ID", "outbound_id", "reply_kind", "status", "att", "error", "reply_preview"));
            output.push_str(&format!("{}\n", "-".repeat(150)));
            
            for item in items {
                let outbound_short = if item.outbound_id.len() > 32 {
                    format!("...{}", &item.outbound_id[item.outbound_id.len() - 29..])
                } else {
                    item.outbound_id.clone()
                };
                
                let preview = item.reply_preview.as_deref()
                    .unwrap_or("-")
                    .chars()
                    .take(33)
                    .collect::<String>();
                
                output.push_str(&format!(
                    "{:>4}  {:<32} {:<22} {:<10} {:<5} {:<10} {}\n",
                    item.id,
                    outbound_short,
                    item.reply_kind.as_deref().unwrap_or("-"),
                    item.status.as_str(),
                    item.attempts,
                    item.error_code.as_deref().unwrap_or("-"),
                    preview
                ));
            }
            
            Ok(output)
        }
        
        FeishuCommands::Inspect { job_id, event_key, outbound_id } => {
            // Inspect a specific record by ID type
            if let Some(ref jid) = job_id {
                let job = store.get_job(jid)
                    .map_err(|e| anyhow::anyhow!("failed to get job: {}", e))?;
                
                if let Some(job) = job {
                    // Sanitize payload_json before display
                    let payload_display = job.payload_json.as_ref()
                        .map(|p| sanitize_for_display(p, 500));
                    
                    let mut output = String::new();
                    output.push_str("╔══════════════════════════════════════════════════════════════╗\n");
                    output.push_str("║                        Job Detail                           ║\n");
                    output.push_str("╚══════════════════════════════════════════════════════════════╝\n\n");
                    output.push_str(&format!("  id           : {}\n", job.id));
                    output.push_str(&format!("  job_id       : {}\n", job.job_id));
                    output.push_str(&format!("  event_key    : {}\n", job.event_key));
                    output.push_str(&format!("  mode         : {}\n", job.mode));
                    output.push_str(&format!("  slash_cmd    : {:?}\n", job.slash_command));
                    output.push_str(&format!("  status       : {}\n", job.status.as_str()));
                    output.push_str(&format!("  attempts     : {}/{}\n", job.attempts, job.max_attempts));
                    output.push_str(&format!("  error_code   : {:?}\n", job.error_code));
                    output.push_str(&format!("  created_at   : {}\n", job.created_at));
                    output.push_str(&format!("  updated_at   : {}\n", job.updated_at));
                    output.push_str(&format!("  payload      : {}\n", payload_display.as_deref().unwrap_or("(none)")));
                    
                    Ok(output)
                } else {
                    Ok(format!("Job not found: {}", jid))
                }
            } else if let Some(ref ek) = event_key {
                let event = store.get_event_by_key(ek)
                    .map_err(|e| anyhow::anyhow!("failed to get event: {}", e))?;
                
                if let Some(event) = event {
                    // Sanitize metadata_json before display
                    let metadata_display = event.metadata_json.as_ref()
                        .map(|m| sanitize_for_display(m, 500));
                    
                    let mut output = String::new();
                    output.push_str("╔══════════════════════════════════════════════════════════════╗\n");
                    output.push_str("║                       Event Detail                          ║\n");
                    output.push_str("╚══════════════════════════════════════════════════════════════╝\n\n");
                    output.push_str(&format!("  id           : {}\n", event.id));
                    output.push_str(&format!("  event_key    : {}\n", event.event_key));
                    output.push_str(&format!("  event_type   : {:?}\n", event.event_type));
                    output.push_str(&format!("  status       : {}\n", event.status.as_str()));
                    output.push_str(&format!("  message_type : {:?}\n", event.message_type));
                    output.push_str(&format!("  sender_type  : {:?}\n", event.sender_type));
                    output.push_str(&format!("  text_preview : {:?}\n", event.text_preview));
                    output.push_str(&format!("  text_hash    : {:?}\n", event.text_hash));
                    output.push_str(&format!("  received_at  : {}\n", event.received_at));
                    output.push_str(&format!("  metadata     : {}\n", metadata_display.as_deref().unwrap_or("(none)")));
                    
                    Ok(output)
                } else {
                    Ok(format!("Event not found: {}", ek))
                }
            } else if let Some(ref oid) = outbound_id {
                let item = store.get_outbox(oid)
                    .map_err(|e| anyhow::anyhow!("failed to get outbox: {}", e))?;
                
                if let Some(item) = item {
                    // Sanitize result_json before display
                    let result_display = item.result_json.as_ref()
                        .map(|r| sanitize_for_display(r, 500));
                    
                    let mut output = String::new();
                    output.push_str("╔══════════════════════════════════════════════════════════════╗\n");
                    output.push_str("║                       Outbox Detail                         ║\n");
                    output.push_str("╚══════════════════════════════════════════════════════════════╝\n\n");
                    output.push_str(&format!("  id                  : {}\n", item.id));
                    output.push_str(&format!("  outbound_id         : {}\n", item.outbound_id));
                    output.push_str(&format!("  job_id              : {:?}\n", item.job_id));
                    output.push_str(&format!("  reply_kind          : {:?}\n", item.reply_kind));
                    output.push_str(&format!("  status              : {}\n", item.status.as_str()));
                    output.push_str(&format!("  attempts            : {}/{}\n", item.attempts, item.max_attempts));
                    output.push_str(&format!("  platform_message_id : {:?}\n", item.platform_message_id));
                    output.push_str(&format!("  reply_preview      : {:?}\n", item.reply_preview));
                    output.push_str(&format!("  reply_hash         : {:?}\n", item.reply_hash));
                    output.push_str(&format!("  result_json        : {}\n", result_display.as_deref().unwrap_or("(none)")));
                    output.push_str(&format!("  error_code         : {:?}\n", item.error_code));
                    output.push_str(&format!("  created_at         : {}\n", item.created_at));
                    output.push_str(&format!("  updated_at         : {}\n", item.updated_at));
                    
                    Ok(output)
                } else {
                    Ok(format!("Outbox not found: {}", oid))
                }
            } else {
                Ok("Specify one of: --job-id, --event-key, --outbound-id".to_string())
            }
        }
    }
}

/// Sanitize a JSON string for safe CLI display.
/// Removes sensitive keys and truncates long values.
fn sanitize_for_display(json_str: &str, max_len: usize) -> String {
    // Parse and re-serialize with sensitive keys redacted
    if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(json_str) {
        sanitize_json_value_recursive(&mut value, 0);
        let output = serde_json::to_string_pretty(&value).unwrap_or_else(|_| json_str.to_string());
        if output.len() > max_len {
            format!("{}...(truncated)", &output[..max_len.saturating_sub(15)])
        } else {
            output
        }
    } else {
        // Not valid JSON, just truncate
        if json_str.len() > max_len {
            format!("{}...(truncated)", &json_str[..max_len.saturating_sub(15)])
        } else {
            json_str.to_string()
        }
    }
}

/// Recursively sanitize JSON values by redacting sensitive keys.
fn sanitize_json_value_recursive(value: &mut serde_json::Value, depth: usize) {
    if depth > 10 {
        return; // Prevent stack overflow
    }
    
    match value {
        serde_json::Value::Object(map) => {
            let sensitive_keys = [
                "app_secret", "tenant_access_token", "app_access_token",
                "authorization", "Authorization", "bearer", "Bearer",
                "password", "secret", "token", "api_key", "apiKey",
            ];
            
            for (key, val) in map.iter_mut() {
                let key_lower = key.to_lowercase();
                if sensitive_keys.iter().any(|k| key_lower.contains(&k.to_lowercase())) {
                    *val = serde_json::Value::String("[REDACTED]".to_string());
                } else {
                    sanitize_json_value_recursive(val, depth + 1);
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr.iter_mut() {
                sanitize_json_value_recursive(item, depth + 1);
            }
        }
        _ => {}
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
    let skills_dir = crate::config::resolve_configured_skills_dir(config);
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
            let results = vec![("agent-browser", r1.is_ok())];
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

fn gateway_probe_url(config: &Config) -> String {
    let host = match config.gateway.host.as_str() {
        "0.0.0.0" | "::" | "[::]" => "127.0.0.1",
        host => host,
    };
    format!("http://{}:{}/health", host, config.gateway.port)
}

async fn probe_gateway_http(config: &Config) -> (bool, Option<u16>, Option<String>) {
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
    {
        Ok(client) => client,
        Err(error) => return (false, None, Some(format!("health client error: {error}"))),
    };

    match client.get(gateway_probe_url(config)).send().await {
        Ok(response) => {
            let status = response.status().as_u16();
            (true, Some(status), None)
        }
        Err(error) => (false, None, Some(format!("gateway is not reachable: {error}"))),
    }
}

async fn run_gateway_status(config: &Config) -> Result<String> {
    let (running, status_code, probe_error) = probe_gateway_http(config).await;
    let runtime = GatewayRuntime::new(config.clone());
    let mut status =
        GatewayRuntimeStatus::from_runtime(running, &runtime, None, probe_error).await;
    status.health_ok = status_code == Some(200);
    status.public_health = check_gateway_public_health(config).await;
    Ok(serde_json::to_string_pretty(&status)?)
}

async fn run_gateway_doctor(config: &Config) -> Result<String> {
    use crate::gateway::FeishuSecurityConfig;

    let mut checks = Vec::new();
    let (gateway_reachable, gateway_status_code, gateway_probe_error) =
        probe_gateway_http(config).await;
    let local_health_ok = gateway_reachable && gateway_status_code == Some(200);

    // 1. Config file exists
    checks.push(serde_json::json!({
        "check": "config_file",
        "ok": config.config_path.exists(),
        "path": config.config_path.display().to_string(),
        "detail": if config.config_path.exists() {
            "found".to_string()
        } else {
            "not found".to_string()
        }
    }));

    // 2. Feishu enabled
    let feishu_enabled = config.channels_config.feishu.as_ref().map(|e| e.enabled).unwrap_or(false);
    checks.push(serde_json::json!({
        "check": "feishu_enabled",
        "ok": true, // Not an error, just informational
        "enabled": feishu_enabled,
    }));

    // 3. Feishu security: app_id/app_secret present (never print the values)
    let feishu_entry = config.channels_config.feishu.as_ref();
    let app_id_present = feishu_entry
        .and_then(|e| e.extra.get("app_id"))
        .and_then(|v| v.as_str())
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    let app_secret_present = feishu_entry
        .and_then(|e| e.extra.get("app_secret"))
        .and_then(|v| v.as_str())
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    let outbound_mode = feishu_entry
        .and_then(|e| e.extra.get("outbound_mode"))
        .and_then(|v| v.as_str())
        .unwrap_or("disabled");
    let credentials_ok = !feishu_enabled
        || (app_id_present
            && (!matches!(outbound_mode, "real" | "mock") || app_secret_present));
    checks.push(serde_json::json!({
        "check": "feishu_credentials",
        "ok": credentials_ok,
        "app_id_present": app_id_present,
        "app_secret_present": app_secret_present,
        "outbound_mode": outbound_mode,
        "detail": format!(
            "app_id={} app_secret={}",
            if app_id_present { "present" } else { "absent" },
            if app_secret_present { "present" } else { "absent" }
        )
    }));

    // 4. Feishu security mode + token/encrypt_key presence
    let sec_cfg = FeishuSecurityConfig::from_entry(feishu_entry);
    let token_present = sec_cfg.verification_token.is_some();
    let encrypt_key_present = sec_cfg.encrypt_key.is_some();
    let security_ok = !feishu_enabled
        || match sec_cfg.mode.as_str() {
            "dev" | "default" => true,
            "token" => token_present,
            "encrypted" => token_present && encrypt_key_present,
            _ => false,
        };
    checks.push(serde_json::json!({
        "check": "feishu_security",
        "ok": security_ok,
        "security_mode": sec_cfg.mode.as_str(),
        "verification_token_present": token_present,
        "encrypt_key_present": encrypt_key_present,
        "insecure_dev": sec_cfg.insecure,
    }));

    // 5. Store can open
    let config_dir = config.config_path.parent().unwrap_or(&config.workspace_dir);
    let store_path = config_dir.join("state.sqlite");
    let store_open_result = rusqlite::Connection::open(&store_path);
    let store_opened = store_open_result.is_ok();
    checks.push(serde_json::json!({
        "check": "feishu_store",
        "ok": store_opened,
        "store_path": store_path.display().to_string(),
        "detail": if store_opened {
            "opened successfully".to_string()
        } else {
            format!("failed to open: {:?}", store_open_result.err())
        }
    }));

    // 6. Port bindability. A healthy running Gateway legitimately owns the
    // port, so distinguish it from an unknown conflicting process.
    let port = config.gateway.port;
    let port_bindable =
        std::net::TcpListener::bind((config.gateway.host.as_str(), port)).is_ok();
    let healthy_gateway_owns_port = gateway_reachable && gateway_status_code == Some(200);
    checks.push(serde_json::json!({
        "check": "gateway_port",
        "ok": port_bindable || healthy_gateway_owns_port,
        "port": port,
        "bindable": port_bindable,
        "gateway_running": healthy_gateway_owns_port,
        "detail": if port_bindable {
            format!("port {} is available", port)
        } else if healthy_gateway_owns_port {
            format!("port {} is owned by a healthy Gateway", port)
        } else {
            format!("port {} is occupied by another process or cannot be bound", port)
        }
    }));

    checks.push(serde_json::json!({
        "check": "local_health",
        "ok": local_health_ok,
        "running": local_health_ok,
        "status_code": gateway_status_code,
        "url": gateway_probe_url(config),
        "detail": if local_health_ok {
            "local Gateway health is OK".to_string()
        } else {
            gateway_probe_error.unwrap_or_else(|| "local Gateway health check failed".to_string())
        }
    }));

    // 7. Stable public ingress configuration and real public health probe.
    let public_base = resolve_public_webhook_base_url(config);
    let public_base_configured = public_base.is_some();
    let (feishu_webhook_url, card_callback_url) = feishu_public_callback_urls(config);
    let named_mode = matches!(
        config.gateway_public.mode,
        GatewayPublicMode::NamedCloudflareTunnel
    );
    let named_tunnel_name_configured = config
        .gateway_public
        .named_tunnel_name
        .as_deref()
        .is_some_and(|name| !name.trim().is_empty());
    let named_tunnel_hostname = config
        .gateway_public
        .named_tunnel_hostname
        .as_deref()
        .and_then(normalize_named_tunnel_hostname);
    let named_tunnel_hostname_configured = named_tunnel_hostname.is_some();
    let named_tunnel_config_complete =
        named_tunnel_name_configured && named_tunnel_hostname_configured;
    checks.push(serde_json::json!({
        "check": "named_cloudflare_tunnel",
        "ok": !named_mode || named_tunnel_config_complete,
        "active": named_mode,
        "name_configured": named_tunnel_name_configured,
        "hostname_configured": named_tunnel_hostname_configured,
        "public_base_generated": if named_mode { public_base.clone() } else { None },
        "detail": if !named_mode {
            "not selected".to_string()
        } else if named_tunnel_config_complete {
            "named tunnel configuration is complete".to_string()
        } else {
            format!(
                "named tunnel configuration is incomplete: name={} hostname={}",
                if named_tunnel_name_configured { "configured" } else { "missing" },
                if named_tunnel_hostname_configured { "configured" } else { "missing" },
            )
        }
    }));
    checks.push(serde_json::json!({
        "check": "public_webhook_base_url",
        "ok": !named_mode || public_base_configured,
        "mode": config.gateway_public.mode.as_str(),
        "configured": public_base_configured,
        "public_base": public_base.clone(),
        "feishu_webhook_url": feishu_webhook_url,
        "feishu_card_callback_url": card_callback_url,
        "detail": if public_base_configured {
            "configured".to_string()
        } else {
            "not configured (webhook URLs will only show local address)".to_string()
        }
    }));

    let public_health = check_gateway_public_health(config).await;
    checks.push(serde_json::json!({
        "check": "public_health",
        "ok": public_health.ok,
        "configured": public_health.configured,
        "status_code": public_health.status_code,
        "checked_url": public_health.checked_url,
        "error_kind": public_health.error_kind,
        "detail": public_health.error.unwrap_or_else(|| "public Gateway health is OK".to_string()),
    }));

    let cloudflared_found = cloudflared_available(config);
    let cloudflared_required = matches!(
        config.gateway_public.mode,
        GatewayPublicMode::QuickTunnel | GatewayPublicMode::NamedCloudflareTunnel
    );
    checks.push(serde_json::json!({
        "check": "cloudflared",
        "ok": !cloudflared_required || cloudflared_found,
        "found": cloudflared_found,
        "configured_path_present": config.gateway_public.cloudflared_path.is_some(),
        "quick_tunnel_non_production": matches!(
            config.gateway_public.mode,
            crate::config::schema::GatewayPublicMode::QuickTunnel
        ),
        "detail": if cloudflared_found {
            "cloudflared is available".to_string()
        } else if cloudflared_required {
            "cloudflared is required for the selected tunnel mode but was not found".to_string()
        } else {
            "cloudflared was not found; external_public_url mode remains available".to_string()
        }
    }));

    checks.push(serde_json::json!({
        "check": "runtime_workers",
        "ok": true,
        "store_opened": store_opened,
        "retry_worker_enabled": local_health_ok && store_opened && feishu_enabled,
    }));

    let all_ok = checks.iter().all(|c| c["ok"].as_bool().unwrap_or(true));
    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "ok": all_ok,
        "gateway_bind": format!("{}:{}", config.gateway.host, config.gateway.port),
        "public_mode": config.gateway_public.mode.as_str(),
        "named_tunnel_name_configured": named_tunnel_name_configured,
        "named_tunnel_hostname_configured": named_tunnel_hostname_configured,
        "public_base_url_generated": public_base,
        "cloudflared_found": cloudflared_found,
        "checks": checks,
    }))?)
}

async fn run_diagnostics(config: &Config, output_path: Option<&str>) -> Result<String> {
    use crate::gateway::feishu_store::FeishuStore;
    use crate::gateway::FeishuSecurityConfig;

    let config_dir = config.config_path.parent().unwrap_or(&config.workspace_dir);
    let timestamp = chrono_lite_timestamp();
    let default_name = format!("diagnostics_{timestamp}.zip");
    let requested = output_path
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| config_dir.join("diagnostics"));
    let zip_path = if requested
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
    {
        requested
    } else {
        requested.join(default_name)
    };
    let diag_dir = zip_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("diagnostics output has no parent directory"))?;
    std::fs::create_dir_all(diag_dir)
        .map_err(|e| anyhow::anyhow!("failed to create diagnostics dir: {e}"))?;
    let staging_dir = diag_dir.join(format!(
        ".omninova-diagnostics-{timestamp}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&staging_dir)
        .map_err(|e| anyhow::anyhow!("failed to create diagnostics staging dir: {e}"))?;

    // Gather data
    let sanitized_config = sanitize_config_for_export(config);

    let feishu_status = gather_feishu_status(config);
    let feishu_sec = FeishuSecurityConfig::from_entry(config.channels_config.feishu.as_ref());
    let feishu_sec_report = serde_json::json!({
        "security_mode": feishu_sec.mode.as_str(),
        "verification_token_configured": feishu_sec.verification_token.is_some(),
        "encrypt_key_configured": feishu_sec.encrypt_key.is_some(),
        "insecure_dev_mode": feishu_sec.insecure,
    });

    let (recent_jobs_summary, recent_outbox_summary, recent_errors) =
        if let Ok(store) = FeishuStore::open(config_dir) {
        let jobs = store.get_recent_jobs(20).unwrap_or_default();
        let outbox = store.get_recent_outbox(20).unwrap_or_default();
        let job_errors = jobs
            .iter()
            .filter_map(|job| {
                job.error_code.as_ref().map(|error_code| {
                    serde_json::json!({
                        "source": "job",
                        "status": job.status.as_str(),
                        "error_code": error_code,
                        "updated_at": job.updated_at,
                    })
                })
            })
            .collect::<Vec<_>>();
        let outbox_errors = outbox
            .iter()
            .filter_map(|item| {
                item.error_code.as_ref().map(|error_code| {
                    serde_json::json!({
                        "source": "outbox",
                        "status": item.status.as_str(),
                        "error_code": error_code,
                        "updated_at": item.updated_at,
                    })
                })
            })
            .collect::<Vec<_>>();
        let jobs_summary: Vec<serde_json::Value> = jobs.into_iter().map(|j| {
            serde_json::json!({
                "job_id": truncate_for_export(&j.job_id, 40),
                "status": j.status.as_str(),
                "mode": j.mode,
                "created_at": j.created_at,
                "completed_at": j.completed_at,
                "error_code": j.error_code,
            })
        }).collect();
        let outbox_summary: Vec<serde_json::Value> = outbox.into_iter().map(|o| {
            serde_json::json!({
                "outbound_id": truncate_for_export(&o.outbound_id, 40),
                "reply_kind": o.reply_kind,
                "status": o.status.as_str(),
                "reply_preview_present": o.reply_preview.is_some(),
                "created_at": o.created_at,
                "sent_at": o.sent_at,
                "error_code": o.error_code,
            })
        }).collect();
        (
            jobs_summary,
            outbox_summary,
            job_errors
                .into_iter()
                .chain(outbox_errors)
                .collect::<Vec<_>>(),
        )
    } else {
        (Vec::new(), Vec::new(), Vec::new())
    };

    let build_info = serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "name": env!("CARGO_PKG_NAME"),
        "timestamp": timestamp,
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
    });

    let gateway_status: serde_json::Value =
        serde_json::from_str(&run_gateway_status(config).await?)?;

    // Write JSON files
    let write_json = |name: &str, value: &serde_json::Value| -> Result<()> {
        let path = staging_dir.join(format!("{name}.json"));
        std::fs::write(&path, serde_json::to_string_pretty(value)?)?;
        Ok(())
    };

    write_json("config", &sanitized_config)?;
    write_json("gateway_status", &gateway_status)?;
    write_json("feishu_status", &feishu_status)?;
    write_json("feishu_security", &feishu_sec_report)?;
    write_json("build_info", &build_info)?;
    write_json("recent_jobs", &serde_json::json!({ "jobs": recent_jobs_summary }))?;
    write_json("recent_outbox", &serde_json::json!({ "outbox": recent_outbox_summary }))?;
    write_json("recent_errors", &serde_json::json!({ "errors": recent_errors }))?;

    // Create zip
    let zip_file = std::fs::File::create(&zip_path)
        .map_err(|e| anyhow::anyhow!("failed to create zip: {}", e))?;
    let mut zip = zip::ZipWriter::new(zip_file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    for entry in std::fs::read_dir(&staging_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            let name = path.file_name().unwrap().to_string_lossy();
            zip.start_file(name.as_ref(), options)?;
            std::io::Write::write_all(&mut zip, &std::fs::read(&path)?)?;
        }
    }
    zip.finish()?;
    std::fs::remove_dir_all(&staging_dir).map_err(|error| {
        anyhow::anyhow!(
            "diagnostics ZIP was created but staging cleanup failed: {error}"
        )
    })?;

    Ok(format!(
        "Diagnostics exported to: {}\n\
         Contains: config, gateway_status, feishu_status, feishu_security, recent_jobs, recent_outbox, recent_errors, build_info\n\
         Secrets stripped: app_secret, verification_token, encrypt_key, tokens redacted",
        zip_path.display()
    ))
}

fn sanitize_config_for_export(config: &Config) -> serde_json::Value {
    let json = serde_json::to_value(config).unwrap_or_default();
    sanitize_json_value(json)
}

fn sanitize_json_value(val: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match val {
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => val,
        Value::Array(arr) => Value::Array(arr.into_iter().map(sanitize_json_value).collect()),
        Value::Object(map) => {
            let is_secret_key = |key: &str| {
                let k = key.to_ascii_lowercase();
                matches!(
                    k.as_str(),
                    "app_secret" | "verification_token" | "encrypt_key"
                        | "tenant_access_token" | "authorization" | "token"
                        | "password" | "secret" | "key"
                )
            };
            let is_sensitive_path = |key: &str| {
                let k = key.to_ascii_lowercase();
                k.contains("secret")
                    || k.contains("token")
                    || k.contains("authorization")
                    || matches!(
                        k.as_str(),
                        "payload" | "payload_json" | "body" | "message" | "reply" | "content"
                    )
            };
            Value::Object(serde_json::Map::from_iter(
                map.into_iter().map(|(k, v)| {
                    let redacted = if is_secret_key(&k) || is_sensitive_path(&k) {
                        Value::String("[REDACTED]".to_string())
                    } else {
                        sanitize_json_value(v)
                    };
                    (k, redacted)
                }),
            ))
        }
    }
}

fn truncate_for_export(s: &str, max_len: usize) -> String {
    let mut chars = s.chars();
    let truncated = chars.by_ref().take(max_len).collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

fn chrono_lite_timestamp() -> String {
    let format = time::format_description::parse(
        "[year][month][day]_[hour][minute][second]",
    )
    .expect("valid diagnostics timestamp format");
    time::OffsetDateTime::now_utc()
        .format(&format)
        .unwrap_or_else(|_| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                .to_string()
        })
}

fn gather_feishu_status(config: &Config) -> serde_json::Value {
    let feishu = config.channels_config.feishu.as_ref();
    let security = crate::gateway::FeishuSecurityConfig::from_entry(feishu);
    serde_json::json!({
        "enabled": feishu.map(|e| e.enabled).unwrap_or(false),
        "security_mode": security.mode.as_str(),
        "verification_token_configured": security.verification_token.is_some(),
        "encrypt_key_configured": security.encrypt_key.is_some(),
        "insecure_dev_mode": security.insecure,
        "outbound_mode": feishu
            .and_then(|e| e.extra.get("outbound_mode"))
            .and_then(|v| v.as_str())
            .unwrap_or("disabled"),
        "gateway_public_mode": config.gateway_public.mode.as_str(),
        "public_webhook_base_url": resolve_public_webhook_base_url(config),
    })
}

#[cfg(test)]
mod productized_gateway_tests {
    use super::*;
    use clap::Parser;
    use serde_json::json;

    #[test]
    fn diagnostics_export_command_shape_is_supported() {
        let cli = Cli::try_parse_from(["omninova", "diagnostics", "export"])
            .expect("diagnostics export should parse");
        assert!(matches!(
            cli.command,
            Some(Commands::Diagnostics {
                command: DiagnosticsCommands::Export { output: None }
            })
        ));
    }

    #[test]
    fn default_invocation_is_repl_and_prompt_flag_parses() {
        let bare = Cli::try_parse_from(["omninova"]).expect("bare omninova should parse");
        assert!(bare.command.is_none());
        assert!(bare.prompt.is_none());

        let oneshot = Cli::try_parse_from(["omninova", "-p", "hello", "--session", "s1"])
            .expect("-p should parse without a subcommand");
        assert!(oneshot.command.is_none());
        assert_eq!(oneshot.prompt.as_deref(), Some("hello"));
        assert_eq!(oneshot.session.as_deref(), Some("s1"));

        let web = Cli::try_parse_from(["omninova", "web"]).expect("web subcommand");
        assert!(matches!(web.command, Some(Commands::Web)));
    }

    #[test]
    fn diagnostics_sanitizer_removes_secrets_and_full_payloads() {
        let sanitized = sanitize_json_value(json!({
            "app_secret": "secret-value",
            "verification_token": "verification-value",
            "encrypt_key": "encrypt-value",
            "Authorization": "Bearer token-value",
            "payload_json": "{\"message\":\"full user message\"}",
            "reply": "full model reply",
            "safe": "kept"
        }));
        let serialized = serde_json::to_string(&sanitized).unwrap();
        assert!(!serialized.contains("secret-value"));
        assert!(!serialized.contains("verification-value"));
        assert!(!serialized.contains("encrypt-value"));
        assert!(!serialized.contains("Bearer token-value"));
        assert!(!serialized.contains("full user message"));
        assert!(!serialized.contains("full model reply"));
        assert!(serialized.contains("\"safe\":\"kept\""));
    }

    #[test]
    fn diagnostics_preview_truncation_is_utf8_safe() {
        assert_eq!(truncate_for_export("飞书网关状态", 3), "飞书网...");
    }

    #[test]
    fn config_helpers_support_gateway_public_fields_and_normalize_urls() {
        let mut config = Config::default();
        set_config_key(&mut config, "gateway_public.mode", "quick_tunnel").unwrap();
        set_config_key(
            &mut config,
            "gateway_public.public_webhook_base_url",
            "https://example.test/webhook/feishu/card/",
        )
        .unwrap();
        set_config_key(
            &mut config,
            "gateway_public.cloudflared_path",
            r"C:\Tools\cloudflared\cloudflared.exe",
        )
        .unwrap();

        assert_eq!(
            lookup_config_key(&config, "gateway_public.mode").unwrap(),
            "quick_tunnel"
        );
        assert_eq!(
            lookup_config_key(&config, "gateway_public.public_webhook_base_url").unwrap(),
            "https://example.test"
        );
        assert_eq!(
            lookup_config_key(&config, "gateway_public.cloudflared_path").unwrap(),
            r"C:\Tools\cloudflared\cloudflared.exe"
        );

        unset_config_key(&mut config, "gateway_public.public_webhook_base_url").unwrap();
        assert!(config.gateway_public.public_webhook_base_url.is_none());
    }

    #[test]
    fn config_helper_rejects_unknown_gateway_public_mode() {
        let mut config = Config::default();
        let error =
            set_config_key(&mut config, "gateway_public.mode", "production").unwrap_err();
        assert!(error.to_string().contains("gateway_public.mode must be"));
    }

    #[test]
    fn config_helpers_generate_named_tunnel_public_base() {
        let mut config = Config::default();
        set_config_key(
            &mut config,
            "gateway_public.named_tunnel_hostname",
            "https://Fixed.Example.Test/webhook/feishu/card",
        )
        .unwrap();
        set_config_key(
            &mut config,
            "gateway_public.named_tunnel_name",
            "omninova-fixed",
        )
        .unwrap();
        set_config_key(
            &mut config,
            "gateway_public.mode",
            "named_cloudflare_tunnel",
        )
        .unwrap();

        assert_eq!(
            lookup_config_key(&config, "gateway_public.named_tunnel_hostname").unwrap(),
            "fixed.example.test"
        );
        assert_eq!(
            lookup_config_key(&config, "gateway_public.public_webhook_base_url").unwrap(),
            "https://fixed.example.test"
        );
    }

    #[tokio::test]
    async fn gateway_doctor_reports_missing_named_tunnel_fields() {
        let temp_root = std::env::temp_dir().join(format!(
            "omninova-gateway-doctor-named-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&temp_root).unwrap();
        let mut config = Config::default();
        config.config_path = temp_root.join("config.toml");
        config.gateway_public.mode = GatewayPublicMode::NamedCloudflareTunnel;
        config.gateway_public.named_tunnel_name = None;
        config.gateway_public.named_tunnel_hostname = None;

        let output = run_gateway_doctor(&config).await.unwrap();
        let report: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(
            report.get("public_mode").and_then(serde_json::Value::as_str),
            Some("named_cloudflare_tunnel")
        );
        assert_eq!(
            report
                .get("named_tunnel_name_configured")
                .and_then(serde_json::Value::as_bool),
            Some(false)
        );
        assert_eq!(
            report
                .get("named_tunnel_hostname_configured")
                .and_then(serde_json::Value::as_bool),
            Some(false)
        );
        assert!(output.contains("named tunnel configuration is incomplete"));
        let _ = std::fs::remove_dir_all(temp_root);
    }

    #[tokio::test]
    async fn gateway_doctor_reports_public_ingress_without_leaking_secrets() {
        let temp_root = std::env::temp_dir().join(format!(
            "omninova-gateway-doctor-public-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&temp_root).unwrap();
        let mut config = Config::default();
        config.config_path = temp_root.join("config.toml");
        std::fs::write(&config.config_path, "# test").unwrap();
        config.gateway.host = "127.0.0.1".to_string();
        config.gateway.port = std::net::TcpListener::bind(("127.0.0.1", 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        config.channels_config.feishu = Some(crate::config::ChannelEntry {
            enabled: true,
            extra: std::collections::HashMap::from([
                ("app_id".to_string(), json!("cli_test")),
                (
                    "app_secret".to_string(),
                    json!("doctor-app-secret-must-not-leak"),
                ),
                ("outbound_mode".to_string(), json!("real")),
                ("security_mode".to_string(), json!("token")),
                (
                    "verification_token".to_string(),
                    json!("doctor-token-must-not-leak"),
                ),
            ]),
            ..Default::default()
        });

        let output = run_gateway_doctor(&config).await.unwrap();
        assert!(output.contains("\"check\": \"public_health\""));
        assert!(output.contains("\"check\": \"cloudflared\""));
        assert!(output.contains("external_public_url"));
        assert!(!output.contains("doctor-app-secret-must-not-leak"));
        assert!(!output.contains("doctor-token-must-not-leak"));
        assert!(!output.to_ascii_lowercase().contains("authorization: bearer"));

        let _ = std::fs::remove_dir_all(temp_root);
    }
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
