import { useEffect, useMemo, useState, type MouseEvent as ReactMouseEvent } from "react";
import { AgentRunTimeline } from "../AgentRun/AgentRunTimeline";
import { UiIcon, type UiIconName } from "../UiIcon";
import type { ExecutionStep } from "../../types/config";
import { invokeTauri } from "../../utils/tauri";
import { writeClipboardText } from "../../utils/clipboard";
import {
  formatDuration,
  taskNeedsAttention,
  taskStatusLabel,
  type TaskHistoryEntry,
  type TaskChangedFile,
  type TaskStatus,
} from "../../utils/taskHistory";
import { MarkdownMessage } from "./MarkdownMessage";
import { ModelPicker, type PickerProvider } from "./ModelPicker";
import "./TaskWorkspace.css";

type InspectorTab = "process" | "changes" | "logs" | "results";

interface ArtifactPreview {
  path: string;
  name: string;
  kind: "image" | "text" | "file";
  extension: string;
  size: number;
  dataUrl?: string | null;
  textPreview?: string | null;
}

const IMAGE_EXTENSIONS = new Set(["png", "jpg", "jpeg"]);
const TEXT_EXTENSIONS = new Set([
  "txt", "md", "markdown", "json", "jsonl", "yaml", "yml", "toml", "csv", "tsv",
  "log", "rs", "tsx", "ts", "jsx", "js", "css", "html", "xml", "py", "go", "java",
  "kt", "swift", "c", "h", "cpp", "hpp", "sql", "sh", "ps1",
]);

function extensionOf(path: string): string {
  const filename = path.split(/[\\/]/).pop() ?? path;
  const dot = filename.lastIndexOf(".");
  return dot > 0 ? filename.slice(dot + 1).toLowerCase() : "";
}

function artifactIcon(path: string): UiIconName {
  const extension = extensionOf(path);
  if (IMAGE_EXTENSIONS.has(extension)) return "fileImage";
  if (TEXT_EXTENSIONS.has(extension)) return "fileText";
  return "file";
}

function artifactKindLabel(path: string): string {
  const extension = extensionOf(path);
  if (IMAGE_EXTENSIONS.has(extension)) return `${extension.toUpperCase()} 图片`;
  if (TEXT_EXTENSIONS.has(extension)) return `${extension.toUpperCase() || "文本"} 文件`;
  return extension ? `${extension.toUpperCase()} 文件` : "文件";
}

function formatFileSize(size: number): string {
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
  return `${(size / 1024 / 1024).toFixed(1)} MB`;
}

function formatArtifactTime(value?: number): string {
  if (!value) return "时间未知";
  return new Date(value).toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
}

function artifactMetadata(file: TaskChangedFile): string {
  return [
    artifactKindLabel(file.path),
    file.size != null ? formatFileSize(file.size) : null,
    file.modifiedAt ? formatArtifactTime(file.modifiedAt) : null,
  ].filter(Boolean).join(" · ");
}

function changeTypeLabel(value?: TaskChangedFile["changeType"]): string {
  if (value === "created" || value === "added") return "新建";
  if (value === "deleted" || value === "removed") return "删除";
  return "修改";
}

function ArtifactThumbnail({
  file,
  workspacePath,
  eager = false,
}: {
  file: TaskChangedFile;
  workspacePath?: string;
  eager?: boolean;
}) {
  const [thumbnail, setThumbnail] = useState<string | null>(null);
  const isImage = IMAGE_EXTENSIONS.has(extensionOf(file.path));

  useEffect(() => {
    if (!isImage || !eager || file.changeType === "deleted") return;
    let cancelled = false;
    void invokeTauri<ArtifactPreview>("task_artifact_preview", {
      path: file.path,
      workspacePath,
    }).then((preview) => {
      if (!cancelled && preview.dataUrl) setThumbnail(preview.dataUrl);
    }).catch(() => {
      // A missing/deleted file keeps its stable file-type fallback.
    });
    return () => { cancelled = true; };
  }, [eager, file.changeType, file.path, isImage, workspacePath]);

  return (
    <span className={`task-artifact-thumbnail${thumbnail ? " has-image" : ""}`} aria-hidden>
      {thumbnail ? <img src={thumbnail} alt="" /> : <UiIcon name={artifactIcon(file.path)} size={16} />}
    </span>
  );
}

