use crate::tools::browser_bin::{AgentBrowserBinaryResolved, BrowserBinarySearch};
use crate::tools::browser_lifecycle::{
    auto_retry_allowed, classify_browser_output, failure_prefix, forget_owned_browser_session,
    is_retryable_action, recover_owned_session, remember_owned_browser_session,
    run_command_with_timeout, BrowserFailureKind, ChildRunError, AGENT_BROWSER_NAMESPACE,
};
use crate::tools::browser_output::{cli_max_output_for_action, parse_action_outcome};
use crate::tools::configure_background_command;
use crate::tools::traits::{Tool, ToolResult};
use crate::tools::web_client::{host_matches_allowlist, redact_secrets_in_text};
use async_trait::async_trait;
use serde_json::json;
use std::io::ErrorKind;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;
#[cfg(test)]
use tokio::time::{timeout, Duration};
use url::Url;

const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Prefix used for every agent-browser session created by OmniNova.
const BROWSER_SESSION_PREFIX: &str = "omninova";
/// Number of hex characters taken from the SHA-256 digest.
const BROWSER_SESSION_HASH_CHARS: usize = 20;

/// Map an OmniNova session id to a stable, shell-safe agent-browser session id.
///
/// `None` / blank ids are a hard error. There is no default, anonymous, or
/// shared-session fallback — those would collapse every session-less agent
/// onto the same agent-browser session.
pub fn browser_session_id(session_id: Option<&str>) -> Result<String, String> {
    let Some(raw) = session_id else {
        return Err(browser_session_missing_error());
    };
    if raw.trim().is_empty() {
        return Err(browser_session_missing_error());
    }
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(raw.as_bytes());
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let hex = &hex[..BROWSER_SESSION_HASH_CHARS.min(hex.len())];
    Ok(format!("{BROWSER_SESSION_PREFIX}-{hex}"))
}

fn browser_session_missing_error() -> String {
    "BrowserSessionMissing: OmniNova session id is required; refusing to use a default or shared agent-browser session. Do not retry this call without a valid session."
        .to_string()
}

/// Only http(s) navigation is allowed. Bare hosts like `example.com` are
/// treated as `https://example.com`. `file:`, `javascript:`, `data:`,
/// `chrome:`, `chrome-extension:`, and `about:` are rejected.
fn parse_browser_open_url(raw: &str) -> Result<Url, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("BrowserUrlRejected: URL is empty".into());
    }
    let parsed = match Url::parse(trimmed) {
        Ok(url) => url,
        Err(_) => Url::parse(&format!("https://{trimmed}"))
            .map_err(|e| format!("BrowserUrlRejected: invalid URL ({e})"))?,
    };
    match parsed.scheme() {
        "http" | "https" => {
            if parsed.host_str().is_none() {
                return Err("BrowserUrlRejected: URL has no host".into());
            }
            Ok(parsed)
        }
        other => Err(format!(
            "BrowserUrlRejected: scheme '{other}' is not allowed; only http/https"
        )),
    }
}

const BROWSER_SESSION_MAX_LEN: usize = 64;

/// Reject names that would fall back to agent-browser's default session or
/// that are unsafe to pass as a CLI argument.
fn validate_cli_session_name(session: &str) -> Result<(), String> {
    if session.is_empty() {
        return Err(
            "BrowserSessionInvalid: session name is empty; refusing to use the agent-browser default session"
                .into(),
        );
    }
    if session.eq_ignore_ascii_case("default") {
        return Err(
            "BrowserSessionInvalid: refusing to use the agent-browser default session".into(),
        );
    }
    if session.len() > BROWSER_SESSION_MAX_LEN {
        return Err(format!(
            "BrowserSessionInvalid: session name exceeds {BROWSER_SESSION_MAX_LEN} characters"
        ));
    }
    if !session
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(
            "BrowserSessionInvalid: session name contains characters that are not safe to pass to the agent-browser CLI"
                .into(),
        );
    }
    Ok(())
}

