use super::{A11yNode, DesktopDriver, ForegroundApp};
#[cfg(target_os = "windows")]
use super::{foreground_display_name, is_desktop_input_helper};
#[cfg(not(target_os = "windows"))]
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
    let primary = match capture_via_screenshots_crate(dest) {
        Ok(dims) => return Ok(dims),
        Err(error) => error,
    };
    // Keep the primary error: on Windows the fallback is a stub, so dropping
    // it would report "fallback unavailable" and hide the real cause.
    match capture_via_os_tool(dest) {
        Ok(()) => png_dimensions(dest),
        Err(fallback) => Err(format!("{fallback}（底层错误：{primary}）")),
    }
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
    // Capture goes through the cross-platform path (DXGI/GDI); there is no
    // second Windows route worth shelling out to. Explain what breaks it.
    Err("Windows 截图失败：需要有交互式桌面会话。以服务/计划任务方式运行、\
锁屏或 RDP 会话断开时都拿不到画面。".into())
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

/// Windows desktop input used to spawn a visible `powershell.exe`, which stole
/// the foreground from Excel and made every screenshot/click/type report the
/// console path as `foreground_app`. Click/type/press now go through user32;
/// PowerShell is only used for UI Automation snapshots, and even then it is
/// created with `CREATE_NO_WINDOW`.
#[cfg(target_os = "windows")]
mod win32 {
    pub const MOUSEEVENTF_LEFTDOWN: u32 = 0x0002;
    pub const MOUSEEVENTF_LEFTUP: u32 = 0x0004;
    pub const MOUSEEVENTF_RIGHTDOWN: u32 = 0x0008;
    pub const MOUSEEVENTF_RIGHTUP: u32 = 0x0010;
    pub const KEYEVENTF_KEYUP: u32 = 0x0002;
    pub const VK_BACK: u8 = 0x08;
    pub const VK_TAB: u8 = 0x09;
    pub const VK_RETURN: u8 = 0x0D;
    pub const VK_SHIFT: u8 = 0x10;
    pub const VK_CONTROL: u8 = 0x11;
    pub const VK_MENU: u8 = 0x12;
    pub const VK_ESCAPE: u8 = 0x1B;
    pub const VK_SPACE: u8 = 0x20;
    pub const VK_PRIOR: u8 = 0x21;
    pub const VK_NEXT: u8 = 0x22;
    pub const VK_END: u8 = 0x23;
    pub const VK_HOME: u8 = 0x24;
    pub const VK_LEFT: u8 = 0x25;
    pub const VK_UP: u8 = 0x26;
    pub const VK_RIGHT: u8 = 0x27;
    pub const VK_DOWN: u8 = 0x28;
    pub const CF_UNICODETEXT: u32 = 13;
    pub const GMEM_MOVEABLE: u32 = 0x0002;
    pub const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    pub const GW_OWNER: u32 = 4;
    pub const GWL_EXSTYLE: i32 = -20;
    pub const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
    pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    #[link(name = "user32")]
    extern "system" {
        pub fn GetForegroundWindow() -> isize;
        pub fn GetWindowTextW(hwnd: isize, buf: *mut u16, max: i32) -> i32;
        pub fn GetClassNameW(hwnd: isize, buf: *mut u16, max: i32) -> i32;
        pub fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
        pub fn SetCursorPos(x: i32, y: i32) -> i32;
        pub fn mouse_event(flags: u32, dx: u32, dy: u32, data: u32, extra: usize);
        pub fn keybd_event(vk: u8, scan: u8, flags: u32, extra: usize);
        pub fn OpenClipboard(owner: isize) -> i32;
        pub fn CloseClipboard() -> i32;
        pub fn EmptyClipboard() -> i32;
        pub fn SetClipboardData(format: u32, mem: isize) -> isize;
        pub fn IsWindowVisible(hwnd: isize) -> i32;
        pub fn GetWindow(hwnd: isize, cmd: u32) -> isize;
        pub fn GetWindowLongW(hwnd: isize, index: i32) -> i32;
        pub fn EnumWindows(
            cb: unsafe extern "system" fn(isize, isize) -> i32,
            lparam: isize,
        ) -> i32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        pub fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
        pub fn QueryFullProcessImageNameW(
            proc: isize,
            flags: u32,
            name: *mut u16,
            size: *mut u32,
        ) -> i32;
        pub fn CloseHandle(handle: isize) -> i32;
        pub fn GlobalAlloc(flags: u32, bytes: usize) -> isize;
        pub fn GlobalLock(mem: isize) -> *mut std::ffi::c_void;
        pub fn GlobalUnlock(mem: isize) -> i32;
        pub fn GlobalFree(mem: isize) -> isize;
    }
}

#[cfg(target_os = "windows")]
struct WindowIdentity {
    hwnd: isize,
    process: String,
    title: String,
    class: String,
}

