use crate::memory::Memory;
use crate::tools::traits::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;

pub struct MemoryRecallTool {
    memory: Arc<dyn Memory>,
}

impl MemoryRecallTool {
    pub fn new(memory: Arc<dyn Memory>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for MemoryRecallTool {
    fn name(&self) -> &str {
        "memory_recall"
    }

    fn description(&self) -> &str {
        "Search long-term memory by keyword query and return matching entries."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "limit": { "type": "integer", "minimum": 1, "maximum": 50, "description": "Max results (default 5)" }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'query' parameter"))?;
        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as usize;

        match self.memory.recall(query, limit, None).await {
            Ok(entries) => {
                if entries.is_empty() {
                    return Ok(ToolResult {
                        success: true,
                        output: "No matching memories found.".to_string(),
                        error: None,
                    });
                }
                let results: Vec<serde_json::Value> = entries
                    .iter()
                    .map(|e| {
                        json!({
                            "key": e.key,
                            "content": e.content,
                            "category": format!("{:?}", e.category),
                            "timestamp": e.timestamp,
                        })
                    })
                    .collect();
                let output = serde_json::to_string_pretty(&results).unwrap_or_default();
                Ok(ToolResult {
                    success: true,
                    output,
                    error: None,
                })
            }
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Memory recall failed: {e}")),
            }),
        }
    }
}
