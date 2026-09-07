//! OS-level computer use: screenshot, a11y snapshot, click, type, press.
//!
//! Web pages stay on the `browser` tool. This module only talks to the local
//! desktop session. Headless hosts must fail with a clear error.

mod a11y;
mod os;

use crate::config::ComputerUseConfig;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub use a11y::A11yNode;
pub use os::OsDesktopDriver;

const OBSERVE_ACTIONS: &[&str] = &["screenshot", "wait", "snapshot"];
const MUTATING_ACTIONS: &[&str] = &["click", "type", "press", "scroll"];
const BLOCKED_HOTKEYS: &[&str] = &[
    "cmd+q",
    "command+q",
    "cmd+shift+q",
    "command+shift+q",
    "alt+f4",
    "option+f4",
    "ctrl+alt+delete",
    "control+alt+delete",
    "cmd+option+esc",
    "command+option+escape",
    "cmd+opt+esc",
];

/// What a desktop backend must implement. Tests inject a fake.
pub trait DesktopDriver: Send + Sync {
    fn capture_png(&self, dest: &Path) -> Result<(u32, u32), String>;
    fn click(&self, x: i32, y: i32, button: &str) -> Result<(), String>;
    fn paste_text(&self, text: &str) -> Result<(), String>;
    fn press(&self, key: &str) -> Result<(), String>;
    fn scroll(&self, direction: &str, amount: i32) -> Result<(), String>;
    fn foreground_app(&self) -> Result<ForegroundApp, String>;
    fn accessibility_snapshot(&self, max_nodes: usize) -> Result<Vec<A11yNode>, String> {
        let _ = max_nodes;
        Err("accessibility snapshot is not available on this driver".into())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForegroundApp {
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct CaptureMemory {
    pub path: PathBuf,
    pub image_width: u32,
    pub image_height: u32,
    pub screen_width: u32,
    pub screen_height: u32,
    pub hash: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ComputerUseOutcome {
    pub ok: bool,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub foreground_app: Option<ForegroundApp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clicked: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nodes: Option<Vec<A11yNode>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thrash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handoff: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handoff_reason: Option<String>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ComputerUseOutcome {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            r#"{"ok":false,"action":"unknown","message":"serialize failed"}"#.into()
        })
    }
}

static INPUT_PAUSED: AtomicBool = AtomicBool::new(false);

/// Shared hourly budget across reconstructed tool instances in one process.
fn hourly_stamps() -> &'static Mutex<Vec<Instant>> {
    static STAMPS: OnceLock<Mutex<Vec<Instant>>> = OnceLock::new();
    STAMPS.get_or_init(|| Mutex::new(Vec::new()))
}

/// E-Stop must freeze mouse/keyboard immediately, not only the next LLM turn.
pub fn set_desktop_input_paused(paused: bool) {
    INPUT_PAUSED.store(paused, Ordering::SeqCst);
}

pub fn desktop_input_paused() -> bool {
    INPUT_PAUSED.load(Ordering::SeqCst)
}

#[cfg(test)]
pub fn reset_hourly_budget_for_tests() {
    if let Ok(mut stamps) = hourly_stamps().lock() {
        stamps.clear();
    }
}

pub fn is_observe_action(action: &str) -> bool {
    OBSERVE_ACTIONS
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(action))
}

pub fn is_mutating_action(action: &str) -> bool {
    MUTATING_ACTIONS
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(action))
}

pub fn app_is_allowed(allowed_apps: &[String], foreground: &str) -> bool {
    if ComputerUseConfig::allowlist_allows_all(allowed_apps) {
        return true;
    }
    if allowed_apps.is_empty() {
        return false;
    }
    let haystack = normalize_app_name(foreground);
    allowed_apps.iter().any(|allowed| {
        let needle = normalize_app_name(allowed);
        !needle.is_empty() && haystack.contains(&needle)
    })
}

