//! 渗透评估「全过程」与报告模板（与 `skills/penetration-assessment/` 对齐），供 CLI / API 返回非空结构化内容。

use crate::config::Config;

const WORKFLOW_PHASES_JSON: &str =
    include_str!("../../../../skills/penetration-assessment/workflow_phases.json");
const REPORT_TEMPLATE_MD: &str =
    include_str!("../../../../skills/penetration-assessment/report_template.md");

fn workflow_phases_value() -> serde_json::Value {
    serde_json::from_str(WORKFLOW_PHASES_JSON).unwrap_or_else(|_| {
        serde_json::json!([{ "error": "workflow_phases.json parse failed" }])
    })
}

/// 供 `GET /api/doctor` 的 `penetration_assessment` 字段使用。
pub fn build_playbook_payload() -> serde_json::Value {
    serde_json::json!({
        "version": "1.0",
        "workflow_phases": workflow_phases_value(),
        "report_template_markdown": REPORT_TEMPLATE_MD,
        "skill_path": "skills/penetration-assessment/",
    })
}

/// `omninova security audit` — 配置校验 + 安全开关快照 + 渗透全流程与报告模板。
pub fn build_audit_report(config: &Config) -> serde_json::Value {
    let validation = config.validate();
    serde_json::json!({
        "kind": "security_audit",
        "config_ok": validation.is_ok(),
        "config_errors": validation.errors,
        "config_warnings": validation.warnings,
        "security": {
            "otp_enabled": config.security.otp.enabled,
            "estop_enabled": config.security.estop.enabled,
            "audit_log_enabled": config.security.audit.enabled,
            "sandbox_enabled": config.security.sandbox.enabled,
            "syscall_anomaly_enabled": config.security.syscall_anomaly.enabled,
        },
        "autonomy": {
            "level": config.autonomy.level,
            "workspace_only": config.autonomy.workspace_only,
            "block_high_risk_commands": config.autonomy.block_high_risk_commands,
        },
        "penetration_assessment_workflow": workflow_phases_value(),
        "report_template_markdown": REPORT_TEMPLATE_MD,
        "note": "本报告模板用于授权范围内的安全评估文档化；不构成对任何未授权系统的测试许可。"
    })
}

/// `omninova security status` — 轻量状态 + 完整 playbook（与 audit 相比减少重复时可调用）。
pub fn build_status_report(config: &Config) -> serde_json::Value {
    serde_json::json!({
        "kind": "security_status",
        "security": {
            "otp_enabled": config.security.otp.enabled,
            "estop_enabled": config.security.estop.enabled,
            "audit_log_enabled": config.security.audit.enabled,
            "sandbox_enabled": config.security.sandbox.enabled,
        },
        "penetration_assessment_workflow": workflow_phases_value(),
        "report_template_markdown": REPORT_TEMPLATE_MD,
    })
}
