import { useRef, useEffect, useState, useCallback, useMemo } from "react";
import { ChatMediaInteraction } from "./ChatMediaInteraction";
import { invokeTauri } from "../../utils/tauri";
import type {
  GatewayHealth,
  GatewayInboundResponse,
  GatewayStatus,
  ProviderHealthSummary,
  RouteDecision,
} from "../../types/config";

const GATEWAY_STATUS_POLL_MS = 8000;
import omninovalLogo from "../../assets/omninoval-logo.png";

const USER_ID = "desktop-user";
const SEND_TIMEOUT_MS = 90_000;

interface ChatMessage {
  role: "user" | "assistant" | "error";
  content: string;
  agent?: string;
}

interface AvatarSession {
  id: string;
  name: string;
  sessionId: string;
  lastAt: string;
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

function formatTime(date: Date) {
  return date.toLocaleTimeString("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
}

function withTimeout<T>(promise: Promise<T>, ms: number): Promise<T> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(
      () =>
        reject(
          new Error(
            `请求超时（${Math.round(ms / 1000)}s），正在补充诊断信息，请稍候重试`
          )
        ),
      ms
    );
    promise.then(
      (v) => { clearTimeout(timer); resolve(v); },
      (e) => { clearTimeout(timer); reject(e); }
    );
  });
}

