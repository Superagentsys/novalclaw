//! Semantic contract for structured context checkpoints.
//!
//! Compaction may shrink representation. It must not change the task's
//! authoritative meaning, promote hypotheses into facts, or nest prior
//! checkpoints into the next one.

use crate::providers::ChatMessage;

const CHECKPOINT_MARKER: &str = "[检查点]";
const TASK_MARKER: &str = "[任务]";

const MAX_ITEMS: usize = 8;
const MAX_ITEM_CHARS: usize = 220;

#[derive(Debug, Default, Clone)]
pub struct SemanticCheckpoint {
    pub goal: String,
    pub constraints: Vec<String>,
    pub decisions: Vec<String>,
    pub completed: Vec<String>,
    pub pending: Vec<String>,
    pub current: Vec<String>,
    pub references: Vec<String>,
    pub failures: Vec<String>,
}

pub fn build_structured_checkpoint(messages: &[ChatMessage], summary: &str) -> ChatMessage {
    let semantic = extract_semantic_checkpoint(messages, summary);
    ChatMessage::system(format!(
        "{CHECKPOINT_MARKER} structured checkpoint\n{}",
        render_semantic_checkpoint(&semantic)
    ))
}

pub fn extract_semantic_checkpoint(
    messages: &[ChatMessage],
    summary: &str,
) -> SemanticCheckpoint {
    let mut state = SemanticCheckpoint::default();
    let mut keyed: Vec<(String, String)> = Vec::new();
    let mut constraint_keys: Vec<(String, String)> = Vec::new();

    for message in messages {
        if message.role == "system" && message.content.starts_with(TASK_MARKER) {
            let goal = message
                .content
                .trim_start_matches(TASK_MARKER)
                .trim()
                .to_string();
            if !goal.is_empty() {
                state.goal = clip(&goal);
            }
            continue;
        }
        if message.role == "system" && message.content.starts_with(CHECKPOINT_MARKER) {
            merge_previous_checkpoint(&mut state, &mut keyed, &message.content);
            continue;
        }
        if message.role == "user" {
            absorb_user(
                &mut state,
                &mut keyed,
                &mut constraint_keys,
                &message.content,
            );
            continue;
        }
        if message.role == "tool" {
            if let Some(visible) = visible_tool_text(message) {
                absorb_tool(&mut state, &mut keyed, &visible);
            }
            continue;
        }
        if message.role == "assistant" {
            absorb_assistant_plan(&mut state, &message.content);
        }
    }

    if state.goal.is_empty() {
        if let Some(first_user) = messages.iter().find(|m| m.role == "user") {
            state.goal = clip(&first_user.content);
        }
    }

    apply_keyed_current(&mut state, &keyed);
    apply_constraints(&mut state, &constraint_keys);
    absorb_summary_notes(&mut state, summary, &keyed);
    drop_unverified_completions(&mut state, messages);
    reconcile_plan(&mut state);
    if git_is_dirty(&keyed) {
        state.completed.retain(|item| !looks_like_commit(item));
        push_unique(
            &mut state.pending,
            "commit remains pending; git status is dirty",
        );
    }
    bound_lists(&mut state);
    state
}

pub fn render_semantic_checkpoint(state: &SemanticCheckpoint) -> String {
    let mut body = String::new();
    body.push_str("## Primary Goal\n");
    body.push_str(if state.goal.is_empty() {
        "(none)"
    } else {
        &state.goal
    });
    body.push_str("\n\n## Current Task State\n");
    body.push_str("COMPLETED:\n");
    body.push_str(&bullets(&state.completed));
    body.push_str("PENDING:\n");
    body.push_str(&bullets(&state.pending));
    body.push_str("CURRENT:\n");
    body.push_str(&bullets(&state.current));
    body.push_str("\n## Important Facts\n");
    body.push_str("CONSTRAINTS:\n");
    body.push_str(&bullets(&state.constraints));
    body.push_str("DECISIONS:\n");
    body.push_str(&bullets(&state.decisions));
    body.push_str("REFERENCES:\n");
    body.push_str(&bullets(&state.references));
    body.push_str("FAILURES / BLOCKERS:\n");
    body.push_str(&bullets(&state.failures));
    body
}

