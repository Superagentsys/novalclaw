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
  return ["ollama", "lmstudio", "vllm", "local"].includes(provider.type);
}

export function ProviderConfigForm({
  value,
  onChange,
  defaultProvider = "",
  defaultModel = "",
  onDefaultChange,
}: Props) {
  const [advancedIds, setAdvancedIds] = useState<Set<string>>(new Set());

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
              添加云端或本地服务，填写密钥即可使用。高级项（显示名、地址）按需展开。
            </div>
          </div>
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
        </div>

        <div className="setup-stack">
          {value.map((provider, index) => {
            const local = isLocalProvider(provider);
            const advanced = advancedIds.has(provider.id);
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
                      {local ? "本地" : "云端"}
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
                        placeholder={local ? "http://localhost:11434" : "https://api.example.com"}
                      />
                    </label>
                  </div>
                ) : null}
              </div>
            );
          })}
          {value.length === 0 ? (
            <div className="empty-state">
              暂未添加模型服务，请先从右上角选择需要接入的平台。
            </div>
          ) : null}
        </div>
      </section>
    </div>
  );
}
