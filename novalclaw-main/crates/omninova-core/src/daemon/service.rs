use anyhow::Result;

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GatewayServiceStatus {
    Running,
    Stopped,
    Unknown,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GatewayServiceStatusReport {
    pub status: GatewayServiceStatus,
    pub detail: String,
    #[serde(default)]
    pub logs: Vec<String>,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GatewayServiceOperation {
    Install,
    Uninstall,
    Start,
    Stop,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GatewayServiceOperationReport {
    pub operation: GatewayServiceOperation,
    pub ok: bool,
    pub status_after: GatewayServiceStatus,
    pub detail: String,
    #[serde(default)]
    pub logs: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GatewayServiceCheckLevel {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GatewayServiceCheckReport {
    pub name: String,
    pub ok: bool,
    #[serde(default = "default_check_level")]
    pub level: GatewayServiceCheckLevel,
    pub detail: String,
}

fn default_check_level() -> GatewayServiceCheckLevel {
    GatewayServiceCheckLevel::Error
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GatewayServicePreflightReport {
    pub ok: bool,
    pub detail: String,
    #[serde(default)]
    pub checks: Vec<GatewayServiceCheckReport>,
    #[serde(default)]
    pub hints: Vec<String>,
}

pub trait GatewayService {
    fn install(&self) -> Result<()>;
    fn uninstall(&self) -> Result<()>;
    fn start(&self) -> Result<()>;
    fn stop(&self) -> Result<()>;
    fn status(&self) -> Result<GatewayServiceStatus>;
    fn status_report(&self) -> Result<GatewayServiceStatusReport> {
        Ok(GatewayServiceStatusReport {
            status: self.status()?,
            detail: String::new(),
            logs: Vec::new(),
        })
    }

    fn preflight_report(&self) -> GatewayServicePreflightReport {
        GatewayServicePreflightReport {
            ok: true,
            detail: "preflight not implemented for this platform adapter".to_string(),
            checks: Vec::new(),
            hints: Vec::new(),
        }
    }

    fn operate_report(&self, operation: GatewayServiceOperation) -> GatewayServiceOperationReport {
        let operation_result = match operation {
            GatewayServiceOperation::Install => self.install(),
            GatewayServiceOperation::Uninstall => self.uninstall(),
            GatewayServiceOperation::Start => self.start(),
            GatewayServiceOperation::Stop => self.stop(),
        };

        match operation_result {
            Ok(()) => match self.status_report() {
                Ok(status) => GatewayServiceOperationReport {
                    operation,
                    ok: true,
                    status_after: status.status,
                    detail: if status.detail.trim().is_empty() {
                        "ok".to_string()
                    } else {
                        status.detail
                    },
                    logs: status.logs,
                },
                Err(e) => GatewayServiceOperationReport {
                    operation,
                    ok: true,
                    status_after: GatewayServiceStatus::Unknown,
                    detail: format!("operation succeeded, but status check failed: {e}"),
                    logs: Vec::new(),
                },
            },
            Err(e) => {
                let fallback = self.status_report().ok();
                GatewayServiceOperationReport {
                    operation,
                    ok: false,
                    status_after: fallback
                        .as_ref()
                        .map(|s| s.status)
                        .unwrap_or(GatewayServiceStatus::Unknown),
                    detail: e.to_string(),
                    logs: fallback.map(|s| s.logs).unwrap_or_default(),
                }
            }
        }
    }
}

pub struct NoopGatewayService;

impl GatewayService for NoopGatewayService {
    fn install(&self) -> Result<()> {
        Ok(())
    }
    fn uninstall(&self) -> Result<()> {
        Ok(())
    }
    fn start(&self) -> Result<()> {
        Ok(())
    }
    fn stop(&self) -> Result<()> {
        Ok(())
    }
    fn status(&self) -> Result<GatewayServiceStatus> {
        Ok(GatewayServiceStatus::Unknown)
    }
    fn status_report(&self) -> Result<GatewayServiceStatusReport> {
        Ok(GatewayServiceStatusReport {
            status: GatewayServiceStatus::Unknown,
            detail: "no platform-specific daemon integration".to_string(),
            logs: Vec::new(),
        })
    }

    fn preflight_report(&self) -> GatewayServicePreflightReport {
        GatewayServicePreflightReport {
            ok: true,
            detail: "noop service (no daemon integration on this platform)".to_string(),
            checks: vec![GatewayServiceCheckReport {
                name: "platform-adapter".to_string(),
                ok: true,
                level: GatewayServiceCheckLevel::Info,
                detail: "no-op adapter selected".to_string(),
            }],
            hints: vec!["run `omninova gateway` directly".to_string()],
        }
    }
}

pub fn resolve_gateway_service() -> Box<dyn GatewayService + Send + Sync> {
    #[cfg(target_os = "macos")]
    {
        return Box::new(crate::daemon::launchd::LaunchdGatewayService);
    }
    #[cfg(target_os = "linux")]
    {
        return Box::new(crate::daemon::systemd::SystemdGatewayService);
    }
    #[cfg(target_os = "windows")]
    {
        return Box::new(crate::daemon::schtasks::SchtasksGatewayService);
    }
    #[allow(unreachable_code)]
    Box::new(NoopGatewayService)
}