fn normalize_app_name(name: &str) -> String {
    name.chars()
        .filter(|ch| !ch.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

/// File stem of a process path or window identity, e.g. `EXCEL.EXE` → `EXCEL`.
/// Splits on both `/` and `\` so Windows paths still parse on Unix test hosts.
pub fn app_process_stem(name: &str) -> String {
    let trimmed = name.trim().trim_matches('"');
    if trimmed.is_empty() {
        return String::new();
    }
    let file = trimmed
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(trimmed);
    Path::new(file)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(file)
        .to_string()
}

/// Console hosts used to drive the desktop. They must not steal the
/// allowlist / a11y target from the app the user actually asked for.
pub fn is_desktop_input_helper(name: &str) -> bool {
    matches!(
        normalize_app_name(&app_process_stem(name)).as_str(),
        "powershell"
            | "pwsh"
            | "conhost"
            | "cmd"
            | "windowsterminal"
            | "openconsole"
            | "powershell_ise"
    )
}

/// Prefer a human/app identity (`Excel` / `工作簿1 - Excel`) over a raw exe path.
pub fn foreground_display_name(process: &str, title: &str) -> String {
    let stem = app_process_stem(process);
    let title = title.trim();
    if is_desktop_input_helper(process) || is_desktop_input_helper(title) {
        if !title.is_empty() && !is_desktop_input_helper(title) {
            return title.to_string();
        }
        return if stem.is_empty() {
            process.trim().to_string()
        } else {
            stem
        };
    }
    if stem.is_empty() {
        return title.to_string();
    }
    if title.is_empty() {
        return stem;
    }
    if normalize_app_name(title).contains(&normalize_app_name(&stem)) {
        title.to_string()
    } else {
        format!("{stem}: {title}")
    }
}

pub fn is_blocked_hotkey(key: &str) -> bool {
    let normalized = normalize_hotkey(key);
    BLOCKED_HOTKEYS
        .iter()
        .any(|blocked| normalize_hotkey(blocked) == normalized)
}

fn normalize_hotkey(key: &str) -> String {
    let mut parts: Vec<String> = key
        .split(['+', '-', ' '])
        .map(|part| part.trim().to_ascii_lowercase())
        .filter(|part| !part.is_empty())
        .map(|part| match part.as_str() {
            "command" | "meta" | "win" | "windows" => "cmd".into(),
            "option" | "opt" => "alt".into(),
            "control" => "ctrl".into(),
            "return" => "enter".into(),
            "escape" => "esc".into(),
            other => other.to_string(),
        })
        .collect();
    let key_name = parts.pop();
    parts.sort();
    if let Some(name) = key_name {
        parts.push(name);
    }
    parts.join("+")
}

pub fn scale_image_to_screen(
    image_x: i32,
    image_y: i32,
    image_width: u32,
    image_height: u32,
    screen_width: u32,
    screen_height: u32,
) -> Option<(i32, i32)> {
    if image_width == 0 || image_height == 0 || screen_width == 0 || screen_height == 0 {
        return None;
    }
    if image_x < 0
        || image_y < 0
        || image_x as u32 >= image_width
        || image_y as u32 >= image_height
    {
        return None;
    }
    let screen_x = (image_x as i64 * screen_width as i64) / image_width as i64;
    let screen_y = (image_y as i64 * screen_height as i64) / image_height as i64;
    Some((screen_x as i32, screen_y as i32))
}

pub fn observation_paths_from_output(output: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(output) else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    if let Some(path) = value
        .pointer("/image/path")
        .and_then(serde_json::Value::as_str)
    {
        if !path.trim().is_empty() {
            paths.push(path.to_string());
        }
    }
    paths
}

pub fn observation_data_urls(paths: &[String], max_dimension_px: u32) -> Vec<String> {
    paths
        .iter()
        .filter_map(|path| encode_observation_jpeg(Path::new(path), max_dimension_px).ok())
        .collect()
}

pub fn is_image_evidence_path(path: &str) -> bool {
    let lowered = path.to_ascii_lowercase();
    lowered.ends_with(".png")
        || lowered.ends_with(".jpg")
        || lowered.ends_with(".jpeg")
        || lowered.ends_with(".webp")
        || lowered.ends_with(".observe.jpg")
}

/// Encode checkpoint evidence images for the next wake (newest first, capped).
pub fn evidence_data_urls(evidence: &[String], max_dimension_px: u32, limit: usize) -> Vec<String> {
    evidence
        .iter()
        .rev()
        .filter(|path| is_image_evidence_path(path) && Path::new(path).is_file())
        .take(limit)
        .filter_map(|path| encode_observation_jpeg(Path::new(path), max_dimension_px).ok())
        .collect()
}

pub fn latest_observation_in(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name()?.to_string_lossy();
        if !(name.ends_with(".observe.jpg") || name.ends_with(".jpg") || name.ends_with(".png")) {
            continue;
        }
        let modified = entry.metadata().ok()?.modified().ok()?;
        if newest
            .as_ref()
            .map(|(ts, _)| modified > *ts)
            .unwrap_or(true)
        {
            newest = Some((modified, path));
        }
    }
    newest.map(|(_, path)| path)
}

pub enum ThrashStatus {
    Soft { advice: String },
    Hard { message: String },
}

pub fn thrash_status_from_output(output: &str) -> Option<ThrashStatus> {
    let value = serde_json::from_str::<serde_json::Value>(output).ok()?;
    match value.get("thrash").and_then(|item| item.as_str()) {
        Some("hard") => Some(ThrashStatus::Hard {
            message: value
                .get("message")
                .and_then(|item| item.as_str())
                .unwrap_or("computer_use thrash: stopped after repeated failures")
                .to_string(),
        }),
        Some("soft") => Some(ThrashStatus::Soft {
            advice: "[computer_use] 同一控件连续失败。请改用 action=snapshot，按 name 或 ref 点击，不要重复同一坐标。".into(),
        }),
        _ => None,
    }
}

pub fn hard_thrash_reply(output: &str) -> Option<String> {
    match thrash_status_from_output(output) {
        Some(ThrashStatus::Hard { message }) => Some(message),
        _ => None,
    }
}

pub fn soft_thrash_advice(output: &str) -> Option<String> {
    match thrash_status_from_output(output) {
        Some(ThrashStatus::Soft { advice }) => Some(advice),
        _ => None,
    }
}

#[allow(dead_code)]
pub fn human_handoff_reply(output: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(output).ok()?;
    if value.get("handoff").and_then(|item| item.as_str()) != Some("human") {
        return None;
    }
    Some(
        value
            .get("message")
            .and_then(|item| item.as_str())
            .unwrap_or("computer_use handoff: window lost or session/network failed; wait for a human")
            .to_string(),
    )
}

#[allow(dead_code)]
pub fn looks_like_desktop_shell(name: &str) -> bool {
    let haystack = normalize_app_name(name);
    DESKTOP_SHELL_APPS
        .iter()
        .any(|shell| haystack.contains(&normalize_app_name(shell)))
}

const DESKTOP_SHELL_APPS: &[&str] = &[
    "finder",
    "explorer",
    "explorer.exe",
    "loginwindow",
    "screensaverengine",
    "dwm",
    "dock",
    "windowserver",
    "progman",
    "desktop window manager",
];

#[allow(dead_code)]
const RELOCATE_PIXELS: i32 = 24;

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

/// Transport ceiling for one frame handed to the model.
///
/// The dimension cap alone does not bound bytes: a busy multi-monitor retina
/// desktop stays detailed after downscaling and encodes far larger than a plain
/// window. Providers reject oversized request bodies outright, and a slightly
/// softer screenshot the model can actually read beats a crisp one it rejects.
const MAX_OBSERVATION_JPEG_BYTES: usize = 320 * 1024;

/// Floor for the step-down loop, below which text-shaped UI stops being legible.
const MIN_OBSERVATION_DIMENSION_PX: u32 = 640;

const OBSERVATION_JPEG_QUALITY: u8 = 72;
const MIN_OBSERVATION_JPEG_QUALITY: u8 = 45;

pub fn encode_observation_jpeg(path: &Path, max_dimension_px: u32) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read screenshot failed: {e}"))?;
    let image =
        image::load_from_memory(&bytes).map_err(|e| format!("decode screenshot failed: {e}"))?;
    let mut dimension = max_dimension_px.max(320);
    let mut quality = OBSERVATION_JPEG_QUALITY;
    let mut buffer = encode_jpeg(&image, dimension, quality)?;
    while buffer.len() > MAX_OBSERVATION_JPEG_BYTES && dimension > MIN_OBSERVATION_DIMENSION_PX {
        dimension = (dimension / 4 * 3).max(MIN_OBSERVATION_DIMENSION_PX);
        quality = quality
            .saturating_sub(8)
            .max(MIN_OBSERVATION_JPEG_QUALITY);
        buffer = encode_jpeg(&image, dimension, quality)?;
    }
    Ok(format!(
        "data:image/jpeg;base64,{}",
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, buffer)
    ))
}

