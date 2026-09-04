//! Accessibility tree helpers. Drivers dump raw nodes; this module filters,
//! assigns stable ids, and resolves click targets by ref or name.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct A11yNode {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    #[serde(default)]
    pub width: i32,
    #[serde(default)]
    pub height: i32,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

impl A11yNode {
    pub fn center(&self) -> (i32, i32) {
        (
            self.x + self.width.max(1) / 2,
            self.y + self.height.max(1) / 2,
        )
    }
}

pub fn finalize_nodes(mut nodes: Vec<A11yNode>, max_nodes: usize) -> Vec<A11yNode> {
    nodes.retain(is_useful);
    nodes.truncate(max_nodes.max(1));
    for (index, node) in nodes.iter_mut().enumerate() {
        node.id = format!("@e{}", index + 1);
        if node.role.trim().is_empty() {
            node.role = "unknown".into();
        }
        node.name = node.name.trim().to_string();
        if let Some(value) = node.value.as_mut() {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() {
                node.value = None;
            } else {
                *value = trimmed;
            }
        }
    }
    nodes
}

pub fn parse_json_nodes(raw: &str) -> Result<Vec<A11yNode>, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "null" {
        return Ok(Vec::new());
    }
    let value: serde_json::Value =
        serde_json::from_str(trimmed).map_err(|e| format!("a11y json: {e}"))?;
    if let Some(error) = value.get("error").and_then(|v| v.as_str()) {
        return Err(error.to_string());
    }
    let items = if value.is_array() {
        value
    } else if value.is_object() {
        serde_json::Value::Array(vec![value])
    } else {
        return Ok(Vec::new());
    };
    serde_json::from_value(items).map_err(|e| format!("a11y nodes: {e}"))
}

pub fn parse_tsv_nodes(raw: &str) -> Vec<A11yNode> {
    let mut nodes = Vec::new();
    for line in raw.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\u{1f}').collect();
        if parts.len() < 6 {
            continue;
        }
        let x = parts[2].parse().unwrap_or(0);
        let y = parts[3].parse().unwrap_or(0);
        let width = parts[4].parse().unwrap_or(0);
        let height = parts[5].parse().unwrap_or(0);
        let enabled = parts
            .get(6)
            .map(|flag| !matches!(*flag, "false" | "0" | "no"))
            .unwrap_or(true);
        let value = parts.get(7).map(|text| text.trim().to_string()).and_then(|text| {
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        });
        nodes.push(A11yNode {
            id: String::new(),
            role: parts[1].trim().to_string(),
            name: parts[0].replace('\t', " ").trim().to_string(),
            value,
            x,
            y,
            width,
            height,
            enabled,
        });
    }
    nodes
}

pub fn find_by_ref<'a>(nodes: &'a [A11yNode], element_ref: &str) -> Option<&'a A11yNode> {
    let want = normalize_ref(element_ref);
    if want.is_empty() {
        return None;
    }
    nodes.iter().find(|node| normalize_ref(&node.id) == want)
}

pub fn find_by_name<'a>(
    nodes: &'a [A11yNode],
    name: &str,
    role: Option<&str>,
) -> Result<&'a A11yNode, String> {
    let needle = normalize_label(name);
    if needle.is_empty() {
        return Err("missing name".into());
    }
    let role_needle = role.map(normalize_label).filter(|text| !text.is_empty());
    let matches: Vec<&A11yNode> = nodes
        .iter()
        .filter(|node| {
            let role_ok = role_needle
                .as_ref()
                .map(|want| normalize_label(&node.role).contains(want))
                .unwrap_or(true);
            role_ok && label_matches(&node.name, &needle)
        })
        .collect();
    let exact: Vec<&A11yNode> = matches
        .iter()
        .copied()
        .filter(|node| normalize_label(&node.name) == needle)
        .collect();
    pick_unique(if exact.is_empty() { matches } else { exact }, name)
}

fn pick_unique<'a>(mut matches: Vec<&'a A11yNode>, name: &str) -> Result<&'a A11yNode, String> {
    if matches.is_empty() {
        return Err(format!(
            "no accessibility node named '{name}'. Call action=snapshot and click with ref."
        ));
    }
    if matches.len() == 1 {
        return Ok(matches[0]);
    }
    let enabled: Vec<&A11yNode> = matches
        .iter()
        .copied()
        .filter(|node| node.enabled)
        .collect();
    if enabled.len() == 1 {
        return Ok(enabled[0]);
    }
    if enabled.len() > 1 {
        matches = enabled;
    }
    let preview = matches
        .into_iter()
        .take(6)
        .map(|node| format!("{}:{}:{:?}", node.id, node.role, node.name))
        .collect::<Vec<_>>()
        .join(", ");
    Err(format!(
        "ambiguous name '{name}' matches multiple nodes ({preview}). Click with ref."
    ))
}

