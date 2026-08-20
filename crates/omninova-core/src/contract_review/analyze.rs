use super::{resolve_contract_review_engine, ContractReviewEngineProfile};
use serde::{Deserialize, Serialize};

pub const RISK_REVIEW_DISCLAIMER: &str = "本结果用于合同风控初审辅助，不构成正式法律意见。";
const MAX_DOCUMENT_CHARS: usize = 120_000;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewMode {
    Review,
    Comparison,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContractDocument {
    pub name: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContractReviewRequest {
    pub documents: Vec<ContractDocument>,
    pub extra_instructions: String,
    pub selected_engine: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VersionChange {
    pub from_document: String,
    pub to_document: String,
    pub clause: String,
    pub change: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContractReviewReport {
    pub title: String,
    pub mode: ReviewMode,
    pub engine: ContractReviewEngineProfile,
    pub documents: Vec<String>,
    pub missing_clauses: Vec<String>,
    pub keywords: Vec<String>,
    pub version_changes: Vec<VersionChange>,
    pub disclaimer: String,
}

impl ContractReviewReport {
    pub fn to_markdown(&self) -> String {
        let missing = if self.missing_clauses.is_empty() {
            "未发现规则清单中的明显缺漏".into()
        } else {
            self.missing_clauses.join("、")
        };
        let changes = if self.version_changes.is_empty() {
            "- 单合同审查，无版本差异。".to_string()
        } else {
            self.version_changes
                .iter()
                .map(|item| {
                    format!(
                        "- {} → {}：{}（{}）",
                        item.from_document, item.to_document, item.clause, item.change
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        format!(
            "# 合同智能审核报告\n\n使用工具：合同智能审核\n\n审核引擎：{}\n\n## 1. 基本信息\n- 文件：{}\n- 模式：{}\n\n## 2. 核心交易要素\n- 由审核引擎结合原文提取。\n\n## 3. 核心条款\n- 审查范围：{}\n\n## 4. 风险发现\n- 由模型根据原文、证据与风险策略输出。\n\n## 5. 缺失条款\n- {}\n\n## 6. 冲突/歧义\n- 由模型逐项说明并引用短原文。\n\n## 7. 修改建议\n- 提供可谈判、可落地的修改建议。\n\n## 8. 关键词\n{}\n\n## 9. 风控初审结论\n- 等待模型生成结构化结论。\n\n## 版本差异\n{}\n\n> {}",
            self.engine.name,
            self.documents.join("、"),
            if self.mode == ReviewMode::Review { "单合同审查" } else { "版本比对" },
            self.engine.clauses.join("、"),
            missing,
            self.keywords.join("、"),
            changes,
            self.disclaimer,
        )
    }

    pub fn to_export_json(&self) -> serde_json::Value {
        serde_json::json!({
            "tool": "合同智能审核", "engine": self.engine.name, "mode": self.mode,
            "documents": self.documents, "missingClauses": self.missing_clauses,
            "keywords": self.keywords, "versionChanges": self.version_changes,
            "disclaimer": self.disclaimer,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ContractReviewError {
    #[error("{0}")]
    Invalid(String),
}

pub fn review_contracts(
    request: &ContractReviewRequest,
) -> Result<ContractReviewReport, ContractReviewError> {
    if request.documents.is_empty() {
        return Err(ContractReviewError::Invalid(
            "请至少上传一份合同后再开始审核".into(),
        ));
    }
    if request
        .documents
        .iter()
        .any(|item| item.text.trim().is_empty())
    {
        return Err(ContractReviewError::Invalid(
            "合同附件没有可审核的文本".into(),
        ));
    }
    let engine = resolve_contract_review_engine(&request.selected_engine)
        .ok_or_else(|| ContractReviewError::Invalid("未知合同审核引擎".into()))?;
    let missing_clauses = engine
        .clauses
        .iter()
        .filter(|clause| !request.documents[0].text.contains(clause.as_str()))
        .cloned()
        .collect();
    let keywords = [
        "付款期限",
        "违约责任",
        "争议解决",
        "价格",
        "交付",
        "验收",
        "保证金",
        "主体",
        "金额",
    ]
    .iter()
    .filter(|word| {
        request
            .documents
            .iter()
            .any(|doc| doc.text.contains(**word))
    })
    .map(|word| (*word).to_string())
    .collect::<Vec<_>>();
    let mut version_changes = Vec::new();
    for pair in request.documents.windows(2) {
        for clause in [
            "付款期限",
            "违约责任",
            "争议解决",
            "价格",
            "交付",
            "验收",
            "保证金",
            "主体",
            "金额",
        ] {
            let before = matching_line(&pair[0].text, clause);
            let after = matching_line(&pair[1].text, clause);
            if before != after {
                version_changes.push(VersionChange {
                    from_document: pair[0].name.clone(),
                    to_document: pair[1].name.clone(),
                    clause: clause.into(),
                    change: format!(
                        "{} → {}",
                        before.unwrap_or("未约定"),
                        after.unwrap_or("未约定")
                    ),
                });
            }
        }
    }
    Ok(ContractReviewReport {
        title: "合同智能审核报告".into(),
        mode: if request.documents.len() == 1 {
            ReviewMode::Review
        } else {
            ReviewMode::Comparison
        },
        engine,
        documents: request
            .documents
            .iter()
            .map(|item| item.name.clone())
            .collect(),
        missing_clauses,
        keywords,
        version_changes,
        disclaimer: RISK_REVIEW_DISCLAIMER.into(),
    })
}

fn matching_line<'a>(text: &'a str, clause: &str) -> Option<&'a str> {
    text.lines()
        .find(|line| line.contains(clause))
        .map(str::trim)
}

pub fn build_provider_request(
    request: &ContractReviewRequest,
    report: &ContractReviewReport,
) -> Result<String, ContractReviewError> {
    let mut documents = String::new();
    for document in &request.documents {
        let bounded: String = document.text.chars().take(MAX_DOCUMENT_CHARS).collect();
        documents.push_str(&format!(
            "\n<document name={:?}>\n{}\n</document>\n",
            document.name, bounded
        ));
    }
    Ok(format!(
        "你正在执行 OmniNova 系统工具『合同智能审核』。不要输出隐藏思考，只输出最终审核报告。\n审核引擎：{}\n审核重点：{}\n必查条款：{}\n风险策略：{}\n输出章节：{}\n补充要求：{}\n初步规则结果（只能作为线索，不可冒充模型结论）：\n{}\n合同原文：{}\n必须以『合同智能审核报告』为标题，并以免责声明『{}』结尾。",
        report.engine.name, report.engine.review_focus.join("、"), report.engine.clauses.join("、"),
        report.engine.risk_policy, report.engine.output_schema.join("、"), request.extra_instructions.trim(),
        report.to_markdown(), documents, RISK_REVIEW_DISCLAIMER,
    ))
}
