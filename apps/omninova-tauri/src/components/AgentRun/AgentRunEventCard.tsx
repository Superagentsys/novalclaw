import React, { memo, useMemo, useState } from "react";
import { formatCoreDurationMs, isContextStep } from "./contextLifecycle";
import { getErrorPresentation, sanitizeDisplayText } from "./executionPresentation";
import type { AgentRunStep } from "./types";
import { UiIcon } from "../UiIcon";

interface AgentRunEventCardProps {
  step: AgentRunStep;
}

function hasMojibake(text: string): boolean {
  return /�|鈥|鉁|鉂|馃|鍛|鑾|鎵|瀹|杩|澶|鏈|鏂|绋|姝/.test(text);
}

function hasLongOutput(text: string): boolean {
  return text.split(/\r?\n/).length > 20 || text.length > 1600;
}

function successTitle(step: AgentRunStep): string {
  if (step.status === "error" && !isContextStep(step)) {
    const presentation = getErrorPresentation(step.result_summary ?? "", {
      toolName: step.tool_name,
      type: "tool",
    });
    return presentation.title;
  }
  return step.title;
}

function parseFileListCount(summary?: string): string | null {
  if (!summary) return null;
  const match = summary.match(/Listing .+ \((\d+) entries\)/i);
  if (!match) return null;
  const count = Number(match[1]);
  if (!Number.isFinite(count)) return null;
  return `${count} 个文件`;
}

function StepIcon({ status }: { status: AgentRunStep["status"] }) {
  if (status === "running") return <span className="agent-run-step-spinner" aria-hidden />;
  if (status === "error") return <span className="agent-run-step-icon agent-run-step-icon--error"><UiIcon name="close" size={11} /></span>;
  if (status === "warning") return <span className="agent-run-step-icon agent-run-step-icon--warning"><UiIcon name="warning" size={11} /></span>;
  return <span className="agent-run-step-icon agent-run-step-icon--success"><UiIcon name="check" size={11} /></span>;
}

function OutputBlock({ outputs, kind = "tool" }: { outputs: string[]; kind?: "tool" | "model" }) {
  const output = sanitizeDisplayText(outputs.filter(Boolean).join("\n"));
  const shouldCollapse = kind === "model" || hasLongOutput(output) || hasMojibake(output);
  const [expanded, setExpanded] = useState(false);

  if (!output.trim()) return null;

  const preview =
    kind === "model"
      ? "模型输出已折叠，主聊天区只会显示最终回复。"
      : shouldCollapse
        ? "输出较长，已折叠。"
        : "输出已折叠。";
  const toggleLabel = kind === "model" ? "模型输出" : "输出";

  return (
    <div className="agent-run-output">
      <button
        type="button"
        className="agent-run-output-toggle"
        onClick={() => setExpanded((value) => !value)}
      >
        {expanded ? `收起${toggleLabel}` : `查看${toggleLabel}`}
      </button>
      {expanded ? (
        <pre className="agent-run-output-pre">{output}</pre>
      ) : (
        <div className="agent-run-output-preview">{preview}</div>
      )}
    </div>
  );
}

export const AgentRunEventCard: React.FC<AgentRunEventCardProps> = memo(function AgentRunEventCard({
  step,
}) {
  const contextStep = isContextStep(step);
  const errorPresentation =
    step.status === "error" && !contextStep
      ? getErrorPresentation(step.result_summary ?? "", { toolName: step.tool_name, type: "tool" })
      : null;
  const title = errorPresentation?.title ?? successTitle(step);
  const fileCount = useMemo(() => parseFileListCount(step.result_summary), [step.result_summary]);
  const fileSummary =
    step.changed_files.length > 1
      ? `${step.changed_files.length} 个文件`
      : step.changed_files[0]?.path;
  const hasDiff = step.additions > 0 || step.deletions > 0;
  const contextDetail = contextStep && step.status !== "error" ? step.result_summary : undefined;
  const durationLabel =
    contextStep && typeof step.duration_ms === "number"
      ? formatCoreDurationMs(step.duration_ms)
      : typeof step.duration_ms === "number"
        ? `${step.duration_ms}ms`
        : null;

  return (
    <article className={`agent-run-step agent-run-step--${step.status}`}>
      <div className="agent-run-step-marker">
        <StepIcon status={step.status} />
      </div>
      <div className="agent-run-step-body">
        <div className="agent-run-step-main">
          <div className="agent-run-step-title">{title}</div>
          <div className="agent-run-step-meta">
            {durationLabel && !contextDetail?.includes(durationLabel) ? (
              <span>{durationLabel}</span>
            ) : null}
            {contextDetail ? <span>{contextDetail}</span> : null}
            {fileCount ? <span>{fileCount}</span> : null}
            {fileSummary && step.tool_name !== "file_list" ? <span>{fileSummary}</span> : null}
            {hasDiff ? (
              <span className="agent-run-diff-badge">
                +{step.additions} -{step.deletions}
              </span>
            ) : null}
          </div>
        </div>
        {errorPresentation?.detail ? (
          <div className="agent-run-step-error-detail">{errorPresentation.detail}</div>
        ) : contextStep && step.status === "error" && step.result_summary ? (
          <div className="agent-run-step-error-detail">{step.result_summary}</div>
        ) : null}
        {step.changed_files.length > 1 ? (
          <div className="agent-run-files">
            {step.changed_files.slice(0, 4).map((file) => (
              <span key={file.path} className="agent-run-file-chip">
                {file.path} <span>+{file.additions}</span> <span>-{file.deletions}</span>
              </span>
            ))}
          </div>
        ) : null}
        {step.patch_hunks.length > 0 ? (
          <div className="agent-run-hunks">
            {step.patch_hunks.map((hunk, index) => (
              <div
                key={`${hunk.path}-${hunk.old_start}-${hunk.new_start}-${index}`}
                className="agent-run-hunk"
              >
                <span className="agent-run-hunk-summary">{hunk.summary}</span>
                <span className="agent-run-hunk-lines">L{hunk.old_start}</span>
                <span className="agent-run-diff-badge">
                  +{hunk.additions} -{hunk.deletions}
                </span>
              </div>
            ))}
          </div>
        ) : null}
        <OutputBlock outputs={step.outputs} kind={step.tool_name === "model" ? "model" : "tool"} />
      </div>
    </article>
  );
});
