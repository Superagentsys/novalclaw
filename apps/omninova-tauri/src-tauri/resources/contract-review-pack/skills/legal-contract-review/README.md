# 法务合同辅助审查

| 项目 | 信息 |
|---|---|
| Skill | `legal-contract-review` |
| 版本 | `1.0.2` |
| 作者 | `Lucky-pl` |
| 版权 | Copyright © 2026 Lucky-pl |

## 简介

面向合同、协议、标书、NDA、采购、服务和合作文件的辅助初筛，支持文本/元数据提取、完整性检查、条款风险分析、版本对比、评分、报告和 Docx 标注/修订。

执行规范以 [SKILL.md](SKILL.md) 为唯一来源。

## 快速使用

```text
请使用 $legal-contract-review 从我方立场审查这份采购合同，说明法域、版本、重大风险和待法务确认事项。
```

默认使用标准审查；高金额、跨境、战略合作或强监管场景使用深度审查。评分只用于排序与沟通，不能单独决定是否签署，也不得表述为“无风险”。

## 关键资源

- `references/legal_playbook.md`：合同类型、行业与条款审查规则。
- `references/workflows-and-operations.md`：单份、批量、对比、标注和修订命令。
- `references/troubleshooting.md`：扫描件、乱码、依赖、评分和报告问题。
- `scripts/extract_contract_text.py`：PDF/Docx 文本提取。
- `scripts/check_completeness.py`：必备条款检查。
- `scripts/calculate_risk_score.py`：辅助风险评分。

## 输出与安全

- 原合同永不覆盖；标注、修订和报告使用新文件。
- 产物写入用户任务目录下的独立结果目录，不写入 Skill 安装目录。
- 现行法律或监管结论必须核验官方来源并注明法域与日期。
- 正式签署、高风险、跨境或争议事项必须由执业律师或企业法务复核。

## 环境

基础处理需要 Python 3.10+ 及 `pdfplumber`、`PyPDF2`、`python-docx`、`lxml`；扫描件 OCR 还需要 Tesseract、`pytesseract` 和 `pdf2image`。

本 Skill 的输出为 AI 辅助审阅，不构成正式法律意见。
