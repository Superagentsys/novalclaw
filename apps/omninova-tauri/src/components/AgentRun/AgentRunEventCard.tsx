import React, { memo, useState } from "react";
import type { AgentRunEvent, RunEvent } from "./types";
import { getToolLabel } from "./types";

type DisplayEvent = RunEvent | AgentRunEvent | Record<string, unknown>;

interface AgentRunEventCardProps {
  event: DisplayEvent;
}

const StatusIcon = memo(function StatusIcon({ status }: { status: string }) {
  switch (status) {
    case "running":
      return (
        <span
          className="inline-block w-3.5 h-3.5 rounded-full border-2 border-blue-400 border-t-transparent animate-spin flex-shrink-0"
          aria-hidden
        />
      );
    case "success":
      return <span aria-hidden>✓</span>;
    case "error":
      return <span aria-hidden>✕</span>;
    default:
      return <span className="text-gray-400" aria-hidden>?</span>;
  }
});

function payloadOf(event: DisplayEvent): Record<string, unknown> {
  return event as Record<string, unknown>;
}

function eventType(event: DisplayEvent): string {
  const type = payloadOf(event).type;
  return typeof type === "string" ? type : "unknown";
}

function textField(event: DisplayEvent, key: string): string {
  const value = payloadOf(event)[key];
  return typeof value === "string" ? value : "";
}

function numberField(event: DisplayEvent, key: string): number {
  const value = payloadOf(event)[key];
  return typeof value === "number" ? value : 0;
}

function booleanField(event: DisplayEvent, key: string, fallback: boolean): boolean {
  const value = payloadOf(event)[key];
  return typeof value === "boolean" ? value : fallback;
}

function diffStats(event: DisplayEvent): { additions: number; deletions: number } | null {
  const value = payloadOf(event).diff_stats;
  if (!value || typeof value !== "object") return null;
  const diff = value as { additions?: number; deletions?: number };
  return {
    additions: diff.additions ?? 0,
    deletions: diff.deletions ?? 0,
  };
}

function isCommandTool(toolName: string): boolean {
  return ["shell", "bash", "run_command", "Command", "git_operations", "git"].includes(toolName);
}

function hasMojibake(text: string): boolean {
  return /�|鈥|鉁|鉂|馃|鍛|鑾|鎵|瀹|杩|澶|鏈|鏂|绋|姝/.test(text);
}

function friendlySummary(toolName: string, success: boolean, raw: string): string {
  if (success) return raw;

  const lower = raw.toLowerCase();
  if (
    (toolName === "git_operations" || toolName === "git") &&
    (lower.includes("not a git repository") ||
      lower.includes("git diff exited with status 129") ||
      lower.includes("exited with status 129"))
  ) {
    return "当前 Workspace 不是 Git 仓库，无法执行 Git diff。";
  }

  if (isCommandTool(toolName)) {
    const status = raw.match(/(?:status|退出码|exit code)[:： ]+(-?\d+)/i)?.[1];
    if (status) return `命令执行失败，退出码：${status}`;
  }

  return raw || `${getToolLabel(toolName)}执行失败`;
}

function CardShell({
  event,
  status,
  children,
}: {
  event: DisplayEvent;
  status: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-start gap-2 py-1.5 px-3 rounded-md bg-white/5 hover:bg-white/8 transition-colors text-xs group">
      <StatusIcon status={status} />
      <div className="flex-1 min-w-0" title={textField(event, "tool_call_id") || undefined}>
        {children}
      </div>
    </div>
  );
}

function CommandOutputBlock({ output }: { output: string }) {
  const shouldCollapse = output.length > 1200 || hasMojibake(output);
  const [expanded, setExpanded] = useState(!shouldCollapse);
  const preview = output.length > 600 ? `${output.slice(0, 600)}\n… 输出已折叠` : output;

  if (!output.trim()) return null;

  return (
    <div className="mt-0.5">
      <button
        type="button"
        className="text-blue-300/70 hover:text-blue-200"
        onClick={() => setExpanded((value) => !value)}
      >
        {expanded ? "收起命令输出" : "展开命令输出"}
      </button>
      {expanded ? (
        <pre className="text-white/50 max-w-full mt-0.5 whitespace-pre-wrap break-words max-h-48 overflow-auto" title={output}>
          {output}
        </pre>
      ) : (
        <pre className="text-white/40 max-w-full mt-0.5 whitespace-pre-wrap break-words max-h-20 overflow-hidden" title={output}>
          {preview}
        </pre>
      )}
    </div>
  );
}

