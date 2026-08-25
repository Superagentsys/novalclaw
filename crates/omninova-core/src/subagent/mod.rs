//! Subagent definitions.
//!
//! A subagent is described by a name, a description telling the parent when to
//! use it, an optional tool allowlist, and a system prompt. Definitions come
//! from three places, in increasing precedence:
//!
//!   1. built-in presets, always available with no configuration;
//!   2. markdown files under `{workspace}/.omninova/agents/*.md`, so a team can
//!      version its own agents alongside the code;
//!   3. `[agents.<name>]` tables in `omninova.toml`.
//!
//! Definitions are folded into `Config::agents` before routing, so everything
//! downstream (route resolution, per-agent prompt, tool allowlist, spawn depth,
//! audit, concurrency limits) works on them unchanged.

use crate::config::{Config, DelegateAgentConfig};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use tracing::warn;

/// Where a definition came from, for diagnostics and listings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubagentSource {
    Builtin,
    File(PathBuf),
}

#[derive(Debug, Clone)]
pub struct SubagentDefinition {
    pub name: String,
    /// Shown to the parent agent so it can pick the right subagent. This is the
    /// single most important field: a vague description means the parent will
    /// either never use the subagent or use it for the wrong thing.
    pub description: String,
    /// Explicit tool allowlist. `None` means "decide from `read_only`".
    pub tools: Option<Vec<String>>,
    /// Restrict to tools that cannot change anything.
    pub read_only: bool,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub max_iterations: Option<usize>,
    pub system_prompt: String,
    pub source: SubagentSource,
}

impl SubagentDefinition {
    /// Tool names this subagent may use. An empty vec means "no restriction".
    pub fn resolve_allowed_tools(&self) -> Vec<String> {
        if let Some(tools) = &self.tools {
            return tools.clone();
        }
        if self.read_only {
            return crate::tools::registry::read_only_tool_names()
                .into_iter()
                .map(str::to_string)
                .collect();
        }
        Vec::new()
    }

    pub fn to_delegate_config(&self) -> DelegateAgentConfig {
        let system_prompt = if self.system_prompt.trim().is_empty() {
            None
        } else {
            Some(self.system_prompt.clone())
        };
        DelegateAgentConfig {
            provider: self.provider.clone(),
            model: self.model.clone(),
            system_prompt,
            description: Some(self.description.clone()),
            max_depth: None,
            agentic: true,
            allowed_tools: self.resolve_allowed_tools(),
            max_iterations: self.max_iterations,
            workspace_dir: None,
        }
    }
}

const EXPLORE_PROMPT: &str = "你是代码库探索专员。你的任务是快速、准确地找到信息并如实汇报，不做修改。\n\
工作方式：优先用 glob_search 按文件名定位、用 content_search 按内容检索，命中后再用 file_read 精读关键片段。\n\
汇报要求：给出结论 + 证据（文件路径与行号）。没找到就直说没找到，不要猜测、不要编造路径或函数名。\n\
你没有写入权限，也不要建议调用者放宽权限。";

const GENERAL_PROMPT: &str = "你是通用执行子智能体，负责独立完成父智能体交给你的一个完整任务。\n\
你看不到父智能体的对话历史，任务描述里就是你拥有的全部信息；信息不足时基于合理默认推进，并在汇报里说明你做了哪些假设。\n\
汇报要求：先给结论和实际改动（涉及的文件、命令、结果），再给必要的细节。失败要如实说明失败在哪一步、原因是什么。";

const SHELL_PROMPT: &str = "你是命令执行专员，负责 git 操作、构建、测试、依赖安装这类终端任务。\n\
执行前先确认工作目录与命令的副作用；命令失败时读完整错误输出再决定下一步，不要盲目重试同一条命令。\n\
汇报要求：给出实际执行的命令、退出码和关键输出，不要只说“已完成”。";

