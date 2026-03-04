use crate::config::schema::Config;
use anyhow::{bail, Result};

#[derive(Debug, Default)]
pub struct ValidationReport {
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

impl ValidationReport {
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }
}

impl Config {
    /// Validate the config and return a report with warnings and errors.
    pub fn validate(&self) -> ValidationReport {
        let mut report = ValidationReport::default();

        if self.api_key.is_none()
            && self.default_provider.as_deref() != Some("ollama")
            && self.default_provider.as_deref() != Some("mock")
        {
            report.warnings.push(
                "No api_key set. Most providers require an API key.".into(),
            );
        }

        if self.default_temperature < 0.0 || self.default_temperature > 2.0 {
            report.errors.push(format!(
                "default_temperature {} is out of range [0.0, 2.0]",
                self.default_temperature
            ));
        }

        if self.gateway.port == 0 {
            report.errors.push("gateway.port must be > 0".into());
        }

        if self.gateway.allow_public_bind && self.gateway.host == "0.0.0.0" && self.gateway.require_pairing {
            report.warnings.push(
                "Public bind on 0.0.0.0 with require_pairing=true; \
                 ensure pairing is configured."
                    .into(),
            );
        }

        if self.autonomy.max_actions_per_hour == 0 {
            report.warnings.push(
                "autonomy.max_actions_per_hour is 0 – the agent will not be able to act.".into(),
            );
        }

        if self.autonomy.level == "autonomous" && self.security.estop.enabled {
            report.warnings.push(
                "Running in autonomous mode with e-stop enabled. \
                 E-stop can still halt the agent."
                    .into(),
            );
        }

        if self.runtime.kind == "wasm" {
            if self.runtime.wasm.fuel_limit == 0 {
                report.errors.push("runtime.wasm.fuel_limit must be > 0".into());
            }
            if self.runtime.wasm.memory_limit_mb == 0 {
                report.errors.push("runtime.wasm.memory_limit_mb must be > 0".into());
            }
        }

        if self.agent.max_tool_iterations == 0 {
            report.warnings.push(
                "agent.max_tool_iterations is 0 – tool calls will be disabled.".into(),
            );
        }

        if self.cost.max_daily_cents == Some(0) {
            report.warnings.push(
                "cost.max_daily_cents is 0 – the agent will not be able to make API calls.".into(),
            );
        }

        for (name, delegate) in &self.agents {
            if delegate.allowed_tools.is_empty() && delegate.agentic {
                report.warnings.push(format!(
                    "Delegate agent '{}' is agentic but has no allowed_tools.",
                    name
                ));
            }
        }

        report
    }

    /// Validate and bail on first error.
    pub fn validate_or_bail(&self) -> Result<()> {
        let report = self.validate();
        for w in &report.warnings {
            tracing::warn!("config warning: {}", w);
        }
        if !report.errors.is_empty() {
            bail!(
                "Config validation failed:\n  - {}",
                report.errors.join("\n  - ")
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_validates() {
        let cfg = Config::default();
        let report = cfg.validate();
        assert!(report.is_ok(), "errors: {:?}", report.errors);
    }

    #[test]
    fn test_bad_temperature() {
        let mut cfg = Config::default();
        cfg.default_temperature = 5.0;
        let report = cfg.validate();
        assert!(!report.is_ok());
        assert!(report.errors.iter().any(|e| e.contains("temperature")));
    }

    #[test]
    fn test_zero_port() {
        let mut cfg = Config::default();
        cfg.gateway.port = 0;
        let report = cfg.validate();
        assert!(!report.is_ok());
    }
}
