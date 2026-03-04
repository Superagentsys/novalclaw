import { type ProviderConfig } from "../../types/config";

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

  return (
    <div className="setup-section">
      <h2>模型服务</h2>
      <div className="setup-stack">
        {value.map((provider, index) => (
          <div key={provider.id} className="provider-card">
            <div className="provider-header">
              <div>
                <h3>{provider.name}</h3>
                <div className="provider-meta">{provider.type}</div>
              </div>
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
      </div>
    </div>
  );
}
