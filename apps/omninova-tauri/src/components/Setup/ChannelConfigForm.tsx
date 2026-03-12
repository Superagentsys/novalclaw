import { useMemo, useState } from "react";
import {
  CHANNEL_PRESETS,
  type ChannelEntryConfig,
  type ChannelsConfig,
  type ChannelPreset,
} from "../../types/config";

interface ChannelConfigFormProps {
  value: ChannelsConfig;
  onChange: (channels: ChannelsConfig) => void;
}

const EMPTY_ENTRY: ChannelEntryConfig = {
  enabled: false,
  token: "",
  token_env: "",
  app_id: "",
  app_secret: "",
  verification_token: "",
  encrypt_key: "",
  webhook_url: "",
};

const DEFAULT_CHANNEL_ID = "feishu";

export function ChannelConfigForm({ value, onChange }: ChannelConfigFormProps) {
  const [selectedId, setSelectedId] = useState<string>(DEFAULT_CHANNEL_ID);

  const selectedPreset: ChannelPreset | undefined = useMemo(
    () => CHANNEL_PRESETS.find((preset) => preset.id === selectedId),
    [selectedId]
  );

  const getEntry = (id: keyof ChannelsConfig): ChannelEntryConfig =>
    value[id] ?? { ...EMPTY_ENTRY };

  const setEntry = (id: keyof ChannelsConfig, entry: ChannelEntryConfig) => {
    onChange({ ...value, [id]: entry });
  };

  const enabledList = useMemo(
    () =>
      CHANNEL_PRESETS.filter((preset) => getEntry(preset.id).enabled).map(
        (preset) => preset.name
      ),
    [value]
  );

  const entry = selectedPreset
    ? getEntry(selectedPreset.id)
    : { ...EMPTY_ENTRY };

  const handleFieldChange = (
    key: keyof ChannelEntryConfig,
    fieldValue: string | boolean
  ) => {
    if (!selectedPreset) return;
    setEntry(selectedPreset.id, { ...entry, [key]: fieldValue });
  };

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
              handleFieldChange("enabled", event.target.checked)
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
              {selectedPreset.id} · {selectedPreset.category === "im" ? "即时通讯" : selectedPreset.category === "webhook" ? "Webhook" : "其他"}
            </span>
            <span
              className={`provider-health-badge ${
                entry.enabled ? "is-ok" : "is-idle"
              }`}
            >
              {entry.enabled ? "已启用" : "未启用"}
            </span>
          </div>

          <div className="setup-grid">
            {selectedPreset.fields.map((field) => (
              <label key={field.key}>
                {field.label}
                <input
                  type={field.type ?? "text"}
                  value={(entry[field.key] as string) ?? ""}
                  onChange={(event) =>
                    handleFieldChange(field.key, event.target.value)
                  }
                  placeholder={
                    field.placeholder || selectedPreset.tokenEnvHint
                  }
                />
              </label>
            ))}
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
                  <strong>App Secret</strong>
                </li>
                <li>
                  在「事件订阅」中设置请求地址为网关地址 +
                  <code>/webhook/feishu</code>（如{" "}
                  <code>https://your-domain/webhook/feishu</code>
                  ），并获取 <strong>Verification Token</strong> 和{" "}
                  <strong>Encrypt Key</strong>
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
