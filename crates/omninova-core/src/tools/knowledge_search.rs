use crate::knowledge::KnowledgeStore;
use crate::tools::traits::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::path::PathBuf;

pub struct KnowledgeSearchTool {
    workspace_dir: PathBuf,
}

impl KnowledgeSearchTool {
    pub fn new(workspace_dir: impl Into<PathBuf>) -> Self {
        Self {
            workspace_dir: workspace_dir.into(),
        }
    }
}

#[async_trait]
impl Tool for KnowledgeSearchTool {
    fn name(&self) -> &str {
        "knowledge_search"
    }

    fn description(&self) -> &str {
        "Search the local knowledge base for relevant document passages. Use this before answering questions about uploaded notes, manuals, or project docs."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "collection": { "type": "string", "description": "Optional collection name" },
                "limit": { "type": "integer", "minimum": 1, "maximum": 20, "description": "Max passages (default 5)" }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'query' parameter"))?;
        let collection = args
            .get("collection")
            .and_then(|v| v.as_str())
            .filter(|value| !value.is_empty());
        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
        let store = KnowledgeStore::open_in(&self.workspace_dir).await?;
        let hits = store.search(query, collection, limit).await;
        if hits.is_empty() {
            return Ok(ToolResult {
                success: true,
                output: "No matching knowledge-base passages.".to_string(),
                error: None,
            });
        }
        let results: Vec<serde_json::Value> = hits
            .iter()
            .map(|hit| {
                json!({
                    "title": hit.title,
                    "collection": hit.collection,
                    "heading": hit.heading,
                    "snippet": hit.snippet,
                    "score": hit.score,
                })
            })
            .collect();
        Ok(ToolResult {
            success: true,
            output: serde_json::to_string_pretty(&results).unwrap_or_default(),
            error: None,
        })
    }
}
