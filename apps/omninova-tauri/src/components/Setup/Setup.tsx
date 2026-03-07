import { useMemo, useState } from "react";
import {
  DEFAULT_PROVIDERS,
  DEFAULT_ROBOT_CONFIG,
  type AppConfig,
} from "../../types/config";
import { ProviderConfigForm } from "./ProviderConfigForm";
import { RobotConfigForm } from "./RobotConfigForm";
import omninovalLogo from "../../assets/omninoval-logo.png";

const initialConfig: AppConfig = {
  api_key: "",
  api_url: "",
  default_provider: "",
  default_model: "",
  workspace_dir: "",
  omninoval_gateway_url: "http://localhost:18789",
  omninoval_config_dir: "~/.omninoval",
  robot: DEFAULT_ROBOT_CONFIG,
  providers: DEFAULT_PROVIDERS,
};

export function Setup() {
  const [config, setConfig] = useState<AppConfig>(initialConfig);
  const enabledProviders = useMemo(
    () => config.providers.filter((provider) => provider.enabled),
    [config.providers]
  );
  const defaultModelOptions = useMemo(() => {
    if (config.default_provider) {
      const activeProvider = enabledProviders.find(
        (provider) => provider.id === config.default_provider
      );

      return activeProvider
        ? [
            {
              providerId: activeProvider.id,
              providerName: activeProvider.name,
              models: activeProvider.models,
            },
          ]
        : [];
    }

    return enabledProviders.map((provider) => ({
      providerId: provider.id,
      providerName: provider.name,
      models: provider.models,
    }));
  }, [config.default_provider, enabledProviders]);

  const jsonPreview = useMemo(
    () => JSON.stringify(config, null, 2),
    [config]
  );

  const handleProvidersChange = (providers: AppConfig["providers"]) => {
    const enabledProviderIds = providers
      .filter((provider) => provider.enabled)
      .map((provider) => provider.id);
    const currentDefaultProvider = enabledProviderIds.includes(
      config.default_provider ?? ""
    )
      ? config.default_provider
      : "";
    const currentProvider = providers.find(
      (provider) => provider.id === currentDefaultProvider
    );
    const currentDefaultModel = currentProvider?.models.includes(
      config.default_model ?? ""
    )
      ? config.default_model
      : "";

    setConfig({
      ...config,
      providers,
      default_provider: currentDefaultProvider,
      default_model: currentDefaultModel,
    });
  };

  const handleDefaultModelChange = (value: string) => {
    if (!value) {
      setConfig({ ...config, default_model: "" });
      return;
    }

    const [providerId, model] = value.split("::");

    setConfig({
      ...config,
      default_provider: providerId,
      default_model: model ?? "",
    });
  };

  const selectedDefaultModelValue =
    config.default_provider && config.default_model
      ? `${config.default_provider}::${config.default_model}`
      : "";

  return (
    <div className="setup-page">
      <header className="setup-header">
        <div className="setup-brand">
          <div className="setup-logo-frame">
            <img
              src={omninovalLogo}
              alt="OmniNova logo"
              className="setup-logo"
            />
          </div>
          <div className="setup-brand-copy">
            <div className="setup-chip">OmniNova Claw</div>
            <div className="setup-title">OmniNova 启动配置</div>
            <div className="setup-subtitle">
              在首次启动前完成工作目录、模型服务与机器人参数初始化
            </div>
          </div>
        </div>
      </header>

      <section className="setup-section">
        <h2>基础信息</h2>
        <div className="setup-grid">
          <label>
            Workspace 目录
            <input
              value={config.workspace_dir}
              onChange={(event) =>
                setConfig({ ...config, workspace_dir: event.target.value })
              }
              placeholder="/path/to/workspace"
            />
          </label>
          <label>
            默认模型服务
            <select
              value={config.default_provider ?? ""}
              onChange={(event) =>
                setConfig({
                  ...config,
                  default_provider: event.target.value,
                  default_model: "",
                })
              }
            >
              <option value="">
                {enabledProviders.length === 0
                  ? "请先启用模型服务"
                  : "选择默认模型服务"}
              </option>
              {enabledProviders.map((provider) => (
                <option key={provider.id} value={provider.id}>
                  {provider.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            默认模型
            <select
              value={selectedDefaultModelValue}
              onChange={(event) => handleDefaultModelChange(event.target.value)}
              disabled={defaultModelOptions.length === 0}
            >
              <option value="">
                {defaultModelOptions.length === 0
                  ? "请先启用模型服务"
                  : "选择默认模型"}
              </option>
              {defaultModelOptions.map((provider) => (
                <optgroup key={provider.providerId} label={provider.providerName}>
                  {provider.models.map((model) => (
                    <option
                      key={`${provider.providerId}-${model}`}
                      value={`${provider.providerId}::${model}`}
                    >
                      {model}
                    </option>
                  ))}
                </optgroup>
              ))}
            </select>
          </label>
          <label>
            API 地址
            <input
              value={config.api_url ?? ""}
              onChange={(event) =>
                setConfig({ ...config, api_url: event.target.value })
              }
              placeholder="https://api.openai.com/v1"
            />
          </label>
          <label>
            API Key
            <input
              value={config.api_key ?? ""}
              onChange={(event) =>
                setConfig({ ...config, api_key: event.target.value })
              }
              placeholder="sk-..."
            />
          </label>
        </div>
      </section>

      <section className="setup-section">
        <h2>omninoval 连接</h2>
        <div className="setup-grid">
          <label>
            Gateway 地址
            <input
              value={config.omninoval_gateway_url ?? ""}
              onChange={(event) =>
                setConfig({
                  ...config,
                  omninoval_gateway_url: event.target.value,
                })
              }
              placeholder="http://localhost:18789"
            />
          </label>
          <label>
            配置目录
            <input
              value={config.omninoval_config_dir ?? ""}
              onChange={(event) =>
                setConfig({
                  ...config,
                  omninoval_config_dir: event.target.value,
                })
              }
              placeholder="~/.omninoval"
            />
          </label>
        </div>
      </section>

      <RobotConfigForm
        value={config.robot ?? DEFAULT_ROBOT_CONFIG}
        onChange={(robot) => setConfig({ ...config, robot })}
      />

      <ProviderConfigForm
        value={config.providers}
        onChange={handleProvidersChange}
      />

      <section className="setup-section">
        <h2>配置预览</h2>
        <textarea readOnly value={jsonPreview} className="setup-preview" />
      </section>
    </div>
  );
}
