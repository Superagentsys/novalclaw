import { useRef, useEffect, useState, useCallback, useMemo } from "react";
import { ChatMediaInteraction } from "./ChatMediaInteraction";
import { invokeTauri } from "../../utils/tauri";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  isTauriEnvironment,
  pickComposerAttachmentPaths,
  readComposerAttachmentsFromPaths,
} from "../../utils/composerAttachments";
import {
  areStoredMessagesEqual,
  fetchSessionHistory,
  fetchWebSessionsFromGateway,
  formatTime,
  loadChatStorage,
  mergeAvatarSessions,
  saveChatStorage,
  type StoredAvatarSession,
  type StoredChatMessage,
} from "../../utils/chatStorage";
import type {
  AgentRunEvent,
} from "../AgentRun/types";
import type {
  AgentPersonaConfig,
  Config,
  GatewayInboundResponse,
  GatewayStatus,
  ExecutionStep,
  RouteDecision,
  WorkspaceStatus,
} from "../../types/config";
import { AgentRunTimeline } from "../AgentRun/AgentRunTimeline";

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

/** 将拖放/选择的文件转为可粘贴进输入框的文本（Markdown） */
async function formatDroppedFilesContent(files: FileList | readonly File[]): Promise<string> {
  const list = Array.from(files).slice(0, DROP_FILES_MAX_COUNT);
  const parts: string[] = [];

  for (const file of list) {
    const displayName = file.name?.trim() || "unnamed";

    if (file.size === 0) {
      parts.push(`\n\n[空文件: ${displayName}]`);
      continue;
    }

    if (file.type.startsWith("image/")) {
      if (file.size > DROP_IMAGE_INLINE_MAX_BYTES) {
        parts.push(
          `\n\n[图片: ${displayName} · ${Math.round(file.size / 1024)} KB — 超过 ${Math.round(DROP_IMAGE_INLINE_MAX_BYTES / 1024)} KB 上限未嵌入；请缩小后再拖入或改用文字描述。]`
        );
        continue;
      }
      try {
        const dataUrl = await readFileAsDataURL(file);
        parts.push(`\n\n![${escapeMarkdownImageAlt(displayName)}](${dataUrl})`);
      } catch {
        parts.push(`\n\n[图片读取失败: ${displayName}]`);
      }
      continue;
    }

    if (isProbablyTextFile(file)) {
      if (file.size > DROP_TEXT_FILE_MAX_BYTES) {
        parts.push(
          `\n\n[文本附件 ${displayName}: 过大 (${Math.round(file.size / 1024)} KB)，上限 ${Math.round(DROP_TEXT_FILE_MAX_BYTES / 1024)} KB — 请拆分或使用更小文件。]`
        );
        continue;
      }
      try {
        const text = await file.text();
        parts.push(`\n\n--- 附件: ${displayName} ---\n${text}\n--- 附件结束 ---`);
      } catch {
        parts.push(`\n\n[文本读取失败: ${displayName}]`);
      }
      continue;
    }

    parts.push(
      `\n\n[附件: ${displayName} · ${file.type || "未知类型"} · ${Math.round(file.size / 1024)} KB — 未能自动读取此类文件内容；可先导出为文本再拖入。]`
    );
  }

  return parts.join("");
}

interface ChatMessage extends StoredChatMessage {
  steps?: ExecutionStep[];
}

type SidebarTab = "avatars" | "channels" | "scheduled";

interface ImChannelEntry {
  id: string;
  name: string;
  platform: string;
  createdAt: string;
}

interface ScheduledTaskEntry {
  id: string;
  name: string;
  cron: string;
  createdAt: string;
}

const IM_PLATFORM_OPTIONS = [
  "feishu",
  "lark",
  "dingtalk",
  "wechat",
  "telegram",
  "discord",
  "slack",
];

interface ChatProps {
  /** 与侧栏「定时任务」入口同步 */
  initialSidebarTab?: SidebarTab;
}

