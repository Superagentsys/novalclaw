use crate::tools::traits::{Tool, ToolResult};
use crate::tools::web_client::{
    build_web_client, check_destination, host_matches_allowlist, map_reqwest_error,
    read_body_limited, redact_secrets_in_text, WebClientError, WebToolSettings,
    MAX_REQUEST_BODY_BYTES,
};
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::json;
use std::collections::HashMap;
use url::Url;

/// Hop-by-hop and structural headers that must not be user-supplied: they
/// corrupt the connection framing or leak credentials to the wrong hop. The
/// HTTP client controls all of them.
const FORBIDDEN_REQUEST_HEADERS: &[&str] = &[
    "host",
    "content-length",
    "transfer-encoding",
    "connection",
    "te",
    "trailer",
    "upgrade",
    "proxy-connection",
    "proxy-authorization",
    "keep-alive",
    "expect",
];

const ALLOWED_METHODS: &[&str] = &["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"];

fn is_sensitive_response_header(name: &str) -> bool {
    matches!(
        name,
        "authorization"
            | "proxy-authorization"
            | "cookie"
            | "set-cookie"
            | "set-cookie2"
            | "www-authenticate"
    )
}

pub struct HttpRequestTool {
    allowed_domains: Vec<String>,
    settings: WebToolSettings,
}

impl HttpRequestTool {
    pub fn new(allowed_domains: Vec<String>, settings: WebToolSettings) -> Self {
        Self {
            allowed_domains,
            settings,
        }
    }

    fn is_domain_allowed(&self, url: &str) -> bool {
        if self.allowed_domains.is_empty() {
            return true;
        }
        let Ok(parsed) = url::Url::parse(url) else {
            return false;
        };
        parsed
            .host_str()
            .is_some_and(|host| host_matches_allowlist(host, &self.allowed_domains))
    }

    fn sanitize_headers(headers: &HashMap<String, String>) -> Result<HeaderMap, WebClientError> {
        let mut sanitized = HeaderMap::with_capacity(headers.len());
        for (key, value) in headers {
            let name = HeaderName::from_bytes(key.as_bytes()).map_err(|_| {
                WebClientError::HeaderRejected {
                    detail: format!("header name '{key}' is not a valid HTTP header name"),
                }
            })?;
            if FORBIDDEN_REQUEST_HEADERS.contains(&name.as_str()) {
                return Err(WebClientError::HeaderRejected {
                    detail: format!(
                        "header '{key}' is controlled by the HTTP client and cannot be set"
                    ),
                });
            }
            let value = HeaderValue::from_bytes(value.as_bytes()).map_err(|_| {
                WebClientError::HeaderRejected {
                    detail: format!("header '{key}' contains non-visible-ASCII characters"),
                }
            })?;
            sanitized.append(name, value);
        }
        Ok(sanitized)
    }
}

#[async_trait]
impl Tool for HttpRequestTool {
    fn name(&self) -> &str {
        "http_request"
    }

    fn description(&self) -> &str {
        "Make a raw HTTP request to a REST/API endpoint. Supports GET, POST, PUT, DELETE, PATCH, \
         HEAD, OPTIONS with custom headers and a body. Returns a bounded raw body — no HTML \
         extraction and no JavaScript. Do not use this to read web pages (use web_fetch or browser) \
         or to search the web (use web_search). \
         Permanent failures such as [PrivateNetworkBlocked] will not succeed on retry."
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
        let body = args
            .get("body")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let headers: HashMap<String, String> = args
            .get("headers")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        if !self.is_domain_allowed(url) {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!(
                    "[InvalidUrl] Domain not in allowed list for URL: {}",
                    redact_secrets_in_text(url)
                )),
            });
        }

        if !ALLOWED_METHODS.contains(&method.as_str()) {
            return Ok(ToolResult::failure(format!(
                "[InvalidUrl] method '{method}' is not supported; allowed: {ALLOWED_METHODS:?}"
            )));
        }

        let parsed = match Url::parse(url) {
            Ok(parsed) => parsed,
            Err(e) => {
                return Ok(ToolResult::failure(
                    WebClientError::InvalidUrl {
                        detail: e.to_string(),
                    }
                    .to_string(),
                ));
            }
        };

        let via_proxy = self.settings.proxy.for_scheme(parsed.scheme()).is_some();

        // Local input validation first so malformed requests fail without any
        // network activity.
        let sanitized_headers = match Self::sanitize_headers(&headers) {
            Ok(headers) => headers,
            Err(e) => return Ok(ToolResult::failure(e.to_string())),
        };

        if let Some(body) = body.as_deref() {
            if body.len() > MAX_REQUEST_BODY_BYTES {
                return Ok(ToolResult::failure(
                    WebClientError::RequestTooLarge {
                        detail: format!(
                            "request body is {} bytes; limit is {MAX_REQUEST_BODY_BYTES} bytes",
                            body.len()
                        ),
                    }
                    .to_string(),
                ));
            }
        }

        if let Err(e) = check_destination(&parsed, &self.allowed_domains, via_proxy).await {
            return Ok(ToolResult::failure(e.to_string()));
        }

        let client = match build_web_client(
            &self
                .settings
                .web_client_settings(self.allowed_domains.clone()),
        ) {
            Ok(client) => client,
            Err(e) => return Ok(ToolResult::failure(e.to_string())),
        };

        let mut request = match method.as_str() {
            "POST" => client.post(parsed.clone()),
            "PUT" => client.put(parsed.clone()),
            "DELETE" => client.delete(parsed.clone()),
            "PATCH" => client.patch(parsed.clone()),
            "HEAD" => client.head(parsed.clone()),
            "OPTIONS" => client.request(reqwest::Method::OPTIONS, parsed.clone()),
            _ => client.get(parsed.clone()),
        };
        request = request.headers(sanitized_headers);
        if let Some(body) = body {
            request = request.body(body);
        }

        let mut response = match request.send().await {
            Ok(response) => response,
            Err(e) => return Ok(ToolResult::failure(map_reqwest_error(e).to_string())),
        };

        let status = response.status().as_u16();
        let resp_headers = response
            .headers()
            .iter()
            .map(|(k, v)| {
                let name = k.as_str();
                if is_sensitive_response_header(name) {
                    format!("{name}: [redacted]")
                } else {
                    format!("{name}: {}", v.to_str().unwrap_or("(binary)"))
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        let resp_body =
            match read_body_limited(&mut response, self.settings.max_response_size).await {
                Ok(body) => body,
                Err(e) => return Ok(ToolResult::failure(e.to_string())),
            };
        let mut body_str = String::from_utf8_lossy(&resp_body.bytes).to_string();
        if resp_body.truncated {
            body_str.push_str(&format!(
                "\n[truncated at {} bytes; received {} bytes or more]",
                self.settings.max_response_size, resp_body.total_read
            ));
        }

        Ok(ToolResult {
            success: (200..400).contains(&status),
            output: format!("HTTP {status}\n{resp_headers}\n\n{body_str}"),
            error: if status >= 400 {
                Some(WebClientError::HttpStatusError { status }.to_string())
            } else {
                None
            },
        })
    }
}
