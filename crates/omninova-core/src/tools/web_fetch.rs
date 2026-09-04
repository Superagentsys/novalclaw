use crate::tools::traits::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::time::Duration;

const MAX_RESPONSE_BYTES: usize = 512 * 1024;
const REQUEST_TIMEOUT_SECS: u64 = 30;
pub(crate) const FETCH_USER_AGENT: &str =
    "Mozilla/5.0 (compatible; OmniNova/1.0; +https://github.com/Superagentsys/novalclaw)";

pub(crate) fn http_client(timeout: Duration) -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::limited(10))
        .user_agent(FETCH_USER_AGENT)
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build HTTP client: {e}"))
}

pub(crate) async fn fetch_url_text(
    client: &reqwest::Client,
    url: &str,
    max_chars: usize,
) -> anyhow::Result<String> {
    let resp = client.get(url).send().await?;
    let status = resp.status();
    if !status.is_success() {
        anyhow::bail!("HTTP {status}");
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
    let text = if content_type.contains("text/html") || content_type.contains("application/xhtml") {
        strip_html_tags(&raw)
    } else {
        raw
    };
    Ok(truncate_chars(&text, max_chars))
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let cut: String = trimmed.chars().take(max_chars).collect();
    format!("{cut}…")
}

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
        "Fetch a public URL and return plaintext (HTML stripped). \
         Use after web_search when you need the full article. \
         Do not use the browser tool for ordinary news or documentation pages."
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

        let client = match http_client(Duration::from_secs(REQUEST_TIMEOUT_SECS)) {
            Ok(client) => client,
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(e.to_string()),
                });
            }
        };

        match fetch_url_text(&client, url, usize::MAX).await {
            Ok(text) => Ok(ToolResult {
                success: true,
                output: text,
                error: None,
            }),
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Fetch failed: {e}")),
            }),
        }
    }
}

pub(crate) fn strip_html_tags(html: &str) -> String {
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
            } else if lower.starts_with("<style") {
                in_script = true;
            } else if lower.starts_with("</style") {
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
