import { useMemo, useState } from "react";
import { UiIcon } from "../UiIcon";
import {
  contractRiskCounts,
  parseContractReviewSections,
  toContractReviewExportView,
  type ContractReviewAttachmentView,
  type ContractReviewEngineCard,
  type ContractReviewStage,
} from "./contractReviewModel";
import "./ContractReviewPanel.css";

interface Props {
  attachments: ContractReviewAttachmentView[];
  engines: ContractReviewEngineCard[];
  selectedEngine: string;
  extraInstructions: string;
  stage: ContractReviewStage;
  documentsPrepared: boolean;
  elapsedSec: number;
  error?: string;
  onChooseFiles: () => void;
  onDropFiles: (files: File[]) => void;
  onRemoveAttachment: (id: string) => void;
  onEngineChange: (id: string) => void;
  onInstructionsChange: (value: string) => void;
  onStart: () => void;
}

interface ReportProps {
  content: string;
  engineName?: string;
  exportData?: unknown;
  onExport?: () => void;
}

const BUSY_STAGES = new Set<ContractReviewStage>(["preparing", "reviewing", "generating"]);

function extensionLabel(name: string): string {
  const extension = name.split(".").pop()?.trim().toUpperCase();
  return extension && extension !== name.toUpperCase() ? extension : "文件";
}

function documentLabel(index: number): string {
  return String.fromCharCode("A".charCodeAt(0) + Math.min(index, 25));
}

function modeDescription(count: number): string {
  if (count === 0) return "添加合同后自动识别审核模式";
  if (count === 1) return "单合同风险审查";
  return `${count} 个版本 · ${Array.from({ length: count }, (_, index) => documentLabel(index)).join(" → ")}`;
}

function fileStatus(stage: ContractReviewStage, documentsPrepared: boolean): string {
  if (stage === "preparing") return "解析中";
  if (stage === "failed") return documentsPrepared ? "已解析" : "解析失败";
  if (["reviewing", "generating", "completed"].includes(stage)) return "已解析";
  return "待解析";
}

function primaryActionLabel(stage: ContractReviewStage, fileCount: number): string {
  if (stage === "preparing") return "正在解析合同...";
  if (stage === "reviewing") return "正在审核...";
  if (stage === "generating") return "正在生成报告...";
  if (stage === "completed") return fileCount > 1 ? "重新比对" : "重新审核";
  return fileCount > 1 ? "开始版本比对" : "开始审核";
}

function collapseStatusLabel(stage: ContractReviewStage, busy: boolean): string {
  if (busy) return "审核中";
  if (stage === "completed") return "已完成";
  if (stage === "failed") return "审核失败";
  if (stage === "cancelled") return "已取消";
  return "待审核";
}

function ProgressIcon({ state }: { state: "done" | "active" | "waiting" | "skipped" }) {
  if (state === "done") return <UiIcon name="check" size={12} />;
  if (state === "active") return <UiIcon name="sync" size={12} className="contract-review-progress-spin" />;
  return <span className={`contract-review-progress-dot is-${state}`} aria-hidden />;
}

function ReviewProgress({ stage, comparison }: { stage: ContractReviewStage; comparison: boolean }) {
  if (stage === "idle") return null;
  const parsed = ["reviewing", "generating", "completed"].includes(stage);
  const analysisDone = ["generating", "completed"].includes(stage);
  const complete = stage === "completed";
  const failed = stage === "failed" || stage === "cancelled";
  const steps: Array<{ label: string; state: "done" | "active" | "waiting" | "skipped" }> = [
    { label: "文档解析", state: parsed ? "done" : stage === "preparing" ? "active" : "waiting" },
    { label: "交易要素识别", state: parsed ? "done" : "waiting" },
    { label: "条款风险分析", state: analysisDone ? "done" : stage === "reviewing" ? "active" : "waiting" },
    {
      label: comparison ? "版本差异分析" : "版本差异分析（单合同无需）",
      state: comparison ? (analysisDone ? "done" : stage === "reviewing" ? "active" : "waiting") : "skipped",
    },
    { label: "报告生成", state: complete ? "done" : stage === "generating" ? "active" : "waiting" },
  ];

  return (
    <section className={`contract-review-progress${failed ? " is-failed" : ""}`} aria-label="审核进度">
      <div className="contract-review-section-heading">
        <span className="contract-review-step-number">4</span>
        <div>
          <h3>审核进度</h3>
          <p>{failed ? "流程已停止，可检查提示后重试" : complete ? "审核完成，报告已生成" : "正在按审核流程处理"}</p>
        </div>
      </div>
      <ol>
        {steps.map((step) => (
          <li key={step.label} className={`is-${step.state}`}>
            <span className="contract-review-progress-icon"><ProgressIcon state={step.state} /></span>
            <span>{step.label}</span>
          </li>
        ))}
      </ol>
    </section>
  );
}

