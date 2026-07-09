import React, { memo, useMemo, useState } from "react";
import type { AgentRunStep } from "./types";
import { getToolLabel } from "./types";

interface AgentRunEventCardProps {
  step: AgentRunStep;
}

function hasMojibake(text: string): boolean {
  return /�|鈥|鉁|鉂|馃|鍛|鑾|鎵|瀹|杩|澶|鏈|鏂|绋|姝/.test(text);
}

function hasLongOutput(text: string): boolean {
  return text.split(/\r?\n/).length > 20 || text.length > 1600;
}

function isCommandTool(toolName: string): boolean {
  return ["shell", "bash", "run_command", "Command", "git_operations", "git"].includes(toolName);
}

function friendlyError(toolName: string, raw: string, title: string): string {
  const lower = raw.toLowerCase();

  if (
    (toolName === "git_operations" || toolName === "git") &&
    (lower.includes("not a git repository") ||
      lower.includes("git diff exited with status 129") ||
      (title.toLowerCase().includes("diff") && lower.includes("exited with status 129")))
  ) {
    return "当前 Workspace 不是 Git 仓库，无法执行 Git diff。";
  }

  if (
    (toolName === "git_operations" || toolName === "git") &&
    (lower.includes("git status exited with status 128") ||
      (title.toLowerCase().includes("status") && lower.includes("exited with status 128")))
  ) {
    return "当前 Workspace 不是 Git 仓库，无法读取 Git 状态。";
  }

  if (lower.includes("absolute paths are not allowed")) {
    return "当前安全策略不允许访问 Workspace 外的绝对路径。";
  }

  if (lower.includes("tool blocked by security policy")) {
    return "工具被安全策略拦截。";
  }

  if (isCommandTool(toolName)) {
    const status = raw.match(/(?:status|退出码|exit code)[:： ]+(-?\d+)/i)?.[1];
    if (status) return `命令执行失败，退出码：${status}`;
  }

  return raw || `${getToolLabel(toolName)}执行失败`;
}

function successTitle(step: AgentRunStep): string {
  if (step.status === "running") return step.title;
  if (step.status === "error") {
    const message = friendlyError(step.tool_name, step.result_summary ?? "", step.title);
    if (step.tool_name === "git_operations" || step.tool_name === "git") {
      return `Git diff 失败：${message}`;
    }
    return `${getToolLabel(step.tool_name)}失败：${message}`;
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
  if (status === "error") return <span className="agent-run-step-icon agent-run-step-icon--error" aria-hidden>×</span>;
  if (status === "warning") return <span className="agent-run-step-icon agent-run-step-icon--warning" aria-hidden>!</span>;
  return <span className="agent-run-step-icon agent-run-step-icon--success" aria-hidden>✓</span>;
}

function OutputBlock({ outputs, kind = "tool" }: { outputs: string[]; kind?: "tool" | "model" }) {
  const output = outputs.filter(Boolean).join("\n");
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
  const title = successTitle(step);
  const fileCount = useMemo(() => parseFileListCount(step.result_summary), [step.result_summary]);
  const fileSummary =
    step.changed_files.length > 1
      ? `${step.changed_files.length} 个文件`
      : step.changed_files[0]?.path;
  const hasDiff = step.additions > 0 || step.deletions > 0;

  return (
    <article className={`agent-run-step agent-run-step--${step.status}`}>
      <div className="agent-run-step-marker">
        <StepIcon status={step.status} />
      </div>
      <div className="agent-run-step-body">
        <div className="agent-run-step-main">
          <div className="agent-run-step-title">{title}</div>
          <div className="agent-run-step-meta">
            {typeof step.duration_ms === "number" ? (
              <span>{step.duration_ms}ms</span>
            ) : null}
            {fileCount ? <span>{fileCount}</span> : null}
            {fileSummary && step.tool_name !== "file_list" ? <span>{fileSummary}</span> : null}
            {hasDiff ? (
              <span className="agent-run-diff-badge">
                +{step.additions} -{step.deletions}
              </span>
            ) : null}
          </div>
        </div>
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
