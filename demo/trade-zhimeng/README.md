# 贸易智獴 AI 演示包

本目录为 **OmniNova Claw × 贸易智獴** 现场演示用的预置数据，覆盖四项需求中的可演示部分：

1. 业务方案自动生成（模板 + 历史 + 信用 + 风险提示）
2. 合同智能审核比对（条款审核 + 文本版差异）
3. 内嵌智能问答（操作手册检索）
4. 单据识别与格式校验（发票/提单/出入库字段模板）

> **说明**：本包不含税务/银联验真接口；审批回写、财务过账为演示 JSON 结构，不会真实提交。

---

## 目录结构

```text
demo/trade-zhimeng/
├── README.md                 # 本文件
├── persona.md                # Agent 人设（复制到「Agents」系统提示）
├── knowledge/                # 导入知识库的 6 个文件
├── attachments/              # 演示时拖入对话框的附件
└── prompts/                  # 现场复制粘贴的提示词
```

---

## 快速导入

### 方式 A：Web UI（推荐）

1. 启动网关并打开 Web：`omninova gateway run` → `http://127.0.0.1:10809/app`
2. 左侧进入 **知识库**
3. 点击 **导入文件**，多选 `knowledge/` 下全部 6 个文件
4. 分类（collection）可填 `trade-zhimeng`，便于检索过滤
5. **Agents** → 将 `persona.md` 内容粘贴为系统提示词

### 方式 B：CLI 逐条导入

在仓库根目录执行（将 `{OMNINOVA}` 换为你的配置目录，默认 `~/.omninova`）：

```bash
cd demo/trade-zhimeng/knowledge

omninova knowledge add --title "方案模板-大宗商品贸易" --file ./方案模板-大宗商品贸易.md --collection trade-zhimeng
omninova knowledge add --title "历史同类业务-铜精矿" --file ./历史同类业务-铜精矿.json --collection trade-zhimeng
omninova knowledge add --title "合作方信用-摘要" --file ./合作方信用-摘要.md --collection trade-zhimeng
omninova knowledge add --title "合同审核规则" --file ./合同审核规则.md --collection trade-zhimeng
omninova knowledge add --title "操作手册-三齐与合同到期日" --file ./操作手册-三齐与合同到期日.md --collection trade-zhimeng
omninova knowledge add --title "单据字段模板-发票提单出入库" --file ./单据字段模板-发票提单出入库.md --collection trade-zhimeng

omninova knowledge search "三齐操作"
```

### 方式 C：桌面端 Tauri

知识库 → 导入文件 → 选择 `knowledge/` 目录下全部文件。

---

## 演示前检查

- [ ] 模型已配置且可用（`omninova models list`）
- [ ] **单据识别**需配置支持视觉的模型（GPT-4o / Qwen-VL / Gemini 等）
- [ ] 知识库检索「三齐操作」能命中 `操作手册-三齐与合同到期日.md`
- [ ] Agent 系统提示已粘贴 `persona.md`
- [ ] 自备一张清晰增值税发票图片，命名为 `发票样例.jpg` 放入 `attachments/`（或使用任意真实发票截图）

---

## 推荐演示顺序（约 20 分钟）

| 顺序 | 场景 | 提示词文件 | 附件 |
|---|---|---|---|
| 1 | 智能问答 | `prompts/01-智能问答.md` | 无 |
| 2 | 业务方案生成 | `prompts/02-业务方案生成.md` | 无 |
| 3 | 合同审核 + 差异 | `prompts/03-合同审核与差异.md` | `attachments/合同-客户稿.txt`、`attachments/合同-审批稿.txt` |
| 4 | 单据识别 | `prompts/04-单据识别.md` | 自备 `发票样例.jpg` |

详细口播与注意事项见各 `prompts/*.md` 文件底部「现场说明」。

---

## 对外口径（演示开场 30 秒）

> OmniNova 是智獴的 AI 执行层。今天演示方案生成、合同审核、操作问答和单据字段抽取；审批回写、税务/票据官方验真需对接贵司接口，不在现场范围。
