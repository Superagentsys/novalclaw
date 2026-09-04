//! Local, bounded document extraction shared by uploads and agent file reads.
use anyhow::{bail, Context, Result};
use std::path::Path;

pub const MAX_DOCUMENT_BYTES: usize = 50 * 1024 * 1024;

pub async fn read(path: &Path) -> Result<String> {
    if tokio::fs::metadata(path).await?.len() > MAX_DOCUMENT_BYTES as u64 {
        bail!("文档超过 50 MB，请拆分后导入");
    }
    extract(path.to_string_lossy().as_ref(), &tokio::fs::read(path).await?).await
}

pub async fn extract(filename: &str, bytes: &[u8]) -> Result<String> {
    if bytes.len() > MAX_DOCUMENT_BYTES {
        bail!("文档超过 50 MB，请拆分后导入");
    }
    let ext = Path::new(filename).extension().and_then(|s| s.to_str())
        .unwrap_or("").to_ascii_lowercase();
    if matches!(ext.as_str(), "doc" | "docx" | "docm" | "ppt" | "pptx" | "pptm"
        | "xls" | "xlsx" | "xlsm" | "pdf" | "rtf" | "odt" | "ods" | "odp" | "epub") {
        let data = bytes.to_vec();
        return tokio::task::spawn_blocking(move || {
            let format = anydoc::Format::from_extension(&ext)
                .ok_or_else(|| anyhow::anyhow!("不支持的文档格式: {ext}"))?;
            anydoc::to_markdown_bytes(&data, format)
                .context("本地文档解析失败（加密文件请先解密；扫描件需要 OCR）")
        }).await.context("document extraction task")?;
    }
    String::from_utf8(bytes.to_vec()).context("文件不是 UTF-8 文本或受支持的 Office 文档，请先转换编码")
}
