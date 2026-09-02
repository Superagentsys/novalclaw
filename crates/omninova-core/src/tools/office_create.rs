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
    body: String,
    #[serde(default)]
    bullets: Vec<String>,
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
        "Create a real, editable DOCX, PPTX, or XLSX file inside the workspace without Python, Node, KDocs, Office, or network access. The extension of path selects the format. Use this tool whenever the user requests a Word document, PowerPoint presentation, or Excel workbook; do not substitute HTML, Markdown, XML, CSV, or a renamed file."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Workspace-relative output path ending in .docx, .pptx, or .xlsx" },
                "title": { "type": "string" },
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
                        "body": { "type": "string" },
                        "bullets": { "type": "array", "items": { "type": "string" } }
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
            "pptx" => build_pptx(&input)?,
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

fn core_properties(title: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><dc:title>{}</dc:title><dc:creator>OmniNova</dc:creator><cp:lastModifiedBy>OmniNova</cp:lastModifiedBy></cp:coreProperties>"#,
        xml(title)
    )
}

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="OFFICE_TARGET"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties" Target="docProps/app.xml"/></Relationships>"#;

fn docx_paragraph(text: &str, style: Option<&str>) -> String {
    let style = style
        .map(|name| format!(r#"<w:pPr><w:pStyle w:val="{name}"/></w:pPr>"#))
        .unwrap_or_default();
    format!(
        r#"<w:p>{style}<w:r><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
        xml(text)
    )
}

fn build_docx(input: &OfficeCreateInput) -> anyhow::Result<Vec<u8>> {
    let mut body = String::new();
    if !input.title.is_empty() {
        body.push_str(&docx_paragraph(&input.title, Some("Title")));
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
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/></w:sectPr></w:body></w:document>"#
    );
    zip_package(vec![
        ("[Content_Types].xml".into(), r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/><Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/></Types>"#.into()),
        ("_rels/.rels".into(), ROOT_RELS.replace("OFFICE_TARGET", "word/document.xml")),
        ("docProps/core.xml".into(), core_properties(&input.title)),
        ("docProps/app.xml".into(), r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>OmniNova</Application></Properties>"#.into()),
        ("word/document.xml".into(), document),
        ("word/styles.xml".into(), r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:rPr><w:sz w:val="22"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:basedOn w:val="Normal"/><w:rPr><w:b/><w:sz w:val="40"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:rPr><w:b/><w:sz w:val="30"/></w:rPr></w:style></w:styles>"#.into()),
        ("word/_rels/document.xml.rels".into(), r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#.into()),
    ])
}

fn ppt_shape(
    id: usize,
    name: &str,
    text: &str,
    x: i64,
    y: i64,
    cx: i64,
    cy: i64,
    size: i32,
) -> String {
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{}"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/><a:ln><a:noFill/></a:ln></p:spPr><p:txBody><a:bodyPr wrap="square"/><a:lstStyle/><a:p><a:r><a:rPr lang="zh-CN" sz="{size}"/><a:t>{}</a:t></a:r><a:endParaRPr lang="zh-CN"/></a:p></p:txBody></p:sp>"#,
        xml(name),
        xml(text)
    )
}

fn build_pptx(input: &OfficeCreateInput) -> anyhow::Result<Vec<u8>> {
    let slides = if input.slides.is_empty() {
        vec![Slide {
            title: input.title.clone(),
            body: input.paragraphs.join("\n"),
            bullets: Vec::new(),
        }]
    } else {
        input.slides.clone()
    };
    if slides.len() > 100 {
        anyhow::bail!("A presentation may contain at most 100 slides");
    }
    let mut entries = Vec::new();
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
        let content = [
            slide.body.clone(),
            slide
                .bullets
                .iter()
                .map(|v| format!("• {v}"))
                .collect::<Vec<_>>()
                .join("\n"),
        ]
        .into_iter()
        .filter(|v| !v.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
        let slide_xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>{}{}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>"#,
            ppt_shape(
                2,
                "Title",
                &slide.title,
                685800,
                457200,
                10820400,
                1143000,
                2800
            ),
            ppt_shape(3, "Content", &content, 914400, 1828800, 10363200, 4114800, 1800)
        );
        entries.push((format!("ppt/slides/slide{number}.xml"), slide_xml));
        entries.push((format!("ppt/slides/_rels/slide{number}.xml.rels"), r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/></Relationships>"#.into()));
    }
    entries.extend(vec![
        ("[Content_Types].xml".into(), format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/><Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/><Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/><Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/><Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/><Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/>{overrides}</Types>"#)),
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
    zip_package(entries)
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
        let mut rows = String::new();
        for (row_index, row) in sheet.rows.iter().enumerate() {
            if row.len() > 256 {
                anyhow::bail!("A row may contain at most 256 cells");
            }
            let cells = row.iter().enumerate().map(|(column_index, value)| {
                let reference = format!("{}{}", column_name(column_index), row_index + 1);
                match value {
                    Value::Number(number) => format!(r#"<c r="{reference}"><v>{number}</v></c>"#),
                    Value::Bool(value) => format!(r#"<c r="{reference}" t="b"><v>{}</v></c>"#, if *value { 1 } else { 0 }),
                    Value::Null => format!(r#"<c r="{reference}"/>"#),
                    other => {
                        let text = other.as_str().map(str::to_owned).unwrap_or_else(|| other.to_string());
                        format!(r#"<c r="{reference}" t="inlineStr"><is><t xml:space="preserve">{}</t></is></c>"#, xml(&text))
                    }
                }
            }).collect::<String>();
            rows.push_str(&format!(r#"<row r="{}">{cells}</row>"#, row_index + 1));
        }
        entries.push((format!("xl/worksheets/sheet{number}.xml"), format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>{rows}</sheetData></worksheet>"#)));
    }
    entries.extend(vec![
        ("[Content_Types].xml".into(), format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/><Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/><Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/>{overrides}</Types>"#)),
        ("_rels/.rels".into(), ROOT_RELS.replace("OFFICE_TARGET", "xl/workbook.xml")),
        ("docProps/core.xml".into(), core_properties(&input.title)),
        ("docProps/app.xml".into(), r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>OmniNova</Application></Properties>"#.into()),
        ("xl/workbook.xml".into(), format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets>{sheet_nodes}</sheets></workbook>"#)),
        ("xl/_rels/workbook.xml.rels".into(), format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{relationships}<Relationship Id="rId{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#, sheets.len() + 1)),
        ("xl/styles.xml".into(), r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="1"><font><sz val="11"/><name val="Aptos"/></font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border/></borders><cellStyleXfs count="1"><xf/></cellStyleXfs><cellXfs count="1"><xf xfId="0"/></cellXfs></styleSheet>"#.into()),
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

    #[test]
    fn creates_real_utf8_office_packages() {
        let base = OfficeCreateInput {
            path: String::new(),
            title: "中文标题".into(),
            paragraphs: vec!["正文内容".into()],
            sections: Vec::new(),
            slides: vec![Slide {
                title: "第一页".into(),
                body: "演示内容".into(),
                bullets: vec!["要点".into()],
            }],
            sheets: vec![Sheet {
                name: "数据".into(),
                rows: vec![vec![json!("名称"), json!(42)]],
            }],
        };
        assert!(entry(&build_docx(&base).unwrap(), "word/document.xml").contains("正文内容"));
        assert!(entry(&build_pptx(&base).unwrap(), "ppt/slides/slide1.xml").contains("第一页"));
        assert!(entry(&build_xlsx(&base).unwrap(), "xl/worksheets/sheet1.xml").contains("名称"));
    }

    #[tokio::test]
    async fn tool_writes_all_three_editable_formats_to_workspace() {
        let root = std::env::temp_dir().join(format!("omninova-office-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let tool = OfficeCreateTool::new(&root);
        let cases = [
            ("验证.docx", json!({"title":"中文 Word", "paragraphs":["正文"]})),
            ("验证.pptx", json!({"title":"中文 PPT", "slides":[{"title":"封面", "body":"内容"}]})),
            ("验证.xlsx", json!({"title":"中文 Excel", "sheets":[{"name":"数据", "rows":[["项目", 1]]}]})),
        ];
        for (path, mut args) in cases {
            args["path"] = json!(path);
            let result = tool.execute(args).await.unwrap();
            assert!(result.success, "{}", result.error.unwrap_or_default());
            let bytes = std::fs::read(root.join(path)).unwrap();
            assert!(bytes.starts_with(b"PK"), "{path} is not an OOXML ZIP package");
            zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
