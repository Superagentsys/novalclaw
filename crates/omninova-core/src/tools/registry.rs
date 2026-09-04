//! Declarative registry of every built-in tool.
//!
//! One table says how to construct a tool, when it is enabled, and what it can
//! do to the world. Agent assembly reads the table instead of hand-writing
//! `vec![Box::new(..)]`, and the security policy reads the declared
//! capabilities instead of matching on hardcoded tool-name lists. Adding a tool
//! means adding one row here.

use crate::config::Config;
use crate::memory::Memory;
use crate::security::{is_tool_globally_allowed, resolve_shell_allowlist};
use crate::tools::web_client::{
    WebToolSettings, DEFAULT_WEB_MAX_RESPONSE_BYTES, DEFAULT_WEB_REQUEST_TIMEOUT_SECS,
};
use crate::tools::{
    BrowserTool, ContentSearchTool, FileEditTool, FileListTool, FilePatchTool, FileReadTool,
    FileWriteTool, GitOperationsTool, GlobSearchTool, HttpRequestTool, KnowledgeSearchTool,
    MemoryRecallTool, MemoryStoreTool, OfficeCreateTool, PdfReadTool, ShellTool,
    TaskCheckpointTool, TodoWriteTool, Tool, WebFetchTool, WebSearchTool,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

/// What kind of state a tool can write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteScope {
    /// Writes nothing.
    None,
    /// Writes only the agent's own bookkeeping: task state, todo list, memory.
    /// Gating these behind approval would stall long-horizon runs without
    /// protecting anything the user cares about.
    AgentState,
    /// Writes user-visible workspace content.
    Workspace,
}

/// A tool's declared reach. The security policy derives risk from this instead
/// of from a name list that has to be kept in sync by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCapabilities {
    /// Observes state without changing it.
    pub read_only: bool,
    pub writes: WriteScope,
    /// Talks to an endpoint outside this machine.
    pub network: bool,
    /// Spawns an OS process, so its blast radius is not bounded by the tool's
    /// own code.
    pub spawns_process: bool,
}

impl ToolCapabilities {
    /// Reads workspace or agent state and changes nothing.
    pub const READ_ONLY: Self = Self {
        read_only: true,
        writes: WriteScope::None,
        network: false,
        spawns_process: false,
    };

    /// Changes user content in the workspace.
    pub const WORKSPACE_WRITE: Self = Self {
        read_only: false,
        writes: WriteScope::Workspace,
        network: false,
        spawns_process: false,
    };

    /// Changes only the agent's own bookkeeping.
    pub const AGENT_STATE_WRITE: Self = Self {
        read_only: false,
        writes: WriteScope::AgentState,
        network: false,
        spawns_process: false,
    };

    /// Reads from a remote endpoint.
    pub const NETWORK_READ: Self = Self {
        read_only: true,
        writes: WriteScope::None,
        network: true,
        spawns_process: false,
    };

    /// Sends arbitrary requests to a remote endpoint.
    pub const NETWORK_WRITE: Self = Self {
        read_only: false,
        writes: WriteScope::None,
        network: true,
        spawns_process: false,
    };

    /// Runs a child process that can read and write the workspace and reach the
    /// network. Used by `shell` and `git_operations`.
    pub const PROCESS: Self = Self {
        read_only: false,
        writes: WriteScope::Workspace,
        network: true,
        spawns_process: true,
    };

    /// Nothing is known about this tool, so assume the worst. Reached when a
    /// model invents a tool name.
    pub const UNKNOWN: Self = Self {
        read_only: false,
        writes: WriteScope::Workspace,
        network: true,
        spawns_process: true,
    };

    pub fn is_high_risk(&self) -> bool {
        matches!(self.writes, WriteScope::Workspace) || self.network || self.spawns_process
    }

    /// Safe to auto-approve alongside `file_read`: observes local state only.
    pub fn is_read_only_workspace(&self) -> bool {
        self.read_only
            && matches!(self.writes, WriteScope::None)
            && !self.network
            && !self.spawns_process
    }

    /// Whether workspace path and pattern arguments should be checked against
    /// the forbidden-path rules before the call runs.
    pub fn touches_workspace_paths(&self) -> bool {
        self.is_read_only_workspace() || matches!(self.writes, WriteScope::Workspace)
    }
}

