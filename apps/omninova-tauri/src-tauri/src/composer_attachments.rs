//! 聊天输入框附件：按本地路径读取内容（Tauri 拖放 / 系统文件对话框）。

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::Serialize;
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use zip::ZipArchive;

const MAX_FILES: usize = 16;
const MAX_TEXT_BYTES: u64 = 512 * 1024;
const MAX_IMAGE_BYTES: u64 = 256 * 1024;
const MAX_OFFICE_TEXT_BYTES: usize = 512 * 1024;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedComposerAttachment {
    name: String,
    original_path: String,
    workspace_relative_path: String,
    size: u64,
    kind: String,
    content: String,
    note: String,
}

fn extension_lower(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
}

fn is_text_extension(ext: &str) -> bool {
    matches!(
        ext,
        "txt"
            | "md"
            | "markdown"
            | "mdx"
            | "json"
            | "jsonl"
            | "jsonc"
            | "csv"
            | "tsv"
            | "log"
            | "yaml"
            | "yml"
            | "xml"
            | "html"
            | "htm"
            | "swift"
            | "rs"
            | "py"
            | "rb"
            | "go"
            | "java"
            | "kt"
            | "kts"
            | "c"
            | "cc"
            | "cpp"
            | "h"
            | "hpp"
            | "cs"
            | "php"
            | "vue"
            | "svelte"
            | "js"
            | "mjs"
            | "cjs"
            | "ts"
            | "tsx"
            | "jsx"
            | "css"
            | "scss"
            | "less"
            | "sass"
            | "sh"
            | "bash"
            | "zsh"
            | "fish"
            | "sql"
            | "toml"
            | "ini"
            | "cfg"
            | "conf"
            | "gradle"
            | "plist"
            | "rst"
            | "tex"
            | "bib"
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

fn safe_path_component(value: &str, fallback: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_control() || matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
            {
                '_'
            } else {
                ch
            }
        })
        .collect::<String>()
        .trim_matches([' ', '.'])
        .to_string();
    if sanitized.is_empty() {
        fallback.to_string()
    } else {
        sanitized
    }
}

fn workspace_relative_display(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn decode_xml_entities(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn xml_visible_text(xml: &str) -> String {
    let mut result = String::new();
    let mut inside_tag = false;
    let mut last_was_space = false;
    for ch in xml.chars() {
        match ch {
            '<' => inside_tag = true,
            '>' => {
                inside_tag = false;
                if !last_was_space && !result.is_empty() {
                    result.push(' ');
                    last_was_space = true;
                }
            }
            _ if inside_tag => {}
            _ if ch.is_whitespace() => {
                if !last_was_space && !result.is_empty() {
                    result.push(' ');
                    last_was_space = true;
                }
            }
            _ => {
                result.push(ch);
                last_was_space = false;
            }
        }
    }
    decode_xml_entities(result.trim())
}

fn extract_office_text(path: &Path, extension: &str) -> Result<String, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("Office 文档结构无效：{error}"))?;
    let mut selected = Vec::new();
    for index in 0..archive.len().min(10_000) {
        let name = archive
            .by_index(index)
            .map_err(|error| error.to_string())?
            .name()
            .replace('\\', "/");
        let include = match extension {
            "pptx" => name.starts_with("ppt/slides/slide") && name.ends_with(".xml"),
            "docx" => {
                name == "word/document.xml"
                    || name.starts_with("word/header")
                    || name.starts_with("word/footer")
            }
            "xlsx" => name == "xl/sharedStrings.xml" || name.starts_with("xl/worksheets/sheet"),
            _ => false,
        };
        if include {
            selected.push(name);
        }
    }
    selected.sort();

    let mut extracted = String::new();
    for name in selected {
        if extracted.len() >= MAX_OFFICE_TEXT_BYTES {
            break;
        }
        let mut entry = archive.by_name(&name).map_err(|error| error.to_string())?;
        let mut bytes = Vec::new();
        entry
            .by_ref()
            .take((MAX_OFFICE_TEXT_BYTES - extracted.len()) as u64)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        let visible = xml_visible_text(&String::from_utf8_lossy(&bytes));
        if !visible.is_empty() {
            if !extracted.is_empty() {
                extracted.push_str("\n\n");
            }
            extracted.push_str(&visible);
        }
    }
    if extracted.is_empty() {
        Err("文档中没有提取到可读文字".to_string())
    } else {
        Ok(extracted)
    }
}

