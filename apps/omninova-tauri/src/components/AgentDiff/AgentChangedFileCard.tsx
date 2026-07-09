import React, { memo } from "react";
import { DiffHunkView } from "./DiffHunkView";
import type { AgentChangedFile } from "./types";

interface AgentChangedFileCardProps {
  file: AgentChangedFile;
  expanded: boolean;
  onToggle: (path: string) => void;
}

function statusLabel(status: AgentChangedFile["status"]): string {
  switch (status) {
    case "active":
      return "修改中";
    case "completed":
      return "完成";
    case "failed":
      return "失败";
    case "interrupted":
      return "未完成";
    default:
      return "等待";
  }
}

function changeTypeLabel(type: AgentChangedFile["changeType"]): string {
  switch (type) {
    case "added":
      return "新增";
    case "modified":
      return "修改";
    case "deleted":
      return "删除";
    default:
      return "变更";
  }
}

function splitPath(path: string): { fileName: string; directory: string } {
  const normalized = path.replace(/\\/g, "/");
  const index = normalized.lastIndexOf("/");
  if (index < 0) return { fileName: normalized, directory: "" };
  return {
    fileName: normalized.slice(index + 1) || normalized,
    directory: normalized.slice(0, index + 1),
  };
}

export const AgentChangedFileCard: React.FC<AgentChangedFileCardProps> = memo(
  function AgentChangedFileCard({ file, expanded, onToggle }) {
    const hasHunks = file.hunks.length > 0;
    const { fileName, directory } = splitPath(file.path);

    return (
      <article className={`agent-changed-file ${file.status}`}>
        <button
          type="button"
          className="agent-changed-file-header"
          onClick={() => onToggle(file.path)}
          aria-expanded={expanded}
        >
          <span className={`agent-changed-file-status agent-changed-file-status--${file.status}`} aria-hidden />
          <span className="agent-changed-file-name-wrap" title={file.path}>
            <span className="agent-changed-file-name">{fileName}</span>
            {directory ? <span className="agent-changed-file-directory">{directory}</span> : null}
          </span>
          <span className="agent-changed-file-kind">{changeTypeLabel(file.changeType)}</span>
          <span className="agent-changed-file-state">{statusLabel(file.status)}</span>
          {file.additions > 0 ? (
            <span className="agent-diff-stat-add agent-diff-stat-bump">+{file.additions}</span>
          ) : null}
          {file.deletions > 0 ? (
            <span className="agent-diff-stat-remove agent-diff-stat-bump">-{file.deletions}</span>
          ) : null}
          <span className="agent-changed-file-toggle">{expanded ? "收起" : "展开"}</span>
        </button>

        {expanded ? (
          <div className="agent-changed-file-body">
            {hasHunks ? (
              file.hunks.map((hunk) => <DiffHunkView key={hunk.id} hunk={hunk} />)
            ) : (
              <div className="agent-diff-summary-only">
                已收到文件级变更统计，但事件没有携带 hunk 代码片段；这里只显示真实摘要。
              </div>
            )}
          </div>
        ) : null}
      </article>
    );
  }
);
