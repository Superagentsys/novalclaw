import { useState } from "react";
import {
  cloneProviderPreset,
  PROVIDER_PRESETS,
  type ProviderConfig,
  type ProviderTransportMode,
} from "../../types/config";
import {
  applyRequestLimitInput,
  formatRequestLimitInput,
  formatTokenCount,
  requestLimitError,
} from "./requestLimit";

type Props = {
  value: ProviderConfig[];
  onChange: (next: ProviderConfig[]) => void;
  defaultProvider?: string;
  defaultModel?: string;
  onDefaultChange?: (providerId: string, model: string) => void;
  onValidationChange?: (error: string | null) => void;
};

const parseStringList = (value: string) =>
  value
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);

const PROVIDER_TRANSPORT_OPTIONS: { value: ProviderTransportMode; label: string }[] = [
  { value: "auto", label: "自动协商（推荐）" },
  { value: "http1", label: "HTTP/1.1" },
  { value: "http2", label: "HTTP/2" },
];

function isLocalProvider(provider: ProviderConfig): boolean {
  const preset = PROVIDER_PRESETS.find((item) => item.id === provider.id);
  if (preset) return preset.category === "local";
  return ["ollama", "lmstudio", "vllm", "sglang", "llamacpp", "local"].includes(
    provider.type
  );
}

function slugifyProviderId(name: string, existing: string[]): string {
  const base =
    name
      .trim()
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "") || "custom";
  const id = base.startsWith("custom-") ? base : `custom-${base}`;
  if (!existing.includes(id)) return id;
  let index = 2;
  while (existing.includes(`${id}-${index}`)) index += 1;
  return `${id}-${index}`;
}

