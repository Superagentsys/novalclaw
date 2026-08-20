import { useRef, useEffect, useState, useCallback, useMemo } from "react";
import { MarkdownMessage } from "./MarkdownMessage";
import { invokeTauri } from "../../utils/tauri";
import { writeClipboardText } from "../../utils/clipboard";
import { listenAgentRunEvents } from "../../utils/events";
import { open } from "@tauri-apps/plugin-dialog";
import {
  isTauriEnvironment,
  pickComposerAttachmentPaths,
  prepareComposerAttachments,
} from "../../utils/composerAttachments";
import {
  areStoredMessagesEqual,
  fetchSessionHistory,
  fetchWebSessionsFromGateway,
  formatTime,
  loadChatStorage,
  mergeAvatarSessions,
  mergeLocalMessageMetadata,
  saveChatStorage,
  type StoredAvatarSession,
  type StoredChatMessage,
} from "../../utils/chatStorage";
import type {
  AgentRunEvent,
} from "../AgentRun/types";
import {
  addTask,
  formatDuration,
  formatTaskTime,
  loadTaskHistory,
  patchTask,
  saveTaskHistory,
  taskNeedsAttention,
  taskStatusLabel,
  type TaskActivityEntry,
  type TaskHistoryEntry,
  type TaskStatus,
  type TaskChangedFile,
} from "../../utils/taskHistory";
import type {
  AgentPersonaConfig,
  Config,
  GatewayInboundResponse,
  GatewayStatus,
  ExecutionStep,
  RouteDecision,
  WorkspaceStatus,
} from "../../types/config";
import { UiIcon } from "../UiIcon";
import { AgentRunTimeline } from "../AgentRun/AgentRunTimeline";
import { SETUP_CONFIG_UPDATED_EVENT } from "../../utils/appEvents";
import {
  TaskDeliverable,
  TaskInspector,
  TaskOnboarding,
  TaskStatusBar,
  type InspectorTab,
} from "./TaskWorkspace";
import {
  ModelPicker,
  parseModelSelection,
  persistModelSelection,
  readStoredMaxMode,
  readStoredModelSelection,
  type PickerProvider,
} from "./ModelPicker";
import { CommandPalette as CommandPaletteMenu } from "./CommandPalette";
import { ContractReviewPanel, ContractReviewReport } from "./ContractReviewPanel";
import type {
  ContractReviewStage,
  ContractReviewEngineCard,
  PreparedContractReview,
} from "./contractReviewModel";
import {
  DEFAULT_CONTRACT_REVIEW_ENGINES,
  friendlyContractReviewError,
} from "./contractReviewModel";
import {
  commandTokenAt,
  emptyCommandPalette,
  filterCommandPalette,
  paletteRows,
  resolveComposerSend,
  type CommandPalette,
  type CommandPaletteItem,
  type SelectedSkill,
  type SelectedSystemTool,
} from "./commandPaletteModel";

const GATEWAY_STATUS_POLL_MS = 8000;
import omninovalLogo from "../../assets/omninoval-logo.png";

const USER_ID = "desktop-user";
const DESKTOP_VISION_SESSION_KEY = "omninova-chat-desktop-vision";

interface DesktopScreenshotPayload {
  dataUrl: string;
  width: number;
  height: number;
}

/** 单个文本附件最大字节（UTF-8），防止拖入巨型日志拖垮前端 */
const DROP_TEXT_FILE_MAX_BYTES = 512 * 1024;
/** 嵌入为 Markdown 图片的最大字节（base64 后更大，勿调高过多） */
const DROP_IMAGE_INLINE_MAX_BYTES = 256 * 1024;
/** 与隐藏 file input 关联，用于 label 触发（避免 Tauri/WebKit 拦截程序化 click） */
const CHAT_ATTACHMENT_INPUT_ID = "chat-composer-file-input";
/** 单次拖放最多处理的文件数 */
const DROP_FILES_MAX_COUNT = 16;
/** 会话级临时 Workspace 的持久化 key（bug#3：工作空间记忆） */
const SESSION_WORKSPACE_STORAGE_KEY = "omninova.chat.sessionWorkspaces.v1";
const COMPOSER_HEIGHT_STORAGE_KEY = "omninova.chat.composerHeight.v1";
const COMPOSER_MIN_HEIGHT = 54;
const COMPOSER_MAX_HEIGHT = 240;
const COMPOSER_HELP_TEXT =
  "可用命令：\n/help — 显示帮助\n/skills — 打开技能设置\n输入 / 打开命令面板，选择已安装技能后再发送任务。";

const TEXT_FILE_EXTENSIONS = new Set([
  "txt",
  "md",
  "markdown",
  "mdx",
  "json",
  "jsonl",
  "jsonc",
  "csv",
  "tsv",
  "log",
  "yaml",
  "yml",
  "xml",
  "html",
  "htm",
  "swift",
  "rs",
  "py",
  "rb",
  "go",
  "java",
  "kt",
  "kts",
  "c",
  "cc",
  "cpp",
  "h",
  "hpp",
  "cs",
  "php",
  "vue",
  "svelte",
  "js",
  "mjs",
  "cjs",
  "ts",
  "tsx",
  "jsx",
  "css",
  "scss",
  "less",
  "sass",
  "sh",
  "bash",
  "zsh",
  "fish",
  "sql",
  "toml",
  "ini",
  "cfg",
  "conf",
  "gradle",
  "plist",
  "rst",
  "tex",
  "bib",
]);

function fileExtensionLower(name: string): string {
  const i = name.lastIndexOf(".");
  return i >= 0 ? name.slice(i + 1).toLowerCase() : "";
}

function workspaceBasename(path: string): string {
  const normalized = path.replace(/\\/g, "/").replace(/\/+$/, "");
  const parts = normalized.split("/").filter(Boolean);
  return parts.at(-1) ?? path;
}

function summarizeWorkspacePath(path: string | null | undefined): string {
  if (!path) return "选择 Workspace";
  const name = workspaceBasename(path);
  if (name.length <= 24) return name;
  return `${name.slice(0, 10)}…${name.slice(-10)}`;
}

function isProbablyTextFile(file: File): boolean {
  const ext = fileExtensionLower(file.name);
  if (TEXT_FILE_EXTENSIONS.has(ext)) return true;
  if (file.type.startsWith("text/")) return true;
  if (
    file.type === "application/json" ||
    file.type === "application/xml" ||
    file.type.includes("javascript") ||
    file.type === "application/x-sh"
  ) {
    return true;
  }
  return false;
}

function escapeMarkdownImageAlt(text: string): string {
  return text.replace(/[[\]]/g, "");
}

function readFileAsDataURL(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error ?? new Error("读取失败"));
    reader.readAsDataURL(file);
  });
}

/**
 * 输入框附件（bug#1/#2）：拖入/选择/粘贴的文件先以「芯片」形式展示在
 * 输入框上方，发送时才把 content 拼接进消息正文。
 */
interface ComposerAttachment {
  id: string;
  name: string;
  kind: "image" | "text" | "other";
  /** 发送时拼接进消息的文本（Markdown 图片 / 文本内容 / 占位说明） */
  content: string;
  /** 芯片上的补充说明，如大小或读取失败原因 */
  note?: string;
  /** Tauri 原生拖放/选择得到的绝对路径；发送前才挂载进当前 Workspace。 */
  sourcePath?: string;
  /** 发送前挂载完成后使用的 Workspace 相对路径。 */
  mountedPath?: string;
}

let composerAttachmentSeq = 0;
function nextAttachmentId(): string {
  composerAttachmentSeq += 1;
  return `attachment-${Date.now()}-${composerAttachmentSeq}`;
}

/** 将拖放/选择/粘贴的文件转为结构化附件（浏览器 File API 路径） */
async function buildAttachmentsFromFiles(
  files: FileList | readonly File[]
): Promise<ComposerAttachment[]> {
  const list = Array.from(files).slice(0, DROP_FILES_MAX_COUNT);
  const items: ComposerAttachment[] = [];

  for (const file of list) {
    const displayName = file.name?.trim() || "unnamed";
    const sizeKb = Math.round(file.size / 1024);

    if (file.size === 0) {
      items.push({
        id: nextAttachmentId(),
        name: displayName,
        kind: "other",
        content: `[空文件: ${displayName}]`,
        note: "空文件",
      });
      continue;
    }

    if (file.type.startsWith("image/")) {
      if (file.size > DROP_IMAGE_INLINE_MAX_BYTES) {
        items.push({
          id: nextAttachmentId(),
          name: displayName,
          kind: "image",
          content: `[图片: ${displayName} · ${sizeKb} KB — 超过 ${Math.round(DROP_IMAGE_INLINE_MAX_BYTES / 1024)} KB 上限未嵌入；请缩小后再添加或改用文字描述。]`,
          note: `${sizeKb} KB · 超限未嵌入`,
        });
        continue;
      }
      try {
        const dataUrl = await readFileAsDataURL(file);
        items.push({
          id: nextAttachmentId(),
          name: displayName,
          kind: "image",
          content: `![${escapeMarkdownImageAlt(displayName)}](${dataUrl})`,
          note: `${sizeKb} KB`,
        });
      } catch {
        items.push({
          id: nextAttachmentId(),
          name: displayName,
          kind: "image",
          content: `[图片读取失败: ${displayName}]`,
          note: "读取失败",
        });
      }
      continue;
    }

    if (isProbablyTextFile(file)) {
      if (file.size > DROP_TEXT_FILE_MAX_BYTES) {
        items.push({
          id: nextAttachmentId(),
          name: displayName,
          kind: "text",
          content: `[文本附件 ${displayName}: 过大 (${sizeKb} KB)，上限 ${Math.round(DROP_TEXT_FILE_MAX_BYTES / 1024)} KB — 请拆分或使用更小文件。]`,
          note: `${sizeKb} KB · 超限`,
        });
        continue;
      }
      try {
        const text = await file.text();
        items.push({
          id: nextAttachmentId(),
          name: displayName,
          kind: "text",
          content: `--- 附件: ${displayName} ---\n${text}\n--- 附件结束 ---`,
          note: `${sizeKb} KB`,
        });
      } catch {
        items.push({
          id: nextAttachmentId(),
          name: displayName,
          kind: "text",
          content: `[文本读取失败: ${displayName}]`,
          note: "读取失败",
        });
      }
      continue;
    }

    items.push({
      id: nextAttachmentId(),
      name: displayName,
      kind: "other",
      content: `[附件: ${displayName} · ${file.type || "未知类型"} · ${sizeKb} KB — 未能自动读取此类文件内容；可先导出为文本再添加，或让 Agent 用工作区工具读取。]`,
      note: `${sizeKb} KB · 未读取内容`,
    });
  }

  return items;
}

interface ChatMessage extends StoredChatMessage {
  steps?: ExecutionStep[];
}

interface ContractReviewUiState {
  stage: ContractReviewStage;
  error?: string;
}

type SidebarTab = "avatars" | "history";

interface ChatProps {
  /** 初始侧栏分区：智能体列表或历史任务。 */
  initialSidebarTab?: SidebarTab;
  /** Chat stays mounted across navigation; refresh configuration when visible. */
  isActive?: boolean;
  /** Open a related configuration surface from prerequisite/action prompts. */
  onOpenSettings?: (target: "providers" | "general" | "channels" | "skills" | "persona") => void;
}

interface CollectedTaskArtifact {
  path: string;
  size: number;
  modifiedAt: number;
  extension: string;
}

