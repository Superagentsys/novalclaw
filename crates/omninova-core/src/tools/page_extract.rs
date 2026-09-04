//! Readable text extraction for `web_fetch`, built on the html5ever parser
//! (via `scraper`). Replaces the old regex-style tag strip: malformed HTML,
//! nested tags, HTML entities, and Unicode are handled by the spec-compliant
//! parser; extraction walks the resulting DOM tree instead of matching tags.
//!
//! This is deliberately not a Readability clone. When no main content can be
//! identified reliably, the cleaned document text is returned with a visible
//! degradation note rather than pretending to be article extraction.

use crate::tools::text_bound::{bound_head, format_count};
use crate::tools::web_client::redact_secrets_in_text;
use ego_tree::NodeRef;
use scraper::{Html, Node};
use url::Url;

/// Char budget for the extracted main text handed to the model. Distinct from
/// the W2.4 network byte limit.
pub const WEB_FETCH_MODEL_CHAR_LIMIT: usize = 40_000;

/// Subtrees that never contribute to readable text.
const SKIP_TAGS: &[&str] = &[
    "script", "style", "noscript", "template", "svg", "iframe", "noembed", "noframes", "head",
    "link", "meta", "source", "track", "object", "embed", "canvas", "audio", "video", "map",
];

/// Block-level elements that force a line break around their content.
const BLOCK_TAGS: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "body",
    "center",
    "details",
    "dd",
    "div",
    "dl",
    "dt",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "header",
    "html",
    "legend",
    "main",
    "nav",
    "p",
    "pre",
    "section",
    "summary",
    "table",
    "tbody",
    "tfoot",
    "thead",
    "textarea",
    "caption",
    "ul",
    "ol",
    "hr",
];

pub struct ExtractedPage {
    pub title: Option<String>,
    pub text: String,
    /// True when structured extraction produced (almost) nothing and the
    /// legacy safe-strip fallback was used instead.
    pub used_fallback: bool,
}

/// Extracts readable text from an HTML document.
pub fn extract_page(html: &str, base_url: Option<&Url>) -> ExtractedPage {
    let document = Html::parse_document(html);
    let title = find_title(&document);
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let root = document.tree.root();
    for child in root.children() {
        walk(child, base_url, &mut lines, &mut current, 0);
    }
    flush(&mut lines, &mut current);
    let text = tidy_lines(&lines);

    if text.trim().is_empty() {
        // Degenerate extraction (e.g. a page whose body is only scripts):
        // fall back to safe text instead of failing the whole tool call.
        ExtractedPage {
            title,
            text: fallback_strip(html),
            used_fallback: true,
        }
    } else {
        ExtractedPage {
            title,
            text,
            used_fallback: false,
        }
    }
}

/// Renders metadata plus bounded main text, wrapped so page bytes cannot be
/// mistaken for OmniNova system instructions.
pub fn format_page_output(
    title: Option<&str>,
    url: Option<&str>,
    fallback_used: bool,
    text: &str,
) -> String {
    let mut out = String::from("Web content from:\n");
    if let Some(title) = title.map(str::trim).filter(|t| !t.is_empty()) {
        out.push_str("Title: ");
        out.push_str(title);
        out.push('\n');
    }
    if let Some(url) = url.map(str::trim).filter(|u| !u.is_empty()) {
        out.push_str("URL: ");
        out.push_str(&redact_secrets_in_text(url));
        out.push('\n');
    }
    if fallback_used {
        out.push_str("[HtmlParseFailed fallback: structured extraction produced no readable text; showing safe-stripped text]\n");
    }
    out.push_str("--- BEGIN WEB CONTENT ---\n");
    out.push_str(&bound_head(text, WEB_FETCH_MODEL_CHAR_LIMIT));
    out.push_str("\n--- END WEB CONTENT ---");
    out
}

fn find_title(document: &Html) -> Option<String> {
    for node in document.tree.nodes() {
        if let Node::Element(element) = node.value() {
            if element.name() == "title" {
                let text = node
                    .children()
                    .filter_map(|child| match child.value() {
                        Node::Text(text) => Some(text.text.to_string()),
                        _ => None,
                    })
                    .collect::<String>();
                let trimmed = text.trim().to_string();
                if !trimmed.is_empty() {
                    return Some(trimmed);
                }
            }
        }
    }
    None
}

fn is_heading(name: &str) -> Option<usize> {
    let bytes = name.as_bytes();
    if bytes.len() == 2 && bytes[0] == b'h' && bytes[1].is_ascii_digit() {
        let level = (bytes[1] - b'0') as usize;
        if (1..=6).contains(&level) {
            return Some(level);
        }
    }
    None
}