export function ProviderConfigForm({
  value,
  onChange,
  defaultProvider = "",
  defaultModel = "",
  onDefaultChange,
  onValidationChange,
}: Props) {
  const [advancedIds, setAdvancedIds] = useState<Set<string>>(new Set());
  const [customOpen, setCustomOpen] = useState(false);
  const [rawRequestLimits, setRawRequestLimits] = useState<Record<string, string>>({});
  const [customDraft, setCustomDraft] = useState({
    name: "",
    baseUrl: "",
    apiKey: "",
    models: "",
  });

  const requestLimitInputValue = (provider: ProviderConfig): string => {
    const raw = rawRequestLimits[provider.id];
    if (raw !== undefined) return raw;
    return formatRequestLimitInput(provider.request_max_output_tokens);
  };

  const handleRequestLimitChange = (provider: ProviderConfig, raw: string) => {
    setRawRequestLimits((prev) => ({ ...prev, [provider.id]: raw }));
    const next = applyRequestLimitInput(
      provider.request_max_output_tokens,
      raw,
      provider.max_output_tokens
    );
    const index = value.findIndex((item) => item.id === provider.id);
    if (index >= 0) {
      updateProvider(index, "request_max_output_tokens", next);
    }
  };

  const updateProvider = (
    index: number,
    key: keyof ProviderConfig,
    nextValue: ProviderConfig[keyof ProviderConfig]
  ) => {
    const next = value.map((provider, currentIndex) =>
      currentIndex === index ? { ...provider, [key]: nextValue } : provider
    );
    onChange(next);
  };

  const addProvider = (providerId: string) => {
    const nextProvider = cloneProviderPreset(providerId);
    if (!nextProvider || value.some((provider) => provider.id === providerId)) {
      return;
    }
    onChange([...value, { ...nextProvider, enabled: true }]);
  };

  const addCustomProvider = () => {
    const name = customDraft.name.trim() || "自定义模型";
    const models = parseStringList(customDraft.models);
    if (!customDraft.baseUrl.trim() || models.length === 0) {
      return;
    }
    const id = slugifyProviderId(name, value.map((provider) => provider.id));
    onChange([
      ...value,
      {
        id,
        name,
        type: "openai",
        api_key_env: customDraft.apiKey.trim() || undefined,
        base_url: customDraft.baseUrl.trim(),
        models,
        enabled: true,
      },
    ]);
    setAdvancedIds((prev) => new Set(prev).add(id));
    setCustomDraft({ name: "", baseUrl: "", apiKey: "", models: "" });
    setCustomOpen(false);
  };

  const removeProvider = (providerId: string) => {
    onChange(value.filter((provider) => provider.id !== providerId));
  };

  const toggleAdvanced = (providerId: string) => {
    setAdvancedIds((prev) => {
      const next = new Set(prev);
      if (next.has(providerId)) next.delete(providerId);
      else next.add(providerId);
      return next;
    });
  };

  const availableProviders = PROVIDER_PRESETS.filter(
    (preset) => !value.some((provider) => provider.id === preset.id)
  );
  const enabledProviders = value.filter((provider) => provider.enabled);
  const activeProvider =
    enabledProviders.find((provider) => provider.id === defaultProvider) ??
    enabledProviders[0];
  const selectedModel =
    activeProvider && activeProvider.models.includes(defaultModel)
      ? defaultModel
      : activeProvider?.models[0] ?? "";

  const validationError = value.reduce<string | null>((found, provider) => {
    if (found) return found;
    return requestLimitError(requestLimitInputValue(provider), provider.max_output_tokens);
  }, null);
  if (onValidationChange) {
    // This is intentionally called during render; it is a cheap callback that
    // lets the Setup save boundary reject invalid raw input before persistence.
    // The parent stores a string and does not set React state synchronously from
    // a child render in a way that loops because the value is derived from the
    // same raw/valid state.
    onValidationChange(validationError);
  }

  return (
    <div className="setup-stack">
      <section className="setup-section">
        <div className="section-heading">
          <div>
            <h2>默认模型</h2>
            <div className="section-subtitle">
              对话未指定服务时使用这里的选择。先启用下方服务，再选默认项。
            </div>
          </div>
        </div>
        <div className="provider-defaults">
          <label>
            服务
            <select
              value={activeProvider?.id ?? ""}
              disabled={enabledProviders.length === 0}
              onChange={(event) => {
                const provider = enabledProviders.find((item) => item.id === event.target.value);
                onDefaultChange?.(event.target.value, provider?.models[0] ?? "");
              }}
            >
              <option value="">
                {enabledProviders.length === 0 ? "请先启用一个服务" : "选择服务"}
              </option>
              {enabledProviders.map((provider) => (
                <option key={provider.id} value={provider.id}>
                  {provider.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            模型
            <select
              value={selectedModel}
              disabled={!activeProvider || activeProvider.models.length === 0}
              onChange={(event) =>
                onDefaultChange?.(activeProvider?.id ?? "", event.target.value)
              }
            >
              <option value="">
                {!activeProvider ? "请先选择服务" : "选择模型"}
              </option>
              {(activeProvider?.models ?? []).map((model) => (
                <option key={model} value={model}>
                  {model}
                </option>
              ))}
            </select>
          </label>
        </div>
      </section>

      <section className="setup-section">
        <div className="section-heading">
          <div>
            <h2>已接入服务</h2>
            <div className="section-subtitle">
              添加云端、本地或自定义 OpenAI 兼容服务。填写密钥即可使用。
            </div>
          </div>
          <div className="provider-picker-row">
            <label className="provider-picker">
              <span>添加服务</span>
              <select
                value=""
                onChange={(event) => addProvider(event.target.value)}
                disabled={availableProviders.length === 0}
              >
                <option value="" disabled>
                  {availableProviders.length === 0 ? "已添加全部预设" : "选择模型服务"}
                </option>
                <optgroup label="云端">
                  {availableProviders
                    .filter((preset) => preset.category === "cloud")
                    .map((preset) => (
                      <option key={preset.id} value={preset.id}>
                        {preset.name}
                      </option>
                    ))}
                </optgroup>
                <optgroup label="本地">
                  {availableProviders
                    .filter((preset) => preset.category === "local")
                    .map((preset) => (
                      <option key={preset.id} value={preset.id}>
                        {preset.name}
                      </option>
                    ))}
                </optgroup>
              </select>
            </label>
            <button
              type="button"
              className="setup-btn setup-btn--secondary"
              onClick={() => setCustomOpen((open) => !open)}
            >
              {customOpen ? "收起自定义" : "添加自定义服务"}
            </button>
          </div>
        </div>

        {customOpen ? (
          <div className="provider-custom-form">
            <p className="section-subtitle">
              适用于任何 OpenAI 兼容网关：填写 Base URL、密钥和模型 ID。
            </p>
            <div className="setup-grid">
              <label>
                显示名称
                <input
                  value={customDraft.name}
                  onChange={(event) =>
                    setCustomDraft((prev) => ({ ...prev, name: event.target.value }))
                  }
                  placeholder="例如公司网关 / 聚合 API"
                />
              </label>
              <label>
                Base URL
                <input
                  value={customDraft.baseUrl}
                  onChange={(event) =>
                    setCustomDraft((prev) => ({ ...prev, baseUrl: event.target.value }))
                  }
                  placeholder="https://api.example.com/v1"
                />
              </label>
              <label>
                API Key / 环境变量
                <input
                  value={customDraft.apiKey}
                  onChange={(event) =>
                    setCustomDraft((prev) => ({ ...prev, apiKey: event.target.value }))
                  }
                  placeholder="OPENAI_API_KEY 或直接填 sk-..."
                />
              </label>
              <label>
                模型 ID
                <input
                  value={customDraft.models}
                  onChange={(event) =>
                    setCustomDraft((prev) => ({ ...prev, models: event.target.value }))
                  }
                  placeholder="glm-5.1, minimax-m3"
                />
              </label>
            </div>
            <div className="provider-custom-actions">
              <button
                type="button"
                className="setup-btn setup-btn--primary"
                onClick={addCustomProvider}
                disabled={!customDraft.baseUrl.trim() || !customDraft.models.trim()}
              >
                加入列表
              </button>
            </div>
          </div>
        ) : null}

        <div className="setup-stack">
          {value.map((provider, index) => {
            const local = isLocalProvider(provider);
            const custom = provider.id.startsWith("custom-") || provider.type === "custom";
            const advanced = advancedIds.has(provider.id) || custom;
            return (
              <div
                key={provider.id}
                className={`provider-card provider-card--compact${
                  provider.enabled ? "" : " is-disabled"
                }`}
              >
                <div className="provider-header">
                  <div className="provider-title">
                    <strong>{provider.name}</strong>
                    <span className="provider-pill">
                      {local ? "本地" : custom ? "自定义" : "云端"}
                    </span>
                    <span className="provider-meta">{provider.id}</span>
                  </div>
                  <div className="provider-actions">
                    <label className="toggle">
                      <input
                        type="checkbox"
                        checked={provider.enabled}
                        onChange={(event) =>
                          updateProvider(index, "enabled", event.target.checked)
                        }
                      />
                      <span>启用</span>
                    </label>
                    <button
                      type="button"
                      className="ghost-button"
                      onClick={() => removeProvider(provider.id)}
                    >
                      移除
                    </button>
                  </div>
                </div>

                <div className={`setup-grid${local ? " provider-grid--local" : ""}`}>
                  {local ? null : (
                    <label>
                      API Key / 环境变量
                      <input
                        value={provider.api_key_env ?? ""}
                        onChange={(event) =>
                          updateProvider(index, "api_key_env", event.target.value)
                        }
                        placeholder="DEEPSEEK_API_KEY 或直接填 sk-..."
                      />
                    </label>
                  )}
                  <label className={local ? undefined : "provider-models-field"}>
                    模型
                    <input
                      value={provider.models.join(", ")}
                      onChange={(event) =>
                        updateProvider(index, "models", parseStringList(event.target.value))
                      }
                      placeholder="model-a, model-b"
                    />
                  </label>
                </div>

                <div className="provider-advanced-row">
                  <button
                    type="button"
                    className="provider-advanced-toggle"
                    onClick={() => toggleAdvanced(provider.id)}
                  >
                    {advanced ? "收起高级选项" : "高级选项"}
                  </button>
                </div>

                {advanced ? (
                  <div className="setup-grid">
                    <label>
                      显示名称
                      <input
                        value={provider.name}
                        onChange={(event) =>
                          updateProvider(index, "name", event.target.value)
                        }
                      />
                    </label>
                    <label>
                      基础地址
                      <input
                        value={provider.base_url ?? ""}
                        onChange={(event) =>
                          updateProvider(index, "base_url", event.target.value)
                        }
                        placeholder={local ? "http://localhost:11434" : "https://api.example.com/v1"}
                      />
                    </label>
                    <label className="provider-transport-field">
                      HTTP 协议
                      <select
                        value={provider.transport?.mode ?? "auto"}
                        onChange={(event) =>
                          updateProvider(index, "transport", {
                            mode: event.target.value as ProviderTransportMode,
                          })
                        }
                      >
                        {PROVIDER_TRANSPORT_OPTIONS.map((option) => (
                          <option key={option.value} value={option.value}>
                            {option.label}
                          </option>
                        ))}
                      </select>
                      <small>自动协商适合大多数服务；仅在第三方 API 出现连接兼容问题时尝试 HTTP/1.1 或 HTTP/2。</small>
                    </label>
                    <label className="provider-request-limit-field">
                      单次请求最大输出
                      <input
                        type="text"
                        inputMode="numeric"
                        value={requestLimitInputValue(provider)}
                        placeholder="使用 OmniNova 默认策略"
                        onChange={(event) => handleRequestLimitChange(provider, event.target.value)}
                      />
                      {requestLimitError(
                        requestLimitInputValue(provider),
                        provider.max_output_tokens
                      ) ? (
                        <small className="provider-field-error">
                          {requestLimitError(
                            requestLimitInputValue(provider),
                            provider.max_output_tokens
                          )}
                        </small>
                      ) : null}
                      {provider.max_output_tokens ? (
                        <small>模型最大输出上限：{formatTokenCount(provider.max_output_tokens)}（只读能力）</small>
                      ) : (
                        <small>模型最大输出上限未知</small>
                      )}
                      <small>
                        限制一次模型请求最多生成的 Token 数量。该值不会改变模型本身的最大输出能力。
                        留空则使用 OmniNova 默认策略（32K，且不超过模型最大输出上限）。
                      </small>
                    </label>
                  </div>
                ) : null}
              </div>
            );
          })}
          {value.length === 0 ? (
            <div className="empty-state">
              暂未添加模型服务，请先从右上角选择平台，或添加自定义 OpenAI 兼容服务。
            </div>
          ) : null}
        </div>
      </section>
    </div>
  );
}