const STATUS_ICON: Record<TaskStatus, UiIconName> = {
  needs_approval: "safety",
  waiting_input: "message",
  running: "sync",
  completed: "check",
  failed: "warning",
  cancelled: "close",
  interrupted: "warning",
};

const STATUS_TONE: Record<TaskStatus, "attention" | "running" | "success" | "danger" | "neutral"> = {
  needs_approval: "attention",
  waiting_input: "attention",
  running: "running",
  completed: "success",
  failed: "danger",
  cancelled: "neutral",
  interrupted: "danger",
};

export function TaskStatusBar({
  task,
  elapsedSec,
  onOpenInspector,
}: {
  task: TaskHistoryEntry | null;
  elapsedSec?: number;
  onOpenInspector: (tab?: InspectorTab) => void;
}) {
  if (!task) {
    return (
      <div className="task-status-strip task-status-strip--neutral" role="status">
        <span className="task-status-strip-icon"><UiIcon name="idea" size={15} /></span>
        <span className="task-status-strip-copy">
          <strong>等待任务</strong>
          <span>输入目标后，任务状态、耗时和成果会集中显示在这里。</span>
        </span>
      </div>
    );
  }

  const elapsed = task.status === "running" && elapsedSec != null
    ? formatDuration(elapsedSec * 1000)
    : formatDuration(task.durationMs);
  const needsAttention = taskNeedsAttention(task.status);
  const actionTab: InspectorTab = task.status === "completed" ? "results" : "process";

  return (
    <div
      className={`task-status-strip task-status-strip--${STATUS_TONE[task.status]}`}
      role={needsAttention ? "alert" : "status"}
    >
      <span className="task-status-strip-icon"><UiIcon name={STATUS_ICON[task.status]} size={15} /></span>
      <span className="task-status-strip-copy">
        <strong>{taskStatusLabel(task.status)}</strong>
        <span>
          {task.attentionReason || task.resultPreview || "任务正在处理中，过程信息会持续更新。"}
        </span>
      </span>
      {elapsed ? <span className="task-status-strip-time">{elapsed}</span> : null}
      <button type="button" onClick={() => onOpenInspector(actionTab)}>
        {task.status === "needs_approval"
          ? "查看授权"
          : task.status === "waiting_input"
            ? "补充输入"
            : task.status === "completed"
              ? "查看成果"
              : "检查任务"}
      </button>
    </div>
  );
}

export function TaskDeliverable({
  task,
  onOpenInspector,
}: {
  task: TaskHistoryEntry | null;
  onOpenInspector: (tab?: InspectorTab) => void;
}) {
  if (!task) return null;
  const files = task.changedFiles ?? [];
  const isComplete = task.status === "completed";

  return (
    <section className={`task-artifact-stage task-artifact-stage--${task.status}`} aria-labelledby="task-artifact-heading">
      <header className="task-artifact-stage-head">
        <div>
          <span className="task-artifact-kicker">当前任务产物</span>
          <h2 id="task-artifact-heading">{task.title}</h2>
        </div>
        <button type="button" className="task-artifact-inspect" onClick={() => onOpenInspector("results")}>
          <UiIcon name="menuUnfold" size={15} /> 打开任务检查器
        </button>
      </header>

      <div className="task-artifact-grid">
        <article className="task-artifact-summary">
          <div className="task-artifact-section-title">
            <UiIcon name={isComplete ? "check" : STATUS_ICON[task.status]} size={15} />
            <span>{isComplete ? "任务摘要" : "当前进展"}</span>
          </div>
          {task.resultPreview ? (
            <MarkdownMessage content={task.resultPreview} workspacePath={task.workspacePath} />
          ) : (
            <p>{task.attentionReason || "任务尚未产生可展示的结果，完成后会在此集中呈现。"}</p>
          )}
        </article>

        <article className="task-artifact-files">
          <div className="task-artifact-section-title">
            <UiIcon name="file" size={15} />
            <span>文件与变更</span>
            <strong>{files.length}</strong>
          </div>
          {files.length ? (
            <ul>
              {files.slice(0, 4).map((file, index) => (
                <li key={file.path}>
                  <button
                    type="button"
                    className="task-artifact-file-link"
                    title={`${file.path} · 在任务检查器中查看`}
                    onClick={() => onOpenInspector("results")}
                  >
                    <ArtifactThumbnail file={file} workspacePath={task.workspacePath} eager={index < 4} />
                    <span>{file.path.split(/[\\/]/).pop()}</span>
                    <small>{artifactMetadata(file)}</small>
                  </button>
                  <code>+{file.additions} / -{file.deletions}</code>
                </li>
              ))}
            </ul>
          ) : (
            <p>暂未记录文件变更</p>
          )}
          {files.length > 4 ? (
            <button type="button" onClick={() => onOpenInspector("changes")}>查看全部 {files.length} 个文件</button>
          ) : null}
        </article>

        <article className="task-artifact-next">
          <div className="task-artifact-section-title"><UiIcon name="idea" size={15} /><span>下一步</span></div>
          <p>{task.nextAction || (isComplete ? "检查成果文件，确认无误后再提交或发布。" : "保持应用运行，必要时按状态条提示完成确认。")}</p>
        </article>
      </div>
    </section>
  );
}

