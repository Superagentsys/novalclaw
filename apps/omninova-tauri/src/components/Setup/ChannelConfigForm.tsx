import { useEffect, useMemo, useState } from "react";
import {
  CHANNEL_PRESETS,
  CLEAR_SENSITIVE_FIELDS_KEY,
  type ChannelEntryConfig,
  type ChannelsConfig,
  type ChannelPreset,
  type ChannelField,
} from "../../types/config";

interface ChannelConfigFormProps {
  value: ChannelsConfig;
  onChange: (channels: ChannelsConfig) => void;
  validationError?: string;
  onValidationChange?: (error: string | undefined) => void;
  gatewayUrl?: string;
  onHealthCheck?: () => Promise<{ ok: boolean; message?: string }>;
  onCopyWebhookUrl?: (url: string) => void;
  selectedChannelId?: string;
  onSelectedChannelChange?: (channelId: string) => void;
}

/** Get card callback path for feishu channel (used when channel is feishu) */
function getCardCallbackPath(_channelId: string): string {
  return "/webhook/feishu/card";
}

/** Get webhook path for a channel */
function getWebhookPath(channelId: string): string {
  switch (channelId) {
    case "feishu": return "/webhook/feishu";
    case "lark": return "/webhook/lark";
    case "wechat": return "/webhook/wechat";
    case "dingtalk": return "/webhook/dingtalk";
    case "webhook": return "/webhook";
    default: return "/webhook";
  }
}

/** Check if URL is localhost/127.0.0.1 */
function isLocalhost(url: string): boolean {
  return url.includes("127.0.0.1") || url.includes("localhost");
}

/** Store only a public base URL; the webhook path is appended for display. */
function normalizePublicWebhookBaseUrl(value: string): string {
  return value
    .trim()
    .replace(/\/webhook\/feishu\/card\/?$/i, "")
    .replace(/\/webhook\/feishu\/?$/i, "")
    .replace(/\/$/, "");
}

const EMPTY_ENTRY: ChannelEntryConfig = {
  enabled: false,
  extra: {},
};

const DEFAULT_CHANNEL_ID = "feishu";

/** Feishu/Lark channels require app_id and app_secret in extra */
const FEISHU_LIKE_CHANNEL_IDS = new Set(["feishu", "lark"]);

