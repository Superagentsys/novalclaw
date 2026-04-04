import { useEffect, useMemo, useState } from "react";
import {
  DEFAULT_PROVIDERS,
  DEFAULT_ROBOT_CONFIG,
  type Config,
  type GatewayStatus,
} from "../../types/config";
import { ChannelConfigForm } from "./ChannelConfigForm";
import { ProviderConfigForm } from "./ProviderConfigForm";
import { RobotConfigForm } from "./RobotConfigForm";
import { SkillsConfigForm } from "./SkillsConfigForm";
import { PersonaConfigForm } from "./PersonaConfigForm";
import { invokeTauri } from "../../utils/tauri";
import omninovalLogo from "../../assets/omninoval-logo.png";

export interface SetupProps {
  /** 配置完成且网关启动成功后调用，用于进入对话界面 */
  onConfigSuccess?: () => void;
  /** 由 AppShell 导航时使用：仅渲染内容区，不显示内置侧栏 */
  embedded?: boolean;
  /** 受控当前标签（与 App 导航同步） */
  activeTab?: SetupTab;
  onTabChange?: (tab: SetupTab) => void;
}

const initialConfig: Config = {
  api_key: "",
  api_url: "https://ark.cn-beijing.volces.com/api/v3",
  default_provider: "doubao",
  default_model: "doubao-seed-2-0-pro-260215",
  robot: DEFAULT_ROBOT_CONFIG,
  providers: DEFAULT_PROVIDERS,
  channels: {
    slack: { enabled: false },
    discord: { enabled: false },
    telegram: { enabled: false },
  },
  skills: {
    open_skills_enabled: true,
    prompt_injection_mode: "full",
  },
  agent: {
    name: "omninova",
    max_tool_iterations: 20,
    compact_context: true,
  },
};

export type SetupTab = "general" | "providers" | "channels" | "skills" | "persona";
type SetupTabItem = {
  id: SetupTab;
  label: string;
  icon: string;
};

const setupTabs: SetupTabItem[] = [
  { id: "general", label: "通用设置", icon: "⚙️" },
  { id: "providers", label: "模型服务", icon: "🤖" },
  { id: "channels", label: "渠道接入", icon: "🔌" },
  { id: "skills", label: "技能扩展", icon: "🛠️" },
  { id: "persona", label: "Agent 人设", icon: "🧠" },
];

const SETUP_PAGE_META: Record<
  SetupTab,
  { title: string; subtitle: string }
> = {
  general: {
    title: "设置",
    subtitle: "工作区、网关与连接信息。保存后可在侧栏启动或停止网关。",
  },
  providers: {
    title: "模型",
    subtitle: "启用模型服务、填写 API 与默认模型，供对话与路由使用。",
  },
  channels: {
    title: "频道",
    subtitle: "配置飞书、钉钉、Slack 等渠道接入与 Webhook。",
  },
  skills: {
    title: "技能",
    subtitle: "导入与管理 Open Skills（SKILL.md），扩展 Agent 专业能力。",
  },
  persona: {
    title: "Agents",
    subtitle: "配置 Agent 名称、工具轮次与人设（灵魂系统）。",
  },
};

