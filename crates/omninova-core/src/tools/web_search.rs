use crate::tools::traits::{Tool, ToolResult};
use crate::tools::web_client::{
    build_web_client, check_destination, map_reqwest_error, read_body_limited, WebClientError,
    WebToolSettings, DEFAULT_WEB_MAX_RESPONSE_BYTES,
};
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::json;
use url::Url;

const BRAVE_SEARCH_URL: &str = "https://api.search.brave.com/res/v1/web/search";

pub struct WebSearchTool {
    api_key: String,
    settings: WebToolSettings,
}

impl WebSearchTool {
    pub fn new(api_key: impl Into<String>, settings: WebToolSettings) -> Self {
        Self {
            api_key: api_key.into(),
            settings,
        }
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web using the Brave Search API. Returns ranked titles, URLs, and snippets. \
         Use this only to discover pages; then read a specific URL with web_fetch (static HTML) \
         or the browser tool (JavaScript / interaction). This is not an HTTP API client."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "count": { "type": "integer", "description": "Number of results (1-20)", "default": 10 }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'query' parameter"))?;
        let count = args
            .get("count")
            .and_then(|v| v.as_u64())
            .unwrap_or(10)
            .min(20)
            .max(1);

        let parsed = match Url::parse(BRAVE_SEARCH_URL) {
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
        if let Err(e) = check_destination(&parsed, &[], via_proxy).await {
            return Ok(ToolResult::failure(e.to_string()));
        }

        let client = match build_web_client(&self.settings.web_client_settings(Vec::new())) {
            Ok(client) => client,
            Err(e) => return Ok(ToolResult::failure(e.to_string())),
        };

        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Subscription-Token",
            HeaderValue::from_str(&self.api_key).unwrap_or(HeaderValue::from_static("")),
        );
        headers.insert("Accept", HeaderValue::from_static("application/json"));

        let mut response = match client
            .get(parsed)
            .headers(headers)
            .query(&[("q", query), ("count", &count.to_string())])
            .send()
            .await
        {
            Ok(response) => response,
            Err(e) => return Ok(ToolResult::failure(map_reqwest_error(e).to_string())),
        };

        if !response.status().is_success() {
            let status = response.status().as_u16();
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!(
                    "Brave Search API error: {}",
                    WebClientError::HttpStatusError { status }
                )),
            });
        }

        let body = match read_body_limited(&mut response, DEFAULT_WEB_MAX_RESPONSE_BYTES).await {
            Ok(body) => body,
            Err(e) => return Ok(ToolResult::failure(e.to_string())),
        };
        if body.truncated {
            return Ok(ToolResult::failure(
                WebClientError::ResponseTooLarge {
                    detail: format!(
                        "Brave Search API response exceeds {DEFAULT_WEB_MAX_RESPONSE_BYTES} bytes"
                    ),
                }
                .to_string(),
            ));
        }

        let json: serde_json::Value = match serde_json::from_slice(&body.bytes) {
            Ok(json) => json,
            Err(e) => {
                return Ok(ToolResult::failure(
                    WebClientError::BodyReadFailed {
                        detail: format!("failed to parse JSON response: {e}"),
                    }
                    .to_string(),
                ));
            }
        };

        // Extract relevant parts (web.results)
        let results = json
            .get("web")
            .and_then(|w| w.get("results"))
            .and_then(|r| r.as_array());

        if let Some(items) = results {
            let mut output = String::new();
            for (i, item) in items.iter().enumerate() {
                let title = item
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("No Title");
                let url = item.get("url").and_then(|v| v.as_str()).unwrap_or("");
                let description = item
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                output.push_str(&format!(
                    "{}. [{}]({})\n{}\n\n",
                    i + 1,
                    title,
                    url,
                    description
                ));
            }

            if output.is_empty() {
                output = "No results found.".to_string();
            }

            Ok(ToolResult::success(output))
        } else {
            Ok(ToolResult::success("No results found.".to_string()))
        }
    }
}
