import type { ContractReviewEngineCard } from "./contractReviewModel";
import "./ContractReviewPanel.css";

interface Props {
  attachmentCount: number;
  engines: ContractReviewEngineCard[];
  selectedEngine: string;
  extraInstructions: string;
  starting: boolean;
  exportReady: boolean;
  onEngineChange: (id: string) => void;
  onInstructionsChange: (value: string) => void;
  onStart: () => void;
  onExport: () => void;
}

export function ContractReviewPanel(props: Props) {
  const mode = props.attachmentCount === 0
    ? "等待合同附件"
    : props.attachmentCount === 1
      ? "单合同审查"
      : `${props.attachmentCount} 份合同 · 版本比对`;
  return (
    <section className="contract-review-panel" aria-label="合同智能审核面板">
      <header>
        <div><strong>合同智能审核</strong><span className="contract-review-badge">系统工具</span></div>
        <p>关键条款审查、风险识别、缺漏检查和版本比对</p>
      </header>
      <div className="contract-review-mode">{mode}</div>
      <p className="contract-review-hint">支持 DOCX、文字层 PDF、TXT、MD；扫描版 PDF 暂不支持 OCR。</p>
      <div className="contract-review-engines" role="radiogroup" aria-label="审核引擎">
        {props.engines.map((engine) => (
          <button
            key={engine.id}
            type="button"
            role="radio"
            aria-checked={props.selectedEngine === engine.id}
            className={props.selectedEngine === engine.id ? "is-selected" : ""}
            onClick={() => props.onEngineChange(engine.id)}
          >
            <strong>{engine.name}</strong>
            <span>{engine.description}</span>
          </button>
        ))}
      </div>
      <label className="contract-review-instructions">
        <span>补充审核要求（可选）</span>
        <textarea
          rows={2}
          value={props.extraInstructions}
          onChange={(event) => props.onInstructionsChange(event.target.value)}
          placeholder="例如：重点检查付款期限、违约责任和争议解决"
        />
      </label>
      <div className="contract-review-actions">
        <button type="button" onClick={props.onStart} disabled={!props.attachmentCount || props.starting}>
          {props.starting ? "正在准备审核…" : props.attachmentCount >= 2 ? "开始版本比对" : "开始审核"}
        </button>
        {props.exportReady ? <button type="button" onClick={props.onExport}>导出 JSON</button> : null}
      </div>
    </section>
  );
}
