# 工作流与脚本操作

## 目录

1. 环境与输入
2. 单份合同
3. 批量、对比与修订
4. 报告分段与目录
5. 评分参数
6. 历史记录

## 1. 环境与输入

Python 3.10+ 为可选增强环境。基础依赖：`pdfplumber`、`PyPDF2`、`python-docx`、`lxml`；扫描件还需 `pytesseract`、`pdf2image`、Tesseract 引擎和中文语言包。
如果环境没有 Python/pip，不要安装依赖：文本提取可退回桌面端解析，DOCX 修订可使用内置 Rust 引擎（见第 3 节）。

审查前记录：全文或指定条款、初审/复审/终审、紧急程度、是否公司模板、合同类型、金额、合作对象等级、审查方与法域。

默认标准审查。NDA 且金额较低可建议快速审查；金额超过 1000 万、战略合作、跨境或强监管场景建议深度审查。阈值只是路由提示，用户制度优先。

## 2. 单份合同

```bash
python scripts/extract_contract_text.py "<合同文件>"
python scripts/extract_metadata.py "<合同文件>"
python scripts/check_completeness.py "<合同文件>"
```

核验提取结果后，按 `legal_playbook.md` 审查并统计风险项，再计算分数：

```bash
python scripts/calculate_risk_score.py --high-risk <N> --medium-risk <N> --low-risk <N> --contract-type <type> --contract-amount <amount> --partner-level <level>
```

不要让脚本分数覆盖条款的实质判断。一个不可接受的高风险条款可以直接触发“不建议签署/修改后再审”。

## 3. 批量、对比与修订

批量提取：

```bash
python scripts/batch_extract_contracts.py <文件1> <文件2> -o extracted.json --summary -w 4
```

版本对比：

```bash
python scripts/compare_contracts.py <V1> <V2> --json-output diff.json --html-output diff.html --summary
```

Docx 标注优先使用兼容性较好的简化脚本：

```bash
python scripts/annotate_contract_simple.py "<合同>.docx" "<合同>_已标注.docx" annotations.json
```

需要完整批注能力时可改用 `annotate_contract.py`。PDF 不能调用 Docx 标注或修订脚本；只有 PDF 时在报告中提供定位和建议文本。

Docx 修订：

```bash
python scripts/modify_contract.py "<合同>.docx" "<合同>_已修改.docx" replacements.json --track-changes
```

无 Python 环境（桌面端已内置原生 DOCX 引擎，无需 Python/pip）时，在任务工作目录根写入 `docx_modification_request.json`，桌面端会在任务结束后自动调用内置引擎生成 DOCX。格式：

```json
{
  "input": "采购合同.docx",
  "output": "采购合同_修订版.docx",
  "trackChanges": false,
  "replacements": [
    { "originalText": "原条款文本", "newText": "替换后的条款文本" }
  ]
}
```

`input`/`output` 使用任务工作目录相对路径或绝对路径；`output` 已存在时引擎会自动追加 `_2`、`_3` 序号，不会覆盖。写入请求后不要声称“修改完成”，应说明“已提交 DOCX 生成请求，等待桌面端生成确认”。

修改前逐项让用户选择确认修改、提供新文本或跳过。不得自动接受会改变价款、责任、范围、权利或期限的文本。

## 4. 报告分段与目录

长报告按以下章节分别生成 Markdown，再合并：管理层摘要、基本信息、风险评分、高风险、中风险、低风险、修改建议、审批建议。文件用两位数字排序，单段建议不超过 2000 字。

```bash
python scripts/merge_report_segments.py "<segments目录>" -o "<输出.docx>" -t "<报告标题>"
```

建议目录：

```text
<任务工作目录>/<合同名>_<日期>/
├── contract_text.txt
├── metadata.json
├── completeness.json
├── risk_score.json
├── annotations.json
├── segments/
├── 风险评估报告_<合同名>_<日期>.docx
├── <合同名>_已标注.docx
└── <合同名>_已修改.docx
```

保留必要中间文件以便追溯，但不要在 Skill 自身目录创建运行结果。

## 5. 评分参数

现有脚本按合同类型、金额、合作对象和行业调整风险分数。历史默认示例包括：NDA 0.8、服务 1.0、采购 0.9、合作 1.1；高金额、金融/医疗或基础合作对象提高风险权重。

分数区间仅作沟通参考：低分意味着需要更多处置，高分只表示规则范围内未发现明显问题，不等于公平、有效或可以直接签署。

## 6. 历史记录

```bash
python scripts/review_history.py query --days 365
python scripts/review_history.py query --contract-type procurement
python scripts/review_history.py query --partner "<合作方>"
python scripts/review_history.py trend --days 90
```

历史目录可通过 `CONTRACT_REVIEW_HISTORY_DIR` 配置。查询历史时遵守合同保密和最小权限原则。
