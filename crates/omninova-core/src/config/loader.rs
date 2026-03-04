use crate::config::schema::Config;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tracing::info;

const APP_DIR_NAME: &str = ".omninova";
const CONFIG_FILE_NAME: &str = "config.toml";
const ACTIVE_WORKSPACE_FILE: &str = "active_workspace.toml";

/// Resolve the config directory with the following priority:
///   1. `OMNINOVA_CONFIG_DIR` env var
///   2. `OMNINOVA_WORKSPACE` env var (config inferred as `<workspace>/../.omninova`)
///   3. `~/.omninova/active_workspace.toml` pointer
///   4. `~/.omninova/`
pub fn resolve_config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("OMNINOVA_CONFIG_DIR") {
        return PathBuf::from(dir);
    }

    if let Ok(ws) = std::env::var("OMNINOVA_WORKSPACE") {
        let ws_path = PathBuf::from(&ws);
        if let Some(parent) = ws_path.parent() {
            let candidate = parent.join(APP_DIR_NAME);
            if candidate.exists() {
                return candidate;
            }
        }
    }

    let home = home::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let default_dir = home.join(APP_DIR_NAME);

    let active_ws_path = default_dir.join(ACTIVE_WORKSPACE_FILE);
    if active_ws_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&active_ws_path) {
            if let Ok(table) = content.parse::<toml::Table>() {
                if let Some(toml::Value::String(dir)) = table.get("config_dir") {
                    let p = PathBuf::from(dir);
                    if p.exists() {
                        return p;
                    }
                }
            }
        }
    }

    default_dir
}

/// Resolve the full path to `config.toml`.
pub fn resolve_config_path() -> PathBuf {
    resolve_config_dir().join(CONFIG_FILE_NAME)
}

impl Config {
    /// Load config from disk, or create a default one if it does not exist.
    pub fn load_or_init() -> Result<Self> {
        let config_path = resolve_config_path();
        if config_path.exists() {
            Self::load_from(&config_path)
        } else {
            info!("No config found at {}, creating default", config_path.display());
            let mut cfg = Config::default();
            cfg.config_path = config_path.clone();
            cfg.ensure_dirs()?;
            cfg.save()?;
            Ok(cfg)
        }
    }

    /// Load config from a specific file path.
    pub fn load_from(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config from {}", path.display()))?;

        let mut cfg: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config from {}", path.display()))?;

        cfg.config_path = path.to_path_buf();
        if let Some(parent) = path.parent() {
            if cfg.workspace_dir.as_os_str().is_empty() {
                cfg.workspace_dir = parent.join("workspace");
            }
        }

        // Resolve relative workspace_dir against config dir
        if cfg.workspace_dir.is_relative() {
            if let Some(parent) = path.parent() {
                cfg.workspace_dir = parent.join(&cfg.workspace_dir);
            }
        }

        super::env::apply_env_overrides(&mut cfg);
        cfg.ensure_dirs()?;

        info!("Config loaded from {}", path.display());
        Ok(cfg)
    }

    /// Load from a TOML string (useful for testing).
    pub fn load_from_str(toml_str: &str) -> Result<Self> {
        let mut cfg: Config = toml::from_str(toml_str)
            .context("Failed to parse TOML config string")?;
        super::env::apply_env_overrides(&mut cfg);
        Ok(cfg)
    }

    /// Save the current config to disk as TOML.
    pub fn save(&self) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .context("Failed to serialize config to TOML")?;

