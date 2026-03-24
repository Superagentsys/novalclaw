use crate::memory::traits::MemoryEntry;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub expand_query: bool,
    pub recency_weight: f64,
    pub recency_half_life_days: f64,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            expand_query: true,
            recency_weight: 2.0,
            recency_half_life_days: 7.0,
        }
    }
}

fn normalize(s: &str) -> String {
    s.to_lowercase()
}

fn tokenize(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(normalize)
        .collect()
}

fn parse_ts(ts: &str) -> Option<i64> {
    ts.parse::<i64>().ok()
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn rank_entries(query: &str, entries: Vec<MemoryEntry>) -> Vec<MemoryEntry> {
    rank_entries_with_options(query, entries, &SearchOptions::default())
}

pub fn rank_entries_with_options(
    query: &str,
    mut entries: Vec<MemoryEntry>,
    options: &SearchOptions,
) -> Vec<MemoryEntry> {
    let query_trimmed = query.trim();
    if query_trimmed.is_empty() {
        entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        return entries;
    }

    let query_norm = normalize(query_trimmed);
    let mut query_tokens = tokenize(query_trimmed);
    if options.expand_query {
        query_tokens = expand_tokens(query_tokens);
    }
    let now = now_unix();
    let mut scored = entries
        .into_iter()
        .map(|entry| {
            let score = score_entry(&entry, &query_norm, &query_tokens, now, options);
            (entry, score)
        })
        .collect::<Vec<_>>();
    scored.sort_by(|(ea, sa), (eb, sb)| {
        sb.total_cmp(sa)
            .then_with(|| eb.timestamp.cmp(&ea.timestamp))
            .then_with(|| ea.key.cmp(&eb.key))
    });
    scored
        .into_iter()
        .map(|(mut entry, score)| {
            entry.score = Some(score);
            entry
        })
        .collect()
}

fn expand_tokens(tokens: Vec<String>) -> Vec<String> {
    let mut expanded = Vec::new();
    for token in tokens {
        expanded.push(token.clone());
        if let Some(stemmed) = stem_token(&token) {
            expanded.push(stemmed);
        }
        for synonym in token_synonyms(&token) {
            expanded.push(synonym.to_string());
        }
    }
    expanded.sort();
    expanded.dedup();
    expanded
}

fn stem_token(token: &str) -> Option<String> {
    if token.len() > 3 && token.ends_with("es") {
        return Some(token[..token.len() - 2].to_string());
    }
    if token.len() > 2 && token.ends_with('s') {
        return Some(token[..token.len() - 1].to_string());
    }
    None
}

fn token_synonyms(token: &str) -> &'static [&'static str] {
    match token {
        "task" => &["todo", "任务"],
        "todo" => &["task", "任务"],
        "任务" => &["task", "todo"],
        "bug" => &["issue", "error", "故障"],
        "issue" => &["bug", "error", "问题"],
        "error" => &["bug", "issue", "错误"],
        "memory" => &["记忆", "memo"],
        "记忆" => &["memory", "memo"],
        _ => &[],
    }
}

fn score_entry(
    entry: &MemoryEntry,
    query_norm: &str,
    query_tokens: &[String],
    now: i64,
    options: &SearchOptions,
) -> f64 {
    let key_norm = normalize(&entry.key);
    let content_norm = normalize(&entry.content);

    let mut score = 0.0;

    if key_norm == query_norm {
        score += 30.0;
    }
    if content_norm == query_norm {
        score += 25.0;
    }
    if key_norm.contains(query_norm) {
        score += 18.0;
    }
    if content_norm.contains(query_norm) {
        score += 12.0;
    }

    for token in query_tokens {
        if key_norm.contains(token) {
            score += 6.0;
        }
        if content_norm.contains(token) {
            score += 3.0;
        }
    }

    if let Some(ts) = parse_ts(&entry.timestamp) {
        let age_days = ((now - ts).max(0) as f64) / 86400.0;
        let half_life = options.recency_half_life_days.max(0.1);
        let decay = f64::exp2(-age_days / half_life);
        let recency = decay * options.recency_weight.max(0.0);
        score += recency;
    }

    score
}

#[cfg(test)]
mod tests {
    use super::{rank_entries, rank_entries_with_options, SearchOptions};
    use crate::memory::traits::{MemoryCategory, MemoryEntry};

    fn mk(key: &str, content: &str, ts: i64) -> MemoryEntry {
        MemoryEntry {
            id: format!("id-{key}"),
            key: key.to_string(),
            content: content.to_string(),
            category: MemoryCategory::Conversation,
            timestamp: ts.to_string(),
            session_id: None,
            score: None,
        }
    }

    #[test]
    fn ranks_exact_match_higher() {
        let now = 2_000_000_000_i64;
        let entries = vec![
            mk("daily-report", "summary and logs", now - 1000),
            mk("task", "task", now - 1000),
        ];
        let ranked = rank_entries("task", entries);
        assert_eq!(ranked.first().map(|e| e.key.as_str()), Some("task"));
        assert!(ranked.first().and_then(|e| e.score).unwrap_or(0.0) > 0.0);
    }

    #[test]
    fn query_expansion_matches_synonyms() {
        let now = 2_000_000_000_i64;
        let entries = vec![
            mk("a", "这是一个任务提醒", now - 1000),
            mk("b", "random note", now - 1000),
        ];
        let ranked = rank_entries("todo", entries);
        assert_eq!(ranked.first().map(|e| e.key.as_str()), Some("a"));
    }

    #[test]
    fn recency_weight_can_be_disabled() {
        let now = 2_000_000_000_i64;
        let entries = vec![
            mk("older", "exact target", now - 90 * 86400),
            mk("newer", "target", now - 60),
        ];
        let options = SearchOptions {
            expand_query: false,
            recency_weight: 0.0,
            recency_half_life_days: 7.0,
        };
        let ranked = rank_entries_with_options("exact target", entries, &options);
        assert_eq!(ranked.first().map(|e| e.key.as_str()), Some("older"));
    }
}