#[cfg(target_os = "windows")]
fn click(x: i32, y: i32, button: &str) -> Result<(), String> {
    let (down, up) = match button {
        "right" => (win32::MOUSEEVENTF_RIGHTDOWN, win32::MOUSEEVENTF_RIGHTUP),
        _ => (win32::MOUSEEVENTF_LEFTDOWN, win32::MOUSEEVENTF_LEFTUP),
    };
    unsafe {
        if win32::SetCursorPos(x, y) == 0 {
            return Err(format!("SetCursorPos({x},{y}) failed"));
        }
    }
    std::thread::sleep(std::time::Duration::from_millis(15));
    unsafe {
        win32::mouse_event(down, 0, 0, 0, 0);
        win32::mouse_event(up, 0, 0, 0, 0);
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn paste_text(text: &str) -> Result<(), String> {
    set_clipboard_unicode(text)?;
    std::thread::sleep(std::time::Duration::from_millis(30));
    unsafe {
        send_vk(win32::VK_CONTROL, false);
        send_vk(b'V', false);
        send_vk(b'V', true);
        send_vk(win32::VK_CONTROL, true);
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn press(key: &str) -> Result<(), String> {
    let chord = windows_key_chord(key)?;
    unsafe {
        if chord.ctrl {
            send_vk(win32::VK_CONTROL, false);
        }
        if chord.alt {
            send_vk(win32::VK_MENU, false);
        }
        if chord.shift {
            send_vk(win32::VK_SHIFT, false);
        }
        send_vk(chord.vk, false);
        send_vk(chord.vk, true);
        if chord.shift {
            send_vk(win32::VK_SHIFT, true);
        }
        if chord.alt {
            send_vk(win32::VK_MENU, true);
        }
        if chord.ctrl {
            send_vk(win32::VK_CONTROL, true);
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn scroll(direction: &str, amount: i32) -> Result<(), String> {
    let vk = match direction {
        "up" => win32::VK_UP,
        "left" => win32::VK_LEFT,
        "right" => win32::VK_RIGHT,
        _ => win32::VK_DOWN,
    };
    for _ in 0..amount.clamp(1, 30) {
        unsafe {
            send_vk(vk, false);
            send_vk(vk, true);
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn foreground_app() -> Result<ForegroundApp, String> {
    let identity = target_window().ok_or_else(|| "could not read foreground window".to_string())?;
    let name = foreground_display_name(&identity.process, &identity.title);
    if name.is_empty() {
        return Err("could not read foreground window title".into());
    }
    Ok(ForegroundApp { name })
}

#[cfg(target_os = "windows")]
fn target_window() -> Option<WindowIdentity> {
    let hwnd = unsafe { win32::GetForegroundWindow() };
    window_identity(hwnd)
        .filter(is_interactive_app)
        .or_else(first_interactive_window)
}

#[cfg(target_os = "windows")]
struct KeyChord {
    ctrl: bool,
    alt: bool,
    shift: bool,
    vk: u8,
}

#[cfg(target_os = "windows")]
fn windows_key_chord(key: &str) -> Result<KeyChord, String> {
    let mut ctrl = false;
    let mut alt = false;
    let mut shift = false;
    let mut name = String::new();
    for part in key.split(['+', '-', ' ']) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => ctrl = true,
            "alt" | "option" => alt = true,
            "shift" => shift = true,
            "cmd" | "command" | "win" => {
                return Err("Windows computer_use 不支持 Win 组合键".into())
            }
            other => name = other.to_string(),
        }
    }
    let vk = match name.as_str() {
        "enter" | "return" => win32::VK_RETURN,
        "tab" => win32::VK_TAB,
        "esc" | "escape" => win32::VK_ESCAPE,
        "backspace" | "delete" => win32::VK_BACK,
        "space" | "spacebar" => win32::VK_SPACE,
        "up" => win32::VK_UP,
        "down" => win32::VK_DOWN,
        "left" => win32::VK_LEFT,
        "right" => win32::VK_RIGHT,
        "home" => win32::VK_HOME,
        "end" => win32::VK_END,
        "pageup" => win32::VK_PRIOR,
        "pagedown" => win32::VK_NEXT,
        other if other.len() == 1 => {
            let ch = other.chars().next().unwrap();
            if ch.is_ascii_alphabetic() {
                ch.to_ascii_uppercase() as u8
            } else if ch.is_ascii_digit() {
                ch as u8
            } else {
                return Err(format!("unsupported key '{key}'"));
            }
        }
        _ => return Err(format!("unsupported key '{key}'")),
    };
    Ok(KeyChord {
        ctrl,
        alt,
        shift,
        vk,
    })
}

#[cfg(target_os = "windows")]
unsafe fn send_vk(vk: u8, up: bool) {
    let flags = if up { win32::KEYEVENTF_KEYUP } else { 0 };
    win32::keybd_event(vk, 0, flags, 0);
}

#[cfg(target_os = "windows")]
fn set_clipboard_unicode(text: &str) -> Result<(), String> {
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    wide.push(0);
    let bytes = wide.len().saturating_mul(2);
    for _ in 0..8 {
        if unsafe { win32::OpenClipboard(0) } != 0 {
            return copy_wide_to_clipboard(&wide, bytes);
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    Err("OpenClipboard failed".into())
}

#[cfg(target_os = "windows")]
fn copy_wide_to_clipboard(wide: &[u16], bytes: usize) -> Result<(), String> {
    unsafe {
        win32::EmptyClipboard();
        let handle = win32::GlobalAlloc(win32::GMEM_MOVEABLE, bytes);
        if handle == 0 {
            win32::CloseClipboard();
            return Err("GlobalAlloc failed".into());
        }
        let ptr = win32::GlobalLock(handle);
        if ptr.is_null() {
            win32::GlobalFree(handle);
            win32::CloseClipboard();
            return Err("GlobalLock failed".into());
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr() as *const u8, ptr as *mut u8, bytes);
        win32::GlobalUnlock(handle);
        if win32::SetClipboardData(win32::CF_UNICODETEXT, handle) == 0 {
            win32::GlobalFree(handle);
            win32::CloseClipboard();
            return Err("SetClipboardData failed".into());
        }
        win32::CloseClipboard();
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn window_identity(hwnd: isize) -> Option<WindowIdentity> {
    if hwnd == 0 {
        return None;
    }
    unsafe {
        if win32::IsWindowVisible(hwnd) == 0 {
            return None;
        }
        if win32::GetWindow(hwnd, win32::GW_OWNER) != 0 {
            return None;
        }
        let ex = win32::GetWindowLongW(hwnd, win32::GWL_EXSTYLE) as u32;
        if ex & win32::WS_EX_TOOLWINDOW != 0 {
            return None;
        }
    }
    let title = window_text(hwnd);
    let class = window_class(hwnd);
    if is_skipped_window_class(&class) {
        return None;
    }
    let process = window_process_path(hwnd).unwrap_or_default();
    if title.is_empty() && process.is_empty() {
        return None;
    }
    Some(WindowIdentity {
        hwnd,
        process,
        title,
        class,
    })
}

#[cfg(target_os = "windows")]
fn is_interactive_app(identity: &WindowIdentity) -> bool {
    !is_skipped_window_class(&identity.class)
        && !is_desktop_input_helper(&identity.process)
        && !is_desktop_input_helper(&identity.title)
}

#[cfg(target_os = "windows")]
fn is_skipped_window_class(class: &str) -> bool {
    matches!(
        class,
        "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "Progman" | "WorkerW" | "ForegroundStaging"
    )
}

#[cfg(target_os = "windows")]
fn first_interactive_window() -> Option<WindowIdentity> {
    let mut found: Option<WindowIdentity> = None;
    unsafe {
        win32::EnumWindows(enum_interactive, &mut found as *mut _ as isize);
    }
    found
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn enum_interactive(hwnd: isize, lparam: isize) -> i32 {
    let found = &mut *(lparam as *mut Option<WindowIdentity>);
    if let Some(identity) = window_identity(hwnd) {
        if is_interactive_app(&identity) {
            *found = Some(identity);
            return 0;
        }
    }
    1
}

#[cfg(target_os = "windows")]
fn window_text(hwnd: isize) -> String {
    let mut buf = [0u16; 512];
    let len = unsafe { win32::GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    wchar_to_string(&buf, len)
}

#[cfg(target_os = "windows")]
fn window_class(hwnd: isize) -> String {
    let mut buf = [0u16; 256];
    let len = unsafe { win32::GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    wchar_to_string(&buf, len)
}

#[cfg(target_os = "windows")]
fn window_process_path(hwnd: isize) -> Option<String> {
    let mut pid = 0u32;
    unsafe {
        win32::GetWindowThreadProcessId(hwnd, &mut pid);
    }
    if pid == 0 {
        return None;
    }
    let handle = unsafe { win32::OpenProcess(win32::PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle == 0 {
        return None;
    }
    let mut buf = [0u16; 512];
    let mut size = buf.len() as u32;
    let ok = unsafe { win32::QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size) };
    unsafe {
        win32::CloseHandle(handle);
    }
    if ok == 0 {
        return None;
    }
    Some(wchar_to_string(&buf, size as i32))
}

#[cfg(target_os = "windows")]
fn wchar_to_string(buf: &[u16], len: i32) -> String {
    let end = (len.max(0) as usize).min(buf.len());
    let slice = &buf[..end];
    let nul = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
    String::from_utf16_lossy(&slice[..nul])
}

#[cfg(target_os = "windows")]
fn powershell_stdout(script: &str) -> Result<String, String> {
    use std::os::windows::process::CommandExt;
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NoLogo",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-STA",
            "-Command",
            script,
        ])
        .stdin(Stdio::null())
        .creation_flags(win32::CREATE_NO_WINDOW)
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
    let hwnd = target_window().map(|window| window.hwnd as i64).unwrap_or(0);
    let script = format!(
        r#"
Add-Type -AssemblyName UIAutomationClient
$hwnd = New-Object System.IntPtr ([int64]{hwnd})
if ($hwnd -eq [IntPtr]::Zero) {{
  Add-Type @"
using System;
using System.Runtime.InteropServices;
public class FgHwnd {{
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
}}
"@
  $hwnd = [FgHwnd]::GetForegroundWindow()
}}
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
