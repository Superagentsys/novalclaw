use super::{A11yNode, DesktopDriver, ForegroundApp};
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::io::Write;
use std::path::Path;
use std::process::Command;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::process::Stdio;

pub struct OsDesktopDriver;

impl DesktopDriver for OsDesktopDriver {
    fn capture_png(&self, dest: &Path) -> Result<(u32, u32), String> {
        capture_png(dest)
    }

    fn click(&self, x: i32, y: i32, button: &str) -> Result<(), String> {
        click(x, y, button)
    }

    fn paste_text(&self, text: &str) -> Result<(), String> {
        paste_text(text)
    }

    fn press(&self, key: &str) -> Result<(), String> {
        press(key)
    }

    fn scroll(&self, direction: &str, amount: i32) -> Result<(), String> {
        scroll(direction, amount)
    }

    fn foreground_app(&self) -> Result<ForegroundApp, String> {
        foreground_app()
    }

    fn accessibility_snapshot(&self, max_nodes: usize) -> Result<Vec<A11yNode>, String> {
        snapshot_front_window(max_nodes)
    }

    fn launch(&self, target: &str, workspace: &Path) -> Result<String, String> {
        launch(target, workspace)
    }

    fn invoke_at(&self, x: i32, y: i32) -> Result<(), String> {
        invoke_at(x, y)
    }
}

fn capture_png(dest: &Path) -> Result<(u32, u32), String> {
    if let Ok(dims) = capture_via_screenshots_crate(dest) {
        return Ok(dims);
    }
    capture_via_os_tool(dest)?;
    png_dimensions(dest)
}

fn capture_via_screenshots_crate(dest: &Path) -> Result<(u32, u32), String> {
    let screens = screenshots::Screen::all().map_err(|e| e.to_string())?;
    let screen = screens
        .into_iter()
        .next()
        .ok_or_else(|| "no display available (headless host cannot use computer_use)".to_string())?;
    let image = screen.capture().map_err(|e| e.to_string())?;
    let width = image.width();
    let height = image.height();
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let file = std::fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer
        .write_image_data(image.as_raw())
        .map_err(|e| e.to_string())?;
    Ok((width, height))
}

fn png_dimensions(path: &Path) -> Result<(u32, u32), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read screenshot failed: {e}"))?;
    let image = image::load_from_memory(&bytes).map_err(|e| format!("decode screenshot failed: {e}"))?;
    use image::GenericImageView;
    Ok(image.dimensions())
}