export function Chat({ initialSidebarTab = "avatars" }: ChatProps) {
  const [avatars, setAvatars] = useState<AvatarSession[]>([
    { id: "main", name: "Main", sessionId: "omninova-chat-session", lastAt: formatTime(new Date()) },
  ]);
  const [activeAvatarId, setActiveAvatarId] = useState("main");
  const [sidebarTab, setSidebarTab] = useState<SidebarTab>(initialSidebarTab);
  const [channels, setChannels] = useState<ImChannelEntry[]>([]);
  const [scheduledTasks, setScheduledTasks] = useState<ScheduledTaskEntry[]>([]);
  const [newChannelName, setNewChannelName] = useState("");
  const [newChannelPlatform, setNewChannelPlatform] = useState(IM_PLATFORM_OPTIONS[0]);
  const [newTaskName, setNewTaskName] = useState("");
  const [newTaskCron, setNewTaskCron] = useState("0 9 * * *");
  const [messagesBySession, setMessagesBySession] = useState<Record<string, ChatMessage[]>>({
    main: [],
  });
  const [input, setInput] = useState("");
  const [sending, setSending] = useState(false);
  const [elapsedSec, setElapsedSec] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [gatewayStatus, setGatewayStatus] = useState<"connecting" | "connected" | "disconnected">("connecting");
  const [gatewayUrl, setGatewayUrl] = useState<string>("");
  const [availableModels] = useState<string[]>(["auto", "openai", "anthropic", "gemini", "ollama"]);
  const [selectedModel, setSelectedModel] = useState("auto");
  const listEndRef = useRef<HTMLDivElement>(null);
  const cancelledRef = useRef(false);
  const elapsedTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const activeSession = avatars.find((a) => a.id === activeAvatarId);
  const sessionId = activeSession?.sessionId ?? "omninova-chat-session";
  const messages = useMemo(
    () => messagesBySession[activeAvatarId] ?? [],
    [messagesBySession, activeAvatarId]
  );

  useEffect(() => {
    setSidebarTab(initialSidebarTab);
  }, [initialSidebarTab]);

  useEffect(() => {
    void refreshGatewayStatus();
    const t = setInterval(refreshGatewayStatus, GATEWAY_STATUS_POLL_MS);
    return () => clearInterval(t);
  }, []);

  useEffect(() => {
    listEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  useEffect(() => {
    return () => {
      if (elapsedTimerRef.current) clearInterval(elapsedTimerRef.current);
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
    const name = `分身 ${avatars.length + 1}`;
    setAvatars((prev) => [
      ...prev,
      { id, name, sessionId: `session-${id}`, lastAt: formatTime(new Date()) },
    ]);
    setMessagesBySession((prev) => ({ ...prev, [id]: [] }));
    setActiveAvatarId(id);
  };

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
    cancelledRef.current = true;
  }, []);

  const handleSend = async () => {
    const text = input.trim();
    if (!text || sending) return;

    if (gatewayStatus !== "connected") {
      setError("网关未连接，请先在侧栏「设置」中启动网关后再发送消息");
      return;
    }

    setInput("");
    setError(null);
    cancelledRef.current = false;
    setElapsedSec(0);

    setMessagesBySession((prev) => ({
      ...prev,
      [activeAvatarId]: [...(prev[activeAvatarId] ?? []), { role: "user", content: text }],
    }));
    setAvatars((prev) =>
      prev.map((a) =>
        a.id === activeAvatarId ? { ...a, lastAt: formatTime(new Date()) } : a
      )
    );
    setSending(true);

    elapsedTimerRef.current = setInterval(() => {
      setElapsedSec((s) => s + 1);
    }, 1000);

    let route: RouteDecision | null = null;
    try {
      const payload = {
        channel: "web" as const,
        text,
        sessionId,
        userId: USER_ID,
        metadata: { preferred_provider: selectedModel === "auto" ? undefined : selectedModel },
      };
      route = await invokeTauri<RouteDecision>("route_inbound_message", {
        payload,
      }).catch(() => null);
      const result = await withTimeout(
        invokeTauri<GatewayInboundResponse>("process_inbound_message", {
          payload,
        }),
        SEND_TIMEOUT_MS
      );

      if (cancelledRef.current) {
        setMessagesBySession((prev) => ({
          ...prev,
          [activeAvatarId]: (prev[activeAvatarId] ?? []).slice(0, -1),
        }));
        setInput(text);
        return;
      }

      const replyText = result?.reply || "(空回复)";
      setMessagesBySession((prev) => ({
        ...prev,
        [activeAvatarId]: [
          ...(prev[activeAvatarId] ?? []),
          {
            role: "assistant",
            content: replyText,
            agent: result?.route?.agent_name,
          },
        ],
      }));
    } catch (e) {
      if (cancelledRef.current) {
        setMessagesBySession((prev) => ({
          ...prev,
          [activeAvatarId]: (prev[activeAvatarId] ?? []).slice(0, -1),
        }));
        setInput(text);
        return;
      }

      const msg = e instanceof Error ? e.message : String(e);
      const errorDetail = await buildSendErrorMessage(msg, route);
      const errorContent = `发送失败：${errorDetail}`;
      setError(errorContent);
      setMessagesBySession((prev) => ({
        ...prev,
        [activeAvatarId]: [
          ...(prev[activeAvatarId] ?? []),
          { role: "error", content: errorContent },
        ],
      }));
    } finally {
      setSending(false);
      setElapsedSec(0);
      if (elapsedTimerRef.current) {
        clearInterval(elapsedTimerRef.current);
        elapsedTimerRef.current = null;
      }
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      void handleSend();
    }
  };

  const appendVoiceTranscript = useCallback((text: string) => {
    setInput((prev) => (prev.trim() ? `${prev} ${text}` : text));
  }, []);

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
                <li key={a.id}>
                  <button
                    type="button"
                    className={`chat-avatar-item ${a.id === activeAvatarId ? "is-active" : ""}`}
                    onClick={() => setActiveAvatarId(a.id)}
                  >
                    <span className="chat-avatar-icon">◇</span>
                    <span className="chat-avatar-name">{a.name}</span>
                    <span className="chat-avatar-time">{a.lastAt}</span>
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
                title="刷新网关状态"
                onClick={() => void refreshGatewayStatus()}
              >
                ↻
              </button>
              {sending ? (
                <span className="chat-typing-badge">
                  正在回复 {elapsedSec}s
                </span>
              ) : null}
            </div>
          </div>

          <div className="chat-messages">
            {messages.length === 0 ? (
              <div className="chat-hero">
                <h1 className="chat-hero-title">我能为你做些什么？</h1>
                <p className="chat-hero-sub">
                  与 {activeSession?.name ?? "Main"} 对话，或通过侧栏管理频道与定时任务。
                </p>
                <div className="chat-quick-pills">
                  {quickPrompts.map((q) => (
                    <button
                      key={q.label}
                      type="button"
                      className="chat-quick-pill"
                      onClick={() => setInput(q.text)}
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
                </div>
              ))
            )}
            {sending && (
              <div className="chat-bubble chat-bubble-assistant chat-bubble-typing">
                <span className="typing-dot" />
                <span className="typing-dot" />
                <span className="typing-dot" />
                <span className="typing-elapsed">{elapsedSec}s</span>
              </div>
            )}
            <div ref={listEndRef} />
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
            <div className="chat-input-row">
              <button
                type="button"
                className="chat-attach-btn"
                title="附件（即将支持）"
                disabled
              >
                <span aria-hidden>📎</span>
              </button>
              <textarea
                className="chat-input"
                value={input}
                onChange={(e) => setInput(e.target.value)}
                onKeyDown={handleKeyDown}
                placeholder={
                  gatewayStatus === "connected"
                    ? "输入消息，Enter 发送…"
                    : "网关未连接…"
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

    return `${rawMessage}。网关健康检查正常${providerHint}${agentHint}，更可能是模型推理耗时过长而不是网关断连。可以稍后重试，或检查上游模型服务响应速度。`;
  } catch {
    return `${rawMessage}。另外，超时后未能取得健康检查结果，请确认网关仍在运行，并检查上游模型服务是否可达。`;
  }
}
