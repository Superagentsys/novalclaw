use super::{A11yNode, DesktopDriver, ForegroundApp};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

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

#[cfg(target_os = "windows")]
fn paste_text(text: &str) -> Result<(), String> {
    let escaped = text.replace('\'', "''");
    let script = format!(
        r#"
Set-Clipboard -Value @'
{escaped}
'@
Add-Type -AssemblyName System.Windows.Forms
[System.Windows.Forms.SendKeys]::SendWait('^v')
"#
    );
    run_powershell(&script)
}

#[cfg(target_os = "windows")]
fn press(key: &str) -> Result<(), String> {
    let send = windows_sendkeys(key)?;
    let script = format!(
        r#"
Add-Type -AssemblyName System.Windows.Forms
[System.Windows.Forms.SendKeys]::SendWait('{send}')
"#
    );
    run_powershell(&script)
}

#[cfg(target_os = "windows")]
fn scroll(direction: &str, amount: i32) -> Result<(), String> {
    let key = match direction {
        "up" => "{{UP}}",
        "left" => "{{LEFT}}",
        "right" => "{{RIGHT}}",
        _ => "{{DOWN}}",
    };
    let repeated = key.repeat(amount.clamp(1, 30) as usize);
    let script = format!(
        r#"
Add-Type -AssemblyName System.Windows.Forms
[System.Windows.Forms.SendKeys]::SendWait('{repeated}')
"#
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
}
"@
$sb = New-Object System.Text.StringBuilder 512
[void][Fg]::GetWindowText([Fg]::GetForegroundWindow(), $sb, $sb.Capacity)
$sb.ToString()
"#;
    let name = powershell_stdout(script)?;
    if name.is_empty() {
        return Err("could not read foreground window title".into());
    }
    Ok(ForegroundApp { name })
}

#[cfg(target_os = "windows")]
fn windows_sendkeys(key: &str) -> Result<String, String> {
    let mut mods = String::new();
    // Owned: the lowercased part is a temporary that dies at the end of each
    // match, so the key name cannot be kept as a borrow of it.
    let mut name = String::new();
    for part in key.split(['+', '-', ' ']) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods.push('^'),
            "alt" | "option" => mods.push('%'),
            "shift" => mods.push('+'),
            "cmd" | "command" | "win" => {
                return Err("Windows computer_use 不支持 Win 组合键".into())
            }
            other => name = other.to_string(),
        }
    }
    let token = match name.as_str() {
        "enter" | "return" => "{ENTER}",
        "tab" => "{TAB}",
        "esc" | "escape" => "{ESC}",
        "backspace" | "delete" => "{BACKSPACE}",
        "space" | "spacebar" => " ",
        "up" => "{UP}",
        "down" => "{DOWN}",
        "left" => "{LEFT}",
        "right" => "{RIGHT}",
        "home" => "{HOME}",
        "end" => "{END}",
        "pageup" => "{PGUP}",
        "pagedown" => "{PGDN}",
        other if other.len() == 1 => other,
        _ => return Err(format!("unsupported key '{key}'")),
    };
    Ok(format!("{mods}{token}"))
}

#[cfg(target_os = "windows")]
fn run_powershell(script: &str) -> Result<(), String> {
    powershell_stdout(script).map(|_| ())
}

#[cfg(target_os = "windows")]
fn powershell_stdout(script: &str) -> Result<String, String> {
    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", script])
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
$root = [System.Windows.Automation.AutomationElement]::FromHandle($hwnd)
if (-not $root) {{ '[]'; exit }}
$els = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
$items = New-Object System.Collections.Generic.List[object]
foreach ($el in $els) {{
  if ($items.Count -ge {max_nodes}) {{ break }}
  $ct = $el.Current.ControlType.ProgrammaticName
  $name = $el.Current.Name
  $rect = $el.Current.BoundingRectangle
  if ($rect.Width -le 0 -or $rect.Height -le 0) {{ continue }}
  $items.Add(@{{
    id = ''
    role = $ct
    name = $name
    x = [int]$rect.X
    y = [int]$rect.Y
    width = [int]$rect.Width
    height = [int]$rect.Height
    enabled = [bool]$el.Current.IsEnabled
  }})
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
