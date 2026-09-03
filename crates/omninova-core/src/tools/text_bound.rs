//! UTF-8-safe character budgeting for model-visible tool output.
//!
//! Byte truncation can split UTF-8 sequences and corrupt JSON payloads, so
//! every model-facing bound operates on `char`s. Two distinct limits exist:
//!   - `NETWORK_RESPONSE_LIMIT` (W2.4): bytes on the wire, per tool config.
//!   - `MODEL_TOOL_OUTPUT_LIMIT` (W2.5): chars that enter model context.

/// Hard cap applied by the ToolRunner to any tool result that reaches model
/// context. Tool-level budgets (browser snapshot, web_fetch text) sit well
/// below this; the net only catches unbounded producers like shell.
pub const MODEL_TOOL_OUTPUT_LIMIT_CHARS: usize = 64_000;

/// Head portion kept by the ToolRunner net.
const MODEL_OUTPUT_HEAD_CHARS: usize = 56_000;
/// Tail portion kept by the ToolRunner net (errors and summaries live at the end).
const MODEL_OUTPUT_TAIL_CHARS: usize = 4_000;

pub fn char_count(text: &str) -> usize {
    text.chars().count()
}

/// Head-biased cut at a char boundary. Returns
/// `(bounded_text, truncated, total_chars)`; the caller is responsible for
/// appending its own truncation marker.
pub fn truncate_head_chars(text: &str, max_chars: usize) -> (String, bool, usize) {
    let total = text.chars().count();
    if total <= max_chars {
        return (text.to_string(), false, total);
    }
    let head: String = text.chars().take(max_chars).collect();
    (head, true, total)
}

/// Thousands-separated number for truncation markers.
pub fn format_count(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    let first = digits.len() % 3;
    if first > 0 {
        out.push_str(&digits[..first]);
    }
    for chunk in digits[first..].as_bytes().chunks(3) {
        if !out.is_empty() {
            out.push(',');
        }
        out.push_str(std::str::from_utf8(chunk).unwrap_or(""));
    }
    out
}

/// Head-only bound with an explicit truncation marker. Always UTF-8 safe.
pub fn bound_head(text: &str, max_chars: usize) -> String {
    let (head, truncated, total) = truncate_head_chars(text, max_chars);
    if !truncated {
        return head;
    }
    format!(
        "{head}\n[content truncated: showing {} of {} chars]",
        format_count(max_chars),
        format_count(total)
    )
}

/// Head+tail bound used by the ToolRunner safety net: errors and operation
/// summaries often live at the end of a long output.
pub fn bound_tool_output_for_model(text: &str) -> String {
    let total = text.chars().count();
    if total <= MODEL_TOOL_OUTPUT_LIMIT_CHARS {
        return text.to_string();
    }
    let head: String = text.chars().take(MODEL_OUTPUT_HEAD_CHARS).collect();
    let tail: String = text
        .chars()
        .skip(total - MODEL_OUTPUT_TAIL_CHARS)
        .collect();
    let omitted = total - MODEL_OUTPUT_HEAD_CHARS - MODEL_OUTPUT_TAIL_CHARS;
    format!(
        "{head}\n[Tool output truncated for model context: showing head {} + tail {} of {} chars; {} chars omitted]\n{tail}",
        format_count(MODEL_OUTPUT_HEAD_CHARS),
        format_count(MODEL_OUTPUT_TAIL_CHARS),
        format_count(total),
        format_count(omitted)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_is_utf8_safe() {
        // Multi-byte characters: cutting at a byte boundary would panic on
        // String::truncate and corrupt the payload.
        let text = "中文字符".repeat(10);
        let (out, truncated, total) = truncate_head_chars(&text, 7);
        assert!(truncated);
        assert_eq!(total, 40);
        assert_eq!(out.chars().count(), 7);
        // Round-trips through bytes losslessly.
        assert!(std::str::from_utf8(out.as_bytes()).is_ok());
    }

    #[test]
    fn truncate_noop_within_budget() {
        let (out, truncated, total) = truncate_head_chars("abc", 5);
        assert!(!truncated);
        assert_eq!(out, "abc");
        assert_eq!(total, 3);
    }

    #[test]
    fn head_bound_carries_marker() {
        let text = "x".repeat(100);
        let out = bound_head(&text, 40);
        assert!(out.contains("[content truncated: showing 40 of 100 chars]"));
        assert_eq!(out.chars().count() >= 40, true);
    }

    #[test]
    fn tool_output_net_keeps_head_and_tail() {
        let mut text = "h".repeat(70_000);
        text.push_str("TAIL-MARKER");
        let out = bound_tool_output_for_model(&text);
        assert!(out.starts_with('h'));
        assert!(out.ends_with("TAIL-MARKER"));
        assert!(out.contains("[Tool output truncated for model context"));
        assert!(out.contains("of 70,011 chars"));
    }

    #[test]
    fn tool_output_net_noop_below_limit() {
        let text = "x".repeat(1_000);
        let out = bound_tool_output_for_model(&text);
        assert_eq!(out, text);
    }

    #[test]
    fn format_count_separates_thousands() {
        assert_eq!(format_count(183240), "183,240");
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(1000), "1,000");
    }
}
