import { useRef, useEffect, useState, useCallback, useMemo } from "react";
import { invokeTauri } from "../../utils/tauri";
import type {
  GatewayHealth,
  GatewayInboundResponse,
  GatewayStatus,
  ProviderHealthSummary,
  RouteDecision,
} from "../../types/config";
import omninovalLogo from "../../assets/omninoval-logo.png";

const USER_ID = "desktop-user";
const SEND_TIMEOUT_MS = 180_000;

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

interface ChatProps {
  onBack: () => void;
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

export function Chat({ onBack }: ChatProps) {
  const [avatars, setAvatars] = useState<AvatarSession[]>([
    { id: "main", name: "OmniNova", sessionId: "omninova-chat-session", lastAt: formatTime(new Date()) },
  ]);
  const [activeAvatarId, setActiveAvatarId] = useState("main");
  const [sidebarTab, setSidebarTab] = useState<SidebarTab>("avatars");
  const [messagesBySession, setMessagesBySession] = useState<Record<string, ChatMessage[]>>({
    main: [],
  });
  const [input, setInput] = useState("");
  const [sending, setSending] = useState(false);
  const [elapsedSec, setElapsedSec] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [gatewayStatus, setGatewayStatus] = useState<"connecting" | "connected" | "disconnected">("connecting");
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
    void refreshGatewayStatus();
    const t = setInterval(refreshGatewayStatus, 8000);
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
      setGatewayStatus(status.running ? "connected" : "disconnected");
    } catch {
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

  const handleCancel = useCallback(() => {
    cancelledRef.current = true;
  }, []);

  const handleSend = async () => {
    const text = input.trim();
    if (!text || sending) return;

    if (gatewayStatus !== "connected") {
      setError("网关未连接，请先在配置页启动网关后再发送消息");
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
        metadata: {},
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

  const statusText =
    gatewayStatus === "connected"
      ? "Gateway 已连接"
      : gatewayStatus === "connecting"
      ? "正在恢复连接…"
      : "等待 Gateway 连接…";

  return (
    <div className="chat-layout">
      <header className="chat-topbar">
        <div className="chat-topbar-left">
          <img src={omninovalLogo} alt="" className="chat-topbar-logo" />
          <span className="chat-topbar-title">OmniNova Claw</span>
          {sending && (
            <span className="chat-topbar-typing">
              正在输入中 ({elapsedSec}s)
            </span>
          )}
        </div>
        <div className="chat-topbar-right">
          <button
            type="button"
            className="chat-topbar-icon-btn"
            onClick={onBack}
            title="配置"
          >
            配置
          </button>
          <span className={`chat-topbar-status chat-topbar-status--${gatewayStatus}`}>
            {statusText}
          </span>
        </div>
      </header>

      <div className="chat-body">
        <aside className="chat-sidebar">
          <section className="chat-sidebar-section">
            <h3 className="chat-sidebar-heading">分身</h3>
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
            <button
              type="button"
              className="chat-new-avatar"
              onClick={handleAddAvatar}
            >
              + 新分身
            </button>
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
        </aside>

        <main className="chat-main">
          <div className="chat-connection-line">
            {gatewayStatus === "connecting" && "正在恢复连接…"}
            {gatewayStatus === "connected" && "已连接"}
            {gatewayStatus === "disconnected" && (
              <span className="chat-warn-text">
                未连接 — 请先在配置页启动网关
              </span>
            )}
          </div>

          <div className="chat-messages">
            {messages.length === 0 ? (
              <div className="chat-welcome">
                <p>输入消息开始与 {activeSession?.name ?? "OmniNova"} 对话。</p>
                <p>按 Enter 发送，Shift+Enter 换行。</p>
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

          <div className="chat-footer">
            <span className="chat-footer-status">{statusText}</span>
            <span className="chat-footer-model">OmniNova</span>
          </div>
          <div className="chat-input-row">
            <textarea
              className="chat-input"
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder={
                gatewayStatus === "connected"
                  ? "输入消息..."
                  : "网关未连接，请先启动网关..."
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
                className="chat-send-button"
                onClick={() => void handleSend()}
                disabled={!input.trim() || gatewayStatus !== "connected"}
              >
                发送
              </button>
            )}
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
  if (!rawMessage.includes("请求超时")) {
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
