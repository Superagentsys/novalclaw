//! 聊天输入框附件：按本地路径读取内容（Tauri 拖放 / 系统文件对话框）。

use base64::{engine::general_purpose::STANDARD, Engine as _};
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ExtendedColorType, GenericImageView};
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
const MAX_IMAGE_SOURCE_BYTES: u64 = 25 * 1024 * 1024;
const MAX_IMAGE_INLINE_BYTES: usize = 400 * 1024;
const MAX_IMAGE_DIMENSION_PX: u32 = 1600;
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

fn resize_max_dimension(image: DynamicImage, max_dimension_px: u32) -> DynamicImage {
    let max_dimension_px = max_dimension_px.max(320);
    let (width, height) = image.dimensions();
    let longest = width.max(height);
    if longest <= max_dimension_px {
        return image;
    }
    let scale = max_dimension_px as f32 / longest as f32;
    let target_w = ((width as f32) * scale).round().max(1.0) as u32;
    let target_h = ((height as f32) * scale).round().max(1.0) as u32;
    image.resize(target_w, target_h, FilterType::Triangle)
}

fn encode_jpeg_bytes(image: &DynamicImage, quality: u8) -> Result<Vec<u8>, String> {
    let rgb = image.to_rgb8();
    let mut buffer = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut buffer, quality);
    encoder
        .encode(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            ExtendedColorType::Rgb8,
        )
        .map_err(|error| format!("JPEG 编码失败: {error}"))?;
    Ok(buffer)
}

struct InlineImage {
    data_url: String,
    compressed: bool,
    encoded_bytes: usize,
}

fn embed_image_bytes(bytes: &[u8], ext: &str) -> Result<InlineImage, String> {
    if bytes.len() <= MAX_IMAGE_INLINE_BYTES {
        return Ok(InlineImage {
            data_url: format!("data:{};base64,{}", image_mime(ext), STANDARD.encode(bytes)),
            compressed: false,
            encoded_bytes: bytes.len(),
        });
    }

    let decoded = image::load_from_memory(bytes).map_err(|error| {
        format!("图片无法解码为可压缩格式（支持 JPEG/PNG）：{error}")
    })?;
    let mut current = resize_max_dimension(decoded, MAX_IMAGE_DIMENSION_PX);
    let qualities = [82_u8, 70, 58, 45];
    let mut best: Option<Vec<u8>> = None;
    for _ in 0..3 {
        for quality in qualities {
            let encoded = encode_jpeg_bytes(&current, quality)?;
            let small_enough = encoded.len() <= MAX_IMAGE_INLINE_BYTES;
            best = Some(encoded);
            if small_enough {
                let encoded = best.expect("JPEG just encoded");
                let encoded_bytes = encoded.len();
                return Ok(InlineImage {
                    data_url: format!("data:image/jpeg;base64,{}", STANDARD.encode(encoded)),
                    compressed: true,
                    encoded_bytes,
                });
            }
        }
        let (width, height) = current.dimensions();
        current = current.resize(
            (width as f32 * 0.75).round().max(1.0) as u32,
            (height as f32 * 0.75).round().max(1.0) as u32,
            FilterType::Triangle,
        );
    }
    let encoded = best.ok_or_else(|| "图片压缩失败".to_string())?;
    let encoded_bytes = encoded.len();
    Ok(InlineImage {
        data_url: format!("data:image/jpeg;base64,{}", STANDARD.encode(encoded)),
        compressed: true,
        encoded_bytes,
    })
}

fn markdown_image(name: &str, data_url: &str) -> String {
    format!("![{}]({data_url})", escape_markdown_alt(name))
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
    if is_image_extension(&ext) {
        if size > MAX_IMAGE_SOURCE_BYTES {
            content.push_str("\n\n内容未内嵌（超过 25MB），请使用工作区文件工具读取上面的已挂载路径。");
            note = format!("已挂载 · {} KB · 过大未嵌入", size.div_ceil(1024));
        } else {
            match tokio::fs::read(&source).await {
                Ok(bytes) => match embed_image_bytes(&bytes, &ext) {
                    Ok(inline) => {
                        content.push_str("\n\n");
                        content.push_str(&markdown_image(&name, &inline.data_url));
                        note = if inline.compressed {
                            format!(
                                "已挂载 · {} KB → {} KB · 已压缩嵌入",
                                size.div_ceil(1024),
                                inline.encoded_bytes.div_ceil(1024)
                            )
                        } else {
                            format!("已挂载 · {} KB · 图片已嵌入", size.div_ceil(1024))
                        };
                    }
                    Err(reason) => {
                        content.push_str(&format!(
                            "\n\n图片未能嵌入：{reason}；仍可通过 Workspace 路径使用工具读取原文件。"
                        ));
                        note = format!("已挂载 · {} KB · 可由工具读取", size.div_ceil(1024));
                    }
                },
                Err(error) => {
                    content.push_str(&format!("\n\n图片读取失败：{error}"));
                    note = format!("已挂载 · {} KB · 读取失败", size.div_ceil(1024));
                }
            }
        }
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
        if size > MAX_IMAGE_SOURCE_BYTES {
            return format!(
                "\n\n[图片: {name} · {} KB — 超过 25MB 读取上限未嵌入。]",
                size / 1024
            );
        }
        match tokio::fs::read(&path).await {
            Ok(bytes) => match embed_image_bytes(&bytes, &ext) {
                Ok(inline) => format!("\n\n{}", markdown_image(&name, &inline.data_url)),
                Err(reason) => format!("\n\n[图片未能嵌入: {name} — {reason}]"),
            },
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

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, Rgb, RgbImage};
    use std::io::Cursor;

    fn noisy_png(width: u32, height: u32) -> Vec<u8> {
        let mut img = RgbImage::new(width, height);
        for (x, y, pixel) in img.enumerate_pixels_mut() {
            *pixel = Rgb([
                (x.wrapping_mul(37) % 256) as u8,
                (y.wrapping_mul(91) % 256) as u8,
                ((x + y).wrapping_mul(13) % 256) as u8,
            ]);
        }
        let mut bytes = Vec::new();
        DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .expect("encode png");
        bytes
    }

    #[test]
    fn small_images_stay_original() {
        let bytes = noisy_png(24, 16);
        assert!(bytes.len() <= MAX_IMAGE_INLINE_BYTES);
        let inline = embed_image_bytes(&bytes, "png").expect("embed");
        assert!(!inline.compressed);
        assert!(inline.data_url.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn large_images_are_compressed_instead_of_dropped() {
        let bytes = noisy_png(1400, 1000);
        assert!(
            bytes.len() > MAX_IMAGE_INLINE_BYTES,
            "fixture should exceed inline cap, got {}",
            bytes.len()
        );
        let inline = embed_image_bytes(&bytes, "png").expect("compress");
        assert!(inline.compressed);
        assert!(inline.data_url.starts_with("data:image/jpeg;base64,"));
        assert!(inline.encoded_bytes <= MAX_IMAGE_INLINE_BYTES);
    }
}