fn flush(lines: &mut Vec<String>, current: &mut String) {
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        lines.push(trimmed.to_string());
    }
    current.clear();
}

/// Collapses whitespace runs to single spaces while preserving whether the
/// piece starts or ends on a word boundary, so text from adjacent inline
/// nodes ("Visit " + "OpenAI") keeps its separating space. A piece that is
/// only whitespace still reports itself as a one-space boundary.
fn collapse_whitespace(text: &str) -> String {
    let is_space = |c: char| c.is_whitespace() || c == '\u{a0}';
    let mut body = String::with_capacity(text.len());
    let mut pending_space = false;
    for ch in text.chars() {
        if is_space(ch) {
            if !body.is_empty() {
                pending_space = true;
            }
        } else {
            if pending_space {
                body.push(' ');
                pending_space = false;
            }
            body.push(ch);
        }
    }
    let mut out = String::with_capacity(body.len() + 2);
    if body.is_empty() {
        // All whitespace: still a boundary.
        out.push(' ');
        return out;
    }
    if text.starts_with(is_space) {
        out.push(' ');
    }
    out.push_str(&body);
    if pending_space {
        out.push(' ');
    }
    out
}

fn append_text(current: &mut String, raw: &str, pre_depth: usize) {
    if raw.is_empty() {
        return;
    }
    if pre_depth > 0 {
        current.push_str(raw);
        return;
    }
    let mut collapsed = collapse_whitespace(raw);
    if current.is_empty() || current.ends_with(' ') {
        collapsed = collapsed.trim_start().to_string();
    }
    if collapsed.is_empty() {
        return;
    }
    current.push_str(&collapsed);
}

fn walk(
    node: NodeRef<'_, Node>,
    base_url: Option<&Url>,
    lines: &mut Vec<String>,
    current: &mut String,
    pre_depth: usize,
) {
    match node.value() {
        Node::Document
        | Node::Fragment
        | Node::Doctype(_)
        | Node::Comment(_)
        | Node::ProcessingInstruction(_) => {
            for child in node.children() {
                walk(child, base_url, lines, current, pre_depth);
            }
        }
        Node::Text(text) => append_text(current, &text.text, pre_depth),
        Node::Element(element) => {
            let name = element.name();
            if SKIP_TAGS.contains(&name) {
                return;
            }
            if name == "br" {
                flush(lines, current);
                return;
            }

            if name == "pre" {
                flush(lines, current);
                for child in node.children() {
                    walk(child, base_url, lines, current, pre_depth + 1);
                }
                flush(lines, current);
                return;
            }

            let heading_level = is_heading(name);
            let is_list_item = name == "li";
            let is_table_cell = name == "td" || name == "th";
            let is_table_row = name == "tr";

            if heading_level.is_some() || BLOCK_TAGS.contains(&name) || is_table_row {
                flush(lines, current);
            }
            if let Some(level) = heading_level {
                current.push_str(&"#".repeat(level.min(3)));
                current.push(' ');
            }
            if is_list_item {
                current.push_str("- ");
            }

            if name == "a" {
                // Inline rendering: keep the label in the current line and
                // append the resolved URL in the task's `text (url)` style.
                let href = element
                    .attr("href")
                    .map(str::trim)
                    .filter(|h| !h.is_empty());
                let mut link_text = String::new();
                for child in node.children() {
                    walk_with_capture(child, base_url, lines, current, pre_depth, &mut link_text);
                }
                if let Some(href) = href {
                    if let Some(resolved) = resolve_link(href, base_url) {
                        let display = link_text.trim();
                        if display.is_empty() {
                            // Empty anchors (image links etc.) expose the URL itself.
                            if current.is_empty() || current.ends_with(' ') {
                                current.push_str(resolved.as_str());
                            } else {
                                current.push_str(&format!(" ({})", resolved.as_str()));
                            }
                        } else if display != resolved.as_str() {
                            current.push_str(&format!(" ({})", resolved.as_str()));
                        }
                    }
                }
                return;
            }

            for child in node.children() {
                walk(child, base_url, lines, current, pre_depth);
            }
            if is_table_cell {
                current.push_str(" | ");
            }
            if heading_level.is_some() || BLOCK_TAGS.contains(&name) || is_table_row {
                flush(lines, current);
            }
        }
    }
}

