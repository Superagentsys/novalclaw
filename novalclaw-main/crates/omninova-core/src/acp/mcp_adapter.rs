//! MCP (Model Context Protocol) JSON-RPC 2.0 adapter.
//!
//! Connects OmniNova to external MCP-compatible servers (e.g., TrendRadar)
//! via HTTP transport. Provides initialize, list_tools, and call_tool.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

// ─── JSON-RPC 2.0 Types ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<u64>,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    #[allow(dead_code)]
    id: u64,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Deserialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(default)]
    data: Option<Value>,
}

// ─── MCP Protocol Types ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerCapabilities {
    pub protocol_version: Option<String>,
    pub server_info: Option<ServerInfo>,
    #[serde(default)]
    pub tools: Option<ToolsCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsCapability {
    #[serde(default)]
    pub list_changed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub input_schema: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallToolResult {
    #[serde(default)]
    pub content: Vec<ContentItem>,
    #[serde(default)]
    pub is_error: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentItem {
    #[serde(rename = "type", default)]
    pub content_type: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub data: Option<String>,
    #[serde(default)]
    pub mime_type: Option<String>,
}

// ─── Transport ────────────────────────────────────────────────────────────

/// Transport mode for MCP communication.
#[derive(Debug, Clone)]
pub enum McpTransport {
    /// HTTP transport — connects to an already-running MCP server.
    Http { base_url: String },
    /// Stdio transport — spawns and communicates via stdin/stdout.
    Stdio { command: String, args: Vec<String> },
}

impl McpTransport {
    /// Create an HTTP transport pointing at a TrendRadar MCP server.
    pub fn http(host: &str, port: u16) -> Self {
        McpTransport::Http {
            base_url: format!("http://{host}:{port}/mcp"),
        }
    }
}

// ─── Adapter ──────────────────────────────────────────────────────────────

/// MCP adapter that communicates with an external MCP-compatible server.
///
/// Used to bridge OmniNova's Agent tools with TrendRadar's 26 MCP tools.
pub struct McpAdapter {
    transport: McpTransport,
    client: reqwest::Client,
    next_id: std::sync::atomic::AtomicU64,
    /// MCP session ID from the initialize response (required for SSE transport).
    session_id: RwLock<Option<String>>,
    /// Cached server capabilities after initialize.
    capabilities: RwLock<Option<ServerCapabilities>>,
    /// Cached tool list after list_tools.
    tools_cache: RwLock<Option<Vec<ToolDef>>>,
    /// Whether the session has been initialized.
    initialized: RwLock<bool>,
}

impl McpAdapter {
    /// Create a new MCP adapter with the given transport.
    pub fn new(transport: McpTransport) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(10))
            .http1_only()
            .pool_max_idle_per_host(0)
            .build()
            .expect("failed to build reqwest client for MCP adapter");

        Self {
            transport,
            client,
            next_id: std::sync::atomic::AtomicU64::new(1),
            session_id: RwLock::new(None),
            capabilities: RwLock::new(None),
            tools_cache: RwLock::new(None),
            initialized: RwLock::new(false),
        }
    }

    /// Create for connecting to a TrendRadar MCP server over HTTP.
    pub fn for_trendradar(host: &str, port: u16) -> Self {
        Self::new(McpTransport::http(host, port))
    }

    // ─── JSON-RPC Helpers ─────────────────────────────────────────────

    fn next_request_id(&self) -> u64 {
        self.next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    async fn send_request(&self, method: &str, params: Option<Value>) -> Result<JsonRpcResponse> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(self.next_request_id()),
            method: method.to_string(),
            params,
        };

        let url = match &self.transport {
            McpTransport::Http { base_url } => base_url.clone(),
            McpTransport::Stdio { .. } => {
                anyhow::bail!("stdio transport is not yet implemented");
            }
        };

        debug!("MCP request: {} -> {}", method, url);

        let mut req = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("Connection", "close");

        if let Some(sid) = self.session_id.read().await.as_deref() {
            req = req.header("Mcp-Session-Id", sid);
        }

        eprintln!("[mcp-adapter] sending request: {method}");
        let response = req.json(&request).send().await
            .with_context(|| format!("MCP HTTP request failed: {method} -> {url}"))?;
        eprintln!("[mcp-adapter] got response for: {method}");

        // Extract MCP session ID from response headers (for SSE transport).
        if let Some(sid) = response.headers().get("Mcp-Session-Id") {
            if let Ok(sid_str) = sid.to_str() {
                if !sid_str.is_empty() && self.session_id.read().await.is_none() {
                    eprintln!("[mcp-adapter] session ID: {}", sid_str);
                    *self.session_id.write().await = Some(sid_str.to_string());
                }
            }
        }

        let status = response.status();
        eprintln!("[mcp-adapter] reading body for: {method} status={}", status.as_u16());
        let body_bytes = tokio::time::timeout(
            Duration::from_secs(30),
            response.bytes(),
        )
        .await
        .context("timed out reading MCP response body")?
        .context("failed to read MCP response body")?;
        eprintln!("[mcp-adapter] body read: {method} len={}", body_bytes.len());
        let body = String::from_utf8_lossy(&body_bytes).to_string();

        if !status.is_success() {
            let truncated = body_truncated(&body);
            anyhow::bail!(
                "MCP server returned HTTP {status}: {truncated}"
            );
        }

        // Parse the body, handling SSE (Server-Sent Events) format.
        // MCP SSE transport wraps JSON-RPC responses as:
        //   event: message
        //   data: {"jsonrpc":"2.0",...}
        let json_body: String = if body.starts_with("event:") || body.starts_with("data:") {
            extract_sse_data(&body).unwrap_or_else(|| body.clone())
        } else {
            body.clone()
        };

        let truncated = body_truncated(&json_body);
        let parsed: JsonRpcResponse = serde_json::from_str(&json_body)
            .with_context(|| format!("failed to parse MCP response: {truncated}"))?;

        if let Some(err) = &parsed.error {
            let detail = err
                .data
                .as_ref()
                .map(|d| format!(" ({d})"))
                .unwrap_or_default();
            anyhow::bail!(
                "MCP error [{code}]: {message}{detail}",
                code = err.code,
                message = err.message,
                detail = detail
            );
        }

        Ok(parsed)
    }

    /// Send a JSON-RPC notification (no `id` field, no response expected).
    async fn send_notification(&self, method: &str, params: Option<Value>) -> Result<()> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: method.to_string(),
            params,
        };

        let url = match &self.transport {
            McpTransport::Http { base_url } => base_url.clone(),
            McpTransport::Stdio { .. } => anyhow::bail!("stdio not yet implemented"),
        };

        let mut req = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("Connection", "close");

        if let Some(sid) = self.session_id.read().await.as_deref() {
            req = req.header("Mcp-Session-Id", sid);
        }

        let _ = req.json(&request).send().await?;
        Ok(())
    }

    // ─── MCP Protocol Methods ──────────────────────────────────────────

    /// Initialize the MCP session with the server.
    ///
    /// Must be called once before any other operations.
    pub async fn initialize(&self) -> Result<ServerCapabilities> {
        if *self.initialized.read().await {
            // Already initialized — return cached capabilities.
            if let Some(caps) = self.capabilities.read().await.clone() {
                return Ok(caps);
            }
        }

        info!("Initializing MCP session...");

        let params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "OmniNova Claw",
                "version": env!("CARGO_PKG_VERSION")
            }
        });

        let response = self.send_request("initialize", Some(params)).await?;

        let capabilities: ServerCapabilities =
            serde_json::from_value(response.result.unwrap_or_default())
                .context("failed to parse MCP initialize response")?;

        info!(
            "MCP session initialized: server={}, version={}",
            capabilities
                .server_info
                .as_ref()
                .map(|s| s.name.as_str())
                .unwrap_or("unknown"),
            capabilities
                .server_info
                .as_ref()
                .map(|s| s.version.as_str())
                .unwrap_or("unknown"),
        );

        // Send initialized notification (no response expected, must NOT have an id).
        eprintln!("[mcp-adapter] sending initialized notification");
        let _ = self
            .send_notification("notifications/initialized", None)
            .await;
        eprintln!("[mcp-adapter] initialized notification sent");

        *self.capabilities.write().await = Some(capabilities.clone());
        *self.initialized.write().await = true;

        eprintln!("[mcp-adapter] initialize complete");
        Ok(capabilities)
    }

    /// List all available tools from the MCP server.
    ///
    /// Results are cached after the first call. Pass `force_refresh=true`
    /// to bypass the cache.
    pub async fn list_tools(&self, force_refresh: bool) -> Result<Vec<ToolDef>> {
        if !force_refresh {
            if let Some(cached) = self.tools_cache.read().await.clone() {
                return Ok(cached);
            }
        }

        let _ = self.initialize().await?;

        let response = self.send_request("tools/list", Some(serde_json::json!({}))).await?;

        let result: Value = response.result.unwrap_or_default();
        let tools: Vec<ToolDef> = result
            .get("tools")
            .cloned()
            .map(|v| {
                serde_json::from_value(v)
                    .context("failed to parse tools/list response")
            })
            .unwrap_or_else(|| Ok(Vec::new()))?;

        info!("Discovered {} MCP tools", tools.len());
        for tool in &tools {
            debug!(
                "  - {}: {}",
                tool.name,
                tool.description.as_deref().unwrap_or("(no description)")
            );
        }

        *self.tools_cache.write().await = Some(tools.clone());

        Ok(tools)
    }

    /// Call a specific MCP tool with the given arguments.
    pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<CallToolResult> {
        // Ensure session is initialized.
        let _ = self.initialize().await?;

        let params = serde_json::json!({
            "name": name,
            "arguments": arguments,
        });

        debug!("MCP call_tool: {} args={}", name, arguments);

        let response = self.send_request("tools/call", Some(params)).await?;

        let result: CallToolResult =
            serde_json::from_value(response.result.unwrap_or_default())
                .with_context(|| format!("failed to parse tools/call response for '{name}'"))?;

        if result.is_error.unwrap_or(false) {
            let error_text = result
                .content
                .first()
                .and_then(|c| c.text.clone())
                .unwrap_or_else(|| "unknown error".to_string());
            warn!("MCP tool '{name}' reported error: {error_text}");
            // Don't fail the entire call — let the caller decide.
        }

        Ok(result)
    }

    /// Extract the text content from a CallToolResult.
    pub fn result_text(result: &CallToolResult) -> String {
        result
            .content
            .iter()
            .filter_map(|item| item.text.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Convenience: call a tool and get the text result directly.
    pub async fn call_tool_text(&self, name: &str, arguments: Value) -> Result<String> {
        let result = self.call_tool(name, arguments).await?;
        Ok(Self::result_text(&result))
    }

    /// Check if the MCP server is reachable.
    pub async fn health_check(&self) -> bool {
        match &self.transport {
            McpTransport::Http { base_url } => {
                let url = base_url.clone();
                match self
                    .client
                    .get(&url)
                    .header("Accept", "application/json, text/event-stream")
                    .timeout(Duration::from_secs(5))
                    .send()
                    .await
                {
                    Ok(resp) => {
                        let status = resp.status().as_u16();
                        status == 200 || status == 406 || status == 400 || status == 405
                    }
                    Err(_) => false,
                }
            }
            McpTransport::Stdio { .. } => false,
        }
    }

    /// Reset the adapter state (clear caches, mark uninitialized).
    pub async fn reset(&self) {
        *self.initialized.write().await = false;
        *self.session_id.write().await = None;
        *self.capabilities.write().await = None;
        *self.tools_cache.write().await = None;
    }
}

impl std::fmt::Debug for McpAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpAdapter")
            .field("transport", &self.transport)
            .field("initialized", &"<locked>")
            .finish()
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────

/// Extract the JSON data payload from an SSE-formatted response.
///
/// SSE format:
///   event: message
///   data: {"jsonrpc":"2.0",...}
///
/// Returns the data payload as a string if found.
fn extract_sse_data(body: &str) -> Option<String> {
    for line in body.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            return Some(data.to_string());
        }
    }
    None
}

fn body_truncated(body: &str) -> &str {
    if body.len() > 500 {
        // Find a valid UTF-8 boundary at or before 500 bytes.
        let mut end = 500;
        while end > 0 && !body.is_char_boundary(end) {
            end -= 1;
        }
        &body[..end]
    } else {
        body
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_adapter_creation() {
        let adapter = McpAdapter::for_trendradar("127.0.0.1", 3333);
        assert!(matches!(
            adapter.transport,
            McpTransport::Http { .. }
        ));
    }

    #[test]
    fn test_body_truncated() {
        assert_eq!(body_truncated("hello"), "hello");
        let long = "x".repeat(600);
        assert_eq!(body_truncated(&long).len(), 500);
    }
}
