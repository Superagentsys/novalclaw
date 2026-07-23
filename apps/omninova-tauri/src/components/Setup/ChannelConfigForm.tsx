import { useMemo, useState } from "react";
import {
  CHANNEL_PRESETS,
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
}: ChannelConfigFormProps) {
  const [selectedId, setSelectedId] = useState<string>(DEFAULT_CHANNEL_ID);
  const [healthStatus, setHealthStatus] = useState<"idle" | "checking" | "ok" | "error">("idle");
  const [healthMessage, setHealthMessage] = useState<string>("");
  const [copyStatus, setCopyStatus] = useState<string>("");
  /** Tracks whether the user has clicked "Clear App Secret" for the current channel */
  const [clearAppSecretRequested, setClearAppSecretRequested] = useState<Record<string, boolean>>({});

  const selectedPreset: ChannelPreset | undefined = useMemo(
    () => CHANNEL_PRESETS.find((preset) => preset.id === selectedId),
    [selectedId]
  );

  const webhookPath = useMemo(() => getWebhookPath(selectedId), [selectedId]);
  const fullWebhookUrl = gatewayUrl ? `${gatewayUrl.replace(/\/$/, "")}${webhookPath}` : "";
  const isLocal = gatewayUrl ? isLocalhost(gatewayUrl) : true;

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

  /** Reset health status when gateway URL changes */
  useMemo(() => {
    setHealthStatus("idle");
    setHealthMessage("");
  }, [gatewayUrl]);

  const visibleFields = useMemo(() => {
    if (!selectedPreset) return [];
    // Feishu/Lark only show app_id, app_secret, and outbound_mode (all are extra fields)
    if (FEISHU_LIKE_CHANNEL_IDS.has(selectedPreset.id)) {
      return selectedPreset.fields.filter(
        (field) => field.key === "app_id" || field.key === "app_secret" || field.key === "outbound_mode"
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
    // Preserve clear_app_secret flag if set
    if (clearAppSecretRequested[id]) {
      cleanedEntry.clear_app_secret = true;
    }
    onChange({ ...value, [id]: cleanedEntry });
  };

  const enabledList = CHANNEL_PRESETS.filter(
    (preset) => getEntry(preset.id).enabled
  ).map((preset) => preset.name);

  const entry = selectedPreset ? getEntry(selectedPreset.id) : { ...EMPTY_ENTRY, extra: {} };

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
      updatedEntry.extra = { ...(updatedEntry.extra ?? {}), [field.key]: fieldValue as string };
    } else {
      (updatedEntry as unknown as Record<string, unknown>)[field.key] = fieldValue;
    }
    setEntry(selectedPreset.id, updatedEntry);
  };

  /** Validate Feishu/Lark has required fields when enabled */
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
    
    if (!appId.trim()) {
      return "启用飞书时，App ID 不能为空";
    }
    
    // Only require app_secret when outbound_mode is "real" or "mock"
    if ((outboundMode === "real" || outboundMode === "mock") && !appSecret.trim()) {
      return "启用飞书时，App Secret 不能为空";
    }
    
    return undefined;
  };

  // Trigger validation when entry changes
  useMemo(() => {
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
            onChange={(event) => setSelectedId(event.target.value)}
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
            <div className="webhook-url-row">
              <span className="webhook-url-label">Webhook 地址：</span>
              <code className="webhook-url-value">{fullWebhookUrl || "未配置 Gateway 地址"}</code>
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
              return (
                <label key={field.key}>
                  {field.label}
                  {field.key === "app_secret" ? (
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
                        onClick={() => {
                          setClearAppSecretRequested((prev) => ({
                            ...prev,
                            [selectedPreset!.id]: true,
                          }));
                          // Clear the input value to empty, which will be submitted as part of the form
                          handleFieldChange(field, "");
                        }}
                        title="清除 App Secret"
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
