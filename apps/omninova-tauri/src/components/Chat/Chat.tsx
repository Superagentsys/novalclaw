import { useRef, useEffect, useState } from "react";
import { invokeTauri } from "../../utils/tauri";
import type { GatewayInboundResponse, GatewayStatus } from "../../types/config";
import omninovalLogo from "../../assets/omninoval-logo.png";

const USER_ID = "desktop-user";

interface ChatMessage {
  role: "user" | "assistant";
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
  const [error, setError] = useState<string | null>(null);
  const [gatewayStatus, setGatewayStatus] = useState<"connecting" | "connected" | "disconnected">("connecting");
  const listEndRef = useRef<HTMLDivElement>(null);

  const activeSession = avatars.find((a) => a.id === activeAvatarId);
  const sessionId = activeSession?.sessionId ?? "omninova-chat-session";
  const messages = messagesBySession[activeAvatarId] ?? [];

  useEffect(() => {
    void refreshGatewayStatus();
    const t = setInterval(refreshGatewayStatus, 8000);
    return () => clearInterval(t);
  }, []);

  useEffect(() => {
    listEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

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

  const handleSend = async () => {
    const text = input.trim();
    if (!text || sending) return;

    setInput("");
    setError(null);
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

    try {
      const result = await invokeTauri<GatewayInboundResponse>(
        "process_inbound_message",
        {
          payload: {
            channel: "web",
            text,
            sessionId,
            userId: USER_ID,
            metadata: {},
          },
        }
      );
      setMessagesBySession((prev) => ({
        ...prev,
        [activeAvatarId]: [
          ...(prev[activeAvatarId] ?? []),
          {
            role: "assistant",
            content: result.reply,
            agent: result.route?.agent_name,
          },
        ],
      }));
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(`发送失败：${msg}`);
      setMessagesBySession((prev) => ({
        ...prev,
        [activeAvatarId]: (prev[activeAvatarId] ?? []).slice(0, -1),
      }));
      setInput(text);
    } finally {
      setSending(false);
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
      {/* 顶栏 */}
      <header className="chat-topbar">
        <div className="chat-topbar-left">
          <img src={omninovalLogo} alt="" className="chat-topbar-logo" />
          <span className="chat-topbar-title">OmniNova Claw</span>
          {sending && (
            <span className="chat-topbar-typing">正在输入中</span>
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
        {/* 左侧边栏 */}
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

        {/* 主对话区 */}
        <main className="chat-main">
          <div className="chat-connection-line">
            {gatewayStatus === "connecting" && "正在恢复连接…"}
            {gatewayStatus === "connected" && "已连接"}
            {gatewayStatus === "disconnected" && "未连接，请先在配置页启动网关"}
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
              </div>
            )}
            <div ref={listEndRef} />
          </div>

          {error && (
            <div className="chat-error" role="alert">
              {error}
            </div>
          )}

          <div className="chat-footer">
            <span className="chat-footer-status">{statusText}</span>
            <span className="chat-footer-model">OmniNova</span>
            <button type="button" className="chat-footer-attach" title="附件" aria-label="附件">
              📎
            </button>
          </div>
          <div className="chat-input-row">
            <textarea
              className="chat-input"
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="输入消息..."
              rows={1}
              disabled={sending}
            />
            <button
              type="button"
              className="chat-send-button"
              onClick={() => void handleSend()}
              disabled={sending || !input.trim()}
            >
              {sending ? "发送中…" : "发送"}
            </button>
          </div>
          <div className="chat-user-bar">
            <span className="chat-user-id">{USER_ID}</span>
          </div>
        </main>
      </div>
    </div>
  );
}
