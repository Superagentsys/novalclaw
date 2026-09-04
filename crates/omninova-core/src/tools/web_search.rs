use crate::config::WebSearchConfig;
use crate::tools::traits::{Tool, ToolResult};
use crate::tools::web_fetch::{fetch_url_text, http_client};
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::json;
use std::time::Duration;

const DEFAULT_MAX_RESULTS: u32 = 8;
const DEFAULT_TIMEOUT_SECS: u64 = 15;
const DEFAULT_FETCH_TOP: u32 = 2;
const MAX_FETCH_TOP: u32 = 4;
const PAGE_CHARS: usize = 3500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchBackend {
    Brave,
    Tavily,
    Bocha,
    Jina,
}

impl SearchBackend {
    fn as_str(self) -> &'static str {
        match self {
            Self::Brave => "brave",
            Self::Tavily => "tavily",
            Self::Bocha => "bocha",
            Self::Jina => "jina",
        }
    }
}

#[derive(Debug, Clone)]
struct SearchHit {
    title: String,
    url: String,
    snippet: String,
    body: Option<String>,
}

pub struct WebSearchTool {
    backend: SearchBackend,
    api_key: Option<String>,
    max_results: u32,
    timeout: Duration,
    fetch_top: u32,
}

impl WebSearchTool {
    pub fn from_config(cfg: &WebSearchConfig) -> Option<Self> {
        if !cfg.enabled {
            return None;
        }
        let backend = resolve_backend(cfg);
        let api_key = resolve_api_key(cfg);
        if matches!(backend, SearchBackend::Brave | SearchBackend::Tavily | SearchBackend::Bocha)
            && api_key.is_none()
        {
            return None;
        }
        Some(Self {
            backend,
            api_key,
            max_results: cfg.max_results.unwrap_or(DEFAULT_MAX_RESULTS).clamp(1, 20),
            timeout: Duration::from_secs(cfg.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS).max(5)),
            fetch_top: cfg.fetch_top.unwrap_or(DEFAULT_FETCH_TOP).min(MAX_FETCH_TOP),
        })
    }

    async fn search(&self, query: &str, count: u32) -> anyhow::Result<Vec<SearchHit>> {
        match self.backend {
            SearchBackend::Brave => self.search_brave(query, count).await,
            SearchBackend::Tavily => self.search_tavily(query, count).await,
            SearchBackend::Bocha => self.search_bocha(query, count).await,
            SearchBackend::Jina => self.search_jina(query, count).await,
        }
    }

    async fn search_brave(&self, query: &str, count: u32) -> anyhow::Result<Vec<SearchHit>> {
        let key = self.api_key.as_deref().unwrap_or("");
        let client = http_client(self.timeout)?;
        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Subscription-Token",
            HeaderValue::from_str(key).unwrap_or(HeaderValue::from_static("")),
        );
        headers.insert("Accept", HeaderValue::from_static("application/json"));
        let resp = client
            .get("https://api.search.brave.com/res/v1/web/search")
            .headers(headers)
            .query(&[("q", query), ("count", &count.to_string())])
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("Brave Search API error: {}", resp.status());
        }
        let json: serde_json::Value = resp.json().await?;
        Ok(hits_from_array(
            json.get("web").and_then(|w| w.get("results")).and_then(|r| r.as_array()),
            "title",
            "url",
            "description",
        ))
    }

    async fn search_tavily(&self, query: &str, count: u32) -> anyhow::Result<Vec<SearchHit>> {
        let key = self.api_key.as_deref().unwrap_or("");
        let client = http_client(self.timeout)?;
        let resp = client
            .post("https://api.tavily.com/search")
            .json(&json!({
                "api_key": key,
                "query": query,
                "max_results": count,
                "include_answer": true,
                "search_depth": "basic",
            }))
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("Tavily Search API error: {}", resp.status());
        }
        let json: serde_json::Value = resp.json().await?;
        let mut hits = hits_from_array(json.get("results").and_then(|r| r.as_array()), "title", "url", "content");
        if let Some(answer) = json.get("answer").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
            if let Some(first) = hits.first_mut() {
                first.snippet = format!("{answer}\n\n{}", first.snippet);
            }
        }
        Ok(hits)
    }

    async fn search_bocha(&self, query: &str, count: u32) -> anyhow::Result<Vec<SearchHit>> {
        let key = self.api_key.as_deref().unwrap_or("");
        let client = http_client(self.timeout)?;
        let resp = client
            .post("https://api.bochaai.com/v1/web-search")
            .bearer_auth(key)
            .json(&json!({
                "query": query,
                "count": count,
                "summary": true,
                "freshness": "oneWeek",
            }))
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("Bocha Search API error: {}", resp.status());
        }
        let json: serde_json::Value = resp.json().await?;
        let pages = json
            .pointer("/data/webPages/value")
            .and_then(|v| v.as_array());
        Ok(pages
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        let url = item.get("url").and_then(|v| v.as_str())?.to_string();
                        let title = item
                            .get("name")
                            .or_else(|| item.get("title"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("No Title")
                            .to_string();
                        let snippet = item
                            .get("summary")
                            .or_else(|| item.get("snippet"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        Some(SearchHit {
                            title,
                            url,
                            snippet,
                            body: None,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn search_jina(&self, query: &str, count: u32) -> anyhow::Result<Vec<SearchHit>> {
        let client = http_client(self.timeout)?;
        let mut req = client
            .get("https://s.jina.ai/")
            .query(&[("q", query)])
            .header("Accept", "application/json");
        if let Some(key) = self.api_key.as_deref().filter(|k| !k.is_empty()) {
            req = req.bearer_auth(key);
        }
        let resp = req.send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("Jina Search API error: {}", resp.status());
        }
        let json: serde_json::Value = resp.json().await?;
        let items = json
            .get("data")
            .and_then(|v| v.as_array())
            .or_else(|| json.get("results").and_then(|v| v.as_array()));
        let mut hits = hits_from_array(items, "title", "url", "description");
        if hits.is_empty() {
            hits = hits_from_array(items, "title", "url", "content");
        }
        hits.truncate(count as usize);
        Ok(hits)
    }

    async fn enrich(&self, hits: &mut [SearchHit], read_top: u32) {
        let n = (read_top as usize).min(hits.len());
        if n == 0 {
            return;
        }
        let Ok(client) = http_client(Duration::from_secs(12)) else {
            return;
        };
        for hit in hits.iter_mut().take(n) {
            if hit.url.is_empty() {
                continue;
            }
            if let Ok(text) = fetch_url_text(&client, &hit.url, PAGE_CHARS).await {
                if !text.trim().is_empty() {
                    hit.body = Some(text);
                }
            }
        }
    }
}

fn resolve_backend(cfg: &WebSearchConfig) -> SearchBackend {
    match cfg
        .provider
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "brave" => SearchBackend::Brave,
        "tavily" => SearchBackend::Tavily,
        "bocha" | "bochaai" => SearchBackend::Bocha,
        "jina" => SearchBackend::Jina,
        _ => {
            let has_generic = nonempty(cfg.api_key.as_deref()).is_some();
            let has_brave = nonempty(cfg.brave_api_key.as_deref()).is_some();
            if has_brave && !has_generic {
                SearchBackend::Brave
            } else if has_generic {
                SearchBackend::Jina
            } else {
                SearchBackend::Jina
            }
        }
    }
}

fn resolve_api_key(cfg: &WebSearchConfig) -> Option<String> {
    nonempty(cfg.api_key.as_deref())
        .or_else(|| nonempty(cfg.brave_api_key.as_deref()))
        .map(str::to_string)
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|s| !s.is_empty())
}

fn hits_from_array(
    items: Option<&Vec<serde_json::Value>>,
    title_key: &str,
    url_key: &str,
    snippet_key: &str,
) -> Vec<SearchHit> {
    let Some(items) = items else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let url = item.get(url_key).and_then(|v| v.as_str())?.to_string();
            Some(SearchHit {
                title: item
                    .get(title_key)
                    .and_then(|v| v.as_str())
                    .unwrap_or("No Title")
                    .to_string(),
                url,
                snippet: item
                    .get(snippet_key)
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                body: None,
            })
        })
        .collect()
}

fn format_hits(backend: SearchBackend, hits: &[SearchHit]) -> String {
    if hits.is_empty() {
        return "No results found.".to_string();
    }
    let mut output = format!("provider: {}\n\n", backend.as_str());
    for (i, hit) in hits.iter().enumerate() {
        output.push_str(&format!("{}. [{}]({})\n", i + 1, hit.title, hit.url));
        if !hit.snippet.trim().is_empty() {
            output.push_str(hit.snippet.trim());
            output.push_str("\n");
        }
        if let Some(body) = hit.body.as_deref().filter(|s| !s.trim().is_empty()) {
            output.push_str("\n");
            output.push_str(body.trim());
            output.push_str("\n");
        }
        output.push('\n');
    }
    output
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the public web and return titles, URLs, snippets, and optional article text. \
         Primary tool for news, current events, policy, market data, and research. \
         Prefer this over the browser. After search, use web_fetch only when you need a full page \
         that was not already inlined. Do not open a browser to search."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "count": { "type": "integer", "description": "Number of results (1-20)", "default": 8 },
                "read_top": {
                    "type": "integer",
                    "description": "Fetch plaintext of the top N result pages (0-4). Use 2–3 for news briefings.",
                    "default": 2
                }
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
            .unwrap_or(self.max_results as u64)
            .clamp(1, 20) as u32;
        let read_top = args
            .get("read_top")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32)
            .unwrap_or(self.fetch_top)
            .min(MAX_FETCH_TOP);

        match self.search(query, count).await {
            Ok(mut hits) => {
                self.enrich(&mut hits, read_top).await;
                Ok(ToolResult {
                    success: true,
                    output: format_hits(self.backend, &hits),
                    error: None,
                })
            }
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(e.to_string()),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(
        provider: Option<&str>,
        api_key: Option<&str>,
        brave: Option<&str>,
    ) -> WebSearchConfig {
        WebSearchConfig {
            enabled: true,
            provider: provider.map(str::to_string),
            api_key: api_key.map(str::to_string),
            brave_api_key: brave.map(str::to_string),
            max_results: None,
            timeout_secs: None,
            fetch_top: None,
        }
    }

    #[test]
    fn auto_picks_jina_without_keys() {
        assert_eq!(resolve_backend(&cfg(None, None, None)), SearchBackend::Jina);
        assert!(WebSearchTool::from_config(&cfg(None, None, None)).is_some());
    }

    #[test]
    fn legacy_brave_key_selects_brave() {
        assert_eq!(
            resolve_backend(&cfg(None, None, Some("BSA123"))),
            SearchBackend::Brave
        );
    }

    #[test]
    fn generic_key_without_provider_uses_jina() {
        assert_eq!(
            resolve_backend(&cfg(None, Some("jina_xxx"), None)),
            SearchBackend::Jina
        );
    }

    #[test]
    fn explicit_provider_wins() {
        assert_eq!(
            resolve_backend(&cfg(Some("tavily"), Some("tvly-x"), None)),
            SearchBackend::Tavily
        );
        assert_eq!(
            resolve_backend(&cfg(Some("brave"), Some("k"), None)),
            SearchBackend::Brave
        );
    }

    #[test]
    fn brave_without_key_is_unavailable() {
        let mut c = cfg(Some("brave"), None, None);
        c.api_key = None;
        c.brave_api_key = None;
        assert!(WebSearchTool::from_config(&c).is_none());
    }

    #[test]
    fn disabled_is_unavailable() {
        let mut c = cfg(None, None, None);
        c.enabled = false;
        assert!(WebSearchTool::from_config(&c).is_none());
    }
}