/// Inputs a tool needs at construction time.
pub struct ToolBuildContext<'a> {
    pub config: &'a Config,
    pub workspace: &'a Path,
    /// Absent for callers that only want to enumerate workspace tools, such as
    /// the `/api/tools` endpoint. Memory-backed tools are skipped then.
    pub memory: Option<&'a Arc<dyn Memory>>,
    pub session_id: Option<&'a str>,
}

/// One row of the registry.
pub struct ToolDef {
    pub name: &'static str,
    pub capabilities: ToolCapabilities,
    /// Alternate names a model might call this tool by. Policy lookups resolve
    /// through them so a hallucinated `read_file` is still judged read-only.
    pub aliases: &'static [&'static str],
    /// Whether the feature is switched on in config, before allow/deny lists.
    pub enabled: fn(&Config) -> bool,
    /// `None` when a required input is missing, e.g. no API key.
    pub build: fn(&ToolBuildContext<'_>) -> Option<Box<dyn Tool>>,
}

fn always(_config: &Config) -> bool {
    true
}

pub static TOOL_REGISTRY: &[ToolDef] = &[
    ToolDef {
        name: "file_read",
        capabilities: ToolCapabilities::READ_ONLY,
        aliases: &["read_file"],
        enabled: always,
        build: |ctx| Some(Box::new(FileReadTool::new(ctx.workspace.to_path_buf()))),
    },
    ToolDef {
        name: "file_write",
        capabilities: ToolCapabilities::WORKSPACE_WRITE,
        aliases: &[],
        enabled: always,
        build: |ctx| Some(Box::new(FileWriteTool::new(ctx.workspace.to_path_buf()))),
    },
    ToolDef {
        name: "office_create",
        capabilities: ToolCapabilities::WORKSPACE_WRITE,
        aliases: &["create_pptx", "create_docx", "create_xlsx"],
        enabled: always,
        build: |ctx| Some(Box::new(OfficeCreateTool::new(ctx.workspace.to_path_buf()))),
    },
    ToolDef {
        name: "file_edit",
        capabilities: ToolCapabilities::WORKSPACE_WRITE,
        aliases: &[],
        enabled: always,
        build: |ctx| Some(Box::new(FileEditTool::new(ctx.workspace.to_path_buf()))),
    },
    ToolDef {
        name: "file_patch",
        capabilities: ToolCapabilities::WORKSPACE_WRITE,
        aliases: &["apply_patch"],
        enabled: always,
        build: |ctx| Some(Box::new(FilePatchTool::new(ctx.workspace.to_path_buf()))),
    },
    ToolDef {
        name: "file_list",
        capabilities: ToolCapabilities::READ_ONLY,
        aliases: &["list_directory"],
        enabled: always,
        build: |ctx| Some(Box::new(FileListTool::new(ctx.workspace.to_path_buf()))),
    },
    ToolDef {
        name: "glob_search",
        capabilities: ToolCapabilities::READ_ONLY,
        aliases: &["glob", "file_search"],
        enabled: always,
        build: |ctx| Some(Box::new(GlobSearchTool::new(ctx.workspace.to_path_buf()))),
    },
    ToolDef {
        name: "content_search",
        capabilities: ToolCapabilities::READ_ONLY,
        aliases: &["grep_search", "grep", "search"],
        enabled: always,
        build: |ctx| {
            Some(Box::new(ContentSearchTool::new(
                ctx.workspace.to_path_buf(),
            )))
        },
    },
    ToolDef {
        name: "git_operations",
        capabilities: ToolCapabilities::PROCESS,
        aliases: &[],
        enabled: always,
        build: |ctx| {
            Some(Box::new(GitOperationsTool::new(
                ctx.workspace.to_path_buf(),
            )))
        },
    },
    ToolDef {
        name: "shell",
        capabilities: ToolCapabilities::PROCESS,
        aliases: &[],
        enabled: always,
        build: |ctx| {
            Some(Box::new(ShellTool::new(
                ctx.workspace.to_path_buf(),
                resolve_shell_allowlist(ctx.config),
                Some(30),
                ctx.config.clone(),
            )))
        },
    },
    ToolDef {
        name: "pdf_read",
        capabilities: ToolCapabilities::READ_ONLY,
        aliases: &[],
        enabled: always,
        build: |ctx| Some(Box::new(PdfReadTool::new(ctx.workspace.to_path_buf()))),
    },
    ToolDef {
        name: "knowledge_search",
        capabilities: ToolCapabilities::READ_ONLY,
        aliases: &[],
        enabled: always,
        build: |ctx| {
            Some(Box::new(KnowledgeSearchTool::new(
                ctx.workspace.to_path_buf(),
            )))
        },
    },
    ToolDef {
        name: "memory_store",
        capabilities: ToolCapabilities::AGENT_STATE_WRITE,
        aliases: &[],
        enabled: always,
        build: |ctx| {
            ctx.memory
                .map(|memory| Box::new(MemoryStoreTool::new(memory.clone())) as Box<dyn Tool>)
        },
    },
    ToolDef {
        name: "memory_recall",
        capabilities: ToolCapabilities::READ_ONLY,
        aliases: &[],
        enabled: always,
        build: |ctx| {
            ctx.memory
                .map(|memory| Box::new(MemoryRecallTool::new(memory.clone())) as Box<dyn Tool>)
        },
    },
    ToolDef {
        name: "task_checkpoint",
        capabilities: ToolCapabilities::AGENT_STATE_WRITE,
        aliases: &[],
        enabled: always,
        build: |ctx| {
            Some(Box::new(TaskCheckpointTool::new(
                ctx.workspace.to_path_buf(),
                ctx.session_id.map(str::to_string),
            )))
        },
    },
    ToolDef {
        name: "todo_write",
        capabilities: ToolCapabilities::AGENT_STATE_WRITE,
        aliases: &[],
        enabled: always,
        build: |ctx| {
            Some(Box::new(TodoWriteTool::new(
                ctx.workspace.to_path_buf(),
                ctx.session_id.map(str::to_string),
            )))
        },
    },
    ToolDef {
        name: "http_request",
        capabilities: ToolCapabilities::NETWORK_WRITE,
        aliases: &[],
        enabled: |config| config.http_request.enabled,
        build: |ctx| {
            Some(Box::new(HttpRequestTool::new(
                ctx.config.http_request.allowed_domains.clone(),
                WebToolSettings::from_config(
                    &ctx.config.proxy,
                    ctx.config.http_request.timeout_secs,
                    ctx.config.http_request.max_response_size,
                ),
            )))
        },
    },
    ToolDef {
        name: "web_fetch",
        capabilities: ToolCapabilities::NETWORK_READ,
        aliases: &[],
        enabled: |config| config.web_fetch.enabled,
        build: |ctx| {
            Some(Box::new(WebFetchTool::new(
                ctx.config.web_fetch.allowed_domains.clone(),
                WebToolSettings::from_config(
                    &ctx.config.proxy,
                    ctx.config.web_fetch.timeout_secs,
                    ctx.config.web_fetch.max_response_size,
                ),
            )))
        },
    },
    ToolDef {
        name: "web_search",
        capabilities: ToolCapabilities::NETWORK_READ,
        aliases: &[],
        enabled: |config| config.web_search.enabled,
        build: |ctx| {
            let timeout_secs = ctx
                .config
                .web_search
                .timeout_secs
                .unwrap_or(DEFAULT_WEB_REQUEST_TIMEOUT_SECS);
            ctx.config.web_search.brave_api_key.as_ref().map(|key| {
                Box::new(WebSearchTool::new(
                    key.clone(),
                    WebToolSettings::from_config(
                        &ctx.config.proxy,
                        timeout_secs,
                        DEFAULT_WEB_MAX_RESPONSE_BYTES,
                    ),
                )) as Box<dyn Tool>
            })
        },
    },
    ToolDef {
        name: "browser",
        capabilities: ToolCapabilities {
            read_only: false,
            writes: WriteScope::None,
            network: true,
            spawns_process: true,
        },
        aliases: &[],
        enabled: |config| {
            crate::tools::browser_agent_backend::browser_backend_enabled(
                config.browser.enabled,
                &config.browser.backend,
            )
        },
        build: |ctx| {
            let session_opts = crate::tools::browser_types::BrowserSessionOptions {
                headless: ctx.config.browser.native_headless,
                attach_only: ctx.config.browser.attach_only,
                cdp_url: ctx.config.browser.cdp_url.clone(),
                profile: None,
            };
            let backend = match crate::tools::browser_agent_backend::backend_from_config(
                &ctx.config.browser.backend,
                None,
                session_opts.clone(),
            ) {
                Ok(backend) => backend,
                Err(err) => {
                    tracing::warn!(
                        target: "browser",
                        backend = %ctx.config.browser.backend,
                        detail = %err.detail,
                        "browser backend unavailable; tool not registered"
                    );
                    return None;
                }
            };
            let policy = crate::tools::browser_runtime::BrowserRuntimePolicy {
                allowed_domains: ctx.config.browser.allowed_domains.clone(),
                ..crate::tools::browser_runtime::BrowserRuntimePolicy::default()
            };
            let runtime = crate::tools::browser_runtime::BrowserRuntime::new(backend, policy);
            let session_key = ctx
                .session_id
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .and_then(|s| crate::tools::browser_types::BrowserSessionKey::new(s).ok());
            Some(Box::new(BrowserTool::from_runtime(
                runtime,
                session_opts,
                session_key,
            )))
        },
    },
];

