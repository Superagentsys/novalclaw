use crate::daemon::service::{
    GatewayService, GatewayServiceCheckLevel, GatewayServiceCheckReport,
    GatewayServicePreflightReport, GatewayServiceStatus, GatewayServiceStatusReport,
};
use anyhow::Result;
use std::path::PathBuf;
use std::process::Command;

pub struct LaunchdGatewayService;

impl GatewayService for LaunchdGatewayService {
    fn preflight_report(&self) -> GatewayServicePreflightReport {
        match service_plist_path() {
            Ok(plist) => {
                let mut checks = Vec::new();
                let mut hints = vec![format!("plist path: {}", plist.display())];
                if let Some(parent) = plist.parent() {
                    hints.push(format!("ensure writable dir: {}", parent.display()));
                }
                checks.push(match ensure_command_exists("launchctl") {
                    Ok(()) => GatewayServiceCheckReport {
                        name: "launchctl".to_string(),
                        ok: true,
                        level: GatewayServiceCheckLevel::Error,
                        detail: "launchctl command found in PATH".to_string(),
                    },
                    Err(e) => GatewayServiceCheckReport {
                        name: "launchctl".to_string(),
                        ok: false,
                        level: GatewayServiceCheckLevel::Error,
                        detail: e.to_string(),
                    },
                });
                checks.push(check_current_exe());
                checks.push(check_parent_dir_writable(&plist, "plist-parent"));

                let ok = checks.iter().all(|c| c.ok);
                GatewayServicePreflightReport {
                    ok,
                    detail: if ok {
                        "launchd preflight passed".to_string()
                    } else {
                        "launchd preflight failed".to_string()
                    },
                    checks,
                    hints,
                }
            }
            Err(e) => GatewayServicePreflightReport {
                ok: false,
                detail: e.to_string(),
                checks: vec![GatewayServiceCheckReport {
                    name: "plist-path".to_string(),
                    ok: false,
                    level: GatewayServiceCheckLevel::Error,
                    detail: "failed to resolve launchd plist path".to_string(),
                }],
                hints: Vec::new(),
            },
        }
    }

    fn install(&self) -> Result<()> {
        let plist = service_plist_path()?;
        if let Some(parent) = plist.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&plist, service_plist_xml()?)?;
        run("launchctl", &["load", "-w", plist.to_string_lossy().as_ref()])?;
        Ok(())
    }
    fn uninstall(&self) -> Result<()> {
        let plist = service_plist_path()?;
        let _ = run("launchctl", &["unload", "-w", plist.to_string_lossy().as_ref()]);
        let _ = std::fs::remove_file(&plist);
        Ok(())
    }
    fn start(&self) -> Result<()> {
        run("launchctl", &["start", SERVICE_LABEL])?;
        Ok(())
    }
    fn stop(&self) -> Result<()> {
        run("launchctl", &["stop", SERVICE_LABEL])?;
        Ok(())
    }
    fn status(&self) -> Result<GatewayServiceStatus> {
        Ok(self.status_report()?.status)
    }
    fn status_report(&self) -> Result<GatewayServiceStatusReport> {
        let output = Command::new("launchctl")
            .args(["list", SERVICE_LABEL])
            .output()?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if output.status.success() {
            Ok(GatewayServiceStatusReport {
                status: GatewayServiceStatus::Running,
                detail: if stdout.trim().is_empty() {
                    "launchd service is loaded".to_string()
                } else {
                    stdout
                },
                logs: default_log_paths(),
            })
        } else {
            Ok(GatewayServiceStatusReport {
                status: GatewayServiceStatus::Stopped,
                detail: if stderr.trim().is_empty() {
                    "launchd service is not loaded".to_string()
                } else {
                    stderr
                },
                logs: default_log_paths(),
            })
        }
    }
}

const SERVICE_LABEL: &str = "com.omninova.gateway";

fn service_plist_path() -> Result<PathBuf> {
    let home = home::home_dir().ok_or_else(|| anyhow::anyhow!("failed to resolve home dir"))?;
    Ok(home.join("Library/LaunchAgents").join(format!("{SERVICE_LABEL}.plist")))
}

fn service_plist_xml() -> Result<String> {
    let exe = std::env::current_exe()?;
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
  <dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
      <string>{exe}</string>
      <string>gateway</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>/tmp/omninova-gateway.out.log</string>
    <key>StandardErrorPath</key>
    <string>/tmp/omninova-gateway.err.log</string>
  </dict>
</plist>
"#,
        label = SERVICE_LABEL,
        exe = exe.to_string_lossy()
    );
    Ok(xml)
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

fn default_log_paths() -> Vec<String> {
    vec![
        "/tmp/omninova-gateway.out.log".to_string(),
        "/tmp/omninova-gateway.err.log".to_string(),
    ]
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
