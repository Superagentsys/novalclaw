pub mod browser;
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
pub mod pdf_read;
pub mod shell;
pub mod traits;
pub mod web_fetch;
pub mod web_search;

pub use browser::BrowserTool;
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
pub use pdf_read::PdfReadTool;
pub use shell::ShellTool;
pub use traits::{Tool, ToolResult, ToolSpec};
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