/// Tools attached outside the registry because they need a runtime handle the
/// build context does not carry. Their capabilities still have to be
/// declared somewhere the policy can find them.
static EXTERNAL_CAPABILITIES: &[(&str, ToolCapabilities)] = &[
    ("use_skill", ToolCapabilities::READ_ONLY),
    // Delegating is itself harmless: the child agent re-enters the inbound
    // pipeline and every tool it calls is gated by the child's own security
    // context.
    (
        "delegate",
        ToolCapabilities {
            read_only: false,
            writes: WriteScope::None,
            network: false,
            spawns_process: false,
        },
    ),
];

/// Build every tool that is enabled in config and not globally denied.
///
/// The global allow/deny filter only applies when the tool policy is on,
/// matching what `evaluate_tool_call` enforces at call time. Filtering here as
/// well keeps tools the policy would reject out of the prompt.
pub fn build_tools(ctx: &ToolBuildContext<'_>) -> Vec<Box<dyn Tool>> {
    let policy_enabled = ctx.config.security.tool_policy.enabled;
    TOOL_REGISTRY
        .iter()
        .filter(|def| (def.enabled)(ctx.config))
        .filter(|def| !policy_enabled || is_tool_globally_allowed(ctx.config, def.name))
        .filter_map(|def| (def.build)(ctx))
        .collect()
}

