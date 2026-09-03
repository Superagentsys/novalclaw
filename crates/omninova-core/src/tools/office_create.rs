use crate::security::sandbox::resolve_workspace_relative;
use crate::tools::{Tool, ToolResult};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{Cursor, Write};
use std::path::PathBuf;
use zip::write::SimpleFileOptions;

#[derive(Debug, Clone, Deserialize, Default)]
struct Section {
    #[serde(default)]
    heading: String,
    #[serde(default)]
    paragraphs: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct Slide {
    #[serde(default)]
    title: String,
    #[serde(default)]
    subtitle: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    bullets: Vec<String>,
    /// Workspace-relative PNG/JPEG used as the visual anchor for this slide.
    #[serde(default)]
    image_path: String,
    #[serde(default)]
    image_alt: String,
    /// auto | image_left | image_right | full_bleed | cards
    #[serde(default)]
    layout: String,
    /// Optional six-digit RGB color, for example 315BE8.
    #[serde(default)]
    accent_color: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct Sheet {
    #[serde(default)]
    name: String,
    #[serde(default)]
    rows: Vec<Vec<Value>>,
}

#[derive(Debug, Clone, Deserialize)]
struct OfficeCreateInput {
    path: String,
    #[serde(default)]
    title: String,
    /// `official_cn` applies a GB/T 9704-friendly Chinese official-document
    /// typography preset. Other values use the general office preset.
    #[serde(default)]
    document_style: String,
    #[serde(default)]
    recipient: String,
    #[serde(default)]
    issuer: String,
    #[serde(default)]
    date: String,
    #[serde(default)]
    subtitle: String,
    #[serde(default)]
    paragraphs: Vec<String>,
    #[serde(default)]
    sections: Vec<Section>,
    #[serde(default)]
    slides: Vec<Slide>,
    #[serde(default)]
    sheets: Vec<Sheet>,
}

pub struct OfficeCreateTool {
    workspace_dir: PathBuf,
}

impl OfficeCreateTool {
    pub fn new(workspace_dir: impl Into<PathBuf>) -> Self {
        Self {
            workspace_dir: workspace_dir.into(),
        }
    }
}

#[async_trait]
impl Tool for OfficeCreateTool {
    fn name(&self) -> &str {
        "office_create"
    }

    fn description(&self) -> &str {
        "Create a polished, real and editable DOCX, PPTX, or XLSX inside the workspace in one call, without Python, Node, KDocs, Office, shell commands, or network access. PPTX supports workspace-local PNG/JPEG images, automatic image/text layouts and themed color blocks. Pass the complete slides array once. For Chinese official documents use document_style=official_cn plus recipient, issuer and date. Do not manually edit OOXML or substitute HTML/Markdown/CSV/XML."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Workspace-relative output path ending in .docx, .pptx, or .xlsx" },
                "title": { "type": "string" },
                "subtitle": { "type": "string", "description": "Presentation cover subtitle" },
                "document_style": { "type": "string", "enum": ["normal", "official_cn"], "description": "Use official_cn for Chinese government notices, reports, requests and letters" },
                "recipient": { "type": "string", "description": "DOCX addressee / 主送机关" },
                "issuer": { "type": "string", "description": "DOCX issuing organization / 发文机关" },
                "date": { "type": "string", "description": "DOCX issue date / 成文日期" },
                "paragraphs": { "type": "array", "items": { "type": "string" }, "description": "DOCX paragraphs" },
                "sections": {
                    "type": "array",
                    "items": { "type": "object", "properties": {
                        "heading": { "type": "string" },
                        "paragraphs": { "type": "array", "items": { "type": "string" } }
                    }}
                },
                "slides": {
                    "type": "array",
                    "items": { "type": "object", "properties": {
                        "title": { "type": "string" },
                        "subtitle": { "type": "string" },
                        "body": { "type": "string" },
                        "bullets": { "type": "array", "items": { "type": "string" } },
                        "image_path": { "type": "string", "description": "Workspace-relative .png, .jpg, or .jpeg image path" },
                        "image_alt": { "type": "string", "description": "Accessible image description" },
                        "layout": { "type": "string", "enum": ["auto", "image_left", "image_right", "full_bleed", "cards"] },
                        "accent_color": { "type": "string", "description": "Optional six-digit RGB accent color" }
                    }}
                },
                "sheets": {
                    "type": "array",
                    "items": { "type": "object", "properties": {
                        "name": { "type": "string" },
                        "rows": { "type": "array", "items": { "type": "array", "items": {} } }
                    }}
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        let input: OfficeCreateInput = match serde_json::from_value(args) {
            Ok(input) => input,
            Err(error) => {
                return Ok(ToolResult::failure(format!(
                    "Invalid Office input: {error}"
                )))
            }
        };
        let extension = PathBuf::from(&input.path)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let bytes = match extension.as_str() {
            "docx" => build_docx(&input)?,
            "pptx" => build_pptx(&input, &self.workspace_dir).await?,
            "xlsx" => build_xlsx(&input)?,
            _ => {
                return Ok(ToolResult::failure(
                    "office_create path must end in .docx, .pptx, or .xlsx",
                ))
            }
        };
        let resolved = match resolve_workspace_relative(&self.workspace_dir, &input.path).await {
            Ok(path) => path,
            Err(error) => return Ok(ToolResult::failure(error.to_string())),
        };
        if let Some(parent) = resolved.parent() {
            if let Err(error) = tokio::fs::create_dir_all(parent).await {
                return Ok(ToolResult::failure(format!(
                    "Failed to create output directory: {error}"
                )));
            }
        }
        let existed = tokio::fs::metadata(&resolved).await.is_ok();
        if let Err(error) = tokio::fs::write(&resolved, &bytes).await {
            return Ok(ToolResult::failure(format!(
                "Failed to write Office file: {error}"
            )));
        }
        let summary = format!(
            "Created editable {} file ({} bytes): {}",
            extension.to_ascii_uppercase(),
            bytes.len(),
            input.path
        );
        Ok(ToolResult::success(serde_json::to_string(&json!({
            "message": summary,
            "path": input.path,
            "bytes": bytes.len(),
            "format": extension,
            "change_type": if existed { "modified" } else { "created" },
            "additions": 1,
            "deletions": if existed { 1 } else { 0 },
            "old_text": null,
            "new_text": summary,
            "content_truncated": false,
            "content_total_chars": summary.chars().count(),
            "content_preview_chars": summary.chars().count()
        }))?))
    }
}

fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn zip_package(entries: Vec<(String, String)>) -> anyhow::Result<Vec<u8>> {
    let cursor = Cursor::new(Vec::new());
    let mut archive = zip::ZipWriter::new(cursor);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);
    for (name, content) in entries {
        archive.start_file(name, options)?;
        archive.write_all(content.as_bytes())?;
    }
    Ok(archive.finish()?.into_inner())
}

fn zip_package_with_binary(
    entries: Vec<(String, String)>,
    binary_entries: Vec<(String, Vec<u8>)>,
) -> anyhow::Result<Vec<u8>> {
    let cursor = Cursor::new(Vec::new());
    let mut archive = zip::ZipWriter::new(cursor);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);
    for (name, content) in entries {
        archive.start_file(name, options)?;
        archive.write_all(content.as_bytes())?;
    }
    for (name, content) in binary_entries {
        archive.start_file(name, options)?;
        archive.write_all(&content)?;
    }
    Ok(archive.finish()?.into_inner())
}

fn core_properties(title: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><dc:title>{}</dc:title><dc:creator>OmniNova</dc:creator><cp:lastModifiedBy>OmniNova</cp:lastModifiedBy></cp:coreProperties>"#,
        xml(title)
    )
}

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="OFFICE_TARGET"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties" Target="docProps/app.xml"/></Relationships>"#;

