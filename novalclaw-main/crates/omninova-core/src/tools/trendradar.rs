//! TrendRadar MCP tool integration.
//!
//! Bridges OmniNova's Tool trait with TrendRadar's 26 MCP tools.
//! Each MCP tool is wrapped as an `McpToolWrapper` that implements `Tool`,
//! so the Agent can discover and invoke them like any other built-in tool.
//!
//! Tool names are namespaced: `trendradar.<mcp_tool_name>`.

use crate::acp::mcp_adapter::{McpAdapter, ToolDef};
use crate::tools::traits::{Tool, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

// ─── Bridge ───────────────────────────────────────────────────────────────

/// High-level bridge that manages the connection to TrendRadar MCP server
/// and creates Tool instances for each discovered MCP tool.
pub struct TrendRadarBridge {
    /// The MCP adapter for communication.
    adapter: Arc<McpAdapter>,
    /// Cached tool definitions from the MCP server.
    tool_defs: RwLock<Vec<ToolDef>>,
    /// Whether tools have been discovered.
    discovered: RwLock<bool>,
}

impl TrendRadarBridge {
    /// Create a new bridge connected to a TrendRadar MCP server over HTTP.
    pub fn new(host: &str, port: u16) -> Self {
        Self {
            adapter: Arc::new(McpAdapter::for_trendradar(host, port)),
            tool_defs: RwLock::new(Vec::new()),
            discovered: RwLock::new(false),
        }
    }

    /// Create a bridge from an existing MCP adapter.
    pub fn with_adapter(adapter: Arc<McpAdapter>) -> Self {
        Self {
            adapter,
            tool_defs: RwLock::new(Vec::new()),
            discovered: RwLock::new(false),
        }
    }

    /// Check if the TrendRadar MCP server is reachable.
    pub async fn is_healthy(&self) -> bool {
        self.adapter.health_check().await
    }

    /// Discover tools from the MCP server and cache them.
    pub async fn discover_tools(&self, force: bool) -> Result<Vec<ToolDef>> {
        if !force && *self.discovered.read().await {
            return Ok(self.tool_defs.read().await.clone());
        }

        let tools = self.adapter.list_tools(force).await?;
        *self.tool_defs.write().await = tools.clone();
        *self.discovered.write().await = true;

        Ok(tools)
    }

    /// Create Tool trait objects for all discovered MCP tools.
    ///
    /// Each tool is namespaced as `trendradar_<original_name>` (underscore
    /// to satisfy provider function-name regex `^[a-zA-Z0-9_-]+$`).
    pub async fn create_tools(&self) -> Result<Vec<Box<dyn Tool>>> {
        let defs = self.discover_tools(false).await?;

        let tools: Vec<Box<dyn Tool>> = defs
            .into_iter()
            .map(|def| {
                let tool_name = format!("trendradar_{}", def.name);
                let description = def
                    .description
                    .unwrap_or_else(|| format!("TrendRadar MCP tool: {}", def.name));
                let input_schema = def.input_schema.unwrap_or_else(|| {
                    serde_json::json!({
                        "type": "object",
                        "properties": {},
                    })
                });

                Box::new(McpToolWrapper {
                    tool_name,
                    description,
                    parameters: input_schema,
                    mcp_tool_name: def.name,
                    adapter: self.adapter.clone(),
                }) as Box<dyn Tool>
            })
            .collect();

        Ok(tools)
    }

    /// Access the underlying MCP adapter.
    pub fn adapter(&self) -> &Arc<McpAdapter> {
        &self.adapter
    }

    /// Call a TrendRadar MCP tool directly (bypassing the Tool trait).
    pub async fn call_tool_direct(&self, name: &str, args: Value) -> Result<String> {
        self.adapter.call_tool_text(name, args).await
    }
}

impl std::fmt::Debug for TrendRadarBridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrendRadarBridge")
            .field("adapter", &"<McpAdapter>")
            .field("discovered", &self.discovered)
            .finish()
    }
}

// ─── MCP Tool Wrapper ─────────────────────────────────────────────────────

/// An individual MCP tool wrapped as an OmniNova Tool.
///
/// Implements the `Tool` trait by forwarding `execute()` calls to the
/// TrendRadar MCP server via the shared `McpAdapter`.
struct McpToolWrapper {
    /// The namespaced tool name (e.g., "trendradar.get_latest_news").
    tool_name: String,
    /// Human-readable description.
    description: String,
    /// JSON Schema for the tool's parameters.
    parameters: Value,
    /// The original MCP tool name (e.g., "get_latest_news").
    mcp_tool_name: String,
    /// Shared MCP adapter for communication.
    adapter: Arc<McpAdapter>,
}

#[async_trait]
impl Tool for McpToolWrapper {
    fn name(&self) -> &str {
        &self.tool_name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters_schema(&self) -> Value {
        self.parameters.clone()
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        debug!(
            "TrendRadar tool call: {} (args: {})",
            self.mcp_tool_name, args
        );

        match self.adapter.call_tool(&self.mcp_tool_name, args).await {
            Ok(result) => {
                let text = McpAdapter::result_text(&result);
                let is_error = result.is_error.unwrap_or(false);

                if is_error {
                    warn!(
                        "TrendRadar tool '{}' returned error: {}",
                        self.mcp_tool_name,
                        text_truncated(&text)
                    );
                    Ok(ToolResult {
                        success: false,
                        output: text.clone(),
                        error: Some(text),
                    })
                } else {
                    debug!(
                        "TrendRadar tool '{}' succeeded: {}",
                        self.mcp_tool_name,
                        text_truncated(&text)
                    );
                    Ok(ToolResult {
                        success: true,
                        output: text,
                        error: None,
                    })
                }
            }
            Err(e) => {
                let err_msg = format!("TrendRadar MCP call failed: {e}");
                warn!("{err_msg}");
                Ok(ToolResult {
                    success: false,
                    output: err_msg.clone(),
                    error: Some(err_msg),
                })
            }
        }
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────

fn text_truncated(s: &str) -> &str {
    if s.len() > 200 {
        &s[..200]
    } else {
        s
    }
}

// ─── Module Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bridge_creation() {
        let bridge = TrendRadarBridge::new("127.0.0.1", 3333);
        // Bridge created without error; health check would fail since
        // no server is running, but that's fine for a unit test.
        assert!(!bridge.is_healthy().await);
    }
}
