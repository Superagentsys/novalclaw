---
name: penetration-assessment
description: 授权范围内的渗透测试流程、阶段产出物与报告模板；用于安全评估、红队演练文档化与复测跟踪。
---

# 渗透评估（Penetration Assessment）

当用户询问「渗透测试全过程」「渗透报告怎么写」「安全评估步骤」时，**不得**只回复空内容或一句「未实现」。应按下述阶段组织回答，并引导使用仓库内模板文件。

## 必须包含的内容

1. **合法前提**：仅讨论**已书面授权**范围内的测试；明确禁止对未授权目标扫描或利用。
2. **六阶段流程**：与 `workflow_phases.json` 一致——授权与范围 → 侦察 → 威胁建模与攻击面 → 漏洞验证 → 横向/提权（授权内）→ 报告与复测。
3. **可交付物**：每阶段列出「目标 / 产出文档」。
4. **报告结构**：按 `report_template.md` 的章节输出骨架；漏洞条目使用表格字段（严重程度、复现、修复建议）。

## 文件引用（本仓库）

- 阶段定义（JSON）：`skills/penetration-assessment/workflow_phases.json`
- 报告模板（Markdown）：`skills/penetration-assessment/report_template.md`

## CLI / 网关

- 运行 `omninova security audit` 或 `omninova security status` 可拉取与本技能对齐的**结构化 JSON**（含上述流程与报告正文模板）。
- 网关 `GET /api/doctor` 的 JSON 中含 `penetration_assessment` 字段，便于桌面端展示。

## 语气

专业、可审计、可追溯；避免夸大 CVSS；对不确定项标注「需进一步验证」。
