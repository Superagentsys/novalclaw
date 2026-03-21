//! Memory Context Enhancement Module
//!
//! This module provides functionality for retrieving relevant memories
//! and building context strings for context-enhanced LLM responses.
//!
//! [Source: Story 5.9 - 上下文增强响应]

use crate::config::schema::MemoryContextConfig;
use crate::memory::{MemoryManager, UnifiedMemoryEntry, MemoryLayer};
use anyhow::Result;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

/// A memory entry with computed relevance score
#[derive(Debug, Clone)]
pub struct ScoredMemory {
    /// The underlying memory entry
    pub entry: UnifiedMemoryEntry,
    /// Computed relevance score (0.0-1.0+)
    pub relevance_score: f32,
}

/// Result of memory context retrieval
#[derive(Debug, Clone)]
pub struct MemoryContextResult {
    /// Scored memory entries sorted by relevance
    pub memories: Vec<ScoredMemory>,
    /// Total character count of all memories
    pub total_chars: usize,
    /// Time taken for retrieval in milliseconds
    pub retrieval_time_ms: u64,
}

impl MemoryContextResult {
    /// Create an empty result
    pub fn empty() -> Self {
        Self {
            memories: Vec::new(),
            total_chars: 0,
            retrieval_time_ms: 0,
        }
    }
}

/// Retrieve relevant memories for a query
///
/// This function:
/// 1. Performs semantic search using the MemoryManager
/// 2. Calculates relevance scores combining similarity, importance, time decay, and marked status
/// 3. Sorts and filters results based on configuration
///
/// # Arguments
/// * `memory_manager` - The MemoryManager instance for searching (wrapped in Arc<Mutex>)
/// * `query` - The user's message to find relevant memories for
/// * `config` - Configuration for memory context
/// * `agent_id` - Optional agent ID for filtering
///
/// # Returns
/// A MemoryContextResult with scored memories
pub async fn retrieve_relevant_memories(
    memory_manager: &Arc<Mutex<MemoryManager>>,
    query: &str,
    config: &MemoryContextConfig,
    agent_id: Option<i64>,
) -> Result<MemoryContextResult> {
    let start = SystemTime::now();

    // Skip if disabled
    if !config.enabled {
        return Ok(MemoryContextResult::empty());
    }

    // Lock the memory manager and perform semantic search
    let manager = memory_manager.lock().await;
    let search_results = manager
        .search(query, config.max_memories * 2, config.min_similarity_threshold)
        .await?;
    drop(manager); // Release lock early

    // Get current timestamp for time decay calculation
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    // Score and filter results
    let mut scored_memories: Vec<ScoredMemory> = search_results
        .into_iter()
        .filter(|entry| {
            // Filter by agent_id if specified
            if let (Some(filter_agent_id), Some(entry_agent_id)) = (agent_id, entry.session_id) {
                // For now, we don't have agent_id in UnifiedMemoryEntry
                // This filter can be enhanced later
                let _ = (filter_agent_id, entry_agent_id);
            }
            true
        })
        .map(|entry| {
            let relevance = calculate_relevance_score(
                entry.similarity_score.unwrap_or(0.5),
                entry.importance,
                now - entry.created_at,
                false, // is_marked - TODO: Add to UnifiedMemoryEntry
                config.time_decay_factor,
            );

            ScoredMemory {
                entry,
                relevance_score: relevance,
            }
        })
        .collect();

    // Sort by relevance score (descending)
    scored_memories.sort_by(|a, b| {
        b.relevance_score.partial_cmp(&a.relevance_score).unwrap_or(std::cmp::Ordering::Equal)
    });

    // Truncate to max_memories
    scored_memories.truncate(config.max_memories);

    // Calculate total characters
    let total_chars: usize = scored_memories
        .iter()
        .map(|m| m.entry.content.len())
        .sum();

    let retrieval_time_ms = start
        .elapsed()
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    Ok(MemoryContextResult {
        memories: scored_memories,
        total_chars,
        retrieval_time_ms,
    })
}