fn bullets(items: &[String]) -> String {
    if items.is_empty() {
        return "- (none)\n".to_string();
    }
    items.iter().map(|item| format!("- {item}\n")).collect()
}

fn clip(text: &str) -> String {
    let trimmed = collapse_ws(text);
    if trimmed.chars().count() <= MAX_ITEM_CHARS {
        return trimmed;
    }
    trimmed.chars().take(MAX_ITEM_CHARS).collect()
}

fn collapse_ws(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn push_unique(list: &mut Vec<String>, value: impl Into<String>) {
    let value = clip(&value.into());
    if value.is_empty() {
        return;
    }
    if !list.iter().any(|existing| existing == &value) {
        list.push(value);
    }
}

fn upsert(keyed: &mut Vec<(String, String)>, key: &str, value: String) {
    let value = clip(&value);
    if value.is_empty() {
        return;
    }
    keyed.retain(|(k, _)| k != key);
    keyed.push((key.to_string(), value));
}

fn visible_tool_text(message: &ChatMessage) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(&message.content).ok()?;
    value
        .get("content")
        .and_then(|item| item.as_str())
        .map(ToString::to_string)
}

fn absorb_user(
    state: &mut SemanticCheckpoint,
    keyed: &mut Vec<(String, String)>,
    constraint_keys: &mut Vec<(String, String)>,
    content: &str,
) {
    capture_identifiers(keyed, content);
    absorb_plan_lines(state, content, true);
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(key) = constraint_subject(line) {
            upsert(constraint_keys, key, line.to_string());
        }
        if looks_like_decision(line) {
            push_unique(&mut state.decisions, line);
            if let Some((key, value)) = keyed_config_value(line) {
                upsert(keyed, &key, value);
            }
        }
        if let Some(approval) = capture_approval_scope(line) {
            upsert(keyed, "approval_scope", approval);
        }
    }
}

fn absorb_tool(state: &mut SemanticCheckpoint, keyed: &mut Vec<(String, String)>, content: &str) {
    capture_identifiers(keyed, content);
    if let Some(branch) = capture_prefixed(content, "On branch ") {
        upsert(keyed, "branch", branch.clone());
        state
            .references
            .retain(|item| !item.starts_with("git branch:"));
        push_unique(&mut state.references, format!("git branch: {branch}"));
    }
    if let Some(status) = capture_test_status(content) {
        upsert(keyed, "test_status", status.clone());
        state
            .references
            .retain(|item| !item.to_ascii_lowercase().starts_with("test:"));
        push_unique(&mut state.references, format!("test: {status}"));
        let lower = status.to_ascii_lowercase();
        if lower.contains("fail") || lower.contains("blocked") {
            state
                .failures
                .retain(|item| !item.to_ascii_lowercase().contains("fail"));
            push_unique(&mut state.failures, status);
        } else {
            state.failures.retain(|item| {
                let item_l = item.to_ascii_lowercase();
                !item_l.contains("fail") && !item_l.contains("test")
            });
        }
    }
    if looks_git_dirty(content) {
        upsert(keyed, "git_dirty", "dirty".to_string());
        push_unique(&mut state.current, "git status remains dirty");
    } else if content.contains("nothing to commit") {
        upsert(keyed, "git_dirty", "clean".to_string());
    }
    for path in extract_paths(content) {
        push_unique(&mut state.references, format!("file: {path}"));
        if content.to_ascii_lowercase().contains("modified") || content.contains("file_changed")
        {
            push_unique(&mut state.completed, format!("{path} modified"));
        }
    }
    if let Some(approval) = capture_approval_scope(content) {
        upsert(keyed, "approval_scope", approval.clone());
        push_unique(&mut state.current, approval);
    }
    absorb_tool_errors(state, content);
}

