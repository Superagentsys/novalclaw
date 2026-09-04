//! Browser tool adapter: frozen V1 schema → `BrowserRuntime` domain calls.
//!
//! Vendor CLI details live in `browser_agent_backend`. This module must not
//! spawn processes or classify vendor diagnostics.

use crate::tools::browser_agent_backend::{browser_session_missing_error, AgentBrowserBackend};
use crate::tools::browser_bin::BrowserBinarySearch;
use crate::tools::browser_runtime::{present_backend_error, BrowserRuntime, BrowserRuntimePolicy};
use crate::tools::browser_types::{
    route_v1_tool_action, BrowserAction, BrowserBackendError, BrowserElementRef,
    BrowserObservation, BrowserObserveKind, BrowserSessionKey, BrowserSessionOptions,
    BrowserTarget, NavigateRequest, ObserveRequest, ScreenshotRequest, ScrollDirection,
};
use crate::tools::traits::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;

pub use crate::tools::browser_agent_backend::{
    browser_session_id, BROWSER_SESSION_HASH_CHARS, BROWSER_SESSION_PREFIX,
};
pub use crate::tools::browser_runtime::{browser_host_allowed, parse_browser_open_url};

pub struct BrowserTool {
    runtime: BrowserRuntime,
    session_key: Option<BrowserSessionKey>,
    session_opts: BrowserSessionOptions,
}

impl BrowserTool {
    pub fn new(
        allowed_domains: Vec<String>,
        headless: bool,
        attach_only: bool,
        cdp_url: Option<String>,
    ) -> Self {
        let session_opts = BrowserSessionOptions {
            headless,
            attach_only,
            cdp_url,
            profile: None,
        };
        Self::assemble(allowed_domains, session_opts, None, None)
    }

    pub fn from_runtime(
        runtime: BrowserRuntime,
        session_opts: BrowserSessionOptions,
        session_key: Option<BrowserSessionKey>,
    ) -> Self {
        Self {
            runtime,
            session_key,
            session_opts,
        }
    }

    fn assemble(
        allowed_domains: Vec<String>,
        session_opts: BrowserSessionOptions,
        search: Option<BrowserBinarySearch>,
        session_key: Option<BrowserSessionKey>,
    ) -> Self {
        let backend = Arc::new(AgentBrowserBackend::new(search, session_opts.clone()));
        let policy = BrowserRuntimePolicy {
            allowed_domains,
            ..BrowserRuntimePolicy::default()
        };
        Self {
            runtime: BrowserRuntime::new(backend, policy),
            session_key,
            session_opts,
        }
    }

    #[cfg(test)]
    fn with_binary_search(self, search: BrowserBinarySearch) -> Self {
        Self::assemble(
            self.runtime.policy().allowed_domains.clone(),
            self.session_opts,
            Some(search),
            self.session_key,
        )
    }

    pub fn with_session(mut self, session: impl Into<String>) -> Self {
        let raw = session.into();
        let trimmed = raw.trim();
        self.session_key = if trimmed.is_empty() {
            None
        } else {
            BrowserSessionKey::new(trimmed).ok()
        };
        self
    }

    #[cfg(test)]
    pub(crate) fn session_id(&self) -> Option<&str> {
        self.session_key.as_ref().map(|key| key.as_str())
    }

    fn selector_target(&self, selector: &str) -> BrowserTarget {
        if selector.starts_with('@') {
            BrowserTarget::Element(BrowserElementRef::new(self.runtime.backend_id(), selector))
        } else {
            BrowserTarget::Css(selector.to_string())
        }
    }

    fn fail(err: BrowserBackendError) -> ToolResult {
        ToolResult {
            success: false,
            output: String::new(),
            error: Some(present_backend_error(&err)),
        }
    }

    fn observation_output(obs: BrowserObservation) -> String {
        obs.text
            .or_else(|| obs.snapshot.map(|snap| snap.text))
            .unwrap_or_default()
    }
}

#[async_trait]
impl Tool for BrowserTool {
    fn name(&self) -> &str {
        "browser"
    }

