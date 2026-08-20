use serde::{Deserialize, Serialize};

pub const DEFAULT_CONTRACT_REVIEW_ENGINE: &str = "omninova-contract-risk";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContractReviewEngineProfile {
    pub id: String,
    pub name: String,
    pub description: String,
    pub review_focus: Vec<String>,
    pub clauses: Vec<String>,
    pub risk_policy: String,
    pub output_schema: Vec<String>,
    pub recommended: bool,
}

fn profile(
    id: &str,
    name: &str,
    description: &str,
    focus: &[&str],
    risk_policy: &str,
    recommended: bool,
) -> ContractReviewEngineProfile {
    ContractReviewEngineProfile {
        id: id.into(),
        name: name.into(),
        description: description.into(),
        review_focus: focus.iter().map(|item| (*item).into()).collect(),
        clauses: [
            "主体与授权",
            "标的与金额",
            "付款期限",
            "交付与验收",
            "保证金",
            "违约责任",
            "解除与终止",
            "保密与知识产权",
            "争议解决",
            "生效与签章",
        ]
        .iter()
        .map(|item| (*item).into())
        .collect(),
        risk_policy: risk_policy.into(),
        output_schema: [
            "基本信息",
            "核心交易要素",
            "核心条款",
            "风险发现",
            "缺失条款",
            "冲突/歧义",
            "修改建议",
            "关键词",
            "风控初审结论",
        ]
        .iter()
        .map(|item| (*item).into())
        .collect(),
        recommended,
    }
}

pub fn contract_review_engines() -> Vec<ContractReviewEngineProfile> {
    vec![
        profile(
            DEFAULT_CONTRACT_REVIEW_ENGINE,
            "OmniNova 合同风险审查",
            "通用合同关键条款、缺漏与版本风险初审",
            &["交易闭环", "权利义务对等", "可执行性", "版本差异"],
            "按事实和合同原文分级；证据不足时标记待人工确认，不作法律结论。",
            true,
        ),
        profile(
            "ai-contract-risk-officer",
            "Ai Contract Risk Officer",
            "偏重商业风险、履约风险与可量化责任边界",
            &["付款安全", "履约保障", "责任上限", "退出机制"],
            "优先识别高损失概率与高影响条款，并给出可直接谈判的修改建议。",
            false,
        ),
        profile(
            "baichen-legal",
            "Baichen Legal",
            "偏重中国商事合同完整性、合规性与争议处理",
            &["主体资格", "条款完备", "合规风险", "争议解决"],
            "区分缺失、歧义、冲突和不利约定；不虚构法律条文。",
            false,
        ),
        profile(
            "legal-contract-review",
            "Legal Contract Review",
            "偏重逐条审阅、语言清晰度与版本变更影响",
            &["逐条审阅", "定义一致", "交叉引用", "版本比对"],
            "对每项结论引用短原文；无法从文本确认时明确说明。",
            false,
        ),
    ]
}

pub fn resolve_contract_review_engine(id: &str) -> Option<ContractReviewEngineProfile> {
    let requested = if id.trim().is_empty() {
        DEFAULT_CONTRACT_REVIEW_ENGINE
    } else {
        id.trim()
    };
    contract_review_engines()
        .into_iter()
        .find(|engine| engine.id == requested)
}
