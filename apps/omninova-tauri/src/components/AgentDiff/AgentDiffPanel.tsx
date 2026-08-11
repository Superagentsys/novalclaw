import React, { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AgentChangedFileCard } from "./AgentChangedFileCard";
import type { AgentChangedFile, AgentDiffRunState } from "./types";
import "./AgentDiffPanel.css";

interface AgentDiffPanelProps {
  diffState: AgentDiffRunState | null;
}

function terminalLabel(status: AgentDiffRunState["terminalStatus"]): string {
  switch (status) {
    case "completed":
      return "改动已完成";
    case "failed":
      return "任务失败，保留已发生的改动";
    case "cancelled":
      return "任务已取消，保留已发生的改动";
    default:
      return "实时改动";
  }
}

type DiffFilter = "all" | "modified" | "added";

function shouldShowFile(file: AgentChangedFile, filter: DiffFilter): boolean {
  if (filter === "added") return file.changeType === "added";
  if (filter === "modified") return file.changeType === "modified";
  return true;
}

export const AgentDiffPanel: React.FC<AgentDiffPanelProps> = memo(function AgentDiffPanel({
  diffState,
}) {
  const [filter, setFilter] = useState<DiffFilter>("all");
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(new Set());
  const runIdRef = useRef<string | null>(null);
  const touchedPathsRef = useRef<Set<string>>(new Set());

  const files = useMemo(
    () => diffState?.orderedPaths.map((path) => diffState.files[path]) ?? [],
    [diffState]
  );
  const visibleFiles = useMemo(
    () => files.filter((file) => shouldShowFile(file, filter)),
    [files, filter]
  );

  useEffect(() => {
    if (!diffState) return;
    let cancelled = false;

    queueMicrotask(() => {
      if (cancelled) return;

      const isNewRun = runIdRef.current !== diffState.runId;
      if (isNewRun) {
        runIdRef.current = diffState.runId;
        touchedPathsRef.current = new Set();
        setFilter("all");
      }

      const existingPaths = new Set(files.map((file) => file.path));
      const latestFile = files.reduce<AgentChangedFile | null>(
        (latest, file) => (!latest || file.lastEventAt > latest.lastEventAt ? file : latest),
        null
      );
      const defaultPath =
        files.length > 3
          ? files.find((file) => file.path === diffState.activePath && file.status === "active")?.path
          : latestFile?.path;

      setExpandedPaths((prev) => {
        const next = isNewRun
          ? new Set<string>()
          : new Set(Array.from(prev).filter((path) => existingPaths.has(path)));
        if (defaultPath && !touchedPathsRef.current.has(defaultPath)) {
          next.add(defaultPath);
        }
        return next;
      });
    });

    return () => {
      cancelled = true;
    };
  }, [diffState, files]);

  const togglePath = useCallback((path: string) => {
    touchedPathsRef.current.add(path);
    setExpandedPaths((prev) => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  }, []);

  const expandVisible = useCallback(() => {
    setExpandedPaths((prev) => {
      const next = new Set(prev);
      visibleFiles.forEach((file) => {
        touchedPathsRef.current.add(file.path);
        next.add(file.path);
      });
      return next;
    });
  }, [visibleFiles]);

  const collapseVisible = useCallback(() => {
    setExpandedPaths((prev) => {
      const next = new Set(prev);
      visibleFiles.forEach((file) => {
        touchedPathsRef.current.add(file.path);
        next.delete(file.path);
      });
      return next;
    });
  }, [visibleFiles]);

  if (!diffState || files.length === 0) return null;

  return (
    <section className={`agent-diff-panel ${files.length > 6 ? "agent-diff-panel--large" : ""}`} aria-label="Agent 代码改动">
      <div className="agent-diff-summary">
        <div className="agent-diff-summary-main">
          <span className="agent-diff-summary-kicker">Changed Files</span>
          <span className="agent-diff-summary-title">{terminalLabel(diffState.terminalStatus)}</span>
        </div>
        <div className="agent-diff-summary-stats" aria-label="改动统计">
          <span>{diffState.totals.files} 个文件</span>
          {diffState.totals.additions > 0 ? (
            <span className="agent-diff-stat-add agent-diff-stat-bump">+{diffState.totals.additions}</span>
          ) : null}
          {diffState.totals.deletions > 0 ? (
            <span className="agent-diff-stat-remove agent-diff-stat-bump">-{diffState.totals.deletions}</span>
          ) : null}
        </div>
      </div>

      {files.length > 6 ? (
        <div className="agent-diff-toolbar" aria-label="Changed Files 操作">
          <button type="button" onClick={expandVisible}>展开全部</button>
          <button type="button" onClick={collapseVisible}>折叠全部</button>
          <button
            type="button"
            className={filter === "all" ? "active" : ""}
            onClick={() => setFilter("all")}
          >
            全部
          </button>
          <button
            type="button"
            className={filter === "modified" ? "active" : ""}
            onClick={() => setFilter("modified")}
          >
            仅修改
          </button>
          <button
            type="button"
            className={filter === "added" ? "active" : ""}
            onClick={() => setFilter("added")}
          >
            仅新增
          </button>
        </div>
      ) : null}

      <div className="agent-diff-files">
        {visibleFiles.length > 0 ? visibleFiles.map((file) => (
          <AgentChangedFileCard
            key={file.path}
            file={file}
            expanded={expandedPaths.has(file.path)}
            onToggle={togglePath}
          />
        )) : (
          <div className="agent-diff-summary-only">当前过滤条件下没有文件。</div>
        )}
      </div>
    </section>
  );
});
