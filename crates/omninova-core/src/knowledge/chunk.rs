//! Split ingested documents into retrieval-sized passages.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub heading: Option<String>,
    pub text: String,
}

const TARGET_CHARS: usize = 1000;
const MAX_CHARS: usize = 1400;
const OVERLAP_CHARS: usize = 120;

pub fn chunk_text(text: &str) -> Vec<Chunk> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let sections = split_markdown_sections(trimmed);
    let mut chunks = Vec::new();
    for (heading, body) in sections {
        if body.chars().count() <= MAX_CHARS {
            let text = body.trim();
            if !text.is_empty() {
                chunks.push(Chunk {
                    heading: heading.clone(),
                    text: text.to_string(),
                });
            }
            continue;
        }
        chunks.extend(window_paragraphs(heading, &body));
    }
    if chunks.is_empty() {
        chunks.push(Chunk {
            heading: None,
            text: trimmed.chars().take(MAX_CHARS).collect(),
        });
    }
    chunks
}

fn split_markdown_sections(text: &str) -> Vec<(Option<String>, String)> {
    let mut sections = Vec::new();
    let mut heading: Option<String> = None;
    let mut body = String::new();
    for line in text.lines() {
        let stripped = line.trim_start();
        if let Some(rest) = stripped.strip_prefix('#') {
            let hashes = rest.chars().take_while(|c| *c == '#').count() + 1;
            let title = rest[hashes.saturating_sub(1)..].trim();
            if hashes <= 6 && !title.is_empty() && stripped.starts_with('#') {
                if !body.trim().is_empty() || heading.is_some() {
                    sections.push((heading.take(), std::mem::take(&mut body)));
                }
                heading = Some(title.trim_start_matches('#').trim().to_string());
                continue;
            }
        }
        body.push_str(line);
        body.push('\n');
    }
    if !body.trim().is_empty() || heading.is_some() {
        sections.push((heading, body));
    }
    if sections.is_empty() {
        sections.push((None, text.to_string()));
    }
    sections
}

fn window_paragraphs(heading: Option<String>, body: &str) -> Vec<Chunk> {
    let paras: Vec<&str> = body
        .split("\n\n")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    if paras.is_empty() {
        return window_chars(heading, body);
    }
    let mut chunks = Vec::new();
    let mut current = String::new();
    for para in paras {
        if current.chars().count() + para.chars().count() + 2 > TARGET_CHARS && !current.is_empty()
        {
            chunks.push(Chunk {
                heading: heading.clone(),
                text: current.trim().to_string(),
            });
            let overlap: String = current
                .chars()
                .rev()
                .take(OVERLAP_CHARS)
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            current = overlap.trim().to_string();
            if !current.is_empty() {
                current.push_str("\n\n");
            }
        }
        if !current.is_empty() && !current.ends_with('\n') {
            current.push_str("\n\n");
        }
        current.push_str(para);
    }
    if !current.trim().is_empty() {
        chunks.push(Chunk {
            heading,
            text: current.trim().to_string(),
        });
    }
    chunks
}

fn window_chars(heading: Option<String>, body: &str) -> Vec<Chunk> {
    let chars: Vec<char> = body.chars().collect();
    if chars.is_empty() {
        return Vec::new();
    }
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let end = (start + TARGET_CHARS).min(chars.len());
        chunks.push(Chunk {
            heading: heading.clone(),
            text: chars[start..end].iter().collect(),
        });
        if end == chars.len() {
            break;
        }
        start = end.saturating_sub(OVERLAP_CHARS);
    }
    chunks
}
