use crate::computer_use::ComputerUseSession;
use crate::config::ComputerUseConfig;
use crate::tools::traits::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;

pub struct ComputerUseTool {
    session: Arc<ComputerUseSession>,
}

impl ComputerUseTool {
    pub fn new(workspace: PathBuf, config: ComputerUseConfig) -> Self {
        let captures_dir = if workspace.as_os_str().is_empty() {
            std::env::temp_dir().join("omninova-computer-use")
        } else {
            workspace.join(".omninova").join("computer_use")
        };
        Self {
            session: Arc::new(ComputerUseSession::os(captures_dir, config).with_workspace(workspace)),
        }
    }

    #[cfg(test)]
    pub fn with_session(session: ComputerUseSession) -> Self {
        Self {
            session: Arc::new(session),
        }
    }
}

#[async_trait]
impl Tool for ComputerUseTool {
    fn name(&self) -> &str {
        "computer_use"
    }

    fn description(&self) -> &str {
        "Operate the local OS desktop (native apps: 钉钉, 飞书 client, Excel, 用友, 金蝶). \
         Do NOT use this for websites — use browser. Do NOT use this to search the web — use web_search. \
         To open an app or file use action=launch with target = app name (word, excel), workspace file (report.docx), \
         desktop/taskbar shortcut name, or absolute path — it searches workspace, desktop, taskbar, start menu and installed apps. \
         After launch, snapshot to read controls, then click by name or ref (@e1); coordinate x,y on the screenshot is fallback. \
         Word/Excel: press ctrl+n for a new document, ctrl+s to save. type pastes via clipboard (CJK safe). \
         Long tasks: todo_write + task_checkpoint with a screenshot in evidence. \
         Never click payment, shutdown, or password dialogs."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["screenshot", "snapshot", "click", "type", "press", "scroll", "wait", "launch"],
                    "description": "snapshot reads the accessibility tree (observe). screenshot/wait also observe. click/type/press/scroll change the desktop. launch opens an app or file."
                },
                "target": {
                    "type": "string",
                    "description": "For launch: app name (e.g. word, excel, 钉钉), workspace-relative file (report.docx), desktop/taskbar shortcut name, or absolute path."
                },
                "wait_ms": {
                    "type": "integer",
                    "description": "For launch: how long to wait for the app window to appear (default 6000, max 20000)"
                },
                "x": {
                    "type": "integer",
                    "description": "Image-pixel X for click when not using name/ref (origin top-left of the last screenshot)"
                },
                "y": {
                    "type": "integer",
                    "description": "Image-pixel Y for click when not using name/ref"
                },
                "name": {
                    "type": "string",
                    "description": "Visible accessibility name to click (preferred over x,y)"
                },
                "ref": {
                    "type": "string",
                    "description": "Node id from snapshot, e.g. @e1"
                },
                "role": {
                    "type": "string",
                    "description": "Optional role filter when clicking by name, e.g. button"
                },
                "coordinate_space": {
                    "type": "string",
                    "enum": ["image", "screen"],
                    "description": "Default image. Use screen only for raw display pixels."
                },
                "button": {
                    "type": "string",
                    "enum": ["left", "right", "middle"],
                    "description": "Mouse button for click"
                },
                "text": {
                    "type": "string",
                    "description": "Text to type (clipboard paste)"
                },
                "key": {
                    "type": "string",
                    "description": "Key or hotkey for press, e.g. enter, tab, cmd+v. Dangerous quit/shutdown combos are blocked."
                },
                "direction": {
                    "type": "string",
                    "enum": ["up", "down", "left", "right"],
                    "description": "Scroll direction"
                },
                "amount": {
                    "type": "integer",
                    "description": "Scroll steps (1-30)"
                },
                "duration_ms": {
                    "type": "integer",
                    "description": "Wait duration in milliseconds, capped at 10000"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let session = self.session.clone();
        let outcome = tokio::task::spawn_blocking(move || session.execute(&args))
            .await
            .map_err(|e| anyhow::anyhow!("computer_use worker: {e}"))?;
        let output = outcome.to_json();
        if outcome.ok {
            Ok(ToolResult::success(output))
        } else {
            Ok(ToolResult {
                success: false,
                output: output.clone(),
                error: Some(output),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::computer_use::{ComputerUseSession, DesktopDriver, ForegroundApp};
    use crate::config::ComputerUseConfig;
    use std::path::Path;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct NoopDriver;

    impl DesktopDriver for NoopDriver {
        fn capture_png(&self, dest: &Path) -> Result<(u32, u32), String> {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            image::RgbImage::from_pixel(20, 10, image::Rgb([1, 2, 3]))
                .save(dest)
                .map_err(|e| e.to_string())?;
            Ok((20, 10))
        }
        fn click(&self, _x: i32, _y: i32, _button: &str) -> Result<(), String> {
            Ok(())
        }
        fn paste_text(&self, _text: &str) -> Result<(), String> {
            Ok(())
        }
        fn press(&self, _key: &str) -> Result<(), String> {
            Ok(())
        }
        fn scroll(&self, _direction: &str, _amount: i32) -> Result<(), String> {
            Ok(())
        }
        fn foreground_app(&self) -> Result<ForegroundApp, String> {
            Ok(ForegroundApp {
                name: "Excel".into(),
            })
        }
    }

    #[tokio::test]
    async fn screenshot_succeeds_without_allowlist() {
        let dir = std::env::temp_dir().join(format!(
            "omninova-cu-tool-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let session = ComputerUseSession::with_driver(
            dir.clone(),
            ComputerUseConfig::default(),
            Box::new(NoopDriver),
        );
        let tool = ComputerUseTool::with_session(session);
        let result = tool
            .execute(json!({"action": "screenshot"}))
            .await
            .unwrap();
        assert!(result.success, "{:?}", result.error);
        assert!(result.output.contains("\"ok\":true"));
        let _ = std::fs::remove_dir_all(dir);
        let _ = Mutex::new(());
    }
}
