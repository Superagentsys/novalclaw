use crate::tools::traits::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::process::Stdio;
use tokio::process::Command;
use tokio::time::{Duration, timeout};

const DEFAULT_TIMEOUT_SECS: u64 = 30;
const MAX_OUTPUT_BYTES: usize = 128 * 1024;
const AGENT_BROWSER_BIN: &str = "agent-browser";
const EMBEDDED_AGENT_BROWSER_BIN_ENV: &str = "OMNINOVA_AGENT_BROWSER_BIN";

pub struct BrowserTool {
    allowed_domains: Vec<String>,
    headless: bool,
    attach_only: bool,
    cdp_url: Option<String>,
    session: Option<String>,
}

impl BrowserTool {
    fn resolve_agent_browser_bin() -> String {
        std::env::var(EMBEDDED_AGENT_BROWSER_BIN_ENV).unwrap_or_else(|_| AGENT_BROWSER_BIN.into())
    }

    pub fn new(
        allowed_domains: Vec<String>,
        headless: bool,
        attach_only: bool,
        cdp_url: Option<String>,
    ) -> Self {
        Self {
            allowed_domains,
            headless,
            attach_only,
            cdp_url,
            session: None,
        }
    }

    pub fn with_session(mut self, session: impl Into<String>) -> Self {
        self.session = Some(session.into());
        self
    }

    fn is_domain_allowed(&self, url: &str) -> bool {
        if self.allowed_domains.is_empty() {
            return true;
        }
        let Ok(parsed) = url::Url::parse(url) else {
            return false;
        };
        let Some(host) = parsed.host_str() else {
            return false;
        };
        self.allowed_domains
            .iter()
            .any(|d| host == d.as_str() || host.ends_with(&format!(".{d}")))
    }

    fn truncate(s: String) -> String {
        if s.len() <= MAX_OUTPUT_BYTES {
            return s;
        }
        let mut out = s;
        out.truncate(MAX_OUTPUT_BYTES);
        out.push_str("\n\n[output truncated]");
        out
    }