        if let Some(parent) = self.config_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory {}", parent.display()))?;
        }

        std::fs::write(&self.config_path, content)
            .with_context(|| format!("Failed to write config to {}", self.config_path.display()))?;

        info!("Config saved to {}", self.config_path.display());
        Ok(())
    }

    /// Ensure workspace and config directories exist.
    fn ensure_dirs(&self) -> Result<()> {
        if !self.workspace_dir.as_os_str().is_empty() {
            std::fs::create_dir_all(&self.workspace_dir).with_context(|| {
                format!("Failed to create workspace dir {}", self.workspace_dir.display())
            })?;
        }
        Ok(())
    }

    /// Save the active workspace pointer so subsequent runs find this config.
    pub fn save_active_workspace(&self) -> Result<()> {
        let home = home::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let app_dir = home.join(APP_DIR_NAME);
        std::fs::create_dir_all(&app_dir)?;

        let active_path = app_dir.join(ACTIVE_WORKSPACE_FILE);
        let dir_str = self
            .config_path
            .parent()
            .unwrap_or(&self.config_path)
            .to_string_lossy();

        let content = format!(
            "# Auto-generated – points to the active OmniNova workspace\nconfig_dir = \"{}\"\n",
            dir_str
        );
        std::fs::write(&active_path, content)?;
        Ok(())
    }

    /// Merge another Config on top of self (non-None values win).
    pub fn merge_from_file(&mut self, path: &Path) -> Result<()> {
        let overlay_content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read overlay config {}", path.display()))?;
        let overlay: toml::Table = toml::from_str(&overlay_content)?;
        let base_content = toml::to_string(self)?;
        let mut base: toml::Table = toml::from_str(&base_content)?;

        merge_toml_tables(&mut base, &overlay);

        let merged_str = toml::to_string(&base)?;
        let merged: Config = toml::from_str(&merged_str)?;

        // Preserve computed fields
        let config_path = self.config_path.clone();
        let workspace_dir = self.workspace_dir.clone();
        *self = merged;
        self.config_path = config_path;
        self.workspace_dir = workspace_dir;
        Ok(())
    }
}

/// Deep-merge two TOML tables (overlay wins for leaf values).
fn merge_toml_tables(base: &mut toml::Table, overlay: &toml::Table) {
    for (key, overlay_val) in overlay {
        match (base.get_mut(key), overlay_val) {
            (Some(toml::Value::Table(b)), toml::Value::Table(o)) => {
                merge_toml_tables(b, o);
            }
            _ => {
                base.insert(key.clone(), overlay_val.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_round_trip() {
        let cfg = Config::default();
        let toml_str = toml::to_string_pretty(&cfg).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.default_temperature, 0.7);
        assert_eq!(parsed.gateway.port, 42617);
    }

    #[test]
    fn test_load_minimal_toml() {
        let toml_str = r#"
api_key = "sk-test"
default_provider = "openai"
default_model = "gpt-4o"
"#;
        let cfg = Config::load_from_str(toml_str).unwrap();
        assert_eq!(cfg.api_key.as_deref(), Some("sk-test"));
        assert_eq!(cfg.default_provider.as_deref(), Some("openai"));
        assert_eq!(cfg.autonomy.level, "supervised");
    }

    #[test]
    fn test_load_full_sections() {
        let toml_str = r#"
api_key = "sk-test"

[gateway]
host = "0.0.0.0"
port = 8080
allow_public_bind = true

[autonomy]
level = "autonomous"
max_actions_per_hour = 100

[security.otp]
enabled = true
method = "totp"

[runtime]
kind = "wasm"
reasoning_enabled = true

[runtime.wasm]
fuel_limit = 500000
memory_limit_mb = 64

[proxy]
enabled = true
http_proxy = "http://proxy:3128"
"#;
        let cfg = Config::load_from_str(toml_str).unwrap();
        assert_eq!(cfg.gateway.host, "0.0.0.0");
        assert_eq!(cfg.gateway.port, 8080);
        assert!(cfg.gateway.allow_public_bind);
        assert_eq!(cfg.autonomy.level, "autonomous");
        assert_eq!(cfg.autonomy.max_actions_per_hour, 100);
        assert!(cfg.security.otp.enabled);
        assert_eq!(cfg.runtime.kind, "wasm");
        assert!(cfg.runtime.reasoning_enabled);
        assert_eq!(cfg.runtime.wasm.fuel_limit, 500000);
        assert!(cfg.proxy.enabled);
    }
}
