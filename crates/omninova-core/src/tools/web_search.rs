use crate::tools::traits::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use reqwest::header::{HeaderMap, HeaderValue};

pub struct WebSearchTool {
    api_key: String,
}

impl WebSearchTool {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
        }
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web using Brave Search API to get relevant results and snippets."
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
        let query = args.get("query").and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'query' parameter"))?;
        let count = args.get("count").and_then(|v| v.as_u64()).unwrap_or(10).min(20).max(1);

        let url = "https://api.search.brave.com/res/v1/web/search";
        let client = reqwest::Client::new();
        
        let mut headers = HeaderMap::new();
        headers.insert("X-Subscription-Token", HeaderValue::from_str(&self.api_key).unwrap_or(HeaderValue::from_static("")));
        headers.insert("Accept", HeaderValue::from_static("application/json"));

        let resp = client.get(url)
            .headers(headers)
            .query(&[("q", query), ("count", &count.to_string())])
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to execute search request: {}", e))?;

        if !resp.status().is_success() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Brave Search API error: {}", resp.status())),
            });
        }

        let json: serde_json::Value = resp.json().await
            .map_err(|e| anyhow::anyhow!("Failed to parse JSON response: {}", e))?;
        
        // Extract relevant parts (web.results)
        let results = json.get("web")
            .and_then(|w| w.get("results"))
            .and_then(|r| r.as_array());
        
        if let Some(items) = results {
            let mut output = String::new();
            for (i, item) in items.iter().enumerate() {
                let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("No Title");
                let url = item.get("url").and_then(|v| v.as_str()).unwrap_or("");
                let description = item.get("description").and_then(|v| v.as_str()).unwrap_or("");
                
                output.push_str(&format!("{}. [{}]({})\n{}\n\n", i + 1, title, url, description));
            }
            
            if output.is_empty() {
                output = "No results found.".to_string();
            }

            Ok(ToolResult {
                success: true,
                output,
                error: None,
            })
        } else {
             Ok(ToolResult {
                success: true,
                output: "No results found.".to_string(),
                error: None,
            })
        }
    }
}
