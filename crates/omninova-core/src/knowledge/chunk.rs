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
    let lines: Vec<&str> = text.lines().collect();
    for (index, line) in lines.iter().enumerate() {
        let followed_by_blank_or_end = match lines.get(index + 1) {
            None => true,
            Some(next) => next.trim().is_empty(),
        };
        if followed_by_blank_or_end {
            if let Some(title) = atx_heading_title(line) {
                if !body.trim().is_empty() || heading.is_some() {
                    sections.push((heading.take(), std::mem::take(&mut body)));
                }
                heading = Some(title);
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

/// CommonMark ATX heading only: 0–3 space indent, 1–6 `#`, then space/tab
/// (or end of line). `#include`, `#!/bin/sh`, `#话题` stay in the body.
fn atx_heading_title(line: &str) -> Option<String> {
    let indent = line.chars().take_while(|c| *c == ' ').count();
    if indent > 3 {
        return None;
    }
    let stripped = line.trim_start_matches(' ');
    if stripped.starts_with('\t') {
        return None;
    }
    let rest = stripped.strip_prefix('#')?;
    let extra = rest.chars().take_while(|c| *c == '#').count();
    let hashes = extra + 1;
    if hashes > 6 {
        return None;
    }
    let after: String = rest.chars().skip(extra).collect();
    if !after.is_empty() {
        let first = after.chars().next()?;
        if first != ' ' && first != '\t' {
            return None;
        }
    }
    let mut title = after.trim_matches([' ', '\t']).to_string();
    if let Some((plain, closing)) = title.rsplit_once(" #") {
        if closing.chars().all(|c| c == '#' || c == ' ' || c == '\t') {
            title = plain.trim_end().to_string();
        }
    }
    if title.is_empty() {
        return None;
    }
    let first_word = title.split_whitespace().next().unwrap_or("");
    const PREPROCESSOR: &[&str] = &[
        "include", "define", "ifdef", "ifndef", "endif", "pragma", "undef", "elif", "error",
        "warning", "line",
    ];
    if PREPROCESSOR
        .iter()
        .any(|word| first_word.eq_ignore_ascii_case(word))
    {
        return None;
    }
    Some(title)
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