export function ContractReviewPanel(props: Props) {
  const [collapsed, setCollapsed] = useState(false);
  const [dragActive, setDragActive] = useState(false);
  const busy = BUSY_STAGES.has(props.stage);
  const comparison = props.attachments.length > 1;
  const status = fileStatus(props.stage, props.documentsPrepared);
  const collapseStatus = collapseStatusLabel(props.stage, busy);

  return (
    <section
      className={`contract-review-panel${collapsed ? " is-collapsed" : ""}`}
      aria-label="合同智能审核面板"
    >
      <header className="contract-review-header">
        <div>
          <div className="contract-review-title-row">
            <UiIcon name="safety" size={18} />
            <h2>合同智能审核</h2>
            {collapsed ? <span className="contract-review-collapse-status">{collapseStatus}</span> : null}
            <span className="contract-review-badge">系统工具</span>
          </div>
          {collapsed ? null : <p>关键条款审查 · 风险识别 · 缺漏检查 · 版本比对</p>}
        </div>
        <button
          type="button"
          className="contract-review-collapse"
          onClick={() => setCollapsed((value) => !value)}
          aria-expanded={!collapsed}
          aria-label={collapsed ? "展开合同智能审核" : "收起合同智能审核"}
          title={collapsed ? "展开合同智能审核" : "收起合同智能审核"}
        >
          <UiIcon name={collapsed ? "chevronDown" : "chevronUp"} size={13} />
          {collapsed ? "展开" : "收起"}
        </button>
      </header>

      {collapsed ? null : (
        <>
      <div className="contract-review-setup-grid">
        <section className="contract-review-section contract-review-files-section">
          <div className="contract-review-section-heading">
            <span className="contract-review-step-number">1</span>
            <div><h3>合同文件</h3><p>支持最多 3 个连续版本</p></div>
          </div>
          <button
            type="button"
            className={`contract-review-dropzone${dragActive ? " is-dragging" : ""}`}
            onClick={props.onChooseFiles}
            onDragEnter={(event) => { event.preventDefault(); setDragActive(true); }}
            onDragOver={(event) => { event.preventDefault(); event.dataTransfer.dropEffect = "copy"; }}
            onDragLeave={(event) => {
              if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDragActive(false);
            }}
            onDrop={(event) => {
              event.preventDefault();
              setDragActive(false);
              props.onDropFiles(Array.from(event.dataTransfer.files));
            }}
            disabled={busy}
          >
            <span className="contract-review-dropzone-icon"><UiIcon name="fileText" size={20} /></span>
            <strong>拖入合同，或点击选择文件</strong>
            <span>DOCX · PDF · TXT · MD</span>
          </button>

          {props.attachments.length ? (
            <ul className="contract-review-file-list" aria-label="合同文件列表">
              {props.attachments.map((attachment, index) => (
                <li key={attachment.id}>
                  <span className="contract-review-file-icon"><UiIcon name="fileText" size={16} /></span>
                  <span className="contract-review-file-copy">
                    <strong><span className="contract-review-version-label">{documentLabel(index)}</span>{attachment.name}</strong>
                    <small>{extensionLabel(attachment.name)}{attachment.note ? ` · ${attachment.note}` : ""}</small>
                  </span>
                  <span className={`contract-review-file-status is-${status}`}>{status}</span>
                  <button
                    type="button"
                    className="contract-review-file-remove"
                    onClick={() => props.onRemoveAttachment(attachment.id)}
                    disabled={busy}
                    aria-label={`移除合同 ${attachment.name}`}
                  >
                    <UiIcon name="close" size={13} />
                  </button>
                </li>
              ))}
            </ul>
          ) : null}
        </section>

        <section className="contract-review-section contract-review-mode-section">
          <div className="contract-review-section-heading">
            <span className="contract-review-step-number">2</span>
            <div><h3>审核模式</h3><p>根据文件数量自动识别</p></div>
          </div>
          <div className="contract-review-mode-summary">
            <UiIcon name={comparison ? "history" : "fileText"} size={17} />
            <div>
              <strong>{props.attachments.length > 1 ? "版本比对" : "单合同审核"}</strong>
              <span>{modeDescription(props.attachments.length)}</span>
            </div>
          </div>
        </section>
      </div>

      <section className="contract-review-section contract-review-engine-section">
        <div className="contract-review-section-heading">
          <span className="contract-review-step-number">3</span>
          <div><h3>审核引擎</h3><p>选择与当前合同场景匹配的审查策略</p></div>
        </div>
        <div className="contract-review-engines" role="radiogroup" aria-label="审核引擎">
          {props.engines.map((engine) => {
            const selected = props.selectedEngine === engine.id;
            return (
              <button
                key={engine.id}
                type="button"
                role="radio"
                aria-checked={selected}
                className={selected ? "is-selected" : ""}
                onClick={() => props.onEngineChange(engine.id)}
                disabled={busy}
              >
                <span className="contract-review-radio" aria-hidden>{selected ? <UiIcon name="check" size={10} /> : null}</span>
                <span className="contract-review-engine-copy">
                  <strong>{engine.name}</strong>
                  <small>{engine.description}</small>
                </span>
                {engine.recommended ? <span className="contract-review-recommended">推荐</span> : null}
              </button>
            );
          })}
        </div>
        <label className="contract-review-instructions">
          <span>补充审核要求 <small>可选</small></span>
          <textarea
            rows={2}
            value={props.extraInstructions}
            onChange={(event) => props.onInstructionsChange(event.target.value)}
            placeholder="例如：重点检查付款期限、违约责任和争议解决"
            disabled={busy}
          />
        </label>
      </section>

      <ReviewProgress stage={props.stage} comparison={comparison} />

      {props.error ? (
        <div className="contract-review-error" role="alert">
          <UiIcon name="warning" size={16} />
          <div><strong>审核失败</strong><span>{props.error}</span></div>
        </div>
      ) : null}

      <div className="contract-review-actions">
        <button
          type="button"
          className="contract-review-primary-action"
          onClick={props.onStart}
          disabled={!props.attachments.length || busy}
        >
          {busy ? <UiIcon name="sync" size={14} className="contract-review-progress-spin" /> : <UiIcon name="safety" size={14} />}
          {primaryActionLabel(props.stage, props.attachments.length)}
          {busy ? <span className="contract-review-elapsed">{props.elapsedSec}s</span> : null}
        </button>
        <p>审核结果用于风控初审辅助，不构成正式法律意见。</p>
        </div>
        </>
      )}
    </section>
  );
}

