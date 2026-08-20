use super::*;
use std::io::Write;
use std::path::PathBuf;

fn request(texts: &[&str]) -> ContractReviewRequest {
    ContractReviewRequest {
        documents: texts
            .iter()
            .enumerate()
            .map(|(index, text)| ContractDocument {
                name: format!("v{}.txt", index + 1),
                text: (*text).into(),
            })
            .collect(),
        extra_instructions: String::new(),
        selected_engine: DEFAULT_CONTRACT_REVIEW_ENGINE.into(),
    }
}

#[test]
fn one_contract_produces_review_and_json_metadata() {
    let report = review_contracts(&request(&[
        "主体：甲方\n付款期限：30日\n违约责任：逾期支付",
    ]))
    .unwrap();
    assert_eq!(report.mode, ReviewMode::Review);
    assert!(report.to_markdown().contains("使用工具：合同智能审核"));
    assert_eq!(report.to_export_json()["tool"], "合同智能审核");
}

#[test]
fn two_and_three_contracts_produce_chained_diffs() {
    let two = review_contracts(&request(&["付款期限：30日", "付款期限：60日"])).unwrap();
    assert!(two
        .version_changes
        .iter()
        .any(|item| item.clause == "付款期限"));
    let three = review_contracts(&request(&[
        "付款期限：30日",
        "付款期限：60日",
        "付款期限：90日",
    ]))
    .unwrap();
    assert!(three
        .version_changes
        .iter()
        .any(|item| item.from_document == "v2.txt" && item.to_document == "v3.txt"));
}

#[test]
fn engine_profiles_are_bounded_and_not_skill_documents() {
    for engine in contract_review_engines() {
        let json = serde_json::to_string(&engine).unwrap();
        assert!(json.len() < 4_000);
        assert!(!json.contains("SKILL.md"));
    }
}

#[test]
fn provider_request_contains_engine_and_report_contract() {
    let request = request(&["主体：甲方\n金额：100元"]);
    let report = review_contracts(&request).unwrap();
    let prompt = build_provider_request(&request, &report).unwrap();
    assert!(prompt.contains("合同智能审核报告"));
    assert!(prompt.contains(RISK_REVIEW_DISCLAIMER));
}

fn temp_file(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "omninova-contract-review-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

#[test]
fn docx_extraction_reads_document_body() {
    let path = temp_file("contract.docx");
    let xml = r#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>付款期限三十日 违约责任</w:t></w:r></w:p></w:body></w:document>"#;
    let file = std::fs::File::create(&path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    zip.start_file(
        "word/document.xml",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    zip.write_all(xml.as_bytes()).unwrap();
    zip.finish().unwrap();
    let extracted = extract_document_text(&path).unwrap();
    assert!(extracted.text.contains("付款期限三十日"));
}

#[test]
fn empty_or_scanned_pdf_is_rejected_without_base64_fallback() {
    let empty = temp_file("empty.txt");
    std::fs::write(&empty, "").unwrap();
    assert!(extract_document_text(&empty)
        .unwrap_err()
        .to_string()
        .contains("空文件"));

    let scan = temp_file("scan.pdf");
    std::fs::write(&scan, b"%PDF-1.4\n%%EOF\n").unwrap();
    let message = extract_document_text(&scan).unwrap_err().to_string();
    assert!(message.contains("PDF") || message.contains("文字层") || message.contains("OCR"));
}

fn minimal_text_pdf(text: &str) -> Vec<u8> {
    let escaped = text
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)");
    let stream = format!("BT /F1 12 Tf 72 720 Td ({escaped}) Tj ET");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Count 1 /Kids [3 0 R] >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
        format!("<< /Length {} >>\nstream\n{}\nendstream", stream.len(), stream),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut body = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(body.len());
        body.push_str(&format!("{} 0 obj\n{}\nendobj\n", index + 1, object));
    }
    let xref = body.len();
    body.push_str(&format!(
        "xref\n0 {}\n0000000000 65535 f \n",
        objects.len() + 1
    ));
    for offset in offsets {
        body.push_str(&format!("{offset:010} 00000 n \n"));
    }
    body.push_str(&format!(
        "trailer << /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    ));
    body.into_bytes()
}

#[test]
fn text_layer_pdf_extracts_readable_content() {
    let path = temp_file("contract.pdf");
    std::fs::write(
        &path,
        minimal_text_pdf("Payment term 30 days and penalty clause"),
    )
    .unwrap();
    let extracted = extract_document_text(&path).unwrap();
    assert!(extracted.text.contains("Payment term 30 days"));
}