pub fn builtin_definitions() -> Vec<SubagentDefinition> {
    vec![
        SubagentDefinition {
            name: "explore".to_string(),
            description: "只读的代码库探索智能体。用于按文件名或内容查找代码、回答“这个功能在哪里实现”“某个流程怎么走”这类问题。它不会修改任何文件，适合在动手改代码前收集上下文。".to_string(),
            tools: None,
            read_only: true,
            provider: None,
            model: None,
            max_iterations: Some(20),
            system_prompt: EXPLORE_PROMPT.to_string(),
            source: SubagentSource::Builtin,
        },
        SubagentDefinition {
            name: "general-purpose".to_string(),
            description: "通用子智能体，拥有完整工具集。用于可以独立完成、且不需要频繁和用户确认的多步任务，例如“把这个模块补上测试并跑通”。".to_string(),
            tools: None,
            read_only: false,
            provider: None,
            model: None,
            max_iterations: None,
            system_prompt: GENERAL_PROMPT.to_string(),
            source: SubagentSource::Builtin,
        },
        SubagentDefinition {
            name: "shell".to_string(),
            description: "命令执行专员。用于 git 操作、构建、跑测试、装依赖等需要连续执行终端命令并根据输出调整的任务。".to_string(),
            tools: Some(vec![
                "shell".to_string(),
                "git_operations".to_string(),
                "file_read".to_string(),
                "file_list".to_string(),
                "glob_search".to_string(),
                "content_search".to_string(),
            ]),
            read_only: false,
            provider: None,
            model: None,
            max_iterations: None,
            system_prompt: SHELL_PROMPT.to_string(),
            source: SubagentSource::Builtin,
        },
    ]
}

/// Accepts both `tools: [a, b]` and `tools: "a, b"`, matching the two styles
/// people copy in from other agent tools.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ToolsField {
    List(Vec<String>),
    Csv(String),
}

impl ToolsField {
    fn into_vec(self) -> Vec<String> {
        match self {
            Self::List(items) => items
                .into_iter()
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect(),
            Self::Csv(raw) => raw
                .split(',')
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct Frontmatter {
    name: Option<String>,
    description: Option<String>,
    tools: Option<ToolsField>,
    #[serde(default, alias = "readonly", alias = "readOnly")]
    read_only: bool,
    provider: Option<String>,
    model: Option<String>,
    #[serde(alias = "maxIterations")]
    max_iterations: Option<usize>,
}

/// Directory holding per-workspace agent definitions.
pub fn definitions_dir(config: &Config) -> PathBuf {
    config
        .agent_defaults_extended
        .subagents
        .as_ref()
        .and_then(|subagents| subagents.definitions_dir.as_ref())
        .map(PathBuf::from)
        .unwrap_or_else(|| config.workspace_dir.join(".omninova").join("agents"))
}

pub fn parse_definition(path: &Path, raw: &str) -> anyhow::Result<SubagentDefinition> {
    let fallback_name = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let (frontmatter, body) = split_frontmatter(raw);
    let meta: Frontmatter = match frontmatter {
        Some(text) => serde_yaml::from_str(text)
            .map_err(|e| anyhow::anyhow!("invalid frontmatter in {}: {e}", path.display()))?,
        None => Frontmatter::default(),
    };

    let name = normalize_name(meta.name.as_deref().unwrap_or(&fallback_name));
    if name.is_empty() {
        anyhow::bail!("subagent in {} has an empty name", path.display());
    }

    Ok(SubagentDefinition {
        description: meta
            .description
            .filter(|text| !text.trim().is_empty())
            .unwrap_or_else(|| format!("子智能体 {name}（定义未提供描述）")),
        tools: meta.tools.map(ToolsField::into_vec),
        read_only: meta.read_only,
        provider: meta.provider,
        model: meta.model,
        max_iterations: meta.max_iterations,
        system_prompt: body.trim().to_string(),
        source: SubagentSource::File(path.to_path_buf()),
        name,
    })
}

/// Split leading YAML frontmatter from the markdown body. Returns `(None, raw)`
/// when the file has no frontmatter, so a plain markdown file still works.
fn split_frontmatter(raw: &str) -> (Option<&str>, &str) {
    let trimmed = raw.trim_start_matches('\u{feff}');
    let Some(rest) = trimmed.strip_prefix("---") else {
        return (None, trimmed);
    };
    let rest = rest.trim_start_matches(['\r', '\n']);
    match rest.split_once("\n---") {
        Some((frontmatter, body)) => (
            Some(frontmatter),
            body.trim_start_matches(['-', '\r', '\n']),
        ),
        None => (None, trimmed),
    }
}

/// Agent names travel through tool-call arguments and session keys, so keep
/// them to a predictable shape instead of trusting the filename.
fn normalize_name(raw: &str) -> String {
    raw.trim()
        .chars()
        .filter_map(|ch| match ch {
            'a'..='z' | '0'..='9' | '-' | '_' => Some(ch),
            'A'..='Z' => Some(ch.to_ascii_lowercase()),
            ' ' | '.' => Some('-'),
            _ => None,
        })
        .collect()
}

pub fn load_definitions_from_dir(dir: &Path) -> Vec<SubagentDefinition> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut definitions = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }
        let Ok(raw) = std::fs::read_to_string(&path) else {
            warn!("failed to read subagent definition {}", path.display());
            continue;
        };
        match parse_definition(&path, &raw) {
            Ok(definition) => definitions.push(definition),
            Err(e) => warn!("skipping subagent definition: {e}"),
        }
    }
    definitions.sort_by(|a, b| a.name.cmp(&b.name));
    definitions
}