async fn prepare_path_attachment(
    raw: String,
    workspace: PathBuf,
    session_id: &str,
    index: usize,
) -> Result<PreparedComposerAttachment, String> {
    let requested = PathBuf::from(&raw);
    if !requested.is_absolute() {
        return Err(format!("路径必须为绝对路径: {raw}"));
    }
    let source = tokio::fs::canonicalize(&requested)
        .await
        .map_err(|error| format!("无法访问附件 {raw}：{error}"))?;
    let metadata = tokio::fs::metadata(&source)
        .await
        .map_err(|error| format!("无法读取附件信息：{error}"))?;
    if !metadata.is_file() {
        return Err(format!("暂不支持拖入目录：{}", source.display()));
    }

    let name = display_name(&source);
    let ext = extension_lower(&source).unwrap_or_default();
    let relative = if source.starts_with(&workspace) {
        source
            .strip_prefix(&workspace)
            .map_err(|error| error.to_string())?
            .to_path_buf()
    } else {
        let session = safe_path_component(session_id, "session");
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let filename = safe_path_component(&name, "attachment");
        let relative = PathBuf::from(".omninova")
            .join("attachments")
            .join(session)
            .join(format!("{timestamp}-{index}-{filename}"));
        let destination = workspace.join(&relative);
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|error| format!("无法创建附件暂存目录：{error}"))?;
        }
        let temporary = destination.with_extension(format!("{}.part", ext));
        tokio::fs::copy(&source, &temporary)
            .await
            .map_err(|error| format!("无法把附件复制到 Workspace：{error}"))?;
        tokio::fs::rename(&temporary, &destination)
            .await
            .map_err(|error| format!("无法完成附件暂存：{error}"))?;
        relative
    };

    let mounted = workspace_relative_display(&relative);
    let size = metadata.len();
    let kind = if is_image_extension(&ext) {
        "image"
    } else if is_text_extension(&ext) {
        "text"
    } else if matches!(ext.as_str(), "pptx" | "docx" | "xlsx") {
        "office"
    } else {
        "file"
    };

    let mut content = format!(
        "--- 用户附件 ---\n文件名：{name}\nWorkspace 相对路径：{mounted}\n类型：{}\n大小：{} 字节\n读取要求：请始终使用以上 Workspace 相对路径读取本附件，不要按同名文件猜测路径。",
        if ext.is_empty() { "未知" } else { &ext },
        size
    );
    let note;
    if is_image_extension(&ext) && size <= MAX_IMAGE_BYTES {
        let bytes = tokio::fs::read(&source)
            .await
            .map_err(|error| error.to_string())?;
        content.push_str(&format!(
            "\n\n![{}](data:{};base64,{})",
            escape_markdown_alt(&name),
            image_mime(&ext),
            STANDARD.encode(bytes)
        ));
        note = format!("已挂载 · {} KB · 图片已嵌入", size.div_ceil(1024));
    } else if is_text_extension(&ext) && size <= MAX_TEXT_BYTES {
        let bytes = tokio::fs::read(&source)
            .await
            .map_err(|error| error.to_string())?;
        content.push_str(&format!("\n\n内容：\n{}", decode_text_bytes(&bytes)));
        note = format!("已挂载 · {} KB · 内容已读取", size.div_ceil(1024));
    } else if matches!(ext.as_str(), "pptx" | "docx" | "xlsx") {
        let source_for_extract = source.clone();
        let ext_for_extract = ext.clone();
        match tokio::task::spawn_blocking(move || {
            extract_office_text(&source_for_extract, &ext_for_extract)
        })
        .await
        {
            Ok(Ok(text)) => {
                content.push_str(&format!("\n\n文档提取文字：\n{text}"));
                note = format!("已挂载 · {} KB · 文字已提取", size.div_ceil(1024));
            }
            Ok(Err(reason)) => {
                content.push_str(&format!(
                    "\n\n文字提取提示：{reason}；仍可通过 Workspace 路径使用工具读取原文件。"
                ));
                note = format!("已挂载 · {} KB · 可由工具读取", size.div_ceil(1024));
            }
            Err(error) => return Err(format!("文档文字提取任务失败：{error}")),
        }
    } else {
        content.push_str("\n\n内容未内嵌，请使用工作区文件工具读取上面的已挂载路径。");
        note = format!("已挂载 · {} KB · 可由工具读取", size.div_ceil(1024));
    }
    content.push_str("\n--- 用户附件结束 ---");

    Ok(PreparedComposerAttachment {
        name,
        original_path: source.to_string_lossy().to_string(),
        workspace_relative_path: mounted,
        size,
        kind: kind.to_string(),
        content,
        note,
    })
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
                format!("\n\n![{}]({data_url})", escape_markdown_alt(&name))
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

/// 把桌面端绝对路径附件安全挂载到当前 Workspace，并返回可直接传给 Agent 的结构化上下文。
#[tauri::command]
pub async fn prepare_composer_attachments(
    paths: Vec<String>,
    workspace_path: String,
    session_id: String,
) -> Result<Vec<PreparedComposerAttachment>, String> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let workspace_raw = PathBuf::from(workspace_path.trim());
    if !workspace_raw.is_absolute() {
        return Err("Workspace 必须是绝对路径。".to_string());
    }
    tokio::fs::create_dir_all(&workspace_raw)
        .await
        .map_err(|error| format!("Workspace 无法创建或访问：{error}"))?;
    let workspace = tokio::fs::canonicalize(&workspace_raw)
        .await
        .map_err(|error| format!("Workspace 无法访问：{error}"))?;

    let mut prepared = Vec::new();
    for (index, raw) in paths.into_iter().take(MAX_FILES).enumerate() {
        prepared.push(prepare_path_attachment(raw, workspace.clone(), &session_id, index).await?);
    }
    Ok(prepared)
}