pub struct BrowserTool {
    allowed_domains: Vec<String>,
    headless: bool,
    attach_only: bool,
    cdp_url: Option<String>,
    session: Option<String>,
    search: Option<BrowserBinarySearch>,
}

fn spawn_error_message(path: &Path, e: std::io::Error) -> String {
    let requested = path.to_string_lossy();
    match e.kind() {
        ErrorKind::NotFound => format!(
            "BrowserBinaryMissing: requested_binary={requested} resolution_source=launch checked_candidates={requested}"
        ),
        ErrorKind::PermissionDenied => format!(
            "BrowserBinaryNotExecutable: requested_binary={requested} detail={e}"
        ),
        _ => format!("BrowserLaunchFailed: requested_binary={requested} detail={e}"),
    }
}

impl BrowserTool {
    async fn readable_page_after_timeout(&self, target: &str) -> Option<ToolResult> {
        // Fixed read-only probe. Never replay clicks, submits, or arbitrary model JS.
        const PROBE: &str = "JSON.stringify({url:location.href,title:document.title,readyState:document.readyState,text:document.body?document.body.innerText.slice(0,6000):''})";
        let (ok, stdout, _, _) = self.run_agent_browser("_navigation_probe", &["eval", PROBE]).await.ok()?;
        if !ok { return None; }
        readable_navigation_probe(target, &stdout)
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
            search: None,
        }
    }

    #[cfg(test)]
    fn with_binary_search(mut self, search: BrowserBinarySearch) -> Self {
        self.search = Some(search);
        self
    }

    pub fn with_session(mut self, session: impl Into<String>) -> Self {
        self.session = Some(session.into());
        self
    }

    #[cfg(test)]
    pub(crate) fn session_id(&self) -> Option<&str> {
        self.session.as_deref()
    }

    fn is_domain_allowed(&self, url: &Url) -> bool {
        url.host_str()
            .is_some_and(|host| host_matches_allowlist(host, &self.allowed_domains))
    }

    fn resolve_binary(&self) -> Result<AgentBrowserBinaryResolved, String> {
        let search = self
            .search
            .clone()
            .unwrap_or_else(BrowserBinarySearch::from_process);
        search.resolve().map_err(|missing| missing.to_string())
    }

    async fn run_agent_browser(
        &self,
        action: &str,
        args: &[&str],
    ) -> anyhow::Result<(bool, String, String, std::path::PathBuf)> {
        let session = self
            .session
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!(browser_session_missing_error()))?;
        validate_cli_session_name(session).map_err(anyhow::Error::msg)?;
        let resolved = self.resolve_binary().map_err(anyhow::Error::msg)?;
        let mut cmd = Command::new(&resolved.path);
        configure_background_command(&mut cmd);

        if !self.attach_only && self.cdp_url.is_none() {
            if let Some(browser) = super::browser_executable::installed_browser() {
                cmd.arg("--executable-path").arg(browser);
            }
        }

        if self.headless {
            // headless is the default for agent-browser, no flag needed
        } else {
            cmd.arg("--headed");
        }

        cmd.arg("--session").arg(session);
        cmd.arg("--namespace").arg(AGENT_BROWSER_NAMESPACE);

        tracing::debug!(
            target: "browser",
            browser_session = session,
            "spawning agent-browser command"
        );

        if self.attach_only {
            cmd.arg("--attach-only");
        }

        if let Some(cdp_url) = &self.cdp_url {
            cmd.arg("--cdp-url").arg(cdp_url);
        }

        cmd.arg("--json");

        if let Some(max_output) = cli_max_output_for_action(action) {
            cmd.arg("--max-output").arg(max_output);
        }

        for arg in args {
            cmd.arg(arg);
        }

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        remember_owned_browser_session(session);

        let timeout_secs = if action == "_navigation_probe" { 5 } else { DEFAULT_TIMEOUT_SECS };
        let output = match run_command_with_timeout(cmd, timeout_secs).await {
            Ok(output) => output,
            Err(ChildRunError::Timeout { .. }) => {
                return Err(anyhow::anyhow!(
                    "BrowserCommandTimeout: requested_binary={} timeout_secs={DEFAULT_TIMEOUT_SECS}. Browser startup or navigation timed out; this is not proof that network/shell access is forbidden. If no local Edge/Chrome is installed, install Chromium via browser setup; otherwise check the destination site's connectivity.",
                    resolved.path.display()
                ));
            }
            Err(ChildRunError::Io(e)) => {
                return Err(anyhow::anyhow!(spawn_error_message(&resolved.path, e)));
            }
        };

        // stdout carries the JSON payload; stderr is diagnostics only and is
        // never merged into successful model content (W2.5).
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok((output.status.success(), stdout, stderr, resolved.path))
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

        let selector = args.get("selector").and_then(|v| v.as_str());
        let value = args.get("value").and_then(|v| v.as_str());
        let url = args.get("url").and_then(|v| v.as_str());

        let cli_args: Vec<String> = match action {
            "open" => {
                let target_url = url
                    .or(value)
                    .ok_or_else(|| anyhow::anyhow!("'open' requires 'url' parameter"))?;
                let parsed = match parse_browser_open_url(target_url) {
                    Ok(parsed) => parsed,
                    Err(err) => {
                        return Ok(ToolResult {
                            success: false,
                            output: String::new(),
                            error: Some(err),
                        });
                    }
                };
                if !self.is_domain_allowed(&parsed) {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(format!(
                            "BrowserUrlRejected: Domain not in allowed list: {}",
                            redact_secrets_in_text(parsed.as_str())
                        )),
                    });
                }
                vec!["open".into(), parsed.to_string()]
            }

            "snapshot" => {
                let mut a = vec!["snapshot".to_string()];
                if args
                    .get("interactive_only")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                {
                    a.push("-i".into());
                }
                if args
                    .get("compact")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                {
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

            "get_text" => match selector {
                Some(sel) => vec!["get".into(), "text".into(), sel.into()],
                // No selector: whole-page readable text via agent-browser's
                // built-in extraction (works for JS-rendered pages).
                None => vec!["read".into()],
            },

            "get_html" => {
                let sel =
                    selector.ok_or_else(|| anyhow::anyhow!("'get_html' requires 'selector'"))?;
                vec!["get".into(), "html".into(), sel.into()]
            }

            "get_value" => {
                let sel =
                    selector.ok_or_else(|| anyhow::anyhow!("'get_value' requires 'selector'"))?;
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
                let sel =
                    selector.ok_or_else(|| anyhow::anyhow!("'select' requires 'selector'"))?;
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
                let js = value
                    .ok_or_else(|| anyhow::anyhow!("'eval' requires 'value' (JavaScript code)"))?;
                vec!["eval".into(), js.into()]
            }

            "back" => vec!["back".into()],
            "forward" => vec!["forward".into()],
            "reload" => vec!["reload".into()],
            "close" => vec!["close".into()],

            "is_visible" => {
                let sel =
                    selector.ok_or_else(|| anyhow::anyhow!("'is_visible' requires 'selector'"))?;
                vec!["is".into(), "visible".into(), sel.into()]
            }

            "is_enabled" => {
                let sel =
                    selector.ok_or_else(|| anyhow::anyhow!("'is_enabled' requires 'selector'"))?;
                vec!["is".into(), "enabled".into(), sel.into()]
            }

            "find" => {
                let role = args
                    .get("find_role")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("'find' requires 'find_role'"))?;
                let find_action = value.unwrap_or("text");
                let mut a = vec![
                    "find".into(),
                    "role".into(),
                    role.into(),
                    find_action.into(),
                ];
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
        let retryable = is_retryable_action(action);
        let mut recovered = false;
        let mut concurrent_followup = false;
        loop {
            match self.run_agent_browser(action, &str_args).await {
                Ok((exit_success, stdout, stderr, binary_path)) => {
                    let outcome = parse_action_outcome(action, &stdout, &stderr, exit_success);
                    if outcome.success {
                        if action == "close" {
                            if let Some(session) = self.session.as_deref() {
                                forget_owned_browser_session(session);
                            }
                        }
                        return Ok(ToolResult {
                            success: true,
                            output: outcome.output,
                            error: None,
                        });
                    }

                    // Classification and recovery run on the error text plus
                    // stderr diagnostics, not on normalized content.
                    let diagnostic = format!(
                        "{} {}",
                        outcome.error_text.as_deref().unwrap_or_default(),
                        stderr
                    );
                    if action == "open" && diagnostic.to_ascii_lowercase().contains("timed out") {
                        if let Some(result) = self.readable_page_after_timeout(&cli_args[1]).await {
                            return Ok(result);
                        }
                    }
                    let kind = classify_browser_output(&diagnostic);
                    if auto_retry_allowed(action, recovered, concurrent_followup, kind, &diagnostic)
                    {
                        if !recovered {
                            if let Some(session) = self.session.as_deref() {
                                recover_owned_session(session);
                            }
                            recovered = true;
                        } else {
                            concurrent_followup = true;
                        }
                        continue;
                    }

                    if action == "close"
                        && matches!(
                            kind,
                            BrowserFailureKind::DaemonUnavailable
                                | BrowserFailureKind::SessionUnavailable
                        )
                    {
                        if let Some(session) = self.session.as_deref() {
                            recover_owned_session(session);
                            forget_owned_browser_session(session);
                        }
                        return Ok(ToolResult {
                            success: true,
                            output: outcome.output,
                            error: None,
                        });
                    }

                    let prefix = failure_prefix(kind);
                    let detail = outcome
                        .error_text
                        .unwrap_or_else(|| "command failed".to_string());
                    let summary: String =
                        redact_secrets_in_text(&detail).chars().take(1200).collect();
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(format!(
                            "{prefix}: requested_binary={} summary={summary}",
                            binary_path.display(),
                        )),
                    });
                }
                Err(e) => {
                    let msg = e.to_string();
                    if action == "open" && msg.starts_with("BrowserCommandTimeout:") {
                        if let Some(result) = self.readable_page_after_timeout(&cli_args[1]).await {
                            return Ok(result);
                        }
                    }
                    if retryable && !recovered && msg.starts_with("BrowserCommandTimeout:") {
                        if let Some(session) = self.session.as_deref() {
                            if recover_owned_session(session) {
                                recovered = true;
                                continue;
                            }
                        }
                    }
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(msg),
                    });
                }
            }
        }
    }

    #[cfg(test)]
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
}

