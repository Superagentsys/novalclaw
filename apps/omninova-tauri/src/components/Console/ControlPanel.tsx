import { useEffect, useMemo, useState } from "react";
import { invokeTauri } from "../../utils/tauri";
import type {
  ChannelKindValue,
  GatewayHealth,
  GatewayInboundResponse,
  ProviderHealthSummary,
  RouteDecision,
  SessionTreeResponse,
} from "../../types/config";

const CHANNEL_OPTIONS: Array<{ value: ChannelKindValue; label: string }> = [
  { value: "cli", label: "CLI" },
  { value: "web", label: "Web" },
  { value: "telegram", label: "Telegram" },
  { value: "discord", label: "Discord" },
  { value: "slack", label: "Slack" },
  { value: "whatsapp", label: "WhatsApp" },
  { value: "matrix", label: "Matrix" },
  { value: "wechat", label: "WeChat" },
  { value: "feishu", label: "Feishu" },
  { value: "lark", label: "Lark" },
  { value: "dingtalk", label: "DingTalk" },
  { value: "email", label: "Email" },
  { value: "webhook", label: "Webhook" },
];

const EMPTY_METADATA = "{}";

interface SessionQueryForm {
  agent_name: string;
  channel: string;
  contains: string;
  limit: string;
  sort_by: "updated_at" | "spawn_depth" | "session_id" | "agent_name";
  sort_order: "asc" | "desc";
}