fn encode_jpeg(
    image: &image::DynamicImage,
    max_dimension_px: u32,
    quality: u8,
) -> Result<Vec<u8>, String> {
    let resized = resize_max_dimension(image.clone(), max_dimension_px);
    let rgb = resized.to_rgb8();
    let mut buffer = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(Cursor::new(&mut buffer), quality)
        .encode_image(&rgb)
        .map_err(|e| format!("JPEG encode failed: {e}"))?;
    Ok(buffer)
}

fn resize_max_dimension(image: image::DynamicImage, max_dimension_px: u32) -> image::DynamicImage {
    use image::GenericImageView;
    let (width, height) = image.dimensions();
    let longest = width.max(height);
    if longest <= max_dimension_px {
        return image;
    }
    let scale = max_dimension_px as f32 / longest as f32;
    let target_w = ((width as f32) * scale).round().max(1.0) as u32;
    let target_h = ((height as f32) * scale).round().max(1.0) as u32;
    image.resize(target_w, target_h, image::imageops::FilterType::Triangle)
}

pub struct ComputerUseSession {
    pub captures_dir: PathBuf,
    pub config: ComputerUseConfig,
    pub last_capture: Mutex<Option<CaptureMemory>>,
    pub     last_snapshot: Mutex<Vec<A11yNode>>,
    #[allow(dead_code)]
    last_guarded_app: Mutex<Option<String>>,
    thrash: Mutex<ThrashState>,
    pub turn_actions: AtomicU32,
    pub driver: Box<dyn DesktopDriver>,
}

#[derive(Default)]
struct ThrashState {
    fingerprint: String,
    count: u32,
}

impl ComputerUseSession {
    pub fn os(captures_dir: PathBuf, config: ComputerUseConfig) -> Self {
        Self {
            captures_dir,
            config,
            last_capture: Mutex::new(None),
            last_snapshot: Mutex::new(Vec::new()),
            last_guarded_app: Mutex::new(None),
            thrash: Mutex::new(ThrashState::default()),
            turn_actions: AtomicU32::new(0),
            driver: Box::new(OsDesktopDriver),
        }
    }

    pub fn with_driver(
        captures_dir: PathBuf,
        config: ComputerUseConfig,
        driver: Box<dyn DesktopDriver>,
    ) -> Self {
        Self {
            captures_dir,
            config,
            last_capture: Mutex::new(None),
            last_snapshot: Mutex::new(Vec::new()),
            last_guarded_app: Mutex::new(None),
            thrash: Mutex::new(ThrashState::default()),
            turn_actions: AtomicU32::new(0),
            driver,
        }
    }

    pub fn execute(&self, args: &serde_json::Value) -> ComputerUseOutcome {
        let action = args
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if action.is_empty() {
            return failure("", "missing action");
        }

        if is_mutating_action(&action) && desktop_input_paused() {
            return failure(
                &action,
                "agent is paused by emergency stop (E-Stop); desktop input is frozen",
            );
        }

        if let Err(error) = self.consume_budget(&action) {
            return failure(&action, error);
        }

        match action.as_str() {
            "screenshot" => self.screenshot(),
            "snapshot" => self.snapshot(),
            "wait" => self.wait(args),
            "click" => self.click(args),
            "type" => self.r#type(args),
            "press" => self.press(args),
            "scroll" => self.scroll(args),
            other => failure(other, format!("unsupported action '{other}'")),
        }
    }

    /// Both budgets treat `0` as unlimited so an unattended desktop task can
    /// keep going. Only the run counter is rolled back when a later check
    /// rejects the action, otherwise a denied action would still be charged.
    fn consume_budget(&self, action: &str) -> Result<(), String> {
        if is_observe_action(action) {
            return Ok(());
        }
        let run_limit = self.config.max_actions_per_turn;
        let charged_run = run_limit > 0;
        if charged_run {
            let used = self.turn_actions.fetch_add(1, Ordering::SeqCst) + 1;
            if used > run_limit {
                self.turn_actions.fetch_sub(1, Ordering::SeqCst);
                return Err(format!(
                    "computer_use run budget exceeded ({}/{run_limit}). Raise computer_use.max_actions_per_turn, or set it to 0 for unlimited.",
                    used - 1
                ));
            }
        }

        let hourly_limit = self.config.max_actions_per_hour;
        if hourly_limit == 0 {
            return Ok(());
        }
        let refund_run = || {
            if charged_run {
                self.turn_actions.fetch_sub(1, Ordering::SeqCst);
            }
        };
        let Ok(mut stamps) = hourly_stamps().lock() else {
            refund_run();
            return Err("computer_use hourly budget lock poisoned".to_string());
        };
        let cutoff = Instant::now()
            .checked_sub(Duration::from_secs(3600))
            .unwrap_or_else(Instant::now);
        stamps.retain(|stamp| *stamp > cutoff);
        if stamps.len() as u32 >= hourly_limit {
            refund_run();
            return Err(format!(
                "computer_use hourly budget exceeded ({}/{hourly_limit}). Raise computer_use.max_actions_per_hour, or set it to 0 for unlimited.",
                stamps.len()
            ));
        }
        stamps.push(Instant::now());
        Ok(())
    }

    fn guard_foreground(&self) -> Result<ForegroundApp, String> {
        let app = self.driver.foreground_app()?;
        if !app_is_allowed(&self.config.allowed_apps, &app.name) {
            return Err(format!(
                "foreground app '{}' is not in computer_use.allowed_apps ({:?}). Add the app name, or set allowed_apps = [\"*\"] to allow every foreground app.",
                app.name, self.config.allowed_apps
            ));
        }
        Ok(app)
    }

    fn screenshot(&self) -> ComputerUseOutcome {
        match self.capture_now("observe") {
            Ok(capture) => {
                self.clear_thrash();
                ComputerUseOutcome {
                    ok: true,
                    action: "screenshot".into(),
                    foreground_app: self.driver.foreground_app().ok(),
                    screen: Some(serde_json::json!({
                        "width": capture.screen_width,
                        "height": capture.screen_height,
                    })),
                    image: Some(image_json(&capture)),
                    clicked: None,
                    nodes: None,
                    thrash: None,
                    handoff: None,
                    handoff_reason: None,
                    message: "captured primary display. Prefer snapshot then click by name/ref; coordinate x,y are image pixels, origin top-left.".into(),
                    error: None,
                }
            }
            Err(error) => failure("screenshot", error),
        }
    }

