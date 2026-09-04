//! Normalizes agent-browser `--json` output into bounded, model-facing text.
//!
//! Pipeline contract (W2.5):
//!   stdout (bounded) → JSON parse → per-action field extraction →
//!   semantic char budgeting.
//! Raw byte truncation of JSON is forbidden; stderr is diagnostics only and
//! never concatenated into successful model content.

use crate::tools::text_bound::bound_head;
use crate::tools::web_client::redact_secrets_in_text;
use serde_json::Value;

/// Char budgets per action class. Snapshot keeps the largest budget because it
/// is the interactive operating surface (refs must survive truncation).
pub const BROWSER_SNAPSHOT_CHAR_LIMIT: usize = 24_000;
pub const BROWSER_TEXT_CHAR_LIMIT: usize = 20_000;
pub const BROWSER_OP_CHAR_LIMIT: usize = 4_000;

/// Hard cap on bytes captured from one agent-browser process stream
/// (ProcessOutputLimit). Applied at capture time in `browser_lifecycle`.
pub const BROWSER_PROCESS_OUTPUT_LIMIT_BYTES: usize = 8 * 1024 * 1024;

/// `--max-output` values forwarded to agent-browser for content-bearing
/// actions so the CLI truncates at the source too.
pub fn cli_max_output_for_action(action: &str) -> Option<&'static str> {
    match action {
        "snapshot" => Some("24000"),
        "get_text" | "get_html" => Some("20000"),
        _ => None,
    }
}

pub struct BrowserOutcome {
    /// Whether the command should be treated as successful.
    pub success: bool,
    /// Bounded, normalized model-facing content.
    pub output: String,
    /// Raw error text (JSON `error` field, falling back to stderr).
    pub error_text: Option<String>,
    /// False when stdout was not valid JSON (degraded raw-output mode).
    pub json_valid: bool,
}

/// Parses and normalizes one agent-browser invocation.
pub fn parse_action_outcome(
    action: &str,
    stdout: &str,
    stderr: &str,
    exit_success: bool,
) -> BrowserOutcome {
    let trimmed = stdout.trim();
    let parsed: Option<Value> = if trimmed.is_empty() {
        None
    } else {
        serde_json::from_str(trimmed).ok()
    };

    match parsed {
        Some(value) => {
            let json_success = value
                .get("success")
                .and_then(Value::as_bool)
                .unwrap_or(exit_success);
            let error_text = value
                .get("error")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|e| !e.is_empty())
                .map(str::to_string)
                .or_else(|| {
                    let stderr_trim = stderr.trim();
                    (!stderr_trim.is_empty()).then(|| stderr_trim.to_string())
                });

            if json_success {
                let data = value.get("data").cloned().unwrap_or(Value::Null);
                BrowserOutcome {
                    success: true,
                    output: normalize_action(action, &data),
                    error_text: None,
                    json_valid: true,
                }
            } else {
                BrowserOutcome {
                    success: false,
                    output: String::new(),
                    error_text: error_text.or_else(|| Some("command failed".to_string())),
                    json_valid: true,
                }
            }
        }
        None => {
            // Degraded mode: agent-browser did not emit JSON (old version,
            // crash mid-write, or plain-text output). Surface the raw text
            // bounded instead of failing, with an explicit marker.
            let raw = if trimmed.is_empty() {
                stderr.trim().to_string()
            } else {
                trimmed.to_string()
            };
            let mut output = String::from("[InvalidBrowserOutput: agent-browser returned non-JSON output; showing bounded raw text]\n");
            output.push_str(&bound_head(&raw, BROWSER_OP_CHAR_LIMIT));
            BrowserOutcome {
                success: exit_success,
                output,
                error_text: (!exit_success).then(|| bound_head(&raw, BROWSER_OP_CHAR_LIMIT)),
                json_valid: false,
            }
        }
    }
}

fn wrap_web_content(body: &str) -> String {
    format!("--- BEGIN WEB CONTENT ---\n{body}\n--- END WEB CONTENT ---")
}

fn push_url_line(out: &mut String, url: &str) {
    out.push_str("URL: ");
    out.push_str(&redact_secrets_in_text(url));
    out.push('\n');
}