export function ControlPanel() {
  const [channel, setChannel] = useState<ChannelKindValue>("cli");
  const [text, setText] = useState("请帮我总结当前工作区的主要模块。");
  const [sessionId, setSessionId] = useState("console-demo");
  const [userId, setUserId] = useState("desktop-user");
  const [metadataText, setMetadataText] = useState(EMPTY_METADATA);
  const [routeDecision, setRouteDecision] = useState<RouteDecision | null>(null);
  const [chatResult, setChatResult] = useState<GatewayInboundResponse | null>(null);
  const [gatewayHealth, setGatewayHealth] = useState<GatewayHealth | null>(null);
  const [providerHealths, setProviderHealths] = useState<ProviderHealthSummary[]>([]);
  const [sessionTree, setSessionTree] = useState<SessionTreeResponse | null>(null);
  const [sessionQuery, setSessionQuery] = useState<SessionQueryForm>({
    agent_name: "",
    channel: "",
    contains: "",
    limit: "20",
    sort_by: "updated_at",
    sort_order: "desc",
  });
  const [busyAction, setBusyAction] = useState<
    "route" | "chat" | "health" | "sessions" | null
  >(null);
  const [statusMessage, setStatusMessage] = useState("控制面已就绪。");

  const parsedMetadata = useMemo(() => {
    const trimmed = metadataText.trim();
    if (!trimmed) {
      return { ok: true as const, value: {} };
    }
    try {
      const value = JSON.parse(trimmed) as unknown;
      if (!value || Array.isArray(value) || typeof value !== "object") {
        return {
          ok: false as const,
          error: "Metadata 必须是一个 JSON 对象，例如 {\"agent\":\"researcher\"}",
        };
      }
      return { ok: true as const, value: value as Record<string, unknown> };
    } catch (error) {
      return {
        ok: false as const,
        error: `Metadata JSON 解析失败：${
          error instanceof Error ? error.message : String(error)
        }`,
      };
    }
  }, [metadataText]);

  useEffect(() => {
    void refreshHealth();
    void refreshSessions();
  }, []);

  const buildInboundPayload = () => {
    if (!parsedMetadata.ok) {
      throw new Error(parsedMetadata.error);
    }
    const payload = {
      channel,
      text: text.trim(),
      sessionId: sessionId.trim() || undefined,
      userId: userId.trim() || undefined,
      metadata: parsedMetadata.value,
    };
    if (!payload.text) {
      throw new Error("请输入要调试的消息内容。");
    }
    return payload;
  };

  const handlePreviewRoute = async () => {
    setBusyAction("route");
    try {
      const payload = buildInboundPayload();
      const result = await invokeTauri<RouteDecision>("route_inbound_message", {
        payload,
      });
      setRouteDecision(result);
      setStatusMessage(`已完成路由预览，命中 Agent：${result.agent_name}`);
    } catch (error) {
      setStatusMessage(
        `路由预览失败：${error instanceof Error ? error.message : String(error)}`
      );
    } finally {
      setBusyAction(null);
    }
  };

  const handleSendMessage = async () => {
    setBusyAction("chat");
    try {
      const payload = buildInboundPayload();
      const result = await invokeTauri<GatewayInboundResponse>(
        "process_inbound_message",
        { payload }
      );
      setChatResult(result);
      setRouteDecision(result.route);
      setStatusMessage(`消息已处理完成，回复来自 Agent：${result.route.agent_name}`);
      await refreshSessions();
    } catch (error) {
      setStatusMessage(
        `发送消息失败：${error instanceof Error ? error.message : String(error)}`
      );
    } finally {
      setBusyAction(null);
    }
  };

  const refreshHealth = async () => {
    setBusyAction("health");
    try {
      const [health, providers] = await Promise.all([
        invokeTauri<GatewayHealth>("gateway_health"),
        invokeTauri<ProviderHealthSummary[]>("provider_health_overview"),
      ]);
      setGatewayHealth(health);
      setProviderHealths(providers);
      setStatusMessage("已刷新网关与 Provider 健康状态。");
    } catch (error) {
      setStatusMessage(
        `健康检查失败：${error instanceof Error ? error.message : String(error)}`
      );
    } finally {
      setBusyAction(null);
    }
  };

  const refreshSessions = async () => {
    setBusyAction("sessions");
    try {
      const limit = Number.parseInt(sessionQuery.limit, 10);
      const query = {
        agent_name: sessionQuery.agent_name.trim() || undefined,
        channel: sessionQuery.channel.trim() || undefined,
        contains: sessionQuery.contains.trim() || undefined,
        limit: Number.isFinite(limit) && limit > 0 ? limit : undefined,
        sort_by: sessionQuery.sort_by,
        sort_order: sessionQuery.sort_order,
      };
      const snapshot = await invokeTauri<SessionTreeResponse>(
        "session_tree_snapshot",
        { query }
      );
      setSessionTree(snapshot);
      setStatusMessage(`已加载 ${snapshot.returned} 条会话树记录。`);
    } catch (error) {
      setStatusMessage(
        `会话树加载失败：${error instanceof Error ? error.message : String(error)}`
      );
    } finally {
      setBusyAction(null);
    }
  };

  return (
    <div className="setup-stack">
      <section className="setup-section">
        <div className="section-heading">
          <div>
            <h2>控制面调试台</h2>
            <div className="section-subtitle">
              直接在桌面端预览路由、发送调试消息、查看会话树和 Provider 健康状态。
            </div>
          </div>
          <div className="gateway-status-chip is-running">{statusMessage}</div>
        </div>

        <div className="control-grid">
          <label>
            Channel
            <select
              value={channel}
              onChange={(event) =>
                setChannel(event.target.value as ChannelKindValue)
              }
            >
              {CHANNEL_OPTIONS.map((item) => (
                <option key={item.value} value={item.value}>
                  {item.label}
                </option>
              ))}
            </select>
          </label>
          <label>
            Session ID
            <input
              value={sessionId}
              onChange={(event) => setSessionId(event.target.value)}
              placeholder="例如 console-demo"
            />
          </label>
          <label>
            User ID
            <input
              value={userId}
              onChange={(event) => setUserId(event.target.value)}
              placeholder="例如 desktop-user"
            />
          </label>
        </div>

        <label>
          调试消息
          <textarea
            value={text}
            onChange={(event) => setText(event.target.value)}
            className="console-textarea"
          />
        </label>

        <label>
          Metadata JSON
          <textarea
            value={metadataText}
            onChange={(event) => setMetadataText(event.target.value)}
            className="console-textarea console-textarea-code"
            placeholder='{"agent":"researcher","accountId":"eng-team"}'
          />
        </label>

        {!parsedMetadata.ok ? (
          <div className="gateway-status-error">{parsedMetadata.error}</div>
        ) : null}

        <div className="setup-actions">
          <button
            type="button"
            onClick={() => void handlePreviewRoute()}
            disabled={busyAction !== null}
          >
            {busyAction === "route" ? "预览中..." : "预览路由"}
          </button>
          <button
            type="button"
            className="primary-button"
            onClick={() => void handleSendMessage()}
            disabled={busyAction !== null}
          >
            {busyAction === "chat" ? "处理中..." : "发送调试消息"}
          </button>
        </div>

        <div className="control-columns">
          <div className="control-card">
            <h3>路由结果</h3>
            <pre className="result-panel">
              {routeDecision
                ? JSON.stringify(routeDecision, null, 2)
                : "尚未执行路由预览。"}
            </pre>
          </div>
          <div className="control-card">
            <h3>回复结果</h3>
            <pre className="result-panel">
              {chatResult ? chatResult.reply : "尚未发送调试消息。"}
            </pre>
          </div>
        </div>
      </section>

      <section className="setup-section">
        <div className="section-heading">
          <div>
            <h2>健康检查</h2>
            <div className="section-subtitle">
              对照 `openclaw-main` 的控制面能力，补上桌面端对网关与 Provider 的可见性。
            </div>
          </div>
          <button
            type="button"
            onClick={() => void refreshHealth()}
            disabled={busyAction !== null}
          >
            {busyAction === "health" ? "刷新中..." : "刷新健康状态"}
          </button>
        </div>

        <div className="health-grid">
          <div className="control-card">
            <h3>网关状态</h3>
            <div className="stat-list">
              <div className="stat-row">
                <span>Provider</span>
                <strong>{gatewayHealth?.provider ?? "-"}</strong>
              </div>
              <div className="stat-row">
                <span>Provider 健康</span>
                <strong>
                  {gatewayHealth
                    ? gatewayHealth.provider_healthy
                      ? "正常"
                      : "异常"
                    : "-"}
                </strong>
              </div>
              <div className="stat-row">
                <span>Memory 健康</span>
                <strong>
                  {gatewayHealth
                    ? gatewayHealth.memory_healthy
                      ? "正常"
                      : "异常"
                    : "-"}
                </strong>
              </div>
            </div>
          </div>

          <div className="control-card">
            <h3>Provider 概览</h3>
            <div className="provider-health-list">
              {providerHealths.length === 0 ? (
                <div className="empty-state">尚未发现 Provider 配置。</div>
              ) : (
                providerHealths.map((provider) => (
                  <div key={provider.id} className="provider-health-item">
                    <div className="provider-health-header">
                      <strong>{provider.name}</strong>
                      <span
                        className={`provider-health-badge ${
                          provider.healthy === true
                            ? "is-ok"
                            : provider.healthy === false
                            ? "is-bad"
                            : "is-idle"
                        }`}
                      >
                        {!provider.enabled
                          ? "未启用"
                          : provider.healthy
                          ? "正常"
                          : "异常"}
                      </span>
                    </div>
                    <div className="provider-health-meta">
                      <span>{provider.id}</span>
                      <span>{provider.model ?? "未配置模型"}</span>
                      <span>{provider.is_default ? "默认 Provider" : "备用 Provider"}</span>
                    </div>
                    <code className="inline-code-block">
                      {provider.base_url ?? "使用 SDK 默认地址"}
                    </code>
                  </div>
                ))
              )}
            </div>
          </div>
        </div>
      </section>

      <section className="setup-section">
        <div className="section-heading">
          <div>
            <h2>会话树浏览</h2>
            <div className="section-subtitle">
              查看本地持久化的会话血缘、Agent 分布和分页信息。
            </div>
          </div>
          <button
            type="button"
            onClick={() => void refreshSessions()}
            disabled={busyAction !== null}
          >
            {busyAction === "sessions" ? "加载中..." : "刷新会话树"}
          </button>
        </div>

        <div className="control-grid">
          <label>
            Agent
            <input
              value={sessionQuery.agent_name}
              onChange={(event) =>
                setSessionQuery((current) => ({
                  ...current,
                  agent_name: event.target.value,
                }))
              }
              placeholder="例如 researcher"
            />
          </label>
          <label>
            Channel
            <input
              value={sessionQuery.channel}
              onChange={(event) =>
                setSessionQuery((current) => ({
                  ...current,
                  channel: event.target.value,
                }))
              }
              placeholder="例如 cli / web / wechat"
            />
          </label>
          <label>
            搜索关键词
            <input
              value={sessionQuery.contains}
              onChange={(event) =>
                setSessionQuery((current) => ({
                  ...current,
                  contains: event.target.value,
                }))
              }
              placeholder="session id / agent / parent"
            />
          </label>
          <label>
            返回条数
            <input
              value={sessionQuery.limit}
              onChange={(event) =>
                setSessionQuery((current) => ({
                  ...current,
                  limit: event.target.value,
                }))
              }
              placeholder="20"
            />
          </label>
          <label>
            排序字段
            <select
              value={sessionQuery.sort_by}
              onChange={(event) =>
                setSessionQuery((current) => ({
                  ...current,
                  sort_by: event.target.value as SessionQueryForm["sort_by"],
                }))
              }
            >
              <option value="updated_at">updated_at</option>
              <option value="spawn_depth">spawn_depth</option>
              <option value="session_id">session_id</option>
              <option value="agent_name">agent_name</option>
            </select>
          </label>
          <label>
            排序方向
            <select
              value={sessionQuery.sort_order}
              onChange={(event) =>
                setSessionQuery((current) => ({
                  ...current,
                  sort_order: event.target.value as SessionQueryForm["sort_order"],
                }))
              }
            >
              <option value="desc">desc</option>
              <option value="asc">asc</option>
            </select>
          </label>
        </div>

        <div className="session-summary-grid">
          <div className="mini-stat">
            <span>筛选前</span>
            <strong>{sessionTree?.total_before_filter ?? 0}</strong>
          </div>
          <div className="mini-stat">
            <span>筛选后</span>
            <strong>{sessionTree?.total_after_filter ?? 0}</strong>
          </div>
          <div className="mini-stat">
            <span>唯一 Agent</span>
            <strong>{sessionTree?.stats_after_filter.unique_agents ?? 0}</strong>
          </div>
          <div className="mini-stat">
            <span>最大深度</span>
            <strong>{sessionTree?.stats_after_filter.max_spawn_depth ?? 0}</strong>
          </div>
        </div>

        <div className="session-table-wrap">
          <table className="session-table">
            <thead>
              <tr>
                <th>Session</th>
                <th>Channel</th>
                <th>Agent</th>
                <th>Parent Agent</th>
                <th>Depth</th>
                <th>Source</th>
                <th>Updated</th>
              </tr>
            </thead>
            <tbody>
              {sessionTree?.sessions.length ? (
                sessionTree.sessions.map((item) => (
                  <tr key={item.session_key ?? `${item.channel}-${item.session_id}`}>
                    <td>{item.session_id ?? item.session_key ?? "-"}</td>
                    <td>{item.channel ?? "-"}</td>
                    <td>{item.agent_name ?? "-"}</td>
                    <td>{item.parent_agent_id ?? "-"}</td>
                    <td>{item.spawn_depth}</td>
                    <td>{item.source}</td>
                    <td>{formatTimestamp(item.updated_at)}</td>
                  </tr>
                ))
              ) : (
                <tr>
                  <td colSpan={7} className="table-empty-cell">
                    暂无会话记录
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </section>
    </div>
  );
}

function formatTimestamp(value: number) {
  if (!value) {
    return "-";
  }
  return new Date(value * 1000).toLocaleString("zh-CN", {
    hour12: false,
  });
}