    fn snapshot(&self) -> ComputerUseOutcome {
        self.clear_thrash();
        let max_nodes = self.config.max_snapshot_nodes.max(1);
        let capture = self.capture_now("snapshot").ok();
        match self.driver.accessibility_snapshot(max_nodes) {
            Ok(raw) => {
                let nodes = a11y::finalize_nodes(raw, max_nodes);
                if let Ok(mut guard) = self.last_snapshot.lock() {
                    *guard = nodes.clone();
                }
                ComputerUseOutcome {
                    ok: true,
                    action: "snapshot".into(),
                    foreground_app: self.driver.foreground_app().ok(),
                    screen: capture.as_ref().map(|item| {
                        serde_json::json!({
                            "width": item.screen_width,
                            "height": item.screen_height,
                        })
                    }),
                    image: capture.as_ref().map(image_json),
                    clicked: None,
                    nodes: Some(nodes.clone()),
                    thrash: None,
                    handoff: None,
                    handoff_reason: None,
                    message: if nodes.is_empty() {
                        "accessibility tree was empty. Fall back to screenshot coordinates.".into()
                    } else {
                        format!(
                            "captured {} interactive nodes. Click with name or ref (e.g. @e1); coordinates are fallback.",
                            nodes.len()
                        )
                    },
                    error: None,
                }
            }
            Err(error) => {
                let mut outcome = failure(
                    "snapshot",
                    format!("{error}. Fall back to screenshot + image coordinates."),
                );
                if let Some(capture) = capture {
                    outcome.screen = Some(serde_json::json!({
                        "width": capture.screen_width,
                        "height": capture.screen_height,
                    }));
                    outcome.image = Some(image_json(&capture));
                }
                outcome
            }
        }
    }

    fn wait(&self, args: &serde_json::Value) -> ComputerUseOutcome {
        let ms = args
            .get("duration_ms")
            .or_else(|| args.get("ms"))
            .and_then(|v| v.as_u64())
            .unwrap_or(500)
            .min(10_000);
        std::thread::sleep(Duration::from_millis(ms));
        let mut outcome = self.screenshot();
        outcome.action = "wait".into();
        outcome.message = format!("waited {ms}ms, then captured the screen");
        outcome
    }

    fn click(&self, args: &serde_json::Value) -> ComputerUseOutcome {
        let foreground = match self.guard_foreground() {
            Ok(app) => app,
            Err(error) => return failure("click", error),
        };
        let button = args
            .get("button")
            .and_then(|v| v.as_str())
            .unwrap_or("left");
        let resolved = match self.resolve_click(args) {
            Ok(resolved) => resolved,
            Err(error) => return failure("click", error),
        };
        let fingerprint = self.fingerprint(
            "click",
            &foreground.name,
            &resolved.target,
        );
        if let Some(blocked) = self.hard_thrash_block("click", &fingerprint) {
            return blocked;
        }
        let before_hash = self.last_capture_hash();
        if let Err(error) = self.driver.click(resolved.screen_x, resolved.screen_y, button) {
            return self.record_repeat(
                failure("click", error),
                &fingerprint,
            );
        }
        std::thread::sleep(Duration::from_millis(200));
        match self.capture_now("after_click") {
            Ok(after) => {
                let unchanged = before_hash == Some(after.hash);
                let outcome = ComputerUseOutcome {
                    ok: true,
                    action: "click".into(),
                    foreground_app: Some(foreground),
                    screen: Some(serde_json::json!({
                        "width": after.screen_width,
                        "height": after.screen_height,
                    })),
                    image: Some(image_json(&after)),
                    clicked: Some(serde_json::json!({
                        "image_x": resolved.image_x,
                        "image_y": resolved.image_y,
                        "screen_x": resolved.screen_x,
                        "screen_y": resolved.screen_y,
                        "button": button,
                        "coordinate_space": resolved.space,
                        "via": resolved.via,
                        "name": resolved.name,
                        "ref": resolved.element_ref,
                    })),
                    nodes: None,
                    thrash: None,
                    handoff: None,
                    handoff_reason: None,
                    message: "clicked and captured after-image".into(),
                    error: None,
                };
                if unchanged {
                    self.record_repeat(outcome, &fingerprint)
                } else {
                    self.clear_thrash();
                    outcome
                }
            }
            Err(error) => failure("click", format!("clicked but after-screenshot failed: {error}")),
        }
    }

