use crate::config::{Config, SandboxConfig};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Minimal environment for sandboxed shell execution.
pub fn sandbox_env(config: &Config) -> HashMap<String, String> {
    let sandbox = &config.security.sandbox;
    if !sandbox.enabled {
        return HashMap::new();
    }

    let mut env = HashMap::new();
    let path = std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_string());
    env.insert("PATH".to_string(), path);
    env.insert(
        "HOME".to_string(),
        sandbox_home_dir(config).to_string_lossy().to_string(),
    );
    env.insert("LANG".to_string(), "C.UTF-8".to_string());
    env.insert("LC_ALL".to_string(), "C.UTF-8".to_string());

    for key in &sandbox.allowed_env_vars {
        if let Ok(val) = std::env::var(key) {
            env.insert(key.clone(), val);
        }
    }
    env
}

pub fn sandbox_home_dir(config: &Config) -> PathBuf {
    config.workspace_dir.join(".omninova-sandbox-home")
}

pub fn sandbox_enabled(config: &Config) -> bool {
    config.security.sandbox.enabled
}

pub async fn ensure_sandbox_home(config: &Config) -> anyhow::Result<()> {
    if !config.security.sandbox.enabled {
        return Ok(());
    }
    tokio::fs::create_dir_all(sandbox_home_dir(config)).await?;
    Ok(())
}

/// Reject paths that match configured forbidden prefixes (supports `~` expansion).
pub fn path_hits_forbidden(config: &Config, path: &str) -> Option<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return None;
    }
    let expanded = if trimmed.starts_with("~/") {
        home::home_dir()
            .map(|h| h.join(trimmed.trim_start_matches("~/")))
            .and_then(|p| p.to_str().map(|s| s.to_string()))
            .unwrap_or_else(|| trimmed.to_string())
    } else {
        trimmed.to_string()
    };

    for forbidden in &config.autonomy.forbidden_paths {
        let prefix = if forbidden.starts_with("~/") {
            home::home_dir()
                .map(|h| h.join(forbidden.trim_start_matches("~/")))
                .and_then(|p| p.to_str().map(|s| s.to_string()))
                .unwrap_or_else(|| forbidden.clone())
        } else {
            forbidden.clone()
        };
        if expanded == prefix || expanded.starts_with(&format!("{prefix}/")) {
            return Some(format!("path is forbidden by policy: {prefix}"));
        }
    }
    None
}

pub fn validate_relative_workspace_path(workspace: &Path, relative: &str) -> anyhow::Result<PathBuf> {
    let rel = Path::new(relative);
    if rel.is_absolute() {
        anyhow::bail!("absolute paths are not allowed");
    }
    if relative.contains('\0') {
        anyhow::bail!("null bytes in path are not allowed");
    }
    if relative.split('/').any(|part| part == "..") {
        anyhow::bail!("path traversal is not allowed");
    }
    Ok(workspace.join(rel))
}

impl SandboxConfig {
    pub fn effective_workspace_jail(&self) -> bool {
        self.enabled && self.workspace_jail
    }
}
