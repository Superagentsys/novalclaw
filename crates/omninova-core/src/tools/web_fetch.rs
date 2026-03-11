use crate::tools::traits::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::time::Duration;

const MAX_RESPONSE_BYTES: usize = 512 * 1024;
const REQUEST_TIMEOUT_SECS: u64 = 30;

pub struct WebFetchTool {
    allowed_domains: Vec<String>,
}

impl WebFetchTool {
    pub fn new(allowed_domains: Vec<String>) -> Self {
        Self { allowed_domains }
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
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Fetch a web page and return its text content. HTML is stripped to plain text."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "URL to fetch" }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let url = args
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'url' parameter"))?;

        if !self.is_domain_allowed(url) {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Domain not in allowed list for URL: {url}")),
            });
        }

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to build HTTP client: {e}"))?;

        match client.get(url).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                if status >= 400 {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(format!("HTTP {status}")),
                    });
                }
                let content_type = resp
                    .headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_lowercase();

                let body_bytes = resp.bytes().await.unwrap_or_default();
                let raw = if body_bytes.len() > MAX_RESPONSE_BYTES {
                    String::from_utf8_lossy(&body_bytes[..MAX_RESPONSE_BYTES]).to_string()
                } else {
                    String::from_utf8_lossy(&body_bytes).to_string()
                };

                let text = if content_type.contains("text/html") {
                    strip_html_tags(&raw)
                } else {
                    raw
                };

                Ok(ToolResult {
                    success: true,
                    output: text,
                    error: None,
                })
            }
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Fetch failed: {e}")),
            }),
        }
    }
}

fn strip_html_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut in_script = false;
    let chars: Vec<char> = html.chars().collect();
    let len = chars.len();
    let mut i = 0;
    while i < len {
        let ch = chars[i];
        if ch == '<' {
            let rest: String = chars[i..].iter().take(10).collect();
            let lower = rest.to_lowercase();
            if lower.starts_with("<script") {
                in_script = true;
            } else if lower.starts_with("</script") {
                in_script = false;
            }
            in_tag = true;
            i += 1;
            continue;
        }
        if ch == '>' {
            in_tag = false;
            i += 1;
            continue;
        }
        if !in_tag && !in_script {
            out.push(ch);
        }
        i += 1;
    }
    let lines: Vec<&str> = out.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    lines.join("\n")
}
