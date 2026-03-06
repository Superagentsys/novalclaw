use crate::daemon::service::{
    GatewayService, GatewayServiceCheckLevel, GatewayServiceCheckReport,
    GatewayServicePreflightReport, GatewayServiceStatus, GatewayServiceStatusReport,
};
use anyhow::Result;
use std::path::PathBuf;
use std::process::Command;

pub struct SystemdGatewayService;

impl GatewayService for SystemdGatewayService {
    fn preflight_report(&self) -> GatewayServicePreflightReport {
        let mut hints = vec![
            "requires user systemd session".to_string(),
            "check loginctl enable-linger if needed".to_string(),
        ];
        let mut checks = Vec::new();
        if let Ok(path) = service_file_path() {
            hints.push(format!("unit path: {}", path.display()));
            checks.push(check_parent_dir_writable(&path, "unit-parent"));
        }
        checks.push(match ensure_command_exists("systemctl") {
            Ok(()) => GatewayServiceCheckReport {
                name: "systemctl".to_string(),
                ok: true,
                level: GatewayServiceCheckLevel::Error,
                detail: "systemctl command found in PATH".to_string(),
            },
            Err(e) => GatewayServiceCheckReport {
                name: "systemctl".to_string(),
                ok: false,
                level: GatewayServiceCheckLevel::Error,
                detail: e.to_string(),
            },
        });
        checks.push(check_current_exe());

        let ok = checks.iter().all(|c| c.ok);
        GatewayServicePreflightReport {
            ok,
            detail: if ok {
                "systemd preflight passed".to_string()
            } else {
                "systemd preflight failed".to_string()
            },
            checks,
            hints,
        }
    }

    fn install(&self) -> Result<()> {
        let service_file = service_file_path()?;
        if let Some(parent) = service_file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&service_file, service_unit()?)?;
        run("systemctl", &["--user", "daemon-reload"])?;
        run("systemctl", &["--user", "enable", "--now", SERVICE_NAME])?;
        Ok(())
    }
    fn uninstall(&self) -> Result<()> {
        let _ = run("systemctl", &["--user", "disable", "--now", SERVICE_NAME]);
        let _ = std::fs::remove_file(service_file_path()?);
        let _ = run("systemctl", &["--user", "daemon-reload"]);
        Ok(())
    }
    fn start(&self) -> Result<()> {
        run("systemctl", &["--user", "start", SERVICE_NAME])?;
        Ok(())
    }
    fn stop(&self) -> Result<()> {
        run("systemctl", &["--user", "stop", SERVICE_NAME])?;
        Ok(())
    }
    fn status(&self) -> Result<GatewayServiceStatus> {
        Ok(self.status_report()?.status)
    }
    fn status_report(&self) -> Result<GatewayServiceStatusReport> {
        let output = Command::new("systemctl")
            .args(["--user", "is-active", SERVICE_NAME])
            .output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if stdout.trim() == "active" {
            Ok(GatewayServiceStatusReport {
                status: GatewayServiceStatus::Running,
                detail: "systemd user service is active".to_string(),
                logs: vec![
                    "journalctl --user -u omninova-gateway.service -n 200 --no-pager".to_string(),
                ],
            })
        } else {
            Ok(GatewayServiceStatusReport {
                status: GatewayServiceStatus::Stopped,
                detail: if !stderr.trim().is_empty() {
                    stderr
                } else {
                    format!("systemd state: {}", stdout.trim())
                },
                logs: vec![
                    "journalctl --user -u omninova-gateway.service -n 200 --no-pager".to_string(),
                ],
            })
        }
    }
}

const SERVICE_NAME: &str = "omninova-gateway.service";

fn service_file_path() -> Result<PathBuf> {
    let home = home::home_dir().ok_or_else(|| anyhow::anyhow!("failed to resolve home dir"))?;
    Ok(home.join(".config/systemd/user").join(SERVICE_NAME))
}

fn service_unit() -> Result<String> {
    let exe = std::env::current_exe()?;
    Ok(format!(
        r#"[Unit]
Description=OmniNova Gateway
After=network.target

[Service]
ExecStart={} gateway
Restart=always
RestartSec=2

[Install]
WantedBy=default.target
"#,
        exe.to_string_lossy()
    ))
}

fn run(program: &str, args: &[&str]) -> Result<()> {
    ensure_command_exists(program)?;
    let output = Command::new(program).args(args).output()?;
    if output.status.success() {
        Ok(())
    } else {
        let code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "{program} {:?} failed (code {}): stdout='{}' stderr='{}'",
            args,
            code,
            stdout.trim(),
            stderr.trim()
        )
    }
}

fn ensure_command_exists(program: &str) -> Result<()> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let exists = std::env::split_paths(&path).any(|p| p.join(program).exists());
    if exists {
        Ok(())
    } else {
        anyhow::bail!("required command '{}' not found in PATH", program)
    }
}

fn check_current_exe() -> GatewayServiceCheckReport {
    match std::env::current_exe() {
        Ok(path) => {
            let ok = path.is_file();
            GatewayServiceCheckReport {
                name: "current-exe".to_string(),
                ok,
                level: GatewayServiceCheckLevel::Error,
                detail: if ok {
                    format!("resolved executable: {}", path.display())
                } else {
                    format!("resolved path is not a file: {}", path.display())
                },
            }
        }
        Err(e) => GatewayServiceCheckReport {
            name: "current-exe".to_string(),
            ok: false,
            level: GatewayServiceCheckLevel::Error,
            detail: e.to_string(),
        },
    }
}

fn check_parent_dir_writable(target: &PathBuf, check_name: &str) -> GatewayServiceCheckReport {
    let Some(parent) = target.parent() else {
        return GatewayServiceCheckReport {
            name: check_name.to_string(),
            ok: false,
            level: GatewayServiceCheckLevel::Error,
            detail: format!("target has no parent directory: {}", target.display()),
        };
    };
    if !parent.exists() {
        return GatewayServiceCheckReport {
            name: check_name.to_string(),
            ok: true,
            level: GatewayServiceCheckLevel::Error,
            detail: format!("parent does not exist yet (will be created): {}", parent.display()),
        };
    }
    match std::fs::metadata(parent) {
        Ok(meta) => {
            if !meta.is_dir() {
                return GatewayServiceCheckReport {
                    name: check_name.to_string(),
                    ok: false,
                    level: GatewayServiceCheckLevel::Error,
                    detail: format!("parent is not a directory: {}", parent.display()),
                };
            }
            GatewayServiceCheckReport {
                name: check_name.to_string(),
                ok: !meta.permissions().readonly(),
                level: GatewayServiceCheckLevel::Error,
                detail: if meta.permissions().readonly() {
                    format!("parent directory is read-only: {}", parent.display())
                } else {
                    format!("parent directory writable: {}", parent.display())
                },
            }
        }
        Err(e) => GatewayServiceCheckReport {
            name: check_name.to_string(),
            ok: false,
            level: GatewayServiceCheckLevel::Error,
            detail: format!("failed to stat parent directory {}: {e}", parent.display()),
        },
    }
}
