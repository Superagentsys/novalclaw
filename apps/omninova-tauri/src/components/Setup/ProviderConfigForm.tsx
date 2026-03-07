import {
  cloneProviderPreset,
  PROVIDER_PRESETS,
  type ProviderConfig,
} from "../../types/config";

type Props = {
  value: ProviderConfig[];
  onChange: (next: ProviderConfig[]) => void;
};

const parseStringList = (value: string) =>
  value
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);

export function ProviderConfigForm({ value, onChange }: Props) {
  const presetOptions = {
    cloud: PROVIDER_PRESETS.filter((preset) => preset.category === "cloud"),
    local: PROVIDER_PRESETS.filter((preset) => preset.category === "local"),
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

    onChange([...value, nextProvider]);
  };

  const removeProvider = (providerId: string) => {
    onChange(value.filter((provider) => provider.id !== providerId));
  };

  const replaceProvider = (index: number, nextProviderId: string) => {
    const nextProvider = cloneProviderPreset(nextProviderId);
    const currentProvider = value[index];

    if (!nextProvider || !currentProvider) {
      return;
    }

    if (
      value.some(
        (provider, currentIndex) =>
          currentIndex !== index && provider.id === nextProviderId
      )
    ) {
      return;
    }

    const next = value.map((provider, currentIndex) =>
      currentIndex === index
        ? {
            ...nextProvider,
            enabled: provider.enabled,
          }
        : provider
    );

    onChange(next);
  };

  const availableProviders = PROVIDER_PRESETS.filter(
    (preset) => !value.some((provider) => provider.id === preset.id)
  );

  return (
    <div className="setup-section">
      <div className="section-heading">
        <div>
          <h2>模型服务</h2>
          <div className="section-subtitle">
            从预设下拉框中选择要接入的云端或本地模型服务，可同时配置多个。
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
            {availableProviders.map((preset) => (
              <option key={preset.id} value={preset.id}>
                {preset.name} · {preset.category === "local" ? "本地模型" : "云端模型"}
              </option>
            ))}
          </select>
        </label>
      </div>
      <div className="setup-stack">
        {value.map((provider, index) => (
          <div key={provider.id} className="provider-card">
            <div className="provider-header">
              <label className="provider-selector">
                <span>模型服务</span>
                <select
                  value={provider.id}
                  onChange={(event) => replaceProvider(index, event.target.value)}
                >
                  <optgroup label="云端模型">
                    {presetOptions.cloud.map((preset) => (
                      <option
                        key={preset.id}
                        value={preset.id}
                        disabled={
                          provider.id !== preset.id &&
                          value.some((item) => item.id === preset.id)
                        }
                      >
                        {preset.name}
                      </option>
                    ))}
                  </optgroup>
                  <optgroup label="本地模型">
                    {presetOptions.local.map((preset) => (
                      <option
                        key={preset.id}
                        value={preset.id}
                        disabled={
                          provider.id !== preset.id &&
                          value.some((item) => item.id === preset.id)
                        }
                      >
                        {preset.name}
                      </option>
                    ))}
                  </optgroup>
                </select>
                <div className="provider-meta">{provider.type}</div>
              </label>
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
                API Key 环境变量
                <input
                  value={provider.api_key_env ?? ""}
                  onChange={(event) =>
                    updateProvider(index, "api_key_env", event.target.value)
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
                />
              </label>
              <label>
                模型列表
                <input
                  value={provider.models.join(",")}
                  onChange={(event) =>
                    updateProvider(
                      index,
                      "models",
                      parseStringList(event.target.value)
                    )
                  }
                />
              </label>
            </div>
          </div>
        ))}
        {value.length === 0 ? (
          <div className="empty-state">
            暂未添加模型服务，请先从右上角下拉框选择需要接入的平台。
          </div>
        ) : null}
      </div>
    </div>
  );
}