export function Chat({
  initialSidebarTab = "avatars",
  isActive = true,
  onOpenSettings,
}: ChatProps) {
  const initialStorage = useMemo(() => loadChatStorage(), []);
  const [avatars, setAvatars] = useState<StoredAvatarSession[]>(initialStorage.avatars);
  const [activeAvatarId, setActiveAvatarId] = useState(initialStorage.activeAvatarId);
  const [sidebarTab, setSidebarTab] = useState<SidebarTab>(initialSidebarTab);
  const [taskHistory, setTaskHistory] = useState<TaskHistoryEntry[]>(() =>
    loadTaskHistory()
  );
  const taskHistoryRef = useRef<TaskHistoryEntry[]>(taskHistory);
  const [selectedTaskRunId, setSelectedTaskRunId] = useState<string | null>(null);
  const [inspectorOpen, setInspectorOpen] = useState(false);
  const [inspectorTab, setInspectorTab] = useState<InspectorTab>("process");
  const [pendingDeleteAvatarId, setPendingDeleteAvatarId] = useState<string | null>(null);
  const [messagesBySession, setMessagesBySession] = useState<Record<string, ChatMessage[]>>(
    initialStorage.messagesBySession
  );
  // 已删除会话墓碑：防止网关同步把删掉的会话重新合并回列表。
  const [deletedSessionIds, setDeletedSessionIds] = useState<string[]>(
    initialStorage.deletedSessionIds ?? []
  );
  const [historyLoading, setHistoryLoading] = useState(false);
  // 输入草稿与运行状态按会话隔离，避免一个会话影响其它会话。
  const [inputs, setInputs] = useState<Record<string, string>>({});
  // bug#1/#2：待发送附件（拖入/选择/粘贴），按会话隔离，以芯片展示。
  const [attachmentsBySession, setAttachmentsBySession] = useState<
    Record<string, ComposerAttachment[]>
  >({});
  const [runs, setRuns] = useState<
    Record<string, { elapsedSec: number; steps: ExecutionStep[]; runId: string; longRunning?: boolean }>
  >({});
  const [error, setError] = useState<string | null>(null);
  const [copiedMessageIndex, setCopiedMessageIndex] = useState<number | null>(null);
  const [copyError, setCopyError] = useState<string | null>(null);
  const [copyNotice, setCopyNotice] = useState<string | null>(null);
  const [gatewayStatus, setGatewayStatus] = useState<"connecting" | "connected" | "disconnected">("connecting");
  const [gatewayUrl, setGatewayUrl] = useState<string>("");
  const [gatewayStarting, setGatewayStarting] = useState(false);
  // bug#10：选项来自实际已启用的 Provider 模型列表，选中值随消息发送生效。
  const [availableProviders, setAvailableProviders] = useState<PickerProvider[]>([]);
  const [selectedModel, setSelectedModel] = useState(readStoredModelSelection);
  const [maxMode, setMaxMode] = useState(readStoredMaxMode);
  const [defaultProviderId, setDefaultProviderId] = useState("");
  const [defaultModelId, setDefaultModelId] = useState("");
  const messagesScrollRef = useRef<HTMLDivElement>(null);
  const composerInputRef = useRef<HTMLTextAreaElement>(null);
  const stickToBottomRef = useRef(true);
  const historyLoadGenRef = useRef(0);
  // 每个会话独立的取消标志与计时器。
  const cancelledRef = useRef<Record<string, boolean>>({});
  const elapsedTimersRef = useRef<Record<string, ReturnType<typeof setInterval>>>({});
  const safetyTimersRef = useRef<Record<string, ReturnType<typeof setTimeout>>>({});
  const commandFallbackTimersRef = useRef<Record<string, ReturnType<typeof setTimeout>>>({});
  const runAvatarIdsRef = useRef<Record<string, string>>({});
  const contractReviewRunAvatarIdsRef = useRef<Record<string, string>>({});
  const activeRunIdRef = useRef<string | null>(null);
  const terminalRunIdsRef = useRef<Set<string>>(new Set());
  const completedRunIdsRef = useRef<Set<string>>(new Set());
  const insertedReplyRunIdsRef = useRef<Set<string>>(new Set());
  const runsRef = useRef(runs);
  const [selectedSkills, setSelectedSkills] = useState<Record<string, SelectedSkill>>({});
  const [selectedSystemTools, setSelectedSystemTools] = useState<Record<string, SelectedSystemTool>>({});
  const [contractReviewEngines, setContractReviewEngines] = useState<ContractReviewEngineCard[]>(
    DEFAULT_CONTRACT_REVIEW_ENGINES
  );
  const [selectedContractEngine, setSelectedContractEngine] = useState("omninova-contract-risk");
  const [contractExtraInstructions, setContractExtraInstructions] = useState("");
  const [contractReviewUiByAvatar, setContractReviewUiByAvatar] = useState<
    Record<string, ContractReviewUiState>
  >({});
  const [lastRiskExport, setLastRiskExport] = useState<unknown>(null);
  const [commandPalette, setCommandPalette] = useState<CommandPalette>(() => emptyCommandPalette());
  const [composerCursor, setComposerCursor] = useState(0);
  const [paletteIndex, setPaletteIndex] = useState(0);
  const [paletteDismissedToken, setPaletteDismissedToken] = useState<string | null>(null);
  const [composerDragActive, setComposerDragActive] = useState(false);
  const [composerInputHeight, setComposerInputHeight] = useState(() => {
    const stored = Number(localStorage.getItem(COMPOSER_HEIGHT_STORAGE_KEY));
    return Number.isFinite(stored)
      ? Math.min(COMPOSER_MAX_HEIGHT, Math.max(COMPOSER_MIN_HEIGHT, stored))
      : COMPOSER_MIN_HEIGHT;
  });
  const [desktopVisionMaster, setDesktopVisionMaster] = useState(false);
  const [desktopVisionOn, setDesktopVisionOn] = useState(false);
  const [desktopVisionMaxPx, setDesktopVisionMaxPx] = useState(1280);
  const [workspaceDir, setWorkspaceDir] = useState<string | null>(null);
  const [workspaceSource, setWorkspaceSource] = useState<"agent" | "global" | null>(null);
  const [workspaceStatus, setWorkspaceStatus] = useState<WorkspaceStatus | null>(null);
  const [workspaceMenuOpen, setWorkspaceMenuOpen] = useState(false);
  const workspaceMenuRef = useRef<HTMLDivElement>(null);
  /**
   * Session-level temporary workspace. Set by the chat-page Workspace button
   * without modifying the agent's default workspace. This takes the highest
   * priority when the Agent processes a message (higher than per-agent or
   * global workspace_dir).
   */
  const [sessionWorkspaceDirs, setSessionWorkspaceDirs] = useState<Record<string, string>>(() => {
    try {
      const raw = localStorage.getItem(SESSION_WORKSPACE_STORAGE_KEY);
      if (!raw) return {};
      const parsed = JSON.parse(raw) as Record<string, string>;
      return parsed && typeof parsed === "object" ? parsed : {};
    } catch {
      return {};
    }
  });

  useEffect(() => {
    try {
      localStorage.setItem(
        SESSION_WORKSPACE_STORAGE_KEY,
        JSON.stringify(sessionWorkspaceDirs)
      );
    } catch {
      // localStorage 不可用时忽略
    }
  }, [sessionWorkspaceDirs]);

  const activeSession = avatars.find((a) => a.id === activeAvatarId);
  const sessionId = activeSession?.sessionId ?? "omninova-chat-session";
  const sessionWorkspaceDir = sessionWorkspaceDirs[activeAvatarId] ?? null;
  const activeWorkspaceDir = sessionWorkspaceDir ?? workspaceDir;
  const workspaceSummary = summarizeWorkspacePath(activeWorkspaceDir);
  const workspaceLabel = sessionWorkspaceDir
    ? "临时"
    : workspaceSource === "agent"
      ? "Agent"
      : workspaceSource === "global"
        ? "全局"
        : "Workspace";
  const messages = useMemo(
    () => messagesBySession[activeAvatarId] ?? [],
    [messagesBySession, activeAvatarId]
  );
  const contractReportMessageIndexes = useMemo(() => {
    const indexes = new Set<number>();
    messages.forEach((message, index) => {
      const request = messages[index - 1];
      if (
        message.role === "assistant" &&
        request?.role === "user" &&
        request.toolId === "tool:contract-review"
      ) {
        indexes.add(index);
      }
    });
    return indexes;
  }, [messages]);
  const latestContractReportIndex = useMemo(() => {
    const indexes = Array.from(contractReportMessageIndexes);
    return indexes.length ? indexes[indexes.length - 1] : -1;
  }, [contractReportMessageIndexes]);

  // 仅反映「当前查看的会话」的运行/输入状态。
  const activeRun = runs[activeAvatarId];
  const sending = Boolean(activeRun);
  const elapsedSec = activeRun?.elapsedSec ?? 0;
  const activeSteps = useMemo(() => activeRun?.steps ?? [], [activeRun?.steps]);
  const activeRunId = activeRun?.runId ?? null;
  const input = inputs[activeAvatarId] ?? "";
  const attachments = attachmentsBySession[activeAvatarId] ?? [];
  const selectedSkill = selectedSkills[activeAvatarId];
  const selectedSystemTool = selectedSystemTools[activeAvatarId];
  const contractReviewUi = contractReviewUiByAvatar[activeAvatarId] ?? {
    stage: "idle" as const,
  };
  const commandToken = commandTokenAt(input, composerCursor);
  const filteredPalette = useMemo(
    () => filterCommandPalette(commandPalette, commandToken?.token ?? ""),
    [commandPalette, commandToken?.token]
  );
  const paletteRowItems = useMemo(() => paletteRows(filteredPalette), [filteredPalette]);
  const paletteVisible =
    Boolean(commandToken) && paletteDismissedToken !== commandToken?.token;
  const activeTask = useMemo(() => {
    const selected = selectedTaskRunId
      ? taskHistory.find((task) => task.runId === selectedTaskRunId)
      : null;
    return selected ?? taskHistory.find((task) => task.avatarId === activeAvatarId) ?? null;
  }, [activeAvatarId, selectedTaskRunId, taskHistory]);

  // Grow the composer with its content up to the CSS max-height, then scroll.
  useEffect(() => {
    if (!isActive) return;
    const node = composerInputRef.current;
    if (!node) return;
    node.style.height = "auto";
    node.style.height = `${node.scrollHeight}px`;
  }, [input, activeAvatarId, isActive]);

  const openInspector = useCallback((tab: InspectorTab = "process") => {
    setInspectorTab(tab);
    setInspectorOpen(true);
  }, []);

  useEffect(() => {
    if (!inspectorOpen) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setInspectorOpen(false);
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [inspectorOpen]);

  useEffect(() => {
    if (!pendingDeleteAvatarId) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setPendingDeleteAvatarId(null);
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [pendingDeleteAvatarId]);

  useEffect(() => {
    runsRef.current = runs;
  }, [runs]);

  useEffect(() => {
    activeRunIdRef.current = activeRunId;
  }, [activeRunId]);

  const findAvatarIdByRunId = useCallback((runId: string): string | null => {
    const mapped = runAvatarIdsRef.current[runId];
    if (mapped) return mapped;
    const entry = Object.entries(runsRef.current).find(([, run]) => run.runId === runId);
    return entry?.[0] ?? null;
  }, []);

  const finishRun = useCallback(
    (runId: string) => {
      const avatarId = findAvatarIdByRunId(runId);
      terminalRunIdsRef.current.add(runId);

      const safetyTimer = safetyTimersRef.current[runId];
      if (safetyTimer) {
        clearTimeout(safetyTimer);
        delete safetyTimersRef.current[runId];
      }
      const commandFallbackTimer = commandFallbackTimersRef.current[runId];
      if (commandFallbackTimer) {
        clearTimeout(commandFallbackTimer);
        delete commandFallbackTimersRef.current[runId];
      }

      if (avatarId) {
        const elapsedTimer = elapsedTimersRef.current[avatarId];
        if (elapsedTimer) {
          clearInterval(elapsedTimer);
          delete elapsedTimersRef.current[avatarId];
        }
      }

      setRuns((prev) => {
        const targetAvatarId =
          avatarId ??
          Object.entries(prev).find(([, run]) => run.runId === runId)?.[0] ??
          null;
        if (!targetAvatarId) return prev;
        const currentRun = prev[targetAvatarId];
        if (!currentRun || currentRun.runId !== runId) return prev;
        const next = { ...prev };
        delete next[targetAvatarId];
        runsRef.current = next;
        return next;
      });
      if (activeRunIdRef.current === runId) {
        activeRunIdRef.current = null;
      }
      if (avatarId) {
        cancelledRef.current[avatarId] = false;
      }
      delete runAvatarIdsRef.current[runId];
    },
    [findAvatarIdByRunId]
  );

  const appendAssistantMessageOnce = useCallback(
    (runId: string, content: string, avatarId: string) => {
      const reply = content.trim();
      if (!reply || insertedReplyRunIdsRef.current.has(runId)) return false;
      insertedReplyRunIdsRef.current.add(runId);

      setMessagesBySession((prev) => ({
        ...prev,
        [avatarId]: [
          ...(prev[avatarId] ?? []),
          {
            role: "assistant",
            content: reply,
          },
        ],
      }));
      setAvatars((prev) =>
        prev.map((a) =>
          a.id === avatarId ? { ...a, lastAt: formatTime(new Date()) } : a
        )
      );
      return true;
    },
    []
  );

  const setActiveInput = useCallback(
    (value: string) =>
      setInputs((prev) => ({ ...prev, [activeAvatarId]: value })),
    [activeAvatarId]
  );

  const syncComposerCursor = useCallback((target: HTMLTextAreaElement) => {
    setComposerCursor(target.selectionStart ?? target.value.length);
  }, []);

  useEffect(() => {
    if (!commandToken) {
      setPaletteDismissedToken(null);
      return;
    }
    let cancelled = false;
    const handle = window.setTimeout(() => {
      void invokeTauri<CommandPalette>("list_command_palette", { query: "" })
        .then((result) => {
          if (!cancelled) setCommandPalette(result);
        })
        .catch((reason) => {
          if (!cancelled) {
            setCommandPalette(emptyCommandPalette());
            setError("命令面板加载失败，请检查网关连接后重试。");
          }
          if (import.meta.env.DEV) {
            console.warn("[skill-ui] palette_load_failed", reason);
          }
        });
    }, 60);
    return () => {
      cancelled = true;
      window.clearTimeout(handle);
    };
  }, [commandToken?.token]);

  useEffect(() => {
    setPaletteIndex(0);
  }, [commandToken?.token, filteredPalette.generation, paletteRowItems.length]);

  useEffect(() => {
    if (!isTauriEnvironment()) return;
    void invokeTauri<ContractReviewEngineCard[]>("list_contract_review_engines")
      .then(setContractReviewEngines)
      .catch((reason) => {
        if (import.meta.env.DEV) console.warn("[contract-review] engine_list_failed", reason);
      });
  }, []);

  const applyPaletteSelection = useCallback(
    (item: CommandPaletteItem) => {
      if (!commandToken) return;
      if (import.meta.env.DEV) {
        console.log(`[skill-ui] palette_select id=${item.id} kind=${item.kind} enabled=${item.enabled}`);
      }
      const next = `${input.slice(0, commandToken.start)}${input.slice(commandToken.end)}`.replace(
        /^\s+/,
        ""
      );
      setActiveInput(next);
      setComposerCursor(Math.min(commandToken.start, next.length));
      // Selection exits command mode immediately. The slash token must not
      // remain active while the user types the actual task.
      setPaletteDismissedToken(commandToken.token);
      if (item.id === "system:help") {
        setMessagesBySession((prev) => ({
          ...prev,
          [activeAvatarId]: [
            ...(prev[activeAvatarId] ?? []),
            { role: "assistant", content: COMPOSER_HELP_TEXT },
          ],
        }));
        return;
      }
      if (item.id === "system:skills") {
        onOpenSettings?.("skills");
        return;
      }
      if (item.kind === "system_tool" && item.id === "tool:contract-review") {
        setSelectedSystemTools((prev) => ({
          ...prev,
          [activeAvatarId]: {
            id: "tool:contract-review",
            displayName: item.displayName,
            commandAlias: item.commandAlias,
          },
        }));
        setSelectedSkills((prev) => {
          if (!prev[activeAvatarId]) return prev;
          const nextSkills = { ...prev };
          delete nextSkills[activeAvatarId];
          return nextSkills;
        });
        return;
      }
      if (item.kind === "skill") {
        if (!item.enabled) {
          setError("所选技能当前不可用，请重新选择。");
          setMessagesBySession((prev) => ({
            ...prev,
            [activeAvatarId]: [
              ...(prev[activeAvatarId] ?? []),
              { role: "assistant", content: "所选技能当前不可用。" },
            ],
          }));
          return;
        }
        // Keep the selected catalog identity synchronously. A refresh is still
        // performed at send time, but it must not race the chip/input state.
        setSelectedSkills((prev) => ({
          ...prev,
          [activeAvatarId]: {
            id: item.id,
            displayName: item.displayName,
            commandAlias: item.commandAlias,
          },
        }));
        setSelectedSystemTools((prev) => {
          if (!prev[activeAvatarId]) return prev;
          const nextTools = { ...prev };
          delete nextTools[activeAvatarId];
          return nextTools;
        });
      }
    },
    [activeAvatarId, commandToken, input, onOpenSettings, setActiveInput]
  );

  useEffect(() => {
    setSidebarTab(initialSidebarTab);
  }, [initialSidebarTab]);

  const loadSessionHistory = useCallback(
    async (
      avatarId: string,
      targetSessionId: string,
      preferGateway: boolean,
      options?: { silent?: boolean }
    ) => {
      if (!preferGateway) {
        return;
      }
      const gen = ++historyLoadGenRef.current;
      if (!options?.silent) {
        setHistoryLoading(true);
      }
      try {
        const remote = await fetchSessionHistory(targetSessionId);
        if (gen !== historyLoadGenRef.current) return;

        setMessagesBySession((prev) => {
          const current = prev[avatarId] ?? [];
          const next = remote.length > 0
            ? mergeLocalMessageMetadata(remote, current)
            : current;
          if (areStoredMessagesEqual(current, next)) {
            return prev;
          }
          return { ...prev, [avatarId]: next };
        });
      } catch {
        // 保留本地缓存
      } finally {
        if (gen === historyLoadGenRef.current && !options?.silent) {
          setHistoryLoading(false);
        }
      }
    },
    []
  );

  const syncChatSessions = useCallback(async () => {
    try {
      const remoteAll = await fetchWebSessionsFromGateway();
      // 过滤掉已被用户删除的会话，避免「删了又回来」。
      const tombstoned = new Set(deletedSessionIds);
      const remote = remoteAll.filter((s) => !tombstoned.has(s.sessionId));
      setAvatars((prev) => {
        const merged = mergeAvatarSessions(prev, remote);
        if (
          merged.length === prev.length &&
          merged.every(
            (a, i) =>
              prev[i]?.id === a.id &&
              prev[i]?.sessionId === a.sessionId &&
              prev[i]?.name === a.name &&
              prev[i]?.lastAt === a.lastAt
          )
        ) {
          return prev;
        }
        return merged;
      });
    } catch {
      // 网关未就绪时仅使用本地会话列表
    }
  }, [deletedSessionIds]);

  useEffect(() => {
    saveChatStorage({
      avatars,
      activeAvatarId,
      messagesBySession,
      deletedSessionIds,
    });
  }, [avatars, activeAvatarId, messagesBySession, deletedSessionIds]);

  useEffect(() => {
    saveTaskHistory(taskHistory);
    taskHistoryRef.current = taskHistory;
  }, [taskHistory]);

  const refreshTaskArtifacts = useCallback(async (runId: string) => {
    const target = taskHistoryRef.current.find((task) => task.runId === runId);
    if (!target?.workspacePath) return;
    try {
      const detected = await invokeTauri<CollectedTaskArtifact[]>("collect_task_artifacts", {
        workspacePath: target.workspacePath,
        startedAt: target.startedAt,
      });
      if (!detected.length) return;
      setTaskHistory((prev) => {
        const latest = prev.find((task) => task.runId === runId);
        if (!latest) return prev;
        const byPath = new Map<string, TaskChangedFile>();
        for (const file of latest.changedFiles ?? []) {
          byPath.set(file.path.replace(/\\/g, "/"), file);
        }
        for (const artifact of detected) {
          const key = artifact.path.replace(/\\/g, "/");
          const existing = byPath.get(key);
          byPath.set(key, {
            path: key,
            additions: existing?.additions ?? 0,
            deletions: existing?.deletions ?? 0,
            changeType: existing?.changeType ?? "modified",
            size: artifact.size,
            modifiedAt: artifact.modifiedAt,
            source: existing?.source ?? "scan",
          });
        }
        const changedFiles = [...byPath.values()]
          .sort((left, right) => (right.modifiedAt ?? 0) - (left.modifiedAt ?? 0))
          .slice(0, 200);
        return patchTask(prev, runId, { changedFiles });
      });
    } catch (reason) {
      if (import.meta.env.DEV) {
        console.debug("[collect_task_artifacts]", reason);
      }
    }
  }, []);

  // 记录一次新任务（发送消息即视为一次任务）。
  const recordTaskStart = useCallback((entry: TaskHistoryEntry) => {
    setTaskHistory((prev) => {
      const next = addTask(prev, entry);
      taskHistoryRef.current = next;
      return next;
    });
    setSelectedTaskRunId(entry.runId);
  }, []);

  const patchTaskProgress = useCallback(
    (runId: string, patch: Partial<TaskHistoryEntry>) => {
      setTaskHistory((prev) => patchTask(prev, runId, patch));
    },
    []
  );

  const appendTaskActivity = useCallback(
    (
      runId: string,
      label: string,
      tone: "info" | "success" | "warning" | "error" = "info",
      process: Omit<TaskActivityEntry, "at" | "label" | "tone"> = {}
    ) => {
      setTaskHistory((prev) => {
        const target = prev.find((task) => task.runId === runId);
        if (!target) return prev;
        const activity = [
          ...(target.activity ?? []),
          {
            at: Date.now(),
            label: label.slice(0, 180),
            tone,
            ...process,
            detail: process.detail?.slice(0, 260),
            path: process.path?.slice(0, 520),
          },
        ].slice(-120);
        return patchTask(prev, runId, { activity });
      });
    },
    []
  );

  // 终态到达时更新任务状态/耗时/结果预览。使用函数式更新以保持回调稳定。
  const finalizeTask = useCallback(
    (runId: string, status: TaskStatus, resultPreview?: string) => {
      setTaskHistory((prev) => {
        const target = prev.find((t) => t.runId === runId);
        if (!target) return prev;
        const endedAt = Date.now();
        return patchTask(prev, runId, {
          status,
          endedAt,
          durationMs: Math.max(0, endedAt - target.startedAt),
          resultPreview: resultPreview
            ? resultPreview.slice(0, 1600)
            : target.resultPreview,
          attentionReason: status === "failed" ? resultPreview?.slice(0, 260) : undefined,
          nextAction:
            status === "completed"
              ? "检查成果文件，确认无误后再提交或发布。"
              : status === "failed"
                ? "打开任务检查器查看最后步骤，修正配置后重新执行。"
                : undefined,
        });
      });
    },
    []
  );

  const settleRunFromCommand = useCallback(
    (
      runId: string,
      avatarId: string,
      outcome: { reply: string } | { error: string }
    ) => {
      const previous = commandFallbackTimersRef.current[runId];
      if (previous) clearTimeout(previous);

      // Tauri normally emits a terminal event before resolving the command.
      // Keep a short grace period for that event, then settle from the command
      // result so a dropped event cannot leave the UI spinning indefinitely.
      commandFallbackTimersRef.current[runId] = setTimeout(() => {
        delete commandFallbackTimersRef.current[runId];
        if (
          terminalRunIdsRef.current.has(runId) ||
          completedRunIdsRef.current.has(runId) ||
          runsRef.current[avatarId]?.runId !== runId
        ) {
          return;
        }

        terminalRunIdsRef.current.add(runId);
        completedRunIdsRef.current.add(runId);
        const contractAvatarId = contractReviewRunAvatarIdsRef.current[runId];

        if (cancelledRef.current[avatarId]) {
          appendAssistantMessageOnce(runId, "任务已取消。", avatarId);
          appendTaskActivity(runId, "任务已取消（由命令结果确认）", "warning");
          finalizeTask(runId, "cancelled");
          if (contractAvatarId) {
            setContractReviewUiByAvatar((prev) => ({
              ...prev,
              [contractAvatarId]: { stage: "cancelled" },
            }));
          }
        } else if ("reply" in outcome) {
          const reply = outcome.reply.trim();
          if (reply) {
            appendAssistantMessageOnce(runId, reply, avatarId);
            appendTaskActivity(runId, "任务已完成（终态事件兜底）", "success");
            finalizeTask(runId, "completed", reply);
            if (contractAvatarId) {
              setContractReviewUiByAvatar((prev) => ({
                ...prev,
                [contractAvatarId]: { stage: "completed" },
              }));
            }
          } else {
            const message = "模型请求已结束，但没有返回可显示的内容。";
            setError(message);
            appendAssistantMessageOnce(runId, `任务失败：${message}`, avatarId);
            appendTaskActivity(runId, message, "error");
            finalizeTask(runId, "failed", message);
            if (contractAvatarId) {
              setContractReviewUiByAvatar((prev) => ({
                ...prev,
                [contractAvatarId]: { stage: "failed", error: message },
              }));
            }
          }
        } else {
          const displayError = contractAvatarId
            ? friendlyContractReviewError(outcome.error)
            : outcome.error;
          setError(displayError);
          appendAssistantMessageOnce(runId, `任务失败：${displayError}`, avatarId);
          appendTaskActivity(runId, `任务失败：${displayError}`, "error");
          finalizeTask(runId, "failed", displayError);
          if (contractAvatarId) {
            setContractReviewUiByAvatar((prev) => ({
              ...prev,
              [contractAvatarId]: { stage: "failed", error: displayError },
            }));
          }
        }

        delete contractReviewRunAvatarIdsRef.current[runId];
        void refreshTaskArtifacts(runId);
        finishRun(runId);
      }, 600);
    },
    [
      appendAssistantMessageOnce,
      appendTaskActivity,
      finalizeTask,
      finishRun,
      refreshTaskArtifacts,
    ]
  );

  const handleClearTaskHistory = useCallback(() => {
    setTaskHistory((prev) =>
      prev.filter(
        (task) =>
          task.status === "running" ||
          task.status === "needs_approval" ||
          task.status === "waiting_input"
      )
    );
  }, []);

  useEffect(() => {
    void refreshGatewayStatus();
    const t = setInterval(refreshGatewayStatus, GATEWAY_STATUS_POLL_MS);
    return () => clearInterval(t);
  }, []);

  useEffect(() => {
    if (gatewayStatus !== "connected") return;
    void syncChatSessions();
  }, [gatewayStatus, syncChatSessions]);

  useEffect(() => {
    if (!sessionId || gatewayStatus !== "connected") return;
    void loadSessionHistory(activeAvatarId, sessionId, true);
  }, [activeAvatarId, sessionId, gatewayStatus, loadSessionHistory]);

  useEffect(() => {
    if (!workspaceMenuOpen) return;

    const handlePointerDown = (event: MouseEvent) => {
      const target = event.target as Node | null;
      if (target && workspaceMenuRef.current?.contains(target)) return;
      setWorkspaceMenuOpen(false);
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setWorkspaceMenuOpen(false);
    };

    document.addEventListener("mousedown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("mousedown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [workspaceMenuOpen]);

  const refreshSetupConfig = useCallback(() => {
    return invokeTauri<Config>("get_setup_config")
      .then((cfg) => {
        const master = cfg.multimodal?.desktop_vision_enabled ?? false;
        const maxPx = cfg.multimodal?.desktop_vision_max_dimension_px ?? 1280;
        setDesktopVisionMaster(master);
        setDesktopVisionMaxPx(maxPx);
        const agentWorkspace = cfg.agent?.workspace_dir ?? null;
        const globalWorkspace = cfg.workspace_dir ?? null;
        setWorkspaceDir(agentWorkspace || globalWorkspace || null);
        setWorkspaceSource(agentWorkspace ? "agent" : globalWorkspace ? "global" : null);
        setWorkspaceStatus(cfg.workspace_status ?? null);
        const enabled = (cfg.providers ?? [])
          .filter((p) => p.enabled)
          .map((p) => ({
            id: p.id,
            label: p.name || p.id,
            type: p.type,
            models: p.models ?? [],
          }));
        setAvailableProviders(enabled);
        setDefaultProviderId(cfg.default_provider ?? "");
        setDefaultModelId(cfg.default_model ?? "");
        // 之前选中的 Provider/模型 已被禁用/删除时回退到自动。
        setSelectedModel((prev) => {
          let next = prev;
          if (prev !== "auto") {
            const parsed = parseModelSelection(prev);
            const provider = enabled.find((item) => item.id === parsed.providerId);
            if (!provider) next = "auto";
            else if (parsed.model && provider.models.includes(parsed.model)) next = prev;
            else if (!parsed.model) {
              next = provider.models[0]
                ? `${provider.id}::${provider.models[0]}`
                : provider.id;
            } else {
              next = "auto";
            }
          }
          if (next !== prev) persistModelSelection(next);
          return next;
        });
        const stored = localStorage.getItem(DESKTOP_VISION_SESSION_KEY);
        if (stored === "1") setDesktopVisionOn(true);
        else if (stored === "0") setDesktopVisionOn(false);
        else setDesktopVisionOn(master);
      })
      .catch(() => {});
  }, []);

  useEffect(() => {
    if (isActive) void refreshSetupConfig();
  }, [isActive, refreshSetupConfig]);

  useEffect(() => {
    const handleSetupConfigUpdated = () => void refreshSetupConfig();
    window.addEventListener(SETUP_CONFIG_UPDATED_EVENT, handleSetupConfigUpdated);
    return () => {
      window.removeEventListener(SETUP_CONFIG_UPDATED_EVENT, handleSetupConfigUpdated);
    };
  }, [refreshSetupConfig]);

  const scrollMessagesToEnd = useCallback((behavior: ScrollBehavior = "auto") => {
    const container = messagesScrollRef.current;
    if (!container) return;
    container.scrollTo({ top: container.scrollHeight, behavior });
  }, []);

  const handleMessagesScroll = useCallback(() => {
    const container = messagesScrollRef.current;
    if (!container) return;
    const distanceFromBottom =
      container.scrollHeight - container.scrollTop - container.clientHeight;
    stickToBottomRef.current = distanceFromBottom < 80;
  }, []);

  useEffect(() => {
    if (!isActive || !stickToBottomRef.current) return;
    scrollMessagesToEnd("auto");
  }, [messages, sending, activeSteps, elapsedSec, isActive, scrollMessagesToEnd]);

  // 窗口缩放导致消息重新换行时，保持视口顶部正在浏览的消息位置不变。
  useEffect(() => {
    if (!isActive) return;
    const container = messagesScrollRef.current;
    if (!container) return;

    type ScrollAnchor = {
      element: HTMLElement;
      offsetTop: number;
    };

    let anchor: ScrollAnchor | null = null;
    let adjusting = false;

    const captureAnchor = () => {
      const distanceFromBottom =
        container.scrollHeight - container.scrollTop - container.clientHeight;
      if (distanceFromBottom < 80) {
        anchor = null;
        return;
      }

      const containerTop = container.getBoundingClientRect().top;
      const bubbles = container.querySelectorAll<HTMLElement>(".chat-bubble");
      let firstVisible: HTMLElement | null = null;
      let low = 0;
      let high = bubbles.length - 1;

      while (low <= high) {
        const middle = Math.floor((low + high) / 2);
        const bubble = bubbles.item(middle);
        if (bubble.getBoundingClientRect().bottom > containerTop) {
          firstVisible = bubble;
          high = middle - 1;
        } else {
          low = middle + 1;
        }
      }

      anchor = firstVisible
        ? {
            element: firstVisible,
            offsetTop: firstVisible.getBoundingClientRect().top - containerTop,
          }
        : null;
    };

    const handleScroll = () => {
      if (!adjusting) captureAnchor();
    };

    captureAnchor();
    container.addEventListener("scroll", handleScroll, { passive: true });

    const ro = new ResizeObserver(() => {
      adjusting = true;

      if (stickToBottomRef.current) {
        container.scrollTop = container.scrollHeight;
      } else if (anchor?.element.isConnected) {
        const currentOffsetTop =
          anchor.element.getBoundingClientRect().top -
          container.getBoundingClientRect().top;
        container.scrollTop += currentOffsetTop - anchor.offsetTop;
      }

      captureAnchor();
      adjusting = false;
    });

    ro.observe(container);
    return () => {
      ro.disconnect();
      container.removeEventListener("scroll", handleScroll);
    };
  }, [isActive]);

  useEffect(() => {
    const timers = elapsedTimersRef.current;
    const safetyTimers = safetyTimersRef.current;
    const commandFallbackTimers = commandFallbackTimersRef.current;
    return () => {
      Object.values(timers).forEach((timer) => clearInterval(timer));
      Object.values(safetyTimers).forEach((timer) => clearTimeout(timer));
      Object.values(commandFallbackTimers).forEach((timer) => clearTimeout(timer));
    };
  }, []);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    listenAgentRunEvents<AgentRunEvent | Record<string, unknown>>("agent-run-event", (event) => {
      const payload = event.payload as AgentRunEvent & {
        type?: string;
        run_id?: string;
        reply?: string;
        reply_preview?: string;
        error?: string;
        message?: string;
      };
      const runId = payload.run_id;
      if (import.meta.env.DEV && payload.type !== "model_delta") {
        console.log("[chat-agent-run-event payload]", payload);
      }
      if (disposed || !runId) return;

      const eventType: string = payload.type ?? "";
      const rawPayload = payload as unknown as Record<string, unknown>;
      const isTerminalEvent =
        eventType === "run_completed" ||
        eventType === "runCompleted" ||
        eventType === "run_failed" ||
        eventType === "runFailed" ||
        eventType === "run_cancelled" ||
        eventType === "runCancelled" ||
        eventType === "error";
      if (terminalRunIdsRef.current.has(runId) && !isTerminalEvent) {
        if (import.meta.env.DEV && payload.type !== "model_delta") {
          console.debug("[chat-agent-run-event ignored after terminal]", payload);
        }
        return;
      }

      // Keep a compact, privacy-aware task record for the sidebar and inspector.
      // Raw command output and model deltas are intentionally never persisted.
      const stringValue = (...keys: string[]) => {
        for (const key of keys) {
          const value = rawPayload[key];
          if (typeof value === "string" && value.trim()) return value.trim();
        }
        return "";
      };

      if (eventType === "approval_required" || eventType === "approvalRequired") {
        const reason = typeof rawPayload.reason === "string"
          ? rawPayload.reason
          : "该工具需要人工确认后才能继续。";
        const toolName = typeof rawPayload.tool_name === "string"
          ? rawPayload.tool_name
          : "受限工具";
        patchTaskProgress(runId, {
          status: "needs_approval",
          attentionReason: reason.slice(0, 260),
          approvalTool: toolName,
          nextAction: "在任务检查器中核对工具与原因，再调整授权策略或重新执行。",
        });
        appendTaskActivity(runId, `需要授权：${toolName} · ${reason}`, "warning", {
          kind: "approval",
          status: "waiting",
          toolName,
          detail: reason,
        });
      } else if (eventType === "file_changed" || eventType === "fileChanged") {
        const path = typeof rawPayload.path === "string" ? rawPayload.path : "";
        if (path) {
          setTaskHistory((prev) => {
            const target = prev.find((task) => task.runId === runId);
            if (!target) return prev;
            const existing = target.changedFiles ?? [];
            const nextFile = {
              path,
              additions: typeof rawPayload.additions === "number" ? rawPayload.additions : 0,
              deletions: typeof rawPayload.deletions === "number" ? rawPayload.deletions : 0,
              changeType:
                typeof rawPayload.change_type === "string"
                  ? rawPayload.change_type as NonNullable<TaskHistoryEntry["changedFiles"]>[number]["changeType"]
                  : undefined,
              source: "event" as const,
            };
            const changedFiles = [
              ...existing.filter((file) => file.path !== path),
              nextFile,
            ].slice(-80);
            return patchTask(prev, runId, { changedFiles });
          });
          appendTaskActivity(runId, `文件变更：${path}`, "success", {
            kind: "file",
            status: "completed",
            path,
            detail: `+${typeof rawPayload.additions === "number" ? rawPayload.additions : 0} / -${typeof rawPayload.deletions === "number" ? rawPayload.deletions : 0}`,
          });
        }
      } else if (eventType === "tool_started" || eventType === "toolStarted") {
        const toolName = stringValue("tool_name", "toolName") || "工具";
        const summary = stringValue("summary", "title") || "开始执行";
        patchTaskProgress(runId, {
          status: "running",
          attentionReason: undefined,
          approvalTool: undefined,
          nextAction: undefined,
        });
        appendTaskActivity(runId, `${toolName}：${summary}`, "info", {
          kind: "tool",
          status: "running",
          toolName,
          detail: summary,
        });
      } else if (eventType === "tool_completed" || eventType === "toolCompleted") {
        const toolName = stringValue("tool_name", "toolName") || "工具";
        const success = rawPayload.success === true;
        const summary = stringValue("result_summary", "resultSummary", "summary");
        appendTaskActivity(runId, `${toolName}${success ? "已完成" : "执行失败"}`, success ? "success" : "error", {
          kind: "tool",
          status: success ? "completed" : "failed",
          toolName,
          detail: summary || undefined,
        });
      } else if (eventType === "skill_activated" || eventType === "skillActivated") {
        const skillId = stringValue("skill_id", "skillId");
        const skillName = stringValue("display_name", "displayName") || skillId;
        const avatarId = findAvatarIdByRunId(runId);
        if (avatarId && skillId) {
          setMessagesBySession((prev) => {
            const existing = prev[avatarId] ?? [];
            let index = -1;
            for (let i = existing.length - 1; i >= 0; i -= 1) {
              if (existing[i]?.role === "user") {
                index = i;
                break;
              }
            }
            if (index < 0) return prev;
            const nextMessages = existing.slice();
            nextMessages[index] = {
              ...nextMessages[index],
              skillId,
              skillName,
            };
            return { ...prev, [avatarId]: nextMessages };
          });
        }
        appendTaskActivity(runId, `已启用技能：${skillName}`, "info", {
          kind: "lifecycle",
          status: "completed",
          detail: skillId,
        });
      } else if (eventType === "model_started" || eventType === "modelStarted") {
        const title = stringValue("title") || "模型开始分析";
        appendTaskActivity(runId, title, "info", {
          kind: "model",
          status: "running",
          detail: title,
        });
      } else if (eventType === "model_completed" || eventType === "modelCompleted") {
        const title = stringValue("title") || "模型阶段已完成";
        const contractAvatarId = contractReviewRunAvatarIdsRef.current[runId];
        if (contractAvatarId) {
          setContractReviewUiByAvatar((prev) => ({
            ...prev,
            [contractAvatarId]: { stage: "generating" },
          }));
        }
        appendTaskActivity(runId, title, "success", {
          kind: "model",
          status: "completed",
          detail: title,
        });
      } else if (eventType === "tool_call_created" || eventType === "toolCallCreated") {
        const toolName = stringValue("tool_name", "toolName") || "工具";
        const title = stringValue("title") || `准备调用 ${toolName}`;
        appendTaskActivity(runId, title, "info", {
          kind: "tool",
          status: "waiting",
          toolName,
          detail: title,
        });
      } else if (eventType === "patch_started" || eventType === "patchStarted") {
        const path = stringValue("path");
        appendTaskActivity(runId, path ? `准备修改：${path}` : "准备修改文件", "info", {
          kind: "file",
          status: "running",
          path: path || undefined,
        });
      } else if (eventType === "patch_applied" || eventType === "patchApplied") {
        const path = stringValue("path");
        appendTaskActivity(runId, path ? `修改已应用：${path}` : "文件修改已应用", "success", {
          kind: "file",
          status: "completed",
          path: path || undefined,
        });
      } else if (eventType === "patch_failed" || eventType === "patchFailed") {
        const path = stringValue("path");
        appendTaskActivity(runId, path ? `修改失败：${path}` : "文件修改失败", "error", {
          kind: "file",
          status: "failed",
          path: path || undefined,
          detail: stringValue("error", "message") || undefined,
        });
      } else if (eventType === "run_started" || eventType === "runStarted") {
        patchTaskProgress(runId, { status: "running" });
        appendTaskActivity(runId, "任务已开始", "info", {
          kind: "lifecycle",
          status: "running",
        });
      }
      if (
        !isTerminalEvent
      ) {
        return;
      }

      const avatarId = findAvatarIdByRunId(runId);
      if (!avatarId) {
        finishRun(runId);
        return;
      }
      terminalRunIdsRef.current.add(runId);

      if (eventType === "run_completed" || eventType === "runCompleted") {
        if (!completedRunIdsRef.current.has(runId)) {
          completedRunIdsRef.current.add(runId);
          const finalReply = payload.reply || payload.reply_preview || stringValue("replyPreview") || "";
          if (import.meta.env.DEV) {
            console.log("[appendAssistantMessageOnce] run_id=" + runId + " reply_len=" + finalReply.length);
          }
          if (!finalReply.trim()) {
            const emptyOutputError = "模型本次未返回可显示的内容，请重试或更换技能。";
            setError(emptyOutputError);
            if (contractReviewRunAvatarIdsRef.current[runId]) {
              setContractReviewUiByAvatar((prev) => ({
                ...prev,
                [avatarId]: { stage: "failed", error: emptyOutputError },
              }));
            }
            if (!cancelledRef.current[avatarId]) {
              appendAssistantMessageOnce(runId, `任务失败：${emptyOutputError}`, avatarId);
            }
            appendTaskActivity(runId, `任务失败：${emptyOutputError}`, "error");
            finalizeTask(runId, "failed", emptyOutputError);
          } else {
            if (!cancelledRef.current[avatarId]) {
              appendAssistantMessageOnce(runId, finalReply, avatarId);
            }
            if (contractReviewRunAvatarIdsRef.current[runId]) {
              setContractReviewUiByAvatar((prev) => ({
                ...prev,
                [avatarId]: { stage: "completed" },
              }));
            }
            appendTaskActivity(runId, "任务已完成", "success");
            finalizeTask(runId, "completed", finalReply);
          }
          void refreshTaskArtifacts(runId);
        }
        if (import.meta.env.DEV) {
          console.log("[finishRun] run_id=" + runId);
        }
        delete contractReviewRunAvatarIdsRef.current[runId];
        finishRun(runId);
        return;
      }

      if (eventType === "run_cancelled" || eventType === "runCancelled") {
        if (!completedRunIdsRef.current.has(runId)) {
          completedRunIdsRef.current.add(runId);
          appendAssistantMessageOnce(runId, "任务已取消。", avatarId);
          appendTaskActivity(runId, "任务已取消", "warning");
          finalizeTask(runId, "cancelled");
          if (contractReviewRunAvatarIdsRef.current[runId]) {
            setContractReviewUiByAvatar((prev) => ({
              ...prev,
              [avatarId]: { stage: "cancelled", error: "审核已取消，可以调整文件或审核要求后重新开始。" },
            }));
          }
          void refreshTaskArtifacts(runId);
        }
        delete contractReviewRunAvatarIdsRef.current[runId];
        finishRun(runId);
        return;
      }

      if (!completedRunIdsRef.current.has(runId)) {
        completedRunIdsRef.current.add(runId);
        const rawError = payload.error || payload.message || "Agent run failed";
        const contractAvatarId = contractReviewRunAvatarIdsRef.current[runId];
        const displayError = contractAvatarId ? friendlyContractReviewError(rawError) : rawError;
        setError(displayError);
        if (contractAvatarId) {
          setContractReviewUiByAvatar((prev) => ({
            ...prev,
            [contractAvatarId]: { stage: "failed", error: displayError },
          }));
          if (import.meta.env.DEV) {
            console.warn("[contract-review] run_failed", rawError);
          }
        }
        if (!cancelledRef.current[avatarId]) {
          appendAssistantMessageOnce(runId, `任务失败：${displayError}`, avatarId);
        }
        appendTaskActivity(runId, `任务失败：${displayError}`, "error");
        finalizeTask(runId, "failed", displayError);
        void refreshTaskArtifacts(runId);
      }
      delete contractReviewRunAvatarIdsRef.current[runId];
      finishRun(runId);
    }).then((fn) => {
      if (disposed) {
        fn();
      } else {
        unlisten = fn;
      }
    });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [
    appendAssistantMessageOnce,
    appendTaskActivity,
    findAvatarIdByRunId,
    finishRun,
    finalizeTask,
    patchTaskProgress,
    refreshTaskArtifacts,
  ]);

  const refreshGatewayStatus = async () => {
    try {
      const status = await invokeTauri<GatewayStatus>("gateway_status");
      setGatewayUrl(status.url ?? "");
      setGatewayStatus(status.running ? "connected" : "disconnected");
    } catch {
      setGatewayUrl("");
      setGatewayStatus("disconnected");
    }
  };

  const handleStartGateway = async () => {
    if (gatewayStarting) return;
    setGatewayStarting(true);
    setError(null);
    try {
      const status = await invokeTauri<GatewayStatus>("start_gateway");
      setGatewayUrl(status.url ?? "");
      setGatewayStatus(status.running ? "connected" : "disconnected");
      if (!status.running) {
        setError("网关未能启动，请打开设置检查监听地址和端口占用。")
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setGatewayStatus("disconnected");
      setError(`启动网关失败：${message}`);
    } finally {
      setGatewayStarting(false);
    }
  };

  const handleAddAvatar = () => {
    const id = `avatar-${Date.now()}`;
    const sessionId = `session-${id}`;
    const name = `智能体 ${avatars.length}`;
    setAvatars((prev) => [
      { id, name, sessionId, lastAt: formatTime(new Date()) },
      ...prev,
    ]);
    setMessagesBySession((prev) => ({ ...prev, [id]: [] }));
    setActiveAvatarId(id);
    setSelectedTaskRunId(null);
  };

  const handleRenameAvatar = (id: string) => {
    const target = avatars.find((avatar) => avatar.id === id);
    if (!target) return;
    const requested = window.prompt("重命名智能体", target.name);
    if (requested === null) return;
    const name = requested.trim();
    if (!name) {
      setError("智能体名称不能为空。");
      return;
    }
    if (name.length > 40) {
      setError("智能体名称最多 40 个字符。");
      return;
    }
    setError(null);
    setAvatars((prev) =>
      prev.map((avatar) => avatar.id === id ? { ...avatar, name } : avatar)
    );
    // 历史任务保留会话关联，同时同步展示名称，避免新旧名称混用。
    setTaskHistory((prev) =>
      prev.map((task) => task.avatarId === id ? { ...task, agentName: name } : task)
    );
  };

  const handleDeleteAvatar = (id: string) => {
    setPendingDeleteAvatarId(id);
  };

  const confirmDeleteAvatar = (id: string, keepTaskRecords: boolean) => {
    setPendingDeleteAvatarId(null);
    // 终止该会话可能正在进行的任务，并清理其计时器/运行态。
    cancelledRef.current[id] = true;
    const deletingRunId = runs[id]?.runId;
    if (deletingRunId) {
      appendTaskActivity(deletingRunId, "会话被删除，任务已取消", "warning");
      finalizeTask(deletingRunId, "cancelled", "会话已删除");
      terminalRunIdsRef.current.add(deletingRunId);
      delete runAvatarIdsRef.current[deletingRunId];
    }
    const timer = elapsedTimersRef.current[id];
    if (timer) {
      clearInterval(timer);
      delete elapsedTimersRef.current[id];
    }
    setRuns((prev) => {
      if (!prev[id]) return prev;
      const next = { ...prev };
      delete next[id];
      return next;
    });

    // 记录墓碑并在网关侧删除，避免被会话同步重新合并回来。
    const target = avatars.find((a) => a.id === id);
    if (target) {
      setDeletedSessionIds((prev) =>
        prev.includes(target.sessionId) ? prev : [...prev, target.sessionId]
      );
      void invokeTauri<boolean>("delete_chat_session", {
        query: { sessionId: target.sessionId, channel: "web" },
      }).catch(() => {
        // 网关未连接时忽略：墓碑已能阻止其在本地重新出现。
      });
    }

    const remaining = avatars.filter((a) => a.id !== id);

    if (!keepTaskRecords) {
      setTaskHistory((prev) => prev.filter((task) => task.avatarId !== id));
      setSelectedTaskRunId((selected) => {
        if (!selected) return selected;
        return taskHistory.some((task) => task.runId === selected && task.avatarId === id)
          ? null
          : selected;
      });
    }

    const dropMaps = (alsoSeed?: string) => {
      setMessagesBySession((prev) => {
        const next = { ...prev };
        delete next[id];
        if (alsoSeed) next[alsoSeed] = [];
        return next;
      });
      setInputs((prev) => {
        const next = { ...prev };
        delete next[id];
        return next;
      });
      setSelectedSkills((prev) => {
        if (!prev[id]) return prev;
        const next = { ...prev };
        delete next[id];
        return next;
      });
      setAttachmentsBySession((prev) => {
        if (!prev[id]) return prev;
        const next = { ...prev };
        delete next[id];
        return next;
      });
      setSessionWorkspaceDirs((prev) => {
        if (!prev[id]) return prev;
        const next = { ...prev };
        delete next[id];
        return next;
      });
    };

    // 始终保留至少一个会话：删光时重建一个空的 Main。
    if (remaining.length === 0) {
      const fresh = {
        id: "main",
        name: "Main",
        sessionId: "omninova-chat-session",
        lastAt: formatTime(new Date()),
      };
      setAvatars([fresh]);
      dropMaps(fresh.id);
      // 重建的 Main 复用默认 sessionId，需从墓碑移除以恢复其同步。
      setDeletedSessionIds((prev) =>
        prev.filter((sid) => sid !== fresh.sessionId)
      );
      setActiveAvatarId(fresh.id);
      return;
    }

    setAvatars(remaining);
    dropMaps();
    if (id === activeAvatarId) {
      setActiveAvatarId(remaining[0].id);
    }
  };

  const handleRefreshHistory = useCallback(() => {
    void refreshGatewayStatus();
    if (gatewayStatus === "connected") {
      void syncChatSessions();
    }
    const session = avatars.find((a) => a.id === activeAvatarId);
    if (session) {
      void loadSessionHistory(activeAvatarId, session.sessionId, gatewayStatus === "connected");
    }
  }, [
    activeAvatarId,
    avatars,
    gatewayStatus,
    loadSessionHistory,
    syncChatSessions,
  ]);

  const handleCancel = useCallback(() => {
    if (!activeRunId) return;
    cancelledRef.current[activeAvatarId] = true;
    void invokeTauri<void>("cancel_agent_run", { runId: activeRunId }).catch((err) => {
      const message = err instanceof Error ? err.message : String(err);
      setError(`取消失败：${message}`);
    });
  }, [activeAvatarId, activeRunId]);

  const handleExportRiskJson = useCallback(() => {
    if (!lastRiskExport) return;
    const blob = new Blob([JSON.stringify(lastRiskExport, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = "contract-review.json";
    link.click();
    URL.revokeObjectURL(url);
  }, [lastRiskExport]);

  const handleStartContractReview = async () => {
    if (!modelReady) {
      setError("请先配置 AI 模型服务。");
      return;
    }
    if (gatewayStatus !== "connected") {
      const message = "Gateway 尚未连接，请启动 Gateway 后再开始审核。";
      setError(message);
      setContractReviewUiByAvatar((prev) => ({
        ...prev,
        [activeAvatarId]: { stage: "failed", error: message },
      }));
      return;
    }
    const pending = attachmentsBySession[activeAvatarId] ?? [];
    const paths = pending.map((item) => item.sourcePath).filter((path): path is string => Boolean(path));
    if (!paths.length) {
      setError("请先上传合同文件（DOCX、文字层 PDF、TXT 或 MD）。");
      return;
    }
    setLastRiskExport(null);
    setContractReviewUiByAvatar((prev) => ({
      ...prev,
      [activeAvatarId]: { stage: "preparing" },
    }));
    setError(null);
    try {
      const prepared = await invokeTauri<PreparedContractReview>("prepare_contract_review", {
        paths,
        extraInstructions: contractExtraInstructions,
        selectedEngine: selectedContractEngine,
      });
      setLastRiskExport(prepared.export);
      setContractReviewUiByAvatar((prev) => ({
        ...prev,
        [activeAvatarId]: { stage: "reviewing" },
      }));
      await handleSend({
        text: prepared.prompt,
        displayText: `合同智能审核：${pending.map((item) => item.name).join("、")}`,
        includeAttachments: false,
        contractEngineName: prepared.engine.name,
      });
    } catch (reason) {
      const displayError = friendlyContractReviewError(reason);
      setContractReviewUiByAvatar((prev) => ({
        ...prev,
        [activeAvatarId]: { stage: "failed", error: displayError },
      }));
      setError(displayError);
      if (import.meta.env.DEV) {
        console.warn("[contract-review] prepare_failed", reason);
      }
    }
  };

  async function handleSend(opts?: {
    text?: string;
    displayText?: string;
    includeAttachments?: boolean;
    contractEngineName?: string;
  }) {
    // 绑定到「发送时」的会话，使后续状态更新只作用于该会话，
    // 即使用户中途切换到其它会话也互不影响。
    const avatarId = activeAvatarId;
    const targetSessionId = sessionId;
    const selectedForSend = opts?.text == null ? selectedSkills[avatarId] : undefined;
    if (import.meta.env.DEV) {
      console.log(
        `[skill-ui] handle_send text_len=${input.trim().length} skill_id=${selectedForSend?.id ?? "none"}`
      );
    }
    const resolved = opts?.text == null
      ? resolveComposerSend(input.trim(), selectedForSend, commandPalette)
      : { text: opts.text.trim(), invocations: [], local: undefined, systemTool: undefined };
    if (resolved.local === "help") {
      setActiveInput("");
      setMessagesBySession((prev) => ({
        ...prev,
        [avatarId]: [...(prev[avatarId] ?? []), { role: "assistant", content: COMPOSER_HELP_TEXT }],
      }));
      return;
    }
    if (resolved.local === "skills") {
      setActiveInput("");
      onOpenSettings?.("skills");
      return;
    }
    if (resolved.systemTool === "contract-review") {
      const tool = commandPalette.system.find((item) => item.id === "tool:contract-review");
      if (tool) applyPaletteSelection(tool);
      setActiveInput(resolved.text);
      return;
    }
    const text = resolved.text;
    const skillInvocations = resolved.invocations;
    if (selectedForSend) {
      try {
        if (import.meta.env.DEV) {
          console.log(`[skill-ui] validating skill id=${selectedForSend.id}`);
        }
        const latest = await invokeTauri<CommandPalette>("list_command_palette", { query: "" });
        const live = latest.skills.find((item) => item.id === selectedForSend.id && item.enabled);
        if (!live) {
          setError("所选技能当前不可用，请重新选择。");
          if (import.meta.env.DEV) {
            console.warn(`[skill-ui] stale_skill id=${selectedForSend.id}`);
          }
          setSelectedSkills((prev) => {
            if (!prev[avatarId]) return prev;
            const next = { ...prev };
            delete next[avatarId];
            return next;
          });
          return;
        }
      } catch (reason) {
        const detail = reason instanceof Error ? reason.message : String(reason);
        setError(`技能列表刷新失败，请重试发送。${detail ? `（${detail}）` : ""}`);
        if (import.meta.env.DEV) {
          console.error(`[skill-ui] skill_validation_failed id=${selectedForSend.id}`, reason);
        }
        return;
      }
    }
    // bug#1/#2：附件在发送时才拼接进正文；聊天气泡只显示简洁的附件名。
    let pendingAttachments = opts?.includeAttachments === false
      ? []
      : attachmentsBySession[avatarId] ?? [];
    if (selectedSystemTools[avatarId]?.id === "tool:contract-review" && opts?.text == null) {
      setError("合同智能审核已激活，请在审核面板中上传合同并点击开始审核。");
      return;
    }
    const active = runsRef.current[avatarId]?.runId ?? (avatarId === activeAvatarId ? activeRunIdRef.current : null);
    if (active && terminalRunIdsRef.current.has(active)) {
      finishRun(active);
    } else if (active) {
      setError("当前 Agent 仍在执行，请等待完成或取消。");
      return;
    }
    // Selecting a skill is metadata, not a user request. Do not let Enter
    // bypass the disabled send button and start an empty agent run.
    if (!text && pendingAttachments.length === 0) return;
    // Generate a run_id for real-time event correlation.
    const runId = crypto.randomUUID();
    runAvatarIdsRef.current[runId] = avatarId;
    if (import.meta.env.DEV) {
      console.log("[agent-run-id]", runId);
    }

    if (gatewayStatus !== "connected") {
      setError("网关未连接，请先在侧栏「设置」中启动网关后再发送消息");
      return;
    }

    const pathAttachments = pendingAttachments.filter(
      (attachment): attachment is ComposerAttachment & { sourcePath: string } =>
        Boolean(attachment.sourcePath)
    );
    if (pathAttachments.length) {
      if (!activeWorkspaceDir) {
        setError("拖入的本地文件需要先选择 Workspace。文件不会被移除，选择后可直接重新发送。");
        return;
      }
      try {
        const prepared = await prepareComposerAttachments(
          pathAttachments.map((attachment) => attachment.sourcePath),
          activeWorkspaceDir,
          targetSessionId,
        );
        let preparedIndex = 0;
        pendingAttachments = pendingAttachments.map((attachment) => {
          if (!attachment.sourcePath) return attachment;
          const mounted = prepared[preparedIndex];
          preparedIndex += 1;
          if (!mounted) return attachment;
          return {
            ...attachment,
            kind: mounted.kind === "image" ? "image" : mounted.kind === "text" ? "text" : "other",
            content: mounted.content,
            note: mounted.note,
            mountedPath: mounted.workspaceRelativePath,
          };
        });
        setAttachmentsBySession((prev) => ({ ...prev, [avatarId]: pendingAttachments }));
      } catch (reason) {
        setError(`附件挂载失败：${reason instanceof Error ? reason.message : String(reason)}`);
        return;
      }
    }

    const attachmentBlock = pendingAttachments
      .map((attachment) => attachment.content.trim())
      .filter(Boolean)
      .join("\n\n");
    const outgoingText = [text, attachmentBlock].filter(Boolean).join("\n\n");
    const displayText = opts?.displayText ?? (pendingAttachments.length
      ? `${text ? `${text}\n\n` : ""}附件：${pendingAttachments.map((attachment) => attachment.name).join("、")}`
      : text || (selectedForSend ? `使用技能 ${selectedForSend.displayName}` : text));

    // 本地维护该会话的步骤列表，避免依赖共享状态。
    let localSteps: ExecutionStep[] = [
      { title: "准备请求", status: "done", detail: `会话：${targetSessionId}` },
      { title: "路由选择", status: "running", detail: "正在选择 Agent / Provider / Model" },
    ];
    const writeSteps = (steps: ExecutionStep[]) => {
      localSteps = steps;
      setRuns((prev) => {
        const next = prev[avatarId]
          ? { ...prev, [avatarId]: { ...prev[avatarId], steps } }
          : prev;
        runsRef.current = next;
        return next;
      });
    };
    const updateStep = (
      title: string,
      status: ExecutionStep["status"],
      detail?: string
    ) => {
      const idx = localSteps.findIndex((step) => step.title === title);
      const nextStep: ExecutionStep = { title, status, detail };
      writeSteps(
        idx < 0
          ? [...localSteps, nextStep]
          : localSteps.map((step, i) => (i === idx ? nextStep : step))
      );
    };
    if (opts?.contractEngineName) {
      contractReviewRunAvatarIdsRef.current[runId] = avatarId;
    }
    setActiveInput("");
    setSelectedSkills((prev) => {
      if (!prev[avatarId]) return prev;
      const next = { ...prev };
      delete next[avatarId];
      return next;
    });
    if (!opts?.contractEngineName) {
      setAttachmentsBySession((prev) => {
        if (!prev[avatarId]?.length) return prev;
        const next = { ...prev };
        delete next[avatarId];
        return next;
      });
    }
    setError(null);
    cancelledRef.current[avatarId] = false;
    activeRunIdRef.current = runId;
    setRuns((prev) => {
      const next = { ...prev, [avatarId]: { elapsedSec: 0, steps: localSteps, runId } };
      runsRef.current = next;
      return next;
    });

    stickToBottomRef.current = true;
    setMessagesBySession((prev) => ({
      ...prev,
      [avatarId]: [
        ...(prev[avatarId] ?? []),
        {
          role: "user",
          content: displayText,
          ...(selectedForSend
            ? { skillId: selectedForSend.id, skillName: selectedForSend.displayName }
            : {}),
          ...(opts?.contractEngineName
            ? { toolId: "tool:contract-review", toolName: "合同智能审核", contractEngineName: opts.contractEngineName }
            : {}),
        },
      ],
    }));
    setAvatars((prev) =>
      prev.map((a) =>
        a.id === avatarId ? { ...a, lastAt: formatTime(new Date()) } : a
      )
    );

    recordTaskStart({
      runId,
      title: displayText,
      avatarId,
      agentName: avatars.find((a) => a.id === avatarId)?.name ?? avatarId,
      sessionId: targetSessionId,
      workspacePath: activeWorkspaceDir ?? undefined,
      status: "running",
      startedAt: Date.now(),
      activity: [{
        at: Date.now(),
        label: "任务已提交，正在准备执行",
        tone: "info",
        kind: "lifecycle",
        status: "waiting",
      }, ...pendingAttachments
        .filter((attachment) => attachment.mountedPath)
        .map((attachment) => ({
          at: Date.now(),
          label: `输入附件已挂载：${attachment.name}`,
          tone: "success" as const,
          kind: "file" as const,
          status: "completed" as const,
          path: attachment.mountedPath,
          detail: attachment.note,
        }))],
    });

    elapsedTimersRef.current[avatarId] = setInterval(() => {
      setRuns((prev) => {
        const next = prev[avatarId]
          ? {
              ...prev,
              [avatarId]: {
                ...prev[avatarId],
                elapsedSec: prev[avatarId].elapsedSec + 1,
              },
            }
          : prev;
        runsRef.current = next;
        return next;
      });
    }, 1000);

    let route: RouteDecision | null = null;

    try {
      const parsedSelection = parseModelSelection(selectedModel);
      const metadata: Record<string, unknown> = {
        preferred_provider:
          selectedModel === "auto" ? undefined : parsedSelection.providerId,
        preferred_model:
          selectedModel === "auto" ? undefined : parsedSelection.model,
        max_mode: maxMode || undefined,
        reasoning_enabled: maxMode || undefined,
        // Session-scoped temporary workspace (takes highest priority in the backend).
        // Clears when the user closes the app or starts a new session.
        ...(sessionWorkspaceDir ? { workspace_dir: sessionWorkspaceDir } : {}),
        // Run ID for real-time event correlation between frontend and backend.
        run_id: runId,
        ...(skillInvocations.length ? { skill_invocations: skillInvocations } : {}),
        ...(opts?.contractEngineName
          ? { system_tool: "tool:contract-review", contract_review_engine: opts.contractEngineName }
          : {}),
      };
      if (import.meta.env.DEV) {
        console.log(
          `[skill-ui] invoking skill_invocations=${skillInvocations.map((item) => item.skillId).join(",") || "none"}`
        );
      }

      if (desktopVisionOn && desktopVisionMaster && isTauriEnvironment()) {
        updateStep("桌面视觉", "running", "正在截取主屏幕…");
        try {
          const shot = await invokeTauri<DesktopScreenshotPayload>("capture_desktop_screenshot", {
            maxDimensionPx: desktopVisionMaxPx,
          });
          metadata.desktop_vision = true;
          metadata.desktop_vision_images = [shot.dataUrl];
          updateStep(
            "桌面视觉",
            "done",
            `已截取 ${shot.width}×${shot.height}，将随消息发送给视觉模型`
          );
        } catch (err) {
          const msg = err instanceof Error ? err.message : String(err);
          updateStep("桌面视觉", "error", msg);
          setError(`桌面截图失败：${msg}`);
          appendTaskActivity(runId, `桌面截图失败：${msg}`, "error");
          finalizeTask(runId, "failed", `桌面截图失败：${msg}`);
          finishRun(runId);
          setMessagesBySession((prev) => ({
            ...prev,
            [avatarId]: (prev[avatarId] ?? []).slice(0, -1),
          }));
          setInputs((prev) => ({ ...prev, [avatarId]: text }));
          if (selectedForSend) {
            setSelectedSkills((prev) => ({ ...prev, [avatarId]: selectedForSend }));
          }
          if (pendingAttachments.length) {
            setAttachmentsBySession((prev) => ({
              ...prev,
              [avatarId]: pendingAttachments,
            }));
          }
          return;
        }
      }

      const payload = {
        channel: "web" as const,
        text: outgoingText,
        sessionId: targetSessionId,
        userId: USER_ID,
        metadata,
        ...(skillInvocations.length ? { skillInvocations } : {}),
      };
      route = await invokeTauri<RouteDecision>("route_inbound_message", {
        payload,
      }).catch((reason) => {
        if (import.meta.env.DEV) {
          console.warn("[skill-ui] route_preview_failed", reason);
        }
        return null;
      });
      if (route) {
        updateStep(
          "路由选择",
          "done",
          `Agent: ${route.agent_name}${route.provider ? ` · Provider: ${route.provider}` : ""}${route.model ? ` · Model: ${route.model}` : ""}`
        );
      } else {
        updateStep("路由选择", "done", "路由预览不可用，交由网关处理");
      }
      updateStep("Agent 执行", "running", "正在调用模型和工具；界面不设置超时，会持续等待后端完成");
      // Long-running notice only. This must not finish the run.
      safetyTimersRef.current[runId] = setTimeout(() => {
        if (import.meta.env.DEV) {
          console.warn("[handleSend] LONG RUN still running run_id=" + runId);
        }
        setRuns((prev) => {
          const current = prev[avatarId];
          if (!current || current.runId !== runId) return prev;
          const next = { ...prev, [avatarId]: { ...current, longRunning: true } };
          runsRef.current = next;
          return next;
        });
        updateStep("Agent 仍在执行", "running", "Agent 仍在生成页面代码，耗时较长，可继续等待或点击取消。");
      }, 30_000);
        if (import.meta.env.DEV) {
          console.log("[handleSend] BEFORE_INVOKE run_id=" + runId + " ts=" + Date.now());
        }
        void invokeTauri<GatewayInboundResponse>("process_inbound_message_streaming", {
          payload,
        }).then((result) => {
          if (import.meta.env.DEV) {
            console.log("[handleSend] AFTER_INVOKE run_id=" + runId + " ts=" + Date.now());
            console.log("[handleSend] resolved run_id=" + runId + " reply_len=" + (result?.reply?.length ?? -1));
          }

          settleRunFromCommand(runId, avatarId, { reply: result?.reply ?? "" });
        })
        .catch((e) => {
          const message = e instanceof Error ? e.message : String(e);
          if (import.meta.env.DEV) {
            console.error("[handleSend] process_inbound_message_streaming error run_id=" + runId + " error=" + message);
          }
          settleRunFromCommand(runId, avatarId, { error: message });
        });
    } catch (e) {
      if (import.meta.env.DEV) {
        console.error("[handleSend] outer-error run_id=" + runId + " error=" + (e instanceof Error ? e.message : String(e)));
      }
      const message = e instanceof Error ? e.message : String(e);
      const displayError = opts?.contractEngineName
        ? friendlyContractReviewError(message)
        : `任务启动失败：${message}`;
      setError(displayError);
      if (opts?.contractEngineName) {
        setContractReviewUiByAvatar((prev) => ({
          ...prev,
          [avatarId]: { stage: "failed", error: displayError },
        }));
        delete contractReviewRunAvatarIdsRef.current[runId];
      }
      appendTaskActivity(runId, displayError, "error");
      finalizeTask(runId, "failed", displayError);
      finishRun(runId);
    } finally {
      // outer finally (safety timer cleanup)
    }
  }

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    const composing = e.nativeEvent.isComposing || e.key === "Process";
    if (paletteVisible && !composing) {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        if (!paletteRowItems.length) return;
        setPaletteIndex((index) => (index + 1) % paletteRowItems.length);
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        if (!paletteRowItems.length) return;
        setPaletteIndex((index) => (index - 1 + paletteRowItems.length) % paletteRowItems.length);
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        setPaletteDismissedToken(commandToken?.token ?? "/");
        return;
      }
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        const item = paletteRowItems[paletteIndex];
        if (item) applyPaletteSelection(item);
        return;
      }
    }
    if (e.key === "Backspace" && !input && selectedSkill && !composing) {
      setSelectedSkills((prev) => {
        if (!prev[activeAvatarId]) return prev;
        const next = { ...prev };
        delete next[activeAvatarId];
        return next;
      });
      return;
    }
    if (e.key === "Enter" && !e.shiftKey && !composing) {
      e.preventDefault();
      void handleSend();
    }
  };

  const updateComposerHeight = useCallback((height: number) => {
    const next = Math.min(COMPOSER_MAX_HEIGHT, Math.max(COMPOSER_MIN_HEIGHT, height));
    setComposerInputHeight(next);
    localStorage.setItem(COMPOSER_HEIGHT_STORAGE_KEY, String(next));
  }, []);

  const handleComposerResizePointerDown = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      event.preventDefault();
      const startY = event.clientY;
      const startHeight = composerInputHeight;
      const pointerId = event.pointerId;
      event.currentTarget.setPointerCapture(pointerId);

      const handlePointerMove = (moveEvent: PointerEvent) => {
        // 输入区固定在窗口底部，向上拖动表示增加高度。
        updateComposerHeight(startHeight + startY - moveEvent.clientY);
      };
      const handlePointerEnd = () => {
        window.removeEventListener("pointermove", handlePointerMove);
        window.removeEventListener("pointerup", handlePointerEnd);
        window.removeEventListener("pointercancel", handlePointerEnd);
      };

      window.addEventListener("pointermove", handlePointerMove);
      window.addEventListener("pointerup", handlePointerEnd, { once: true });
      window.addEventListener("pointercancel", handlePointerEnd, { once: true });
    },
    [composerInputHeight, updateComposerHeight]
  );

  const handleComposerResizeKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLDivElement>) => {
      if (event.key === "ArrowUp") {
        event.preventDefault();
        updateComposerHeight(composerInputHeight + 16);
      } else if (event.key === "ArrowDown") {
        event.preventDefault();
        updateComposerHeight(composerInputHeight - 16);
      } else if (event.key === "Home") {
        event.preventDefault();
        updateComposerHeight(COMPOSER_MIN_HEIGHT);
      } else if (event.key === "End") {
        event.preventDefault();
        updateComposerHeight(COMPOSER_MAX_HEIGHT);
      }
    },
    [composerInputHeight, updateComposerHeight]
  );

  const addAttachments = useCallback(
    (items: ComposerAttachment[]) => {
      if (!items.length) return;
      setAttachmentsBySession((prev) => ({
        ...prev,
        [activeAvatarId]: [...(prev[activeAvatarId] ?? []), ...items],
      }));
    },
    [activeAvatarId]
  );

  const removeAttachment = useCallback(
    (id: string) => {
      setAttachmentsBySession((prev) => ({
        ...prev,
        [activeAvatarId]: (prev[activeAvatarId] ?? []).filter((a) => a.id !== id),
      }));
    },
    [activeAvatarId]
  );

  /** Tauri 桌面：先保留原始路径；发送时再挂载到当时生效的 Workspace。 */
  const mergePathsIntoInput = useCallback(
    async (paths: string[]) => {
      const limited = paths.slice(0, DROP_FILES_MAX_COUNT);
      const items: ComposerAttachment[] = limited.map((path) => {
        const name = workspaceBasename(path);
        const ext = fileExtensionLower(name);
        const isImage = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg"].includes(ext);
        return {
          id: nextAttachmentId(),
          name,
          kind: isImage ? "image" : TEXT_FILE_EXTENSIONS.has(ext) ? "text" : "other",
          content: "",
          note: activeWorkspaceDir ? "发送时挂载到当前 Workspace" : "请先选择 Workspace",
          sourcePath: path,
        };
      });
      addAttachments(items);
    },
    [activeWorkspaceDir, addAttachments]
  );

  /** 浏览器预览等非 Tauri 环境：用 File API 读取 */
  const mergeDroppedIntoInput = useCallback(
    async (files: FileList | readonly File[]) => {
      const items = await buildAttachmentsFromFiles(files);
      addAttachments(items);
    },
    [addAttachments]
  );

  /** bug#2：粘贴图片/文件进输入框 */
  const handleComposerPaste = useCallback(
    (e: React.ClipboardEvent<HTMLTextAreaElement>) => {
      const files = e.clipboardData?.files;
      if (!files?.length) return;
      e.preventDefault();
      if (sending) return;
      void mergeDroppedIntoInput(files).catch((err) => {
        setError(err instanceof Error ? err.message : String(err));
      });
    },
    [mergeDroppedIntoInput, sending]
  );

  /** Tauri 桌面：WKWebView 会拦截 HTML5 拖放，需用原生 onDragDropEvent 拿路径 */
  useEffect(() => {
    if (!isTauriEnvironment()) return;

    let disposed = false;
    let unlisten: (() => void) | undefined;

    void (async () => {
      const { getCurrentWebview } = await import("@tauri-apps/api/webview");
      const webview = getCurrentWebview();
      unlisten = await webview.onDragDropEvent(async (event) => {
        if (disposed) return;
        const { payload } = event;
        if (payload.type === "enter" || payload.type === "over") {
          setComposerDragActive(true);
          return;
        }
        if (payload.type === "leave") {
          setComposerDragActive(false);
          return;
        }
        if (payload.type !== "drop") return;

        setComposerDragActive(false);
        if (sending) return;
        if (!payload.paths?.length) return;

        try {
          await mergePathsIntoInput(payload.paths);
        } catch (err) {
          setError(err instanceof Error ? err.message : String(err));
        }
      });
    })();

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [mergePathsIntoInput, sending]);

  const handleAttachTauri = useCallback(async () => {
    try {
      const paths = await pickComposerAttachmentPaths();
      if (!paths.length) return;
      await mergePathsIntoInput(paths);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }, [mergePathsIntoInput]);

  /**
   * Chat-page Workspace button — two modes:
   * 1. Shift+click  → save to agent config (persistent, survives session)
   * 2. plain click  → session-scoped temporary workspace (highest priority,
   *                    resets on app restart / new chat session)
   *
   * Requirement #11: "聊天页点 Workspace 按钮选目录，只作为当前会话临时
   * workspace，不要强制覆盖 Agent 默认 workspace，除非用户点击'保存到该 Agent'。"
   */
  const handleChooseWorkspace = useCallback(async (event?: React.MouseEvent) => {
    const saveToAgent = event?.shiftKey ?? false;
    setWorkspaceMenuOpen(false);
    try {
      let dir: string | null = null;
      if (isTauriEnvironment()) {
        const selected = await open({
          directory: true,
          multiple: false,
          title: saveToAgent ? "选择该 Agent 的 Workspace 目录" : "选择临时 Workspace 目录",
        });
        if (selected == null) return;
        dir = selected as string;
      } else {
        const typed = window.prompt(
          saveToAgent ? "输入该 Agent 的 Workspace 绝对路径" : "输入临时 Workspace 绝对路径"
        );
        dir = typed?.trim() || null;
        if (!dir) return;
      }

      if (saveToAgent) {
        // Save to the agent config (persistent).
        const current = await invokeTauri<Config>("get_setup_config").catch(() => null);
        const next: Config = {
          ...(current ?? ({} as Config)),
          agent: {
            ...(current?.agent ?? {}),
            workspace_dir: dir,
          } as AgentPersonaConfig,
        };
        const saveResult = await invokeTauri<{ gateway_restarted: boolean }>(
          "save_setup_config",
          { config: next }
        );
        setWorkspaceDir(dir);
        setWorkspaceSource("agent");
        setSessionWorkspaceDirs((prev) => {
          if (!prev[activeAvatarId]) return prev;
          const next = { ...prev };
          delete next[activeAvatarId];
          return next;
        });
        if (saveResult?.gateway_restarted) {
          await new Promise<void>((resolve) => setTimeout(resolve, 1000));
          const status = await invokeTauri<GatewayStatus>("gateway_status").catch(() => null);
          if (status) {
            setGatewayStatus(status.running ? "connected" : "disconnected");
          }
          const refreshed = await invokeTauri<Config>("get_setup_config").catch(() => null);
          if (refreshed) {
            const agentWorkspace = refreshed.agent?.workspace_dir ?? null;
            const globalWorkspace = refreshed.workspace_dir ?? null;
            setWorkspaceDir(agentWorkspace || globalWorkspace || null);
            setWorkspaceSource(agentWorkspace ? "agent" : globalWorkspace ? "global" : null);
            setWorkspaceStatus(refreshed.workspace_status ?? null);
          }
        }
      } else {
        // Session-scoped temporary workspace (no save, no gateway restart).
        setSessionWorkspaceDirs((prev) => ({ ...prev, [activeAvatarId]: dir }));
      }
    } catch (err) {
      setError(
        `选择 Workspace 失败：${err instanceof Error ? err.message : String(err)}`
      );
    }
  }, [activeAvatarId]);

  const handleOpenWorkspace = useCallback(async () => {
    setWorkspaceMenuOpen(false);
    if (!activeWorkspaceDir) {
      setError("请先选择 Workspace。");
      return;
    }
    try {
      await invokeTauri<void>("open_workspace_dir", { path: activeWorkspaceDir });
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }, [activeWorkspaceDir]);

  const handleClearSessionWorkspace = useCallback(() => {
    setWorkspaceMenuOpen(false);
    setSessionWorkspaceDirs((prev) => {
      if (!prev[activeAvatarId]) return prev;
      const next = { ...prev };
      delete next[activeAvatarId];
      return next;
    });
  }, [activeAvatarId]);

  const handleComposerDragEnter = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (!e.dataTransfer.types.includes("Files")) return;
    setComposerDragActive(true);
  }, []);

  const handleComposerDragLeave = useCallback((e: React.DragEvent<HTMLDivElement>) => {
    const next = e.relatedTarget as Node | null;
    if (next && e.currentTarget.contains(next)) return;
    setComposerDragActive(false);
  }, []);

  /** 子区域（尤其是 textarea）必须 preventDefault，否则浏览器不会触发 drop */
  const handleComposerDragOverFiles = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (e.dataTransfer.types.includes("Files")) {
      e.dataTransfer.dropEffect = "copy";
    }
  }, []);

  const handleComposerDropFiles = useCallback(
    async (e: React.DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      setComposerDragActive(false);
      // Tauri 下 dataTransfer.files 常为空，由 onDragDropEvent 处理
      if (isTauriEnvironment()) return;
      if (sending) return;
      const files = e.dataTransfer.files;
      if (!files?.length) return;
      try {
        await mergeDroppedIntoInput(files);
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      }
    },
    [mergeDroppedIntoInput, sending]
  );

  const handleAttachFilesChange = useCallback(
    async (e: React.ChangeEvent<HTMLInputElement>) => {
      const files = e.target.files;
      e.target.value = "";
      if (!files?.length) return;
      try {
        await mergeDroppedIntoInput(files);
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      }
    },
    [mergeDroppedIntoInput]
  );

  const gatewayPort = (() => {
    try {
      const u = new URL(gatewayUrl || "http://127.0.0.1:10809");
      return u.port || (u.protocol === "https:" ? "443" : "80");
    } catch {
      return "—";
    }
  })();

  const quickPrompts = [
    { label: "处理任务", text: "请帮我处理当前工作区中的任务，并给出可执行步骤。" },
    { label: "持续执行", text: "请持续执行直到任务完成，中途如需确认请说明。" },
    { label: "多智能体并行", text: "请说明如何在本机配置多 Agent 并行与路由。" },
  ];
  const modelReady = availableProviders.length > 0;
  const workspaceReady = Boolean(activeWorkspaceDir);
  const gatewayReady = gatewayStatus === "connected";
  const showOnboarding =
    taskHistory.length === 0 &&
    messages.length === 0 &&
    !(modelReady && workspaceReady && gatewayReady);

  return (
    <div className="chat-layout">
      <div className="chat-body">
        <aside className="chat-sidebar">
          <button
            type="button"
            className="chat-new-chat-pill"
            onClick={handleAddAvatar}
          >
            <UiIcon name="plus" size={15} />
            <span>新智能体</span>
          </button>
          <section className="chat-sidebar-section">
            <h3 className="chat-sidebar-heading">智能体</h3>
            <ul className="chat-avatar-list">
              {avatars.map((a) => (
                <li
                  key={a.id}
                  className={`chat-avatar-row ${a.id === activeAvatarId ? "is-active" : ""}`}
                >
                  <button
                    type="button"
                    className={`chat-avatar-item ${a.id === activeAvatarId ? "is-active" : ""}`}
                    onClick={() => {
                      setActiveAvatarId(a.id);
                      setSelectedTaskRunId(null);
                    }}
                  >
                    <span className="chat-avatar-icon"><UiIcon name="agent" size={15} /></span>
                    <span className="chat-avatar-name">{a.name}</span>
                    {runs[a.id] ? (
                      <span
                        className="chat-avatar-running"
                        title="该会话正在运行"
                        aria-label="运行中"
                      />
                    ) : (
                      <span className="chat-avatar-time">{a.lastAt}</span>
                    )}
                  </button>
                  <button
                    type="button"
                    className="chat-avatar-edit"
                    title="重命名智能体"
                    aria-label={`重命名智能体 ${a.name}`}
                    onClick={(e) => {
                      e.stopPropagation();
                      handleRenameAvatar(a.id);
                    }}
                  >
                    <UiIcon name="edit" size={13} />
                  </button>
                  <button
                    type="button"
                    className="chat-avatar-delete"
                    title="删除智能体"
                    aria-label={`删除智能体 ${a.name}`}
                    onClick={(e) => {
                      e.stopPropagation();
                      handleDeleteAvatar(a.id);
                    }}
                  >
                    <UiIcon name="delete" size={14} />
                  </button>
                </li>
              ))}
            </ul>
          </section>
          <nav className="chat-sidebar-tabs">
            <button
              type="button"
              className={sidebarTab === "avatars" ? "is-active" : ""}
              onClick={() => setSidebarTab("avatars")}
            >
              对话
            </button>
            <button
              type="button"
              className={sidebarTab === "history" ? "is-active" : ""}
              onClick={() => setSidebarTab("history")}
            >
              任务
            </button>
          </nav>
          {sidebarTab === "history" ? (
            <section className="chat-sidebar-section">
              <div className="chat-history-head">
                <h3 className="chat-sidebar-heading">任务列表</h3>
                {taskHistory.some(
                  (task) =>
                    task.status !== "running" &&
                    task.status !== "needs_approval" &&
                    task.status !== "waiting_input"
                ) ? (
                  <button
                    type="button"
                    className="chat-history-clear"
                    onClick={handleClearTaskHistory}
                    title="清空已结束的历史任务"
                  >
                    清空
                  </button>
                ) : null}
              </div>
              <ul className="chat-task-list">
                {taskHistory.length === 0 ? (
                  <li className="chat-avatar-time">暂无历史任务</li>
                ) : (
                  taskHistory.map((task) => {
                    const exists = avatars.some((a) => a.id === task.avatarId);
                    return (
                      <li key={task.runId}>
                        <button
                          type="button"
                          className={`chat-task-item${selectedTaskRunId === task.runId ? " is-selected" : ""}${taskNeedsAttention(task.status) ? " needs-attention" : ""}`}
                          title={
                            exists
                              ? "跳转到该任务所属会话"
                              : "原会话已删除，任务过程与成果记录仍可查看"
                          }
                          onClick={() => {
                            if (exists) setActiveAvatarId(task.avatarId);
                            setSelectedTaskRunId(task.runId);
                          }}
                        >
                          <div className="chat-task-row">
                            <span
                              className={`chat-task-badge chat-task-badge--${task.status}`}
                            >
                              {taskStatusLabel(task.status)}
                            </span>
                            <span className="chat-task-title">
                              {task.title}
                            </span>
                          </div>
                          <div className="chat-task-meta">
                            <span className="chat-task-agent">
                              {task.agentName}
                            </span>
                            <span className="chat-task-time">
                              {formatTaskTime(task.startedAt)}
                              {task.durationMs != null
                                ? ` · ${formatDuration(task.durationMs)}`
                                : ""}
                            </span>
                          </div>
                          <div className="chat-task-result">
                            {task.attentionReason || task.resultPreview || "尚未产生结果"}
                          </div>
                          <div className="chat-task-signals">
                            <span>
                              <UiIcon name="file" size={11} />
                              {task.changedFiles?.length ?? 0} 个文件
                            </span>
                            {taskNeedsAttention(task.status) ? (
                              <span className="chat-task-attention">
                                <UiIcon name="warning" size={11} /> 需要处理
                              </span>
                            ) : (
                              <span><UiIcon name="check" size={11} /> 无需确认</span>
                            )}
                          </div>
                        </button>
                      </li>
                    );
                  })
                )}
              </ul>
            </section>
          ) : null}
        </aside>

        <main className="chat-main">
          <div className="chat-main-toolbar">
            <div className="chat-main-toolbar-spacer" />
            <div className="chat-target-pill">
              <span className="chat-target-pill-icon" aria-hidden>
                <img src={omninovalLogo} alt="" />
              </span>
              <span className="chat-target-pill-text">
                当前智能体：{activeSession?.name ?? "Main"}
              </span>
            </div>
            <div className="chat-main-toolbar-actions">
              <button
                type="button"
                className="chat-icon-btn"
                title="打开任务检查器"
                aria-label="打开任务检查器"
                aria-expanded={inspectorOpen}
                onClick={() => openInspector("process")}
              >
                <UiIcon name="menuUnfold" size={15} />
              </button>
              <button
                type="button"
                className="chat-icon-btn"
                title="从网关重新加载当前会话历史"
                aria-label="从网关重新加载当前会话历史"
                disabled={historyLoading || gatewayStatus !== "connected"}
                onClick={() => void handleRefreshHistory()}
              >
                <UiIcon name="history" size={15} />
              </button>
              <button
                type="button"
                className="chat-icon-btn"
                title="刷新网关状态"
                aria-label="刷新网关状态"
                onClick={() => void refreshGatewayStatus()}
              >
                <UiIcon name="reload" size={15} />
              </button>
              <span
                className="chat-main-toolbar-status"
                aria-live="polite"
                aria-busy={historyLoading || sending}
              >
                {historyLoading ? (
                  <span className="chat-history-loading">加载历史…</span>
                ) : sending ? (
                  <span className="chat-typing-badge">
                    正在回复 {elapsedSec}s
                  </span>
                ) : (
                  <span className="chat-toolbar-status-placeholder" aria-hidden>
                    &nbsp;
                  </span>
                )}
              </span>
            </div>
          </div>

          <TaskStatusBar
            task={activeTask}
            elapsedSec={sending ? elapsedSec : undefined}
            onOpenInspector={openInspector}
          />

          {workspaceStatus && workspaceStatus.state !== "ok" && !sessionWorkspaceDir ? (
            <div
              className={`chat-workspace-banner chat-workspace-banner--${workspaceStatus.state}`}
              role="status"
            >
              <strong>Workspace 未就绪：</strong>
              {workspaceStatus.message}
              {workspaceStatus.path ? (
                <code className="chat-workspace-banner-path">{workspaceStatus.path}</code>
              ) : null}
              <button
                type="button"
                className="chat-workspace-banner-action"
                onClick={() => void handleChooseWorkspace()}
                disabled={sending}
              >
                {workspaceStatus.state === "unselected" ? "选择目录" : "重新选择"}
              </button>
            </div>
          ) : null}

          <div
            ref={messagesScrollRef}
            className="chat-messages"
            onScroll={handleMessagesScroll}
          >
            {showOnboarding ? (
              <TaskOnboarding
                modelReady={modelReady}
                workspaceReady={workspaceReady}
                gatewayReady={gatewayReady}
                selectedModel={selectedModel}
                providers={availableProviders}
                onModelChange={setSelectedModel}
                maxMode={maxMode}
                onMaxModeChange={setMaxMode}
                defaultProvider={defaultProviderId}
                defaultModel={defaultModelId}
                onConfigureModel={() => onOpenSettings?.("providers")}
                onChooseWorkspace={() => void handleChooseWorkspace()}
                onStartGateway={() => void handleStartGateway()}
                gatewayBusy={gatewayStarting}
              />
            ) : null}
            {!showOnboarding ? (
              <TaskDeliverable task={activeTask} onOpenInspector={openInspector} />
            ) : null}
            {showOnboarding ? null : historyLoading && messages.length === 0 ? (
              <div className="chat-hero">
                <p className="chat-hero-sub">正在加载会话历史…</p>
              </div>
            ) : messages.length === 0 ? (
              <div className="chat-hero">
                <h1 className="chat-hero-title">我能为你做些什么？</h1>
                <p className="chat-hero-sub">
                  与智能体 {activeSession?.name ?? "Main"} 对话；历史记录会保存在网关并在下次打开时恢复。
                </p>
                <div className="chat-quick-pills">
                  {quickPrompts.map((q) => (
                    <button
                      key={q.label}
                      type="button"
                      className="chat-quick-pill"
                      onClick={() => setActiveInput(q.text)}
                    >
                      {q.label}
                    </button>
                  ))}
                </div>
              </div>
            ) : (
              messages.map((msg, i) => {
                const isContractReport = contractReportMessageIndexes.has(i);
                const contractRequest = isContractReport ? messages[i - 1] : undefined;
                return (
                <div
                  key={i}
                  className={`chat-bubble chat-bubble-${msg.role}${isContractReport ? " chat-bubble-contract-review" : ""}`}
                >
                  <div className="chat-bubble-content">
                    {isContractReport ? (
                      <ContractReviewReport
                        content={msg.content}
                        engineName={contractRequest?.contractEngineName}
                        exportData={i === latestContractReportIndex ? lastRiskExport : undefined}
                        onExport={i === latestContractReportIndex && lastRiskExport ? handleExportRiskJson : undefined}
                      />
                    ) : msg.role === "assistant" ? (
                      <MarkdownMessage content={msg.content} workspacePath={activeWorkspaceDir} />
                    ) : (
                      msg.content
                    )}
                  </div>
                  <div className="chat-bubble-actions">
                    <button
                      type="button"
                      onClick={() => {
                        setCopyError(null);
                        setCopyNotice(null);
                        void writeClipboardText(msg.content).then(() => {
                          setCopiedMessageIndex(i);
                          window.setTimeout(() => setCopiedMessageIndex((current) => current === i ? null : current), 1600);
                        }).catch((reason) => setCopyError(String(reason)));
                      }}
                    >
                      {copiedMessageIndex === i ? "已复制" : "复制"}
                    </button>
                  </div>
                  {msg.agent && (
                    <div className="chat-bubble-meta">Agent: {msg.agent}</div>
                  )}
                  {msg.skillName && (
                    <div className="chat-bubble-meta">使用技能：{msg.skillName}</div>
                  )}
                  {msg.toolName && (
                    <div className="chat-bubble-meta">使用工具：{msg.toolName}</div>
                  )}
                  {msg.contractEngineName && (
                    <div className="chat-bubble-meta">审核引擎：{msg.contractEngineName}</div>
                  )}
                  {msg.steps?.length ? <ExecutionSteps steps={msg.steps} /> : null}
                </div>
                );
              })
            )}
            {sending && (
              <div className="chat-bubble chat-bubble-assistant chat-bubble-typing">
                <div className="chat-typing-row">
                  <span className="typing-dot" />
                  <span className="typing-dot" />
                  <span className="typing-dot" />
                  <span className="typing-elapsed">
                    {activeRun?.longRunning
                      ? `Agent 仍在执行，已运行 ${elapsedSec}s`
                      : `${elapsedSec}s`}
                  </span>
                </div>
                {activeSteps.length > 0 || activeRunId ? (
                  <ExecutionSteps steps={activeSteps} liveSessionId={activeRunId} />
                ) : null}
              </div>
            )}
          </div>

          {error && (
            <div className="chat-error" role="alert">
              <span>{error}</span>
              <button
                type="button"
                className="chat-error-copy"
                onClick={() => {
                  setCopyError(null);
                  setCopyNotice(null);
                  void writeClipboardText(error).then(() => {
                    setCopyNotice("错误信息已复制");
                    window.setTimeout(() => setCopyNotice(null), 1600);
                  }).catch((reason) => setCopyError(String(reason)));
                }}
              >
                复制错误
              </button>
              <button
                type="button"
                className="chat-error-dismiss"
                aria-label="关闭错误提示"
                onClick={() => setError(null)}
              >
                <UiIcon name="close" size={14} />
              </button>
            </div>
          )}
          {copyError ? <div className="chat-copy-error" role="alert">复制失败：{copyError}</div> : null}
          {copyNotice ? <div className="chat-copy-notice" role="status">{copyNotice}</div> : null}

          <div className="chat-composer-wrap">
            <input
              id={CHAT_ATTACHMENT_INPUT_ID}
              type="file"
              multiple
              accept={selectedSystemTool?.id === "tool:contract-review" ? ".docx,.pdf,.txt,.md" : undefined}
              disabled={sending}
              className="chat-file-input-hidden"
              aria-label="选择附件文件"
              onChange={handleAttachFilesChange}
            />
            <div className="chat-composer-stack">
            {paletteVisible ? (
              <CommandPaletteMenu
                palette={filteredPalette}
                selectedIndex={
                  paletteRowItems.length
                    ? Math.min(paletteIndex, paletteRowItems.length - 1)
                    : 0
                }
                onHover={setPaletteIndex}
                onSelect={applyPaletteSelection}
              />
            ) : null}
            {selectedSystemTool?.id === "tool:contract-review" ? (
              <ContractReviewPanel
                attachments={attachments}
                engines={contractReviewEngines}
                selectedEngine={selectedContractEngine}
                extraInstructions={contractExtraInstructions}
                stage={contractReviewUi.stage}
                documentsPrepared={lastRiskExport != null}
                elapsedSec={elapsedSec}
                error={contractReviewUi.error}
                onChooseFiles={() => {
                  setLastRiskExport(null);
                  setContractReviewUiByAvatar((prev) => ({
                    ...prev,
                    [activeAvatarId]: { stage: "idle" },
                  }));
                  if (isTauriEnvironment()) {
                    void handleAttachTauri();
                  } else {
                    document.getElementById(CHAT_ATTACHMENT_INPUT_ID)?.click();
                  }
                }}
                onDropFiles={(files) => {
                  setLastRiskExport(null);
                  setContractReviewUiByAvatar((prev) => ({
                    ...prev,
                    [activeAvatarId]: { stage: "idle" },
                  }));
                  void mergeDroppedIntoInput(files).catch((reason) => {
                    setError(reason instanceof Error ? reason.message : String(reason));
                  });
                }}
                onRemoveAttachment={(id) => {
                  removeAttachment(id);
                  setLastRiskExport(null);
                  setContractReviewUiByAvatar((prev) => ({
                    ...prev,
                    [activeAvatarId]: { stage: "idle" },
                  }));
                }}
                onEngineChange={setSelectedContractEngine}
                onInstructionsChange={setContractExtraInstructions}
                onStart={() => void handleStartContractReview()}
              />
            ) : null}
            <div className="chat-composer-card">
              <div
                className="chat-composer-resize-handle"
                role="separator"
                aria-label="调整消息输入框高度"
                aria-orientation="horizontal"
                aria-valuemin={COMPOSER_MIN_HEIGHT}
                aria-valuemax={COMPOSER_MAX_HEIGHT}
                aria-valuenow={composerInputHeight}
                tabIndex={0}
                title="向上或向下拖动调整输入框高度；也可使用方向键"
                onPointerDown={handleComposerResizePointerDown}
                onKeyDown={handleComposerResizeKeyDown}
              >
                <span aria-hidden />
              </div>
              {selectedSkill ? (
                <div className="chat-skill-chips" aria-label="已选择技能">
                  <span className="chat-skill-chip">
                    <UiIcon name="apps" size={13} />
                    <span className="chat-skill-chip-name">{selectedSkill.displayName}</span>
                    <button
                      type="button"
                      className="chat-skill-chip-remove"
                      aria-label={`取消技能 ${selectedSkill.displayName}`}
                      onClick={() =>
                        setSelectedSkills((prev) => {
                          if (!prev[activeAvatarId]) return prev;
                          const next = { ...prev };
                          delete next[activeAvatarId];
                          return next;
                        })
                      }
                      disabled={sending}
                    >
                      <UiIcon name="close" size={11} />
                    </button>
                  </span>
                </div>
              ) : null}
              {selectedSystemTool ? (
                <div className="chat-skill-chips" aria-label="已选择系统工具">
                  <span className="chat-skill-chip">
                    <UiIcon name="settings" size={13} />
                    <span className="chat-skill-chip-name">{selectedSystemTool.displayName}</span>
                    <button
                      type="button"
                      className="chat-skill-chip-remove"
                      aria-label={`取消工具 ${selectedSystemTool.displayName}`}
                      onClick={() => setSelectedSystemTools((prev) => {
                        if (!prev[activeAvatarId]) return prev;
                        const next = { ...prev };
                        delete next[activeAvatarId];
                        return next;
                      })}
                      disabled={sending}
                    >
                      <UiIcon name="close" size={11} />
                    </button>
                  </span>
                </div>
              ) : null}
              {attachments.length && selectedSystemTool?.id !== "tool:contract-review" ? (
                <div className="chat-attachment-chips" aria-label="待发送附件">
                  {attachments.map((a) => (
                    <span
                      key={a.id}
                      className={`chat-attachment-chip chat-attachment-chip--${a.kind}`}
                      title={a.note ? `${a.name}（${a.note}）${a.mountedPath ? `\n${a.mountedPath}` : ""}` : a.name}
                    >
                      <UiIcon
                        name={a.kind === "image" ? "fileImage" : a.kind === "text" ? "fileText" : "file"}
                        size={14}
                      />
                      <span className="chat-attachment-chip-name">{a.name}</span>
                      {a.note ? <span className="chat-attachment-chip-note">{a.note}</span> : null}
                      {a.mountedPath ? <span className="chat-attachment-chip-path">{a.mountedPath}</span> : null}
                      <button
                        type="button"
                        className="chat-attachment-chip-remove"
                        aria-label={`移除附件 ${a.name}`}
                        onClick={() => removeAttachment(a.id)}
                        disabled={sending}
                      >
                        <UiIcon name="close" size={11} />
                      </button>
                    </span>
                  ))}
                </div>
              ) : null}

              <div
                className={`chat-input-row${composerDragActive ? " chat-input-row--drag-over" : ""}`}
                onDragEnter={handleComposerDragEnter}
                onDragLeave={handleComposerDragLeave}
                onDragOver={handleComposerDragOverFiles}
                onDrop={handleComposerDropFiles}
                aria-label="消息输入区域：可拖动输入框右下角调整高度，也可拖入或粘贴文件"
              >
                <textarea
                  ref={composerInputRef}
                  className="chat-input"
                  value={input}
                  onChange={(e) => {
                    setActiveInput(e.target.value);
                    syncComposerCursor(e.target);
                  }}
                  onClick={(e) => syncComposerCursor(e.currentTarget)}
                  onKeyUp={(e) => syncComposerCursor(e.currentTarget)}
                  onSelect={(e) => syncComposerCursor(e.currentTarget)}
                  onKeyDown={handleKeyDown}
                  onDragOver={handleComposerDragOverFiles}
                  onDrop={handleComposerDropFiles}
                  onPaste={handleComposerPaste}
                  placeholder={
                    gatewayStatus === "connected"
                      ? selectedSystemTool
                        ? "合同审核要求可在上方面板填写；请添加合同附件。"
                        : selectedSkill
                        ? `已选择 ${selectedSkill.displayName}，输入任务后发送…`
                        : "输入 / 选择技能，或直接发消息…（支持拖入/粘贴文件）"
                      : "网关未连接…（仍可拖入文件编辑草稿）"
                  }
                  rows={2}
                  style={{ height: `${composerInputHeight}px` }}
                  disabled={gatewayStatus !== "connected"}
                />
                {sending ? (
                  <button type="button" className="chat-cancel-button" onClick={handleCancel}>
                    取消
                  </button>
                ) : (
                  <button
                    type="button"
                    className="chat-send-fab"
                    onClick={() => void handleSend()}
                    disabled={(!input.trim() && attachments.length === 0) || gatewayStatus !== "connected"}
                    aria-label="发送"
                  >
                    <UiIcon name="send" size={17} />
                  </button>
                )}
              </div>

              <div className="chat-composer-toolbar" aria-label="任务输入设置">
                <div className="chat-composer-tools-left">
                  {sending ? (
                    <span className="chat-attach-btn chat-attach-btn--disabled" title="发送中暂不可添加附件">
                      <UiIcon name="paperclip" size={16} />
                    </span>
                  ) : isTauriEnvironment() ? (
                    <button
                      type="button"
                      className="chat-attach-btn"
                      title="添加附件（也可直接拖入输入框）"
                      aria-label="添加附件"
                      onClick={() => void handleAttachTauri()}
                    >
                      <UiIcon name="paperclip" size={16} />
                    </button>
                  ) : (
                    <label htmlFor={CHAT_ATTACHMENT_INPUT_ID} className="chat-attach-btn" title="添加附件">
                      <UiIcon name="paperclip" size={16} />
                    </label>
                  )}
                  <label
                    className={`chat-vision-toggle${desktopVisionMaster ? "" : " chat-vision-toggle--disabled"}`}
                    title={
                      desktopVisionMaster
                        ? "发送时截取主屏幕并传给支持视觉的多模态模型"
                        : "请先在 设置 → 通用 中开启桌面视觉监控"
                    }
                  >
                    <input
                      type="checkbox"
                      checked={desktopVisionOn && desktopVisionMaster}
                      disabled={!desktopVisionMaster || sending || gatewayStatus !== "connected"}
                      onChange={() => {
                        if (!desktopVisionMaster) return;
                        setDesktopVisionOn((prev) => {
                          const next = !prev;
                          localStorage.setItem(DESKTOP_VISION_SESSION_KEY, next ? "1" : "0");
                          return next;
                        });
                      }}
                    />
                    <span>桌面视觉</span>
                  </label>
                </div>

                <div className="chat-composer-tools-center">
                  <div className="chat-composer-model-shell">
                    <ModelPicker
                      value={selectedModel}
                      onChange={setSelectedModel}
                      providers={availableProviders}
                      defaultProvider={defaultProviderId}
                      defaultModel={defaultModelId}
                      maxMode={maxMode}
                      onMaxModeChange={setMaxMode}
                      onConfigureCustom={() => onOpenSettings?.("providers")}
                      disabled={sending}
                      variant="inline"
                    />
                  </div>
                  {!modelReady ? (
                    <button
                      type="button"
                      className="chat-composer-permission chat-composer-config-action"
                      onClick={() => onOpenSettings?.("providers")}
                    >
                      配置模型
                    </button>
                  ) : null}
                  <button
                    type="button"
                    className="chat-composer-permission"
                    title="工具授权规则由本地安全设置统一管理"
                    onClick={() => onOpenSettings?.("general")}
                  >
                    <UiIcon name="safety" size={14} />
                    <span>默认权限</span>
                  </button>
                  <div
                    ref={workspaceMenuRef}
                    className={`chat-workspace-actions${sessionWorkspaceDir ? " chat-workspace-actions--temporary" : ""}`}
                  >
                    <button
                      type="button"
                      className="chat-workspace-pill"
                      title={
                        activeWorkspaceDir
                          ? `${workspaceLabel} Workspace：${activeWorkspaceDir}\n点击重新选择；右键查看更多操作`
                          : "选择当前任务使用的 Workspace"
                      }
                      aria-label={activeWorkspaceDir ? `${workspaceLabel} Workspace：${activeWorkspaceDir}` : "选择 Workspace"}
                      onClick={(e) => void handleChooseWorkspace(e)}
                      onContextMenu={(e) => {
                        e.preventDefault();
                        if (!sending) setWorkspaceMenuOpen((open) => !open);
                      }}
                      disabled={sending}
                    >
                      <UiIcon name="folder" size={15} />
                      {activeWorkspaceDir ? <span className="chat-workspace-pill-scope">{workspaceLabel}</span> : null}
                      <span className="chat-workspace-pill-path">{workspaceSummary}</span>
                    </button>
                    {workspaceMenuOpen ? (
                      <div className="chat-workspace-menu" role="menu">
                        <button type="button" role="menuitem" onClick={(e) => void handleChooseWorkspace(e)}>
                          重新选择 Workspace
                        </button>
                        <button
                          type="button"
                          role="menuitem"
                          onClick={() => void handleOpenWorkspace()}
                          disabled={!activeWorkspaceDir}
                        >
                          打开当前 Workspace 文件夹
                        </button>
                        {sessionWorkspaceDir ? (
                          <button type="button" role="menuitem" onClick={handleClearSessionWorkspace}>
                            清除当前会话临时 Workspace
                          </button>
                        ) : null}
                      </div>
                    ) : null}
                  </div>
                </div>

                <button
                  type="button"
                  className={`chat-composer-gateway chat-composer-gateway--${gatewayStatus}`}
                  title={gatewayStatus === "connected" ? gatewayUrl || "网关在线" : "点击启动或刷新网关"}
                  onClick={() => {
                    if (gatewayStatus === "disconnected") void handleStartGateway();
                    else void refreshGatewayStatus();
                  }}
                  disabled={gatewayStarting || gatewayStatus === "connecting"}
                >
                  <span className={`chat-gateway-dot chat-gateway-dot--${gatewayStatus}`} aria-hidden />
                  {gatewayStatus === "connected"
                    ? `网关 ${gatewayPort}`
                    : gatewayStatus === "connecting"
                      ? "检查中"
                      : gatewayStarting
                        ? "启动中…"
                        : "启动网关"}
                </button>
              </div>
            </div>
            </div>
          </div>

          <TaskInspector
            open={inspectorOpen}
            tab={inspectorTab}
            onTabChange={setInspectorTab}
            onClose={() => setInspectorOpen(false)}
            task={activeTask}
            liveSessionId={activeRunId}
            activeSteps={activeSteps}
            onAddArtifactToChat={(path) => void mergePathsIntoInput([path])}
          />
        </main>
      </div>
      {pendingDeleteAvatarId ? (
        <div
          className="chat-delete-dialog-backdrop"
          role="presentation"
          onMouseDown={(event) => {
            if (event.currentTarget === event.target) setPendingDeleteAvatarId(null);
          }}
        >
          <section
            className="chat-delete-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="chat-delete-dialog-title"
            aria-describedby="chat-delete-dialog-description"
          >
            <span className="chat-delete-dialog-icon"><UiIcon name="delete" size={18} /></span>
            <div>
              <h2 id="chat-delete-dialog-title">删除会话前，请选择任务记录的处理方式</h2>
              <p id="chat-delete-dialog-description">
                {runs[pendingDeleteAvatarId]
                  ? "该会话仍有任务运行，继续删除会终止任务并清空对话。"
                  : "对话记录会被清空，任务过程、成果与文件索引可以单独保留。"}
              </p>
            </div>
            <div className="chat-delete-dialog-actions">
              <button type="button" onClick={() => setPendingDeleteAvatarId(null)}>取消</button>
              <button
                type="button"
                className="is-retain"
                onClick={() => confirmDeleteAvatar(pendingDeleteAvatarId, true)}
              >
                删除会话，保留任务记录
              </button>
              <button
                type="button"
                className="is-danger"
                onClick={() => confirmDeleteAvatar(pendingDeleteAvatarId, false)}
              >
                对话与任务记录一并删除
              </button>
            </div>
          </section>
        </div>
      ) : null}
    </div>
  );
}