export function ChannelConfigForm({
  value,
  onChange,
  validationError,
  onValidationChange,
  gatewayUrl,
  onHealthCheck,
  onCopyWebhookUrl,
  selectedChannelId,
  onSelectedChannelChange,
}: ChannelConfigFormProps) {
  const [uncontrolledSelectedId, setUncontrolledSelectedId] = useState<string>(DEFAULT_CHANNEL_ID);
  const selectedId = selectedChannelId ?? uncontrolledSelectedId;
  const [healthStatus, setHealthStatus] = useState<"idle" | "checking" | "ok" | "error">("idle");
  const [healthMessage, setHealthMessage] = useState<string>("");
  const [copyStatus, setCopyStatus] = useState<string>("");
  const [copyCardStatus, setCopyCardStatus] = useState<string>("");
  const [publicUrlNormalizationNotice, setPublicUrlNormalizationNotice] = useState("");

  const selectedPreset: ChannelPreset | undefined = useMemo(
    () => CHANNEL_PRESETS.find((preset) => preset.id === selectedId),
    [selectedId]
  );

  const webhookPath = useMemo(() => getWebhookPath(selectedId), [selectedId]);
  const cardCallbackPath = useMemo(
    () => selectedId === "feishu" ? getCardCallbackPath(selectedId) : "",
    [selectedId]
  );

  /** Handle health check button click */
  const handleHealthCheck = async () => {
    if (!onHealthCheck) return;
    setHealthStatus("checking");
    setHealthMessage("");
    try {
      const result = await onHealthCheck();
      if (result.ok) {
        setHealthStatus("ok");
        setHealthMessage("Gateway 健康检查通过");
      } else {
        setHealthStatus("error");
        setHealthMessage(result.message || "健康检查失败");
      }
    } catch (err) {
      setHealthStatus("error");
      setHealthMessage(err instanceof Error ? err.message : "连接失败");
    }
  };

  /** Handle copy webhook URL */
  const handleCopyWebhookUrl = () => {
    if (!fullWebhookUrl) return;
    if (onCopyWebhookUrl) {
      onCopyWebhookUrl(fullWebhookUrl);
    } else {
      void navigator.clipboard.writeText(fullWebhookUrl);
    }
    setCopyStatus("已复制");
    setTimeout(() => setCopyStatus(""), 2000);
  };

  /** Handle copy card callback URL */
  const handleCopyCardCallbackUrl = () => {
    if (!fullCardCallbackUrl) return;
    void navigator.clipboard.writeText(fullCardCallbackUrl);
    setCopyCardStatus("已复制");
    setTimeout(() => setCopyCardStatus(""), 2000);
  };

  /** Reset health status when gateway URL changes. */
  useEffect(() => {
    setHealthStatus("idle");
    setHealthMessage("");
  }, [gatewayUrl]);

  const selectChannel = (channelId: string) => {
    setUncontrolledSelectedId(channelId);
    onSelectedChannelChange?.(channelId);
  };

  const visibleFields = useMemo(() => {
    if (!selectedPreset) return [];
    // Lark remains limited to its existing credentials. Feishu alone exposes
    // webhook security controls because its inbound endpoint is handled here.
    if (FEISHU_LIKE_CHANNEL_IDS.has(selectedPreset.id)) {
      const base = new Set(["app_id", "app_secret", "outbound_mode"]);
      return selectedPreset.fields.filter((field) =>
        selectedPreset.id === "feishu"
          ? base.has(field.key)
            || field.key === "security_mode"
            || field.key === "verification_token"
            || field.key === "encrypt_key"
            || field.key === "public_webhook_base_url"
          : base.has(field.key)
      );
    }
    return selectedPreset.fields;
  }, [selectedPreset]);

  const getEntry = (id: keyof ChannelsConfig): ChannelEntryConfig =>
    value[id] ?? { ...EMPTY_ENTRY, extra: {} };

  const setEntry = (id: keyof ChannelsConfig, entry: ChannelEntryConfig) => {
    // Clean up empty strings and empty extra
    const cleanedEntry: ChannelEntryConfig = {
      ...entry,
      token: entry.token?.trim() || undefined,
      token_env: entry.token_env?.trim() || undefined,
      extra: entry.extra
        ? Object.fromEntries(
            Object.entries(entry.extra).filter(([, v]) => v.trim() !== "")
          )
        : undefined,
    };
    if (Object.keys(cleanedEntry.extra || {}).length === 0) {
      cleanedEntry.extra = undefined;
    }
    onChange({ ...value, [id]: cleanedEntry });
  };

  const enabledList = CHANNEL_PRESETS.filter(
    (preset) => getEntry(preset.id).enabled
  ).map((preset) => preset.name);

  const entry = selectedPreset ? getEntry(selectedPreset.id) : { ...EMPTY_ENTRY, extra: {} };
  const publicWebhookBaseUrl = selectedPreset?.id === "feishu"
    ? normalizePublicWebhookBaseUrl(entry.extra?.["public_webhook_base_url"] ?? "")
    : "";
  const webhookBaseUrl = publicWebhookBaseUrl || gatewayUrl?.trim() || "";
  const fullWebhookUrl = webhookBaseUrl
    ? `${webhookBaseUrl.replace(/\/$/, "")}${webhookPath}`
    : "";
  const fullCardCallbackUrl = webhookBaseUrl
    ? `${webhookBaseUrl.replace(/\/$/, "")}${cardCallbackPath}`
    : "";
  const isLocal = webhookBaseUrl ? isLocalhost(webhookBaseUrl) : false;

  /** Get field value: check extra for extra fields, otherwise direct property */
  const getFieldValue = (field: ChannelField): string => {
    if (field.isExtra) {
      return entry.extra?.[field.key] ?? "";
    }
    const val = (entry as unknown as Record<string, unknown>)[field.key];
    return typeof val === "string" ? val : "";
  };

  /** Handle field value change */
  const handleFieldChange = (field: ChannelField, fieldValue: string | boolean) => {
    if (!selectedPreset) return;

    if (field.key === "enabled") {
      setEntry(selectedPreset.id, { ...entry, enabled: fieldValue as boolean });
      return;
    }

    const updatedEntry = { ...entry };
    if (field.isExtra) {
      let normalizedFieldValue = fieldValue as string;
      if (field.key === "public_webhook_base_url" && typeof fieldValue === "string") {
        normalizedFieldValue = normalizePublicWebhookBaseUrl(fieldValue);
        setPublicUrlNormalizationNotice(
          normalizedFieldValue !== fieldValue.trim().replace(/\/$/, "")
            ? "已自动移除 /webhook/feishu 或 /webhook/feishu/card，仅保存 Public Base URL。"
            : ""
        );
      }
      updatedEntry.extra = { ...(updatedEntry.extra ?? {}), [field.key]: normalizedFieldValue };
      if (
        (field.key === "verification_token" || field.key === "encrypt_key") &&
        typeof fieldValue === "string" &&
        fieldValue.trim()
      ) {
        const clearFields = (updatedEntry.extra[CLEAR_SENSITIVE_FIELDS_KEY] ?? "")
          .split(",")
          .map((value) => value.trim())
          .filter((value) => value && value !== field.key);
        if (clearFields.length) {
          updatedEntry.extra[CLEAR_SENSITIVE_FIELDS_KEY] = clearFields.join(",");
        } else {
          delete updatedEntry.extra[CLEAR_SENSITIVE_FIELDS_KEY];
        }
      }
    } else {
      (updatedEntry as unknown as Record<string, unknown>)[field.key] = fieldValue;
    }
    if (field.key === "app_secret" && typeof fieldValue === "string" && fieldValue.trim()) {
      updatedEntry.clear_app_secret = false;
    }
    setEntry(selectedPreset.id, updatedEntry);
  };

  const clearSensitiveField = (field: "app_secret" | "verification_token" | "encrypt_key") => {
    if (!selectedPreset) return;
    const updatedEntry: ChannelEntryConfig = {
      ...entry,
      extra: { ...(entry.extra ?? {}), [field]: "" },
    };
    if (field === "app_secret") {
      updatedEntry.clear_app_secret = true;
    } else {
      const requested = new Set(
        (updatedEntry.extra?.[CLEAR_SENSITIVE_FIELDS_KEY] ?? "")
          .split(",")
          .map((value) => value.trim())
          .filter(Boolean)
      );
      requested.add(field);
      updatedEntry.extra![CLEAR_SENSITIVE_FIELDS_KEY] = [...requested].join(",");
    }
    setEntry(selectedPreset.id, updatedEntry);
  };

  /** Validate only the channel currently being edited. */
  const validateFeishuLike = (): string | undefined => {
    if (!FEISHU_LIKE_CHANNEL_IDS.has(selectedPreset?.id ?? "")) {
      return undefined;
    }
    if (!entry.enabled) {
      return undefined;
    }
    const appId = entry.extra?.["app_id"] ?? "";
    const appSecret = entry.extra?.["app_secret"] ?? "";
    const outboundMode = entry.extra?.["outbound_mode"] ?? "disabled";
    const channelName = selectedPreset?.id === "lark" ? "Lark" : "Feishu";
    
    if (!appId.trim()) {
      return `启用 ${channelName} 时必须填写 App ID`;
    }
    
    // Only require app_secret when outbound_mode is "real" or "mock"
    if ((outboundMode === "real" || outboundMode === "mock") && !appSecret.trim()) {
      return `启用 ${channelName} ${outboundMode} outbound 时必须填写 App Secret`;
    }
    
    if (selectedPreset?.id === "feishu") {
      const securityMode = entry.extra?.["security_mode"] || "dev";
      const verificationToken = entry.extra?.["verification_token"] || "";
      const encryptKey = entry.extra?.["encrypt_key"] || "";
      if ((securityMode === "token" || securityMode === "encrypted") && !verificationToken.trim()) {
        return "Feishu token / encrypted 模式必须填写 Verification Token";
      }
      if (securityMode === "encrypted" && !encryptKey.trim()) {
        return "Feishu encrypted 模式必须填写 Encrypt Key";
      }
    }

    return undefined;
  };

  // Trigger validation when the currently edited channel changes.
  useEffect(() => {
    const error = validateFeishuLike();
    if (onValidationChange) {
      onValidationChange(error);
    }
  }, [entry, selectedPreset, onValidationChange]);

  return (
    <section className="setup-section">
      <div className="section-heading">
        <div>
          <h2>渠道配置</h2>
          <div className="section-subtitle">
            选择消息渠道并配置接入参数。默认推荐飞书 Feishu。
          </div>
        </div>
        <div className="gateway-status-chip is-running">
          {enabledList.length > 0
            ? `已启用：${enabledList.join("、")}`
            : "无已启用渠道"}
        </div>
      </div>

      <div className="channel-selector-row">
        <label className="channel-selector-label">
          选择渠道
          <select
            value={selectedId}
            onChange={(event) => selectChannel(event.target.value)}
            className="channel-selector-select"
          >
            {CHANNEL_PRESETS.map((preset) => {
              const isEnabled = getEntry(preset.id).enabled;
              return (
                <option key={preset.id} value={preset.id}>
                  {preset.name}
                  {isEnabled ? " ✓" : ""}
                  {preset.isDefault ? " (推荐)" : ""}
                </option>
              );
            })}
          </select>
        </label>

        <label className="toggle channel-enable-toggle">
          <input
            type="checkbox"
            checked={entry.enabled}
            onChange={(event) =>
              handleFieldChange({ key: "enabled", label: "", placeholder: "" } as ChannelField, event.target.checked)
            }
          />
          启用 {selectedPreset?.name ?? ""}
        </label>
      </div>

      {selectedPreset && (
        <div className="channel-config-card">
          <div className="channel-config-header">
            <strong>{selectedPreset.name}</strong>
            <span className="provider-meta">
              {selectedPreset.id} ·{" "}
              {selectedPreset.category === "im"
                ? "即时通讯"
                : selectedPreset.category === "webhook"
                ? "Webhook"
                : "其他"}
            </span>
            <span
              className={`provider-health-badge ${
                entry.enabled ? "is-ok" : "is-idle"
              }`}
            >
              {entry.enabled ? "已启用" : "未启用"}
            </span>
          </div>

          {/* Webhook URL section */}
          <div className="channel-webhook-section">
            {/* Primary webhook URL (for normal message events) */}
            <div className="webhook-url-row">
              <span className="webhook-url-label">普通事件回调地址：</span>
              <code className="webhook-url-value">
                {fullWebhookUrl || "Gateway 未运行，启动后自动生成"}
              </code>
              <button
                type="button"
                className="setup-btn setup-btn--secondary"
                onClick={handleCopyWebhookUrl}
                disabled={!fullWebhookUrl}
              >
                {copyStatus || "复制"}
              </button>
              <button
                type="button"
                className={`setup-btn setup-btn--secondary ${healthStatus === "checking" ? "is-loading" : ""}`}
                onClick={handleHealthCheck}
                disabled={healthStatus === "checking"}
              >
                {healthStatus === "checking" ? "检测中..." : "测试连接"}
              </button>
            </div>
            {/* Card callback URL (for interactive card button clicks) — Feishu only */}
            {selectedPreset?.id === "feishu" && (
              <div className="webhook-url-row webhook-url-row--card">
                <span className="webhook-url-label">卡片交互回调地址：</span>
                <code className="webhook-url-value">
                  {fullCardCallbackUrl || "Gateway 未运行，启动后自动生成"}
                </code>
                <button
                  type="button"
                  className="setup-btn setup-btn--secondary"
                  onClick={handleCopyCardCallbackUrl}
                  disabled={!fullCardCallbackUrl}
                >
                  {copyCardStatus || "复制"}
                </button>
              </div>
            )}
            <div className="setup-action-hint">
              {selectedPreset?.id === "feishu" ? (
                <>
                  <strong>普通事件</strong>（普通消息、@机器人）：使用<strong>普通事件回调地址</strong>。
                  <strong>卡片按钮点击</strong>（功能菜单按钮）：使用<strong>卡片交互回调地址</strong>。
                  请分别在飞书开放平台的「事件与回调」页面配置这两个地址。
                  如需公网回调，请填写下方 Public Webhook Base URL（不要包含 /webhook/feishu 路径）。
                </>
              ) : (
                <>自动生成，用于复制到飞书开放平台。</>
              )}
            </div>
            {publicUrlNormalizationNotice ? (
              <div className="setup-action-hint">{publicUrlNormalizationNotice}</div>
            ) : null}
            {healthMessage && (
              <div className={`health-message ${healthStatus}`}>
                {healthMessage}
              </div>
            )}
            {isLocal && fullWebhookUrl && (
              <div className="localhost-warning">
                ⚠️ 127.0.0.1 / localhost 只能被本机访问，飞书、Slack 等公网平台无法直接回调。真实接入需要公网服务器、反向代理或内网穿透。
              </div>
            )}
          </div>

          {validationError && (
            <div className="setup-error" style={{ marginBottom: "12px" }}>
              {validationError}
            </div>
          )}

          <div className="setup-grid">
            {visibleFields.map((field) => {
              // Special handling for outbound_mode - render as dropdown
              if (field.key === "outbound_mode") {
                return (
                  <label key={field.key}>
                    {field.label}
                    <select
                      value={getFieldValue(field) || "disabled"}
                      onChange={(event) =>
                        handleFieldChange(field, event.target.value)
                      }
                    >
                      <option value="disabled">disabled（禁用）</option>
                      <option value="mock">mock（模拟）</option>
                      <option value="real">real（真实）</option>
                    </select>
                  </label>
                );
              }
              if (field.key === "security_mode") {
                return (
                  <label key={field.key}>
                    {field.label}
                    <select
                      value={getFieldValue(field) || "dev"}
                      onChange={(event) => handleFieldChange(field, event.target.value)}
                    >
                      <option value="dev">dev（允许未校验请求，仅限开发）</option>
                      <option value="token">token（校验 Verification Token）</option>
                      <option value="encrypted">encrypted（解密并校验 Token）</option>
                    </select>
                  </label>
                );
              }
              return (
                <label key={field.key}>
                  {field.label}
                  {field.key === "app_secret" || field.key === "verification_token" || field.key === "encrypt_key" ? (
                    <div className="app-secret-input-row">
                      <input
                        type={field.type ?? "text"}
                        value={getFieldValue(field)}
                        onChange={(event) =>
                          handleFieldChange(field, event.target.value)
                        }
                        placeholder={
                          field.placeholder || selectedPreset.tokenEnvHint
                        }
                      />
                      <button
                        type="button"
                        className="setup-btn setup-btn--secondary app-secret-clear-btn"
                        onClick={() => clearSensitiveField(field.key as "app_secret" | "verification_token" | "encrypt_key")}
                        title={`清除 ${field.label}`}
                      >
                        清除
                      </button>
                    </div>
                  ) : (
                    <input
                      type={field.type ?? "text"}
                      value={getFieldValue(field)}
                      onChange={(event) =>
                        handleFieldChange(field, event.target.value)
                      }
                      placeholder={
                        field.placeholder || selectedPreset.tokenEnvHint
                      }
                    />
                  )}
                </label>
              );
            })}
          </div>

          {selectedPreset.id === "feishu" && (
            <div className="channel-guide">
              <h3>飞书接入指引</h3>
              <ol>
                <li>
                  登录{" "}
                  <a
                    href="https://open.feishu.cn/app"
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    飞书开放平台
                  </a>
                  ，创建企业自建应用
                </li>
                <li>
                  在「凭证与基础信息」中获取 <strong>App ID</strong> 和{" "}
                  <strong>App Secret</strong>（本页仅需填写这两项）
                </li>
                <li>
                  在「事件订阅」中设置请求地址为网关地址 +
                  <code>/webhook/feishu</code>（如{" "}
                  <code>https://your-domain/webhook/feishu</code>）
                </li>
                <li>
                  订阅 <code>im.message.receive_v1</code>{" "}
                  事件以接收用户消息
                </li>
                <li>
                  在「权限管理」中开通 <code>im:message</code>、
                  <code>im:message:send_as_bot</code> 权限
                </li>
                <li>发布应用版本并审核通过</li>
              </ol>
            </div>
          )}

          {selectedPreset.id === "dingtalk" && (
            <div className="channel-guide">
              <h3>钉钉接入指引</h3>
              <ol>
                <li>
                  登录{" "}
                  <a
                    href="https://open-dev.dingtalk.com"
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    钉钉开放平台
                  </a>
                  ，创建企业内部应用
                </li>
                <li>获取 App Key 和 App Secret</li>
                <li>
                  配置消息接收地址为 <code>/webhook/dingtalk</code>
                </li>
              </ol>
            </div>
          )}

          {selectedPreset.id === "wechat" && (
            <div className="channel-guide">
              <h3>企业微信接入指引</h3>
              <ol>
                <li>
                  登录{" "}
                  <a
                    href="https://work.weixin.qq.com"
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    企业微信管理后台
                  </a>
                </li>
                <li>创建自建应用，获取 Corp ID 和 App Secret</li>
                <li>
                  设置接收消息的 URL 为 <code>/webhook/wechat</code>
                  ，获取 Token 和 EncodingAESKey
                </li>
              </ol>
            </div>
          )}
        </div>
      )}
    </section>
  );
}