function renderSectionContent(title: string, content: string) {
  const lines = content.split("\n").map((line) => line.trim()).filter(Boolean);
  return (
    <div className="contract-review-report-copy">
      {lines.map((line, index) => {
        const quote = line.startsWith(">");
        const bullet = /^[-*]\s+/.test(line);
        const clean = line.replace(/^>\s?/, "").replace(/^[-*]\s+/, "").replace(/\*\*/g, "");
        const riskLevel = title === "风险发现"
          ? clean.includes("高风险")
            ? "high"
            : clean.includes("中风险")
              ? "medium"
              : /低风险|提示/.test(clean)
                ? "notice"
                : null
          : null;
        if (riskLevel) {
          const label = riskLevel === "high" ? "高风险" : riskLevel === "medium" ? "中风险" : "提示";
          return (
            <article key={index} className={`contract-review-risk-item is-${riskLevel}`}>
              <span>{label}</span>
              <p>{clean}</p>
            </article>
          );
        }
        return bullet ? <div key={index} className="contract-review-report-bullet"><span />{clean}</div>
          : quote ? <p key={index} className="contract-review-report-note">{clean}</p>
          : <p key={index}>{clean}</p>;
      })}
    </div>
  );
}

export function ContractReviewReport({ content, engineName, exportData, onExport }: ReportProps) {
  const [copied, setCopied] = useState(false);
  const sections = useMemo(() => parseContractReviewSections(content), [content]);
  const riskCounts = useMemo(() => contractRiskCounts(content), [content]);
  const report = useMemo(() => toContractReviewExportView(exportData), [exportData]);
  const hasRiskSummary = riskCounts.high + riskCounts.medium + riskCounts.notice > 0;
  const resolvedEngine = engineName || report?.engine || "合同审核引擎";

  const copyReport = async () => {
    try {
      await navigator.clipboard.writeText(content);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1800);
    } catch {
      setCopied(false);
    }
  };

  return (
    <article className="contract-review-report" aria-label="合同智能审核报告">
      <header className="contract-review-report-header">
        <div>
          <span className="contract-review-report-eyebrow">合同智能审核</span>
          <h2>合同智能审核报告</h2>
          <div className="contract-review-report-meta">
            <span>使用工具：合同智能审核</span>
            <span>审核引擎：{resolvedEngine}</span>
            {report?.documents.length ? <span>文件：{report.documents.length}</span> : null}
            {report?.mode ? <span>模式：{report.mode === "comparison" ? "版本比对" : "单合同审核"}</span> : null}
          </div>
        </div>
        <div className="contract-review-report-actions">
          {onExport ? <button type="button" onClick={onExport}><UiIcon name="code" size={13} />导出 JSON</button> : null}
          <button type="button" onClick={() => void copyReport()}><UiIcon name={copied ? "check" : "fileText"} size={13} />{copied ? "已复制" : "复制报告"}</button>
        </div>
      </header>

      {hasRiskSummary ? (
        <section className="contract-review-risk-overview" aria-label="风险概览">
          <div className="is-high"><span>高风险</span><strong>{riskCounts.high}</strong></div>
          <div className="is-medium"><span>中风险</span><strong>{riskCounts.medium}</strong></div>
          <div className="is-notice"><span>提示</span><strong>{riskCounts.notice}</strong></div>
        </section>
      ) : null}

      {report?.documents.length && report.mode === "comparison" ? (
        <section className="contract-review-version-flow" aria-label="版本顺序">
          {report.documents.map((document, index) => (
            <div key={`${document}-${index}`}>
              <span>{documentLabel(index)}</span><strong>{document}</strong>
              {index < report.documents.length - 1 ? <UiIcon name="history" size={13} /> : null}
            </div>
          ))}
        </section>
      ) : null}

      <div className="contract-review-report-sections">
        {sections.map((section) => (
          <details key={section.id} open={section.defaultOpen}>
            <summary><span>{section.title}</span><UiIcon name="plus" size={13} /></summary>
            {renderSectionContent(section.title, section.content)}
          </details>
        ))}
      </div>

      {report?.versionChanges.length ? (
        <section className="contract-review-version-diff" aria-label="版本差异">
          <h3>版本差异</h3>
          <div className="contract-review-version-diff-list">
            {report.versionChanges.map((change, index) => (
              <article key={`${change.fromDocument}-${change.toDocument}-${change.clause}-${index}`}>
                <div><span>{change.fromDocument}</span><UiIcon name="history" size={12} /><span>{change.toDocument}</span></div>
                <strong>{change.clause}</strong>
                <p>{change.change}</p>
              </article>
            ))}
          </div>
        </section>
      ) : null}

      {report?.disclaimer ? <footer>{report.disclaimer}</footer> : null}
    </article>
  );
}