    async fn run_agent_browser(&self, args: &[&str]) -> anyhow::Result<(bool, String)> {
        let mut cmd = Command::new(Self::resolve_agent_browser_bin());

        if self.headless {
            // headless is the default for agent-browser, no flag needed
        } else {
            cmd.arg("--headed");
        }

        if let Some(session) = &self.session {
            cmd.arg("--session").arg(session);
        }

        if self.attach_only {
            cmd.arg("--attach-only");
        }

        if let Some(cdp_url) = &self.cdp_url {
            cmd.arg("--cdp-url").arg(cdp_url);
        }

        cmd.arg("--json");

        for arg in args {
            cmd.arg(arg);
        }

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let output = timeout(
            Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            cmd.output(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("agent-browser command timed out after {DEFAULT_TIMEOUT_SECS}s"))?
        .map_err(|e| anyhow::anyhow!("failed to execute agent-browser: {e}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let merged = if stderr.trim().is_empty() {
            stdout
        } else if stdout.trim().is_empty() {
            stderr
        } else {
            format!("{stdout}\n{stderr}")
        };

        Ok((output.status.success(), Self::truncate(merged)))
    }
}

#[async_trait]
impl Tool for BrowserTool {
    fn name(&self) -> &str {
        "browser"
    }

    fn description(&self) -> &str {
        "Control a headless browser via agent-browser CLI. Supports: open (navigate to URL), \
         snapshot (get accessibility tree with element refs), click (click element by ref or selector), \
         fill (fill input by ref or selector), type (type text), screenshot (capture page), \
         get_text (get element text), get_html (get element HTML), get_url (current URL), \
         get_title (page title), wait (wait for element/text/time), scroll (scroll page), \
         select (select dropdown option), press (press keyboard key), eval (run JavaScript), \
         close (close browser). Use snapshot first to discover element refs (@e1, @e2, etc.), \
         then interact using those refs."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": [
                        "open", "snapshot", "click", "fill", "type", "screenshot",
                        "get_text", "get_html", "get_url", "get_title", "get_value",
                        "wait", "scroll", "select", "press", "hover", "eval",
                        "back", "forward", "reload", "close",
                        "is_visible", "is_enabled", "find"
                    ],
                    "description": "Browser action to perform"
                },
                "url": {
                    "type": "string",
                    "description": "URL for open action"
                },
                "selector": {
                    "type": "string",
                    "description": "Element ref (@e1) or CSS selector for interaction"
                },
                "value": {
                    "type": "string",
                    "description": "Text value for fill/type/select/eval actions"
                },
                "key": {
                    "type": "string",
                    "description": "Key name for press action (Enter, Tab, etc.)"
                },
                "direction": {
                    "type": "string",
                    "enum": ["up", "down", "left", "right"],
                    "description": "Scroll direction"
                },
                "pixels": {
                    "type": "integer",
                    "description": "Scroll distance in pixels"
                },
                "interactive_only": {
                    "type": "boolean",
                    "description": "For snapshot: only show interactive elements"
                },
                "compact": {
                    "type": "boolean",
                    "description": "For snapshot: remove empty structural elements"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Wait timeout in milliseconds"
                },
                "wait_text": {
                    "type": "string",
                    "description": "For wait: text to wait for"
                },
                "find_role": {
                    "type": "string",
                    "description": "For find: ARIA role (button, link, textbox, etc.)"
                },
                "find_name": {
                    "type": "string",
                    "description": "For find: accessible name filter"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let action = args
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'action' parameter"))?;

        let selector = args.get("selector").and_then(|v| v.as_str());
        let value = args.get("value").and_then(|v| v.as_str());
        let url = args.get("url").and_then(|v| v.as_str());

        let cli_args: Vec<String> = match action {
            "open" => {
                let target_url = url
                    .or(value)
                    .ok_or_else(|| anyhow::anyhow!("'open' requires 'url' parameter"))?;
                if !self.is_domain_allowed(target_url) {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(format!("Domain not in allowed list: {target_url}")),
                    });
                }
                vec!["open".into(), target_url.into()]
            }

            "snapshot" => {
                let mut a = vec!["snapshot".to_string()];
                if args.get("interactive_only").and_then(|v| v.as_bool()).unwrap_or(false) {
                    a.push("-i".into());
                }
                if args.get("compact").and_then(|v| v.as_bool()).unwrap_or(false) {
                    a.push("-c".into());
                }
                a
            }

            "click" => {
                let sel = selector.ok_or_else(|| anyhow::anyhow!("'click' requires 'selector'"))?;
                vec!["click".into(), sel.into()]
            }

            "fill" => {
                let sel = selector.ok_or_else(|| anyhow::anyhow!("'fill' requires 'selector'"))?;
                let val = value.ok_or_else(|| anyhow::anyhow!("'fill' requires 'value'"))?;
                vec!["fill".into(), sel.into(), val.into()]
            }

            "type" => {
                let sel = selector.ok_or_else(|| anyhow::anyhow!("'type' requires 'selector'"))?;
                let val = value.ok_or_else(|| anyhow::anyhow!("'type' requires 'value'"))?;
                vec!["type".into(), sel.into(), val.into()]
            }

            "screenshot" => {
                vec!["screenshot".into()]
            }

            "get_text" => {
                let sel = selector.ok_or_else(|| anyhow::anyhow!("'get_text' requires 'selector'"))?;
                vec!["get".into(), "text".into(), sel.into()]
            }

            "get_html" => {
                let sel = selector.ok_or_else(|| anyhow::anyhow!("'get_html' requires 'selector'"))?;
                vec!["get".into(), "html".into(), sel.into()]
            }

            "get_value" => {
                let sel = selector.ok_or_else(|| anyhow::anyhow!("'get_value' requires 'selector'"))?;
                vec!["get".into(), "value".into(), sel.into()]
            }

            "get_url" => vec!["get".into(), "url".into()],

            "get_title" => vec!["get".into(), "title".into()],

            "wait" => {
                if let Some(text) = args.get("wait_text").and_then(|v| v.as_str()) {
                    vec!["wait".into(), "--text".into(), text.into()]
                } else if let Some(ms) = args.get("timeout_ms").and_then(|v| v.as_u64()) {
                    vec!["wait".into(), ms.to_string()]
                } else if let Some(sel) = selector {
                    vec!["wait".into(), sel.into()]
                } else {
                    vec!["wait".into(), "1000".into()]
                }
            }

            "scroll" => {
                let dir = args
                    .get("direction")
                    .and_then(|v| v.as_str())
                    .unwrap_or("down");
                let px = args
                    .get("pixels")
                    .and_then(|v| v.as_u64())
                    .map(|v| v.to_string());
                let mut a = vec!["scroll".into(), dir.into()];
                if let Some(px) = px {
                    a.push(px);
                }
                a
            }

            "select" => {
                let sel = selector.ok_or_else(|| anyhow::anyhow!("'select' requires 'selector'"))?;
                let val = value.ok_or_else(|| anyhow::anyhow!("'select' requires 'value'"))?;
                vec!["select".into(), sel.into(), val.into()]
            }

            "press" => {
                let key = args
                    .get("key")
                    .and_then(|v| v.as_str())
                    .or(value)
                    .ok_or_else(|| anyhow::anyhow!("'press' requires 'key'"))?;
                vec!["press".into(), key.into()]
            }

            "hover" => {
                let sel = selector.ok_or_else(|| anyhow::anyhow!("'hover' requires 'selector'"))?;
                vec!["hover".into(), sel.into()]
            }

            "eval" => {
                let js = value.ok_or_else(|| anyhow::anyhow!("'eval' requires 'value' (JavaScript code)"))?;
                vec!["eval".into(), js.into()]
            }

            "back" => vec!["back".into()],
            "forward" => vec!["forward".into()],
            "reload" => vec!["reload".into()],
            "close" => vec!["close".into()],

            "is_visible" => {
                let sel = selector.ok_or_else(|| anyhow::anyhow!("'is_visible' requires 'selector'"))?;
                vec!["is".into(), "visible".into(), sel.into()]
            }

            "is_enabled" => {
                let sel = selector.ok_or_else(|| anyhow::anyhow!("'is_enabled' requires 'selector'"))?;
                vec!["is".into(), "enabled".into(), sel.into()]
            }

            "find" => {
                let role = args
                    .get("find_role")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("'find' requires 'find_role'"))?;
                let find_action = value.unwrap_or("text");
                let mut a = vec!["find".into(), "role".into(), role.into(), find_action.into()];
                if let Some(name) = args.get("find_name").and_then(|v| v.as_str()) {
                    a.push("--name".into());
                    a.push(name.into());
                }
                a
            }

            _ => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("Unknown browser action: {action}")),
                });
            }
        };

        let str_args: Vec<&str> = cli_args.iter().map(String::as_str).collect();
        match self.run_agent_browser(&str_args).await {
            Ok((success, output)) => Ok(ToolResult {
                success,
                output,
                error: if success {
                    None
                } else {
                    Some("browser command returned non-zero exit code".into())
                },
            }),
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(e.to_string()),
            }),
        }
    }
}