fn label_matches(name: &str, needle: &str) -> bool {
    let haystack = normalize_label(name);
    haystack == needle || haystack.contains(needle)
}

fn normalize_label(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

fn normalize_ref(value: &str) -> String {
    value.trim().trim_start_matches('@').to_ascii_lowercase()
}

#[allow(dead_code)]
pub fn find_handoff_banner(nodes: &[A11yNode]) -> Option<&'static str> {
    for node in nodes {
        let mut haystack = node.name.clone();
        if let Some(value) = &node.value {
            haystack.push(' ');
            haystack.push_str(value);
        }
        for (needle, reason) in HANDOFF_BANNERS {
            if haystack.contains(needle) {
                return Some(reason);
            }
        }
    }
    None
}

const HANDOFF_BANNERS: &[(&str, &str)] = &[
    ("网络异常", "network_or_session"),
    ("网络连接失败", "network_or_session"),
    ("重新连接", "network_or_session"),
    ("加载失败", "network_or_session"),
    ("会话已过期", "network_or_session"),
    ("会话过期", "network_or_session"),
    ("请重新登录", "network_or_session"),
    ("登录已失效", "network_or_session"),
    ("弱网", "network_or_session"),
    ("无法连接", "network_or_session"),
    ("连接已断开", "network_or_session"),
    ("session expired", "network_or_session"),
    ("reconnect", "network_or_session"),
    ("disconnected", "network_or_session"),
];

fn is_useful(node: &A11yNode) -> bool {
    if node.width <= 0 || node.height <= 0 {
        return false;
    }
    let role = node.role.to_ascii_lowercase();
    if INTERACTIVE_HINTS
        .iter()
        .any(|hint| role.contains(hint))
    {
        return true;
    }
    if node.name.trim().is_empty() {
        return false;
    }
    !NOISE_ROLES.iter().any(|hint| role.contains(hint))
}

const INTERACTIVE_HINTS: &[&str] = &[
    "button",
    "textfield",
    "text field",
    "textfield",
    "axtextfield",
    "edit",
    "entry",
    "checkbox",
    "radio",
    "combo",
    "menu",
    "tab",
    "link",
    "hyperlink",
    "slider",
    "popup",
    "pop up",
    "toolbar",
    "search",
    "row",
    "cell",
    "listitem",
    "list item",
    "treeitem",
    "tree item",
    "toggle",
    "switch",
    "spin",
    "incrementor",
    "disclosure",
    "splitbutton",
    "document",
];

const NOISE_ROLES: &[&str] = &[
    "static",
    "image",
    "group",
    "window",
    "pane",
    "filler",
    "separator",
    "thumb",
    "scroll area",
    "scrollarea",
    "layout",
    "unknown",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, role: &str, name: &str, x: i32, y: i32) -> A11yNode {
        A11yNode {
            id: id.into(),
            role: role.into(),
            name: name.into(),
            value: None,
            x,
            y,
            width: 80,
            height: 24,
            enabled: true,
        }
    }

    #[test]
    fn finalize_assigns_refs_and_drops_empty_static() {
        let nodes = finalize_nodes(
            vec![
                node("", "static text", "", 0, 0),
                node("", "AXButton", "发送", 10, 20),
            ],
            80,
        );
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].id, "@e1");
        assert_eq!(nodes[0].center(), (50, 32));
    }

    #[test]
    fn json_and_tsv_dumps_parse() {
        let nodes = parse_json_nodes(
            r#"[{"role":"button","name":"OK","x":1,"y":2,"width":10,"height":8}]"#,
        )
        .unwrap();
        assert_eq!(nodes[0].name, "OK");
        let tsv = "发送\u{1f}AXButton\u{1f}10\u{1f}20\u{1f}80\u{1f}24\u{1f}true\u{1f}";
        assert_eq!(parse_tsv_nodes(tsv)[0].role, "AXButton");
    }

    #[test]
    fn name_lookup_prefers_exact_and_reports_ambiguous() {
        let nodes = vec![
            node("@e1", "button", "发送", 0, 0),
            node("@e2", "button", "发送给所有人", 100, 0),
        ];
        assert_eq!(find_by_name(&nodes, "发送", None).unwrap().id, "@e1");
        let err = find_by_name(&nodes, "发", None).unwrap_err();
        assert!(err.contains("ambiguous"), "{err}");
        assert_eq!(find_by_ref(&nodes, "e1").unwrap().name, "发送");
    }
}