fn absorb_tool_errors(state: &mut SemanticCheckpoint, content: &str) {
    let lower = content.to_ascii_lowercase();
    if !(lower.contains("401")
        || lower.contains("403")
        || lower.contains("credential")
        || lower.contains("error")
        || lower.contains("denied"))
    {
        return;
    }
    if let Some(line) = content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
    {
        push_unique(&mut state.failures, line);
    }
}

fn absorb_assistant_plan(state: &mut SemanticCheckpoint, content: &str) {
    if content.trim_start().starts_with('{') {
        return;
    }
    absorb_plan_lines(state, content, false);
}

fn absorb_plan_lines(state: &mut SemanticCheckpoint, content: &str, from_user: bool) {
    let lower = content.to_ascii_lowercase();
    let numbered = content
        .lines()
        .filter(|line| numbered_item(line.trim()).is_some())
        .count();
    let in_plan = from_user && (lower.contains("plan:") || numbered >= 2);
    if !in_plan && (from_user || numbered < 2) {
        return;
    }
    for line in content.lines() {
        let line = line.trim();
        if let Some(item) = numbered_item(line) {
            push_unique(&mut state.pending, item);
        }
    }
}

fn numbered_item(line: &str) -> Option<String> {
    let bytes = line.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_digit() {
        return None;
    }
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i >= bytes.len() || (bytes[i] != b'.' && bytes[i] != b')') {
        return None;
    }
    Some(clip(line[i + 1..].trim()))
}

fn absorb_summary_notes(
    state: &mut SemanticCheckpoint,
    summary: &str,
    keyed: &[(String, String)],
) {
    let summary = summary.trim();
    if summary.is_empty() {
        return;
    }
    if let Some(parsed) = parse_structured_body(summary) {
        merge_parsed(state, &parsed, false);
        return;
    }
    let mut notes = 0usize;
    for line in summary.lines() {
        let line = line.trim();
        if line.is_empty()
            || is_hypothesis(line)
            || looks_like_unverified_completion(line)
            || contradicts_keyed(line, keyed)
            || (line.contains("max_output_tokens") && !line.contains("request_max_output_tokens"))
        {
            continue;
        }
        if notes >= 4 {
            break;
        }
        push_unique(&mut state.current, line);
        notes += 1;
    }
}