/// Like `walk`, but also mirrors visited text into `captured` so inline
/// context (currently: anchor labels) keeps a flat copy of its text.
fn walk_with_capture(
    node: NodeRef<'_, Node>,
    base_url: Option<&Url>,
    lines: &mut Vec<String>,
    current: &mut String,
    pre_depth: usize,
    captured: &mut String,
) {
    match node.value() {
        Node::Text(text) => {
            append_text(current, &text.text, pre_depth);
            captured.push_str(&text.text);
        }
        Node::Element(element) => {
            if SKIP_TAGS.contains(&element.name()) {
                return;
            }
            for child in node.children() {
                walk_with_capture(child, base_url, lines, current, pre_depth, captured);
            }
        }
        _ => {
            for child in node.children() {
                walk_with_capture(child, base_url, lines, current, pre_depth, captured);
            }
        }
    }
}

fn resolve_link(href: &str, base_url: Option<&Url>) -> Option<Url> {
    let lowered = href.to_lowercase();
    if lowered.starts_with("javascript:") || lowered.starts_with("data:") {
        return None;
    }
    match base_url {
        Some(base) => base.join(href).ok(),
        // Absolute URLs still resolve without a base.
        None => Url::parse(href).ok(),
    }
}

/// Collapse runs of blank lines and drop trailing table separators.
fn tidy_lines(lines: &[String]) -> String {
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut previous_blank = true;
    for line in lines {
        let trimmed = line.trim_end();
        let trimmed = trimmed
            .strip_suffix('|')
            .map(|s| s.trim_end())
            .unwrap_or(trimmed);
        if trimmed.is_empty() {
            if !previous_blank {
                out.push(String::new());
            }
            previous_blank = true;
        } else {
            out.push(trimmed.to_string());
            previous_blank = false;
        }
    }
    while out.last().map(|line| line.is_empty()).unwrap_or(false) {
        out.pop();
    }
    out.join("\n")
}