#[cfg(target_os = "macos")]
fn capture_via_os_tool(dest: &Path) -> Result<(), String> {
    let path = dest
        .to_str()
        .ok_or_else(|| "screenshot path is not valid UTF-8".to_string())?;
    let status = Command::new("/usr/sbin/screencapture")
        .args(["-x", "-C", "-t", "png", path])
        .status()
        .map_err(|e| format!("screencapture: {e}"))?;
    if !status.success() {
        return Err(
            "屏幕截取失败。macOS 需在 系统设置 → 隐私与安全性 → 屏幕录制 中授权 OmniNova。".into(),
        );
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn capture_via_os_tool(dest: &Path) -> Result<(), String> {
    let path = dest
        .to_str()
        .ok_or_else(|| "screenshot path is not valid UTF-8".to_string())?;
    for (bin, args) in [
        ("gnome-screenshot", vec!["-f", path]),
        ("import", vec!["-window", "root", path]),
        ("scrot", vec![path]),
    ] {
        if let Ok(status) = Command::new(bin).args(&args).status() {
            if status.success() {
                return Ok(());
            }
        }
    }
    Err("Linux 截图失败：未找到可用桌面，或缺少 gnome-screenshot/import/scrot。无图形会话时不能使用 computer_use。".into())
}

#[cfg(target_os = "windows")]
fn capture_via_os_tool(_dest: &Path) -> Result<(), String> {
    Err("Windows screenshot fallback unavailable; screenshots crate failed".into())
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn capture_via_os_tool(_dest: &Path) -> Result<(), String> {
    Err("unsupported platform for computer_use".into())
}

#[cfg(target_os = "macos")]
fn click(x: i32, y: i32, button: &str) -> Result<(), String> {
    let script = match button {
        "right" => format!(
            "tell application \"System Events\" to click at {{{x}, {y}}} with right button"
        ),
        _ => format!("tell application \"System Events\" to click at {{{x}, {y}}}"),
    };
    run_osascript(&script).map(|_| ())
}

#[cfg(target_os = "macos")]
fn paste_text(text: &str) -> Result<(), String> {
    let previous = Command::new("pbpaste").output().ok().map(|o| o.stdout);
    write_pasteboard(text.as_bytes())?;
    run_osascript(
        "tell application \"System Events\" to keystroke \"v\" using command down",
    )?;
    if let Some(previous) = previous {
        let _ = write_pasteboard(&previous);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn write_pasteboard(bytes: &[u8]) -> Result<(), String> {
    let mut child = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("pbcopy: {e}"))?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(bytes)
            .map_err(|e| format!("pbcopy write: {e}"))?;
    }
    let status = child.wait().map_err(|e| format!("pbcopy wait: {e}"))?;
    if !status.success() {
        return Err("pbcopy failed".into());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn press(key: &str) -> Result<(), String> {
    let script = macos_key_script(key)?;
    run_osascript(&script).map(|_| ())
}

#[cfg(target_os = "macos")]
fn scroll(direction: &str, amount: i32) -> Result<(), String> {
    let code = match direction {
        "up" => 126,
        "left" => 123,
        "right" => 124,
        _ => 125,
    };
    let n = amount.clamp(1, 30);
    let script = format!(
        "tell application \"System Events\"\nrepeat {n} times\nkey code {code}\nend repeat\nend tell"
    );
    run_osascript(&script).map(|_| ())
}

#[cfg(target_os = "macos")]
fn foreground_app() -> Result<ForegroundApp, String> {
    let name = run_osascript(
        "tell application \"System Events\" to get name of first application process whose frontmost is true",
    )?;
    if name.is_empty() {
        return Err("could not read frontmost app".into());
    }
    Ok(ForegroundApp { name })
}

#[cfg(target_os = "macos")]
fn run_osascript(script: &str) -> Result<String, String> {
    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| format!("osascript: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "桌面控制失败。macOS 需在 系统设置 → 隐私与安全性 → 辅助功能 中授权 OmniNova。{stderr}"
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(target_os = "macos")]
fn macos_key_script(key: &str) -> Result<String, String> {
    let (code, modifiers) = parse_macos_hotkey(key)?;
    let using = if modifiers.is_empty() {
        String::new()
    } else {
        format!(" using {{{}}}", modifiers.join(", "))
    };
    Ok(format!(
        "tell application \"System Events\" to key code {code}{using}"
    ))
}

#[cfg(target_os = "macos")]
fn parse_macos_hotkey(key: &str) -> Result<(u32, Vec<&'static str>), String> {
    let mut modifiers = Vec::new();
    let mut name = String::new();
    for part in key.split(['+', '-', ' ']) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        match part.to_ascii_lowercase().as_str() {
            "cmd" | "command" | "meta" => modifiers.push("command down"),
            "shift" => modifiers.push("shift down"),
            "alt" | "option" | "opt" => modifiers.push("option down"),
            "ctrl" | "control" => modifiers.push("control down"),
            other => name = other.to_string(),
        }
    }
    let code = macos_key_code(&name).ok_or_else(|| format!("unsupported key '{key}'"))?;
    Ok((code, modifiers))
}

#[cfg(target_os = "macos")]
fn macos_key_code(name: &str) -> Option<u32> {
    Some(match name {
        "enter" | "return" => 36,
        "tab" => 48,
        "esc" | "escape" => 53,
        "backspace" | "delete" => 51,
        "space" | "spacebar" => 49,
        "up" => 126,
        "down" => 125,
        "left" => 123,
        "right" => 124,
        "home" => 115,
        "end" => 119,
        "pageup" => 116,
        "pagedown" => 121,
        other if other.len() == 1 => {
            let ch = other.chars().next()?.to_ascii_lowercase();
            match ch {
                'a' => 0,
                's' => 1,
                'd' => 2,
                'f' => 3,
                'h' => 4,
                'g' => 5,
                'z' => 6,
                'x' => 7,
                'c' => 8,
                'v' => 9,
                'b' => 11,
                'q' => 12,
                'w' => 13,
                'e' => 14,
                'r' => 15,
                'y' => 16,
                't' => 17,
                '1' => 18,
                '2' => 19,
                '3' => 20,
                '4' => 21,
                '6' => 22,
                '5' => 23,
                '=' => 24,
                '9' => 25,
                '7' => 26,
                '-' => 27,
                '8' => 28,
                '0' => 29,
                'o' => 31,
                'u' => 32,
                'i' => 34,
                'p' => 35,
                'l' => 37,
                'j' => 38,
                'k' => 40,
                'n' => 45,
                'm' => 46,
                _ => return None,
            }
        }
        _ => return None,
    })
}

#[cfg(target_os = "linux")]
fn click(x: i32, y: i32, button: &str) -> Result<(), String> {
    let btn = match button {
        "right" => "3",
        "middle" => "2",
        _ => "1",
    };
    run_xdotool(&["mousemove", &x.to_string(), &y.to_string(), "click", btn])
}

#[cfg(target_os = "linux")]
fn paste_text(text: &str) -> Result<(), String> {
    let previous = Command::new("xclip")
        .args(["-selection", "clipboard", "-o"])
        .output()
        .ok()
        .map(|o| o.stdout);
    let mut child = Command::new("xclip")
        .args(["-selection", "clipboard"])
        .stdin(Stdio::piped())
        .spawn()
        .or_else(|_| {
            Command::new("wl-copy")
                .stdin(Stdio::piped())
                .spawn()
        })
        .map_err(|_| {
            "Linux type 需要 xclip 或 wl-copy，以及 xdotool。无图形会话时不能使用 computer_use。"
                .to_string()
        })?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    }
    child.wait().map_err(|e| e.to_string())?;
    run_xdotool(&["key", "ctrl+v"])?;
    if let Some(previous) = previous {
        if let Ok(mut restore) = Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(Stdio::piped())
            .spawn()
        {
            if let Some(stdin) = restore.stdin.as_mut() {
                let _ = stdin.write_all(&previous);
            }
            let _ = restore.wait();
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn press(key: &str) -> Result<(), String> {
    run_xdotool(&["key", &normalize_xdotool_key(key)])
}

#[cfg(target_os = "linux")]
fn scroll(direction: &str, amount: i32) -> Result<(), String> {
    let key = match direction {
        "up" => "Up",
        "left" => "Left",
        "right" => "Right",
        _ => "Down",
    };
    for _ in 0..amount.clamp(1, 30) {
        run_xdotool(&["key", key])?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn foreground_app() -> Result<ForegroundApp, String> {
    let id = command_stdout("xdotool", &["getactivewindow"])?;
    let name = command_stdout("xdotool", &["getwindowname", id.trim()])?;
    Ok(ForegroundApp { name })
}

#[cfg(target_os = "linux")]
fn run_xdotool(args: &[&str]) -> Result<(), String> {
    command_stdout("xdotool", args).map(|_| ())
}

#[cfg(target_os = "linux")]
fn normalize_xdotool_key(key: &str) -> String {
    key.replace("cmd", "super")
        .replace("command", "super")
        .replace("option", "alt")
        .replace("return", "Return")
        .replace("enter", "Return")
        .replace("esc", "Escape")
        .replace("escape", "Escape")
}

#[cfg(target_os = "linux")]
fn command_stdout(bin: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(bin)
        .args(args)
        .output()
        .map_err(|e| {
            format!("{bin} 不可用：{e}。Linux computer_use 需要图形会话和 xdotool。")
        })?;
    if !output.status.success() {
        return Err(format!(
            "{bin} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(target_os = "windows")]
fn click(x: i32, y: i32, button: &str) -> Result<(), String> {
    let down_up = match button {
        "right" => (0x0008, 0x0010),
        _ => (0x0002, 0x0004),
    };
    let script = format!(
        r#"
Add-Type -AssemblyName System.Windows.Forms
[System.Windows.Forms.Cursor]::Position = New-Object System.Drawing.Point({x}, {y})
Add-Type -Namespace W -Name U -MemberDefinition '[DllImport("user32.dll")] public static extern void mouse_event(int f,int a,int b,int c,int d);'
[W.U]::mouse_event({down},0,0,0,0)
[W.U]::mouse_event({up},0,0,0,0)
"#,
        down = down_up.0,
        up = down_up.1
    );
    run_powershell(&script)
}

/// Shared P/Invoke preamble for keyboard/mouse input on Windows. `keybd_event`
/// / `mouse_event` accept virtual-key codes directly, so unlike SendKeys they
/// support the Win key and never misparse brace or modifier characters.
#[cfg(target_os = "windows")]
const WIN_INPUT_PREAMBLE: &str = r#"
Add-Type -Namespace W -Name Inp -MemberDefinition '[DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, System.UIntPtr extra); [DllImport("user32.dll")] public static extern void mouse_event(int f, int a, int b, int c, int d);'
"#;

#[cfg(target_os = "windows")]
fn paste_text(text: &str) -> Result<(), String> {
    // Base64 round-trip: here-strings corrupt apostrophes and any line that
    // looks like a terminator; base64 survives every character including CJK.
    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, text.as_bytes());
    let script = format!(
        r#"
{preamble}
$text = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String('{b64}'))
Set-Clipboard -Value $text
Start-Sleep -Milliseconds 120
[W.Inp]::keybd_event(0x11, 0, 0, [System.UIntPtr]::Zero)
[W.Inp]::keybd_event(0x56, 0, 0, [System.UIntPtr]::Zero)
[W.Inp]::keybd_event(0x56, 0, 2, [System.UIntPtr]::Zero)
[W.Inp]::keybd_event(0x11, 0, 2, [System.UIntPtr]::Zero)
"#,
        preamble = WIN_INPUT_PREAMBLE,
        b64 = b64
    );
    run_powershell(&script)
}

#[cfg(target_os = "windows")]
fn press(key: &str) -> Result<(), String> {
    let sequence = windows_key_vks(key)?;
    let mut body = String::new();
    for vk in &sequence {
        body.push_str(&format!(
            "[W.Inp]::keybd_event(0x{vk:02X}, 0, 0, [System.UIntPtr]::Zero)\n"
        ));
    }
    for vk in sequence.iter().rev() {
        body.push_str(&format!(
            "[W.Inp]::keybd_event(0x{vk:02X}, 0, 2, [System.UIntPtr]::Zero)\n"
        ));
    }
    let script = format!("{}\n{}", WIN_INPUT_PREAMBLE, body);
    run_powershell(&script)
}

#[cfg(target_os = "windows")]
fn scroll(direction: &str, amount: i32) -> Result<(), String> {
    // Real wheel events scroll the control under the cursor (Excel grids,
    // Word pages); arrow keys only move a selection/caret.
    const WHEEL: i32 = 0x0800;
    const HWHEEL: i32 = 0x01000;
    let step = 120 * amount.clamp(1, 30);
    let (flag, delta) = match direction {
        "up" => (WHEEL, step),
        "left" => (HWHEEL, -step),
        "right" => (HWHEEL, step),
        _ => (WHEEL, -step),
    };
    let script = format!(
        "{}\n[W.Inp]::mouse_event({flag}, 0, 0, {delta}, 0)\n",
        WIN_INPUT_PREAMBLE
    );
    run_powershell(&script)
}

#[cfg(target_os = "windows")]
fn foreground_app() -> Result<ForegroundApp, String> {
    let script = r#"
Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public class Fg {
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
}
"@
$hwnd = [Fg]::GetForegroundWindow()
$sb = New-Object System.Text.StringBuilder 512
[void][Fg]::GetWindowText($hwnd, $sb, $sb.Capacity)
$title = $sb.ToString()
if (-not [string]::IsNullOrWhiteSpace($title)) { $title; exit }
# Desktop / shell windows have no title; fall back to the process name so the
# allowlist guard still has something stable to match (e.g. "explorer").
$procId = 0
[void][Fg]::GetWindowThreadProcessId($hwnd, [ref]$procId)
$proc = Get-Process -Id $procId -ErrorAction SilentlyContinue
if ($proc) { $proc.ProcessName } else { '' }
"#;
    let name = powershell_stdout(script)?;
    if name.is_empty() {
        return Err("could not read foreground window title".into());
    }
    Ok(ForegroundApp { name })
}

/// Map a key spec like "ctrl+shift+s" or "win+r" to ordered virtual-key codes
/// (modifiers first, main key last). The caller presses them down in order and
/// releases in reverse.
#[cfg(target_os = "windows")]
fn windows_key_vks(key: &str) -> Result<Vec<u8>, String> {
    let mut modifiers: Vec<u8> = Vec::new();
    let mut name = String::new();
    for part in key.split(['+', '-', ' ']) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers.push(0x11),
            "alt" | "option" | "opt" => modifiers.push(0x12),
            "shift" => modifiers.push(0x10),
            "cmd" | "command" | "meta" | "win" | "windows" => modifiers.push(0x5B),
            other => name = other.to_string(),
        }
    }
    let main: u8 = match name.as_str() {
        "" if !modifiers.is_empty() => {
            // Modifier-only press (e.g. "win" opens the Start menu): drop the
            // last modifier into the main slot so it still gets tapped.
            modifiers.pop().expect("non-empty")
        }
        "" => return Err("missing key".into()),
        "enter" | "return" => 0x0D,
        "tab" => 0x09,
        "esc" | "escape" => 0x1B,
        "backspace" => 0x08,
        "delete" => 0x2E,
        "space" | "spacebar" => 0x20,
        "up" => 0x26,
        "down" => 0x28,
        "left" => 0x25,
        "right" => 0x27,
        "home" => 0x24,
        "end" => 0x23,
        "pageup" => 0x21,
        "pagedown" => 0x22,
        other if other.len() == 1 => {
            let ch = other.chars().next().unwrap().to_ascii_uppercase();
            match ch {
                'A'..='Z' | '0'..='9' => ch as u8,
                _ => return Err(format!("unsupported key '{key}'")),
            }
        }
        other if other.len() <= 3 => {
            let digits = other
                .strip_prefix('f')
                .and_then(|rest| rest.parse::<u8>().ok());
            match digits {
                Some(n @ 1..=12) => 0x70 + (n - 1),
                _ => return Err(format!("unsupported key '{key}'")),
            }
        }
        _ => return Err(format!("unsupported key '{key}'")),
    };
    modifiers.dedup();
    modifiers.push(main);
    Ok(modifiers)
}

#[cfg(target_os = "windows")]
fn run_powershell(script: &str) -> Result<(), String> {
    powershell_stdout(script).map(|_| ())
}

#[cfg(target_os = "windows")]
fn powershell_stdout(script: &str) -> Result<String, String> {
    use std::os::windows::process::CommandExt;
    // CREATE_NO_WINDOW: a visible console would briefly steal foreground focus
    // and corrupt the very foreground-app detection these scripts feed.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("powershell: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "Windows 桌面控制失败：{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Launch an app or open a file on Windows. Resolution order: direct path
/// (absolute, or relative to the workspace) → desktop shortcuts → pinned
/// taskbar shortcuts → Start Menu shortcuts → registered Start apps
/// (covers UWP) → executables on PATH.
#[cfg(target_os = "windows")]
fn launch(target: &str, workspace: &Path) -> Result<String, String> {
    let b64 = |s: &str| {
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, s.as_bytes())
    };
    let ws = workspace.to_string_lossy().to_string();
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$target = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String('{target_b64}'))
$workspace = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String('{ws_b64}'))
$needle = [System.Management.Automation.WildcardPattern]::Escape($target)

# Apps launched from a background process do not always take foreground.
# Actively focus the new window: by PID when we have one, else by title.
function Focus-App($procId, $name) {{
  $wsh = New-Object -ComObject WScript.Shell
  for ($i = 0; $i -lt 16; $i++) {{
    Start-Sleep -Milliseconds 500
    try {{ if ($procId -and $wsh.AppActivate($procId)) {{ return }} }} catch {{}}
    try {{ if ($name -and $wsh.AppActivate($name)) {{ return }} }} catch {{}}
  }}
}}

function Start-Target($path) {{
  $baseName = [System.IO.Path]::GetFileNameWithoutExtension($path)
  $proc = Start-Process -FilePath $path -PassThru
  Focus-App $proc.Id $baseName
  Write-Output "started: $path"
  exit 0
}}

# 1. Direct path: absolute, workspace-relative, or desktop file name.
$pathCandidates = @()
if ($target -match '[\\/]' -or $target -match '^[A-Za-z]:' -or $target -match '\.[A-Za-z0-9]{{1,6}}$') {{
  $pathCandidates += $target
  if ($workspace) {{ $pathCandidates += (Join-Path $workspace $target) }}
  $pathCandidates += (Join-Path ([Environment]::GetFolderPath('Desktop')) $target)
  $pathCandidates += (Join-Path ([Environment]::GetFolderPath('CommonDesktopDirectory')) $target)
}}
foreach ($candidate in $pathCandidates) {{
  if (Test-Path -LiteralPath $candidate) {{ Start-Target (Resolve-Path -LiteralPath $candidate).Path }}
}}

# 2. Shortcut / exe search by name: desktop, taskbar pins, start menu.
$dirs = @(
  [Environment]::GetFolderPath('Desktop'),
  [Environment]::GetFolderPath('CommonDesktopDirectory'),
  (Join-Path $env:APPDATA 'Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar'),
  (Join-Path ([Environment]::GetFolderPath('StartMenu')) 'Programs'),
  (Join-Path ([Environment]::GetFolderPath('CommonStartMenu')) 'Programs')
)
$found = @()
foreach ($dir in $dirs) {{
  if (Test-Path -LiteralPath $dir) {{
    $found += Get-ChildItem -LiteralPath $dir -Recurse -Include *.lnk,*.url,*.exe -ErrorAction SilentlyContinue |
      Where-Object {{ $_.BaseName -like "*$needle*" }}
  }}
}}
if ($found.Count -gt 0) {{
  $exact = $found | Where-Object {{ $_.BaseName -ieq $target }} | Select-Object -First 1
  $best = if ($exact) {{ $exact }} else {{ $found | Sort-Object {{ $_.BaseName.Length }} | Select-Object -First 1 }}
  Start-Target $best.FullName
}}

# 3. Registered Start apps (covers UWP / store apps) via the
# IApplicationActivationManager COM API. explorer.exe shell:AppsFolder opens
# a stray Documents window on some builds, and Start-Process cannot resolve
# the shell: protocol — the activation manager is the supported API and
# returns the new process id for foregrounding.
# Exact name first: a substring match on "计算器" would otherwise launch
# "计算机管理" (Computer Management) — same prefix, very different tool.
$apps = @(Get-StartApps | Where-Object {{ $_.Name -like "*$needle*" }})
$app = $apps | Where-Object {{ $_.Name -ieq $target }} | Select-Object -First 1
if (-not $app) {{ $app = $apps | Sort-Object {{ $_.Name.Length }} | Select-Object -First 1 }}
if ($app) {{
  $uwpSrc = @'
using System;
using System.Runtime.InteropServices;
public static class UwpLauncher {{
    [ComImport, Guid("2e941141-7f97-4756-ba1d-9decde894a3d"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    private interface IApplicationActivationManager {{
        int ActivateApplication(string appUserModelId, string arguments, uint options, out uint processId);
    }}
    public static uint Launch(string appId) {{
        var type = Type.GetTypeFromCLSID(new Guid("45ba127d-10a8-46ea-8ab7-56ea9078943c"));
        var mgr = (IApplicationActivationManager)Activator.CreateInstance(type);
        uint pid;
        int hr = mgr.ActivateApplication(appId, null, 0, out pid);
        if (hr != 0) throw new System.ComponentModel.Win32Exception(hr);
        return pid;
    }}
}}
'@
  Add-Type -TypeDefinition $uwpSrc
  $uwpPid = [UwpLauncher]::Launch($app.AppID)
  Focus-App $uwpPid $app.Name
  Write-Output "started: $($app.Name) (pid $uwpPid)"
  exit 0
}}

# 4. Executable on PATH.
$exeName = $target
if ($exeName -notmatch '\.exe$') {{ $exeName = "$exeName.exe" }}
$cmd = Get-Command $exeName -ErrorAction SilentlyContinue
if ($cmd) {{ Start-Target $cmd.Source }}

Write-Error "找不到应用或文件: $target"
exit 1
"#,
        target_b64 = b64(target),
        ws_b64 = b64(&ws)
    );
    powershell_stdout(&script)
}

/// Invoke the control at a screen point via UI Automation (InvokePattern,
/// walking up to ancestors; LegacyIAccessible as last resort). Avoids the
/// physical mouse entirely, so occluded or hard-to-hit controls still work.
#[cfg(target_os = "windows")]
fn invoke_at(x: i32, y: i32) -> Result<(), String> {
    let script = format!(
        r#"
Add-Type -AssemblyName UIAutomationClient,WindowsBase
function Try-Activate($el) {{
  # Button/menu item: Invoke. Tab/list item: Select. Checkbox: Toggle.
  # Combo box / collapsible group: Expand or Collapse.
  try {{
    $p = $el.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
    if ($p) {{ $p.Invoke(); return $true }}
  }} catch {{}}
  try {{
    $p = $el.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern)
    if ($p) {{ $p.Select(); return $true }}
  }} catch {{}}
  try {{
    $p = $el.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern)
    if ($p) {{ $p.Toggle(); return $true }}
  }} catch {{}}
  try {{
    $p = $el.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern)
    if ($p) {{
      if ($p.Current.ExpandCollapseState -eq 'Collapsed') {{ $p.Expand() }} else {{ $p.Collapse() }}
      return $true
    }}
  }} catch {{}}
  return $false
}}
$pt = New-Object System.Windows.Point({x}, {y})
try {{
  $el = [System.Windows.Automation.AutomationElement]::FromPoint($pt)
}} catch {{
  Write-Error "FromPoint failed: $_"
  exit 1
}}
if (-not $el) {{ Write-Error 'no element at point'; exit 1 }}
$cur = $el
for ($i = 0; $i -lt 6 -and $cur; $i++) {{
  if (Try-Activate $cur) {{ Write-Output 'invoked'; exit 0 }}
  try {{
    $cur = [System.Windows.Automation.TreeWalker]::RawViewWalker.GetParent($cur)
  }} catch {{ $cur = $null }}
}}
try {{
  $legacy = $el.GetCurrentPattern([System.Windows.Automation.LegacyIAccessiblePattern]::Pattern)
  if ($legacy) {{ $legacy.DoDefaultAction(); Write-Output 'legacy-invoked'; exit 0 }}
}} catch {{}}
Write-Error 'no invokable pattern at point'
exit 1
"#
    );
    powershell_stdout(&script).map(|_| ())
}

#[cfg(target_os = "macos")]
fn launch(target: &str, workspace: &Path) -> Result<String, String> {
    let path = Path::new(target);
    let workspace_path = workspace.join(target);
    if path.is_absolute() && path.exists() {
        Command::new("open")
            .arg(path)
            .status()
            .map_err(|e| format!("open: {e}"))?;
        return Ok(format!("started: {target}"));
    }
    if workspace_path.exists() {
        Command::new("open")
            .arg(&workspace_path)
            .status()
            .map_err(|e| format!("open: {e}"))?;
        return Ok(format!("started: {}", workspace_path.display()));
    }
    let status = Command::new("open")
        .args(["-a", target])
        .status()
        .map_err(|e| format!("open -a: {e}"))?;
    if status.success() {
        Ok(format!("started app: {target}"))
    } else {
        Err(format!("找不到应用或文件: {target}"))
    }
}

#[cfg(target_os = "linux")]
fn launch(target: &str, workspace: &Path) -> Result<String, String> {
    let path = Path::new(target);
    let workspace_path = workspace.join(target);
    let file = if path.is_absolute() && path.exists() {
        Some(path.to_path_buf())
    } else if workspace_path.exists() {
        Some(workspace_path)
    } else {
        None
    };
    if let Some(file) = file {
        Command::new("xdg-open")
            .arg(&file)
            .status()
            .map_err(|e| format!("xdg-open: {e}"))?;
        return Ok(format!("started: {}", file.display()));
    }
    if Command::new("gtk-launch")
        .arg(target)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return Ok(format!("started app: {target}"));
    }
    if Command::new(target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .is_ok()
    {
        return Ok(format!("started: {target}"));
    }
    Err(format!("找不到应用或文件: {target}"))
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn launch(_target: &str, _workspace: &Path) -> Result<String, String> {
    Err("unsupported platform for computer_use".into())
}

#[cfg(not(target_os = "windows"))]
fn invoke_at(_x: i32, _y: i32) -> Result<(), String> {
    Err("invoke is only implemented on Windows".into())
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn click(_x: i32, _y: i32, _button: &str) -> Result<(), String> {
    Err("unsupported platform for computer_use".into())
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn paste_text(_text: &str) -> Result<(), String> {
    Err("unsupported platform for computer_use".into())
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn press(_key: &str) -> Result<(), String> {
    Err("unsupported platform for computer_use".into())
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn scroll(_direction: &str, _amount: i32) -> Result<(), String> {
    Err("unsupported platform for computer_use".into())
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn foreground_app() -> Result<ForegroundApp, String> {
    Err("unsupported platform for computer_use".into())
}

#[cfg(target_os = "macos")]
fn snapshot_front_window(max_nodes: usize) -> Result<Vec<A11yNode>, String> {
    let cap = max_nodes.max(1).saturating_mul(8).min(400);
    let script = format!(
        r#"with timeout of 8 seconds
tell application "System Events"
  tell (first application process whose frontmost is true)
    set output to ""
    set i to 0
    set elems to entire contents of window 1
    repeat with e in elems
      if i ≥ {cap} then exit repeat
      try
        set n to name of e
        if n is missing value then set n to ""
        set r to role of e
        if r is missing value then set r to ""
        set p to position of e
        set s to size of e
        set en to enabled of e
        set v to ""
        try
          set v to value of e
          if v is missing value then set v to ""
        end try
        set output to output & n & (character id 31) & r & (character id 31) & (item 1 of p) & (character id 31) & (item 2 of p) & (character id 31) & (item 1 of s) & (character id 31) & (item 2 of s) & (character id 31) & en & (character id 31) & v & linefeed
        set i to i + 1
      end try
    end repeat
  end tell
end tell
end timeout
return output
"#
    );
    let raw = run_osascript_stdin(&script)?;
    Ok(super::a11y::parse_tsv_nodes(&raw))
}

#[cfg(target_os = "macos")]
fn run_osascript_stdin(script: &str) -> Result<String, String> {
    let mut child = Command::new("osascript")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("osascript: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(script.as_bytes())
            .map_err(|e| format!("osascript write: {e}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("osascript wait: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "无障碍快照失败。macOS 需在 系统设置 → 隐私与安全性 → 辅助功能 中授权 OmniNova。{stderr}"
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(target_os = "linux")]
fn snapshot_front_window(max_nodes: usize) -> Result<Vec<A11yNode>, String> {
    let script = format!(
        r#"
import json, os, sys
max_nodes = {max_nodes}
out = []
try:
    import gi
    gi.require_version('Atspi', '2.0')
    from gi.repository import Atspi
except Exception as e:
    print(json.dumps({{"error": "Linux AT-SPI unavailable: %s. Install at-spi2-core / python3-gi." % e}}))
    sys.exit(0)

def role_name(acc):
    try:
        return acc.get_role_name() or ""
    except Exception:
        return ""

def walk(acc, depth):
    if len(out) >= max_nodes or depth > 8:
        return
    try:
        name = acc.get_name() or ""
        role = role_name(acc)
        try:
            x, y, w, h = acc.get_extents(Atspi.CoordType.SCREEN)
        except Exception:
            x = y = w = h = 0
        try:
            st = acc.get_state_set()
            enabled = bool(st.contains(Atspi.StateType.ENABLED)) if st is not None else True
        except Exception:
            enabled = True
        if w > 0 and h > 0:
            out.append({{"name": name, "role": role, "x": int(x), "y": int(y), "width": int(w), "height": int(h), "enabled": bool(enabled), "id": ""}})
        n = acc.get_child_count()
        for i in range(n):
            if len(out) >= max_nodes:
                break
            child = acc.get_child_at_index(i)
            if child is not None:
                walk(child, depth + 1)
    except Exception:
        return

desktop = Atspi.get_desktop(0)
for i in range(desktop.get_child_count()):
    app = desktop.get_child_at_index(i)
    if app is not None:
        walk(app, 0)
    if len(out) >= max_nodes:
        break
print(json.dumps(out, ensure_ascii=False))
"#
    );
    let output = Command::new("python3")
        .args(["-c", &script])
        .output()
        .map_err(|e| {
            format!("python3 不可用，无法读取 AT-SPI：{e}。可改用 screenshot + 坐标点击。")
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() {
        return Err(format!(
            "Linux 无障碍快照失败：{} {}",
            stdout,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    super::a11y::parse_json_nodes(stdout.trim())
}

#[cfg(target_os = "windows")]
fn snapshot_front_window(max_nodes: usize) -> Result<Vec<A11yNode>, String> {
    // Collect extra raw nodes because finalize_nodes drops noise roles (pane,
    // group, ...); capping the raw walk at exactly max_nodes left Word/Excel
    // windows with almost nothing usable.
    let raw_cap = max_nodes.max(1).saturating_mul(8).clamp(64, 600);
    let script = format!(
        r#"
Add-Type -AssemblyName UIAutomationClient
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class FgHwnd {{
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
}}
"@
$hwnd = [FgHwnd]::GetForegroundWindow()
if ($hwnd -eq [IntPtr]::Zero) {{ '[]'; exit }}
try {{
  $root = [System.Windows.Automation.AutomationElement]::FromHandle($hwnd)
}} catch {{
  Write-Error "UIA FromHandle failed: $_"
  exit 1
}}
if (-not $root) {{ '[]'; exit }}
$items = New-Object System.Collections.Generic.List[object]
$walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker
# Bounded breadth-first walk: FindAll(Descendants) on a full Office document
# tree enumerates thousands of elements and can stall for minutes.
$queue = New-Object System.Collections.Queue
$queue.Enqueue(@($root, 0))
while ($queue.Count -gt 0 -and $items.Count -lt {raw_cap}) {{
  $pair = $queue.Dequeue()
  $el = $pair[0]
  $depth = [int]$pair[1]
  try {{
    $rect = $el.Current.BoundingRectangle
    if ($rect.Width -gt 0 -and $rect.Height -gt 0) {{
      $items.Add(@{{
        id = ''
        role = $el.Current.ControlType.ProgrammaticName
        name = $el.Current.Name
        x = [int]$rect.X
        y = [int]$rect.Y
        width = [int]$rect.Width
        height = [int]$rect.Height
        enabled = [bool]$el.Current.IsEnabled
      }})
    }}
  }} catch {{}}
  if ($depth -ge 8) {{ continue }}
  try {{
    $child = $walker.GetFirstChild($el)
    while ($child) {{
      $queue.Enqueue(@($child, $depth + 1))
      $child = $walker.GetNextSibling($child)
    }}
  }} catch {{}}
}}
ConvertTo-Json -InputObject @($items.ToArray()) -Compress -Depth 5
"#
    );
    let raw = powershell_stdout(&script)?;
    super::a11y::parse_json_nodes(&raw)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn snapshot_front_window(_max_nodes: usize) -> Result<Vec<A11yNode>, String> {
    Err("unsupported platform for computer_use".into())
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::windows_key_vks;

    #[test]
    fn key_specs_map_to_virtual_key_sequences() {
        assert_eq!(windows_key_vks("enter").unwrap(), vec![0x0D]);
        assert_eq!(windows_key_vks("ctrl+v").unwrap(), vec![0x11, 0x56]);
        assert_eq!(
            windows_key_vks("ctrl+shift+s").unwrap(),
            vec![0x11, 0x10, 0x53]
        );
        assert_eq!(windows_key_vks("win+r").unwrap(), vec![0x5B, 0x52]);
        // Modifier-only taps (win opens the Start menu) still emit one key.
        assert_eq!(windows_key_vks("win").unwrap(), vec![0x5B]);
        assert_eq!(windows_key_vks("f5").unwrap(), vec![0x74]);
        assert_eq!(windows_key_vks("cmd+c").unwrap(), vec![0x5B, 0x43]);
        assert!(windows_key_vks("").is_err());
        assert!(windows_key_vks("notakey").is_err());
    }
}