fn find(name: &str) -> Option<&'static ToolDef> {
    TOOL_REGISTRY.iter().find(|def| {
        def.name.eq_ignore_ascii_case(name)
            || def
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(name))
    })
}

pub fn capabilities_for(name: &str) -> Option<ToolCapabilities> {
    if let Some(def) = find(name) {
        return Some(def.capabilities);
    }
    EXTERNAL_CAPABILITIES
        .iter()
        .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
        .map(|(_, capabilities)| *capabilities)
}

/// Capabilities for a tool name, assuming the worst for names we do not know.
pub fn capabilities_or_unknown(name: &str) -> ToolCapabilities {
    capabilities_for(name).unwrap_or(ToolCapabilities::UNKNOWN)
}

/// Every built-in tool that only observes local state. Used to assemble the
/// toolset for read-only subagents without listing names in two places.
pub fn read_only_tool_names() -> Vec<&'static str> {
    TOOL_REGISTRY
        .iter()
        .filter(|def| def.capabilities.is_read_only_workspace())
        .map(|def| def.name)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Names of everything the registry builds for this config. Owned so the
    /// caller is not borrowing from a temporary tool list.
    fn built_names(config: &Config, memory: Option<&Arc<dyn Memory>>) -> Vec<String> {
        build_tools(&ToolBuildContext {
            config,
            workspace: Path::new("/fake/workspace"),
            memory,
            session_id: None,
        })
        .iter()
        .map(|tool| tool.name().to_string())
        .collect()
    }

    fn contains(names: &[String], needle: &str) -> bool {
        names.iter().any(|name| name == needle)
    }

    #[test]
    fn registry_names_are_unique_across_names_and_aliases() {
        let mut seen: Vec<&str> = Vec::new();
        for def in TOOL_REGISTRY {
            seen.push(def.name);
            seen.extend(def.aliases.iter().copied());
        }
        for (name, _) in EXTERNAL_CAPABILITIES {
            seen.push(name);
        }
        let mut sorted = seen.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), seen.len(), "duplicate tool name or alias");
    }

    #[test]
    fn network_tools_are_built_when_enabled() {
        let mut config = Config::default();
        config.web_fetch.enabled = true;
        config.web_fetch.allowed_domains = vec!["example.com".into()];
        config.http_request.enabled = true;
        config.web_search.enabled = true;
        config.web_search.brave_api_key = Some("key".into());

        let names = built_names(&config, None);

        assert!(contains(&names, "web_fetch"), "names={names:?}");
        assert!(contains(&names, "http_request"), "names={names:?}");
        assert!(contains(&names, "web_search"), "names={names:?}");
    }

    #[test]
    fn web_search_is_skipped_without_an_api_key() {
        let mut config = Config::default();
        config.web_search.enabled = true;
        config.web_search.brave_api_key = None;

        let names = built_names(&config, None);

        assert!(!contains(&names, "web_search"), "names={names:?}");
    }

    #[test]
    fn browser_is_registered_only_when_configured_and_runtime_available() {
        let mut config = Config::default();
        config.browser.enabled = true;

        let names = built_names(&config, None);
        let present = contains(&names, "browser");
        assert_eq!(
            present,
            crate::tools::browser_bin::agent_browser_runtime_available(),
            "browser tool presence must follow runtime availability: names={names:?}"
        );
    }

    #[test]
    fn browser_tool_gets_stable_session_mapping_from_build_context() {
        let mut config = Config::default();
        config.browser.enabled = true;
        config.browser.allowed_domains = Vec::new();
        let memory: Arc<dyn Memory> = Arc::new(crate::InMemoryMemory::new());

        let build = |session: Option<&str>| -> String {
            let mut tools = build_tools(&ToolBuildContext {
                config: &config,
                workspace: Path::new("/fake/workspace"),
                memory: Some(&memory),
                session_id: session,
            });
            let browser = tools
                .iter_mut()
                .find(|tool| tool.name() == "browser")
                .expect("browser should be built when runtime available");
            let browser = browser
                .as_any()
                .and_then(|any| any.downcast_ref::<crate::tools::BrowserTool>())
                .expect("browser tool should be a BrowserTool");
            browser.session_id().unwrap_or("").to_string()
        };

        // If no runtime is available the tool is intentionally not built.
        if !crate::tools::browser_bin::agent_browser_runtime_available() {
            eprintln!("skip runtime-dependent test: agent-browser unavailable");
            return;
        }

        let session_a = "chat/房间 1".to_string();
        let session_b = "chat/房间 2".to_string();
        let logical_a = build(Some(&session_a));
        let logical_b = build(Some(&session_b));
        assert_ne!(logical_a, logical_b);
        assert_eq!(logical_a, session_a);
        assert_eq!(logical_b, session_b);
        let mapped_a = crate::tools::browser::browser_session_id(Some(&logical_a)).unwrap();
        let mapped_b = crate::tools::browser::browser_session_id(Some(&logical_b)).unwrap();
        assert_ne!(mapped_a, mapped_b);
        assert!(mapped_a.starts_with("omninova-"));
        assert!(mapped_b.starts_with("omninova-"));
        assert_eq!(
            logical_a,
            build(Some(&session_a)),
            "same session must map stable"
        );
        assert_eq!(
            mapped_a,
            crate::tools::browser::browser_session_id(Some(&session_a)).unwrap()
        );

        let mapped_none = build(None);
        assert!(
            mapped_none.is_empty(),
            "missing session must not bind a shared browser session: {mapped_none}"
        );
        assert_ne!(mapped_none, "default");
        assert!(!mapped_none.contains("anonymous"));

        for blank in ["", "   "] {
            let mapped_blank = build(Some(blank));
            assert!(
                mapped_blank.is_empty(),
                "blank session {blank:?} must not bind a shared browser session: {mapped_blank}"
            );
        }

        let long = format!("x{}y", "a".repeat(500));
        let logical_long = build(Some(&long));
        assert_eq!(logical_long, long);
        let mapped_long = crate::tools::browser::browser_session_id(Some(&logical_long)).unwrap();
        assert!(mapped_long.starts_with("omninova-"));
        assert!(mapped_long.len() < 64);

        let unicode = "会话-😀-中文-空格 end";
        let logical_unicode = build(Some(unicode));
        assert_eq!(logical_unicode, unicode);
        let mapped_unicode = crate::tools::browser::browser_session_id(Some(unicode)).unwrap();
        assert!(mapped_unicode
            .chars()
            .all(|c| c.is_ascii_lowercase() || c == '-' || c.is_ascii_digit()));
        assert_eq!(
            mapped_unicode,
            crate::tools::browser::browser_session_id(Some(&logical_unicode)).unwrap()
        );
    }

    #[test]
    fn browser_tool_is_not_registered_for_unsupported_backends() {
        let mut config = Config::default();
        config.browser.enabled = true;
        config.browser.backend = "personal-chrome".into();
        assert!(
            !contains(&built_names(&config, None), "browser"),
            "personal-chrome must not silently fall back"
        );
        config.browser.backend = "unknown-backend".into();
        assert!(
            !contains(&built_names(&config, None), "browser"),
            "unknown backend must not silently fall back"
        );
        config.browser.backend = "playwright".into();
        assert_eq!(
            contains(&built_names(&config, None), "browser"),
            crate::tools::browser_bin::agent_browser_runtime_available()
        );
    }

    #[test]
    fn disabled_features_are_not_built() {
        let mut config = Config::default();
        config.web_fetch.enabled = false;
        config.http_request.enabled = false;
        config.browser.enabled = false;

        let names = built_names(&config, None);

        assert!(!contains(&names, "web_fetch"), "names={names:?}");
        assert!(!contains(&names, "http_request"), "names={names:?}");
        assert!(!contains(&names, "browser"), "names={names:?}");
    }

    #[test]
    fn denied_tools_are_not_built_when_policy_is_on() {
        let mut config = Config::default();
        config.security.tool_policy.enabled = true;
        config.security.tool_policy.denied_tools = vec!["shell".into()];

        let names = built_names(&config, None);

        assert!(!contains(&names, "shell"), "names={names:?}");
        assert!(contains(&names, "file_read"), "names={names:?}");
    }

    #[test]
    fn memory_tools_need_a_backend() {
        let config = Config::default();

        let without = built_names(&config, None);
        assert!(!contains(&without, "memory_store"), "names={without:?}");

        let memory: Arc<dyn Memory> = Arc::new(crate::InMemoryMemory::new());
        let with = built_names(&config, Some(&memory));
        assert!(contains(&with, "memory_store"), "names={with:?}");
    }

    #[test]
    fn aliases_resolve_to_the_canonical_capabilities() {
        assert_eq!(
            capabilities_for("read_file"),
            Some(ToolCapabilities::READ_ONLY)
        );
        assert_eq!(
            capabilities_for("apply_patch"),
            Some(ToolCapabilities::WORKSPACE_WRITE)
        );
        assert!(capabilities_for("grep").unwrap().is_read_only_workspace());
    }

    #[test]
    fn unknown_tools_are_treated_as_high_risk() {
        let capabilities = capabilities_or_unknown("totally_made_up");
        assert!(capabilities.is_high_risk());
        assert!(!capabilities.is_read_only_workspace());
    }

    #[test]
    fn agent_bookkeeping_writes_are_not_high_risk() {
        for name in ["task_checkpoint", "todo_write", "memory_store"] {
            let capabilities = capabilities_or_unknown(name);
            assert!(
                !capabilities.is_high_risk(),
                "{name} should not need approval to record its own progress"
            );
        }
    }

    #[test]
    fn workspace_writers_and_process_spawners_are_high_risk() {
        for name in [
            "shell",
            "file_write",
            "office_create",
            "file_edit",
            "file_patch",
            "git_operations",
            "browser",
            "http_request",
        ] {
            assert!(
                capabilities_or_unknown(name).is_high_risk(),
                "{name} should be high risk"
            );
        }
    }

    #[test]
    fn read_only_tool_names_exclude_writers_and_network() {
        let names = read_only_tool_names();
        assert!(names.contains(&"file_read"), "names={names:?}");
        assert!(names.contains(&"content_search"), "names={names:?}");
        for forbidden in [
            "file_write",
            "office_create",
            "file_edit",
            "file_patch",
            "shell",
            "git_operations",
            "http_request",
            "web_search",
            "browser",
            "todo_write",
            "task_checkpoint",
        ] {
            assert!(
                !names.contains(&forbidden),
                "{forbidden} must not be in the read-only set: {names:?}"
            );
        }
    }
}
