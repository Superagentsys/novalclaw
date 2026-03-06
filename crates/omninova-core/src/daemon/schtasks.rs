use crate::daemon::service::{
    GatewayService, GatewayServiceCheckLevel, GatewayServiceCheckReport,
    GatewayServicePreflightReport, GatewayServiceStatus, GatewayServiceStatusReport,
};
use anyhow::Result;
use std::process::Command;

pub struct SchtasksGatewayService;

impl GatewayService for SchtasksGatewayService {
    fn preflight_report(&self) -> GatewayServicePreflightReport {
        let hints = vec![
            format!("task name: {}", TASK_NAME),
            "run shell as Administrator if needed".to_string(),
        ];
        let checks = vec![
            match ensure_command_exists("schtasks") {
                Ok(()) => GatewayServiceCheckReport {
                    name: "schtasks".to_string(),
                    ok: true,
                    level: GatewayServiceCheckLevel::Error,
                    detail: "schtasks command found in PATH".to_string(),
                },
                Err(e) => GatewayServiceCheckReport {
                    name: "schtasks".to_string(),
                    ok: false,
                    level: GatewayServiceCheckLevel::Error,
                    detail: e.to_string(),
                },
            },
            check_current_exe(),
        ];
        let ok = checks.iter().all(|c| c.ok);
        GatewayServicePreflightReport {
            ok,
            detail: if ok {
                "schtasks preflight passed".to_string()
            } else {
                "schtasks preflight failed".to_string()
            },
            checks,
            hints,
        }
    }

    fn install(&self) -> Result<()> {
        let exe = std::env::current_exe()?;
        run(
            "schtasks",
            &[
                "/Create",
                "/TN",
                TASK_NAME,
                "/TR",
                &format!("\"{}\" gateway", exe.to_string_lossy()),
                "/SC",
                "ONLOGON",
                "/RL",
                "LIMITED",
                "/F",
            ],
        )?;
        Ok(())
    }
    fn uninstall(&self) -> Result<()> {
        run("schtasks", &["/Delete", "/TN", TASK_NAME, "/F"])?;
        Ok(())
    }
    fn start(&self) -> Result<()> {
        run("schtasks", &["/Run", "/TN", TASK_NAME])?;
        Ok(())
    }
    fn stop(&self) -> Result<()> {
        // schtasks cannot directly stop arbitrary process instance reliably.
        // We keep this as a best-effort no-op.
        Ok(())
    }
    fn status(&self) -> Result<GatewayServiceStatus> {
        Ok(self.status_report()?.status)
    }
    fn status_report(&self) -> Result<GatewayServiceStatusReport> {
        let output = Command::new("schtasks")
            .args(["/Query", "/TN", TASK_NAME])
            .output()?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if output.status.success() {
            Ok(GatewayServiceStatusReport {
                status: GatewayServiceStatus::Running,
                detail: if stdout.trim().is_empty() {
                    "schtasks entry exists".to_string()
                } else {
                    stdout
                },
                logs: vec!["Windows Event Viewer > TaskScheduler".to_string()],
            })
        } else {
            Ok(GatewayServiceStatusReport {
                status: GatewayServiceStatus::Stopped,
                detail: if stderr.trim().is_empty() {
                    "schtasks entry not found".to_string()
                } else {
                    stderr
                },
                logs: vec!["Windows Event Viewer > TaskScheduler".to_string()],
            })
        }
    }
}

const TASK_NAME: &str = "OmniNovaGateway";

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
