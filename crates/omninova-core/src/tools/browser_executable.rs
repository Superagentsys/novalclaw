//! Prefer installed Chromium-family browsers over an implicit runtime download.
//! No user profile, remote debugging attachment, or sandbox relaxation is added.
use std::path::PathBuf;

pub(super) fn installed_browser() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("AGENT_BROWSER_EXECUTABLE_PATH").filter(|s| !s.is_empty()) {
        // Respect explicit user configuration, including errors in that configuration.
        return Some(PathBuf::from(path));
    }
    let mut candidates = Vec::new();
    #[cfg(target_os = "windows")]
    for root in ["PROGRAMFILES", "PROGRAMFILES(X86)", "LOCALAPPDATA"] {
        if let Some(root) = std::env::var_os(root) {
            for suffix in ["Microsoft/Edge/Application/msedge.exe", "Google/Chrome/Application/chrome.exe"] {
                candidates.push(PathBuf::from(&root).join(suffix));
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        let mut roots = vec![PathBuf::from("/Applications")];
        if let Some(home) = std::env::var_os("HOME") { roots.push(PathBuf::from(home).join("Applications")); }
        for root in roots {
            for suffix in ["Google Chrome.app/Contents/MacOS/Google Chrome", "Microsoft Edge.app/Contents/MacOS/Microsoft Edge", "Chromium.app/Contents/MacOS/Chromium"] {
                candidates.push(root.join(suffix));
            }
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    for path in ["/usr/bin/chromium", "/usr/bin/chromium-browser", "/usr/bin/google-chrome"] {
        candidates.push(PathBuf::from(path));
    }
    candidates.into_iter().find(|path| path.is_file())
}