export function TaskInspector({
  open,
  tab,
  onTabChange,
  onClose,
  task,
  liveSessionId,
  activeSteps,
  onAddArtifactToChat,
}: {
  open: boolean;
  tab: InspectorTab;
  onTabChange: (tab: InspectorTab) => void;
  onClose: () => void;
  task: TaskHistoryEntry | null;
  liveSessionId: string | null;
  activeSteps: ExecutionStep[];
  onAddArtifactToChat?: (path: string) => void;
}) {
  const tabs: Array<{ id: InspectorTab; label: string; icon: UiIconName }> = [
    { id: "process", label: "过程", icon: "sync" },
    { id: "changes", label: "文件变更", icon: "file" },
    { id: "logs", label: "日志", icon: "history" },
    { id: "results", label: "成果", icon: "check" },
  ];
  const files = useMemo(() => task?.changedFiles ?? [], [task?.changedFiles]);
  const activity = useMemo(() => task?.activity ?? [], [task?.activity]);
  const processEntries = useMemo(
    () => activity.filter((item) => item.kind || item.label),
    [activity]
  );
  const [selectedArtifactPath, setSelectedArtifactPath] = useState<string | null>(null);
  const [contextMenu, setContextMenu] = useState<{ file: TaskChangedFile; x: number; y: number } | null>(null);
  const [contextFeedback, setContextFeedback] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    queueMicrotask(() => {
      if (cancelled) return;
      setSelectedArtifactPath((current) => {
        if (current && files.some((file) => file.path === current)) {
          return current;
        }
        return files[0]?.path ?? null;
      });
    });
    return () => { cancelled = true; };
  }, [files, task?.runId]);

  useEffect(() => {
    if (!contextMenu) return;
    const close = () => setContextMenu(null);
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") close();
    };
    window.addEventListener("pointerdown", close);
    window.addEventListener("resize", close);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("pointerdown", close);
      window.removeEventListener("resize", close);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [contextMenu]);

  const selectedArtifact = files.find((file) => file.path === selectedArtifactPath) ?? null;

  const showArtifactMenu = (event: ReactMouseEvent, file: TaskChangedFile) => {
    event.preventDefault();
    event.stopPropagation();
    setSelectedArtifactPath(file.path);
    setContextFeedback(null);
    setContextMenu({
      file,
      x: Math.min(event.clientX, window.innerWidth - 190),
      y: Math.min(event.clientY, window.innerHeight - 270),
    });
  };

  const resolveArtifact = (file: TaskChangedFile) => invokeTauri<ArtifactPreview>("task_artifact_preview", {
    path: file.path,
    workspacePath: task?.workspacePath,
  });

  const runMenuAction = async (action: "preview" | "open" | "reveal" | "save" | "copy" | "attach") => {
    const file = contextMenu?.file;
    if (!file) return;
    setContextMenu(null);
    setContextFeedback(null);
    if (action === "preview") {
      onTabChange("results");
      return;
    }
    try {
      if (action === "open" || action === "reveal") {
        await invokeTauri("open_task_artifact", {
          path: file.path,
          workspacePath: task?.workspacePath,
          reveal: action === "reveal",
        });
        return;
      }
      const resolved = await resolveArtifact(file);
      if (action === "copy") {
        await writeClipboardText(resolved.path);
        setContextFeedback("文件路径已复制");
      } else if (action === "attach") {
        onAddArtifactToChat?.(resolved.path);
        setContextFeedback("已重新加入对话附件");
      } else {
        const { save } = await import("@tauri-apps/plugin-dialog");
        const destination = await save({ title: "成果文件另存为", defaultPath: resolved.name });
        if (!destination) return;
        await invokeTauri("save_task_artifact_as", {
          path: file.path,
          workspacePath: task?.workspacePath,
          destination,
        });
        setContextFeedback("文件已另存");
      }
    } catch (reason) {
      setContextFeedback(`操作失败：${String(reason)}`);
    }
  };

  const openProcessArtifact = (path: string) => {
    const normalized = path.replace(/\\/g, "/");
    const file = files.find((candidate) => candidate.path.replace(/\\/g, "/") === normalized);
    if (file) {
      setSelectedArtifactPath(file.path);
      onTabChange("results");
      return;
    }
    void invokeTauri<void>("open_task_artifact", {
      path,
      workspacePath: task?.workspacePath,
      reveal: true,
    }).catch(() => {
      // 过程中的临时路径可能已不存在；保留文字记录即可。
    });
  };

  return (
    <aside className={`task-inspector${open ? " is-open" : ""}`} aria-hidden={!open} aria-label="任务检查器">
      <header className="task-inspector-head">
        <div>
          <span>任务检查器</span>
          <strong>{task?.title || "暂无任务"}</strong>
        </div>
        <button type="button" onClick={onClose} aria-label="关闭任务检查器"><UiIcon name="close" size={15} /></button>
      </header>
      <div className="task-inspector-tabs" role="tablist" aria-label="检查器分区">
        {tabs.map((item) => (
          <button
            key={item.id}
            type="button"
            role="tab"
            aria-selected={tab === item.id}
            className={tab === item.id ? "is-active" : ""}
            onClick={() => onTabChange(item.id)}
          >
            <UiIcon name={item.icon} size={14} />
            <span>{item.label}</span>
          </button>
        ))}
      </div>
      <div className="task-inspector-body">
        <div className="task-inspector-tabpane" hidden={tab !== "process"}>
          {liveSessionId ? (
            <AgentRunTimeline events={[]} isRunning defaultCollapsed={false} liveSessionId={liveSessionId} />
          ) : processEntries.length ? (
            <ol className="task-inspector-process-list">
              {processEntries.map((item, index) => (
                <li
                  key={`${item.at}-${index}`}
                  data-status={item.status || (item.tone === "error" ? "failed" : item.tone === "success" ? "completed" : "waiting")}
                >
                  <span className="task-inspector-process-icon">
                    <UiIcon
                      name={item.kind === "file" ? "file" : item.kind === "approval" ? "safety" : item.kind === "model" ? "agent" : item.kind === "tool" ? "tool" : "sync"}
                      size={13}
                    />
                  </span>
                  <span>
                    <strong>{item.label}</strong>
                    {item.detail && item.detail !== item.label ? <small>{item.detail}</small> : null}
                    {item.path ? (
                      <button
                        type="button"
                        className="task-inspector-process-path"
                        onClick={() => openProcessArtifact(item.path!)}
                        title="查看成果；若尚未收录则在文件资源管理器中定位"
                      >
                        <UiIcon name="folder" size={11} />
                        <span>{item.path}</span>
                      </button>
                    ) : null}
                  </span>
                  <time>{new Date(item.at).toLocaleTimeString("zh-CN", { hour12: false })}</time>
                </li>
              ))}
            </ol>
          ) : activeSteps.length ? (
            <ol className="task-inspector-step-list">
              {activeSteps.map((step, index) => (
                <li key={`${step.title}-${index}`} data-status={step.status ?? "pending"}>
                  <span>{step.title}</span><small>{step.detail || step.status || "记录"}</small>
                </li>
              ))}
            </ol>
          ) : (
            <InspectorEmpty icon="sync" title="暂无执行过程" detail="任务启动后，工具调用与步骤会出现在这里。" />
          )}
        </div>

        {tab === "changes" ? (
          files.length ? (
            <>
              <ul className="task-inspector-file-list">
                {files.map((file, index) => (
                  <li key={file.path} className={selectedArtifactPath === file.path ? "is-selected" : ""}>
                    <button type="button" onClick={() => setSelectedArtifactPath(file.path)} onContextMenu={(event) => showArtifactMenu(event, file)}>
                      <ArtifactThumbnail file={file} workspacePath={task?.workspacePath} eager={index < 12} />
                      <span>
                        <strong title={file.path}>{file.path}</strong>
                        <small>{artifactMetadata(file)} · {changeTypeLabel(file.changeType)}</small>
                      </span>
                      <code>+{file.additions} -{file.deletions}</code>
                    </button>
                  </li>
                ))}
              </ul>
              {selectedArtifact ? <ArtifactPreviewPane file={selectedArtifact} workspacePath={task?.workspacePath} onAddToChat={onAddArtifactToChat} /> : null}
            </>
          ) : <InspectorEmpty icon="file" title="暂无文件变更" detail="只有后端确认的真实变更才会写入此处。" />
        ) : null}

        {tab === "logs" ? (
          activity.length ? (
            <ol className="task-inspector-log-list">
              {activity.map((item, index) => (
                <li key={`${item.at}-${index}`} data-tone={item.tone || "info"}>
                  <time>{new Date(item.at).toLocaleTimeString("zh-CN", { hour12: false })}</time>
                  <span>{item.label}</span>
                </li>
              ))}
            </ol>
          ) : <InspectorEmpty icon="history" title="暂无安全日志" detail="这里只保留步骤摘要，不保存命令原始输出或敏感内容。" />
        ) : null}

        {tab === "results" ? (
          task?.resultPreview || files.length ? (
            <div className="task-inspector-results-stack">
              {task?.resultPreview ? (
                <div className="task-inspector-result"><MarkdownMessage content={task.resultPreview} workspacePath={task.workspacePath} /></div>
              ) : null}
              {files.length ? (
                <section className="task-inspector-result-files" aria-label="成果文件">
                  <header><strong>成果文件</strong><span>{files.length}</span></header>
                  <div className="task-inspector-artifact-grid">
                    {files.map((file, index) => (
                      <button
                        type="button"
                        key={file.path}
                        className={selectedArtifactPath === file.path ? "is-selected" : ""}
                        onClick={() => setSelectedArtifactPath(file.path)}
                        onContextMenu={(event) => showArtifactMenu(event, file)}
                        title={file.path}
                      >
                        <ArtifactThumbnail file={file} workspacePath={task?.workspacePath} eager={index < 12} />
                        <span>{file.path.split(/[\\/]/).pop()}</span>
                        <small>{artifactMetadata(file)}</small>
                      </button>
                    ))}
                  </div>
                </section>
              ) : null}
              {selectedArtifact ? <ArtifactPreviewPane file={selectedArtifact} workspacePath={task?.workspacePath} onAddToChat={onAddArtifactToChat} /> : null}
            </div>
          ) : <InspectorEmpty icon="check" title="暂无成果" detail="任务完成后的摘要与成果文件会集中显示在这里。" />
        ) : null}
      </div>
      {contextFeedback ? <div className="task-inspector-toast" role="status">{contextFeedback}</div> : null}
      {contextMenu ? (
        <div
          className="task-artifact-context-menu"
          role="menu"
          style={{ left: contextMenu.x, top: contextMenu.y }}
          onPointerDown={(event) => event.stopPropagation()}
        >
          <button type="button" role="menuitem" onClick={() => void runMenuAction("preview")}>预览内容</button>
          <button type="button" role="menuitem" onClick={() => void runMenuAction("open")} disabled={contextMenu.file.changeType === "deleted"}>打开文件</button>
          <button type="button" role="menuitem" onClick={() => void runMenuAction("reveal")} disabled={contextMenu.file.changeType === "deleted"}>在文件夹中定位</button>
          <button type="button" role="menuitem" onClick={() => void runMenuAction("save")} disabled={contextMenu.file.changeType === "deleted"}>另存为…</button>
          <button type="button" role="menuitem" onClick={() => void runMenuAction("copy")} disabled={contextMenu.file.changeType === "deleted"}>复制完整路径</button>
          <button type="button" role="menuitem" onClick={() => void runMenuAction("attach")} disabled={contextMenu.file.changeType === "deleted" || !onAddArtifactToChat}>重新加入对话</button>
        </div>
      ) : null}
    </aside>
  );
}