fn readable_navigation_probe(target: &str, stdout: &str) -> Option<ToolResult> {
    let envelope: serde_json::Value = serde_json::from_str(stdout).ok()?;
    if envelope.get("success")?.as_bool()? != true { return None; }
    let data: serde_json::Value = serde_json::from_str(envelope.get("data")?.get("result")?.as_str()?).ok()?;
    let current = Url::parse(data.get("url")?.as_str()?).ok()?;
    let requested = Url::parse(target).ok()?;
    // Do not mistake a previous page, redirect to another site, or Chrome error page for success.
    if current.origin() != requested.origin() || current.path() != requested.path()
        || current.query() != requested.query()
        || !matches!(data.get("readyState")?.as_str()?, "interactive" | "complete") { return None; }
    let body = data.get("text")?.as_str()?.trim();
    if body.is_empty() { return None; }
    Some(ToolResult {
        success: true,
        output: format!("Navigation warning: the load wait timed out, but the requested page is readable. Some resources may be incomplete; use snapshot for interactive controls.\nURL: {}\n--- BEGIN WEB CONTENT ---\nTitle: {}\n{}\n--- END WEB CONTENT ---",
            redact_secrets_in_text(current.as_str()), data.get("title")?.as_str()?, body.chars().take(6000).collect::<String>()),
        error: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::browser_bin::{bundled_agent_browser_relative_path, BrowserBinarySearch};
    use serde_json::json;

    #[test]
    fn navigation_timeout_probe_requires_readable_requested_page() {
        let response = |url: &str, state: &str, text: &str| json!({"success":true,"data":{"result":json!({
            "url":url,"title":"Fixture","readyState":state,"text":text
        }).to_string()}}).to_string();
        assert!(readable_navigation_probe("https://example.com/", &response("https://example.com/", "complete", "正文")).is_some());
        assert!(readable_navigation_probe("https://example.com/", &response("https://example.com/old", "complete", "旧页面")).is_none());
        assert!(readable_navigation_probe("https://example.com/", &response("https://example.com/", "loading", "正文")).is_none());
        assert!(readable_navigation_probe("https://example.com/", &response("chrome-error://chromewebdata/", "complete", "错误")).is_none());
    }

    #[tokio::test]
    #[ignore = "requires installed browser runtime; manually run smoke test"]
    async fn browser_live_smoke() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                tokio::spawn(async move {
                    let mut request = [0u8;4096];
                    let _ = stream.read(&mut request).await;
                    let body = "<!doctype html><title>OmniNova Browser QA</title><h1>Browser working</h1><p>Independent test session.</p>";
                    let _ = stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await;
                });
            }
        });
        let tool = BrowserTool::new(vec!["127.0.0.1".into()], true, false, None)
            .with_session(format!("omninova-qa-{}",uuid::Uuid::new_v4().simple()));
        let result = tool.execute(json!({"action":"open","url":format!("http://{addr}/")})).await.unwrap();
        let snapshot = tool.execute(json!({"action":"snapshot","compact":true})).await.unwrap();
        let _ = tool.execute(json!({"action":"close"})).await;
        server.abort();
        assert!(result.success, "{:?}", result.error);
        assert!(snapshot.success && snapshot.output.contains("Browser working"), "{:?} {}",snapshot.error,snapshot.output);
    }

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
    async fn resolved_native_version_does_not_hang_under_create_no_window() {
        let Ok(resolved) = BrowserBinarySearch::from_process().resolve() else {
            eprintln!("skip: agent-browser unavailable");
            return;
        };
        let ext = resolved
            .path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");
        assert!(
            !ext.eq_ignore_ascii_case("cmd") && !ext.eq_ignore_ascii_case("bat"),
            "resolver must return a native binary, got {}",
            resolved.path.display()
        );

        let mut cmd = Command::new(&resolved.path);
        configure_background_command(&mut cmd);
        cmd.arg("--version")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let output = timeout(Duration::from_secs(15), cmd.output())
            .await
            .expect("native --version must not hang under CREATE_NO_WINDOW")
            .expect("spawn native --version");
        assert!(
            output.status.success(),
            "stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(
            text.to_lowercase().contains("agent-browser") || text.contains("0."),
            "unexpected --version output: {text}"
        );
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
    async fn unsafe_or_default_session_names_are_rejected() {
        for name in ["default", "Default", "foo;rm", "bad name", ""] {
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
                error.starts_with("BrowserSessionMissing:")
                    || error.starts_with("BrowserSessionInvalid:"),
                "name={name:?} error={error}"
            );
            assert!(
                !error.contains("BrowserBinaryMissing"),
                "session guard should run before binary resolution: {error}"
            );
        }
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
        let mapped_a = browser_session_id(Some(&format!("w22-iso-a-{nonce}"))).unwrap();
        let mapped_b = browser_session_id(Some(&format!("w22-iso-b-{nonce}"))).unwrap();
        assert_ne!(mapped_a, mapped_b);
        validate_cli_session_name(&mapped_a).unwrap();
        validate_cli_session_name(&mapped_b).unwrap();

        let tool_a = BrowserTool::new(Vec::new(), true, false, None)
            .with_session(mapped_a.clone())
            .with_binary_search(search.clone());
        let tool_b = BrowserTool::new(Vec::new(), true, false, None)
            .with_session(mapped_b.clone())
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
            .with_session(mapped_a.clone())
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

        // Dynamic view: browser renders and executes the script.
        let session = browser_session_id(Some(&format!("w25-dynamic-{}", uuid::Uuid::new_v4())))
            .expect("mapped session");
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

        // Snapshot refs survive normalization and drive interaction.
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
}
