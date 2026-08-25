//! Turn inbound screenshots and chat-attachment data URLs into Chat Completions
//! `image_url` parts, instead of leaving megabytes of base64 in user text.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedInboundVision {
    pub text: String,
    pub images: Vec<String>,
    pub dropped_unsupported: usize,
}

pub fn resolve_inbound_vision(
    text: &str,
    extra_images: Vec<String>,
    provider_supports: bool,
) -> ResolvedInboundVision {
    let (stripped, embedded) = extract_and_strip_data_images(text);
    let mut images = Vec::new();
    for url in extra_images.into_iter().chain(embedded) {
        let url = normalize_data_url(&url);
        if url.is_empty() || images.iter().any(|existing| existing == &url) {
            continue;
        }
        images.push(url);
    }

    if images.is_empty() {
        return ResolvedInboundVision {
            text: text.to_string(),
            images,
            dropped_unsupported: 0,
        };
    }
    if !provider_supports {
        return ResolvedInboundVision {
            text: text.to_string(),
            images: Vec::new(),
            dropped_unsupported: images.len(),
        };
    }
    ResolvedInboundVision {
        text: stripped,
        images,
        dropped_unsupported: 0,
    }
}

pub fn extract_and_strip_data_images(text: &str) -> (String, Vec<String>) {
    let spans = find_data_image_spans(text);
    if spans.is_empty() {
        return (text.to_string(), Vec::new());
    }

    let mut images = Vec::new();
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;
    for (url_start, url_end) in spans {
        let (replace_start, replace_end, alt) =
            markdown_image_bounds(text, url_start, url_end).unwrap_or((url_start, url_end, None));
        if replace_start < cursor {
            continue;
        }
        out.push_str(&text[cursor..replace_start]);
        let url = normalize_data_url(&text[url_start..url_end]);
        if !url.is_empty() && !images.iter().any(|existing| existing == &url) {
            images.push(url);
        }
        match alt {
            Some(name) if !name.is_empty() => {
                out.push_str(&format!("![{name}](已作为视觉输入附加)"));
            }
            _ => out.push_str("[已作为视觉输入附加]"),
        }
        cursor = replace_end;
    }
    out.push_str(&text[cursor..]);
    (out, images)
}

fn normalize_data_url(raw: &str) -> String {
    raw.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn find_data_image_spans(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = text[from..].find("data:image/") {
        let start = from + rel;
        let Some(end) = consume_data_url(text, start) else {
            from = start + 1;
            continue;
        };
        spans.push((start, end));
        from = end;
    }
    spans
}

fn consume_data_url(text: &str, start: usize) -> Option<usize> {
    let rest = &text[start..];
    let header = rest.find(";base64,")?;
    if !rest[..header].starts_with("data:image/") {
        return None;
    }
    let payload_start = start + header + ";base64,".len();
    if payload_start >= text.len() {
        return None;
    }
    let bytes = text.as_bytes();
    let mut end = payload_start;
    while end < bytes.len() {
        let c = bytes[end];
        if c.is_ascii_alphanumeric() || matches!(c, b'+' | b'/' | b'=') || c.is_ascii_whitespace() {
            end += 1;
        } else {
            break;
        }
    }
    if end == payload_start {
        return None;
    }
    Some(end)
}

fn markdown_image_bounds(
    text: &str,
    url_start: usize,
    url_end: usize,
) -> Option<(usize, usize, Option<String>)> {
    if url_start < 2 || &text[url_start - 2..url_start] != "](" {
        return None;
    }
    let alt_close = url_start - 2;
    let alt_open = text[..alt_close].rfind("![")?;
    let alt = text[alt_open + 2..alt_close].to_string();
    let mut end = url_end;
    while end < text.len() && text.as_bytes()[end].is_ascii_whitespace() {
        end += 1;
    }
    if end < text.len() && text.as_bytes()[end] == b')' {
        end += 1;
    }
    Some((alt_open, end, Some(alt)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaves_plain_text_unchanged() {
        let (text, images) = extract_and_strip_data_images("hello");
        assert_eq!(text, "hello");
        assert!(images.is_empty());
    }

    #[test]
    fn extracts_markdown_data_url_and_strips_payload() {
        let input = "看图\n\n![probe.png](data:image/png;base64,QUJDRA==)\n完";
        let (text, images) = extract_and_strip_data_images(input);
        assert_eq!(images, vec!["data:image/png;base64,QUJDRA==".to_string()]);
        assert!(text.contains("![probe.png](已作为视觉输入附加)"));
        assert!(!text.contains("QUJDRA=="));
    }

    #[test]
    fn resolve_keeps_text_when_provider_has_no_vision() {
        let input = "![x](data:image/png;base64,QUJDRA==)";
        let resolved = resolve_inbound_vision(input, Vec::new(), false);
        assert_eq!(resolved.text, input);
        assert!(resolved.images.is_empty());
        assert_eq!(resolved.dropped_unsupported, 1);
    }

    #[test]
    fn resolve_merges_desktop_and_embedded_images() {
        let input = "![x](data:image/png;base64,QUJDRA==)";
        let extra = vec!["data:image/jpeg;base64,/9j/".to_string()];
        let resolved = resolve_inbound_vision(input, extra, true);
        assert_eq!(resolved.images.len(), 2);
        assert_eq!(resolved.dropped_unsupported, 0);
        assert!(!resolved.text.contains("QUJDRA=="));
    }
}
