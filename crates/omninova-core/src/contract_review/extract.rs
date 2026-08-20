use std::fs::File;
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

const MAX_TEXT_BYTES: usize = 512 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    #[error("无法读取附件：{0}")]
    Io(String),
    #[error("不支持的合同格式：{0}。请使用 .docx / .pdf / .txt / .md")]
    Unsupported(String),
    #[error("{0}")]
    Empty(String),
    #[error("附件已损坏或无法解析：{0}")]
    Corrupt(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedDocument {
    pub name: String,
    pub extension: String,
    pub text: String,
}

pub fn extract_document_text(path: &Path) -> Result<ExtractedDocument, ExtractError> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("unnamed")
        .to_string();
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let metadata = std::fs::metadata(path).map_err(|error| ExtractError::Io(error.to_string()))?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(ExtractError::Empty(format!(
            "附件「{name}」是空文件，未提交审核"
        )));
    }
    let text = match extension.as_str() {
        "txt" | "md" | "markdown" => {
            let bytes = std::fs::read(path).map_err(|error| ExtractError::Io(error.to_string()))?;
            String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_TEXT_BYTES)]).into_owned()
        }
        "docx" => extract_docx(path)?,
        "pdf" => pdf_extract::extract_text(path).map_err(|error| {
            ExtractError::Corrupt(format!("PDF「{name}」文字层读取失败：{error}"))
        })?,
        _ => return Err(ExtractError::Unsupported(extension)),
    };
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err(ExtractError::Empty(if extension == "pdf" {
            "当前不支持扫描版 PDF 的 OCR，请上传文字层 PDF、DOCX、TXT 或 MD。".into()
        } else {
            format!("附件「{name}」没有可审核的文本")
        }));
    }
    Ok(ExtractedDocument {
        name,
        extension,
        text,
    })
}

fn extract_docx(path: &Path) -> Result<String, ExtractError> {
    let file = File::open(path).map_err(|error| ExtractError::Io(error.to_string()))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|error| ExtractError::Corrupt(format!("DOCX 结构无效：{error}")))?;
    let mut entry = archive
        .by_name("word/document.xml")
        .map_err(|error| ExtractError::Corrupt(error.to_string()))?;
    let mut bytes = Vec::new();
    entry
        .by_ref()
        .take(MAX_TEXT_BYTES as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| ExtractError::Io(error.to_string()))?;
    Ok(xml_visible_text(&String::from_utf8_lossy(&bytes)))
}

fn xml_visible_text(xml: &str) -> String {
    let mut output = String::new();
    let mut in_tag = false;
    for ch in xml.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                output.push(' ');
            }
            _ if !in_tag => output.push(ch),
            _ => {}
        }
    }
    output
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