/// Fold built-in and file-based definitions into `config.agents`.
///
/// Existing `[agents.<name>]` tables win, so a TOML entry can override a
/// built-in without the file having to be deleted. Returns the names that were
/// added, for logging.
pub fn merge_into_config(config: &mut Config) -> Vec<String> {
    let mut added = Vec::new();
    let from_files = load_definitions_from_dir(&definitions_dir(config));

    // Built-ins first so a file with the same name replaces them.
    for definition in builtin_definitions().into_iter().chain(from_files) {
        if config.agents.contains_key(&definition.name) {
            continue;
        }
        config
            .agents
            .insert(definition.name.clone(), definition.to_delegate_config());
        added.push(definition.name.clone());
    }
    added
}

/// A subagent the parent may hand work to.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SubagentTarget {
    pub name: String,
    pub description: String,
}

/// Everything the current agent may delegate to, sorted by name. Excludes the
/// caller so an agent cannot hand work back to itself.
pub fn available_targets(config: &Config, current_agent: &str) -> Vec<SubagentTarget> {
    let mut targets: Vec<SubagentTarget> = config
        .agents
        .iter()
        .filter(|(name, _)| name.as_str() != current_agent)
        .map(|(name, delegate)| SubagentTarget {
            name: name.clone(),
            description: delegate
                .description
                .clone()
                .unwrap_or_else(|| format!("已配置的子智能体 {name}")),
        })
        .collect();
    targets.sort_by(|a, b| a.name.cmp(&b.name));
    targets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_explore_is_read_only_and_cannot_write() {
        let explore = builtin_definitions()
            .into_iter()
            .find(|definition| definition.name == "explore")
            .expect("explore builtin");

        let tools = explore.resolve_allowed_tools();
        assert!(tools.contains(&"file_read".to_string()), "tools={tools:?}");
        assert!(tools.contains(&"content_search".to_string()), "tools={tools:?}");
        for forbidden in ["file_write", "shell", "file_patch", "git_operations"] {
            assert!(
                !tools.contains(&forbidden.to_string()),
                "explore must not get {forbidden}"
            );
        }
    }

    #[test]
    fn builtin_general_purpose_has_no_restriction() {
        let general = builtin_definitions()
            .into_iter()
            .find(|definition| definition.name == "general-purpose")
            .expect("general-purpose builtin");
        assert!(general.resolve_allowed_tools().is_empty());
    }

    #[test]
    fn parses_frontmatter_and_body() {
        let raw = "---\nname: Reviewer\ndescription: 审查代码改动\ntools: file_read, content_search\nmodel: gpt-x\n---\n\n你是评审员。\n";
        let definition = parse_definition(Path::new("/tmp/reviewer.md"), raw).unwrap();

        assert_eq!(definition.name, "reviewer");
        assert_eq!(definition.description, "审查代码改动");
        assert_eq!(
            definition.tools,
            Some(vec!["file_read".to_string(), "content_search".to_string()])
        );
        assert_eq!(definition.model.as_deref(), Some("gpt-x"));
        assert_eq!(definition.system_prompt, "你是评审员。");
    }

    #[test]
    fn parses_tools_as_a_yaml_list() {
        let raw = "---\nname: lister\ndescription: d\ntools:\n  - file_read\n  - shell\n---\nbody\n";
        let definition = parse_definition(Path::new("/tmp/lister.md"), raw).unwrap();
        assert_eq!(
            definition.tools,
            Some(vec!["file_read".to_string(), "shell".to_string()])
        );
    }

    #[test]
    fn read_only_flag_derives_the_toolset() {
        let raw = "---\nname: scout\ndescription: d\nread_only: true\n---\nbody\n";
        let definition = parse_definition(Path::new("/tmp/scout.md"), raw).unwrap();
        let tools = definition.resolve_allowed_tools();
        assert!(tools.contains(&"file_read".to_string()));
        assert!(!tools.contains(&"file_write".to_string()));
    }

    #[test]
    fn file_without_frontmatter_falls_back_to_the_filename() {
        let definition = parse_definition(Path::new("/tmp/notes.md"), "just a prompt").unwrap();
        assert_eq!(definition.name, "notes");
        assert_eq!(definition.system_prompt, "just a prompt");
    }

    #[test]
    fn names_are_normalized() {
        let raw = "---\nname: \"Code Reviewer!\"\ndescription: d\n---\nbody";
        let definition = parse_definition(Path::new("/tmp/x.md"), raw).unwrap();
        assert_eq!(definition.name, "code-reviewer");
    }

    #[test]
    fn toml_entries_win_over_builtins() {
        let mut config = Config::default();
        config.agents.insert(
            "explore".to_string(),
            DelegateAgentConfig {
                model: Some("my-model".into()),
                ..DelegateAgentConfig::default()
            },
        );

        let added = merge_into_config(&mut config);

        assert!(!added.contains(&"explore".to_string()));
        assert_eq!(
            config.agents["explore"].model.as_deref(),
            Some("my-model"),
            "an explicit TOML entry must not be overwritten"
        );
        assert!(config.agents.contains_key("general-purpose"));
    }

    #[test]
    fn merge_is_idempotent() {
        let mut config = Config::default();
        let first = merge_into_config(&mut config);
        let second = merge_into_config(&mut config);
        assert!(!first.is_empty());
        assert!(second.is_empty(), "second merge should add nothing");
    }

    #[test]
    fn targets_exclude_the_calling_agent_and_carry_descriptions() {
        let mut config = Config::default();
        merge_into_config(&mut config);

        let targets = available_targets(&config, "explore");

        assert!(!targets.iter().any(|target| target.name == "explore"));
        let general = targets
            .iter()
            .find(|target| target.name == "general-purpose")
            .expect("general-purpose is a target");
        assert!(!general.description.is_empty());
    }

    #[test]
    fn markdown_definition_is_merged_from_workspace() {
        let dir = std::env::temp_dir().join(format!(
            "omninova-subagent-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("reviewer.md"),
            "---\nname: reviewer\ndescription: 审查本次改动\ntools: file_read, content_search\n---\n你是评审员。\n",
        )
        .unwrap();

        let mut config = Config::default();
        config.workspace_dir = dir.parent().unwrap().to_path_buf();
        config.agent_defaults_extended.subagents =
            Some(crate::config::schema::SubagentsConfig {
                definitions_dir: Some(dir.to_string_lossy().into_owned()),
                ..crate::config::schema::SubagentsConfig::default()
            });

        let added = merge_into_config(&mut config);
        assert!(added.contains(&"reviewer".to_string()), "added={added:?}");
        assert_eq!(
            config.agents["reviewer"].description.as_deref(),
            Some("审查本次改动")
        );
        assert_eq!(
            config.agents["reviewer"].allowed_tools,
            vec!["file_read".to_string(), "content_search".to_string()]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
