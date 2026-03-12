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
use std::fs;
use crate::skills::load_skills_from_dir;

#[derive(Debug, Parser)]
#[command(name = "omninova", version, about = "OmniNova CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Send a single message to the agent.
    Agent {
        #[arg(short, long)]
        message: String,
        #[arg(long)]
        session_id: Option<String>,
    },
    /// Start HTTP gateway service.
    Gateway {
        #[arg(long)]
        host: Option<String>,
        #[arg(long)]
        port: Option<u16>,
    },
    /// Print current config as pretty JSON.
    ConfigPrint,
    /// Check runtime health.
    Health,
    /// Resolve routing decision for an inbound message.
    Route {
        #[arg(long, default_value = "cli")]
        channel: String,
        #[arg(short, long)]
        text: String,
        #[arg(long)]
        agent: Option<String>,
    },
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
    /// Install optional dependencies (agent-browser, etc.).
    Setup {
        #[command(subcommand)]
        command: SetupCommands,
    },
    /// Run diagnostics on environment and dependencies.
    Doctor,
    /// Manage skills.
    Skills {
        #[command(subcommand)]
        command: SkillsCommands,
    },
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
    Install,
    Uninstall,
    Start,
    Stop,
    Status,
    Check {
        #[arg(long)]
        strict: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum SetupCommands {
    /// Install agent-browser (headless browser automation for AI agents).
    Browser,
    /// Install all optional dependencies.
    All,
}

pub async fn run_cli(cli: Cli) -> Result<String> {
    let mut config = Config::load_or_init()?;
    match cli.command {
        Commands::Agent {
            message,
            session_id,
        } => {
            let runtime = GatewayRuntime::new(config);
            let inbound = crate::channels::adapters::cli::inbound_from_cli(
                message,
                session_id,
                None,
            );
            let resp = runtime.process_inbound(&inbound).await?;
            Ok(resp.reply)
        }
        Commands::Gateway { host, port } => {
            if let Some(host) = host {
                config.gateway.host = host;
            }
            if let Some(port) = port {
                config.gateway.port = port;
            }
            let runtime = GatewayRuntime::new(config.clone());
            runtime.serve_http().await?;
            Ok("gateway stopped".to_string())
        }
        Commands::ConfigPrint => {
            let runtime = GatewayRuntime::new(config);
            let cfg = runtime.get_config().await;
            Ok(serde_json::to_string_pretty(&cfg)?)
        }
        Commands::Health => {
            let runtime = GatewayRuntime::new(config);
            let health = runtime.health().await;
            Ok(serde_json::to_string_pretty(&health)?)
        }
        Commands::Route {
            channel,
            text,
            agent,
        } => {
            let runtime = GatewayRuntime::new(config);
            let mut metadata = std::collections::HashMap::new();
            if let Some(agent) = agent {
                metadata.insert("agent".to_string(), serde_json::Value::String(agent));
            }
            let inbound = crate::channels::InboundMessage {
                channel: parse_channel_kind(&channel),
                user_id: None,
                session_id: None,
                text,
                metadata,
            };
            let route = runtime.route(&inbound).await;
            Ok(serde_json::to_string_pretty(&route)?)
        }
        Commands::Estop { command } => {
            let runtime = GatewayRuntime::new(config);
            match command {
                EstopCommands::Status => Ok(serde_json::to_string_pretty(
                    &runtime.estop_status().await?,
                )?),
                EstopCommands::Pause {
                    level,
                    domain,
                    tool,
                    reason,
                } => Ok(serde_json::to_string_pretty(
                    &runtime.estop_pause(level, domain, tool, reason).await?,
                )?),
                EstopCommands::Resume => Ok(serde_json::to_string_pretty(
                    &runtime.estop_resume().await?,
                )?),
            }
        }
        Commands::Setup { command } => run_setup(command).await,
        Commands::Doctor => run_doctor(&config).await,
        Commands::Daemon { command } => {
            let svc = resolve_gateway_service();
            match command {
                DaemonCommands::Install => Ok(serde_json::to_string_pretty(
                    &svc.operate_report(GatewayServiceOperation::Install),
                )?),
                DaemonCommands::Uninstall => Ok(serde_json::to_string_pretty(
                    &svc.operate_report(GatewayServiceOperation::Uninstall),
                )?),
                DaemonCommands::Start => Ok(serde_json::to_string_pretty(
                    &svc.operate_report(GatewayServiceOperation::Start),
                )?),
                DaemonCommands::Stop => Ok(serde_json::to_string_pretty(
                    &svc.operate_report(GatewayServiceOperation::Stop),
                )?),
                DaemonCommands::Status => Ok(serde_json::to_string_pretty(&svc.status_report()?)?),
                DaemonCommands::Check { strict } => {
                    let mut report = svc.preflight_report();
                    let extra_checks = build_generic_daemon_checks(&config);
                    report.checks.extend(extra_checks);
                    let hard_failed = report.checks.iter().any(|c| !c.ok);
                    let warn_exists = report
                        .checks
                        .iter()
                        .any(|c| matches!(c.level, GatewayServiceCheckLevel::Warn));
                    report.ok = !hard_failed && !(strict && warn_exists);
                    report.detail = if report.ok {
                        if strict {
                            "daemon preflight passed (strict mode)".to_string()
                        } else {
                            "daemon preflight passed".to_string()
                        }
                    } else {
                        if strict && !hard_failed && warn_exists {
                            "daemon preflight failed in strict mode (warnings present)".to_string()
                        } else {
                            "daemon preflight failed".to_string()
                        }
                    };
                    if !report.ok {
                        report.hints.push(
                            "fix failed checks and rerun: omninova daemon check".to_string(),
                        );
                        if strict && warn_exists {
                            report.hints.push(
                                "strict mode treats warnings as failures; rerun without --strict if needed"
                                    .to_string(),
                            );
                        }
                    }
                    Ok(serde_json::to_string_pretty(&report)?)
                }
            }
        }
        Commands::Skills { command } => run_skills(command, &config).await,
    }
}

async fn run_skills(command: SkillsCommands, config: &Config) -> Result<String> {
    let skills_dir = config.skills.open_skills_dir.as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| config.workspace_dir.join("skills"));

    match command {
        SkillsCommands::List => {
            let skills = load_skills_from_dir(&skills_dir)?;
            if skills.is_empty() {
                return Ok(format!("No skills found in {:?}.", skills_dir));
            }
            let mut output = String::new();
            output.push_str(&format!("Found {} skills in {:?}:\n\n", skills.len(), skills_dir));
            for skill in skills {
                output.push_str(&format!("- {} ({})\n", skill.metadata.name, skill.metadata.description));
            }
            Ok(output)
        }
        SkillsCommands::Import { from, to, overwrite } => {
            let target_dir = to.map(PathBuf::from).unwrap_or(skills_dir);
            if !target_dir.exists() {
                fs::create_dir_all(&target_dir)?;
            }

            let source_dir = PathBuf::from(from);
            if !source_dir.exists() {
                anyhow::bail!("Source directory does not exist: {:?}", source_dir);
            }

            let mut count = 0;
            // Iterate over subdirectories in source_dir
            for entry in fs::read_dir(&source_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    let skill_file = path.join("SKILL.md");
                    if skill_file.exists() {
                        let skill_name = path.file_name().unwrap();
                        let target_skill_dir = target_dir.join(skill_name);

                        if target_skill_dir.exists() {
                            if !overwrite {
                                println!("Skipping existing skill: {:?}", skill_name);
                                continue;
                            }
                        } else {
                            fs::create_dir_all(&target_skill_dir)?;
                        }

                        // Copy all files
                        for sub_entry in fs::read_dir(&path)? {
                            let sub_entry = sub_entry?;
                            let sub_path = sub_entry.path();
                            if sub_path.is_file() {
                                fs::copy(&sub_path, target_skill_dir.join(sub_entry.file_name()))?;
                            } else if sub_path.is_dir() {
                                // Simple recursive copy for 1 level deep (e.g. scripts/)
                                let sub_dir_name = sub_entry.file_name();
                                let target_sub_dir = target_skill_dir.join(sub_dir_name);
                                fs::create_dir_all(&target_sub_dir)?;
                                for deep_entry in fs::read_dir(&sub_path)? {
                                    let deep_entry = deep_entry?;
                                    if deep_entry.path().is_file() {
                                        fs::copy(deep_entry.path(), target_sub_dir.join(deep_entry.file_name()))?;
                                    }
                                }
                            }
                        }
                        count += 1;
                    }
                }
            }
            Ok(format!("Imported {} skills to {:?}", count, target_dir))
        }
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

fn build_config_validation_checks(config: &Config) -> Vec<GatewayServiceCheckReport> {
    let report = config.validate();
    let mut checks = Vec::new();
    if report.errors.is_empty() && report.warnings.is_empty() {
        checks.push(GatewayServiceCheckReport {
            name: "config-validation".to_string(),
            ok: true,
            level: GatewayServiceCheckLevel::Info,
            detail: "config.validate reported no warnings/errors".to_string(),
        });
        return checks;
    }
    for (idx, warning) in report.warnings.iter().enumerate() {
        checks.push(GatewayServiceCheckReport {
            name: format!("config-warning-{}", idx + 1),
            ok: true,
            level: GatewayServiceCheckLevel::Warn,
            detail: warning.clone(),
        });
    }
    for (idx, error) in report.errors.iter().enumerate() {
        checks.push(GatewayServiceCheckReport {
            name: format!("config-error-{}", idx + 1),
            ok: false,
            level: GatewayServiceCheckLevel::Error,
            detail: error.clone(),
        });
    }
    checks
}

fn check_file_readable(path: &Path, name: &str) -> GatewayServiceCheckReport {
    match std::fs::File::open(path) {
        Ok(_) => GatewayServiceCheckReport {
            name: name.to_string(),
            ok: true,
            level: GatewayServiceCheckLevel::Error,
            detail: format!("readable file: {}", path.display()),
        },
        Err(e) => GatewayServiceCheckReport {
            name: name.to_string(),
            ok: false,
            level: GatewayServiceCheckLevel::Error,
            detail: format!("cannot read {}: {e}", path.display()),
        },
    }
}

fn check_dir_writable(path: &Path, name: &str) -> GatewayServiceCheckReport {
    if !path.exists() {
        return match std::fs::create_dir_all(path) {
            Ok(()) => GatewayServiceCheckReport {
                name: name.to_string(),
                ok: true,
                level: GatewayServiceCheckLevel::Error,
                detail: format!("created directory for write checks: {}", path.display()),
            },
            Err(e) => GatewayServiceCheckReport {
                name: name.to_string(),
                ok: false,
                level: GatewayServiceCheckLevel::Error,
                detail: format!("cannot create directory {}: {e}", path.display()),
            },
        };
    }
    if !path.is_dir() {
        return GatewayServiceCheckReport {
            name: name.to_string(),
            ok: false,
            level: GatewayServiceCheckLevel::Error,
            detail: format!("not a directory: {}", path.display()),
        };
    }
    let probe = path.join(format!(".omninova-preflight-{}.tmp", std::process::id()));
    match std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            GatewayServiceCheckReport {
                name: name.to_string(),
                ok: true,
                level: GatewayServiceCheckLevel::Error,
                detail: format!("directory writable: {}", path.display()),
            }
        }
        Err(e) => GatewayServiceCheckReport {
            name: name.to_string(),
            ok: false,
            level: GatewayServiceCheckLevel::Error,
            detail: format!("directory not writable {}: {e}", path.display()),
        },
    }
}

