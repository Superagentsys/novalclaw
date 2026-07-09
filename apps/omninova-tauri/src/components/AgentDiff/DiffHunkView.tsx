import React, { memo, useMemo, useState } from "react";
import type { AgentDiffHunk, AgentDiffLine } from "./types";

interface DiffHunkViewProps {
  hunk: AgentDiffHunk;
}

function visibleLines(lines: AgentDiffLine[], expanded: boolean): AgentDiffLine[] {
  if (expanded || lines.length <= 160) return lines;
  return [...lines.slice(0, 80), ...lines.slice(-40)];
}

function lineMarker(type: AgentDiffLine["type"]): string {
  if (type === "add") return "+";
  if (type === "remove") return "-";
  return " ";
}

export const DiffHunkView: React.FC<DiffHunkViewProps> = memo(function DiffHunkView({ hunk }) {
  const [expanded, setExpanded] = useState(false);
  const hasRealLines = hunk.lines.length > 0;
  const shouldCollapse = hunk.lines.length > 160;
  const lines = useMemo(() => visibleLines(hunk.lines, expanded), [expanded, hunk.lines]);
  const hiddenCount = Math.max(0, hunk.lines.length - lines.length);
  const headCount = shouldCollapse && !expanded ? 80 : lines.length;

  return (
    <div className="diff-hunk">
      <div className="diff-hunk-header">
        <span className="diff-hunk-summary">{hunk.summary || "局部修改"}</span>
        <span className="diff-hunk-location">
          L{hunk.oldStart || hunk.newStart || 1}
        </span>
        {hunk.additions > 0 ? <span className="agent-diff-stat-add">+{hunk.additions}</span> : null}
        {hunk.deletions > 0 ? <span className="agent-diff-stat-remove">-{hunk.deletions}</span> : null}
      </div>

      {hasRealLines ? (
        <div className="diff-lines" role="table" aria-label={`${hunk.path} 的代码差异`}>
          {lines.slice(0, headCount).map((line, index) => (
            <div key={`${line.type}-${line.oldLine ?? ""}-${line.newLine ?? ""}-${index}`} className={`diff-line ${line.type}`}>
              <span className="diff-line-no">{line.oldLine ?? ""}</span>
              <span className="diff-line-no">{line.newLine ?? ""}</span>
              <span className="diff-line-marker">{lineMarker(line.type)}</span>
              <code className="diff-line-code">{line.content || " "}</code>
            </div>
          ))}
          {hiddenCount > 0 ? (
            <button
              type="button"
              className="diff-hunk-expand"
              onClick={() => setExpanded(true)}
            >
              展开中间 {hiddenCount} 行
            </button>
          ) : null}
          {shouldCollapse && !expanded
            ? lines.slice(80).map((line, index) => (
                <div key={`tail-${line.type}-${line.oldLine ?? ""}-${line.newLine ?? ""}-${index}`} className={`diff-line ${line.type}`}>
                  <span className="diff-line-no">{line.oldLine ?? ""}</span>
                  <span className="diff-line-no">{line.newLine ?? ""}</span>
                  <span className="diff-line-marker">{lineMarker(line.type)}</span>
                  <code className="diff-line-code">{line.content || " "}</code>
                </div>
              ))
            : null}
          {expanded && shouldCollapse ? (
            <button
              type="button"
              className="diff-hunk-expand"
              onClick={() => setExpanded(false)}
            >
              收起长 hunk
            </button>
          ) : null}
        </div>
      ) : (
        <div className="diff-hunk-empty">
          该 hunk 已应用，但当前事件未携带完整代码片段。
        </div>
      )}

      {hunk.textTruncated ? (
        <div className="diff-hunk-note">
          文件内容较长，仅显示前 {hunk.contentPreviewChars ?? "部分"} 字符预览。
        </div>
      ) : null}
    </div>
  );
});