    fn r#type(&self, args: &serde_json::Value) -> ComputerUseOutcome {
        let foreground = match self.guard_foreground() {
            Ok(app) => app,
            Err(error) => return failure("type", error),
        };
        let Some(text) = args
            .get("text")
            .or_else(|| args.get("value"))
            .and_then(|v| v.as_str())
        else {
            return failure("type", "missing text");
        };
        if text.chars().count() > 8_000 {
            return failure("type", "text exceeds 8000 characters");
        }
        let target: String = text.chars().take(40).collect();
        let fingerprint = self.fingerprint("type", &foreground.name, &target);
        if let Some(blocked) = self.hard_thrash_block("type", &fingerprint) {
            return blocked;
        }
        if let Err(error) = self.driver.paste_text(text) {
            return self.record_repeat(failure("type", error), &fingerprint);
        }
        self.after_input(
            "type",
            foreground,
            format!("typed {} characters", text.chars().count()),
            fingerprint,
        )
    }

    fn press(&self, args: &serde_json::Value) -> ComputerUseOutcome {
        let foreground = match self.guard_foreground() {
            Ok(app) => app,
            Err(error) => return failure("press", error),
        };
        let Some(key) = args.get("key").and_then(|v| v.as_str()) else {
            return failure("press", "missing key");
        };
        if is_blocked_hotkey(key) {
            return failure("press", format!("hotkey '{key}' is blocked"));
        }
        let fingerprint = self.fingerprint("press", &foreground.name, key);
        if let Some(blocked) = self.hard_thrash_block("press", &fingerprint) {
            return blocked;
        }
        if let Err(error) = self.driver.press(key) {
            return self.record_repeat(failure("press", error), &fingerprint);
        }
        self.after_input("press", foreground, format!("pressed {key}"), fingerprint)
    }

    fn scroll(&self, args: &serde_json::Value) -> ComputerUseOutcome {
        let foreground = match self.guard_foreground() {
            Ok(app) => app,
            Err(error) => return failure("scroll", error),
        };
        let direction = args
            .get("direction")
            .and_then(|v| v.as_str())
            .unwrap_or("down");
        let amount = int_arg(args, &["amount", "pixels"]).unwrap_or(3).clamp(1, 30);
        let fingerprint = self.fingerprint(
            "scroll",
            &foreground.name,
            &format!("{direction}:{amount}"),
        );
        if let Some(blocked) = self.hard_thrash_block("scroll", &fingerprint) {
            return blocked;
        }
        if let Err(error) = self.driver.scroll(direction, amount) {
            return self.record_repeat(failure("scroll", error), &fingerprint);
        }
        self.after_input(
            "scroll",
            foreground,
            format!("scrolled {direction} x{amount}"),
            fingerprint,
        )
    }

    fn after_input(
        &self,
        action: &str,
        foreground: ForegroundApp,
        message: String,
        fingerprint: String,
    ) -> ComputerUseOutcome {
        std::thread::sleep(Duration::from_millis(150));
        let before_hash = self.last_capture_hash();
        match self.capture_now(&format!("after_{action}")) {
            Ok(after) => {
                let unchanged = before_hash == Some(after.hash);
                let outcome = ComputerUseOutcome {
                    ok: true,
                    action: action.into(),
                    foreground_app: Some(foreground),
                    screen: Some(serde_json::json!({
                        "width": after.screen_width,
                        "height": after.screen_height,
                    })),
                    image: Some(image_json(&after)),
                    clicked: None,
                    nodes: None,
                    thrash: None,
                    handoff: None,
                    handoff_reason: None,
                    message,
                    error: None,
                };
                if unchanged {
                    self.record_repeat(outcome, &fingerprint)
                } else {
                    self.clear_thrash();
                    outcome
                }
            }
            Err(error) => failure(action, format!("{message}, but screenshot failed: {error}")),
        }
    }

    fn ensure_fresh_capture(&self) -> Result<(), String> {
        if !self.config.require_screenshot_before_click {
            return Ok(());
        }
        let has = self
            .last_capture
            .lock()
            .map_err(|_| "capture lock poisoned".to_string())?
            .is_some();
        if has {
            return Ok(());
        }
        self.capture_now("before_click").map(|_| ())
    }

    fn resolve_click(&self, args: &serde_json::Value) -> Result<ResolvedClick, String> {
        let name = string_arg(args, &["name", "label", "title"]);
        let element_ref = string_arg(args, &["ref", "element"]);
        let role = string_arg(args, &["role"]);
        if element_ref.is_some() || name.is_some() {
            let nodes = self.nodes_for_hit()?;
            let node = if let Some(element_ref) = element_ref.as_deref() {
                a11y::find_by_ref(&nodes, element_ref)
                    .cloned()
                    .ok_or_else(|| {
                        format!("unknown ref '{element_ref}'. Call action=snapshot first.")
                    })?
            } else {
                a11y::find_by_name(&nodes, name.as_deref().unwrap_or(""), role.as_deref())?.clone()
            };
            let (screen_x, screen_y) = node.center();
            return Ok(ResolvedClick {
                screen_x,
                screen_y,
                image_x: None,
                image_y: None,
                space: "screen".into(),
                via: if element_ref.is_some() { "ref" } else { "name" },
                target: element_ref
                    .clone()
                    .or(name.clone())
                    .unwrap_or_else(|| node.name.clone()),
                name: Some(node.name),
                element_ref: Some(node.id),
            });
        }

        if let Err(error) = self.ensure_fresh_capture() {
            return Err(error);
        }
        let Some(x) = int_arg(args, &["x", "image_x"]) else {
            return Err("missing x, name, or ref".into());
        };
        let Some(y) = int_arg(args, &["y", "image_y"]) else {
            return Err("missing y, name, or ref".into());
        };
        let space = args
            .get("coordinate_space")
            .and_then(|v| v.as_str())
            .unwrap_or("image");
        let memory = self
            .last_capture
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
            .ok_or_else(|| "no screenshot in this session".to_string())?;
        let (screen_x, screen_y) = if space.eq_ignore_ascii_case("screen") {
            (x, y)
        } else {
            scale_image_to_screen(
                x,
                y,
                memory.image_width,
                memory.image_height,
                memory.screen_width,
                memory.screen_height,
            )
            .ok_or_else(|| {
                format!(
                    "image coordinate ({x},{y}) outside {}x{}",
                    memory.image_width, memory.image_height
                )
            })?
        };
        Ok(ResolvedClick {
            screen_x,
            screen_y,
            image_x: Some(x),
            image_y: Some(y),
            space: space.to_string(),
            via: "coordinates",
            target: format!("{x},{y}"),
            name: None,
            element_ref: None,
        })
    }

    fn nodes_for_hit(&self) -> Result<Vec<A11yNode>, String> {
        if let Ok(guard) = self.last_snapshot.lock() {
            if !guard.is_empty() {
                return Ok(guard.clone());
            }
        }
        let max_nodes = self.config.max_snapshot_nodes.max(1);
        let nodes = a11y::finalize_nodes(self.driver.accessibility_snapshot(max_nodes)?, max_nodes);
        if let Ok(mut guard) = self.last_snapshot.lock() {
            *guard = nodes.clone();
        }
        if nodes.is_empty() {
            return Err(
                "accessibility tree was empty; call snapshot or click with image x,y".into(),
            );
        }
        Ok(nodes)
    }

    fn fingerprint(&self, action: &str, app: &str, target: &str) -> String {
        format!(
            "{}|{}|{}",
            normalize_app_name(app),
            action,
            target.trim().to_ascii_lowercase()
        )
    }

    fn last_capture_hash(&self) -> Option<u64> {
        self.last_capture
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|capture| capture.hash))
    }

    fn matching_thrash_count(&self, fingerprint: &str) -> u32 {
        self.thrash
            .lock()
            .ok()
            .map(|guard| {
                if guard.fingerprint == fingerprint {
                    guard.count
                } else {
                    0
                }
            })
            .unwrap_or(0)
    }

    fn hard_thrash_block(&self, action: &str, fingerprint: &str) -> Option<ComputerUseOutcome> {
        let limit = self.config.thrash_hard_limit;
        if limit == 0 {
            return None;
        }
        if self.matching_thrash_count(fingerprint) < limit {
            return None;
        }
        let mut outcome = failure(
            action,
            format!(
                "computer_use thrash: same target failed {limit} times. Stop, call snapshot, pick a different control, or task_checkpoint(status=blocked)."
            ),
        );
        outcome.thrash = Some("hard".into());
        Some(outcome)
    }

    fn record_repeat(&self, mut outcome: ComputerUseOutcome, fingerprint: &str) -> ComputerUseOutcome {
        let count = if let Ok(mut guard) = self.thrash.lock() {
            if guard.fingerprint == fingerprint {
                guard.count = guard.count.saturating_add(1);
            } else {
                guard.fingerprint = fingerprint.to_string();
                guard.count = 1;
            }
            guard.count
        } else {
            1
        };
        let soft = self.config.thrash_soft_limit;
        let hard = self.config.thrash_hard_limit;
        if hard > 0 && count >= hard {
            outcome.ok = false;
            outcome.thrash = Some("hard".into());
            outcome.message = format!(
                "computer_use thrash: same target failed {count} times. Stop, call snapshot, pick a different control, or task_checkpoint(status=blocked)."
            );
            outcome.error = Some(outcome.message.clone());
        } else if soft > 0 && count >= soft {
            outcome.thrash = Some("soft".into());
            outcome.message = format!(
                "{} [thrash {count}/{}: use snapshot and click by name/ref]",
                outcome.message, hard.max(soft)
            );
        }
        outcome
    }

    fn clear_thrash(&self) {
        if let Ok(mut guard) = self.thrash.lock() {
            *guard = ThrashState::default();
        }
    }

    fn capture_now(&self, prefix: &str) -> Result<CaptureMemory, String> {
        std::fs::create_dir_all(&self.captures_dir)
            .map_err(|e| format!("cannot create captures dir: {e}"))?;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let dest = self
            .captures_dir
            .join(format!("{prefix}_{timestamp}.png"));
        let (screen_width, screen_height) = self.driver.capture_png(&dest)?;
        let bytes =
            std::fs::read(&dest).map_err(|e| format!("read captured png failed: {e}"))?;
        let image =
            image::load_from_memory(&bytes).map_err(|e| format!("decode captured png failed: {e}"))?;
        let resized = resize_max_dimension(image, self.config.max_dimension_px.max(320));
        let (image_width, image_height) = {
            use image::GenericImageView;
            resized.dimensions()
        };
        let preview = dest.with_extension("observe.jpg");
        resized
            .to_rgb8()
            .save(&preview)
            .map_err(|e| format!("write observation jpeg failed: {e}"))?;
        let jpeg = std::fs::read(&preview).unwrap_or_default();
        let memory = CaptureMemory {
            path: preview,
            image_width,
            image_height,
            screen_width,
            screen_height,
            hash: hash_bytes(&jpeg),
        };
        if let Ok(mut guard) = self.last_capture.lock() {
            *guard = Some(memory.clone());
        }
        Ok(memory)
    }
}

