//! Computer-use driver diagnostics for the local desktop.
//!
//! Exercises every `DesktopDriver` capability against the real OS and prints
//! PASS/FAIL per step so Windows regressions are visible without the full app.
//! Mutating steps are contained inside a Notepad instance launched (and
//! killed) by this script.

use omninova_core::computer_use::{DesktopDriver, OsDesktopDriver};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

fn report(name: &str, result: &Result<String, String>) {
    match result {
        Ok(detail) => println!("[PASS] {name}: {detail}"),
        Err(error) => println!("[FAIL] {name}: {error}"),
    }
}

fn cursor_pos() -> Result<(i32, i32), String> {
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "Add-Type -AssemblyName System.Windows.Forms; $p = [System.Windows.Forms.Cursor]::Position; Write-Output \"$($p.X),$($p.Y)\"",
        ])
        .output()
        .map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let mut parts = text.split(',');
    let x = parts.next().and_then(|v| v.parse().ok());
    let y = parts.next().and_then(|v| v.parse().ok());
    match (x, y) {
        (Some(x), Some(y)) => Ok((x, y)),
        _ => Err(format!("cannot parse cursor pos from '{text}'")),
    }
}

fn main() {
    let driver = OsDesktopDriver;
    let dir = std::env::temp_dir().join("omninova-cu-diag");
    std::fs::create_dir_all(&dir).ok();
    let shot: PathBuf = dir.join("diag.png");

    println!("== 1. observe capabilities (current foreground) ==");
    report(
        "foreground_app",
        &driver.foreground_app().map(|app| app.name),
    );
    report(
        "capture_png",
        &driver
            .capture_png(&shot)
            .map(|(w, h)| format!("{w}x{h} -> {}", shot.display())),
    );
    report(
        "accessibility_snapshot",
        &driver.accessibility_snapshot(30).map(|nodes| {
            let preview: Vec<String> = nodes
                .iter()
                .take(5)
                .map(|n| format!("{}:{}@({},{})", n.role, n.name, n.x, n.y))
                .collect();
            format!("{} nodes; first: {:?}", nodes.len(), preview)
        }),
    );

    println!("\n== 2. launch notepad as controlled target ==");
    let mut child = match Command::new("notepad.exe").spawn() {
        Ok(child) => child,
        Err(e) => {
            println!("[FAIL] launch notepad: {e}");
            return;
        }
    };
    std::thread::sleep(Duration::from_millis(2500));

    report(
        "foreground_app(notepad)",
        &driver.foreground_app().map(|app| app.name),
    );
    let nodes = driver.accessibility_snapshot(60);
    report(
        "accessibility_snapshot(notepad)",
        &nodes.as_ref().map(|n| format!("{} nodes", n.len())).map_err(|e| e.clone()),
    );

    println!("\n== 3. mutating actions inside notepad ==");
    report(
        "paste_text(CJK+quotes)",
        &driver
            .paste_text("OmniNova 桌面控制测试 it's \"quoted\" 123")
            .map(|_| "ok".to_string()),
    );
    std::thread::sleep(Duration::from_millis(400));
    report("press(enter)", &driver.press("enter").map(|_| "ok".to_string()));
    std::thread::sleep(Duration::from_millis(300));

    if let Ok(nodes) = &nodes {
        if let Some(first) = nodes.first() {
            let (cx, cy) = first.center();
            let before = cursor_pos().ok();
            let click_result = driver.click(cx, cy, "left");
            std::thread::sleep(Duration::from_millis(300));
            let after = cursor_pos().ok();
            report(
                "click(node center)",
                &click_result.map(|_| format!("target=({cx},{cy}) cursor {before:?} -> {after:?}")),
            );
        }
    }

    report(
        "scroll(down)",
        &driver.scroll("down", 3).map(|_| "ok".to_string()),
    );

    println!("\n== 4. win-key support (needed to open apps like Word/Excel) ==");
    report(
        "press(win+r)",
        &driver.press("win+r").map(|_| "ok".to_string()),
    );
    std::thread::sleep(Duration::from_millis(1200));
    // The Run dialog should now be foreground; dismiss it without launching.
    report(
        "foreground_app(run dialog?)",
        &driver.foreground_app().map(|app| app.name),
    );
    let _ = driver.press("esc");

    println!("\n== 5. cleanup ==");
    let _ = child.kill();
    let _ = Command::new("taskkill")
        .args(["/IM", "notepad.exe", "/F"])
        .output();
    println!("done.");
}
