import { useRef, useEffect, useState, useCallback, useMemo } from "react";
import { ChatMediaInteraction } from "./ChatMediaInteraction";
import { invokeTauri } from "../../utils/tauri";
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
  Config,
  GatewayHealth,
  GatewayInboundResponse,
  GatewayStatus,
  ProviderHealthSummary,
  ExecutionStep,
  RouteDecision,
} from "../../types/config";

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
  const [historyLoading, setHistoryLoading] = useState(false);
  // 输入草稿与运行状态按会话隔离，避免一个会话影响其它会话。
  const [inputs, setInputs] = useState<Record<string, string>>({});
  const [runs, setRuns] = useState<
    Record<string, { elapsedSec: number; steps: ExecutionStep[] }>
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
  const [composerDragActive, setComposerDragActive] = useState(false);
  const [desktopVisionMaster, setDesktopVisionMaster] = useState(false);
  const [desktopVisionOn, setDesktopVisionOn] = useState(false);
  const [desktopVisionMaxPx, setDesktopVisionMaxPx] = useState(1280);

  const activeSession = avatars.find((a) => a.id === activeAvatarId);
  const sessionId = activeSession?.sessionId ?? "omninova-chat-session";
  const messages = useMemo(
    () => messagesBySession[activeAvatarId] ?? [],
    [messagesBySession, activeAvatarId]
  );

  // 仅反映「当前查看的会话」的运行/输入状态。
  const activeRun = runs[activeAvatarId];
  const sending = Boolean(activeRun);
  const elapsedSec = activeRun?.elapsedSec ?? 0;
  const activeSteps = activeRun?.steps ?? [];
  const input = inputs[activeAvatarId] ?? "";

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
      const remote = await fetchWebSessionsFromGateway();
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
  }, []);

  useEffect(() => {
    saveChatStorage({
      avatars,
      activeAvatarId,
      messagesBySession,
    });
  }, [avatars, activeAvatarId, messagesBySession]);

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
    void invokeTauri<Config>("get_setup_config")
      .then((cfg) => {
        const master = cfg.multimodal?.desktop_vision_enabled ?? false;
        const maxPx = cfg.multimodal?.desktop_vision_max_dimension_px ?? 1280;
        setDesktopVisionMaster(master);
        setDesktopVisionMaxPx(maxPx);
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
    if (!stickToBottomRef.current && !sending) return;
    scrollMessagesToEnd("auto");
  }, [messages, sending, activeSteps, elapsedSec, scrollMessagesToEnd]);

  useEffect(() => {
    const timers = elapsedTimersRef.current;
    return () => {
      Object.values(timers).forEach((timer) => clearInterval(timer));
    };
  }, []);

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
    const name = `对话 ${avatars.length}`;
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
    // 仅取消当前查看的会话。
    cancelledRef.current[activeAvatarId] = true;
  }, [activeAvatarId]);

  const handleSend = async () => {
    // 绑定到「发送时」的会话，使后续状态更新只作用于该会话，
    // 即使用户中途切换到其它会话也互不影响。
    const avatarId = activeAvatarId;
    const targetSessionId = sessionId;
    const text = input.trim();
    if (!text || runs[avatarId]) return;

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
      setRuns((prev) =>
        prev[avatarId]
          ? { ...prev, [avatarId]: { ...prev[avatarId], steps } }
          : prev
      );
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
    const finishRun = () => {
      const timer = elapsedTimersRef.current[avatarId];
      if (timer) {
        clearInterval(timer);
        delete elapsedTimersRef.current[avatarId];
      }
      setRuns((prev) => {
        if (!prev[avatarId]) return prev;
        const next = { ...prev };
        delete next[avatarId];
        return next;
      });
    };

    setActiveInput("");
    setError(null);
    cancelledRef.current[avatarId] = false;
    setRuns((prev) => ({ ...prev, [avatarId]: { elapsedSec: 0, steps: localSteps } }));

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
      setRuns((prev) =>
        prev[avatarId]
          ? {
              ...prev,
              [avatarId]: {
                ...prev[avatarId],
                elapsedSec: prev[avatarId].elapsedSec + 1,
              },
            }
          : prev
      );
    }, 1000);

    let route: RouteDecision | null = null;
    try {
      const metadata: Record<string, unknown> = {
        preferred_provider: selectedModel === "auto" ? undefined : selectedModel,
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
          finishRun();
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
      const result = await invokeTauri<GatewayInboundResponse>("process_inbound_message", {
        payload,
      });

      if (cancelledRef.current[avatarId]) {
        setMessagesBySession((prev) => ({
          ...prev,
          [avatarId]: (prev[avatarId] ?? []).slice(0, -1),
        }));
        setInputs((prev) => ({ ...prev, [avatarId]: text }));
        return;
      }

      const replyText = result?.reply || "(空回复)";
      const steps = result?.steps?.length ? result.steps : localSteps;
      setMessagesBySession((prev) => ({
        ...prev,
        [avatarId]: [
          ...(prev[avatarId] ?? []),
          {
            role: "assistant",
            content: replyText,
            agent: result?.route?.agent_name,
            steps,
          },
        ],
      }));
    } catch (e) {
      if (cancelledRef.current[avatarId]) {
        setMessagesBySession((prev) => ({
          ...prev,
          [avatarId]: (prev[avatarId] ?? []).slice(0, -1),
        }));
        setInputs((prev) => ({ ...prev, [avatarId]: text }));
        return;
      }

      const msg = e instanceof Error ? e.message : String(e);
      const errorDetail = await buildSendErrorMessage(msg, route);
      const errorContent = `发送失败：${errorDetail}`;
      setError(errorContent);
      setMessagesBySession((prev) => ({
        ...prev,
        [avatarId]: [
          ...(prev[avatarId] ?? []),
          { role: "error", content: errorContent },
        ],
      }));
    } finally {
      finishRun();
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
            + 新对话
          </button>
          <section className="chat-sidebar-section">
            <h3 className="chat-sidebar-heading">会话</h3>
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
                    title="删除会话"
                    aria-label={`删除会话 ${a.name}`}
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
                当前对话对象：{activeSession?.name ?? "Main"}
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
                  与 {activeSession?.name ?? "Main"} 对话；历史记录会保存在网关并在下次打开时恢复。
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
                  <span className="typing-elapsed">{elapsedSec}s</span>
                </div>
                {activeSteps.length ? <ExecutionSteps steps={activeSteps} /> : null}
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
                disabled={sending || gatewayStatus !== "connected"}
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

function ExecutionSteps({ steps }: { steps: ExecutionStep[] }) {
  const statusLabel = (status?: ExecutionStep["status"]) => {
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
      <ol>
        {steps.map((step, index) => (
          <li key={`${step.title}-${index}`} className={`chat-execution-step chat-execution-step--${step.status ?? "info"}`}>
            <span className="chat-execution-step-title">{step.title}</span>
            <span className="chat-execution-step-status">{statusLabel(step.status)}</span>
            {step.detail ? <div className="chat-execution-step-detail">{step.detail}</div> : null}
          </li>
        ))}
      </ol>
    </div>
  );
}

async function buildSendErrorMessage(
  rawMessage: string,
  route: RouteDecision | null
) {
  const isConnectivityIssue =
    rawMessage.includes("请求超时") ||
    rawMessage.includes("连接失败") ||
    rawMessage.includes("网络请求失败") ||
    rawMessage.includes("timed out") ||
    rawMessage.includes("timeout") ||
    rawMessage.includes("connection refused") ||
    rawMessage.includes("connect error");

  if (!isConnectivityIssue) {
    return rawMessage;
  }

  try {
    const [gatewayHealth, providers] = await Promise.all([
      invokeTauri<GatewayHealth>("gateway_health"),
      invokeTauri<ProviderHealthSummary[]>("provider_health_overview"),
    ]);

    const routedProviderId =
      route?.provider ?? providers.find((item) => item.is_default)?.id ?? gatewayHealth.provider;
    const matchedProvider = providers.find((item) => item.id === routedProviderId);
    const agentHint = route?.agent_name ? `，Agent 为 ${route.agent_name}` : "";
    const providerHint = routedProviderId ? `，Provider 为 ${routedProviderId}` : "";

    if (!gatewayHealth.provider_healthy) {
      return `${rawMessage}。网关已响应，但当前 Provider 健康检查失败${providerHint}${agentHint}，请检查 API Key、Base URL、网络连通性或本地模型服务是否启动。`;
    }

    if (matchedProvider?.healthy === false) {
      return `${rawMessage}。路由命中的 Provider 健康检查失败${providerHint}${agentHint}，请优先检查该模型服务是否可达。`;
    }

    if (matchedProvider?.enabled === false) {
      return `${rawMessage}。当前路由命中的 Provider 未启用${providerHint}${agentHint}，请先在配置页启用并保存。`;
    }

    return `${rawMessage}。网关健康检查正常${providerHint}${agentHint}，更可能是模型推理耗时过长而不是网关断连。界面不会主动超时；如长期无响应，请检查上游模型服务状态。`;
  } catch {
    return `${rawMessage}。另外，超时后未能取得健康检查结果，请确认网关仍在运行，并检查上游模型服务是否可达。`;
  }
}