export function Chat({ initialSidebarTab = "avatars" }: ChatProps) {
  const initialStorage = useMemo(() => loadChatStorage(), []);
  const [avatars, setAvatars] = useState<StoredAvatarSession[]>(initialStorage.avatars);
  const [activeAvatarId, setActiveAvatarId] = useState(initialStorage.activeAvatarId);
  const [sidebarTab, setSidebarTab] = useState<SidebarTab>(initialSidebarTab);
  const [channels, setChannels] = useState<ImChannelEntry[]>([]);
  const [scheduledTasks, setScheduledTasks] = useState<ScheduledTaskEntry[]>([]);
  const [newChannelName, setNewChannelName] = useState("");
  const [newChannelPlatform, setNewChannelPlatform] = useState(IM_PLATFORM_OPTIONS[0]);
  const [newTaskName, setNewTaskName] = useState("");
  const [newTaskCron, setNewTaskCron] = useState("0 9 * * *");
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
  const [runs, setRuns] = useState<
    Record<string, { elapsedSec: number; steps: ExecutionStep[]; runId: string; longRunning?: boolean }>
  >({});
  const [error, setError] = useState<string | null>(null);
  const [gatewayStatus, setGatewayStatus] = useState<"connecting" | "connected" | "disconnected">("connecting");
  const [gatewayUrl, setGatewayUrl] = useState<string>("");
  const [availableModels] = useState<string[]>(["auto", "openai", "anthropic", "gemini", "ollama"]);
  const [selectedModel, setSelectedModel] = useState("auto");
  const messagesScrollRef = useRef<HTMLDivElement>(null);
  const stickToBottomRef = useRef(true);
  const historyLoadGenRef = useRef(0);
  // 每个会话独立的取消标志与计时器。
  const cancelledRef = useRef<Record<string, boolean>>({});
  const elapsedTimersRef = useRef<Record<string, ReturnType<typeof setInterval>>>({});
  const safetyTimersRef = useRef<Record<string, ReturnType<typeof setTimeout>>>({});
  const runAvatarIdsRef = useRef<Record<string, string>>({});
  const activeRunIdRef = useRef<string | null>(null);
  const terminalRunIdsRef = useRef<Set<string>>(new Set());
  const completedRunIdsRef = useRef<Set<string>>(new Set());
  const insertedReplyRunIdsRef = useRef<Set<string>>(new Set());
  const runsRef = useRef(runs);
  const [composerDragActive, setComposerDragActive] = useState(false);
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
  const [sessionWorkspaceDirs, setSessionWorkspaceDirs] = useState<Record<string, string>>({});

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

  // 仅反映「当前查看的会话」的运行/输入状态。
  const activeRun = runs[activeAvatarId];
  const sending = Boolean(activeRun);
  const elapsedSec = activeRun?.elapsedSec ?? 0;
  const activeSteps = activeRun?.steps ?? [];
  const activeRunId = activeRun?.runId ?? null;
  const input = inputs[activeAvatarId] ?? "";

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
  const appendActiveInput = useCallback(
    (updater: (prev: string) => string) =>
      setInputs((prev) => ({
        ...prev,
        [activeAvatarId]: updater(prev[activeAvatarId] ?? ""),
      })),
    [activeAvatarId]
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
          const next = remote.length > 0 ? remote : current;
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

  useEffect(() => {
    void invokeTauri<Config>("get_setup_config")
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
        const stored = localStorage.getItem(DESKTOP_VISION_SESSION_KEY);
        if (stored === "1") setDesktopVisionOn(true);
        else if (stored === "0") setDesktopVisionOn(false);
        else setDesktopVisionOn(master);
      })
      .catch(() => {});
  }, []);

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
    if (!stickToBottomRef.current) return;
    scrollMessagesToEnd("auto");
  }, [messages, sending, activeSteps, elapsedSec, scrollMessagesToEnd]);

  // 窗口缩放导致消息重新换行时，保持视口顶部正在浏览的消息位置不变。
  useEffect(() => {
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
  }, []);

  useEffect(() => {
    const timers = elapsedTimersRef.current;
    return () => {
      Object.values(timers).forEach((timer) => clearInterval(timer));
      Object.values(safetyTimersRef.current).forEach((timer) => clearTimeout(timer));
    };
  }, []);

  useEffect(() => {
    if (!isTauriEnvironment()) return;

    let disposed = false;
    let unlisten: (() => void) | undefined;

    listen<AgentRunEvent | Record<string, unknown>>("agent-run-event", (event) => {
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

      const eventType = payload.type;
      const isTerminalEvent =
        eventType === "run_completed" ||
        eventType === "run_failed" ||
        eventType === "run_cancelled" ||
        eventType === "error";
      if (terminalRunIdsRef.current.has(runId) && !isTerminalEvent) {
        if (import.meta.env.DEV && payload.type !== "model_delta") {
          console.debug("[chat-agent-run-event ignored after terminal]", payload);
        }
        return;
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

      if (eventType === "run_completed") {
        if (!completedRunIdsRef.current.has(runId)) {
          completedRunIdsRef.current.add(runId);
          const finalReply = payload.reply || payload.reply_preview || "";
          if (import.meta.env.DEV) {
            console.log("[appendAssistantMessageOnce] run_id=" + runId + " reply_len=" + finalReply.length);
          }
          if (!cancelledRef.current[avatarId]) {
            appendAssistantMessageOnce(runId, finalReply, avatarId);
          }
        }
        if (import.meta.env.DEV) {
          console.log("[finishRun] run_id=" + runId);
        }
        finishRun(runId);
        return;
      }

      if (eventType === "run_cancelled") {
        if (!completedRunIdsRef.current.has(runId)) {
          completedRunIdsRef.current.add(runId);
          appendAssistantMessageOnce(runId, "任务已取消。", avatarId);
        }
        finishRun(runId);
        return;
      }

      if (!completedRunIdsRef.current.has(runId)) {
        completedRunIdsRef.current.add(runId);
        const rawError = payload.error || payload.message || "Agent run failed";
        setError(rawError);
        if (!cancelledRef.current[avatarId]) {
          appendAssistantMessageOnce(runId, `任务失败：${rawError}`, avatarId);
        }
      }
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
  }, [appendAssistantMessageOnce, findAvatarIdByRunId, finishRun]);

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
  };

  const handleDeleteAvatar = (id: string) => {
    // 终止该会话可能正在进行的任务，并清理其计时器/运行态。
    cancelledRef.current[id] = true;
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

  const handleCreateChannel = () => {
    const name = newChannelName.trim();
    if (!name) {
      setError("请先输入 IM 频道名称");
      return;
    }
    const entry: ImChannelEntry = {
      id: `im-${Date.now()}-${Math.random().toString(16).slice(2, 8)}`,
      name,
      platform: newChannelPlatform,
      createdAt: formatTime(new Date()),
    };
    setChannels((prev) => [entry, ...prev]);
    setNewChannelName("");
    setError(null);
  };

  const handleCreateScheduledTask = () => {
    const name = newTaskName.trim();
    const cron = newTaskCron.trim();
    if (!name) {
      setError("请先输入定时任务名称");
      return;
    }
    if (!cron) {
      setError("请先输入 Cron 表达式");
      return;
    }
    const entry: ScheduledTaskEntry = {
      id: `cron-${Date.now()}-${Math.random().toString(16).slice(2, 8)}`,
      name,
      cron,
      createdAt: formatTime(new Date()),
    };
    setScheduledTasks((prev) => [entry, ...prev]);
    setNewTaskName("");
    setError(null);
  };

  const handleCancel = useCallback(() => {
    if (!activeRunId) return;
    cancelledRef.current[activeAvatarId] = true;
    void invokeTauri<void>("cancel_agent_run", { runId: activeRunId }).catch((err) => {
      const message = err instanceof Error ? err.message : String(err);
      setError(`取消失败：${message}`);
    });
  }, [activeAvatarId, activeRunId]);

  const handleSend = async () => {
    // 绑定到「发送时」的会话，使后续状态更新只作用于该会话，
    // 即使用户中途切换到其它会话也互不影响。
    const avatarId = activeAvatarId;
    const targetSessionId = sessionId;
    const text = input.trim();
    const active = runsRef.current[avatarId]?.runId ?? (avatarId === activeAvatarId ? activeRunIdRef.current : null);
    if (active && terminalRunIdsRef.current.has(active)) {
      finishRun(active);
    } else if (active) {
      setError("当前 Agent 仍在执行，请等待完成或取消。");
      return;
    }
    if (!text) return;
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
    setActiveInput("");
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
      [avatarId]: [...(prev[avatarId] ?? []), { role: "user", content: text }],
    }));
    setAvatars((prev) =>
      prev.map((a) =>
        a.id === avatarId ? { ...a, lastAt: formatTime(new Date()) } : a
      )
    );

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
      const metadata: Record<string, unknown> = {
        preferred_provider: selectedModel === "auto" ? undefined : selectedModel,
        // Session-scoped temporary workspace (takes highest priority in the backend).
        // Clears when the user closes the app or starts a new session.
        ...(sessionWorkspaceDir ? { workspace_dir: sessionWorkspaceDir } : {}),
        // Run ID for real-time event correlation between frontend and backend.
        run_id: runId,
      };

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
          finishRun(runId);
          setMessagesBySession((prev) => ({
            ...prev,
            [avatarId]: (prev[avatarId] ?? []).slice(0, -1),
          }));
          setInputs((prev) => ({ ...prev, [avatarId]: text }));
          return;
        }
      }

      const payload = {
        channel: "web" as const,
        text,
        sessionId: targetSessionId,
        userId: USER_ID,
        metadata,
      };
      route = await invokeTauri<RouteDecision>("route_inbound_message", {
        payload,
      }).catch(() => null);
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

          // Normal completion is driven only by terminal agent-run-event payloads:
          // run_completed / run_failed / run_cancelled. The invoke Promise may
          // resolve late or never resolve, so it must not mutate chat completion state.
        })
        .catch((e) => {
          if (import.meta.env.DEV) {
            console.error("[handleSend] process_inbound_message_streaming error run_id=" + runId + " error=" + (e instanceof Error ? e.message : String(e)));
          }
          // The Tauri command emits run_failed for backend errors when a run_id exists.
          // Chat cleanup still happens only from that terminal event.
        });
    } catch (e) {
      if (import.meta.env.DEV) {
        console.error("[handleSend] outer-error run_id=" + runId + " error=" + (e instanceof Error ? e.message : String(e)));
      }
      finishRun(runId);
    } finally {
      // outer finally (safety timer cleanup)
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      void handleSend();
    }
  };

  const appendVoiceTranscript = useCallback(
    (text: string) => {
      appendActiveInput((prev) => (prev.trim() ? `${prev} ${text}` : text));
    },
    [appendActiveInput]
  );

  const appendAttachmentContent = useCallback(
    (insert: string) => {
      const trimmed = insert.trim();
      if (!trimmed) return;
      appendActiveInput((prev) => (prev.trim() ? `${prev}\n${trimmed}` : trimmed));
    },
    [appendActiveInput]
  );

  const mergePathsIntoInput = useCallback(
    async (paths: string[]) => {
      const insert = await readComposerAttachmentsFromPaths(paths);
      appendAttachmentContent(insert);
    },
    [appendAttachmentContent]
  );

  /** 浏览器预览等非 Tauri 环境：用 File API 读取 */
  const mergeDroppedIntoInput = useCallback(
    async (files: FileList | readonly File[]) => {
      const insert = await formatDroppedFilesContent(files);
      appendAttachmentContent(insert);
    },
    [appendAttachmentContent]
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
      const selected = await open({
        directory: true,
        multiple: false,
        title: saveToAgent ? "选择该 Agent 的 Workspace 目录" : "选择临时 Workspace 目录",
      });
      if (selected == null) return;
      const dir = selected as string;

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

  const statusText =
    gatewayStatus === "connected"
      ? "gateway 已连接"
      : gatewayStatus === "connecting"
      ? "gateway 正在恢复"
      : "gateway 未连接";

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

  return (
    <div className="chat-layout">
      <div className="chat-body">
        <aside className="chat-sidebar">
          <button
            type="button"
            className="chat-new-chat-pill"
            onClick={handleAddAvatar}
          >
            + 新智能体
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
                    onClick={() => setActiveAvatarId(a.id)}
                  >
                    <span className="chat-avatar-icon">◇</span>
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
                    className="chat-avatar-delete"
                    title="删除智能体"
                    aria-label={`删除智能体 ${a.name}`}
                    onClick={(e) => {
                      e.stopPropagation();
                      handleDeleteAvatar(a.id);
                    }}
                  >
                    ✕
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
              分身
            </button>
            <button
              type="button"
              className={sidebarTab === "channels" ? "is-active" : ""}
              onClick={() => setSidebarTab("channels")}
            >
              IM 频道
            </button>
            <button
              type="button"
              className={sidebarTab === "scheduled" ? "is-active" : ""}
              onClick={() => setSidebarTab("scheduled")}
            >
              定时任务
            </button>
          </nav>
          {sidebarTab === "channels" ? (
            <section className="chat-sidebar-section">
              <h3 className="chat-sidebar-heading">创建 IM 频道</h3>
              <select
                value={newChannelPlatform}
                onChange={(event) => setNewChannelPlatform(event.target.value)}
              >
                {IM_PLATFORM_OPTIONS.map((platform) => (
                  <option key={platform} value={platform}>
                    {platform}
                  </option>
                ))}
              </select>
              <input
                value={newChannelName}
                onChange={(event) => setNewChannelName(event.target.value)}
                placeholder="频道名称，例如：研发群"
              />
              <button type="button" className="chat-new-avatar" onClick={handleCreateChannel}>
                + 创建 IM 频道
              </button>
              <ul className="chat-avatar-list">
                {channels.length === 0 ? (
                  <li className="chat-avatar-time">暂无 IM 频道</li>
                ) : (
                  channels.map((item) => (
                    <li key={item.id}>
                      <div className="chat-avatar-item">
                        <span className="chat-avatar-icon">#</span>
                        <span className="chat-avatar-name">{item.name}</span>
                        <span className="chat-avatar-time">{item.platform}</span>
                      </div>
                    </li>
                  ))
                )}
              </ul>
            </section>
          ) : null}
          {sidebarTab === "scheduled" ? (
            <section className="chat-sidebar-section">
              <h3 className="chat-sidebar-heading">创建定时任务</h3>
              <input
                value={newTaskName}
                onChange={(event) => setNewTaskName(event.target.value)}
                placeholder="任务名称，例如：早报推送"
              />
              <input
                value={newTaskCron}
                onChange={(event) => setNewTaskCron(event.target.value)}
                placeholder="Cron，例如：0 9 * * *"
              />
              <button
                type="button"
                className="chat-new-avatar"
                onClick={handleCreateScheduledTask}
              >
                + 创建定时任务
              </button>
              <ul className="chat-avatar-list">
                {scheduledTasks.length === 0 ? (
                  <li className="chat-avatar-time">暂无定时任务</li>
                ) : (
                  scheduledTasks.map((task) => (
                    <li key={task.id}>
                      <div className="chat-avatar-item">
                        <span className="chat-avatar-icon">⏰</span>
                        <span className="chat-avatar-name">{task.name}</span>
                        <span className="chat-avatar-time">{task.cron}</span>
                      </div>
                    </li>
                  ))
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
                title="从网关重新加载当前会话历史"
                disabled={historyLoading || gatewayStatus !== "connected"}
                onClick={() => void handleRefreshHistory()}
              >
                ⟳
              </button>
              <button
                type="button"
                className="chat-icon-btn"
                title="刷新网关状态"
                onClick={() => void refreshGatewayStatus()}
              >
                ↻
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
            {historyLoading && messages.length === 0 ? (
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
              messages.map((msg, i) => (
                <div
                  key={i}
                  className={`chat-bubble chat-bubble-${msg.role}`}
                >
                  <div className="chat-bubble-content">{msg.content}</div>
                  {msg.agent && (
                    <div className="chat-bubble-meta">Agent: {msg.agent}</div>
                  )}
                  {msg.steps?.length ? <ExecutionSteps steps={msg.steps} /> : null}
                </div>
              ))
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
              {error}
              <button
                type="button"
                className="chat-error-dismiss"
                onClick={() => setError(null)}
              >
                ✕
              </button>
            </div>
          )}

          <div className="chat-composer-wrap">
            <ChatMediaInteraction
              appendTranscript={appendVoiceTranscript}
              disabled={sending || gatewayStatus !== "connected"}
              trailingActions={
                <div
                  ref={workspaceMenuRef}
                  className={`chat-workspace-actions${
                    sessionWorkspaceDir ? " chat-workspace-actions--temporary" : ""
                  }`}
                >
                  <button
                    type="button"
                    className="chat-workspace-pill"
                    title={
                      activeWorkspaceDir
                        ? `${workspaceLabel} Workspace：${activeWorkspaceDir}\n普通点击 = 重新选择临时 Workspace；Shift+点击 = 保存到该 Agent；右键 = 更多操作`
                        : "选择 Workspace（普通点击 = 当前会话临时 Workspace；Shift+点击 = 保存到该 Agent）"
                    }
                    aria-label={
                      activeWorkspaceDir
                        ? `${workspaceLabel} Workspace：${activeWorkspaceDir}`
                        : "选择 Workspace"
                    }
                    onClick={(e) => void handleChooseWorkspace(e)}
                    onContextMenu={(e) => {
                      e.preventDefault();
                      if (!sending) setWorkspaceMenuOpen((open) => !open);
                    }}
                    disabled={sending}
                  >
                    <span aria-hidden>📁</span>
                    {activeWorkspaceDir ? (
                      <span className="chat-workspace-pill-scope">{workspaceLabel}</span>
                    ) : null}
                    <span className="chat-workspace-pill-path">{workspaceSummary}</span>
                  </button>
                  {workspaceMenuOpen ? (
                    <div className="chat-workspace-menu" role="menu">
                      <button
                        type="button"
                        role="menuitem"
                        onClick={(e) => void handleChooseWorkspace(e)}
                      >
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
                        <button
                          type="button"
                          role="menuitem"
                          onClick={handleClearSessionWorkspace}
                        >
                          清除当前会话临时 Workspace
                        </button>
                      ) : null}
                    </div>
                  ) : null}
                </div>
              }
            />

            <input
              id={CHAT_ATTACHMENT_INPUT_ID}
              type="file"
              multiple
              disabled={sending}
              className="chat-file-input-hidden"
              aria-label="选择附件文件"
              onChange={handleAttachFilesChange}
            />

            <div
              className={`chat-input-row${composerDragActive ? " chat-input-row--drag-over" : ""}`}
              onDragEnter={handleComposerDragEnter}
              onDragLeave={handleComposerDragLeave}
              onDragOver={handleComposerDragOverFiles}
              onDrop={handleComposerDropFiles}
              aria-label="消息输入区域：可将文件拖入白色输入栏或点击下方曲别针添加附件"
            >
              {sending ? (
                <span className="chat-attach-btn chat-attach-btn--disabled" title="发送中暂不可添加附件">
                  <span aria-hidden>📎</span>
                </span>
              ) : isTauriEnvironment() ? (
                <button
                  type="button"
                  className="chat-attach-btn"
                  title="添加附件（系统文件对话框；亦可拖入输入框）"
                  onClick={() => void handleAttachTauri()}
                >
                  <span aria-hidden>📎</span>
                </button>
              ) : (
                <label
                  htmlFor={CHAT_ATTACHMENT_INPUT_ID}
                  className="chat-attach-btn"
                  title="添加附件（点击选择文件；亦可拖入输入框）"
                >
                  <span aria-hidden>📎</span>
                </label>
              )}
              <textarea
                className="chat-input"
                value={input}
                onChange={(e) => setActiveInput(e.target.value)}
                onKeyDown={handleKeyDown}
                onDragOver={handleComposerDragOverFiles}
                onDrop={handleComposerDropFiles}
                placeholder={
                  gatewayStatus === "connected"
                    ? "输入消息，Enter 发送…（支持拖入文件）"
                    : "网关未连接…（仍可拖入文件编辑草稿）"
                }
                rows={1}
                disabled={gatewayStatus !== "connected"}
              />
              {sending ? (
                <button
                  type="button"
                  className="chat-cancel-button"
                  onClick={handleCancel}
                >
                  取消
                </button>
              ) : (
                <button
                  type="button"
                  className="chat-send-fab"
                  onClick={() => void handleSend()}
                  disabled={!input.trim() || gatewayStatus !== "connected"}
                  aria-label="发送"
                >
                  ↑
                </button>
              )}
            </div>
            <div className="chat-composer-meta">
              <select
                className="chat-model-select"
                value={selectedModel}
                onChange={(e) => setSelectedModel(e.target.value)}
                title="模型"
              >
                {availableModels.map((m) => (
                  <option key={m} value={m}>
                    {m === "auto" ? "自动模型" : m}
                  </option>
                ))}
              </select>
              <label
                className={`chat-vision-toggle${desktopVisionMaster ? "" : " chat-vision-toggle--disabled"}`}
                title={
                  desktopVisionMaster
                    ? "发送时截取主屏幕并传给支持视觉的多模态模型"
                    : "请先在 设置 → 通用 中开启「桌面视觉监控」"
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
            <div className="chat-gateway-footer">
              <span
                className={`chat-gateway-dot chat-gateway-dot--${gatewayStatus}`}
                aria-hidden
              />
              <span className="chat-gateway-footer-text">
                {statusText} · port: {gatewayPort}
                {gatewayUrl ? ` · ${gatewayUrl}` : ""}
              </span>
            </div>
          </div>
        </main>
      </div>
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

    switch (status) {
      case "running":
        return "进行中";
      case "done":
        return "完成";
      case "error":
        return "失败";
      case "pending":
        return "等待";
      default:
        return "记录";
    }
  };

  return (
    <div className="chat-execution-steps">
      <div className="chat-execution-title">执行步骤</div>
      {liveSessionId ? (
        <div className="mt-1">
          <AgentRunTimeline
            events={[]}
            isRunning={true}
            defaultCollapsed={false}
            liveSessionId={liveSessionId}
          />
        </div>
      ) : null}
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