function ArtifactPreviewPane({
  file,
  workspacePath,
  onAddToChat,
}: {
  file: TaskChangedFile;
  workspacePath?: string;
  onAddToChat?: (path: string) => void;
}) {
  const [preview, setPreview] = useState<ArtifactPreview | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [actionFeedback, setActionFeedback] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    queueMicrotask(() => {
      if (cancelled) return;
      setPreview(null);
      setError(null);
      setActionError(null);
      setActionFeedback(null);
      if (file.changeType === "deleted") {
        setError("该文件已删除，只保留变更记录，无法生成预览。");
        return;
      }
      setLoading(true);
      void invokeTauri<ArtifactPreview>("task_artifact_preview", {
        path: file.path,
        workspacePath,
      }).then((value) => {
        if (!cancelled) setPreview(value);
      }).catch((reason) => {
        if (!cancelled) setError(String(reason));
      }).finally(() => {
        if (!cancelled) setLoading(false);
      });
    });
    return () => { cancelled = true; };
  }, [file.changeType, file.path, workspacePath]);

  const openArtifact = async (reveal: boolean) => {
    setActionError(null);
    try {
      await invokeTauri<void>("open_task_artifact", {
        path: file.path,
        workspacePath,
        reveal,
      });
    } catch (reason) {
      setActionError(String(reason));
    }
  };

  const copyPath = async () => {
    setActionError(null);
    try {
      await writeClipboardText(preview?.path || file.path);
      setActionFeedback("路径已复制");
    } catch (reason) {
      setActionError(String(reason));
    }
  };

  const saveAs = async () => {
    setActionError(null);
    try {
      const resolved = preview ?? await invokeTauri<ArtifactPreview>("task_artifact_preview", {
        path: file.path,
        workspacePath,
      });
      const { save } = await import("@tauri-apps/plugin-dialog");
      const destination = await save({ title: "成果文件另存为", defaultPath: resolved.name });
      if (!destination) return;
      await invokeTauri("save_task_artifact_as", { path: file.path, workspacePath, destination });
      setActionFeedback("已另存到所选位置");
    } catch (reason) {
      setActionError(String(reason));
    }
  };

  const addToChat = () => {
    onAddToChat?.(preview?.path || file.path);
    setActionFeedback("已加入输入区附件");
  };

  return (
    <section className="task-artifact-preview" aria-label="文件预览">
      <header>
        <span className="task-artifact-preview-icon"><UiIcon name={artifactIcon(file.path)} size={15} /></span>
        <span>
          <strong title={file.path}>{file.path.split(/[\\/]/).pop()}</strong>
          <small>{preview ? `${artifactKindLabel(file.path)} · ${formatFileSize(preview.size)} · ${formatArtifactTime(file.modifiedAt)}` : artifactMetadata(file)}</small>
        </span>
        <span className="task-artifact-preview-actions">
          <button type="button" onClick={() => void openArtifact(false)} disabled={file.changeType === "deleted"}>打开</button>
          <button type="button" onClick={() => void openArtifact(true)} disabled={file.changeType === "deleted"}>定位</button>
          <button type="button" onClick={() => void saveAs()} disabled={file.changeType === "deleted"}>另存为</button>
          <button type="button" onClick={() => void copyPath()} disabled={file.changeType === "deleted"}>复制路径</button>
          {onAddToChat ? <button type="button" onClick={addToChat} disabled={file.changeType === "deleted"}>加入对话</button> : null}
        </span>
      </header>
      <button
        type="button"
        className="task-artifact-path-link"
        onClick={() => void openArtifact(true)}
        disabled={file.changeType === "deleted"}
        title="在文件资源管理器中定位"
      >
        {preview?.path || file.path}
      </button>
      {loading ? <div className="task-artifact-preview-state">正在读取安全预览…</div> : null}
      {error ? <div className="task-artifact-preview-state is-error">{error}</div> : null}
      {actionError ? <div className="task-artifact-preview-state is-error">{actionError}</div> : null}
      {actionFeedback ? <div className="task-artifact-preview-state is-success" role="status">{actionFeedback}</div> : null}
      {preview?.dataUrl ? <img className="task-artifact-preview-image" src={preview.dataUrl} alt={`${preview.name} 缩略图`} /> : null}
      {preview?.textPreview ? <pre className="task-artifact-preview-text">{preview.textPreview}</pre> : null}
      {preview && !preview.dataUrl && !preview.textPreview ? (
        <div className="task-artifact-preview-state">该文件类型不提供内嵌预览，可使用“打开”或“定位”。</div>
      ) : null}
    </section>
  );
}

