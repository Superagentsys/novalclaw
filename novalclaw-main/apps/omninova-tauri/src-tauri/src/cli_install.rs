//! 将随包分发的 `omninova` CLI 安装到用户目录并配置 PATH（macOS / Linux / Windows）。

use serde::Serialize;
use std::io::Write;
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::process::Command as StdCommand;
use tauri::AppHandle;
use tauri::Manager;

const MARKER_LINE: &str = "# OmniNova CLI (PATH)";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliInstallStatus {
    pub bundled_available: bool,
    pub bundled_path: Option<String>,
    pub install_dir: String,
    pub installed_path: Option<String>,
    pub installed_same_as_bundle: bool,
    pub on_path: bool,
    pub hint: String,
}

fn bin_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "omninova.exe"
    } else {
        "omninova"
    }
}

pub fn resolve_bundled_omninova(app: &AppHandle) -> Option<PathBuf> {
    let resource_dir = app.path().resource_dir().ok()?;
    let name = bin_name();
    let candidates = [
        resource_dir.join("resources").join("cli").join(name),
        resource_dir.join("cli").join(name),
    ];
    candidates.iter().find(|p| p.is_file()).cloned()
}

fn user_install_dir() -> Result<PathBuf, String> {
    #[cfg(windows)]
    {
        let base = std::env::var("LOCALAPPDATA")
            .map_err(|_| "无法读取 LOCALAPPDATA（仅 Windows）".to_string())?;
        Ok(PathBuf::from(base).join("omninova").join("bin"))
    }
    #[cfg(not(windows))]
    {
        let home = std::env::var("HOME").map_err(|_| "无法读取 HOME".to_string())?;
        Ok(PathBuf::from(home).join(".local").join("bin"))
    }
}

fn user_install_path() -> Result<PathBuf, String> {
    Ok(user_install_dir()?.join(bin_name()))
}

fn path_var_contains_dir(path_var: &str, dir: &Path) -> bool {
    let dir_norm = normalize_path_for_compare(dir);
    for part in path_var.split(if cfg!(windows) { ';' } else { ':' }) {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        if normalize_path_for_compare(Path::new(p)) == dir_norm {
            return true;
        }
    }
    false
}

fn normalize_path_for_compare(p: &Path) -> String {
    let s = p.to_string_lossy();
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s.into_owned()
    }
}

fn file_len(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|m| m.len())
}

fn same_executable_as_bundle(src: &Path, dst: &Path) -> bool {
    match (file_len(src), file_len(dst)) {
        (Some(a), Some(b)) if a > 0 && a == b => true,
        _ => false,
    }
}

#[cfg(windows)]
fn windows_add_user_path(dir: &Path) -> Result<(), String> {
    let d = dir.to_string_lossy().replace('\'', "''");
    let ps = format!(
        "$d = [System.IO.Path]::GetFullPath('{d}'); \
         $u = [Environment]::GetEnvironmentVariable('Path', 'User'); \
         if ($null -eq $u) {{ $u = '' }} \
         if ($u -notlike ('*' + $d + '*')) {{ \
           [Environment]::SetEnvironmentVariable('Path', $u.TrimEnd(';') + ';' + $d, 'User') \
         }}",
        d = d
    );
    let output = StdCommand::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &ps])
        .output()
        .map_err(|e| format!("无法执行 PowerShell：{e}"))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        let out = String::from_utf8_lossy(&output.stdout);
        return Err(format!("更新用户 PATH 失败：{err}{out}"));
    }
    Ok(())
}

