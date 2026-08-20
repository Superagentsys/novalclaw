use super::catalog::{
    list_skill_catalog, normalize_skill_id, source_badge_label, SKILL_ID_PREFIX, SYSTEM_ID_PREFIX,
};
use crate::config::Config;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CommandPaletteItem {
    pub id: String,
    pub kind: String,
    pub display_name: String,
    pub description: String,
    pub source: String,
    pub source_badge: String,
    pub command_alias: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CommandPalette {
    pub generation: u64,
    pub open_skills_enabled: bool,
    pub system: Vec<CommandPaletteItem>,
    pub skills: Vec<CommandPaletteItem>,
    pub skills_empty_reason: Option<String>,
}

pub fn system_command_items() -> Vec<CommandPaletteItem> {
    vec![
        CommandPaletteItem {
            id: format!("{SYSTEM_ID_PREFIX}help"),
            kind: "system".into(),
            display_name: "Help".into(),
            description: "Show available slash commands".into(),
            source: "system".into(),
            source_badge: source_badge_label("system").into(),
            command_alias: "/help".into(),
            aliases: vec!["/?".into()],
            enabled: true,
        },
        CommandPaletteItem {
            id: format!("{SYSTEM_ID_PREFIX}skills"),
            kind: "system".into(),
            display_name: "Skills".into(),
            description: "List installed skills in the catalog".into(),
            source: "system".into(),
            source_badge: source_badge_label("system").into(),
            command_alias: "/skills".into(),
            aliases: Vec::new(),
            enabled: true,
        },
        CommandPaletteItem {
            id: "tool:contract-review".into(),
            kind: "system_tool".into(),
            display_name: "合同智能审核".into(),
            description: "上传合同进行关键条款审查、风险识别、缺漏检查和版本比对".into(),
            source: "system_tool".into(),
            source_badge: "系统工具".into(),
            command_alias: "/contract".into(),
            aliases: vec!["/合同".into(), "/合同审核".into()],
            enabled: true,
        },
    ]
}

pub fn list_command_palette(config: &Config) -> CommandPalette {
    let catalog = list_skill_catalog(config);
    let skills = if !catalog.open_skills_enabled {
        Vec::new()
    } else {
        catalog
            .entries
            .iter()
            .filter(|entry| entry.runtime_visible)
            .map(|entry| CommandPaletteItem {
                id: entry.id.clone(),
                kind: "skill".into(),
                display_name: entry.display_name.clone(),
                description: entry.description.clone(),
                source: entry.source.clone(),
                source_badge: source_badge_label(&entry.source).to_string(),
                command_alias: entry.command_alias.clone(),
                aliases: Vec::new(),
                enabled: entry.enabled && entry.runtime_visible,
            })
            .collect()
    };
    let skills_empty_reason = if !catalog.open_skills_enabled {
        Some("技能功能已关闭".to_string())
    } else if skills.is_empty() {
        Some("暂无可用技能".to_string())
    } else {
        None
    };
    CommandPalette {
        generation: catalog.generation,
        open_skills_enabled: catalog.open_skills_enabled,
        system: system_command_items(),
        skills,
        skills_empty_reason,
    }
}

pub fn filter_command_palette(palette: &CommandPalette, query: &str) -> CommandPalette {
    let needle = query.trim().trim_start_matches('/').to_lowercase();
    let keep = |item: &CommandPaletteItem| {
        if needle.is_empty() {
            return true;
        }
        item.display_name.to_lowercase().contains(&needle)
            || item.id.to_lowercase().contains(&needle)
            || item.command_alias.to_lowercase().contains(&needle)
            || item
                .aliases
                .iter()
                .any(|alias| alias.to_lowercase().contains(&needle))
            || item.description.to_lowercase().contains(&needle)
    };
    CommandPalette {
        generation: palette.generation,
        open_skills_enabled: palette.open_skills_enabled,
        system: palette
            .system
            .iter()
            .filter(|item| keep(item))
            .cloned()
            .collect(),
        skills: palette
            .skills
            .iter()
            .filter(|item| keep(item))
            .cloned()
            .collect(),
        skills_empty_reason: palette.skills_empty_reason.clone(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedSlashCommand {
    System { id: String, rest: String },
    SystemTool { id: String, rest: String },
    Skill { id: String, rest: String },
    Unknown { token: String, rest: String },
}

/// Shared parser for CLI and Desktop. `line` is a full composer/CLI line.
pub fn parse_slash_command(line: &str) -> Option<ParsedSlashCommand> {
    let trimmed = line.trim();
    if !trimmed.starts_with('/') {
        return None;
    }
    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let token = parts.next().unwrap_or("");
    let rest = parts.next().unwrap_or("").trim().to_string();
    if token.eq_ignore_ascii_case("/help") || token == "/?" {
        return Some(ParsedSlashCommand::System {
            id: format!("{SYSTEM_ID_PREFIX}help"),
            rest,
        });
    }
    if token.eq_ignore_ascii_case("/skills") {
        return Some(ParsedSlashCommand::System {
            id: format!("{SYSTEM_ID_PREFIX}skills"),
            rest,
        });
    }
    if token.eq_ignore_ascii_case("/contract") || token == "/合同" || token == "/合同审核" {
        return Some(ParsedSlashCommand::SystemTool {
            id: "tool:contract-review".into(),
            rest,
        });
    }
    if token.eq_ignore_ascii_case("/skill") {
        let mut rest_parts = rest.splitn(2, char::is_whitespace);
        let slug = rest_parts.next().unwrap_or("").trim();
        let leftover = rest_parts.next().unwrap_or("").trim().to_string();
        if slug.is_empty() {
            return Some(ParsedSlashCommand::Unknown {
                token: token.to_string(),
                rest,
            });
        }
        return Some(ParsedSlashCommand::Skill {
            id: normalize_skill_id(slug),
            rest: leftover,
        });
    }
    if token.len() > 1 {
        let slug = token.trim_start_matches('/');
        if slug.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_' || byte == b'/'
        }) {
            return Some(ParsedSlashCommand::Skill {
                id: format!("{SKILL_ID_PREFIX}{slug}"),
                rest,
            });
        }
    }
    Some(ParsedSlashCommand::Unknown {
        token: token.to_string(),
        rest,
    })
}

#[cfg(test)]
mod command_tests {
    use super::*;

    #[test]
    fn contract_review_is_always_a_system_tool() {
        let mut config = Config::default();
        config.skills.open_skills_enabled = false;
        let palette = list_command_palette(&config);
        let item = palette
            .system
            .iter()
            .find(|item| item.id == "tool:contract-review")
            .unwrap();
        assert_eq!(item.kind, "system_tool");
        assert_eq!(item.source_badge, "系统工具");
    }

    #[test]
    fn contract_review_aliases_parse_and_search() {
        assert!(matches!(
            parse_slash_command("/合同审核"),
            Some(ParsedSlashCommand::SystemTool { .. })
        ));
        let palette = list_command_palette(&Config::default());
        assert_eq!(
            filter_command_palette(&palette, "/合同审核").system.len(),
            1
        );
        assert_eq!(
            filter_command_palette(&palette, "/contract").system.len(),
            1
        );
    }
}

/// Current composer token at `cursor` if it starts with `/`.
pub fn command_token_at(input: &str, cursor: usize) -> Option<(usize, usize, String)> {
    let cursor = cursor.min(input.len());
    let before = &input[..cursor];
    let start = before
        .rfind(|ch: char| ch.is_whitespace())
        .map(|idx| {
            idx + before[idx..]
                .chars()
                .next()
                .map(|ch| ch.len_utf8())
                .unwrap_or(1)
        })
        .unwrap_or(0);
    let after = &input[cursor..];
    let end_rel = after
        .find(|ch: char| ch.is_whitespace())
        .unwrap_or(after.len());
    let end = cursor + end_rel;
    let token = &input[start..end];
    if token.starts_with('/') {
        Some((start, end, token.to_string()))
    } else {
        None
    }
}