function InspectorEmpty({ icon, title, detail }: { icon: UiIconName; title: string; detail: string }) {
  return (
    <div className="task-inspector-empty">
      <UiIcon name={icon} size={22} />
      <strong>{title}</strong>
      <span>{detail}</span>
    </div>
  );
}

export function TaskOnboarding({
  modelReady,
  workspaceReady,
  gatewayReady,
  selectedModel,
  providers,
  onModelChange,
  maxMode,
  onMaxModeChange,
  defaultProvider,
  defaultModel,
  onConfigureModel,
  onChooseWorkspace,
  onStartGateway,
  gatewayBusy,
}: {
  modelReady: boolean;
  workspaceReady: boolean;
  gatewayReady: boolean;
  selectedModel: string;
  providers: PickerProvider[];
  onModelChange: (value: string) => void;
  maxMode: boolean;
  onMaxModeChange: (value: boolean) => void;
  defaultProvider?: string;
  defaultModel?: string;
  onConfigureModel: () => void;
  onChooseWorkspace: () => void;
  onStartGateway: () => void;
  gatewayBusy: boolean;
}) {
  const allReady = modelReady && workspaceReady && gatewayReady;
  if (allReady) return null;
  const steps = [modelReady, workspaceReady, gatewayReady];
  const currentStep = Math.max(0, steps.findIndex((ready) => !ready));

  return (
    <section className="task-onboarding" aria-labelledby="task-onboarding-title">
      <div className="task-onboarding-copy">
        <span className="task-artifact-kicker">首次使用 · 3 步准备</span>
        <h2 id="task-onboarding-title">完成基础配置后进入任务中心</h2>
        <p>这些设置只用于本机任务执行，可随时在顶部工具栏中更改。</p>
      </div>
      <ol className="task-onboarding-rail" aria-label="配置进度">
        {["选择模型", "选择 Workspace", "启动网关"].map((label, index) => (
          <li key={label} className={steps[index] ? "is-done" : index === currentStep ? "is-current" : ""}>
            <span>{steps[index] ? <UiIcon name="check" size={13} /> : index + 1}</span>
            <strong>{label}</strong>
          </li>
        ))}
      </ol>
      <div className="task-onboarding-action">
        {currentStep === 0 ? (
          providers.length ? (
            <div className="task-onboarding-model">
              <span>本次任务优先模型</span>
              <ModelPicker
                variant="inline"
                value={selectedModel}
                onChange={onModelChange}
                providers={providers}
                defaultProvider={defaultProvider}
                defaultModel={defaultModel}
                maxMode={maxMode}
                onMaxModeChange={onMaxModeChange}
                onConfigureCustom={onConfigureModel}
              />
            </div>
          ) : (
            <button type="button" onClick={onConfigureModel}><UiIcon name="settings" size={15} /> 配置模型</button>
          )
        ) : currentStep === 1 ? (
          <button type="button" onClick={onChooseWorkspace}><UiIcon name="folder" size={15} /> 选择 Workspace</button>
        ) : (
          <button type="button" onClick={onStartGateway} disabled={gatewayBusy}>
            <UiIcon name={gatewayBusy ? "sync" : "api"} size={15} /> {gatewayBusy ? "正在启动…" : "启动网关"}
          </button>
        )}
      </div>
    </section>
  );
}

export type { InspectorTab };
