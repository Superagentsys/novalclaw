//! 聊天输入框附件：按本地路径读取内容（Tauri 拖放 / 系统文件对话框）。

use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::path::{Path, PathBuf};

const MAX_FILES: usize = 16;
const MAX_TEXT_BYTES: u64 = 512 * 1024;
const MAX_IMAGE_BYTES: u64 = 256 * 1024;

fn extension_lower(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
}

fn is_text_extension(ext: &str) -> bool {
    matches!(
        ext,
        "txt" | "md" | "markdown" | "mdx" | "json" | "jsonl" | "jsonc" | "csv" | "tsv" | "log"
            | "yaml" | "yml" | "xml" | "html" | "htm" | "swift" | "rs" | "py" | "rb" | "go"
            | "java" | "kt" | "kts" | "c" | "cc" | "cpp" | "h" | "hpp" | "cs" | "php" | "vue"
            | "svelte" | "js" | "mjs" | "cjs" | "ts" | "tsx" | "jsx" | "css" | "scss" | "less"
            | "sass" | "sh" | "bash" | "zsh" | "fish" | "sql" | "toml" | "ini" | "cfg" | "conf"
            | "gradle" | "plist" | "rst" | "tex" | "bib"
    )
}

fn is_image_extension(ext: &str) -> bool {
    matches!(
        ext,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" | "ico" | "heic" | "heif"
    )
}

fn image_mime(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "heic" => "image/heic",
        "heif" => "image/heif",
        _ => "application/octet-stream",
    }
}

fn escape_markdown_alt(text: &str) -> String {
    text.replace(['[', ']'], "")
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("unnamed")
        .to_string()
}

fn decode_text_bytes(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => String::from_utf8_lossy(bytes).into_owned(),
    }
}

async fn format_path_attachment(path: PathBuf) -> String {
    let name = display_name(&path);
    let ext = extension_lower(&path).unwrap_or_default();

    let meta = match tokio::fs::metadata(&path).await {
        Ok(m) => m,
        Err(e) => {
            return format!("\n\n[无法访问: {name} — {e}]");
        }
    };

    if meta.is_dir() {
        return format!("\n\n[跳过目录: {name}]");
    }

    let size = meta.len();
    if size == 0 {
        return format!("\n\n[空文件: {name}]");
    }

    if is_image_extension(&ext) {
        if size > MAX_IMAGE_BYTES {
            return format!(
                "\n\n[图片: {name} · {} KB — 超过 {} KB 上限未嵌入；请缩小后再添加。]",
                size / 1024,
                MAX_IMAGE_BYTES / 1024
            );
        }
        match tokio::fs::read(&path).await {
            Ok(bytes) => {
                let mime = image_mime(&ext);
                let data_url = format!("data:{mime};base64,{}", STANDARD.encode(bytes));
                format!(
                    "\n\n![{}]({data_url})",
                    escape_markdown_alt(&name)
                )
            }
            Err(e) => format!("\n\n[图片读取失败: {name} — {e}]"),
        }
    } else if is_text_extension(&ext) {
        if size > MAX_TEXT_BYTES {
            return format!(
                "\n\n[文本附件 {name}: 过大 ({} KB)，上限 {} KB — 请拆分或使用更小文件。]",
                size / 1024,
                MAX_TEXT_BYTES / 1024
            );
        }
        match tokio::fs::read(&path).await {
            Ok(bytes) => {
                let text = decode_text_bytes(&bytes);
                format!("\n\n--- 附件: {name} ---\n{text}\n--- 附件结束 ---")
            }
            Err(e) => format!("\n\n[文本读取失败: {name} — {e}]"),
        }
    } else {
        format!(
            "\n\n[附件: {name} · {} · {} KB — 未能自动读取此类文件内容；可先导出为 .txt/.md 再添加，或让 Agent 用工作区工具读取。]",
            if ext.is_empty() { "未知类型" } else { &ext },
            size / 1024
        )
    }
}

/// 从绝对路径列表读取附件并格式化为可拼入消息的 Markdown 文本。
#[tauri::command]
pub async fn read_composer_attachments(paths: Vec<String>) -> Result<String, String> {
    if paths.is_empty() {
        return Ok(String::new());
    }

    let mut parts = Vec::new();
    for raw in paths.into_iter().take(MAX_FILES) {
        let path = PathBuf::from(&raw);
        if !path.is_absolute() {
            return Err(format!("路径必须为绝对路径: {raw}"));
        }
        parts.push(format_path_attachment(path).await);
    }

    Ok(parts.join(""))
}
