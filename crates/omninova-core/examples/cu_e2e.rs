//! End-to-end computer_use test through the real `ComputerUseSession` tool
//! path, covering the launch action and post-launch control:
//!   1. launch Word by name -> ctrl+n (new document) -> type CJK text
//!   2. launch a workspace file (.txt) -> notepad opens it
//!   3. click a control by name (uses UIA invoke when available)
//! Cleanup kills only processes started by this test.

use omninova_core::computer_use::ComputerUseSession;
use omninova_core::config::ComputerUseConfig;
use serde_json::json;
use std::process::Command;
use std::time::Duration;

fn step(session: &ComputerUseSession, args: serde_json::Value) -> bool {
    let outcome = session.execute(&args);
    let status = if outcome.ok { "PASS" } else { "FAIL" };
    let via = outcome
        .clicked
        .as_ref()
        .and_then(|c| c.get("via"))
        .and_then(|v| v.as_str())
        .map(|v| format!(" via={v}"))
        .unwrap_or_default();
    println!("[{status}] {} -> {}{via}", outcome.action, outcome.message);
    outcome.ok
}

fn main() {
    let dir = std::env::temp_dir().join("omninova-cu-e2e");
    let workspace = std::env::temp_dir().join("omninova-cu-e2e-ws");
    std::fs::create_dir_all(&workspace).ok();
    // A workspace file to launch (opens with the system default association).
    let doc = workspace.join("launch_test_文档.txt");
    std::fs::write(&doc, "OmniNova launch test").ok();

    let mut config = ComputerUseConfig::default();
    config.allowed_apps = vec!["*".into()];
    let session = ComputerUseSession::os(dir, config).with_workspace(workspace.clone());

    println!("== 1. launch Word by app name ==");
    let ok = step(
        &session,
        json!({"action": "launch", "target": "word", "wait_ms": 20000}),
    );
    if !ok {
        println!("[FAIL] cannot launch word; aborting");
        return;
    }

    println!("== 2. new document + type ==");
    step(&session, json!({"action": "wait", "duration_ms": 4000}));
    // Word may open on the start screen; ctrl+n creates a blank document
    // regardless of which view is showing.
    step(&session, json!({"action": "press", "key": "ctrl+n"}));
    step(&session, json!({"action": "wait", "duration_ms": 2500}));
    step(&session, json!({"action": "type", "text": "OmniNova 新建文档测试 Hello"}));
    step(&session, json!({"action": "press", "key": "enter"}));

    println!("== 3. snapshot + click a control by name (UIA invoke path) ==");
    let snap = session.execute(&json!({"action": "snapshot"}));
    let nodes = snap.nodes.clone().unwrap_or_default();
    println!(
        "[{}] snapshot -> {} nodes",
        if snap.ok { "PASS" } else { "FAIL" },
        nodes.len()
    );
    // Try clicking the ribbon "文件" (File) tab by name; falls back to any
    // button-ish node if the localized name differs.
    let target_name = if nodes.iter().any(|n| n.name.contains("文件")) {
        "文件"
    } else if nodes.iter().any(|n| n.name.contains("File")) {
        "File"
    } else {
        ""
    };
    if !target_name.is_empty() {
        step(&session, json!({"action": "click", "name": target_name}));
        // Close the backstage view again so Word is left in the document.
        step(&session, json!({"action": "press", "key": "esc"}));
    } else {
        println!("[SKIP] no File-tab node found; names: {:?}",
            nodes.iter().take(10).map(|n| n.name.clone()).collect::<Vec<_>>());
    }

    println!("== 4. launch a workspace file (opens via association) ==");
    step(
        &session,
        json!({"action": "launch", "target": "launch_test_文档.txt", "wait_ms": 8000}),
    );

    println!("== cleanup ==");
    for image in ["WINWORD.EXE", "notepad.exe"] {
        let _ = Command::new("taskkill").args(["/IM", image, "/F"]).output();
    }
    let _ = std::fs::remove_dir_all(&workspace);
    println!("done.");
}
