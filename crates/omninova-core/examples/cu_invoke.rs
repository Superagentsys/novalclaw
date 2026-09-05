//! Focused test for the UIA invoke path: open Calculator, find a digit
//! button via snapshot, invoke it by coordinates, and verify the display
//! changes without any physical mouse click.

use omninova_core::computer_use::{ComputerUseSession, DesktopDriver, OsDesktopDriver};
use omninova_core::config::ComputerUseConfig;
use serde_json::json;
use std::process::Command;
use std::time::Duration;

fn main() {
    let dir = std::env::temp_dir().join("omninova-cu-invoke");
    let mut config = ComputerUseConfig::default();
    config.allowed_apps = vec!["*".into()];
    let session = ComputerUseSession::os(dir, config);

    let r = session.execute(&json!({"action": "launch", "target": "计算器", "wait_ms": 8000}));
    println!("launch calc: {} {}", r.ok, r.message);
    std::thread::sleep(Duration::from_millis(1500));

    let snap = session.execute(&json!({"action": "snapshot"}));
    let nodes = snap.nodes.clone().unwrap_or_default();
    println!("snapshot: {} nodes", nodes.len());
    for n in nodes.iter().take(20) {
        println!("  {} {} {} ({},{})", n.id, n.role, n.name, n.x, n.y);
    }

    // Find the "5" button (name may be localized, e.g. "五" or "5").
    let target = nodes
        .iter()
        .find(|n| n.name == "5" || n.name == "五" || n.name.contains('5'))
        .cloned();
    let Some(node) = target else {
        println!("[FAIL] no digit-5 node found");
        for image in ["CalculatorApp.exe", "Calculator.exe"] {
            let _ = Command::new("taskkill").args(["/IM", image, "/F"]).output();
        }
        return;
    };
    let (x, y) = node.center();
    println!("invoking '{}' at ({x},{y})", node.name);

    let driver = OsDesktopDriver;
    match driver.invoke_at(x, y) {
        Ok(()) => println!("[PASS] invoke_at succeeded (no physical click)"),
        Err(e) => println!("[FAIL] invoke_at: {e}"),
    }

    // Verify via a fresh snapshot that the display now shows 5.
    std::thread::sleep(Duration::from_millis(600));
    let after = session.execute(&json!({"action": "snapshot"}));
    if let Some(nodes) = after.nodes.as_ref() {
        let names: Vec<String> = nodes
            .iter()
            .filter(|n| n.role.to_ascii_lowercase().contains("text") || n.name.contains('5'))
            .map(|n| format!("{}:{}", n.role, n.name))
            .collect();
        println!("post-invoke text/5 nodes: {names:?}");
        let display_changed = nodes
            .iter()
            .any(|n| n.name.trim() == "5" || n.name.contains("显示为 5") || n.name.contains("5 is"));
        println!("display shows 5: {display_changed}");
    }

    for image in ["CalculatorApp.exe", "Calculator.exe"] {
        let _ = Command::new("taskkill").args(["/IM", image, "/F"]).output();
    }
    println!("done.");
}