fn data_string(data: &Value, keys: &[&str]) -> Option<String> {
    let object = data.as_object()?;
    for key in keys {
        if let Some(text) = object.get(*key).and_then(Value::as_str) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn lifecycle_field(data: &Value, key: &str) -> Option<String> {
    data.get("lifecycle")?
        .get(key)?
        .as_bool()
        .map(|b| b.to_string())
}

/// Compact rendering of `data` without the noisy `lifecycle` block.
fn compact_data(data: &Value) -> String {
    let mut object = match data.as_object() {
        Some(object) => object.clone(),
        None => return "OK".to_string(),
    };
    object.remove("lifecycle");
    if object.is_empty() {
        return "OK".to_string();
    }
    let rendered = Value::Object(object).to_string();
    bound_head(&rendered, BROWSER_OP_CHAR_LIMIT)
}

/// Action-specific normalization. The snapshot text is passed through
/// verbatim (only bounded) so `@eN` refs are never rewritten, deduplicated,
/// or reordered.
pub fn normalize_action(action: &str, data: &Value) -> String {
    match action {
        "open" => {
            let mut out = String::new();
            if let Some(url) = data_string(data, &["url"]) {
                push_url_line(&mut out, &url);
            }
            if let Some(title) = data_string(data, &["title"]) {
                out.push_str("Title: ");
                out.push_str(&title);
                out.push('\n');
            }
            match (
                lifecycle_field(data, "launched"),
                lifecycle_field(data, "reused"),
            ) {
                (Some(launched), Some(reused)) => {
                    out.push_str(&format!(
                        "Browser session: launched={launched} reused={reused}"
                    ));
                }
                _ => {
                    if out.is_empty() {
                        out.push_str("Page opened.");
                    }
                }
            }
            if out.is_empty() {
                compact_data(data)
            } else {
                out
            }
        }
        "get_url" => data_string(data, &["url"])
            .map(|url| redact_secrets_in_text(&url))
            .unwrap_or_else(|| compact_data(data)),
        "get_title" => data_string(data, &["title"]).unwrap_or_else(|| compact_data(data)),
        "get_text" => {
            let text = data_string(data, &["text", "content"]).unwrap_or_default();
            if text.is_empty() {
                compact_data(data)
            } else {
                wrap_web_content(&bound_head(&text, BROWSER_TEXT_CHAR_LIMIT))
            }
        }
        "read" => {
            let text = data_string(data, &["content", "text"]).unwrap_or_default();
            if text.is_empty() {
                compact_data(data)
            } else {
                wrap_web_content(&bound_head(&text, BROWSER_TEXT_CHAR_LIMIT))
            }
        }
        "get_html" => {
            let text = data_string(data, &["html", "content"]).unwrap_or_default();
            if text.is_empty() {
                compact_data(data)
            } else {
                wrap_web_content(&bound_head(&text, BROWSER_TEXT_CHAR_LIMIT))
            }
        }
        "get_value" => data_string(data, &["value", "text"]).unwrap_or_else(|| compact_data(data)),
        "snapshot" => {
            let snapshot = data_string(data, &["snapshot"]).unwrap_or_default();
            if snapshot.is_empty() {
                return compact_data(data);
            }
            let mut out = String::new();
            if let Some(origin) = data_string(data, &["origin", "url"]) {
                push_url_line(&mut out, &origin);
                out.push('\n');
            }
            out.push_str(&wrap_web_content(&bound_head(
                &snapshot,
                BROWSER_SNAPSHOT_CHAR_LIMIT,
            )));
            out
        }
        "screenshot" => match data_string(data, &["path", "file", "screenshot"]) {
            Some(path) => format!("Screenshot saved to: {path}"),
            None => compact_data(data),
        },
        "close" => "Browser session closed.".to_string(),
        // Interaction/state actions keep their small op-result surface. No
        // follow-up page snapshot is attached automatically; agents call
        // `get_url` / `snapshot` explicitly when they need new state.
        _ => {
            let verb = match action {
                "click" => Some("Clicked"),
                "fill" => Some("Filled"),
                "type" => Some("Typed"),
                "select" => Some("Selected"),
                "press" => Some("Pressed"),
                "hover" => Some("Hovered"),
                _ => None,
            };
            let known = data_string(
                data,
                &[
                    "clicked", "filled", "typed", "selected", "pressed", "hovered", "value",
                    "text", "result", "url",
                ],
            );
            match known {
                Some(value) => {
                    let bounded = bound_head(&value, BROWSER_OP_CHAR_LIMIT);
                    match verb {
                        Some(verb) => format!("{verb} {bounded}."),
                        None => bounded,
                    }
                }
                None => compact_data(data),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn valid_json_open_is_normalized() {
        let stdout = r#"{"success":true,"data":{"url":"https://example.com/","title":"Example Domain","lifecycle":{"launched":true,"reused":false}},"error":null}"#;
        let outcome = parse_action_outcome("open", stdout, "", true);
        assert!(outcome.success);
        assert!(outcome.json_valid);
        assert_eq!(
            outcome.output,
            "URL: https://example.com/\nTitle: Example Domain\nBrowser session: launched=true reused=false"
        );
    }

    #[test]
    fn lifecycle_noise_is_stripped_from_op_results() {
        let stdout = r#"{"success":true,"data":{"clicked":"@e2","lifecycle":{"launched":false,"reused":true}},"error":null}"#;
        let outcome = parse_action_outcome("click", stdout, "", true);
        assert!(outcome.success);
        assert_eq!(outcome.output, "Clicked @e2.");
    }

    #[test]
    fn failure_json_surfaces_error_field() {
        let stdout = r#"{"success":false,"data":null,"error":"Unknown ref: e999"}"#;
        let outcome = parse_action_outcome("click", stdout, "", false);
        assert!(!outcome.success);
        assert_eq!(outcome.error_text.as_deref(), Some("Unknown ref: e999"));
    }

    #[test]
    fn malformed_json_degrades_to_bounded_raw() {
        let stdout = r#"{"success":true,"data":{"snapshot":"truncat"#;
        let outcome = parse_action_outcome("snapshot", stdout, "", true);
        assert!(outcome.success);
        assert!(!outcome.json_valid);
        assert!(outcome.output.contains("[InvalidBrowserOutput"));
        assert!(outcome.output.contains("truncat"));
    }

    #[test]
    fn snapshot_refs_pass_through_verbatim() {
        let stdout = r#"{"success":true,"data":{"origin":"https://example.com/","snapshot":"- link \"Learn more\" [ref=e2]"},"error":null}"#;
        let outcome = parse_action_outcome("snapshot", stdout, "", true);
        assert!(outcome.output.starts_with("URL: https://example.com/\n\n"));
        assert!(outcome.output.contains("--- BEGIN WEB CONTENT ---"));
        assert!(outcome.output.contains(r#"- link "Learn more" [ref=e2]"#));
    }

    #[test]
    fn same_text_different_refs_are_never_merged() {
        let snapshot =
            "- button \"Buy\" [ref=e1]\n- button \"Buy\" [ref=e2]\n- button \"Buy\" [ref=e3]";
        let stdout = json!({
            "success": true,
            "data": {
                "origin": "https://shop.example/",
                "snapshot": snapshot,
                "refs": {"e1": {"name": "Buy", "role": "button"}, "e2": {"name": "Buy", "role": "button"}},
            },
            "error": null,
        })
        .to_string();
        let outcome = parse_action_outcome("snapshot", &stdout, "", true);
        for reference in ["ref=e1", "ref=e2", "ref=e3"] {
            assert!(
                outcome.output.contains(reference),
                "ref {reference} must survive normalization"
            );
        }
        assert_eq!(outcome.output.matches("Buy").count(), 3);
    }

    #[test]
    fn snapshot_truncation_is_semantic_and_marked() {
        let snapshot = "- line\n".repeat(10_000);
        let stdout = json!({
            "success": true,
            "data": {"origin": "https://example.com/", "snapshot": snapshot},
            "error": null,
        })
        .to_string();
        let outcome = parse_action_outcome("snapshot", &stdout, "", true);
        assert!(outcome
            .output
            .contains("[content truncated: showing 24,000 of"));
        // The header survives truncation.
        assert!(outcome.output.starts_with("URL: https://example.com/"));
    }

    #[test]
    fn get_text_is_bounded_with_marker() {
        let stdout = json!({
            "success": true,
            "data": {"text": "x".repeat(30_000)},
            "error": null,
        })
        .to_string();
        let outcome = parse_action_outcome("get_text", &stdout, "", true);
        assert!(outcome
            .output
            .contains("[content truncated: showing 20,000 of 30,000 chars]"));
    }

    #[test]
    fn stderr_never_pollutes_success_output() {
        let stdout = r#"{"success":true,"data":{"url":"https://example.com/"},"error":null}"#;
        let stderr = "Warning: deprecated daemon flag\nmore noise";
        let outcome = parse_action_outcome("open", stdout, stderr, true);
        assert!(outcome.success);
        assert!(!outcome.output.contains("deprecated"));
        assert!(!outcome.output.contains("noise"));
    }

    #[test]
    fn empty_stdout_with_stderr_failure_uses_stderr() {
        let outcome = parse_action_outcome("open", "", "daemon died", false);
        assert!(!outcome.success);
        assert_eq!(outcome.error_text.as_deref(), Some("daemon died"));
    }

    #[test]
    fn screenshot_reports_path() {
        let stdout = r#"{"success":true,"data":{"path":"C:\\tmp\\shot.png"},"error":null}"#;
        let outcome = parse_action_outcome("screenshot", stdout, "", true);
        assert_eq!(outcome.output, "Screenshot saved to: C:\\tmp\\shot.png");
    }
}