#[cfg(not(windows))]
fn unix_append_path_to_shell_rc() -> Result<(), String> {
    let home = std::env::var("HOME").map_err(|_| "HOME 未设置".to_string())?;
    let line = r#"export PATH="$HOME/.local/bin:$PATH""#;
    let block = format!("\n{MARKER_LINE}\n{line}\n");

    let primary = if cfg!(target_os = "macos") {
        PathBuf::from(&home).join(".zshrc")
    } else {
        PathBuf::from(&home).join(".profile")
    };

    if !unix_shell_file_has_path_hint(&primary)? {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&primary)
            .map_err(|e| format!("无法写入 {}：{e}", primary.display()))?;
        f.write_all(block.as_bytes()).map_err(|e| e.to_string())?;
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let bashrc = PathBuf::from(&home).join(".bashrc");
        if bashrc.is_file() && !unix_shell_file_has_path_hint(&bashrc)? {
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&bashrc)
                .map_err(|e| format!("无法写入 {}：{e}", bashrc.display()))?;
            f.write_all(block.as_bytes()).map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

#[cfg(not(windows))]
fn unix_shell_file_has_path_hint(path: &Path) -> Result<bool, String> {
    if !path.is_file() {
        return Ok(false);
    }
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    Ok(
        content.contains(MARKER_LINE)
            || (content.contains(".local/bin") && content.contains("PATH")),
    )
}

pub fn install_omninova_cli(app: &AppHandle) -> Result<String, String> {
    let src = resolve_bundled_omninova(app)
        .ok_or_else(|| "安装包内未找到 omninova CLI。请使用完整构建（需先编译 omninova 二进制）。".to_string())?;

    let dst_dir = user_install_dir()?;
    let dst = user_install_path()?;
    std::fs::create_dir_all(&dst_dir).map_err(|e| format!("创建目录失败：{e}"))?;
    if dst.is_file() {
        let _ = std::fs::remove_file(&dst);
    }
    std::fs::copy(&src, &dst).map_err(|e| format!("复制 CLI 失败：{e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&dst)
            .map_err(|e| e.to_string())?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&dst, perms).map_err(|e| e.to_string())?;
    }

    #[cfg(windows)]
    windows_add_user_path(&dst_dir)?;

    #[cfg(not(windows))]
    {
        let path_var = std::env::var("PATH").unwrap_or_default();
        if !path_var_contains_dir(&path_var, &dst_dir) {
            unix_append_path_to_shell_rc()?;
        }
    }

    let mut msg = format!("已安装到 {}", dst.display());
    if cfg!(windows) {
        msg.push_str("。请关闭并重新打开终端后运行 omninova --version。");
    } else {
        msg.push_str("。请重新打开终端；若仍无法识别 omninova，请确认已加载 shell 配置（如 source ~/.zshrc）。");
    }
    Ok(msg)
}

pub fn cli_install_status(app: &AppHandle) -> Result<CliInstallStatus, String> {
    let bundled = resolve_bundled_omninova(app);
    let bundled_available = bundled.is_some();
    let bundled_path = bundled.as_ref().map(|p| p.to_string_lossy().into_owned());

    let install_dir = user_install_dir()?;
    let install_dir_s = install_dir.to_string_lossy().into_owned();
    let installed_path_buf = user_install_path()?;
    let installed_path = if installed_path_buf.is_file() {
        Some(installed_path_buf.to_string_lossy().into_owned())
    } else {
        None
    };

    let installed_same_as_bundle = match (&bundled, installed_path_buf.is_file()) {
        (Some(src), true) => same_executable_as_bundle(src, &installed_path_buf),
        _ => false,
    };

    let path_var = std::env::var("PATH").unwrap_or_default();
    let on_path = path_var_contains_dir(&path_var, &install_dir);

    let hint = if !bundled_available {
        "当前未检测到随包 CLI。开发构建请先执行：cargo build -p omninova-core --bin omninova，再构建桌面应用。"
            .to_string()
    } else if installed_path.is_none() {
        "点击下方按钮将 omninova 安装到用户目录并写入 PATH（无需管理员权限）。".to_string()
    } else if !on_path && !cfg!(windows) {
        format!(
            "已安装到 {}，但当前进程 PATH 中可能尚未包含 {}。新终端或执行 source shell 配置后即可使用。",
            installed_path.as_deref().unwrap_or(""),
            install_dir_s
        )
    } else if !on_path && cfg!(windows) {
        "已安装；若终端仍找不到命令，请重新打开终端以使 PATH 生效。".to_string()
    } else if bundled_available && !installed_same_as_bundle {
        "检测到已安装版本与随包版本可能不一致，可重新安装以更新。".to_string()
    } else {
        "omninova 已在 PATH 中可用。".to_string()
    };

    Ok(CliInstallStatus {
        bundled_available,
        bundled_path,
        install_dir: install_dir_s,
        installed_path,
        installed_same_as_bundle,
        on_path,
        hint,
    })
}
