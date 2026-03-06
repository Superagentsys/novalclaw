use crate::config::Config;
use crate::daemon::service::{
    GatewayServiceCheckLevel, GatewayServiceCheckReport, GatewayServiceOperation,
    resolve_gateway_service,
};
use crate::gateway::GatewayRuntime;
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::net::ToSocketAddrs;
use std::path::Path;

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
        "telegram" => crate::channels::ChannelKind::Telegram,
        "discord" => crate::channels::ChannelKind::Discord,
        "slack" => crate::channels::ChannelKind::Slack,
        "whatsapp" => crate::channels::ChannelKind::Whatsapp,
        "matrix" => crate::channels::ChannelKind::Matrix,
        "email" => crate::channels::ChannelKind::Email,
        "webhook" => crate::channels::ChannelKind::Webhook,
        other => crate::channels::ChannelKind::Other(other.to_string()),
    }
}
