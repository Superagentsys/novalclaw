pub mod browser;
pub mod browser_agent_backend;
pub mod browser_backend;
pub mod browser_bin;
pub mod browser_executable;
pub mod browser_installed_profile;
pub mod browser_lifecycle;
pub mod browser_output;
pub mod browser_profile;
pub mod browser_profile_manager;
pub mod browser_runtime;
pub mod browser_types;
pub mod computer_use;
pub mod content_search;
pub mod delegate;
pub mod file_edit;
pub mod file_list;
pub mod file_patch;
pub mod file_read;
pub mod file_write;
pub mod git_operations;
pub mod glob_search;
pub mod http_request;
pub mod knowledge_search;
pub mod memory_recall;
pub mod memory_store;
pub mod office_create;
pub mod page_extract;
pub mod pdf_read;
pub mod registry;
pub mod shell;
pub mod task_checkpoint;
pub mod text_bound;
pub mod todo_write;
pub mod traits;
pub mod use_skill;
pub mod web_client;
pub mod web_fetch;
pub mod web_search;
mod workspace_walk;

pub use browser::BrowserTool;
pub use browser_bin::{
    agent_browser_runtime_available, bundled_agent_browser_relative_path,
    effective_browser_capability, resolve_agent_browser_binary, set_agent_browser_search_roots,
    sync_browser_enabled_with_runtime, AgentBrowserBinaryMissing, AgentBrowserBinaryResolved,
    AgentBrowserBinarySource, AgentBrowserResolveError, BrowserBinarySearch, AGENT_BROWSER_BIN_ENV,
};
pub use browser_lifecycle::{
    cleanup_owned_browser_sessions, forget_owned_browser_session, remember_owned_browser_session,
    AGENT_BROWSER_NAMESPACE,
};
pub use computer_use::ComputerUseTool;
pub use content_search::ContentSearchTool;
pub use delegate::{AgentInvoker, DelegateRequest, DelegateTool};
pub use file_edit::FileEditTool;
pub use file_list::FileListTool;
pub use file_patch::FilePatchTool;
pub use file_read::FileReadTool;
pub use file_write::FileWriteTool;
pub use git_operations::GitOperationsTool;
pub use glob_search::GlobSearchTool;
pub use http_request::HttpRequestTool;
pub use knowledge_search::KnowledgeSearchTool;
pub use memory_recall::MemoryRecallTool;
pub use memory_store::MemoryStoreTool;
pub use office_create::OfficeCreateTool;
pub use pdf_read::PdfReadTool;
pub use registry::{
    build_tools, capabilities_for, capabilities_or_unknown, read_only_tool_names, ToolBuildContext,
    ToolCapabilities, WriteScope,
};
pub use shell::ShellTool;
pub use task_checkpoint::TaskCheckpointTool;
pub use todo_write::TodoWriteTool;
pub use traits::{Tool, ToolResult, ToolSpec};
pub use use_skill::{SkillActivationGate, UseSkillTool};
pub use web_fetch::WebFetchTool;
pub use web_search::WebSearchTool;

/// Prevent background tool processes from flashing a console window on Windows.
/// Keep this in one place so every Tokio child-process tool applies the same flag.
pub(crate) fn configure_background_command(command: &mut tokio::process::Command) {
    #[cfg(target_os = "windows")]
    command.creation_flags(0x0800_0000);

    #[cfg(not(target_os = "windows"))]
    let _ = command;
}
