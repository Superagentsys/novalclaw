use crate::memory::{Memory, MemoryCategory};
use crate::tools::traits::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;

pub struct MemoryStoreTool {
    memory: Arc<dyn Memory>,
}

impl MemoryStoreTool {
    pub fn new(memory: Arc<dyn Memory>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for MemoryStoreTool {
    fn name(&self) -> &str {
        "memory_store"
    }

    fn description(&self) -> &str {
        "Store a fact, preference, or note into long-term memory for later recall."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "key": { "type": "string", "description": "Unique key for this memory entry" },
                "content": { "type": "string", "description": "Content to remember" },
                "category": { "type": "string", "description": "Category: core, daily, conversation, or custom" }
            },
            "required": ["key", "content"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let key = args
            .get("key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'key' parameter"))?;
        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'content' parameter"))?;
        let category_str = args
            .get("category")
            .and_then(|v| v.as_str())
            .unwrap_or("core");

        let category = match category_str {
            "daily" => MemoryCategory::Daily,
            "conversation" => MemoryCategory::Conversation,
            _ => MemoryCategory::Core,
        };

        match self.memory.store(key, content, category, None).await {
            Ok(()) => Ok(ToolResult {
                success: true,
                output: format!("Stored memory with key '{key}'"),
                error: None,
            }),
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Failed to store memory: {e}")),
            }),
        }
    }
}