    fn description(&self) -> &str {
        "Control a real headless browser via the agent-browser CLI. Use ONLY for JavaScript-rendered \
         pages, login, clicks, forms, and typing. Do not use this to search the web (web_search), \
         read static HTML (web_fetch), or call REST APIs (http_request). \
         Actions: open (http/https only), snapshot (@eN refs), click, fill, type, screenshot \
         (local file path), get_text, get_html, get_url, get_title, wait, scroll, select, press, \
         eval, close. \
         Permanent errors (BrowserBinaryMissing, BrowserSessionMissing, BrowserUrlRejected) will \
         not succeed on retry — do not repeat the same failed call."
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

        if route_v1_tool_action(action).is_none() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Unknown browser action: {action}")),
            });
        }

        let Some(key) = self.session_key.as_ref() else {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(browser_session_missing_error()),
            });
        };

        let selector = args.get("selector").and_then(|v| v.as_str());
        let value = args.get("value").and_then(|v| v.as_str());
        let url = args.get("url").and_then(|v| v.as_str());
        let opts = &self.session_opts;

        let output = match action {
            "open" => {
                let target_url = url
                    .or(value)
                    .ok_or_else(|| anyhow::anyhow!("'open' requires 'url' parameter"))?;
                match self
                    .runtime
                    .open(
                        key,
                        opts,
                        &NavigateRequest {
                            url: target_url.to_string(),
                        },
                    )
                    .await
                {
                    Ok(result) => result.detail,
                    Err(err) => return Ok(Self::fail(err)),
                }
            }
            "snapshot" | "get_text" | "get_html" | "get_url" | "get_title" | "get_value"
            | "is_visible" | "is_enabled" | "find" => {
                let req = match action {
                    "snapshot" => ObserveRequest {
                        kind: BrowserObserveKind::Snapshot,
                        interactive_only: args
                            .get("interactive_only")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false),
                        compact: args
                            .get("compact")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false),
                    },
                    "get_text" => ObserveRequest {
                        kind: BrowserObserveKind::Text {
                            target: selector.map(|sel| self.selector_target(sel)),
                        },
                        interactive_only: false,
                        compact: false,
                    },
                    "get_html" => {
                        let sel = selector
                            .ok_or_else(|| anyhow::anyhow!("'get_html' requires 'selector'"))?;
                        ObserveRequest {
                            kind: BrowserObserveKind::Html {
                                target: Some(self.selector_target(sel)),
                            },
                            interactive_only: false,
                            compact: false,
                        }
                    }
                    "get_url" => ObserveRequest {
                        kind: BrowserObserveKind::Url,
                        interactive_only: false,
                        compact: false,
                    },
                    "get_title" => ObserveRequest {
                        kind: BrowserObserveKind::Title,
                        interactive_only: false,
                        compact: false,
                    },
                    "get_value" => {
                        let sel = selector
                            .ok_or_else(|| anyhow::anyhow!("'get_value' requires 'selector'"))?;
                        ObserveRequest {
                            kind: BrowserObserveKind::Value {
                                target: self.selector_target(sel),
                            },
                            interactive_only: false,
                            compact: false,
                        }
                    }
                    "is_visible" => {
                        let sel = selector
                            .ok_or_else(|| anyhow::anyhow!("'is_visible' requires 'selector'"))?;
                        ObserveRequest {
                            kind: BrowserObserveKind::Visibility {
                                target: self.selector_target(sel),
                            },
                            interactive_only: false,
                            compact: false,
                        }
                    }
                    "is_enabled" => {
                        let sel = selector
                            .ok_or_else(|| anyhow::anyhow!("'is_enabled' requires 'selector'"))?;
                        ObserveRequest {
                            kind: BrowserObserveKind::Enabled {
                                target: self.selector_target(sel),
                            },
                            interactive_only: false,
                            compact: false,
                        }
                    }
                    "find" => {
                        let role = args
                            .get("find_role")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| anyhow::anyhow!("'find' requires 'find_role'"))?;
                        ObserveRequest {
                            kind: BrowserObserveKind::Find {
                                role: role.to_string(),
                                name: args
                                    .get("find_name")
                                    .and_then(|v| v.as_str())
                                    .map(str::to_string),
                                action: value.map(str::to_string),
                            },
                            interactive_only: false,
                            compact: false,
                        }
                    }
                    _ => unreachable!(),
                };
                match self.runtime.observe(key, opts, &req).await {
                    Ok(obs) => Self::observation_output(obs),
                    Err(err) => return Ok(Self::fail(err)),
                }
            }
            "click" | "fill" | "type" | "press" | "scroll" | "select" | "hover" | "eval"
            | "wait" | "back" | "forward" | "reload" => {
                let mapped = match action {
                    "click" => {
                        let sel = selector
                            .ok_or_else(|| anyhow::anyhow!("'click' requires 'selector'"))?;
                        BrowserAction::Click {
                            target: self.selector_target(sel),
                        }
                    }
                    "fill" => {
                        let sel = selector
                            .ok_or_else(|| anyhow::anyhow!("'fill' requires 'selector'"))?;
                        let val =
                            value.ok_or_else(|| anyhow::anyhow!("'fill' requires 'value'"))?;
                        BrowserAction::Fill {
                            target: self.selector_target(sel),
                            value: val.to_string(),
                        }
                    }
                    "type" => {
                        let sel = selector
                            .ok_or_else(|| anyhow::anyhow!("'type' requires 'selector'"))?;
                        let val =
                            value.ok_or_else(|| anyhow::anyhow!("'type' requires 'value'"))?;
                        BrowserAction::Type {
                            target: self.selector_target(sel),
                            text: val.to_string(),
                        }
                    }
                    "press" => {
                        let key_name = args
                            .get("key")
                            .and_then(|v| v.as_str())
                            .or(value)
                            .ok_or_else(|| anyhow::anyhow!("'press' requires 'key'"))?;
                        BrowserAction::Press {
                            key: key_name.to_string(),
                        }
                    }
                    "scroll" => {
                        let dir = args
                            .get("direction")
                            .and_then(|v| v.as_str())
                            .unwrap_or("down");
                        let direction = match dir {
                            "up" => ScrollDirection::Up,
                            "left" => ScrollDirection::Left,
                            "right" => ScrollDirection::Right,
                            _ => ScrollDirection::Down,
                        };
                        let pixels = args
                            .get("pixels")
                            .and_then(|v| v.as_u64())
                            .map(|v| v as u32);
                        BrowserAction::Scroll {
                            direction,
                            pixels,
                            target: selector.map(|sel| self.selector_target(sel)),
                        }
                    }
                    "select" => {
                        let sel = selector
                            .ok_or_else(|| anyhow::anyhow!("'select' requires 'selector'"))?;
                        let val =
                            value.ok_or_else(|| anyhow::anyhow!("'select' requires 'value'"))?;
                        BrowserAction::Select {
                            target: self.selector_target(sel),
                            value: val.to_string(),
                        }
                    }
                    "hover" => {
                        let sel = selector
                            .ok_or_else(|| anyhow::anyhow!("'hover' requires 'selector'"))?;
                        BrowserAction::Hover {
                            target: self.selector_target(sel),
                        }
                    }
                    "eval" => {
                        let js = value.ok_or_else(|| {
                            anyhow::anyhow!("'eval' requires 'value' (JavaScript code)")
                        })?;
                        BrowserAction::Eval {
                            script: js.to_string(),
                        }
                    }
                    "wait" => BrowserAction::Wait {
                        timeout_ms: args.get("timeout_ms").and_then(|v| v.as_u64()),
                        text: args
                            .get("wait_text")
                            .and_then(|v| v.as_str())
                            .map(str::to_string),
                        target: selector.map(|sel| self.selector_target(sel)),
                    },
                    "back" => BrowserAction::Back,
                    "forward" => BrowserAction::Forward,
                    "reload" => BrowserAction::Reload,
                    _ => unreachable!(),
                };
                match self.runtime.act(key, opts, &mapped).await {
                    Ok(result) => result.detail,
                    Err(err) => return Ok(Self::fail(err)),
                }
            }
            "screenshot" => match self
                .runtime
                .screenshot(key, opts, &ScreenshotRequest { locator: None })
                .await
            {
                Ok(result) => result.locator,
                Err(err) => return Ok(Self::fail(err)),
            },
            "close" => match self.runtime.close_session(key, opts).await {
                Ok(()) => "Browser session closed.".to_string(),
                Err(err) => return Ok(Self::fail(err)),
            },
            _ => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("Unknown browser action: {action}")),
                });
            }
        };

        Ok(ToolResult {
            success: true,
            output,
            error: None,
        })
    }

    #[cfg(test)]
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::browser_agent_backend::validate_cli_session_name;
    use crate::tools::browser_bin::{bundled_agent_browser_relative_path, BrowserBinarySearch};
    use crate::tools::browser_lifecycle::{
        forget_owned_browser_session, recover_owned_session, remember_owned_browser_session,
    };
    use crate::tools::browser_output::parse_action_outcome;
    use crate::tools::browser_types::{BrowserErrorKind, BrowserHealth, V1_TOOL_ACTIONS};
    use serde_json::json;

    fn isolated_missing_search() -> BrowserBinarySearch {
        let root = std::env::temp_dir().join(format!(
            "omninova-browser-tool-missing-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        BrowserBinarySearch {
            env_path: None,
            bundled_candidates: Vec::new(),
            extra_roots: Vec::new(),
            include_exe_relative: false,
            path_dirs: Some(vec![root.join("no-such-bin")]),
        }
    }

    /// Prefer the staged Tauri resource (bundled) with PATH/env disabled so
    /// live tests cannot silently pass via a developer global npm install.
    fn native_cli_search() -> Option<BrowserBinarySearch> {
        let resource_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/omninova-tauri/src-tauri/resources");
        let bundled = resource_root.join(bundled_agent_browser_relative_path());
        if bundled.is_file() {
            return Some(BrowserBinarySearch {
                env_path: None,
                bundled_candidates: Vec::new(),
                extra_roots: vec![resource_root],
                include_exe_relative: false,
                path_dirs: Some(Vec::new()),
            });
        }
        let resolved = BrowserBinarySearch::from_process().resolve().ok()?;
        Some(BrowserBinarySearch {
            env_path: Some(resolved.path),
            bundled_candidates: Vec::new(),
            extra_roots: Vec::new(),
            include_exe_relative: false,
            path_dirs: Some(Vec::new()),
        })
    }

    #[tokio::test]
    async fn missing_binary_returns_structured_error() {
        let tool = BrowserTool::new(Vec::new(), true, false, None)
            .with_session("omninova-test-session")
            .with_binary_search(isolated_missing_search());
        let result = tool
            .execute(json!({"action": "open", "url": "https://example.com"}))
            .await
            .unwrap();
        assert!(!result.success);
        let error = result.error.expect("missing binary must set error");
        assert!(error.starts_with("BrowserBinaryMissing:"), "error={error}");
        assert!(error.contains("requested_binary="), "error={error}");
        assert!(error.contains("resolution_source="), "error={error}");
        assert!(error.contains("checked_candidates="), "error={error}");
    }

    #[tokio::test]
    async fn missing_session_returns_structured_error_instead_of_using_default() {
        let tool = BrowserTool::new(Vec::new(), true, false, None)
            .with_binary_search(isolated_missing_search());
        let result = tool
            .execute(json!({"action": "open", "url": "https://example.com"}))
            .await
            .unwrap();
        assert!(!result.success);
        let error = result.error.expect("missing session must set error");
        assert!(error.starts_with("BrowserSessionMissing:"), "error={error}");
        assert!(
            !error.contains("BrowserBinaryMissing"),
            "session guard should run before binary resolution: {error}"
        );
    }

    #[test]
    fn browser_session_mapping_is_deterministic_safe_and_distinct() {
        let a = "chat/room 1";
        let b = "chat/room 2";
        let mapped_a = browser_session_id(Some(a)).expect("A");
        let mapped_b = browser_session_id(Some(b)).expect("B");
        assert_ne!(mapped_a, mapped_b);
        assert!(mapped_a.starts_with("omninova-"));
        assert!(mapped_a.len() < 64);
        assert_eq!(mapped_a, browser_session_id(Some(a)).unwrap());
        assert_eq!(mapped_b, browser_session_id(Some(b)).unwrap());

        for missing in [None, Some(""), Some("   ")] {
            let err = browser_session_id(missing).expect_err("blank must not map");
            assert!(
                err.starts_with("BrowserSessionMissing:"),
                "missing={missing:?} err={err}"
            );
            assert!(
                !err.contains("omninova-"),
                "missing session must not produce a mapped id: {err}"
            );
        }

        let long = format!("x{}y", "a".repeat(2000));
        let mapped_long = browser_session_id(Some(&long)).unwrap();
        assert!(mapped_long.starts_with("omninova-"));
        assert!(mapped_long.len() < 64);

        let unicode = "会 话-😀/测试";
        let mapped_unicode = browser_session_id(Some(unicode)).unwrap();
        assert!(mapped_unicode
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'));
        assert_eq!(mapped_unicode, browser_session_id(Some(unicode)).unwrap());
        assert!(!mapped_unicode.contains("anonymous"));
    }

    #[test]
    fn recovery_keeps_same_mapped_session_id() {
        let mapped = browser_session_id(Some("w23-recover-chat-a")).unwrap();
        let _ = recover_owned_session(&mapped);
        assert_eq!(
            mapped,
            browser_session_id(Some("w23-recover-chat-a")).unwrap()
        );
        validate_cli_session_name(&mapped).unwrap();
    }

    #[test]
    fn close_forgets_current_owned_session_only() {
        use crate::tools::browser_lifecycle::{owned_browser_sessions, with_owned_sessions_lock};
        with_owned_sessions_lock(|| {
            remember_owned_browser_session("omninova-sess-a");
            remember_owned_browser_session("omninova-sess-b");
            forget_owned_browser_session("omninova-sess-a");
            assert_eq!(
                owned_browser_sessions(),
                vec!["omninova-sess-b".to_string()]
            );
        });
    }

    #[test]
    fn browser_open_rejects_dangerous_schemes_and_accepts_https() {
        assert!(parse_browser_open_url("https://example.com/").is_ok());
        assert!(parse_browser_open_url("http://example.com").is_ok());
        assert_eq!(
            parse_browser_open_url("example.com").unwrap().as_str(),
            "https://example.com/"
        );
        for raw in [
            "file:///C:/Windows/win.ini",
            "javascript:alert(1)",
            "data:text/html,hi",
            "chrome://settings",
            "chrome-extension://abc/page.html",
            "about:blank",
        ] {
            let err = parse_browser_open_url(raw).expect_err(raw);
            assert!(
                err.starts_with("BrowserUrlRejected:"),
                "raw={raw} err={err}"
            );
        }
    }

    #[tokio::test]
    async fn browser_open_blocks_file_scheme_before_cli() {
        let tool = BrowserTool::new(Vec::new(), true, false, None)
            .with_session("omninova-test-session")
            .with_binary_search(isolated_missing_search());
        let result = tool
            .execute(json!({"action": "open", "url": "file:///etc/passwd"}))
            .await
            .unwrap();
        assert!(!result.success);
        let error = result.error.expect("must set error");
        assert!(error.starts_with("BrowserUrlRejected:"), "error={error}");
        assert!(
            !error.contains("BrowserBinaryMissing"),
            "scheme guard must run before binary spawn: {error}"
        );
    }

    #[test]
    fn browser_session_id_never_uses_default_or_anonymous_fallback() {
        let mapped = browser_session_id(Some("anything")).unwrap();
        assert!(mapped.starts_with("omninova-"));
        assert_eq!(
            mapped.len(),
            BROWSER_SESSION_PREFIX.len() + 1 + BROWSER_SESSION_HASH_CHARS
        );
        assert!(mapped
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'));
        assert_ne!(mapped, "default");
        assert!(!mapped.contains("anything"));
        assert!(!mapped.contains("anonymous"));
        validate_cli_session_name(&mapped).expect("mapped id must be CLI-safe");
        assert!(browser_session_id(None)
            .unwrap_err()
            .starts_with("BrowserSessionMissing:"));
    }

    #[tokio::test]
    async fn blank_session_names_are_rejected_before_binary() {
        for name in ["", "   "] {
            let mut tool = BrowserTool::new(Vec::new(), true, false, None)
                .with_binary_search(isolated_missing_search());
            if !name.is_empty() {
                tool = tool.with_session(name);
            }
            let result = tool
                .execute(json!({"action": "open", "url": "https://example.com"}))
                .await
                .unwrap();
            assert!(!result.success, "name={name:?} should fail");
            let error = result.error.expect("must set error");
            assert!(
                error.starts_with("BrowserSessionMissing:"),
                "name={name:?} error={error}"
            );
            assert!(
                !error.contains("BrowserBinaryMissing"),
                "session guard should run before binary resolution: {error}"
            );
        }
    }

    #[tokio::test]
    async fn default_logical_session_returns_session_invalid_before_binary() {
        for name in ["default", "Default"] {
            let tool = BrowserTool::new(Vec::new(), true, false, None)
                .with_session(name)
                .with_binary_search(isolated_missing_search());
            let result = tool
                .execute(json!({"action": "open", "url": "https://example.com"}))
                .await
                .unwrap();
            assert!(!result.success, "name={name:?} should fail");
            let error = result.error.expect("must set error");
            assert!(
                error.starts_with("BrowserSessionInvalid:"),
                "name={name:?} error={error}"
            );
            assert!(
                !error.contains("BrowserBinaryMissing"),
                "logical default must fail before binary resolution: {error}"
            );
        }
    }

    #[test]
    fn tool_schema_remains_frozen() {
        let tool = BrowserTool::new(Vec::new(), true, false, None);
        let actions = tool.parameters_schema()["properties"]["action"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            actions,
            V1_TOOL_ACTIONS
                .iter()
                .map(|s| (*s).to_string())
                .collect::<Vec<_>>()
        );
        assert!(!actions.iter().any(|a| a == "tabs"));
        assert!(!actions.iter().any(|a| a == "read"));
    }

    #[test]
    fn read_does_not_change_tool_schema() {
        let tool = BrowserTool::new(Vec::new(), true, false, None);
        let schema = tool.parameters_schema();
        let encoded = schema.to_string();
        assert!(!encoded.contains("\"read\""));
        let actions = schema["properties"]["action"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(actions, V1_TOOL_ACTIONS.to_vec());
        assert!(!actions.contains(&"read"));
    }

    #[test]
    fn v1_normalizer_still_owns_clicked_ref_and_truncation_markers() {
        let click = parse_action_outcome(
            "click",
            r#"{"success":true,"data":{"clicked":"@e2"},"error":null}"#,
            "",
            true,
        );
        assert_eq!(click.output, "Clicked @e2.");
        let url = parse_action_outcome(
            "get_url",
            r#"{"success":true,"data":{"url":"https://example.com/"},"error":null}"#,
            "",
            true,
        );
        assert!(url.output.contains("https://example.com/"));
        let title = parse_action_outcome(
            "get_title",
            r#"{"success":true,"data":{"title":"Example"},"error":null}"#,
            "",
            true,
        );
        assert!(title.output.contains("Example"));
        let shot = parse_action_outcome(
            "screenshot",
            r#"{"success":true,"data":{"path":"C:\\tmp\\shot.png"},"error":null}"#,
            "",
            true,
        );
        assert_eq!(shot.output, "Screenshot saved to: C:\\tmp\\shot.png");
    }

    /// Live CLI isolation. Ignored by default: `cargo test` in this environment
    /// cannot reliably launch Chromium within the 30s BrowserTool timeout.
    /// Run with `--ignored` (or the manual agent-browser CLI) when a desktop
    /// browser runtime is available.
    #[tokio::test]
    #[ignore = "requires a live agent-browser + Chromium runtime"]
    async fn real_agent_browser_sessions_stay_isolated_across_rebuild() {
        let Some(search) = native_cli_search() else {
            eprintln!("skip runtime-dependent test: agent-browser unavailable");
            return;
        };

        let nonce = uuid::Uuid::new_v4();
        let logical_a = format!("w22-iso-a-{nonce}");
        let logical_b = format!("w22-iso-b-{nonce}");
        let mapped_a = browser_session_id(Some(&logical_a)).unwrap();
        let mapped_b = browser_session_id(Some(&logical_b)).unwrap();
        assert_ne!(mapped_a, mapped_b);
        validate_cli_session_name(&mapped_a).unwrap();
        validate_cli_session_name(&mapped_b).unwrap();

        let tool_a = BrowserTool::new(Vec::new(), true, false, None)
            .with_session(logical_a.clone())
            .with_binary_search(search.clone());
        let tool_b = BrowserTool::new(Vec::new(), true, false, None)
            .with_session(logical_b.clone())
            .with_binary_search(search.clone());

        let open_a = tool_a
            .execute(json!({"action": "open", "url": "https://example.com"}))
            .await
            .unwrap();
        let open_b = tool_b
            .execute(json!({"action": "open", "url": "https://www.iana.org/"}))
            .await
            .unwrap();

        let rebuilt_a = BrowserTool::new(Vec::new(), true, false, None)
            .with_session(logical_a)
            .with_binary_search(search);
        let url_a = rebuilt_a
            .execute(json!({"action": "get_url"}))
            .await
            .unwrap();
        let url_b = tool_b.execute(json!({"action": "get_url"})).await.unwrap();
        let snap_a = tool_a.execute(json!({"action": "snapshot"})).await.unwrap();
        let snap_b = tool_b.execute(json!({"action": "snapshot"})).await.unwrap();

        let _ = tool_a.execute(json!({"action": "close"})).await;
        let _ = tool_b.execute(json!({"action": "close"})).await;

        assert!(open_a.success, "open A failed: {:?}", open_a.error);
        assert!(open_b.success, "open B failed: {:?}", open_b.error);
        assert!(url_a.success, "get_url A failed: {:?}", url_a.error);
        assert!(url_b.success, "get_url B failed: {:?}", url_b.error);
        assert!(
            url_a.output.contains("https://example.com"),
            "rebuild A must keep example.com: {}",
            url_a.output
        );
        assert!(
            !url_a.output.contains("iana.org"),
            "A must not show B's page: {}",
            url_a.output
        );
        assert!(
            url_b.output.contains("https://www.iana.org"),
            "B must keep iana.org: {}",
            url_b.output
        );
        assert!(
            !url_b.output.contains("example.com"),
            "B must not show A's page: {}",
            url_b.output
        );
        assert!(snap_a.success, "snapshot A failed: {:?}", snap_a.error);
        assert!(snap_b.success, "snapshot B failed: {:?}", snap_b.error);
        assert!(
            snap_a.output.contains("example"),
            "snapshot A should describe example.com: {}",
            snap_a.output
        );
        assert!(
            snap_b.output.contains("iana.org") || snap_b.output.contains("IANA"),
            "snapshot B should describe iana.org: {}",
            snap_b.output
        );
    }

    /// W2.5 dynamic-page verification against a local JS page: web_fetch sees
    /// only the raw HTML (placeholder), while the browser sees the JS-rendered
    /// content; snapshot refs drive click and the URL changes afterwards.
    #[tokio::test]
    #[ignore = "requires a live agent-browser + Chromium runtime"]
    async fn real_browser_dynamic_page_extraction_and_interaction() {
        let Some(search) = native_cli_search() else {
            eprintln!("skip runtime-dependent test: agent-browser unavailable");
            return;
        };

        let page_port = crate::tools::web_client::tests::spawn_test_server(|req, stream| {
            let path = req.request_line.split(' ').nth(1).unwrap_or("/");
            if path.starts_with("/after-click") {
                crate::tools::web_client::tests::write_response(
                    stream,
                    "HTTP/1.1 200 OK",
                    "<html><head><title>After Click</title></head><body><p>navigated</p></body></html>",
                    &["content-type: text/html".to_string()],
                );
            } else {
                crate::tools::web_client::tests::write_response(
                    stream,
                    "HTTP/1.1 200 OK",
                    "<html><head><title>Static Fetch Title</title></head><body>\
                     <h1>W28 Dynamic Test</h1>\
                     <p id=\"jscontent\">Loading...</p>\
                     <a id=\"navlink\" href=\"/after-click\">go-w25</a>\
                     <script>setTimeout(() => { \
                     document.getElementById(\"jscontent\").textContent = \"Dynamic content loaded\"; \
                     document.title = \"JS-TITLE-42\"; }, 300);</script>\
                     </body></html>",
                    &["content-type: text/html".to_string()],
                );
            }
        });

        // Static view: web_fetch must NOT execute the script.
        let fetch_settings = crate::tools::web_client::WebToolSettings {
            proxy: Default::default(),
            request_timeout: std::time::Duration::from_secs(5),
            max_response_size: 1_048_576,
        };
        let fetch_tool = crate::tools::web_fetch::WebFetchTool::new(
            vec!["127.0.0.1".to_string()],
            fetch_settings,
        );
        let fetched = fetch_tool
            .execute(json!({"url": format!("http://127.0.0.1:{page_port}/page")}))
            .await
            .unwrap();
        assert!(fetched.success, "web_fetch failed: {:?}", fetched.error);
        assert!(
            fetched.output.contains("Loading..."),
            "static HTML must contain the placeholder: {}",
            fetched.output
        );
        assert!(
            !fetched.output.contains("Dynamic content loaded"),
            "web_fetch must not run JS: {}",
            fetched.output
        );

        let session = format!("w25-dynamic-{}", uuid::Uuid::new_v4());
        let tool = BrowserTool::new(vec!["127.0.0.1".to_string()], true, false, None)
            .with_session(session)
            .with_binary_search(search);
        let open = tool
            .execute(json!({"action": "open", "url": format!("http://127.0.0.1:{page_port}/page")}))
            .await
            .unwrap();
        assert!(open.success, "open failed: {:?}", open.error);

        let waited = tool
            .execute(json!({"action": "wait", "timeout_ms": 500}))
            .await
            .unwrap();
        assert!(waited.success, "wait failed: {:?}", waited.error);

        let text = tool.execute(json!({"action": "get_text"})).await.unwrap();
        assert!(text.success, "get_text failed: {:?}", text.error);
        assert!(
            text.output.contains("Dynamic content loaded"),
            "browser must see JS-rendered text: {}",
            text.output
        );

        let title = tool.execute(json!({"action": "get_title"})).await.unwrap();
        assert!(title.success);
        assert!(
            title.output.contains("JS-TITLE-42"),
            "browser title must reflect JS update: {}",
            title.output
        );

        let snapshot = tool
            .execute(json!({"action": "snapshot", "interactive_only": true}))
            .await
            .unwrap();
        assert!(snapshot.success, "snapshot failed: {:?}", snapshot.error);
        let reference = snapshot
            .output
            .lines()
            .find(|line| line.contains("go-w25"))
            .and_then(|line| {
                line.split("[ref=")
                    .nth(1)
                    .and_then(|rest| rest.split(']').next().map(|r| format!("@{r}")))
            })
            .expect("snapshot must expose a ref for the nav link");
        let clicked = tool
            .execute(json!({"action": "click", "selector": reference}))
            .await
            .unwrap();
        assert!(clicked.success, "click failed: {:?}", clicked.error);

        let url_after = tool.execute(json!({"action": "get_url"})).await.unwrap();
        assert!(url_after.success);
        assert!(
            url_after.output.contains("/after-click"),
            "click must navigate: {}",
            url_after.output
        );

        let _ = tool.execute(json!({"action": "close"})).await;
    }

    /// Domain-level structured snapshot: `data.refs` → `BrowserElement` → act.
    /// Ignored by default; run with `--ignored` when Chromium is available.
    #[tokio::test]
    #[ignore = "requires a live agent-browser + Chromium runtime"]
    async fn real_browser_structured_snapshot_elements_and_act() {
        let Some(search) = native_cli_search() else {
            eprintln!("skip runtime-dependent test: agent-browser unavailable");
            return;
        };

        let page_port = crate::tools::web_client::tests::spawn_test_server(|req, stream| {
            let path = req.request_line.split(' ').nth(1).unwrap_or("/");
            if path.starts_with("/pay") {
                crate::tools::web_client::tests::write_response(
                    stream,
                    "HTTP/1.1 200 OK",
                    "<html><body>\
                     <label>Card number <input type=\"text\" name=\"card\"></label>\
                     <button>Pay</button>\
                     </body></html>",
                    &["content-type: text/html".to_string()],
                );
            } else {
                crate::tools::web_client::tests::write_response(
                    stream,
                    "HTTP/1.1 200 OK",
                    "<html><head><title>B32B</title></head><body>\
                     <h1>Example</h1>\
                     <label><input type=\"checkbox\"> Remember me</label>\
                     <button>Submit</button>\
                     <button>Submit</button>\
                     <iframe src=\"/pay\" title=\"checkout\"></iframe>\
                     </body></html>",
                    &["content-type: text/html".to_string()],
                );
            }
        });

        let backend = Arc::new(AgentBrowserBackend::new(
            Some(search),
            BrowserSessionOptions::default(),
        ));
        let runtime = BrowserRuntime::new(
            backend,
            BrowserRuntimePolicy {
                allowed_domains: vec!["127.0.0.1".into()],
                ..BrowserRuntimePolicy::default()
            },
        );
        let key = BrowserSessionKey::new(format!("b32b-{}", uuid::Uuid::new_v4())).unwrap();
        let opts = BrowserSessionOptions::default();
        let open = runtime
            .open(
                &key,
                &opts,
                &NavigateRequest {
                    url: format!("http://127.0.0.1:{page_port}/page"),
                },
            )
            .await;
        assert!(open.is_ok(), "open failed: {open:?}");
        let _ = runtime
            .act(
                &key,
                &opts,
                &BrowserAction::Wait {
                    timeout_ms: Some(800),
                    text: None,
                    target: None,
                },
            )
            .await;

        let obs = runtime
            .observe(&key, &opts, &ObserveRequest::snapshot())
            .await
            .expect("snapshot observe");
        let snap = obs.snapshot.expect("snapshot payload");
        assert!(
            !snap.elements.is_empty(),
            "structured elements must be non-empty; text={}",
            snap.text
        );
        let buttons: Vec<_> = snap
            .elements
            .iter()
            .filter(|el| el.role.as_deref() == Some("button"))
            .collect();
        assert!(
            buttons
                .iter()
                .any(|el| el.reference.as_str().starts_with('e')),
            "button refs should be opaque eN handles: {:?}",
            buttons
                .iter()
                .map(|el| el.reference.as_str())
                .collect::<Vec<_>>()
        );
        let submits: Vec<_> = snap
            .elements
            .iter()
            .filter(|el| {
                el.role.as_deref() == Some("button")
                    && el
                        .name
                        .as_deref()
                        .is_some_and(|name| name.contains("Submit"))
            })
            .collect();
        assert_eq!(
            submits.len(),
            2,
            "two Submit buttons must stay distinct; elements={:?}",
            snap.elements
                .iter()
                .map(|el| (
                    el.reference.as_str(),
                    el.role.as_deref(),
                    el.name.as_deref()
                ))
                .collect::<Vec<_>>()
        );
        assert_ne!(submits[0].reference, submits[1].reference);
        assert!(submits.iter().all(|el| el.interactive));

        let clicked = runtime
            .act(
                &key,
                &opts,
                &BrowserAction::Click {
                    target: BrowserTarget::Element(submits[0].reference.clone()),
                },
            )
            .await
            .expect("click via BrowserElement.reference");
        assert!(
            clicked.detail.to_lowercase().contains("click")
                || clicked.detail.contains('@')
                || !clicked.detail.is_empty(),
            "click detail={}",
            clicked.detail
        );

        let card = snap
            .elements
            .iter()
            .find(|el| {
                el.role.as_deref() == Some("textbox")
                    && el.name.as_deref().is_some_and(|name| name.contains("Card"))
            })
            .unwrap_or_else(|| {
                panic!(
                    "iframe textbox must be a normal element ref; elements={:?}",
                    snap.elements
                        .iter()
                        .map(|el| (
                            el.reference.as_str(),
                            el.role.as_deref(),
                            el.name.as_deref()
                        ))
                        .collect::<Vec<_>>()
                )
            });
        let filled = runtime
            .act(
                &key,
                &opts,
                &BrowserAction::Fill {
                    target: BrowserTarget::Element(card.reference.clone()),
                    value: "4242".into(),
                },
            )
            .await
            .expect("iframe fill via ordinary element ref");
        assert!(
            filled.detail.to_lowercase().contains("fill")
                || filled.detail.contains('@')
                || !filled.detail.is_empty(),
            "fill detail={}",
            filled.detail
        );

        let _ = runtime.close_session(&key, &opts).await;
    }

    /// Live Chromium: document Read + stale element-ref diagnostic + re-observe.
    #[tokio::test]
    #[ignore = "requires a live agent-browser + Chromium runtime"]
    async fn real_browser_read_and_stale_ref_diagnostics() {
        let Some(search) = native_cli_search() else {
            eprintln!("skip runtime-dependent test: agent-browser unavailable");
            return;
        };

        let page_port = crate::tools::web_client::tests::spawn_test_server(|req, stream| {
            let path = req.request_line.split(' ').nth(1).unwrap_or("/");
            if path.starts_with("/after-click") {
                crate::tools::web_client::tests::write_response(
                    stream,
                    "HTTP/1.1 200 OK",
                    "<html><body><p>after click</p></body></html>",
                    &["content-type: text/html".to_string()],
                );
            } else {
                crate::tools::web_client::tests::write_response(
                    stream,
                    "HTTP/1.1 200 OK",
                    "<html><head><title>B32C Readable</title></head><body>\
                     <article>\
                     <h1>Security Handbook</h1>\
                     <p>This article discusses authentication and security practices.</p>\
                     <h2>Auth section</h2>\
                     <p>Use unique credentials on every service.</p>\
                     <h2>Cooking</h2>\
                     <p>Pasta recipes are unrelated filler text.</p>\
                     </article>\
                     <button id=\"delete-me\">DeleteMeUniqueB32C</button>\
                     <a id=\"keep-me\" href=\"/after-click\">KeepMeUniqueB32C</a>\
                     </body></html>",
                    &["content-type: text/html".to_string()],
                );
            }
        });

        let backend = Arc::new(AgentBrowserBackend::new(
            Some(search),
            BrowserSessionOptions::default(),
        ));
        let runtime = BrowserRuntime::new(
            backend,
            BrowserRuntimePolicy {
                allowed_domains: vec!["127.0.0.1".into()],
                ..BrowserRuntimePolicy::default()
            },
        );
        let key = BrowserSessionKey::new(format!("b32c-{}", uuid::Uuid::new_v4())).unwrap();
        let opts = BrowserSessionOptions::default();
        let open = runtime
            .open(
                &key,
                &opts,
                &NavigateRequest {
                    url: format!("http://127.0.0.1:{page_port}/page"),
                },
            )
            .await;
        assert!(open.is_ok(), "open failed: {open:?}");
        let _ = runtime
            .act(
                &key,
                &opts,
                &BrowserAction::Wait {
                    timeout_ms: Some(800),
                    text: None,
                    target: None,
                },
            )
            .await;

        let read = runtime
            .observe(&key, &opts, &ObserveRequest::read(false, None))
            .await
            .expect("read observe");
        let read_text = read.text.as_deref().unwrap_or("");
        assert!(
            read_text.contains("BEGIN WEB CONTENT"),
            "read must reuse WEB CONTENT bounds: {read_text}"
        );
        assert!(
            read_text.to_ascii_lowercase().contains("security")
                || read_text.to_ascii_lowercase().contains("authentication"),
            "read content missing article body: {read_text}"
        );
        assert!(read.snapshot.is_none());
        assert!(
            read.url
                .as_deref()
                .is_some_and(|url| url.contains("127.0.0.1")),
            "read should prefer page url, got {:?}",
            read.url
        );

        let outline = runtime
            .observe(&key, &opts, &ObserveRequest::read(true, None))
            .await
            .expect("read outline");
        let outline_text = outline.text.as_deref().unwrap_or("");
        assert!(
            outline_text.to_ascii_lowercase().contains("outline")
                || outline_text.contains("Security Handbook")
                || outline_text.contains("Auth"),
            "outline missing headings: {outline_text}"
        );

        let filtered = runtime
            .observe(
                &key,
                &opts,
                &ObserveRequest::read(false, Some("security".into())),
            )
            .await
            .expect("read filter");
        let filtered_text = filtered.text.as_deref().unwrap_or("");
        assert!(
            filtered_text.to_ascii_lowercase().contains("security"),
            "filtered read should keep the filter term: {filtered_text}"
        );

        let snap = runtime
            .observe(&key, &opts, &ObserveRequest::snapshot())
            .await
            .expect("snapshot")
            .snapshot
            .expect("structured snapshot");
        let delete = snap
            .elements
            .iter()
            .find(|el| {
                el.name
                    .as_deref()
                    .is_some_and(|name| name.contains("DeleteMeUniqueB32C"))
            })
            .unwrap_or_else(|| {
                panic!(
                    "DeleteMeUniqueB32C missing; elements={:?}",
                    snap.elements
                        .iter()
                        .map(|el| (el.reference.as_str(), el.name.as_deref()))
                        .collect::<Vec<_>>()
                )
            });
        let old_ref = delete.reference.clone();

        let removed = runtime
            .act(
                &key,
                &opts,
                &BrowserAction::Eval {
                    script: "document.getElementById('delete-me').remove(); true".into(),
                },
            )
            .await;
        assert!(removed.is_ok(), "eval remove failed: {removed:?}");
        let _ = runtime
            .act(
                &key,
                &opts,
                &BrowserAction::Wait {
                    timeout_ms: Some(400),
                    text: None,
                    target: None,
                },
            )
            .await;

        let stale = runtime
            .act(
                &key,
                &opts,
                &BrowserAction::Click {
                    target: BrowserTarget::Element(old_ref.clone()),
                },
            )
            .await
            .expect_err("old ref must fail after DOM removal");
        assert_eq!(
            stale.kind,
            BrowserErrorKind::StaleReference,
            "stale click classified as {:?}: {}",
            stale.kind,
            stale.detail
        );
        assert_ne!(stale.kind, BrowserErrorKind::SessionNotFound);
        assert!(!stale.retryable);
        let presented = present_backend_error(&stale);
        assert!(
            presented.starts_with("BrowserCommandFailed:"),
            "model prefix changed: {presented}"
        );
        assert!(
            presented.contains("snapshot again"),
            "re-observe guidance missing: {presented}"
        );
        let health = runtime.session_health(&key, &opts).await;
        assert!(
            matches!(health, BrowserHealth::Healthy),
            "session must stay healthy after stale ref: {health:?}"
        );

        let snap2 = runtime
            .observe(&key, &opts, &ObserveRequest::snapshot())
            .await
            .expect("re-observe snapshot")
            .snapshot
            .expect("re-observe payload");
        assert!(
            snap2.elements.iter().all(|el| el.reference != old_ref || {
                !el.name
                    .as_deref()
                    .is_some_and(|name| name.contains("DeleteMeUniqueB32C"))
            }),
            "removed unique button must not reappear under the old handle"
        );
        let keep = snap2
            .elements
            .iter()
            .find(|el| {
                el.name
                    .as_deref()
                    .is_some_and(|name| name.contains("KeepMeUniqueB32C"))
            })
            .unwrap_or_else(|| {
                panic!(
                    "KeepMeUniqueB32C missing after re-observe; elements={:?}",
                    snap2
                        .elements
                        .iter()
                        .map(|el| (el.reference.as_str(), el.name.as_deref()))
                        .collect::<Vec<_>>()
                )
            });
        let clicked = runtime
            .act(
                &key,
                &opts,
                &BrowserAction::Click {
                    target: BrowserTarget::Element(keep.reference.clone()),
                },
            )
            .await
            .expect("click via fresh ref after re-observe");
        assert!(
            clicked.detail.to_lowercase().contains("click")
                || clicked.detail.contains('@')
                || !clicked.detail.is_empty(),
            "click detail={}",
            clicked.detail
        );
        let url_after = runtime
            .observe(
                &key,
                &opts,
                &ObserveRequest {
                    kind: BrowserObserveKind::Url,
                    interactive_only: false,
                    compact: false,
                },
            )
            .await
            .expect("get url after re-observe click");
        assert!(
            url_after
                .text
                .as_deref()
                .or(url_after.url.as_deref())
                .is_some_and(|url| url.contains("/after-click")),
            "fresh ref click should navigate: text={:?} url={:?}",
            url_after.text,
            url_after.url
        );

        let _ = runtime.close_session(&key, &opts).await;
    }
}
