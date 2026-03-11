use crate::tools::traits::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;

const MAX_RESPONSE_BYTES: usize = 1_048_576;
const REQUEST_TIMEOUT_SECS: u64 = 30;

pub struct HttpRequestTool {
    allowed_domains: Vec<String>,
}

impl HttpRequestTool {
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
            .any(|domain| host == domain.as_str() || host.ends_with(&format!(".{domain}")))
    }

    fn is_private_url(url: &str) -> bool {
        let Ok(parsed) = url::Url::parse(url) else {
            return true;
        };
        let Some(host) = parsed.host_str() else {
            return true;
        };
        host == "localhost"
            || host == "127.0.0.1"
            || host == "::1"
            || host.starts_with("10.")
            || host.starts_with("172.")
            || host.starts_with("192.168.")
    }
}

#[async_trait]
impl Tool for HttpRequestTool {
    fn name(&self) -> &str {
        "http_request"
    }

    fn description(&self) -> &str {
        "Make an HTTP request. Supports GET, POST, PUT, DELETE, PATCH, HEAD, OPTIONS."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "url": { "type": "string" },
                "method": { "type": "string", "enum": ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"] },
                "headers": { "type": "object", "additionalProperties": { "type": "string" } },
                "body": { "type": "string" }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let url = args
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'url' parameter"))?;
        let method = args
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("GET")
            .to_uppercase();
        let body = args.get("body").and_then(|v| v.as_str()).map(str::to_string);
        let headers: HashMap<String, String> = args
            .get("headers")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        if !self.is_domain_allowed(url) {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Domain not in allowed list for URL: {url}")),
            });
        }

        if Self::is_private_url(url) {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("Requests to private/local addresses are not allowed".to_string()),
            });
        }

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to build HTTP client: {e}"))?;

        let mut req = match method.as_str() {
            "POST" => client.post(url),
            "PUT" => client.put(url),
            "DELETE" => client.delete(url),
            "PATCH" => client.patch(url),
            "HEAD" => client.head(url),
            _ => client.get(url),
        };

        for (key, value) in &headers {
            req = req.header(key.as_str(), value.as_str());
        }
        if let Some(body) = body {
            req = req.body(body);
        }

        match req.send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let resp_headers = resp
                    .headers()
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, v.to_str().unwrap_or("(binary)")))
                    .collect::<Vec<_>>()
                    .join("\n");
                let body_bytes = resp.bytes().await.unwrap_or_default();
                let body_str = if body_bytes.len() > MAX_RESPONSE_BYTES {
                    format!(
                        "{}\n[truncated at {MAX_RESPONSE_BYTES} bytes, total {} bytes]",
                        String::from_utf8_lossy(&body_bytes[..MAX_RESPONSE_BYTES]),
                        body_bytes.len()
                    )
                } else {
                    String::from_utf8_lossy(&body_bytes).to_string()
                };

                Ok(ToolResult {
                    success: (200..400).contains(&status),
                    output: format!("HTTP {status}\n{resp_headers}\n\n{body_str}"),
                    error: if status >= 400 {
                        Some(format!("HTTP {status}"))
                    } else {
                        None
                    },
                })
            }
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("HTTP request failed: {e}")),
            }),
        }
    }
}
