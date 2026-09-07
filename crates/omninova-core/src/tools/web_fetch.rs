use crate::tools::page_extract::{extract_page, format_page_output, WEB_FETCH_MODEL_CHAR_LIMIT};
use crate::tools::text_bound::bound_head;
use crate::tools::traits::{Tool, ToolResult};
use crate::tools::web_client::{
    build_web_client, check_destination, host_matches_allowlist, map_reqwest_error,
    read_body_limited, redact_secrets_in_text, WebClientError, WebToolSettings,
};
use async_trait::async_trait;
use serde_json::json;
use url::Url;

pub struct WebFetchTool {
    allowed_domains: Vec<String>,
    settings: WebToolSettings,
}

impl WebFetchTool {
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
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Fetch a static web page over HTTP and return readable text (title, final URL, main content). \
         Does NOT execute JavaScript and cannot click, log in, or fill forms. \
         Use web_search to discover URLs, this tool to read static HTML, the browser tool for \
         JavaScript/interaction, and http_request for REST APIs. \
         Permanent failures such as [PrivateNetworkBlocked] or [DnsFailure] will not succeed on retry."
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
                error: Some(format!(
                    "[InvalidUrl] Domain not in allowed list for URL: {}",
                    redact_secrets_in_text(url)
                )),
            });
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

        let mut response = match client.get(parsed).send().await {
            Ok(response) => response,
            Err(e) => return Ok(ToolResult::failure(map_reqwest_error(e).to_string())),
        };

        let status = response.status().as_u16();
        if status >= 400 {
            return Ok(ToolResult::failure(
                WebClientError::HttpStatusError { status }.to_string(),
            ));
        }

        // Final URL after redirects; captured before the response is consumed.
        let final_url = response.url().clone();

        let body = match read_body_limited(&mut response, self.settings.max_response_size).await {
            Ok(body) => body,
            Err(e) => return Ok(ToolResult::failure(e.to_string())),
        };

        let mut raw = String::from_utf8_lossy(&body.bytes).to_string();
        if body.truncated {
            raw.push_str(&format!(
                "\n[truncated at {} bytes]",
                self.settings.max_response_size
            ));
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();

        let is_html = content_type.contains("text/html")
            || content_type.contains("application/xhtml+xml")
            || (content_type.is_empty() && raw.trim_start().starts_with('<'));

        let output = if is_html {
            let page = extract_page(&raw, Some(&final_url));
            format_page_output(
                page.title.as_deref(),
                Some(final_url.as_str()),
                page.used_fallback,
                &page.text,
            )
        } else {
            bound_head(&raw, WEB_FETCH_MODEL_CHAR_LIMIT)
        };

        Ok(ToolResult::success(output))
    }
}
