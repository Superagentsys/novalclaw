use crate::knowledge::KnowledgeStore;
use crate::tools::traits::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;

pub struct KnowledgeSearchTool {
    store: Arc<KnowledgeStore>,
}

impl KnowledgeSearchTool {
    pub fn new(store: Arc<KnowledgeStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for KnowledgeSearchTool {
    fn name(&self) -> &str {
        "knowledge_search"
    }

    fn description(&self) -> &str {
        "Search the external Excel knowledge base uploaded by the user. Returns matching table rows as text snippets."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search keywords or question" },
                "limit": { "type": "integer", "minimum": 1, "maximum": 20, "description": "Max snippets (default 5)" }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        if !self.store.is_enabled() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("外挂知识库未启用".into()),
            });
        }
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'query' parameter"))?;
        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as usize;

        let hits = self.store.search(query, limit);
        if hits.is_empty() {
            return Ok(ToolResult {
                success: true,
                output: "知识库中未找到与查询相关的表格行。".to_string(),
                error: None,
            });
        }
        let results: Vec<serde_json::Value> = hits
            .iter()
            .map(|h| {
                json!({
                    "doc_id": h.doc_id,
                    "filename": h.filename,
                    "sheet": h.sheet,
                    "row": h.row_index,
                    "text": h.text,
                    "score": h.score,
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