fn parse_channel_kind(raw: &str) -> crate::channels::ChannelKind {
    match raw.to_lowercase().as_str() {
        "cli" => crate::channels::ChannelKind::Cli,
        "web" => crate::channels::ChannelKind::Web,
        "webchat" => crate::channels::ChannelKind::WebChat,
        "telegram" => crate::channels::ChannelKind::Telegram,
        "discord" => crate::channels::ChannelKind::Discord,
        "slack" => crate::channels::ChannelKind::Slack,
        "whatsapp" => crate::channels::ChannelKind::Whatsapp,
        "google_chat" | "googlechat" => crate::channels::ChannelKind::GoogleChat,
        "signal" => crate::channels::ChannelKind::Signal,
        "bluebubbles" => crate::channels::ChannelKind::BlueBubbles,
        "imessage" => crate::channels::ChannelKind::Imessage,
        "irc" => crate::channels::ChannelKind::Irc,
        "msteams" | "teams" => crate::channels::ChannelKind::Msteams,
        "matrix" => crate::channels::ChannelKind::Matrix,
        "feishu" => crate::channels::ChannelKind::Feishu,
        "line" => crate::channels::ChannelKind::Line,
        "mattermost" => crate::channels::ChannelKind::Mattermost,
        "nextcloud_talk" | "nextcloudtalk" => crate::channels::ChannelKind::NextcloudTalk,
        "nostr" => crate::channels::ChannelKind::Nostr,
        "synology_chat" | "synologychat" => crate::channels::ChannelKind::SynologyChat,
        "tlon" => crate::channels::ChannelKind::Tlon,
        "twitch" => crate::channels::ChannelKind::Twitch,
        "wechat" | "wecom" => crate::channels::ChannelKind::Wechat,
        "zalo" => crate::channels::ChannelKind::Zalo,
        "zalo_personal" | "zalopersonal" => crate::channels::ChannelKind::ZaloPersonal,
        "lark" => crate::channels::ChannelKind::Lark,
        "dingtalk" | "ding_talk" | "dingding" => crate::channels::ChannelKind::Dingtalk,
        "email" => crate::channels::ChannelKind::Email,
        "webhook" => crate::channels::ChannelKind::Webhook,
        other => crate::channels::ChannelKind::Other(other.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Setup & Doctor
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
struct DepStatus {
    name: String,
    installed: bool,
    version: Option<String>,
    detail: String,
}

async fn check_dep_installed(bin: &str, version_flag: &str) -> DepStatus {
    use std::process::Stdio;
    use tokio::process::Command;
    match Command::new(bin)
        .arg(version_flag)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
    {
        Ok(output) if output.status.success() => {
            let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let version = raw
                .split_whitespace()
                .find(|s| s.chars().next().map_or(false, |c| c.is_ascii_digit()))
                .map(ToString::to_string);
            DepStatus {
                name: bin.to_string(),
                installed: true,
                version,
                detail: raw,
            }
        }
        Ok(output) => DepStatus {
            name: bin.to_string(),
            installed: false,
            version: None,
            detail: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        },
        Err(e) => DepStatus {
            name: bin.to_string(),
            installed: false,
            version: None,
            detail: format!("not found: {e}"),
        },
    }
}

async fn install_agent_browser() -> Result<String> {
    use std::process::Stdio;
    use tokio::process::Command;
    println!("Installing agent-browser via npm...");
    let install_out = Command::new("npm")
        .args(["install", "-g", "agent-browser"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;
    if !install_out.status.success() {
        let stderr = String::from_utf8_lossy(&install_out.stderr);
        anyhow::bail!("npm install -g agent-browser failed: {stderr}");
    }

    println!("Downloading Chromium browser engine...");
    let chromium_out = Command::new("agent-browser")
        .arg("install")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;
    if !chromium_out.status.success() {
        let stderr = String::from_utf8_lossy(&chromium_out.stderr);
        anyhow::bail!("agent-browser install failed: {stderr}");
    }

    let status = check_dep_installed("agent-browser", "--version").await;
    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "ok": true,
        "installed": status.installed,
        "version": status.version,
    }))?)
}

async fn run_setup(command: SetupCommands) -> Result<String> {
    match command {
        SetupCommands::Browser => install_agent_browser().await,
        SetupCommands::All => {
            let browser_result = install_agent_browser().await;
            let mut results = Vec::new();
            results.push(serde_json::json!({
                "dep": "agent-browser",
                "ok": browser_result.is_ok(),
                "detail": browser_result.as_deref().unwrap_or("failed"),
            }));
            Ok(serde_json::to_string_pretty(&serde_json::json!({
                "ok": results.iter().all(|r| r["ok"].as_bool().unwrap_or(false)),
                "results": results,
            }))?)
        }
    }
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
    checks.push(serde_json::json!({
        "check": "memory",
        "ok": health.memory_healthy,
    }));
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
            "detail": dep.detail,
            "required": required,
        }));
    }

    if config.browser.enabled && !agent_browser.installed {
        checks.push(serde_json::json!({
            "check": "browser_tool_ready",
            "ok": false,
            "detail": "browser.enabled=true but agent-browser is not installed. Run: omninova setup browser",
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
    }))?)
}