struct ResolvedClick {
    screen_x: i32,
    screen_y: i32,
    image_x: Option<i32>,
    image_y: Option<i32>,
    space: String,
    via: &'static str,
    target: String,
    name: Option<String>,
    element_ref: Option<String>,
}

fn image_json(capture: &CaptureMemory) -> serde_json::Value {
    serde_json::json!({
        "path": capture.path,
        "width": capture.image_width,
        "height": capture.image_height,
        "coordinate_space": "image",
    })
}

fn failure(action: impl Into<String>, error: impl Into<String>) -> ComputerUseOutcome {
    let error = error.into();
    ComputerUseOutcome {
        ok: false,
        action: action.into(),
        foreground_app: None,
        screen: None,
        image: None,
        clicked: None,
        nodes: None,
        thrash: None,
        handoff: None,
        handoff_reason: None,
        message: error.clone(),
        error: Some(error),
    }
}

fn string_arg(args: &serde_json::Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(text) = args.get(*key).and_then(|value| value.as_str()) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn int_arg(args: &serde_json::Value, keys: &[&str]) -> Option<i32> {
    for key in keys {
        let Some(value) = args.get(*key) else {
            continue;
        };
        if let Some(n) = value.as_i64() {
            return i32::try_from(n).ok();
        }
        if let Some(n) = value.as_u64() {
            return i32::try_from(n).ok();
        }
        if let Some(text) = value.as_str() {
            if let Ok(n) = text.parse::<i32>() {
                return Some(n);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU8, AtomicUsize};
    use std::sync::Arc;

    struct FakeDriver {
        app: String,
        clicks: Arc<AtomicUsize>,
        last_text: Mutex<Option<String>>,
        fail_capture: bool,
        fail_click: bool,
        frozen: bool,
        generation: AtomicU8,
        nodes: Vec<A11yNode>,
    }

    impl DesktopDriver for FakeDriver {
        fn capture_png(&self, dest: &Path) -> Result<(u32, u32), String> {
            if self.fail_capture {
                return Err("no display".into());
            }
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            let tint = self.generation.load(Ordering::SeqCst);
            let img =
                image::RgbImage::from_pixel(200, 100, image::Rgb([20, 40, 80u8.wrapping_add(tint)]));
            img.save(dest).map_err(|e| e.to_string())?;
            Ok((2000, 1000))
        }

        fn click(&self, _x: i32, _y: i32, _button: &str) -> Result<(), String> {
            if self.fail_click {
                return Err("click rejected".into());
            }
            self.clicks.fetch_add(1, Ordering::SeqCst);
            if !self.frozen {
                self.generation.fetch_add(1, Ordering::SeqCst);
            }
            Ok(())
        }

        fn paste_text(&self, text: &str) -> Result<(), String> {
            *self.last_text.lock().unwrap() = Some(text.to_string());
            if !self.frozen {
                self.generation.fetch_add(1, Ordering::SeqCst);
            }
            Ok(())
        }

        fn press(&self, _key: &str) -> Result<(), String> {
            if !self.frozen {
                self.generation.fetch_add(1, Ordering::SeqCst);
            }
            Ok(())
        }

        fn scroll(&self, _direction: &str, _amount: i32) -> Result<(), String> {
            if !self.frozen {
                self.generation.fetch_add(1, Ordering::SeqCst);
            }
            Ok(())
        }

        fn foreground_app(&self) -> Result<ForegroundApp, String> {
            Ok(ForegroundApp {
                name: self.app.clone(),
            })
        }

        fn accessibility_snapshot(&self, _max_nodes: usize) -> Result<Vec<A11yNode>, String> {
            Ok(self.nodes.clone())
        }
    }

    /// The E-Stop flag and the hourly budget are process-global, so a test that
    /// pauses input denies the clicks of every test running beside it. Handing
    /// this guard out with the session makes the reset above hold for the whole
    /// test rather than just until the next one starts.
    type DesktopStateGuard = std::sync::MutexGuard<'static, ()>;

    fn desktop_state_guard() -> DesktopStateGuard {
        static GUARD: Mutex<()> = Mutex::new(());
        // A panicking test poisons the lock, and every acquirer resets the
        // state it cares about, so the poison carries no stale expectations.
        GUARD.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn session(
        app: &str,
        allowed: &[&str],
    ) -> (
        ComputerUseSession,
        Arc<AtomicUsize>,
        PathBuf,
        DesktopStateGuard,
    ) {
        session_with(app, allowed, |driver| driver)
    }

    fn session_with(
        app: &str,
        allowed: &[&str],
        tweak: impl FnOnce(FakeDriver) -> FakeDriver,
    ) -> (
        ComputerUseSession,
        Arc<AtomicUsize>,
        PathBuf,
        DesktopStateGuard,
    ) {
        let guard = desktop_state_guard();
        reset_hourly_budget_for_tests();
        set_desktop_input_paused(false);
        let dir = std::env::temp_dir().join(format!(
            "omninova-cu-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let clicks = Arc::new(AtomicUsize::new(0));
        let mut config = ComputerUseConfig::default();
        config.allowed_apps = allowed.iter().map(|s| (*s).to_string()).collect();
        config.max_actions_per_turn = 20;
        config.max_actions_per_hour = 40;
        let driver = tweak(FakeDriver {
            app: app.into(),
            clicks: clicks.clone(),
            last_text: Mutex::new(None),
            fail_capture: false,
            fail_click: false,
            frozen: false,
            generation: AtomicU8::new(0),
            nodes: Vec::new(),
        });
        (
            ComputerUseSession::with_driver(dir.clone(), config, Box::new(driver)),
            clicks,
            dir,
            guard,
        )
    }

    #[test]
    fn whitelist_matches_normalized_names() {
        assert!(app_is_allowed(&["钉钉".into()], "钉钉"));
        assert!(app_is_allowed(&["DingTalk".into()], "ding talk"));
        assert!(!app_is_allowed(&["Excel".into()], "Finder"));
        assert!(!app_is_allowed(&[], "钉钉"));
        assert!(app_is_allowed(&["*".into()], "Finder"));
        assert!(app_is_allowed(&["all".into()], "Excel"));
        assert!(app_is_allowed(
            &["Excel".into()],
            r"C:\Program Files\Microsoft Office\root\Office16\EXCEL.EXE"
        ));
        assert!(app_is_allowed(&["Excel".into()], "工作簿1 - Excel"));
        assert!(!app_is_allowed(
            &["Excel".into()],
            r"C:\WINDOWS\System32\WindowsPowerShell\v1.0\powershell.exe"
        ));
    }

    #[test]
    fn powershell_console_is_an_input_helper_not_excel() {
        assert!(is_desktop_input_helper(
            r"C:\WINDOWS\System32\WindowsPowerShell\v1.0\powershell.exe"
        ));
        assert!(is_desktop_input_helper("pwsh"));
        assert!(!is_desktop_input_helper(
            r"C:\Program Files\Microsoft Office\root\Office16\EXCEL.EXE"
        ));
        assert_eq!(
            foreground_display_name(
                r"C:\Program Files\Microsoft Office\root\Office16\EXCEL.EXE",
                "工作簿1 - Excel"
            ),
            "工作簿1 - Excel"
        );
        assert_eq!(
            foreground_display_name("EXCEL.EXE", "Book1"),
            "EXCEL: Book1"
        );
    }

    #[test]
    fn blocked_hotkeys_normalize_aliases() {
        assert!(is_blocked_hotkey("Command+Q"));
        assert!(is_blocked_hotkey("alt-f4"));
        assert!(!is_blocked_hotkey("enter"));
        assert!(!is_blocked_hotkey("cmd+v"));
    }

    #[test]
    fn image_coordinates_scale_to_screen() {
        assert_eq!(
            scale_image_to_screen(100, 50, 200, 100, 2000, 1000),
            Some((1000, 500))
        );
        assert_eq!(scale_image_to_screen(-1, 0, 200, 100, 2000, 1000), None);
        assert_eq!(scale_image_to_screen(200, 0, 200, 100, 2000, 1000), None);
    }

    #[test]
    fn empty_allowlist_blocks_click() {
        let (session, clicks, dir, _guard) = session("钉钉", &[]);
        let shot = session.execute(&serde_json::json!({"action": "screenshot"}));
        assert!(shot.ok, "{}", shot.message);
        let clicked = session.execute(&serde_json::json!({"action": "click", "x": 10, "y": 10}));
        assert!(!clicked.ok);
        assert_eq!(clicks.load(Ordering::SeqCst), 0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn star_allowlist_allows_any_foreground_app() {
        let (session, clicks, dir, _guard) = session("Finder", &["*"]);
        let shot = session.execute(&serde_json::json!({"action": "screenshot"}));
        assert!(shot.ok, "{}", shot.message);
        let clicked = session.execute(&serde_json::json!({"action": "click", "x": 10, "y": 10}));
        assert!(clicked.ok, "{}", clicked.message);
        assert_eq!(clicks.load(Ordering::SeqCst), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn click_uses_image_space_and_captures_after() {
        let (session, clicks, dir, _guard) = session("钉钉", &["钉钉"]);
        let shot = session.execute(&serde_json::json!({"action": "screenshot"}));
        assert!(shot.ok, "{}", shot.message);
        let clicked = session.execute(&serde_json::json!({
            "action": "click",
            "x": 100,
            "y": 50
        }));
        assert!(clicked.ok, "{}", clicked.message);
        assert_eq!(clicks.load(Ordering::SeqCst), 1);
        let pair = clicked.clicked.expect("clicked payload");
        assert_eq!(pair.get("screen_x").and_then(|v| v.as_i64()), Some(1000));
        assert_eq!(pair.get("screen_y").and_then(|v| v.as_i64()), Some(500));
        assert!(clicked.image.is_some());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn turn_budget_stops_extra_clicks() {
        let (mut session, clicks, dir, _guard) = session("Excel", &["Excel"]);
        session.config.max_actions_per_turn = 3;
        session.execute(&serde_json::json!({"action": "screenshot"}));
        for _ in 0..3 {
            let result = session.execute(&serde_json::json!({"action": "click", "x": 10, "y": 10}));
            assert!(result.ok, "{}", result.message);
        }
        let denied = session.execute(&serde_json::json!({"action": "click", "x": 10, "y": 10}));
        assert!(!denied.ok);
        assert!(denied.message.contains("run budget"), "{}", denied.message);
        assert_eq!(clicks.load(Ordering::SeqCst), 3);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn unlimited_budgets_keep_driving_the_desktop() {
        let (mut session, clicks, dir, _guard) = session("Excel", &["*"]);
        session.config.max_actions_per_turn = 0;
        session.config.max_actions_per_hour = 0;
        session.execute(&serde_json::json!({"action": "screenshot"}));
        // Comfortably past the old 15-per-run / 40-per-hour walls.
        for index in 0..60 {
            let result = session.execute(&serde_json::json!({"action": "click", "x": 10, "y": 10}));
            assert!(result.ok, "click {index} failed: {}", result.message);
        }
        assert_eq!(clicks.load(Ordering::SeqCst), 60);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn denied_action_is_not_charged_to_the_run_budget() {
        let (mut session, clicks, dir, _guard) = session("Excel", &["Excel"]);
        session.config.max_actions_per_turn = 2;
        session.config.max_actions_per_hour = 0;
        session.execute(&serde_json::json!({"action": "screenshot"}));
        for _ in 0..2 {
            assert!(session
                .execute(&serde_json::json!({"action": "click", "x": 10, "y": 10}))
                .ok);
        }
        for _ in 0..3 {
            let denied = session.execute(&serde_json::json!({"action": "click", "x": 10, "y": 10}));
            assert!(!denied.ok);
            assert!(denied.message.contains("run budget"), "{}", denied.message);
        }
        // Rejected attempts must not keep inflating the counter.
        assert_eq!(session.turn_actions.load(Ordering::SeqCst), 2);
        assert_eq!(clicks.load(Ordering::SeqCst), 2);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn observation_paths_are_extracted_from_json() {
        let output = r#"{"ok":true,"action":"screenshot","image":{"path":"/tmp/a.observe.jpg"}}"#;
        assert_eq!(
            observation_paths_from_output(output),
            vec!["/tmp/a.observe.jpg".to_string()]
        );
        assert!(observation_paths_from_output("not-json").is_empty());
    }

    #[test]
    fn estop_blocks_mutating_actions() {
        let (session, clicks, dir, _guard) = session("钉钉", &["钉钉"]);
        session.execute(&serde_json::json!({"action": "screenshot"}));
        set_desktop_input_paused(true);
        let clicked = session.execute(&serde_json::json!({"action": "click", "x": 10, "y": 10}));
        assert!(!clicked.ok);
        assert!(clicked.message.contains("E-Stop"));
        assert_eq!(clicks.load(Ordering::SeqCst), 0);
        set_desktop_input_paused(false);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn latest_observation_picks_newest_image() {
        let dir = std::env::temp_dir().join(format!(
            "omninova-cu-latest-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let older = dir.join("older.observe.jpg");
        let newer = dir.join("newer.observe.jpg");
        image::RgbImage::from_pixel(8, 8, image::Rgb([1, 2, 3]))
            .save(&older)
            .unwrap();
        std::thread::sleep(Duration::from_millis(20));
        image::RgbImage::from_pixel(8, 8, image::Rgb([4, 5, 6]))
            .save(&newer)
            .unwrap();
        let found = latest_observation_in(&dir).unwrap();
        assert_eq!(found.file_name(), newer.file_name());
        assert!(is_image_evidence_path(&found.to_string_lossy()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn snapshot_is_observe_and_click_by_name_uses_node_center() {
        let (session, clicks, dir, _guard) = session_with("钉钉", &["钉钉"], |mut driver| {
            driver.nodes = vec![A11yNode {
                id: String::new(),
                role: "button".into(),
                name: "发送".into(),
                value: None,
                x: 100,
                y: 50,
                width: 80,
                height: 30,
                enabled: true,
            }];
            driver
        });
        let snap = session.execute(&serde_json::json!({"action": "snapshot"}));
        assert!(snap.ok, "{}", snap.message);
        assert!(is_observe_action("snapshot"));
        let nodes = snap.nodes.expect("nodes");
        assert_eq!(nodes[0].id, "@e1");
        let clicked = session.execute(&serde_json::json!({
            "action": "click",
            "name": "发送"
        }));
        assert!(clicked.ok, "{}", clicked.message);
        assert_eq!(clicks.load(Ordering::SeqCst), 1);
        let pair = clicked.clicked.expect("clicked payload");
        assert_eq!(pair.get("via").and_then(|v| v.as_str()), Some("name"));
        assert_eq!(pair.get("screen_x").and_then(|v| v.as_i64()), Some(140));
        assert_eq!(pair.get("screen_y").and_then(|v| v.as_i64()), Some(65));
        let by_ref = session.execute(&serde_json::json!({
            "action": "click",
            "ref": "@e1"
        }));
        assert!(by_ref.ok, "{}", by_ref.message);
        assert_eq!(clicks.load(Ordering::SeqCst), 2);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn thrash_soft_then_hard_stops_repeated_failed_clicks() {
        let (session, clicks, dir, _guard) = session_with("钉钉", &["钉钉"], |mut driver| {
            driver.fail_click = true;
            driver
        });
        session.execute(&serde_json::json!({"action": "screenshot"}));
        let mut last = None;
        for _ in 0..3 {
            last = Some(session.execute(&serde_json::json!({"action": "click", "x": 10, "y": 10})));
        }
        let soft = last.unwrap();
        assert!(!soft.ok);
        assert_eq!(soft.thrash.as_deref(), Some("soft"));
        assert_eq!(clicks.load(Ordering::SeqCst), 0);
        session.execute(&serde_json::json!({"action": "click", "x": 10, "y": 10}));
        let hard = session.execute(&serde_json::json!({"action": "click", "x": 10, "y": 10}));
        assert!(!hard.ok);
        assert_eq!(hard.thrash.as_deref(), Some("hard"));
        let blocked = session.execute(&serde_json::json!({"action": "click", "x": 10, "y": 10}));
        assert_eq!(blocked.thrash.as_deref(), Some("hard"));
        assert!(blocked.message.contains("thrash"));
        session.execute(&serde_json::json!({"action": "snapshot"}));
        let after_reset = session.execute(&serde_json::json!({"action": "click", "x": 10, "y": 10}));
        assert_ne!(after_reset.thrash.as_deref(), Some("hard"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn thrash_status_parses_soft_and_hard() {
        assert!(matches!(
            thrash_status_from_output(r#"{"thrash":"soft","message":"x"}"#),
            Some(ThrashStatus::Soft { .. })
        ));
        assert!(matches!(
            thrash_status_from_output(r#"{"thrash":"hard","message":"stop"}"#),
            Some(ThrashStatus::Hard { message }) if message == "stop"
        ));
        assert!(hard_thrash_reply(r#"{"thrash":"hard","message":"stop"}"#).is_some());
        assert!(soft_thrash_advice(r#"{"thrash":"soft"}"#).is_some());
    }
}
