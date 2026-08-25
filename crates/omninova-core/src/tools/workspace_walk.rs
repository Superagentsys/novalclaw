use globset::{Glob, GlobSet, GlobSetBuilder};
use std::path::{Path, PathBuf};
use walkdir::{DirEntry, WalkDir};

/// Directories that are almost always dependency/build caches rather than
/// project source. They are ignored even when a project forgot to add them to
/// `.gitignore`, which keeps tool output and model context bounded.
const NOISE_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    ".venv",
    "venv",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    ".tox",
    ".nox",
    "node_modules",
    "target",
    ".omninova-sandbox-home",
];

pub(crate) fn is_noise_dir(entry: &DirEntry) -> bool {
    entry.file_type().is_dir()
        && NOISE_DIRS.iter().any(|name| {
            entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(name)
        })
}

/// Build a deterministic, symlink-safe workspace walker. Root `.gitignore`
/// exclusions are applied in addition to the built-in cache exclusions.
pub(crate) fn walk_workspace(root: &Path, max_depth: usize) -> WalkDir {
    WalkDir::new(root)
        .follow_links(false)
        .sort_by_file_name()
        .max_depth(max_depth)
}

pub(crate) fn root_gitignore(root: &Path) -> Option<GlobSet> {
    let contents = std::fs::read_to_string(root.join(".gitignore")).ok()?;
    let mut builder = GlobSetBuilder::new();
    let mut added = false;
    for raw in contents.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('!') {
            continue;
        }
        let normalized = line.trim_start_matches('/').trim_end_matches('/');
        if normalized.is_empty() {
            continue;
        }
        let patterns: Vec<String> = if normalized.contains('/') {
            vec![normalized.to_string(), format!("{normalized}/**")]
        } else {
            vec![
                normalized.to_string(),
                format!("**/{normalized}"),
                format!("**/{normalized}/**"),
            ]
        };
        for pattern in patterns {
            if let Ok(glob) = Glob::new(&pattern) {
                builder.add(glob);
                added = true;
            }
        }
    }
    added.then(|| builder.build().ok()).flatten()
}

pub(crate) fn relative_path<'a>(root: &Path, path: &'a Path) -> &'a Path {
    path.strip_prefix(root).unwrap_or(path)
}

pub(crate) fn is_gitignored(root: &Path, ignored: Option<&GlobSet>, path: &Path) -> bool {
    ignored
        .map(|set| set.is_match(relative_path(root, path)))
        .unwrap_or(false)
}

pub(crate) fn normalized_relative(root: &Path, path: &Path) -> String {
    relative_path(root, path)
        .to_string_lossy()
        .replace('\\', "/")
}

pub(crate) fn canonical_workspace(path: PathBuf) -> std::io::Result<PathBuf> {
    std::fs::canonicalize(path)
}