/// Legacy safe strip: removes tags without parsing. Only used as a fallback
/// when structured extraction degenerates; never panics. Skips the raw-text
/// containers that would otherwise leak non-content into the output.
fn fallback_strip(html: &str) -> String {
    const SKIP_RAW: &[&str] = &["script", "style", "noscript", "template", "svg"];
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut skip_depth = 0usize;
    let mut skipped_tag = String::new();
    let chars: Vec<char> = html.chars().collect();
    let len = chars.len();
    let mut i = 0;
    while i < len {
        let ch = chars[i];
        if ch == '<' {
            let rest: String = chars[i..].iter().take(12).collect();
            let lower = rest.to_lowercase();
            if lower.starts_with("</") {
                let closing = lower[2..].trim_start_matches('/');
                if skip_depth > 0
                    && skipped_tag
                        .chars()
                        .next()
                        .map(|c| closing.starts_with(c))
                        .unwrap_or(false)
                {
                    // Skip through the matching closing tag.
                    if let Some(end) = chars[i..].iter().position(|c| *c == '>') {
                        i += end;
                        skip_depth -= 1;
                        if skip_depth == 0 {
                            skipped_tag.clear();
                        }
                    }
                }
                in_tag = true;
                i += 1;
                continue;
            }
            let tag_name: String = lower[1..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            if skip_depth > 0 {
                in_tag = true;
                i += 1;
                continue;
            }
            if SKIP_RAW.contains(&tag_name.as_str()) {
                skip_depth += 1;
                skipped_tag = tag_name;
                in_tag = true;
                i += 1;
                continue;
            }
            in_tag = true;
            i += 1;
            continue;
        }
        if ch == '>' {
            in_tag = false;
            i += 1;
            continue;
        }
        if !in_tag && skip_depth == 0 {
            out.push(ch);
        }
        i += 1;
    }
    let lines: Vec<&str> = out
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_title_paragraphs_headings() {
        let html = r#"<html><head><title>My Page</title></head><body>
            <h1>Introduction</h1>
            <p>First paragraph here.</p>
            <h2>Details</h2>
            <p>Second paragraph.</p>
        </body></html>"#;
        let page = extract_page(html, None);
        assert_eq!(page.title.as_deref(), Some("My Page"));
        assert!(page.text.contains("# Introduction"));
        assert!(page.text.contains("First paragraph here."));
        assert!(page.text.contains("## Details"));
        assert!(page.text.contains("Second paragraph."));
    }

    #[test]
    fn script_style_and_noise_removed() {
        let html = r#"<html><body>
            <script>var evil = "<p>not content</p>";</script>
            <style>.hidden { color: red }</style>
            <noscript>enable js</noscript>
            <svg><circle/></svg>
            <template><span>tpl</span></template>
            <p>visible text</p>
        </body></html>"#;
        let page = extract_page(html, None);
        assert!(!page.text.contains("evil"), "text={:?}", page.text);
        assert!(!page.text.contains("color: red"), "text={:?}", page.text);
        assert!(!page.text.contains("enable js"), "text={:?}", page.text);
        assert!(!page.text.contains("tpl"), "text={:?}", page.text);
        assert!(page.text.contains("visible text"), "text={:?}", page.text);
    }

    #[test]
    fn entities_are_decoded() {
        let html = "<body><p>Fish &amp; Chips &lt;3 &quot;quoted&quot;</p></body>";
        let page = extract_page(html, None);
        assert!(page.text.contains("Fish & Chips <3 \"quoted\""));
    }

    #[test]
    fn links_rendered_as_text_url() {
        let html = r#"<body><p>Visit <a href="https://openai.com">OpenAI</a> and
            <a href="/docs/guide">the guide</a> plus a <a href="javascript:void(0)">noop</a>.</p></body>"#;
        let base = Url::parse("https://example.com/start").unwrap();
        let page = extract_page(html, Some(&base));
        assert!(
            page.text.contains("OpenAI (https://openai.com/)"),
            "text={:?}",
            page.text
        );
        assert!(
            page.text
                .contains("the guide (https://example.com/docs/guide)"),
            "text={:?}",
            page.text
        );
        // javascript: URLs are not appended.
        assert!(!page.text.contains("void(0)"), "text={:?}", page.text);
        assert!(page.text.contains("noop"), "text={:?}", page.text);
    }

    #[test]
    fn lists_and_tables_keep_structure() {
        let html = r#"<body>
            <ul><li>alpha</li><li>beta</li></ul>
            <table><tr><th>Name</th><th>Value</th></tr><tr><td>a</td><td>1</td></tr></table>
        </body>"#;
        let page = extract_page(html, None);
        assert!(page.text.contains("- alpha"));
        assert!(page.text.contains("- beta"));
        assert!(page.text.contains("Name | Value"));
        assert!(page.text.contains("a | 1"));
    }

    #[test]
    fn malformed_html_does_not_panic_and_keeps_text() {
        let html = "<body><p>unclosed paragraph <b>bold <i>italic <div>block in wrong place</p>";
        let page = extract_page(html, None);
        assert!(page.text.contains("unclosed paragraph"));
        assert!(page.text.contains("bold"));
        assert!(page.text.contains("italic"));
        assert!(page.text.contains("block in wrong place"));
    }

    #[test]
    fn chinese_and_emoji_survive_extraction() {
        let html =
            "<body><h1>中文标题</h1><p>这是中文正文，包含 emoji 😀🎉 和日文テスト。</p></body>";
        let page = extract_page(html, None);
        assert!(page.text.contains("# 中文标题"));
        assert!(page
            .text
            .contains("这是中文正文，包含 emoji 😀🎉 和日文テスト。"));
    }

    #[test]
    fn whitespace_runs_are_collapsed() {
        let html = "<body><p>spaced\n\n   out\t\ttext</p></body>";
        let page = extract_page(html, None);
        assert!(page.text.contains("spaced out text"));
    }

    #[test]
    fn degenerate_extraction_falls_back_to_safe_strip() {
        // Body is only scripts: structured extraction finds no readable text.
        let html =
            "<html><head><title>t</title></head><body><script>only(); js();</script></body></html>";
        let page = extract_page(html, None);
        assert!(page.used_fallback);
        let formatted =
            format_page_output(page.title.as_deref(), None, page.used_fallback, &page.text);
        assert!(formatted.contains("[HtmlParseFailed fallback"));
    }

    #[test]
    fn format_page_output_includes_metadata() {
        let out = format_page_output(Some("Title!"), Some("https://x/"), false, "body text");
        assert!(out.contains("Title: Title!"));
        assert!(out.contains("URL: https://x/"));
        assert!(out.contains("--- BEGIN WEB CONTENT ---\nbody text\n--- END WEB CONTENT ---"));
    }

    #[test]
    fn oversized_text_is_bounded_with_marker() {
        let text = "文".repeat(WEB_FETCH_MODEL_CHAR_LIMIT + 500);
        let out = format_page_output(None, None, false, &text);
        assert!(out.contains(&format!(
            "[content truncated: showing {} of {} chars]",
            format_count(WEB_FETCH_MODEL_CHAR_LIMIT),
            format_count(WEB_FETCH_MODEL_CHAR_LIMIT + 500)
        )));
        // UTF-8 stays valid after the char-boundary cut.
        assert!(std::str::from_utf8(out.as_bytes()).is_ok());
    }
}