fn docx_runs(text: &str) -> String {
    text.split('\n')
        .enumerate()
        .map(|(index, line)| {
            let br = if index == 0 { "" } else { "<w:br/>" };
            format!(
                r#"<w:r>{br}<w:t xml:space="preserve">{}</w:t></w:r>"#,
                xml(line)
            )
        })
        .collect()
}

fn docx_paragraph(text: &str, style: Option<&str>) -> String {
    let properties = style
        .map(|name| format!(r#"<w:pPr><w:pStyle w:val="{name}"/></w:pPr>"#))
        .unwrap_or_default();
    format!(r#"<w:p>{properties}{}</w:p>"#, docx_runs(text))
}

fn build_docx(input: &OfficeCreateInput) -> anyhow::Result<Vec<u8>> {
    let official = input.document_style.eq_ignore_ascii_case("official_cn");
    let mut body = String::new();
    if !input.title.is_empty() {
        body.push_str(&docx_paragraph(&input.title, Some("Title")));
    }
    if !input.recipient.is_empty() {
        body.push_str(&docx_paragraph(&input.recipient, Some("Recipient")));
    }
    for paragraph in &input.paragraphs {
        body.push_str(&docx_paragraph(paragraph, None));
    }
    for section in &input.sections {
        if !section.heading.is_empty() {
            body.push_str(&docx_paragraph(&section.heading, Some("Heading1")));
        }
        for paragraph in &section.paragraphs {
            body.push_str(&docx_paragraph(paragraph, None));
        }
    }
    if body.is_empty() {
        body.push_str(&docx_paragraph("", None));
    }
    if !input.issuer.is_empty() {
        body.push_str(&docx_paragraph(&input.issuer, Some("Signature")));
    }
    if !input.date.is_empty() {
        body.push_str(&docx_paragraph(&input.date, Some("Signature")));
    }
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="2098" w:right="1496" w:bottom="1984" w:left="1600"/><w:cols w:space="425"/><w:docGrid w:type="lines" w:linePitch="560"/></w:sectPr></w:body></w:document>"#
    );
    let styles = if official {
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="FangSong" w:eastAsia="仿宋_GB2312" w:hAnsi="FangSong"/><w:lang w:val="zh-CN" w:eastAsia="zh-CN"/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="公文正文"/><w:pPr><w:spacing w:before="0" w:after="0" w:line="560" w:lineRule="exact"/><w:ind w:firstLineChars="200"/><w:jc w:val="both"/></w:pPr><w:rPr><w:rFonts w:ascii="FangSong" w:eastAsia="仿宋_GB2312" w:hAnsi="FangSong"/><w:sz w:val="32"/><w:szCs w:val="32"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Title"><w:name w:val="公文标题"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:before="0" w:after="560" w:line="560" w:lineRule="exact"/><w:ind w:firstLineChars="0"/><w:jc w:val="center"/></w:pPr><w:rPr><w:rFonts w:ascii="SimSun" w:eastAsia="方正小标宋简体" w:hAnsi="SimSun"/><w:b/><w:sz w:val="44"/><w:szCs w:val="44"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="一级标题"/><w:basedOn w:val="Normal"/><w:pPr><w:ind w:firstLineChars="200"/><w:keepNext/></w:pPr><w:rPr><w:rFonts w:ascii="SimHei" w:eastAsia="黑体" w:hAnsi="SimHei"/><w:b/><w:sz w:val="32"/><w:szCs w:val="32"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Recipient"><w:name w:val="主送机关"/><w:basedOn w:val="Normal"/><w:pPr><w:ind w:firstLineChars="0"/></w:pPr></w:style><w:style w:type="paragraph" w:styleId="Signature"><w:name w:val="落款"/><w:basedOn w:val="Normal"/><w:pPr><w:ind w:firstLineChars="0"/><w:jc w:val="right"/></w:pPr></w:style></w:styles>"#.to_string()
    } else {
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:pPr><w:spacing w:after="160" w:line="360" w:lineRule="auto"/></w:pPr><w:rPr><w:rFonts w:ascii="Aptos" w:eastAsia="微软雅黑"/><w:sz w:val="22"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:basedOn w:val="Normal"/><w:pPr><w:jc w:val="center"/><w:spacing w:after="360"/></w:pPr><w:rPr><w:rFonts w:ascii="Aptos Display" w:eastAsia="微软雅黑"/><w:b/><w:sz w:val="40"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:pPr><w:keepNext/></w:pPr><w:rPr><w:b/><w:sz w:val="30"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Recipient"><w:name w:val="Recipient"/><w:basedOn w:val="Normal"/></w:style><w:style w:type="paragraph" w:styleId="Signature"><w:name w:val="Signature"/><w:basedOn w:val="Normal"/><w:pPr><w:jc w:val="right"/></w:pPr></w:style></w:styles>"#.to_string()
    };
    zip_package(vec![
        ("[Content_Types].xml".into(), r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/><Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/></Types>"#.into()),
        ("_rels/.rels".into(), ROOT_RELS.replace("OFFICE_TARGET", "word/document.xml")),
        ("docProps/core.xml".into(), core_properties(&input.title)),
        ("docProps/app.xml".into(), r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>OmniNova</Application></Properties>"#.into()),
        ("word/document.xml".into(), document),
        ("word/styles.xml".into(), styles),
        ("word/_rels/document.xml.rels".into(), r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#.into()),
    ])
}

fn ppt_rect(
    id: usize,
    name: &str,
    x: i64,
    y: i64,
    cx: i64,
    cy: i64,
    fill: &str,
    line: &str,
) -> String {
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{}"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="roundRect"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="{fill}"/></a:solidFill><a:ln w="9525"><a:solidFill><a:srgbClr val="{line}"/></a:solidFill></a:ln></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:endParaRPr lang="zh-CN"/></a:p></p:txBody></p:sp>"#,
        xml(name)
    )
}

fn normalized_rgb(value: &str, fallback: &str) -> String {
    let trimmed = value.trim().trim_start_matches('#');
    if trimmed.len() == 6
        && trimmed
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        trimmed.to_ascii_uppercase()
    } else {
        fallback.to_string()
    }
}

#[derive(Debug)]
struct PptImage {
    bytes: Vec<u8>,
    extension: &'static str,
    width: u32,
    height: u32,
}

async fn load_ppt_image(
    workspace_dir: &std::path::Path,
    path: &str,
    slide_number: usize,
) -> anyhow::Result<Option<PptImage>> {
    if path.trim().is_empty() {
        return Ok(None);
    }
    let resolved = resolve_workspace_relative(workspace_dir, path)
        .await
        .map_err(|error| anyhow::anyhow!("Slide {slide_number} image path is invalid: {error}"))?;
    let metadata = tokio::fs::metadata(&resolved).await.map_err(|error| {
        anyhow::anyhow!(
            "Slide {slide_number} image could not be read ({}): {error}",
            resolved.display()
        )
    })?;
    if metadata.len() > 25 * 1024 * 1024 {
        anyhow::bail!("Slide {slide_number} image exceeds the 25 MB limit");
    }
    let bytes = tokio::fs::read(&resolved).await?;
    let format = image::guess_format(&bytes).map_err(|_| {
        anyhow::anyhow!("Slide {slide_number} image is not a valid PNG or JPEG file")
    })?;
    let extension = match format {
        image::ImageFormat::Png => "png",
        image::ImageFormat::Jpeg => "jpeg",
        _ => {
            anyhow::bail!("Slide {slide_number} image format is unsupported; use PNG, JPG, or JPEG")
        }
    };
    let reader = image::ImageReader::new(Cursor::new(bytes.as_slice()))
        .with_guessed_format()
        .map_err(|error| anyhow::anyhow!("Slide {slide_number} image is invalid: {error}"))?;
    let (width, height) = reader
        .into_dimensions()
        .map_err(|error| anyhow::anyhow!("Slide {slide_number} image is invalid: {error}"))?;
    if width == 0 || height == 0 {
        anyhow::bail!("Slide {slide_number} image has invalid dimensions");
    }
    Ok(Some(PptImage {
        bytes,
        extension,
        width,
        height,
    }))
}

#[allow(clippy::too_many_arguments)]
fn ppt_picture(
    id: usize,
    relationship_id: &str,
    name: &str,
    alt: &str,
    x: i64,
    y: i64,
    cx: i64,
    cy: i64,
    image_width: u32,
    image_height: u32,
) -> String {
    let frame_ratio = cx as f64 / cy as f64;
    let image_ratio = image_width as f64 / image_height as f64;
    let (left, top, right, bottom) = if image_ratio > frame_ratio {
        let visible = frame_ratio / image_ratio;
        let crop = (((1.0 - visible) / 2.0) * 100_000.0).round() as i64;
        (crop, 0, crop, 0)
    } else {
        let visible = image_ratio / frame_ratio;
        let crop = (((1.0 - visible) / 2.0) * 100_000.0).round() as i64;
        (0, crop, 0, crop)
    };
    format!(
        r#"<p:pic><p:nvPicPr><p:cNvPr id="{id}" name="{}" descr="{}"/><p:cNvPicPr><a:picLocks noChangeAspect="1"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="{relationship_id}"/><a:srcRect l="{left}" t="{top}" r="{right}" b="{bottom}"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:ln><a:noFill/></a:ln></p:spPr></p:pic>"#,
        xml(name),
        xml(alt)
    )
}

fn ppt_paragraph(
    text: &str,
    size: i32,
    color: &str,
    bold: bool,
    align: &str,
    bullet: bool,
) -> String {
    let bullet_properties = if bullet {
        format!(
            r#"<a:pPr algn="{align}" marL="342900" indent="-285750"><a:buChar char="•"/></a:pPr>"#
        )
    } else {
        format!(r#"<a:pPr algn="{align}"/>"#)
    };
    let bold = if bold { " b=\"1\"" } else { "" };
    format!(
        r#"<a:p>{bullet_properties}<a:r><a:rPr lang="zh-CN" sz="{size}"{bold}><a:solidFill><a:srgbClr val="{color}"/></a:solidFill><a:latin typeface="Aptos"/><a:ea typeface="微软雅黑"/></a:rPr><a:t>{}</a:t></a:r><a:endParaRPr lang="zh-CN" sz="{size}"/></a:p>"#,
        xml(text)
    )
}

#[allow(clippy::too_many_arguments)]
fn ppt_text_box(
    id: usize,
    name: &str,
    paragraphs: &[(String, bool)],
    x: i64,
    y: i64,
    cx: i64,
    cy: i64,
    size: i32,
    color: &str,
    bold: bool,
    align: &str,
    anchor: &str,
) -> String {
    let content = if paragraphs.is_empty() {
        ppt_paragraph("", size, color, bold, align, false)
    } else {
        paragraphs
            .iter()
            .map(|(text, bullet)| ppt_paragraph(text, size, color, bold, align, *bullet))
            .collect::<String>()
    };
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{}"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/><a:ln><a:noFill/></a:ln></p:spPr><p:txBody><a:bodyPr wrap="square" anchor="{anchor}" lIns="91440" rIns="91440" tIns="45720" bIns="45720"><a:normAutofit fontScale="90000" lnSpcReduction="10000"/></a:bodyPr><a:lstStyle/>{content}</p:txBody></p:sp>"#,
        xml(name)
    )
}

fn presentation_slide_shapes(
    slide: &Slide,
    index: usize,
    total: usize,
    deck_subtitle: &str,
    image: Option<&PptImage>,
) -> String {
    let accent = normalized_rgb(&slide.accent_color, "315BE8");
    if index == 0 {
        let subtitle = if slide.subtitle.trim().is_empty() {
            deck_subtitle
        } else {
            slide.subtitle.as_str()
        };
        let mut shapes = String::new();
        shapes.push_str(&ppt_rect(
            2,
            "Background",
            0,
            0,
            12_192_000,
            6_858_000,
            "0B1F3A",
            "0B1F3A",
        ));
        let has_image = image.is_some();
        let full_bleed = slide.layout.eq_ignore_ascii_case("full_bleed");
        if let Some(image) = image {
            let (x, cx) = if full_bleed {
                (0, 12_192_000)
            } else {
                (7_315_200, 4_876_800)
            };
            shapes.push_str(&ppt_picture(
                80,
                "rId2",
                "Cover Visual",
                if slide.image_alt.trim().is_empty() {
                    "Presentation cover image"
                } else {
                    &slide.image_alt
                },
                x,
                0,
                cx,
                6_858_000,
                image.width,
                image.height,
            ));
            if full_bleed {
                shapes.push_str(&ppt_rect(
                    81,
                    "Title Panel",
                    0,
                    0,
                    6_949_440,
                    6_858_000,
                    "0B1F3A",
                    "0B1F3A",
                ));
            }
        }
        shapes.push_str(&ppt_rect(
            3, "Accent", 685_800, 1_097_280, 114_300, 3_657_600, &accent, &accent,
        ));
        shapes.push_str(&ppt_text_box(
            4,
            "Eyebrow",
            &[("OMNINOVA · INSIGHT DECK".into(), false)],
            914_400,
            731_520,
            5_486_400,
            457_200,
            1200,
            "62D9E6",
            true,
            "l",
            "ctr",
        ));
        shapes.push_str(&ppt_text_box(
            5,
            "Cover Title",
            &[(slide.title.clone(), false)],
            914_400,
            1_554_480,
            if has_image { 5_943_600 } else { 9_601_200 },
            2_102_640,
            5200,
            "FFFFFF",
            true,
            "l",
            "ctr",
        ));
        if !subtitle.trim().is_empty() {
            shapes.push_str(&ppt_text_box(
                6,
                "Cover Subtitle",
                &[(subtitle.to_string(), false)],
                914_400,
                3_794_760,
                if has_image { 5_760_720 } else { 8_686_800 },
                914_400,
                2000,
                "B9C9DB",
                false,
                "l",
                "ctr",
            ));
        }
        shapes.push_str(&ppt_text_box(
            7,
            "Cover Footer",
            &[(format!("共 {total} 页  ·  由 OmniNova 生成"), false)],
            914_400,
            5_806_440,
            4_572_000,
            365_760,
            1000,
            "7F9AB5",
            false,
            "l",
            "ctr",
        ));
        return shapes;
    }

    if let Some(image) = image.filter(|_| slide.layout.eq_ignore_ascii_case("full_bleed")) {
        let mut shapes = String::new();
        shapes.push_str(&ppt_picture(
            2,
            "rId2",
            "Full Slide Visual",
            if slide.image_alt.trim().is_empty() {
                "Presentation visual"
            } else {
                &slide.image_alt
            },
            0,
            0,
            12_192_000,
            6_858_000,
            image.width,
            image.height,
        ));
        shapes.push_str(&ppt_rect(
            3,
            "Content Panel",
            0,
            0,
            6_583_680,
            6_858_000,
            "0B1F3A",
            "0B1F3A",
        ));
        shapes.push_str(&ppt_rect(
            4, "Accent", 685_800, 731_520, 114_300, 4_937_760, &accent, &accent,
        ));
        shapes.push_str(&ppt_text_box(
            5,
            "Slide Title",
            &[(slide.title.clone(), false)],
            1_005_840,
            731_520,
            4_846_320,
            1_280_160,
            3500,
            "FFFFFF",
            true,
            "l",
            "ctr",
        ));
        let mut paragraphs = Vec::new();
        if !slide.body.trim().is_empty() {
            paragraphs.push((slide.body.clone(), false));
        }
        paragraphs.extend(
            slide
                .bullets
                .iter()
                .take(5)
                .cloned()
                .map(|item| (item, true)),
        );
        shapes.push_str(&ppt_text_box(
            6,
            "Content",
            &paragraphs,
            1_005_840,
            2_194_560,
            4_937_760,
            3_291_840,
            1800,
            "E8F0FF",
            false,
            "l",
            "t",
        ));
        shapes.push_str(&ppt_text_box(
            90,
            "Page Number",
            &[(format!("{:02} / {:02}", index + 1, total), false)],
            10_424_160,
            6_217_920,
            1_066_800,
            274_320,
            900,
            "FFFFFF",
            false,
            "r",
            "ctr",
        ));
        return shapes;
    }

    let mut shapes = String::new();
    shapes.push_str(&ppt_rect(
        2,
        "Background",
        0,
        0,
        12_192_000,
        6_858_000,
        "F5F7FB",
        "F5F7FB",
    ));
    shapes.push_str(&ppt_rect(
        3,
        "Top Accent",
        0,
        0,
        12_192_000,
        91_440,
        &accent,
        &accent,
    ));
    shapes.push_str(&ppt_rect(
        4,
        "Section Badge",
        685_800,
        438_912,
        731_520,
        457_200,
        &accent,
        &accent,
    ));
    shapes.push_str(&ppt_text_box(
        5,
        "Section Number",
        &[(format!("{:02}", index + 1), false)],
        685_800,
        438_912,
        731_520,
        457_200,
        1200,
        "FFFFFF",
        true,
        "ctr",
        "ctr",
    ));
    shapes.push_str(&ppt_text_box(
        6,
        "Slide Title",
        &[(slide.title.clone(), false)],
        1_600_200,
        365_760,
        9_144_000,
        731_520,
        3500,
        "102A43",
        true,
        "l",
        "ctr",
    ));

    if let Some(image) = image {
        let image_left = slide.layout.eq_ignore_ascii_case("image_left")
            || ((slide.layout.is_empty() || slide.layout.eq_ignore_ascii_case("auto"))
                && index % 2 == 0);
        let image_x = if image_left { 685_800 } else { 6_766_560 };
        let text_x = if image_left { 6_217_920 } else { 685_800 };
        shapes.push_str(&ppt_rect(
            10,
            "Image Accent",
            image_x - 68_580,
            1_211_580,
            5_006_340,
            4_937_760,
            &accent,
            &accent,
        ));
        shapes.push_str(&ppt_picture(
            11,
            "rId2",
            "Slide Visual",
            if slide.image_alt.trim().is_empty() {
                "Presentation visual"
            } else {
                &slide.image_alt
            },
            image_x,
            1_280_160,
            4_937_760,
            4_754_880,
            image.width,
            image.height,
        ));
        let mut paragraphs = Vec::new();
        if !slide.body.trim().is_empty() {
            paragraphs.push((slide.body.clone(), false));
        }
        paragraphs.extend(
            slide
                .bullets
                .iter()
                .take(6)
                .cloned()
                .map(|item| (item, true)),
        );
        shapes.push_str(&ppt_text_box(
            12,
            "Image Slide Content",
            &paragraphs,
            text_x,
            1_371_600,
            4_937_760,
            4_572_000,
            1800,
            "183B56",
            false,
            "l",
            "t",
        ));
        if !slide.image_alt.trim().is_empty() {
            shapes.push_str(&ppt_text_box(
                13,
                "Image Caption",
                &[(slide.image_alt.clone(), false)],
                image_x,
                5_669_280,
                4_937_760,
                274_320,
                900,
                "6B7C93",
                false,
                "r",
                "ctr",
            ));
        }
        shapes.push_str(&ppt_text_box(
            90,
            "Page Number",
            &[(format!("{:02} / {:02}", index + 1, total), false)],
            10_424_160,
            6_217_920,
            1_066_800,
            274_320,
            900,
            "6B7C93",
            false,
            "r",
            "ctr",
        ));
        return shapes;
    }

    let has_bullets = !slide.bullets.is_empty();
    if !slide.body.trim().is_empty() {
        let body_y = 1_280_160;
        let body_h = if has_bullets { 822_960 } else { 4_754_880 };
        shapes.push_str(&ppt_rect(
            7,
            "Summary Card",
            685_800,
            body_y,
            10_820_400,
            body_h,
            "E8F0FF",
            "C7D7F7",
        ));
        shapes.push_str(&ppt_text_box(
            8,
            "Summary",
            &[(slide.body.clone(), false)],
            914_400,
            body_y + 91_440,
            10_363_200,
            body_h - 182_880,
            if has_bullets { 1650 } else { 1900 },
            "294766",
            false,
            "l",
            if has_bullets { "ctr" } else { "t" },
        ));
    }

    if has_bullets {
        let start_y = if slide.body.trim().is_empty() {
            1_280_160
        } else {
            2_331_720
        };
        let rows = slide.bullets.len().min(8).div_ceil(2);
        let available_h = 5_943_600 - start_y;
        let gap = 137_160;
        let card_h = ((available_h - gap * (rows.saturating_sub(1) as i64)) / rows.max(1) as i64)
            .max(640_080);
        let card_w = 5_257_800;
        for (bullet_index, bullet) in slide.bullets.iter().take(8).enumerate() {
            let column = bullet_index % 2;
            let row = bullet_index / 2;
            let x = 685_800 + column as i64 * (card_w + 304_800);
            let y = start_y + row as i64 * (card_h + gap);
            let shape_id = 20 + bullet_index * 2;
            shapes.push_str(&ppt_rect(
                shape_id,
                "Point Card",
                x,
                y,
                card_w,
                card_h,
                "FFFFFF",
                "D8E0EC",
            ));
            shapes.push_str(&ppt_text_box(
                shape_id + 1,
                "Point",
                &[(format!("{}  {}", bullet_index + 1, bullet), false)],
                x + 182_880,
                y + 91_440,
                card_w - 365_760,
                card_h - 182_880,
                1650,
                "183B56",
                bullet.len() < 28,
                "l",
                "ctr",
            ));
        }
    }
    shapes.push_str(&ppt_text_box(
        90,
        "Page Number",
        &[(format!("{:02} / {:02}", index + 1, total), false)],
        10_424_160,
        6_217_920,
        1_066_800,
        274_320,
        900,
        "6B7C93",
        false,
        "r",
        "ctr",
    ));
    shapes
}

async fn build_pptx(
    input: &OfficeCreateInput,
    workspace_dir: &std::path::Path,
) -> anyhow::Result<Vec<u8>> {
    let slides = if input.slides.is_empty() {
        vec![Slide {
            title: input.title.clone(),
            subtitle: input.subtitle.clone(),
            body: input.paragraphs.join("\n"),
            bullets: Vec::new(),
            image_path: String::new(),
            image_alt: String::new(),
            layout: String::new(),
            accent_color: String::new(),
        }]
    } else {
        input.slides.clone()
    };
    if slides.len() > 100 {
        anyhow::bail!("A presentation may contain at most 100 slides");
    }
    let mut slide_images = Vec::with_capacity(slides.len());
    for (index, slide) in slides.iter().enumerate() {
        slide_images.push(load_ppt_image(workspace_dir, &slide.image_path, index + 1).await?);
    }
    let mut entries = Vec::new();
    let mut binary_entries = Vec::new();
    let mut overrides = String::new();
    let mut slide_ids = String::new();
    let mut relationships = String::new();
    for (index, slide) in slides.iter().enumerate() {
        let number = index + 1;
        overrides.push_str(&format!(r#"<Override PartName="/ppt/slides/slide{number}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>"#));
        slide_ids.push_str(&format!(
            r#"<p:sldId id="{}" r:id="rId{}"/>"#,
            255 + number,
            number + 1
        ));
        relationships.push_str(&format!(r#"<Relationship Id="rId{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide{number}.xml"/>"#, number + 1));
        let image = slide_images[index].as_ref();
        let shapes = presentation_slide_shapes(slide, index, slides.len(), &input.subtitle, image);
        let slide_xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>{shapes}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>"#
        );
        entries.push((format!("ppt/slides/slide{number}.xml"), slide_xml));
        let image_relationship = if let Some(image) = image {
            let media_name = format!("image{number}.{}", image.extension);
            binary_entries.push((format!("ppt/media/{media_name}"), image.bytes.clone()));
            format!(
                r#"<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/{media_name}"/>"#
            )
        } else {
            String::new()
        };
        entries.push((format!("ppt/slides/_rels/slide{number}.xml.rels"), format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>{image_relationship}</Relationships>"#)));
    }
    entries.extend(vec![
        ("[Content_Types].xml".into(), format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Default Extension="jpg" ContentType="image/jpeg"/><Default Extension="jpeg" ContentType="image/jpeg"/><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/><Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/><Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/><Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/><Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/><Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/>{overrides}</Types>"#)),
        ("_rels/.rels".into(), ROOT_RELS.replace("OFFICE_TARGET", "ppt/presentation.xml")),
        ("docProps/core.xml".into(), core_properties(&input.title)),
        ("docProps/app.xml".into(), format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>OmniNova</Application><Slides>{}</Slides></Properties>"#, slides.len())),
        ("ppt/presentation.xml".into(), format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:presentation xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst><p:sldIdLst>{slide_ids}</p:sldIdLst><p:sldSz cx="12192000" cy="6858000" type="screen16x9"/><p:notesSz cx="6858000" cy="9144000"/></p:presentation>"#)),
        ("ppt/_rels/presentation.xml.rels".into(), format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="slideMasters/slideMaster1.xml"/>{relationships}</Relationships>"#)),
        ("ppt/slideMasters/slideMaster1.xml".into(), r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sldMaster xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr></p:spTree></p:cSld><p:clrMap accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" bg1="lt1" bg2="lt2" folHlink="folHlink" hlink="hlink" tx1="dk1" tx2="dk2"/><p:sldLayoutIdLst><p:sldLayoutId id="1" r:id="rId1"/></p:sldLayoutIdLst><p:txStyles><p:titleStyle/><p:bodyStyle/><p:otherStyle/></p:txStyles></p:sldMaster>"#.into()),
        ("ppt/slideMasters/_rels/slideMaster1.xml.rels".into(), r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="../theme/theme1.xml"/></Relationships>"#.into()),
        ("ppt/slideLayouts/slideLayout1.xml".into(), r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sldLayout xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" type="blank"><p:cSld name="Blank"><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr></p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"#.into()),
        ("ppt/slideLayouts/_rels/slideLayout1.xml.rels".into(), r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="../slideMasters/slideMaster1.xml"/></Relationships>"#.into()),
        ("ppt/theme/theme1.xml".into(), r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="OmniNova"><a:themeElements><a:clrScheme name="OmniNova"><a:dk1><a:srgbClr val="101828"/></a:dk1><a:lt1><a:srgbClr val="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="1F2937"/></a:dk2><a:lt2><a:srgbClr val="F3F6FC"/></a:lt2><a:accent1><a:srgbClr val="315BE8"/></a:accent1><a:accent2><a:srgbClr val="12B8B0"/></a:accent2><a:accent3><a:srgbClr val="6B7C93"/></a:accent3><a:accent4><a:srgbClr val="F59E0B"/></a:accent4><a:accent5><a:srgbClr val="7C3AED"/></a:accent5><a:accent6><a:srgbClr val="EF4444"/></a:accent6><a:hlink><a:srgbClr val="0563C1"/></a:hlink><a:folHlink><a:srgbClr val="954F72"/></a:folHlink></a:clrScheme><a:fontScheme name="OmniNova"><a:majorFont><a:latin typeface="Aptos Display"/><a:ea typeface="微软雅黑"/><a:cs typeface="Arial"/></a:majorFont><a:minorFont><a:latin typeface="Aptos"/><a:ea typeface="微软雅黑"/><a:cs typeface="Arial"/></a:minorFont></a:fontScheme><a:fmtScheme name="OmniNova"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst><a:lnStyleLst><a:ln w="9525"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme></a:themeElements></a:theme>"#.into()),
    ]);
    zip_package_with_binary(entries, binary_entries)
}

fn column_name(mut index: usize) -> String {
    let mut name = String::new();
    loop {
        name.insert(0, (b'A' + (index % 26) as u8) as char);
        if index < 26 {
            break;
        }
        index = index / 26 - 1;
    }
    name
}

fn sheet_name(name: &str, index: usize) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| !matches!(c, ':' | '\\' | '/' | '?' | '*' | '[' | ']'))
        .take(31)
        .collect();
    if cleaned.trim().is_empty() {
        format!("Sheet{}", index + 1)
    } else {
        cleaned
    }
}

fn build_xlsx(input: &OfficeCreateInput) -> anyhow::Result<Vec<u8>> {
    let sheets = if input.sheets.is_empty() {
        vec![Sheet {
            name: input.title.clone(),
            rows: Vec::new(),
        }]
    } else {
        input.sheets.clone()
    };
    if sheets.len() > 32 {
        anyhow::bail!("A workbook may contain at most 32 sheets");
    }
    let mut entries = Vec::new();
    let mut overrides = String::new();
    let mut sheet_nodes = String::new();
    let mut relationships = String::new();
    for (sheet_index, sheet) in sheets.iter().enumerate() {
        let number = sheet_index + 1;
        let name = sheet_name(&sheet.name, sheet_index);
        overrides.push_str(&format!(r#"<Override PartName="/xl/worksheets/sheet{number}.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>"#));
        sheet_nodes.push_str(&format!(
            r#"<sheet name="{}" sheetId="{number}" r:id="rId{number}"/>"#,
            xml(&name)
        ));
        relationships.push_str(&format!(r#"<Relationship Id="rId{number}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet{number}.xml"/>"#));
        if sheet.rows.len() > 20_000 {
            anyhow::bail!("A sheet may contain at most 20000 rows");
        }
        let column_count = sheet.rows.iter().map(Vec::len).max().unwrap_or(1);
        let columns = (0..column_count)
            .map(|column_index| {
                let longest = sheet
                    .rows
                    .iter()
                    .filter_map(|row| row.get(column_index))
                    .map(|value| {
                        value
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| value.to_string())
                            .chars()
                            .count()
                    })
                    .max()
                    .unwrap_or(8);
                let width = (longest as f64 * 1.7 + 3.0).clamp(11.0, 36.0);
                format!(
                    r#"<col min="{}" max="{}" width="{width:.1}" customWidth="1"/>"#,
                    column_index + 1,
                    column_index + 1
                )
            })
            .collect::<String>();
        let mut rows = String::new();
        for (row_index, row) in sheet.rows.iter().enumerate() {
            if row.len() > 256 {
                anyhow::bail!("A row may contain at most 256 cells");
            }
            let style = if row_index == 0 {
                1
            } else if row_index % 2 == 0 {
                2
            } else {
                0
            };
            let cells = row.iter().enumerate().map(|(column_index, value)| {
                let reference = format!("{}{}", column_name(column_index), row_index + 1);
                match value {
                    Value::Number(number) => format!(r#"<c r="{reference}" s="{style}"><v>{number}</v></c>"#),
                    Value::Bool(value) => format!(r#"<c r="{reference}" s="{style}" t="b"><v>{}</v></c>"#, if *value { 1 } else { 0 }),
                    Value::Null => format!(r#"<c r="{reference}" s="{style}"/>"#),
                    other => {
                        let text = other.as_str().map(str::to_owned).unwrap_or_else(|| other.to_string());
                        format!(r#"<c r="{reference}" s="{style}" t="inlineStr"><is><t xml:space="preserve">{}</t></is></c>"#, xml(&text))
                    }
                }
            }).collect::<String>();
            rows.push_str(&format!(r#"<row r="{}">{cells}</row>"#, row_index + 1));
        }
        entries.push((format!("xl/worksheets/sheet{number}.xml"), format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetViews><sheetView workbookViewId="0" showGridLines="0"><pane ySplit="1" topLeftCell="A2" activePane="bottomLeft" state="frozen"/></sheetView></sheetViews><cols>{columns}</cols><sheetData>{rows}</sheetData><autoFilter ref="A1:{}{}"/></worksheet>"#, column_name(column_count.saturating_sub(1)), sheet.rows.len().max(1))));
    }
    entries.extend(vec![
        ("[Content_Types].xml".into(), format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/><Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/><Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/>{overrides}</Types>"#)),
        ("_rels/.rels".into(), ROOT_RELS.replace("OFFICE_TARGET", "xl/workbook.xml")),
        ("docProps/core.xml".into(), core_properties(&input.title)),
        ("docProps/app.xml".into(), r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>OmniNova</Application></Properties>"#.into()),
        ("xl/workbook.xml".into(), format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets>{sheet_nodes}</sheets></workbook>"#)),
        ("xl/_rels/workbook.xml.rels".into(), format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{relationships}<Relationship Id="rId{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#, sheets.len() + 1)),
        ("xl/styles.xml".into(), r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="2"><font><sz val="11"/><name val="Aptos"/><family val="2"/></font><font><b/><color rgb="FFFFFFFF"/><sz val="11"/><name val="微软雅黑"/></font></fonts><fills count="4"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill><fill><patternFill patternType="solid"><fgColor rgb="FF315BE8"/><bgColor indexed="64"/></patternFill></fill><fill><patternFill patternType="solid"><fgColor rgb="FFF3F6FC"/><bgColor indexed="64"/></patternFill></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="3"><xf xfId="0" fontId="0" fillId="0" borderId="0" applyAlignment="1"><alignment vertical="center" wrapText="1"/></xf><xf xfId="0" fontId="1" fillId="2" borderId="0" applyFont="1" applyFill="1" applyAlignment="1"><alignment horizontal="center" vertical="center" wrapText="1"/></xf><xf xfId="0" fontId="0" fillId="3" borderId="0" applyFill="1" applyAlignment="1"><alignment vertical="center" wrapText="1"/></xf></cellXfs></styleSheet>"#.into()),
    ]);
    zip_package(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn entry(bytes: &[u8], name: &str) -> String {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut file = archive.by_name(name).unwrap();
        let mut text = String::new();
        file.read_to_string(&mut text).unwrap();
        text
    }

    fn entry_bytes(bytes: &[u8], name: &str) -> Vec<u8> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut file = archive.by_name(name).unwrap();
        let mut content = Vec::new();
        file.read_to_end(&mut content).unwrap();
        content
    }

    #[tokio::test]
    async fn creates_real_utf8_office_packages() {
        let base = OfficeCreateInput {
            path: String::new(),
            title: "中文标题".into(),
            document_style: "official_cn".into(),
            recipient: "各有关单位：".into(),
            issuer: "示例机关".into(),
            date: "2026年9月3日".into(),
            subtitle: "经营分析汇报".into(),
            paragraphs: vec!["正文内容".into()],
            sections: Vec::new(),
            slides: vec![Slide {
                title: "第一页".into(),
                subtitle: "副标题".into(),
                body: "演示内容".into(),
                bullets: vec!["要点".into()],
                image_path: String::new(),
                image_alt: String::new(),
                layout: String::new(),
                accent_color: String::new(),
            }],
            sheets: vec![Sheet {
                name: "数据".into(),
                rows: vec![vec![json!("名称"), json!(42)]],
            }],
        };
        assert!(entry(&build_docx(&base).unwrap(), "word/document.xml").contains("正文内容"));
        assert!(entry(
            &build_pptx(&base, std::path::Path::new(".")).await.unwrap(),
            "ppt/slides/slide1.xml"
        )
        .contains("第一页"));
        assert!(entry(&build_xlsx(&base).unwrap(), "xl/worksheets/sheet1.xml").contains("名称"));
    }

    #[tokio::test]
    async fn tool_writes_all_three_editable_formats_to_workspace() {
        let root = std::env::temp_dir().join(format!("omninova-office-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let tool = OfficeCreateTool::new(&root);
        let cases = [
            (
                "验证.docx",
                json!({"title":"中文 Word", "paragraphs":["正文"]}),
            ),
            (
                "验证.pptx",
                json!({"title":"中文 PPT", "slides":[{"title":"封面", "body":"内容"}]}),
            ),
            (
                "验证.xlsx",
                json!({"title":"中文 Excel", "sheets":[{"name":"数据", "rows":[["项目", 1]]}]}),
            ),
        ];
        for (path, mut args) in cases {
            args["path"] = json!(path);
            let result = tool.execute(args).await.unwrap();
            assert!(result.success, "{}", result.error.unwrap_or_default());
            let bytes = std::fs::read(root.join(path)).unwrap();
            assert!(
                bytes.starts_with(b"PK"),
                "{path} is not an OOXML ZIP package"
            );
            zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn pptx_embeds_workspace_image_with_relationship_and_crop() {
        let root =
            std::env::temp_dir().join(format!("omninova-ppt-image-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("assets")).unwrap();
        let image_path = root.join("assets/cover.png");
        image::RgbaImage::from_pixel(320, 180, image::Rgba([49, 91, 232, 255]))
            .save(&image_path)
            .unwrap();
        let source_bytes = std::fs::read(&image_path).unwrap();
        let input = OfficeCreateInput {
            path: "图文演示.pptx".into(),
            title: "图文演示".into(),
            subtitle: "真实图片嵌入".into(),
            document_style: String::new(),
            recipient: String::new(),
            issuer: String::new(),
            date: String::new(),
            paragraphs: Vec::new(),
            sections: Vec::new(),
            slides: vec![Slide {
                title: "图文封面".into(),
                subtitle: "副标题".into(),
                body: String::new(),
                bullets: Vec::new(),
                image_path: "assets/cover.png".into(),
                image_alt: "蓝色封面图".into(),
                layout: "full_bleed".into(),
                accent_color: "24C8DB".into(),
            }],
            sheets: Vec::new(),
        };
        let pptx = build_pptx(&input, &root).await.unwrap();
        assert!(entry(&pptx, "ppt/slides/slide1.xml").contains("<p:pic>"));
        assert!(entry(&pptx, "ppt/slides/slide1.xml").contains("蓝色封面图"));
        assert!(entry(&pptx, "ppt/slides/_rels/slide1.xml.rels").contains("/image"));
        assert_eq!(entry_bytes(&pptx, "ppt/media/image1.png"), source_bytes);
        std::fs::remove_dir_all(root).unwrap();
    }
}