fn contradicts_keyed(line: &str, keyed: &[(String, String)]) -> bool {
    for (key, value) in keyed {
        match key.as_str() {
            "request_max_output_tokens" => {
                if (line.contains("request_max_output_tokens") || line.contains("32K") || line.contains("64K"))
                    && !line.contains(value)
                {
                    return true;
                }
            }
            "branch" => {
                if (line.contains("branch") || line.contains("feature/")) && !line.contains(value) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn merge_previous_checkpoint(
    state: &mut SemanticCheckpoint,
    keyed: &mut Vec<(String, String)>,
    content: &str,
) {
    let body = content.trim_start_matches(CHECKPOINT_MARKER).trim();
    if let Some(parsed) = parse_structured_body(body) {
        if state.goal.is_empty() && !parsed.goal.is_empty() {
            state.goal = parsed.goal.clone();
        }
        merge_parsed(state, &parsed, true);
        for item in parsed
            .current
            .iter()
            .chain(parsed.decisions.iter())
            .chain(parsed.references.iter())
        {
            if let Some((key, value)) = keyed_config_value(item) {
                upsert(keyed, &key, value);
            }
            capture_identifiers(keyed, item);
        }
        return;
    }
    if !body.is_empty() && !body.contains("## Primary Goal") {
        push_unique(&mut state.current, body);
    }
}

fn merge_parsed(state: &mut SemanticCheckpoint, parsed: &SemanticCheckpoint, replace_lists: bool) {
    if replace_lists {
        if !parsed.completed.is_empty() {
            state.completed = parsed.completed.clone();
        }
        if !parsed.pending.is_empty() {
            state.pending = parsed.pending.clone();
        }
        if !parsed.constraints.is_empty() {
            state.constraints = parsed.constraints.clone();
        }
        if !parsed.decisions.is_empty() {
            state.decisions = parsed.decisions.clone();
        }
        if !parsed.references.is_empty() {
            state.references = parsed.references.clone();
        }
        if !parsed.failures.is_empty() {
            state.failures = parsed.failures.clone();
        }
        if !parsed.current.is_empty() {
            state.current = parsed.current.clone();
        }
    } else {
        for item in &parsed.pending {
            push_unique(&mut state.pending, item.clone());
        }
        for item in &parsed.constraints {
            push_unique(&mut state.constraints, item.clone());
        }
        for item in &parsed.current {
            if !is_hypothesis(item) && !looks_like_unverified_completion(item) {
                push_unique(&mut state.current, item.clone());
            }
        }
    }
}

fn parse_structured_body(body: &str) -> Option<SemanticCheckpoint> {
    if !body.contains("## Primary Goal") && !body.contains("COMPLETED:") {
        return None;
    }
    let mut parsed = SemanticCheckpoint::default();
    let mut section = "";
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.eq_ignore_ascii_case("## Primary Goal") {
            section = "goal";
            continue;
        }
        if trimmed.eq_ignore_ascii_case("## Current Task State") {
            section = "current_block";
            continue;
        }
        if trimmed.eq_ignore_ascii_case("## Important Facts") {
            section = "facts";
            continue;
        }
        if trimmed.eq_ignore_ascii_case("COMPLETED:") {
            section = "completed";
            continue;
        }
        if trimmed.eq_ignore_ascii_case("PENDING:") {
            section = "pending";
            continue;
        }
        if trimmed.eq_ignore_ascii_case("CURRENT:") {
            section = "current";
            continue;
        }
        if trimmed.eq_ignore_ascii_case("CONSTRAINTS:") {
            section = "constraints";
            continue;
        }
        if trimmed.eq_ignore_ascii_case("DECISIONS:") {
            section = "decisions";
            continue;
        }
        if trimmed.eq_ignore_ascii_case("REFERENCES:") {
            section = "references";
            continue;
        }
        if trimmed.eq_ignore_ascii_case("FAILURES / BLOCKERS:")
            || trimmed.eq_ignore_ascii_case("FAILURES/BLOCKERS:")
        {
            section = "failures";
            continue;
        }
        let item = trimmed.strip_prefix("- ").unwrap_or(trimmed);
        if item.is_empty() || item == "(none)" {
            continue;
        }
        match section {
            "goal" => {
                if parsed.goal.is_empty() {
                    parsed.goal = clip(item);
                }
            }
            "completed" => push_unique(&mut parsed.completed, item),
            "pending" => push_unique(&mut parsed.pending, item),
            "current" | "current_block" => push_unique(&mut parsed.current, item),
            "constraints" => push_unique(&mut parsed.constraints, item),
            "decisions" => push_unique(&mut parsed.decisions, item),
            "references" => push_unique(&mut parsed.references, item),
            "failures" => push_unique(&mut parsed.failures, item),
            _ => {}
        }
    }
    Some(parsed)
}

fn apply_keyed_current(state: &mut SemanticCheckpoint, keyed: &[(String, String)]) {
    for (key, value) in keyed {
        match key.as_str() {
            "request_max_output_tokens" => {
                state
                    .current
                    .retain(|item| !item.contains("request_max_output_tokens"));
                state
                    .decisions
                    .retain(|item| !item.contains("request_max_output_tokens"));
                push_unique(
                    &mut state.current,
                    format!("request_max_output_tokens = {value}"),
                );
                push_unique(
                    &mut state.decisions,
                    format!("request_max_output_tokens = {value}"),
                );
            }
            "branch" => {
                state.current.retain(|item| !item.starts_with("branch:"));
                push_unique(&mut state.current, format!("branch: {value}"));
            }
            "model" => {
                state.current.retain(|item| !item.starts_with("model:"));
                push_unique(&mut state.current, format!("model: {value}"));
            }
            "provider" => push_unique(&mut state.current, format!("provider: {value}")),
            "test_status" => {
                state
                    .current
                    .retain(|item| !item.to_ascii_lowercase().starts_with("test:"));
                push_unique(&mut state.current, format!("test: {value}"));
            }
            "approval_scope" => push_unique(&mut state.current, value.clone()),
            _ => {}
        }
    }
}

fn apply_constraints(state: &mut SemanticCheckpoint, constraint_keys: &[(String, String)]) {
    state.constraints.clear();
    for (_, value) in constraint_keys {
        if still_restrictive(value) {
            push_unique(&mut state.constraints, value.clone());
        }
    }
}

fn still_restrictive(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("do not")
        || lower.contains("don't")
        || lower.contains("不得")
        || lower.contains("不要")
        || lower.contains("only change")
        || lower.contains("preserve")
        || lower.contains("no provider")
        || lower.contains("must not")
}

fn constraint_subject(line: &str) -> Option<&'static str> {
    let lower = line.to_ascii_lowercase();
    if !(still_restrictive(line)
        || lower.contains("you may")
        || lower.contains("restriction is lifted")
        || lower.contains("is allowed"))
    {
        return None;
    }
    if lower.contains("push") {
        return Some("push");
    }
    if lower.contains("dingtalk") {
        return Some("dingtalk");
    }
    if lower.contains("security") {
        return Some("security");
    }
    if lower.contains("provider call") || lower.contains("session open") {
        return Some("session_open");
    }
    if lower.contains("only change") || lower.contains("subsystem") {
        return Some("scope");
    }
    Some("other")
}

fn looks_like_decision(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("request_max_output_tokens")
        || lower.contains("use model")
        || lower.contains("changed")
        || lower.contains("decision:")
        || lower.contains("set provider")
}

fn keyed_config_value(text: &str) -> Option<(String, String)> {
    if let Some(value) = capture_after(text, "request_max_output_tokens") {
        if looks_like_limit(&value) {
            return Some(("request_max_output_tokens".into(), value));
        }
    }
    if let Some(value) = capture_prefixed(text, "branch:") {
        return Some(("branch".into(), value));
    }
    None
}

fn capture_identifiers(keyed: &mut Vec<(String, String)>, text: &str) {
    if let Some(value) = capture_after(text, "request_max_output_tokens") {
        if looks_like_limit(&value) {
            upsert(keyed, "request_max_output_tokens", value);
        }
    }
    if text.contains("deepseek-v4-flash") {
        upsert(keyed, "model", "deepseek-v4-flash".to_string());
    }
}

fn looks_like_limit(value: &str) -> bool {
    let trimmed = value.trim_end_matches(|c: char| !c.is_ascii_alphanumeric());
    let digits = trimmed.trim_end_matches(|c: char| matches!(c, 'k' | 'K' | 'm' | 'M'));
    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
}

fn capture_after(text: &str, key: &str) -> Option<String> {
    let pos = text.find(key)?;
    let rest = text[pos + key.len()..].trim_start();
    let rest = rest.trim_start_matches(['=', ':', ' ']);
    let token = rest
        .split(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == '"' || c == '\'')
        .find(|part| !part.is_empty())?;
    if token == "max_output_tokens" {
        return None;
    }
    Some(token.trim_end_matches('.').to_string())
}

fn capture_prefixed(text: &str, prefix: &str) -> Option<String> {
    let pos = text.find(prefix)?;
    let rest = text[pos + prefix.len()..].trim_start();
    let token = rest
        .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
        .find(|part| !part.is_empty())?;
    Some(token.to_string())
}

fn capture_test_status(text: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    if lower.contains("not_run") || lower.contains("not run") {
        return Some(clip_status(text, "NOT_RUN"));
    }
    if lower.contains("blocked") {
        return Some(clip_status(text, "BLOCKED"));
    }
    if lower.contains("skipped") {
        return Some(clip_status(text, "SKIPPED"));
    }
    if lower.contains("failed") || (lower.contains("fail") && lower.contains("test")) {
        if let Some(line) = first_line_containing(text, "fail") {
            return Some(clip(line));
        }
        return Some("FAIL".to_string());
    }
    if lower.contains("passed") {
        if let Some(line) = first_line_containing(text, "passed") {
            return Some(clip(line));
        }
        return Some("PASS".to_string());
    }
    None
}

fn clip_status(text: &str, fallback: &str) -> String {
    first_line_containing(text, &fallback.to_ascii_lowercase())
        .map(clip)
        .filter(|line| !line.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

fn first_line_containing<'a>(text: &'a str, needle: &str) -> Option<&'a str> {
    text.lines()
        .map(str::trim)
        .find(|line| line.to_ascii_lowercase().contains(needle))
}

fn extract_paths(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in text.split_whitespace() {
        let token = raw.trim_matches(|c: char| {
            matches!(c, ',' | ';' | '"' | '\'' | '(' | ')' | '[' | ']' | '`')
        });
        if token.contains('/')
            && (token.ends_with(".rs")
                || token.ends_with(".ts")
                || token.ends_with(".tsx")
                || token.ends_with(".toml"))
        {
            push_unique(&mut out, token);
        }
    }
    out
}

fn capture_approval_scope(text: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    if lower.contains("approved once")
        || lower.contains("this command only")
        || lower.contains("this once")
    {
        return Some(clip(text));
    }
    None
}

fn looks_git_dirty(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("changes not staged")
        || lower.contains("changes to be committed")
        || lower.contains("untracked files")
        || lower.contains("git status remains dirty")
}

fn git_is_dirty(keyed: &[(String, String)]) -> bool {
    keyed.iter().any(|(k, v)| k == "git_dirty" && v == "dirty")
}

fn looks_like_commit(item: &str) -> bool {
    let lower = item.to_ascii_lowercase();
    lower.contains("commit") && !lower.contains("pending")
}

fn is_hypothesis(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("可能")
        || lower.contains("也许")
        || lower.contains("maybe")
        || lower.contains("might be")
        || lower.contains("probably")
        || lower.contains("hypothesis")
        || lower.contains("speculative")
        || lower.contains("i think")
        || lower.contains("root_cause =")
}

fn looks_like_unverified_completion(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if lower.contains("shell commands are approved") {
        return true;
    }
    if lower.contains("all tests passed")
        || lower.contains("everything is done")
        || lower == "done"
        || lower.contains("已全部完成")
        || (lower.contains("complete") && !lower.contains("pending"))
    {
        return true;
    }
    lower.contains("passed") && !lower.chars().any(|c| c.is_ascii_digit())
}

fn reconcile_plan(state: &mut SemanticCheckpoint) {
    state.pending.retain(|pending| {
        let pending_l = pending.to_ascii_lowercase();
        !state.completed.iter().any(|done| {
            let done_l = done.to_ascii_lowercase();
            pending_l.split_whitespace().any(|token| {
                token.len() > 2 && (done_l.contains(token) || token.contains('/') && done_l.contains(token))
            }) && (pending_l.contains("modify")
                || pending_l.contains("test")
                || pending_l.contains("commit")
                || done_l.contains("modified"))
        })
    });
}

fn drop_unverified_completions(state: &mut SemanticCheckpoint, messages: &[ChatMessage]) {
    let tool_backed = messages.iter().any(|m| m.role == "tool");
    if tool_backed {
        return;
    }
    state.completed.retain(|item| {
        !looks_like_unverified_completion(item) && !item.to_ascii_lowercase().contains("done")
    });
}

fn bound_lists(state: &mut SemanticCheckpoint) {
    for list in [
        &mut state.constraints,
        &mut state.decisions,
        &mut state.completed,
        &mut state.pending,
        &mut state.current,
        &mut state.references,
        &mut state.failures,
    ] {
        if list.len() > MAX_ITEMS {
            let keep = list.split_off(list.len() - MAX_ITEMS);
            *list = keep;
        }
    }
}
