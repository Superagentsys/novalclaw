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
/// Paths that live under `config.workspace_dir` are always allowed through
/// here — the workspace is the agent's jail, so trying to read or write
/// inside it must not collide with system-wide forbidden prefixes like
/// `/home`, `/root`, etc. The actual per-tool canonicalization (e.g.
/// `resolve_workspace_relative`) is what guarantees the path cannot escape.
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

    // Permit anything under the configured workspace_dir. The workspace
    // is the agent's own sandbox; the jail check is enforced by the
    // individual tools (file_read / file_write / file_edit / shell).
    if let Ok(workspace_canon) = std::fs::canonicalize(&config.workspace_dir) {
        let expanded_path = std::path::Path::new(&expanded);
        if expanded_path.is_absolute() {
            if let Ok(expanded_canon) = std::fs::canonicalize(expanded_path) {
                if expanded_canon.starts_with(&workspace_canon) {
                    return None;
                }
            } else {
                // Path doesn't exist yet (e.g. a new file) — try to
                // canonicalize the longest existing parent.
                let mut cursor = expanded_path;
                while let Some(parent) = cursor.parent() {
                    if parent.as_os_str().is_empty() {
                        break;
                    }
                    if let Ok(parent_canon) = std::fs::canonicalize(parent) {
                        if let Some(file_name) = expanded_path.file_name() {
                            let candidate = parent_canon.join(file_name);
                            if candidate.starts_with(&workspace_canon) {
                                return None;
                            }
                        }
                        break;
                    }
                    cursor = parent;
                }
            }
        }
    }

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

/// Resolve a relative path against the workspace root, returning a canonical
/// absolute path that is guaranteed to live under the canonical workspace
/// root. Handles three cases that the per-tool implementations previously
/// got wrong:
///
/// 1. "." (or empty) → canonicalize the workspace root itself.
/// 2. An existing file or directory → canonicalize the full path.
/// 3. A *new* file inside an existing directory → canonicalize the parent
///    directory and re-attach the final segment. This prevents the
///    `path escapes workspace` error that occurred when the file itself
///    did not exist on disk.
///
/// `workspace_dir` may itself not exist yet; we ensure it (without
/// canonicalizing) so the basic "list / read the workspace root" case
/// works after a user picks a brand-new directory in the Workspace
/// button. The actual jail check uses the canonicalized form so symlinks
/// and `\\?\` UNC prefixes on Windows can be compared consistently.
pub async fn resolve_workspace_relative(
    workspace_dir: &Path,
    relative: &str,
) -> anyhow::Result<PathBuf> {
    let trimmed = relative.trim();
    if trimmed.is_empty() {
        anyhow::bail!("path is empty");
    }
    if trimmed.contains('\0') {
        anyhow::bail!("null bytes in path are not allowed");
    }
    let rel = Path::new(trimmed);
    if rel.is_absolute() {
        anyhow::bail!("absolute paths are not allowed");
    }
    if rel.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        anyhow::bail!("path traversal is not allowed");
    }

    // Ensure the workspace root exists; if the user just picked a fresh
    // directory, we create it on demand so read/list still succeed.
    if !workspace_dir.exists() {
        tokio::fs::create_dir_all(workspace_dir).await.map_err(|e| {
            anyhow::anyhow!(
                "workspace dir does not exist and could not be created: {e}"
            )
        })?;
    }
    let workspace_canon = tokio::fs::canonicalize(workspace_dir)
        .await
        .map_err(|e| anyhow::anyhow!("failed to resolve workspace dir: {e}"))?;

    let full_path = workspace_canon.join(rel);
    let resolved = match tokio::fs::metadata(&full_path).await {
        Ok(_) => tokio::fs::canonicalize(&full_path)
            .await
            .map_err(|e| anyhow::anyhow!("failed to resolve {trimmed}: {e}"))?,
        Err(_) => {
            // File does not exist (likely a new file). Canonicalize the
            // parent directory and re-attach the final segment; if the
            // parent doesn't exist either, try to create it so the caller
            // can proceed with a write.
            let parent = full_path
                .parent()
                .ok_or_else(|| anyhow::anyhow!("path has no parent directory"))?;
            let resolved_parent = if parent.exists() {
                tokio::fs::canonicalize(parent)
                    .await
                    .map_err(|e| anyhow::anyhow!("failed to resolve parent: {e}"))?
            } else {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| anyhow::anyhow!("parent dir does not exist: {e}"))?;
                tokio::fs::canonicalize(parent)
                    .await
                    .map_err(|e| anyhow::anyhow!("failed to resolve parent: {e}"))?
            };
            let file_name = full_path
                .file_name()
                .ok_or_else(|| anyhow::anyhow!("path has no file name"))?;
            resolved_parent.join(file_name)
        }
    };

    if !resolved.starts_with(&workspace_canon) {
        anyhow::bail!("path escapes workspace");
    }
    Ok(resolved)
}

impl SandboxConfig {
    pub fn effective_workspace_jail(&self) -> bool {
        self.enabled && self.workspace_jail
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_workspace_relative;
    use std::path::PathBuf;

    fn temp_workspace() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "omninova-workspace-resolver-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn resolves_dot_to_workspace_root() {
        let workspace = temp_workspace();
        let resolved = resolve_workspace_relative(&workspace, ".")
            .await
            .expect("dot resolves");
        let workspace_canon = tokio::fs::canonicalize(&workspace).await.unwrap();
        assert_eq!(resolved, workspace_canon);
        let _ = std::fs::remove_dir_all(&workspace);
    }

    #[tokio::test]
    async fn resolves_existing_relative_file() {
        let workspace = temp_workspace();
        let target = workspace.join("notes.txt");
        std::fs::write(&target, "hello").unwrap();
        let resolved = resolve_workspace_relative(&workspace, "notes.txt")
            .await
            .expect("existing file resolves");
        let expected = tokio::fs::canonicalize(&target).await.unwrap();
        assert_eq!(resolved, expected);
        let _ = std::fs::remove_dir_all(&workspace);
    }

    #[tokio::test]
    async fn resolves_new_file_without_treating_as_traversal() {
        let workspace = temp_workspace();
        let resolved = resolve_workspace_relative(&workspace, "presentation.html")
            .await
            .expect("new file resolves to inside workspace");
        let workspace_canon = tokio::fs::canonicalize(&workspace).await.unwrap();
        assert!(
            resolved.starts_with(&workspace_canon),
            "resolved {resolved:?} should be under {workspace_canon:?}"
        );
        assert!(resolved.ends_with("presentation.html"));
        let _ = std::fs::remove_dir_all(&workspace);
    }

    #[tokio::test]
    async fn rejects_parent_traversal() {
        let workspace = temp_workspace();
        let result = resolve_workspace_relative(&workspace, "../escape.txt").await;
        assert!(result.is_err(), "parent traversal must be rejected");
        let _ = std::fs::remove_dir_all(&workspace);
    }

    #[tokio::test]
    async fn rejects_absolute_path() {
        let workspace = temp_workspace();
        let result = resolve_workspace_relative(&workspace, "/etc/passwd").await;
        assert!(result.is_err(), "absolute paths must be rejected");
        let _ = std::fs::remove_dir_all(&workspace);
    }
}