export function Setup({
  onConfigSuccess,
  embedded = false,
  activeTab: activeTabProp,
  onTabChange,
}: SetupProps) {
  const [activeTabInternal, setActiveTabInternal] = useState<SetupTab>("general");
  const activeTab = activeTabProp ?? activeTabInternal;
  const setActiveTab = (tab: SetupTab) => {
    onTabChange?.(tab);
    if (activeTabProp === undefined) {
      setActiveTabInternal(tab);
    }
  };
  const [config, setConfig] = useState<Config>(initialConfig);
  const [previewCollapsed, setPreviewCollapsed] = useState(true);
  const [gatewayStatus, setGatewayStatus] = useState<GatewayStatus>({
    running: false,
    url: "http://127.0.0.1:10809",
    last_error: null,
  });
  const [busyAction, setBusyAction] = useState<
    "load" | "save" | "start" | "stop" | null
  >(null);
  const [actionMessage, setActionMessage] = useState("");
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

  const handleProvidersChange = (providers: Config["providers"]) => {
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

  useEffect(() => {
    void loadSetupState();
  }, []);

  const loadSetupState = async () => {
    setBusyAction("load");
    try {
      const [nextConfig, nextGatewayStatus] = await Promise.all([
        invokeTauri<Config>("get_setup_config"),
        invokeTauri<GatewayStatus>("gateway_status"),
      ]);

      setConfig({
        ...initialConfig,
        ...nextConfig,
        robot: nextConfig.robot ?? DEFAULT_ROBOT_CONFIG,
        providers: nextConfig.providers ?? DEFAULT_PROVIDERS,
        skills: nextConfig.skills ?? initialConfig.skills,
        agent: nextConfig.agent ?? initialConfig.agent,
      });
      setGatewayStatus(nextGatewayStatus);
      setActionMessage("已加载当前配置。");
    } catch (error) {
      setActionMessage(
        `加载配置失败：${error instanceof Error ? error.message : String(error)}`
      );
    } finally {
      setBusyAction(null);
    }
  };

  const saveSetupConfig = async () => {
    await invokeTauri("save_setup_config", { config });
    const nextGatewayStatus = await invokeTauri<GatewayStatus>("gateway_status");
    setGatewayStatus(nextGatewayStatus);
  };

  const handleSaveConfig = async () => {
    setBusyAction("save");
    try {
      await saveSetupConfig();
      setActionMessage("配置已保存。");
    } catch (error) {
      setActionMessage(
        `保存配置失败：${error instanceof Error ? error.message : String(error)}`
      );
    } finally {
      setBusyAction(null);
    }
  };

  const handleSaveAndStartGateway = async () => {
    setBusyAction("start");
    try {
      await saveSetupConfig();
      const nextGatewayStatus = await invokeTauri<GatewayStatus>("start_gateway");
      setGatewayStatus(nextGatewayStatus);
      setActionMessage(`网关已启动：${nextGatewayStatus.url}`);
      if (nextGatewayStatus.running && onConfigSuccess) {
        onConfigSuccess();
      }
    } catch (error) {
      setActionMessage(
        `启动网关失败：${error instanceof Error ? error.message : String(error)}`
      );
      const nextGatewayStatus = await invokeTauri<GatewayStatus>(
        "gateway_status"
      ).catch(() => gatewayStatus);
      setGatewayStatus(nextGatewayStatus);
    } finally {
      setBusyAction(null);
    }
  };

  const handleStopGateway = async () => {
    setBusyAction("stop");
    try {
      const nextGatewayStatus = await invokeTauri<GatewayStatus>("stop_gateway");
      setGatewayStatus(nextGatewayStatus);
      setActionMessage("网关已停止。");
    } catch (error) {
      setActionMessage(
        `停止网关失败：${error instanceof Error ? error.message : String(error)}`
      );
    } finally {
      setBusyAction(null);
    }
  };

  const renderTabContent = () => {
    switch (activeTab) {
      case "general":
        return (
          <div className="space-y-8">
            <section className="setup-section">
              <h2>基础信息</h2>
              <div className="setup-grid">
                <label>
                  Workspace 目录
                  <input
                    value={config.workspace_dir ?? ""}
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
                    type="password"
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
              <h2>OmniNova 连接</h2>
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
                    placeholder="http://localhost:10809"
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
          </div>
        );
      case "providers":
        return <ProviderConfigForm value={config.providers} onChange={handleProvidersChange} />;
      case "channels":
        return <ChannelConfigForm value={config.channels} onChange={(channels) => setConfig({ ...config, channels })} />;
      case "skills":
        return (
          <div className="setup-section">
            <h2>技能扩展</h2>
            <SkillsConfigForm 
              config={config.skills || { open_skills_enabled: true }}
              onChange={(skills) => setConfig({ ...config, skills })}
            />
          </div>
        );
      case "persona":
        return (
          <div className="setup-section">
            <h2>Agent 人设 (灵魂系统)</h2>
            <PersonaConfigForm 
              config={config.agent || { name: "omninova", max_tool_iterations: 20, compact_context: true }}
              onChange={(agent) => setConfig({ ...config, agent })}
            />
          </div>
        );
    }
  };

  const meta = SETUP_PAGE_META[activeTab];

  const gatewayActions = (
    <div className="setup-embed-actions">
      <div className="setup-gateway-pill">
        <span
          className={`setup-gateway-dot ${gatewayStatus.running ? "is-on" : "is-off"}`}
        />
        <span>网关 {gatewayStatus.running ? "运行中" : "已停止"}</span>
        {gatewayStatus.url ? (
          <code className="setup-gateway-url">{gatewayStatus.url}</code>
        ) : null}
      </div>
      <div className="setup-embed-buttons">
        <button
          type="button"
          className="setup-btn setup-btn--secondary"
          onClick={handleSaveConfig}
          disabled={busyAction !== null}
        >
          {busyAction === "save" ? "保存中…" : "保存配置"}
        </button>
        {!gatewayStatus.running ? (
          <button
            type="button"
            className="setup-btn setup-btn--primary"
            onClick={handleSaveAndStartGateway}
            disabled={busyAction !== null}
          >
            {busyAction === "start" ? "启动中…" : "保存并启动网关"}
          </button>
        ) : (
          <button
            type="button"
            className="setup-btn setup-btn--danger"
            onClick={handleStopGateway}
            disabled={busyAction !== null}
          >
            {busyAction === "stop" ? "停止中…" : "停止网关"}
          </button>
        )}
      </div>
      {actionMessage ? <p className="setup-action-hint">{actionMessage}</p> : null}
    </div>
  );

  const setupPreviewBlock = (
    <div className="setup-preview-wrap">
      <div className="setup-preview">
        <div className="setup-preview-header">
          <span>配置预览 (JSON)</span>
          <div className="setup-preview-actions">
            <button
              type="button"
              className="setup-preview-copy"
              onClick={() => setPreviewCollapsed((prev) => !prev)}
            >
              {previewCollapsed ? "展开" : "折叠"}
            </button>
            <button
              type="button"
              className="setup-preview-copy"
              onClick={() => {
                void navigator.clipboard.writeText(jsonPreview);
                setActionMessage("配置已复制到剪贴板。");
              }}
            >
              复制
            </button>
          </div>
        </div>
        {!previewCollapsed ? (
          <pre className="setup-preview-content">{jsonPreview}</pre>
        ) : null}
      </div>
    </div>
  );

  const setupMainInner = (
    <>
      {!embedded ? (
        <div className="setup-header setup-header--legacy mb-10">
          <img src={omninovalLogo} alt="" className="setup-logo-frame" />
          <div className="setup-brand-copy">
            <div className="setup-chip">OmniNova Claw</div>
            <h1 className="setup-title">智能助手配置中心</h1>
            <p className="setup-subtitle">
              设置你的 AI 模型、渠道连接与扩展技能
            </p>
          </div>
        </div>
      ) : (
        <header className="setup-embed-hero">
          <h1 className="setup-embed-title">{meta.title}</h1>
          <p className="setup-embed-sub">{meta.subtitle}</p>
        </header>
      )}

      {renderTabContent()}

      {embedded ? gatewayActions : null}

      {setupPreviewBlock}
    </>
  );

  if (embedded) {
    return (
      <div className="setup-page setup-page--embedded">{setupMainInner}</div>
    );
  }

  return (
    <div className="setup-page setup-page--standalone">
      <aside className="setup-standalone-sidebar">
        <div className="setup-standalone-brand">
          <img src={omninovalLogo} alt="" className="setup-standalone-logo" />
          <div>
            <div className="setup-standalone-kicker">OmniNova</div>
            <div className="setup-standalone-name">Claw 控制面</div>
          </div>
        </div>
        <nav className="setup-standalone-nav">
          {setupTabs.map((tab) => (
            <button
              key={tab.id}
              type="button"
              className={`setup-standalone-nav-item ${
                activeTab === tab.id ? "is-active" : ""
              }`}
              onClick={() => setActiveTab(tab.id)}
            >
              <span>{tab.icon}</span>
              <span>{tab.label}</span>
            </button>
          ))}
        </nav>
        <div className="setup-standalone-foot">
          <div className="setup-gateway-pill">
            <span
              className={`setup-gateway-dot ${gatewayStatus.running ? "is-on" : "is-off"}`}
            />
            <span>网关 {gatewayStatus.running ? "运行中" : "已停止"}</span>
          </div>
          <button
            type="button"
            className="setup-btn setup-btn--secondary setup-btn--block"
            onClick={handleSaveConfig}
            disabled={busyAction !== null}
          >
            {busyAction === "save" ? "保存中…" : "保存配置"}
          </button>
          {!gatewayStatus.running ? (
            <button
              type="button"
              className="setup-btn setup-btn--primary setup-btn--block"
              onClick={handleSaveAndStartGateway}
              disabled={busyAction !== null}
            >
              {busyAction === "start" ? "启动中…" : "保存并启动网关"}
            </button>
          ) : (
            <button
              type="button"
              className="setup-btn setup-btn--danger setup-btn--block"
              onClick={handleStopGateway}
              disabled={busyAction !== null}
            >
              {busyAction === "stop" ? "停止中…" : "停止网关"}
            </button>
          )}
          {actionMessage ? (
            <p className="setup-action-hint">{actionMessage}</p>
          ) : null}
        </div>
      </aside>
      <main className="setup-standalone-main">{setupMainInner}</main>
    </div>
  );
}
