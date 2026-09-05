//! Broad compatibility test: open a series of common desktop apps through the
//! real `ComputerUseSession` tool path (Win key -> type name -> Enter), verify
//! the window becomes foreground, take an a11y snapshot, do one small safe
//! action, then close only the processes WE started.

use omninova_core::computer_use::ComputerUseSession;
use omninova_core::config::ComputerUseConfig;
use serde_json::json;
use std::collections::HashSet;
use std::process::Command;
use std::time::{Duration, Instant};

struct AppSpec {
    label: &'static str,
    search: &'static str,
    process: &'static str,
    foreground_hints: &'static [&'static str],
    action: Option<(&'static str, serde_json::Value)>,
}

fn pids_of(image: &str) -> HashSet<u32> {
    let output = Command::new("tasklist")
        .args(["/FI", &format!("IMAGENAME eq {image}"), "/FO", "CSV", "/NH"])
        .output();
    let mut set = HashSet::new();
    if let Ok(out) = output {
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let cols: Vec<&str> = line.split(',').collect();
            if cols.len() >= 2 {
                if let Ok(pid) = cols[1].trim_matches('"').parse::<u32>() {
                    set.insert(pid);
                }
            }
        }
    }
    set
}

fn kill_new(image: &str, before: &HashSet<u32>) {
    let now = pids_of(image);
    let mut killed = 0;
    for pid in now.difference(before) {
        let _ = Command::new("taskkill")
            .args(["/F", "/PID", &pid.to_string()])
            .output();
        killed += 1;
    }
    if killed == 0 {
        println!("    (no new {image} process to kill; window may belong to an existing process)");
    }
}

fn run(session: &ComputerUseSession, args: serde_json::Value) -> omninova_core::computer_use::ComputerUseOutcome {
    session.execute(&args)
}

fn test_app(session: &ComputerUseSession, spec: &AppSpec) {
    println!("\n=== {} (search '{}') ===", spec.label, spec.search);
    let before = pids_of(spec.process);

    // Close any leftover Start menu, then open this app.
    let _ = run(session, json!({"action": "press", "key": "esc"}));
    std::thread::sleep(Duration::from_millis(400));
    let r = run(session, json!({"action": "press", "key": "win"}));
    if !r.ok {
        println!("[FAIL] press win: {}", r.message);
        return;
    }
    std::thread::sleep(Duration::from_millis(1200));
    let r = run(session, json!({"action": "type", "text": spec.search}));
    if !r.ok {
        println!("[FAIL] type search: {}", r.message);
        let _ = run(session, json!({"action": "press", "key": "esc"}));
        return;
    }
    std::thread::sleep(Duration::from_millis(1500));
    let r = run(session, json!({"action": "press", "key": "enter"}));
    if !r.ok {
        println!("[FAIL] press enter: {}", r.message);
        return;
    }

    // Wait for the target window to become foreground.
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut foreground = String::new();
    let mut matched = false;
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_secs(2));
        let shot = run(session, json!({"action": "screenshot"}));
        foreground = shot
            .foreground_app
            .as_ref()
            .map(|a| a.name.clone())
            .unwrap_or_default();
        let hay = foreground.to_lowercase();
        if spec.foreground_hints.iter().any(|h| hay.contains(&h.to_lowercase())) {
            matched = true;
            break;
        }
    }
    if !matched {
        println!("[FAIL] foreground never matched {:?} (last: '{foreground}')", spec.foreground_hints);
        kill_new(spec.process, &before);
        let _ = run(session, json!({"action": "press", "key": "esc"}));
        return;
    }
    println!("[PASS] foreground: {foreground}");

    std::thread::sleep(Duration::from_millis(2500));
    let snap = run(session, json!({"action": "snapshot"}));
    let nodes = snap.nodes.as_ref().map(|n| n.len()).unwrap_or(0);
    println!(
        "[{}] snapshot: {} nodes{}",
        if snap.ok { "PASS" } else { "FAIL" },
        nodes,
        if snap.ok { String::new() } else { format!(" ({})", snap.message) }
    );

    if let Some((label, args)) = &spec.action {
        let r = run(session, args.clone());
        println!("[{}] {label}: {}", if r.ok { "PASS" } else { "FAIL" }, r.message);
    }

    kill_new(spec.process, &before);
    std::thread::sleep(Duration::from_millis(800));
}

fn main() {
    let dir = std::env::temp_dir().join("omninova-cu-apps");
    let mut config = ComputerUseConfig::default();
    config.allowed_apps = vec!["*".into()];
    let session = ComputerUseSession::os(dir, config);

    let apps = [
        AppSpec {
            label: "Notepad",
            search: "notepad",
            process: "notepad.exe",
            foreground_hints: &["notepad", "记事本"],
            action: Some(("type text", json!({"action": "type", "text": "OmniNova notepad 测试"}))),
        },
        AppSpec {
            label: "Calculator (UWP)",
            search: "calculator",
            process: "Calculator.exe",
            foreground_hints: &["计算器", "calculator"],
            action: None,
        },
        AppSpec {
            label: "Excel",
            search: "excel",
            process: "EXCEL.EXE",
            foreground_hints: &["excel"],
            action: Some(("type into cell + enter", json!({"action": "type", "text": "OmniNova Excel 测试"}))),
        },
        AppSpec {
            label: "WPS",
            search: "wps",
            process: "wps.exe",
            foreground_hints: &["wps"],
            action: Some(("type text", json!({"action": "type", "text": "OmniNova WPS 测试"}))),
        },
        AppSpec {
            label: "Edge",
            search: "edge",
            process: "msedge.exe",
            foreground_hints: &["edge"],
            action: Some(("focus address bar + type", json!({"action": "press", "key": "ctrl+l"}))),
        },
        AppSpec {
            label: "Chrome",
            search: "chrome",
            process: "chrome.exe",
            foreground_hints: &["chrome"],
            action: Some(("focus address bar + type", json!({"action": "press", "key": "ctrl+l"}))),
        },
        AppSpec {
            label: "Paint",
            search: "paint",
            process: "mspaint.exe",
            foreground_hints: &["paint", "画图"],
            action: None,
        },
    ];

    let mut pass = 0;
    let total = apps.len();
    for spec in &apps {
        let before_count = pass;
        let _ = before_count;
        test_app(&session, spec);
        pass += 1; // counted below via output inspection; placeholder
    }
    let _ = (pass, total);

    // Make sure the Start menu is closed at the end.
    let _ = run(&session, json!({"action": "press", "key": "esc"}));
    println!("\nall app tests finished.");
}