/// Calculate relevance score for a memory
///
/// The relevance score combines:
/// - Semantic similarity (50% weight)
/// - Importance (30% weight)
/// - Time decay (20% weight)
/// - Marked bonus (+0.2 if marked as important)
///
/// # Arguments
/// * `similarity` - Semantic similarity score (0.0-1.0)
/// * `importance` - Importance rating (1-10)
/// * `seconds_ago` - How many seconds ago the memory was created
/// * `is_marked` - Whether the memory is marked as important
/// * `time_decay_factor` - Time decay factor (higher = more decay)
///
/// # Returns
/// A relevance score (0.0-1.0+, can exceed 1.0 due to marked bonus)
pub fn calculate_relevance_score(
    similarity: f32,
    importance: u8,
    seconds_ago: i64,
    is_marked: bool,
    time_decay_factor: f32,
) -> f32 {
    // Calculate days ago for time decay
    let days_ago = seconds_ago as f32 / 86400.0;

    // Time decay using exponential decay
    let time_decay = (-time_decay_factor * days_ago).exp();

    // Marked bonus for important memories
    let marked_bonus = if is_marked { 0.2 } else { 0.0 };

    // Importance weight normalized to 0.0-1.0
    let importance_weight = importance as f32 / 10.0;

    // Combine scores with weights
    similarity * 0.5 + importance_weight * 0.3 + time_decay * 0.2 + marked_bonus
}

/// Build a context string from scored memories
///
/// Creates a formatted context section that can be injected into LLM prompts.
///
/// # Arguments
/// * `result` - The memory context result
/// * `max_chars` - Maximum character limit for the context
///
/// # Returns
/// A formatted context string for LLM prompts
pub fn build_context_string(result: &MemoryContextResult, max_chars: usize) -> String {
    if result.memories.is_empty() {
        return String::new();
    }

    let mut context = String::from("## 相关记忆上下文\n\n以下是与当前对话相关的历史记忆，请参考这些信息进行回答：\n\n");

    for (i, memory) in result.memories.iter().enumerate() {
        let entry = &memory.entry;

        // Format date
        let date = chrono::DateTime::from_timestamp(entry.created_at, 0)
            .map(|dt| dt.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "未知日期".to_string());

        // Format layer
        let _layer = match entry.source_layer {
            MemoryLayer::L1 => "L1",
            MemoryLayer::L2 => "L2",
            MemoryLayer::L3 => "L3",
            MemoryLayer::All => "ALL",
        };

        // Format similarity if available
        let similarity_str = entry.similarity_score
            .map(|s| format!(" (相似度: {:.0}%)", s * 100.0))
            .unwrap_or_default();

        let line = format!("{}. [{}] {}{} {}\n",
            i + 1,
            date,
            entry.content.chars().take(100).collect::<String>(),
            if entry.content.len() > 100 { "..." } else { "" },
            similarity_str
        );

        // Check character limit
        if context.len() + line.len() > max_chars {
            break;
        }

        context.push_str(&line);
    }

    context.push_str("\n---\n\n");

    // Truncate if still over limit
    if context.len() > max_chars {
        context.truncate(max_chars);
        context.push_str("...\n");
    }

    context
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_relevance_score() {
        // High similarity, recent, important
        let score = calculate_relevance_score(0.9, 8, 3600, true, 0.1);
        assert!(score > 0.8);

        // Low similarity, old, not important
        let score = calculate_relevance_score(0.3, 3, 86400 * 30, false, 0.1);
        assert!(score < 0.5);

        // Marked memories get bonus
        let score_marked = calculate_relevance_score(0.7, 5, 86400, true, 0.1);
        let score_unmarked = calculate_relevance_score(0.7, 5, 86400, false, 0.1);
        assert!(score_marked > score_unmarked);
        assert!((score_marked - score_unmarked - 0.2).abs() < 0.01);
    }

    #[test]
    fn test_build_context_string_empty() {
        let result = MemoryContextResult::empty();
        let context = build_context_string(&result, 1000);
        assert!(context.is_empty());
    }

    #[test]
    fn test_build_context_string_respects_char_limit() {
        let result = MemoryContextResult {
            memories: vec![
                ScoredMemory {
                    entry: UnifiedMemoryEntry {
                        id: "1".to_string(),
                        content: "A".repeat(500),
                        role: None,
                        importance: 5,
                        session_id: None,
                        created_at: 0,
                        source_layer: MemoryLayer::L2,
                        similarity_score: Some(0.8),
                    },
                    relevance_score: 0.8,
                },
                ScoredMemory {
                    entry: UnifiedMemoryEntry {
                        id: "2".to_string(),
                        content: "B".repeat(500),
                        role: None,
                        importance: 5,
                        session_id: None,
                        created_at: 0,
                        source_layer: MemoryLayer::L2,
                        similarity_score: Some(0.7),
                    },
                    relevance_score: 0.7,
                },
            ],
            total_chars: 1000,
            retrieval_time_ms: 10,
        };

        let context = build_context_string(&result, 300);
        assert!(context.len() <= 303); // 300 + "...\n"
    }
}