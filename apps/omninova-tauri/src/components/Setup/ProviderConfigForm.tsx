import { useState } from "react";
import {
  cloneProviderPreset,
  PROVIDER_PRESETS,
  type ProviderConfig,
} from "../../types/config";

type Props = {
  value: ProviderConfig[];
  onChange: (next: ProviderConfig[]) => void;
  defaultProvider?: string;
  defaultModel?: string;
  onDefaultChange?: (providerId: string, model: string) => void;
};

const parseStringList = (value: string) =>
  value
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);

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
}: Props) {
  const [advancedIds, setAdvancedIds] = useState<Set<string>>(new Set());
  const [customOpen, setCustomOpen] = useState(false);
  const [customDraft, setCustomDraft] = useState({
    name: "",
    baseUrl: "",
    apiKey: "",
    models: "",
  });

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