export const AgentRunEventCard: React.FC<AgentRunEventCardProps> = memo(function AgentRunEventCard({
  event,
}) {
  const type = eventType(event);

  if (type === "run_started") {
    return (
      <CardShell event={event} status="running">
        <span className="text-blue-300/80">Agent 启动：{textField(event, "agent_name")}</span>
      </CardShell>
    );
  }

  if (type === "tool_started" || type === "toolStarted") {
    const toolName = textField(event, "tool_name");
    const summary = textField(event, "summary") || `正在执行工具：${toolName}`;
    return (
      <CardShell event={event} status="running">
        <span className="text-blue-300/80">{summary}</span>
      </CardShell>
    );
  }

  if (type === "command_output" || type === "commandOutput") {
    return (
      <CardShell event={event} status="running">
        <span className="text-blue-300/80">命令输出</span>
        <CommandOutputBlock output={textField(event, "output")} />
      </CardShell>
    );
  }

  if (type === "tool_completed" || type === "toolCompleted") {
    const success = booleanField(event, "success", true);
    const diff = diffStats(event);
    const toolName = textField(event, "tool_name");
    const resultSummary = friendlySummary(toolName, success, textField(event, "result_summary"));
    return (
      <CardShell event={event} status={success ? "success" : "error"}>
        <div className="space-y-0.5">
          <div className="flex items-center gap-2 flex-wrap">
            <span className={success ? "text-green-300" : "text-red-300"}>
              {success ? "✓" : "✕"} <span className="text-white/60">{getToolLabel(toolName)}</span>
            </span>
            <span className="text-white/30">·</span>
            <span className="text-white/40">{numberField(event, "duration_ms")}ms</span>
            {diff && (
              <>
                <span className="text-white/30">·</span>
                <span className="text-green-400/80">+{diff.additions}</span>
                <span className="text-red-400/80">-{diff.deletions}</span>
              </>
            )}
          </div>
          {resultSummary.trim() && (
            <p className="text-white/45 truncate max-w-xs" title={textField(event, "result_summary")}>
              {resultSummary}
            </p>
          )}
        </div>
      </CardShell>
    );
  }

  if (type === "file_changed" || type === "fileChanged") {
    return (
      <CardShell event={event} status="success">
        <div className="flex items-center gap-2">
          <span aria-hidden>📄</span>
          <span className="text-white/70 truncate" title={textField(event, "path")}>
            {textField(event, "path")}
          </span>
          <span className="text-green-400/80">+{numberField(event, "additions")}</span>
          <span className="text-red-400/80">-{numberField(event, "deletions")}</span>
        </div>
      </CardShell>
    );
  }

  if (type === "run_completed") {
    const success = booleanField(event, "success", true);
    const preview = textField(event, "reply_preview");
    return (
      <CardShell event={event} status={success ? "success" : "error"}>
        <span className={success ? "text-green-300" : "text-red-300"}>
          {success ? "Agent 执行完成" : "Agent 执行失败"}
        </span>
        {preview && (
          <p className="text-white/40 truncate max-w-xs mt-0.5" title={preview}>
            {preview}
          </p>
        )}
      </CardShell>
    );
  }

  if (type === "error") {
    return (
      <CardShell event={event} status="error">
        <span className="text-red-300">错误：{textField(event, "message")}</span>
      </CardShell>
    );
  }

  return (
    <div className="flex items-start gap-2 py-1.5 px-3 rounded-md bg-yellow-500/10 text-yellow-300 text-xs">
      <span aria-hidden>⚠️</span>
      <div className="flex-1 min-w-0">
        <span>未知事件：{type}</span>
        <pre className="text-yellow-200/70 text-xs mt-0.5 overflow-auto">
          {JSON.stringify(event, null, 2)}
        </pre>
      </div>
    </div>
  );
});