function ExecutionSteps({ steps, liveSessionId }: { steps: ExecutionStep[]; liveSessionId?: string | null }) {
  const statusLabel = (status?: ExecutionStep["status"]) => {
    const labels: Record<string, string> = {
      running: "进行中",
      done: "完成",
      error: "失败",
      pending: "等待",
    };
    return labels[status ?? ""] ?? "记录";
  };

  return (
    <div className="chat-execution-steps">
      <div className="chat-execution-title">
        {liveSessionId ? "当前进度（详细过程见任务检查器）" : "执行步骤"}
      </div>
      {/* 实时时间线只挂载在右侧任务检查器，避免重复订阅同一个 Tauri 事件。 */}
      <ol>
        {steps.map((step, index) => (
          <li key={`${step.title}-${index}`} className={`chat-execution-step chat-execution-step--${step.status ?? "info"}`}>
            <span className="chat-execution-step-title">{step.title}</span>
            <span className="chat-execution-step-status">{statusLabel(step.status)}</span>
            {step.detail ? <div className="chat-execution-step-detail">{step.detail}</div> : null}
            {!liveSessionId && step.run_events && step.run_events.length > 0 && (
              <div className="mt-1">
                <AgentRunTimeline
                  events={step.run_events}
                  isRunning={step.status === "running"}
                  defaultCollapsed={true}
                />
              </div>
            )}
          </li>
        ))}
      </ol>
    </div>
  );
}

