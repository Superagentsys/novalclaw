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
import { ControlPanel } from "../Console/ControlPanel";
import { AccountSettingsForm } from "../account/AccountSettingsForm";
import { BackupSettings } from "../backup/BackupSettings";
import { PrivacySettings } from "../settings/PrivacySettings";
import { invokeTauri } from "../../utils/tauri";
import omninovalLogo from "../../assets/omninoval-logo.png";

export interface SetupProps {
  /** 配置完成且网关启动成功后调用，用于进入对话界面 */
  onConfigSuccess?: () => void;
}

const initialConfig: Config = {
  api_key: "",
  api_url: "",
  default_provider: "",
  default_model: "",
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

export function Setup({ onConfigSuccess }: SetupProps) {
  const [activeTab, setActiveTab] = useState<"general" | "providers" | "channels" | "skills" | "persona" | "account" | "backup" | "privacy">("general");
  const [config, setConfig] = useState<Config>(initialConfig);
  const [gatewayStatus, setGatewayStatus] = useState<GatewayStatus>({
    running: false,
    url: "http://127.0.0.1:42617",
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
      case "account":
        return (
          <div className="setup-section">
            <h2>账户管理</h2>
            <AccountSettingsForm />
          </div>
        );
      case "backup":
        return (
          <div className="setup-section">
            <h2>备份与恢复</h2>
            <BackupSettings onImportComplete={loadSetupState} />
          </div>
        );
      case "privacy":
        return (
          <div className="setup-section">
            <h2>隐私与安全</h2>
            <PrivacySettings />
          </div>
        );
    }
  };

  return (
    <div className="setup-page" style={{ maxWidth: 'none', margin: 0, padding: 0, height: '100vh', display: 'flex', flexDirection: 'row', backgroundColor: '#090909' }}>
      {/* Sidebar */}
      <aside style={{ width: '280px', backgroundColor: 'rgba(255, 255, 255, 0.03)', borderRight: '1px solid rgba(255, 255, 255, 0.1)', display: 'flex', flexDirection: 'column', padding: '24px' }}>
        <div className="flex items-center gap-4 mb-8">
          <img src={omninovalLogo} alt="Logo" style={{ width: '48px', height: '48px', borderRadius: '12px' }} />
          <div>
            <div className="text-sm font-bold opacity-50 uppercase tracking-wider">OmniNova</div>
            <div className="text-lg font-bold">Claw 控制面</div>
          </div>
        </div>

        <nav className="flex-1 space-y-2">
          {[
            { id: 'general', label: '通用设置', icon: '⚙️' },
            { id: 'providers', label: '模型服务', icon: '🤖' },
            { id: 'channels', label: '渠道接入', icon: '🔌' },
            { id: 'skills', label: '技能扩展', icon: '🛠️' },
            { id: 'persona', label: 'Agent 人设', icon: '🧠' },
            { id: 'account', label: '账户管理', icon: '👤' },
            { id: 'privacy', label: '隐私与安全', icon: '🔒' },
            { id: 'backup', label: '备份恢复', icon: '💾' },
          ].map((tab) => (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id as any)}
              style={{
                width: '100%',
                textAlign: 'left',
                padding: '12px 16px',
                borderRadius: '12px',
                backgroundColor: activeTab === tab.id ? 'rgba(255, 255, 255, 0.1)' : 'transparent',
                border: 'none',
                display: 'flex',
                alignItems: 'center',
                gap: '12px',
                color: activeTab === tab.id ? '#fff' : 'rgba(255, 255, 255, 0.5)',
                cursor: 'pointer',
                transition: 'all 0.2s'
              }}
            >
              <span>{tab.icon}</span>
              <span className="font-medium">{tab.label}</span>
            </button>
          ))}
        </nav>

        <div className="mt-auto pt-6 space-y-3 border-t border-white/10">
          <div className="flex items-center gap-3 px-2 py-2 mb-2">
            <div className={`w-3 h-3 rounded-full ${gatewayStatus.running ? 'bg-green-500 shadow-[0_0_8px_rgba(34,197,94,0.6)]' : 'bg-red-500'}`} />
            <span className="text-sm font-medium opacity-80">
              网关{gatewayStatus.running ? '运行中' : '已停止'}
            </span>
          </div>

          <button
            className="w-full py-3 bg-white/5 hover:bg-white/10 text-white rounded-xl font-medium border border-white/10 transition-all cursor-pointer"
            onClick={handleSaveConfig}
            disabled={busyAction !== null}
          >
            {busyAction === "save" ? "保存中..." : "保存配置"}
          </button>

          {!gatewayStatus.running ? (
            <button
              className="w-full py-3 bg-orange-600 hover:bg-orange-500 text-white rounded-xl font-bold shadow-lg shadow-orange-900/20 transition-all cursor-pointer"
              onClick={handleSaveAndStartGateway}
              disabled={busyAction !== null}
            >
              {busyAction === "start" ? "启动中..." : "保存并启动网关"}
            </button>
          ) : (
            <button
              className="w-full py-3 bg-red-600/20 hover:bg-red-600/30 text-red-500 rounded-xl font-medium border border-red-600/20 transition-all cursor-pointer"
              onClick={handleStopGateway}
              disabled={busyAction !== null}
            >
              {busyAction === "stop" ? "停止中..." : "停止网关"}
            </button>
          )}

          {actionMessage && (
            <div className="mt-4 p-3 bg-white/5 rounded-lg text-xs opacity-60 text-center italic">
              {actionMessage}
            </div>
          )}
        </div>
      </aside>

      {/* Main Content */}
      <main style={{ flex: 1, overflowY: 'auto', padding: '40px', backgroundColor: 'transparent' }}>
        <div style={{ maxWidth: '800px', margin: '0 auto' }}>
          {renderTabContent()}
          
          <div className="mt-12 pt-8 border-t border-white/5">
            <div className="setup-preview">
              <div className="setup-preview-header">
                <span>配置预览 (JSON)</span>
                <button
                  className="setup-preview-copy"
                  onClick={() => {
                    navigator.clipboard.writeText(jsonPreview);
                    setActionMessage("配置已复制到剪贴板。");
                  }}
                >
                  复制
                </button>
              </div>
              <pre className="setup-preview-content">{jsonPreview}</pre>
            </div>
          </div>
        </div>
      </main>

      <ControlPanel />
    </div>
  );
}
