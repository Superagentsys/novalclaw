import { useCallback, useEffect, useMemo, useState } from "react";
import {
  DEFAULT_PROVIDERS,
  DEFAULT_ROBOT_CONFIG,
  type Config,
  type GatewayPublicMode,
  type GatewayStatus,
} from "../../types/config";
import { ChannelConfigForm } from "./ChannelConfigForm";
import { ProviderConfigForm } from "./ProviderConfigForm";
import { RobotConfigForm } from "./RobotConfigForm";
import { SkillsConfigForm } from "./SkillsConfigForm";
import { PersonaConfigForm } from "./PersonaConfigForm";
import { invokeTauri } from "../../utils/tauri";
import omninovalLogo from "../../assets/omninoval-logo.png";
import { open } from "@tauri-apps/plugin-dialog";

/** Sensitive field names that should be redacted in JSON preview */
const SENSITIVE_KEYS = new Set([
  "app_secret",
  "app_secret_env",
  "secret",
  "signing_secret",
  "signing_secret_env",
  "encrypt_key",
  "encrypt_key_env",
  "verification_token",
  "verification_token_env",
  "authorization",
  "token",
  "token_env",
  "password",
  "api_key",
  "api_key_env",
]);

const LARK_BLOCKER_MESSAGE = "Lark 已启用但缺少 App ID。请补全 Lark 配置或关闭 Lark。";

function enabledChannelIds(config: Config): string[] {
  return Object.entries(config.channels)
    .filter(([, channel]) => channel?.enabled)
    .map(([channelId]) => channelId);
}

function formatGatewayStartError(message: string): string {
  return message.includes(LARK_BLOCKER_MESSAGE)
    ? `网关启动被 Lark 配置阻止：${LARK_BLOCKER_MESSAGE}`
    : message;
}

function normalizePublicBaseUrl(value: string | null | undefined): string | null {
  const trimmed = value?.trim().split(/[?#]/, 1)[0]?.replace(/\/+$/, "") ?? "";
  if (!trimmed) {
    return null;
  }
  const normalized = trimmed.replace(/\/webhook\/feishu(?:\/card)?$/i, "");
  const base = normalized.replace(/\/+$/, "");
  if (/^https?:\/\//i.test(base)) {
    try {
      const parsed = new URL(base);
      if (parsed.username || parsed.password) {
        return null;
      }
    } catch {
      return null;
    }
  }
  return base || null;
}

function normalizeNamedTunnelHostname(value: string | null | undefined): string | null {
  const trimmed = value?.trim() ?? "";
  if (!trimmed) {
    return null;
  }
  try {
    const parsed = new URL(
      /^https?:\/\//i.test(trimmed) ? trimmed : `https://${trimmed}`
    );
    if (
      !["http:", "https:"].includes(parsed.protocol) ||
      parsed.username ||
      parsed.password
    ) {
      return null;
    }
    return parsed.hostname.replace(/\.$/, "").toLowerCase() || null;
  } catch {
    return null;
  }
}

/** Redact sensitive values in a JSON object for display */
function redactSensitiveFields(obj: unknown): unknown {
  if (obj === null || obj === undefined) {
    return obj;
  }
  if (Array.isArray(obj)) {
    return obj.map(redactSensitiveFields);
  }
  if (typeof obj === "object") {
    const result: Record<string, unknown> = {};
    for (const [key, value] of Object.entries(obj as Record<string, unknown>)) {
      if (SENSITIVE_KEYS.has(key.toLowerCase()) && typeof value === "string") {
        result[key] = "********";
      } else if (typeof value === "object" && value !== null) {
        result[key] = redactSensitiveFields(value);
      } else {
        result[key] = value;
      }
    }
    return result;
  }
  return obj;
}
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
    lark: { enabled: false },
  },
  gateway_public: {
    mode: "external_public_url",
    public_webhook_base_url: null,
    cloudflared_path: null,
    named_tunnel_name: null,
    named_tunnel_hostname: null,
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
  multimodal: {
    desktop_vision_enabled: false,
    desktop_vision_max_dimension_px: 1280,
  },
  observability: {
    prometheus_enabled: false,
    prometheus_port: 9090,
  },
  audit: {
    enabled: false,
    record_arguments: false,
  },
};

export type SetupTab = "general" | "providers" | "channels" | "skills" | "persona";

interface CliInstallStatus {
  bundledAvailable: boolean;
  bundledPath: string | null;
  installDir: string;
  installedPath: string | null;
  installedSameAsBundle: boolean;
  onPath: boolean;
  hint: string;
}
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

/** 当仅启用一个 provider 时，自动设为 default_provider / default_model。 */
function resolveDefaultProviderSelection(
  providers: Config["providers"],
  currentDefaultProvider: string,
  currentDefaultModel: string
): Pick<Config, "default_provider" | "default_model"> {
  const enabled = providers.filter((provider) => provider.enabled);
  const enabledIds = enabled.map((provider) => provider.id);

  let defaultProvider = enabledIds.includes(currentDefaultProvider)
    ? currentDefaultProvider
    : "";

  if (enabled.length === 1) {
    defaultProvider = enabled[0].id;
  }

  const activeProvider = providers.find(
    (provider) => provider.id === defaultProvider
  );
  let defaultModel =
    activeProvider?.models.includes(currentDefaultModel)
      ? currentDefaultModel
      : "";

  if (activeProvider && !defaultModel && activeProvider.models.length > 0) {
    defaultModel = activeProvider.models[0];
  }

  return {
    default_provider: defaultProvider,
    default_model: defaultModel,
  };
}

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
    gateway_host: "127.0.0.1",
    gateway_port: 10809,
    gateway_public_mode: "external_public_url",
    quick_tunnel_non_production: false,
    cloudflared_configured: false,
    cloudflared_found: false,
    named_tunnel_name_configured: false,
    named_tunnel_hostname_configured: false,
    named_tunnel_config_complete: false,
    public_health: {
      configured: false,
      ok: false,
      checked_at: null,
      error_kind: "not_configured",
      error: "Public Base URL 未配置。",
    },
    enabled_channels: [],
    store_opened: false,
    retry_worker_enabled: false,
    health_ok: false,
    last_error: null,
  });
  const [busyAction, setBusyAction] = useState<
    "load" | "save" | "start" | "stop" | "restart" | "health" | "public-health" | null
  >(null);
  const [actionMessage, setActionMessage] = useState("");
  const [channelValidationError, setChannelValidationError] = useState<string | undefined>();
  const [activeChannelId, setActiveChannelId] = useState("feishu");
  const [cliInstall, setCliInstall] = useState<CliInstallStatus | null>(null);
  const [cliBusy, setCliBusy] = useState(false);
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

  const jsonPreview = useMemo(() => {
    const redacted = redactSensitiveFields(config);
    return JSON.stringify(redacted, null, 2);
  }, [config]);

  const handleProvidersChange = (providers: Config["providers"]) => {
    const { default_provider, default_model } = resolveDefaultProviderSelection(
      providers,
      config.default_provider ?? "",
      config.default_model ?? ""
    );

    setConfig({
      ...config,
      providers,
      default_provider,
      default_model,
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

  const refreshCliInstall = useCallback(async () => {
    try {
      const s = await invokeTauri<CliInstallStatus>("cli_install_status");
      setCliInstall(s);
    } catch {
      setCliInstall(null);
    }
  }, []);

  useEffect(() => {
    void loadSetupState();
  }, []);

  useEffect(() => {
    if (activeTab === "general") {
      void refreshCliInstall();
    }
  }, [activeTab, refreshCliInstall]);

  useEffect(() => {
    if (activeTab !== "channels") {
      return;
    }
    let disposed = false;
    const refresh = async () => {
      try {
        const status = await invokeTauri<GatewayStatus>("gateway_status");
        if (!disposed) {
          setGatewayStatus(status);
        }
      } catch {
        // Keep the most recent snapshot. Explicit actions surface errors.
      }
    };
    void refresh();
    const interval = window.setInterval(() => void refresh(), 5000);
    return () => {
      disposed = true;
      window.clearInterval(interval);
    };
  }, [activeTab]);

  const loadSetupState = async () => {
    setBusyAction("load");
    try {
      const [nextConfig, nextGatewayStatus] = await Promise.all([
        invokeTauri<Config>("get_setup_config"),
        invokeTauri<GatewayStatus>("gateway_status"),
      ]);

      const merged: Config = {
        ...initialConfig,
        ...nextConfig,
        robot: nextConfig.robot ?? DEFAULT_ROBOT_CONFIG,
        providers: nextConfig.providers ?? DEFAULT_PROVIDERS,
        skills: nextConfig.skills ?? initialConfig.skills,
        agent: nextConfig.agent ?? initialConfig.agent,
        gateway_public: {
          ...initialConfig.gateway_public!,
          ...nextConfig.gateway_public,
        },
      };
      const { default_provider, default_model } = resolveDefaultProviderSelection(
        merged.providers,
        merged.default_provider ?? "",
        merged.default_model ?? ""
      );

      setConfig({
        ...merged,
        default_provider,
        default_model,
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

  const saveSetupConfig = async (
    validateAllChannels: boolean,
    configToSave = config,
    channelId = activeChannelId,
  ): Promise<boolean> => {
    const result = await invokeTauri<{ gateway_restarted: boolean }>("save_setup_config", {
      config: configToSave,
      validateAllChannels,
      activeChannelId: channelId,
    });
    const nextGatewayStatus = await invokeTauri<GatewayStatus>("gateway_status");
    setGatewayStatus(nextGatewayStatus);
    return result?.gateway_restarted ?? false;
  };

  const handleSaveConfig = async () => {
    if (channelValidationError) {
      setActionMessage(`配置验证失败：${channelValidationError}`);
      return;
    }
    setBusyAction("save");
    try {
      const restarted = await saveSetupConfig(false);
      if (restarted) {
        setActionMessage("Workspace 已切换，网关已重启。");
      } else {
        setActionMessage("配置已保存。");
      }
    } catch (error) {
      setActionMessage(
        `保存配置失败：${error instanceof Error ? error.message : String(error)}`
      );
    } finally {
      setBusyAction(null);
    }
  };

  const handleSaveAndStartGateway = async () => {
    if (channelValidationError) {
      setActionMessage(`配置验证失败：${channelValidationError}`);
      return;
    }
    setBusyAction("start");
    setActionMessage(""); // Clear previous errors
    try {
      const restarted = await saveSetupConfig(true);
      const nextGatewayStatus = await invokeTauri<GatewayStatus>("start_gateway");
      setGatewayStatus(nextGatewayStatus);
      if (nextGatewayStatus.running) {
        const enabledChannels = enabledChannelIds(config);
        const msg = restarted
          ? `Workspace 已切换，网关已重启：${nextGatewayStatus.url}`
          : `网关已启动：${nextGatewayStatus.url}`;
        setActionMessage(`${msg}。已启用频道：${enabledChannels.join(", ") || "无"}`);
        if (onConfigSuccess) {
          onConfigSuccess();
        }
      } else {
        // Gateway failed to start - show detailed error
        const errorMsg = nextGatewayStatus.last_error || "网关启动失败，原因未知";
        setActionMessage(formatGatewayStartError(errorMsg));
      }
    } catch (error) {
      // Tauri returns the error message as a string
      const errorMsg = error instanceof Error ? error.message : String(error);
      setActionMessage(formatGatewayStartError(errorMsg));
      // Refresh status
      try {
        const nextGatewayStatus = await invokeTauri<GatewayStatus>("gateway_status");
        setGatewayStatus(nextGatewayStatus);
      } catch {
        // Ignore status refresh errors
      }
    } finally {
      setBusyAction(null);
    }
  };

  const handleGoToLarkConfig = () => {
    setActiveChannelId("lark");
  };

  const handleDisableLark = async () => {
    const existingLark = config.channels.lark ?? { enabled: false };
    const nextConfig: Config = {
      ...config,
      channels: {
        ...config.channels,
        // Preserve the existing credentials and extra fields; only disable it.
        lark: { ...existingLark, enabled: false },
      },
    };

    setBusyAction("save");
    try {
      await saveSetupConfig(false, nextConfig, "lark");
      setConfig(nextConfig);
      setChannelValidationError(undefined);
      setActionMessage("Lark 已关闭，配置已保存。现在可以再次启动 Feishu Gateway。");
    } catch (error) {
      setActionMessage(
        `关闭 Lark 失败：${error instanceof Error ? error.message : String(error)}`
      );
    } finally {
      setBusyAction(null);
    }
  };

  const larkBlockerActions = actionMessage.includes(LARK_BLOCKER_MESSAGE) ? (
    <div className="setup-embed-buttons" style={{ marginTop: "0.5rem" }}>
      <button
        type="button"
        className="setup-btn setup-btn--secondary"
        onClick={handleGoToLarkConfig}
        disabled={busyAction !== null}
      >
        前往 Lark 配置
      </button>
      <button
        type="button"
        className="setup-btn setup-btn--secondary"
        onClick={() => void handleDisableLark()}
        disabled={busyAction !== null}
      >
        关闭 Lark
      </button>
    </div>
  ) : null;

  const handleCliInstall = async () => {
    setCliBusy(true);
    try {
      const msg = await invokeTauri<string>("cli_install_to_user_path");
      setActionMessage(msg);
      await refreshCliInstall();
    } catch (error) {
      setActionMessage(
        `CLI 安装失败：${error instanceof Error ? error.message : String(error)}`
      );
    } finally {
      setCliBusy(false);
    }
  };

  const handleStopGateway = async () => {
    setBusyAction("stop");
    setActionMessage(""); // Clear previous errors
    try {
      const nextGatewayStatus = await invokeTauri<GatewayStatus>("stop_gateway");
      setGatewayStatus(nextGatewayStatus);
      if (!nextGatewayStatus.running) {
        setActionMessage("网关已停止。");
      } else {
        // Should not happen normally, but handle gracefully
        setActionMessage("网关停止可能未完全成功，请检查状态。");
      }
    } catch (error) {
      // Tauri returns the error message as a string
      const errorMsg = error instanceof Error ? error.message : String(error);
      setActionMessage(errorMsg);
      // Refresh status
      try {
        const nextGatewayStatus = await invokeTauri<GatewayStatus>("gateway_status");
        setGatewayStatus(nextGatewayStatus);
      } catch {
        // Ignore status refresh errors
      }
    } finally {
      setBusyAction(null);
    }
  };

  const handleRestartGateway = async () => {
    setBusyAction("restart");
    setActionMessage("");
    try {
      await saveSetupConfig(true);
      const nextGatewayStatus = await invokeTauri<GatewayStatus>("restart_gateway");
      setGatewayStatus(nextGatewayStatus);
      setActionMessage(
        nextGatewayStatus.running
          ? `Gateway 已重启：${nextGatewayStatus.url}`
          : nextGatewayStatus.last_error || "Gateway 重启失败。"
      );
    } catch (error) {
      setActionMessage(
        formatGatewayStartError(error instanceof Error ? error.message : String(error))
      );
      try {
        setGatewayStatus(await invokeTauri<GatewayStatus>("gateway_status"));
      } catch {
        // Keep the last known status.
      }
    } finally {
      setBusyAction(null);
    }
  };

  const handleTestGatewayHealth = async () => {
    setBusyAction("health");
    try {
      const result = await invokeTauri<{
        ok: boolean;
        status_code?: number | null;
        message: string;
      }>("test_gateway_health");
      setActionMessage(result.message);
      setGatewayStatus(await invokeTauri<GatewayStatus>("gateway_status"));
    } catch (error) {
      setActionMessage(
        `Gateway 健康检查失败：${error instanceof Error ? error.message : String(error)}`
      );
    } finally {
      setBusyAction(null);
    }
  };

  const handleTestGatewayPublicHealth = async () => {
    setBusyAction("public-health");
    try {
      const result = await invokeTauri<GatewayStatus["public_health"]>(
        "test_gateway_public_health"
      );
      setActionMessage(
        result.ok
          ? `公网 Health 检查通过：${result.checked_url ?? result.base_url ?? ""}`
          : `公网 Health 检查失败：${result.error ?? "未知错误"}`
      );
      setGatewayStatus(await invokeTauri<GatewayStatus>("gateway_status"));
    } catch (error) {
      setActionMessage(
        `公网 Health 检查失败：${error instanceof Error ? error.message : String(error)}`
      );
    } finally {
      setBusyAction(null);
    }
  };

  const copyGatewayUrl = (url: string | null | undefined, label: string) => {
    if (!url) {
      setActionMessage(`${label}尚未生成。`);
      return;
    }
    void navigator.clipboard.writeText(url);
    setActionMessage(`${label}已复制。`);
  };

  const handlePickWorkspaceDir = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "选择 Agent Workspace 目录",
      });
      if (selected != null) {
        setConfig({ ...config, workspace_dir: selected as string });
        setActionMessage(`已选择 Workspace 目录：${selected}`);
      }
    } catch (error) {
      setActionMessage(
        `选择目录失败：${error instanceof Error ? error.message : String(error)}`
      );
    }
  };

  const handleClearWorkspaceDir = () => {
    setConfig({ ...config, workspace_dir: undefined });
    setActionMessage("Workspace 目录已清空。保存后 Agent 会要求先选择真实工作目录。");
  };

  const namedTunnelMode =
    config.gateway_public?.mode === "named_cloudflare_tunnel";
  const draftNamedTunnelHostname = normalizeNamedTunnelHostname(
    config.gateway_public?.named_tunnel_hostname
  );
  const draftNamedTunnelBase =
    namedTunnelMode && draftNamedTunnelHostname
      ? `https://${draftNamedTunnelHostname}`
      : null;
  const draftPublicBase = namedTunnelMode
    ? draftNamedTunnelBase
    : normalizePublicBaseUrl(config.gateway_public?.public_webhook_base_url);
  const callbackBase = namedTunnelMode
    ? draftNamedTunnelBase
    : draftPublicBase ?? gatewayStatus.url?.replace(/\/$/, "") ?? null;
  const namedTunnelNameConfigured =
    Boolean(config.gateway_public?.named_tunnel_name?.trim());
  const namedTunnelConfigComplete =
    namedTunnelNameConfigured && Boolean(draftNamedTunnelHostname);
  const runtimeWebhookUrl = callbackBase ? `${callbackBase}/webhook/feishu` : null;
  const runtimeCardCallbackUrl = callbackBase
    ? `${callbackBase}/webhook/feishu/card`
    : null;
  const lastStartedLabel = gatewayStatus.last_started_at
    ? new Date(gatewayStatus.last_started_at * 1000).toLocaleString()
    : "尚未记录";
  const publicHealthCheckedLabel = gatewayStatus.public_health?.checked_at
    ? new Date(gatewayStatus.public_health.checked_at * 1000).toLocaleString()
    : "尚未检测";

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
                  <div className="setup-input-with-actions">
                    <input
                      value={config.workspace_dir ?? ""}
                      onChange={(event) =>
                        setConfig({ ...config, workspace_dir: event.target.value })
                      }
                      placeholder="/path/to/workspace"
                    />
                    <button
                      type="button"
                      className="setup-btn setup-btn--secondary"
                      onClick={() => void handlePickWorkspaceDir()}
                    >
                      选择目录
                    </button>
                    <button
                      type="button"
                      className="setup-btn setup-btn--ghost"
                      onClick={handleClearWorkspaceDir}
                      disabled={!config.workspace_dir}
                    >
                      清空
                    </button>
                  </div>
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

            <section className="setup-section">
              <h2>桌面视觉监控</h2>
              <p className="setup-embed-sub" style={{ marginTop: 0, marginBottom: "0.75rem" }}>
                开启后，聊天区可打开「桌面视觉」开关；每次发送消息时会截取主屏幕并附加到请求中，供支持视觉的多模态模型分析（需使用
                GPT-4o、DeepSeek-VL、豆包视觉等 OpenAI 兼容视觉模型）。macOS 需在
                系统设置 → 隐私与安全性 → 屏幕录制 中授权本应用。
              </p>
              <div className="setup-grid">
                <label className="setup-toggle-row">
                  <input
                    type="checkbox"
                    checked={config.multimodal?.desktop_vision_enabled ?? false}
                    onChange={(event) =>
                      setConfig({
                        ...config,
                        multimodal: {
                          ...config.multimodal,
                          desktop_vision_enabled: event.target.checked,
                          desktop_vision_max_dimension_px:
                            config.multimodal?.desktop_vision_max_dimension_px ?? 1280,
                        },
                      })
                    }
                  />
                  <span>允许桌面视觉监控（总开关）</span>
                </label>
                <label>
                  截图最长边（像素）
                  <input
                    type="number"
                    min={320}
                    max={4096}
                    value={config.multimodal?.desktop_vision_max_dimension_px ?? 1280}
                    disabled={!config.multimodal?.desktop_vision_enabled}
                    onChange={(event) =>
                      setConfig({
                        ...config,
                        multimodal: {
                          desktop_vision_enabled:
                            config.multimodal?.desktop_vision_enabled ?? false,
                          desktop_vision_max_dimension_px: Math.max(
                            320,
                            Number(event.target.value) || 1280
                          ),
                        },
                      })
                    }
                  />
                </label>
              </div>
            </section>

            <section className="setup-section">
              <h2>审计与可观测性</h2>
              <p className="setup-embed-sub" style={{ marginTop: 0, marginBottom: "0.75rem" }}>
                全链路审计写入工作区 <code>.omninova-audit.log</code>（JSONL）。
                Prometheus 指标在网关独立端口暴露（默认 9090），主网关仍保留{" "}
                <code>/metrics</code> 路径；可在 Grafana 导入{" "}
                <code>docs/grafana/omninova-dashboard.json</code>。
              </p>
              <div className="setup-grid">
                <label className="setup-toggle-row">
                  <input
                    type="checkbox"
                    checked={config.audit?.enabled ?? false}
                    onChange={(event) =>
                      setConfig({
                        ...config,
                        audit: {
                          ...config.audit,
                          enabled: event.target.checked,
                          record_arguments: config.audit?.record_arguments ?? false,
                        },
                      })
                    }
                  />
                  <span>启用全链路审计日志</span>
                </label>
                <label className="setup-toggle-row">
                  <input
                    type="checkbox"
                    checked={config.audit?.record_arguments ?? false}
                    disabled={!config.audit?.enabled}
                    onChange={(event) =>
                      setConfig({
                        ...config,
                        audit: {
                          enabled: config.audit?.enabled ?? false,
                          record_arguments: event.target.checked,
                        },
                      })
                    }
                  />
                  <span>审计记录工具参数（敏感，默认关闭）</span>
                </label>
                <label className="setup-toggle-row">
                  <input
                    type="checkbox"
                    checked={config.observability?.prometheus_enabled ?? false}
                    onChange={(event) =>
                      setConfig({
                        ...config,
                        observability: {
                          ...config.observability,
                          prometheus_enabled: event.target.checked,
                          prometheus_port: config.observability?.prometheus_port ?? 9090,
                        },
                      })
                    }
                  />
                  <span>启用 Prometheus 指标</span>
                </label>
                <label>
                  Prometheus 端口
                  <input
                    type="number"
                    min={1024}
                    max={65535}
                    value={config.observability?.prometheus_port ?? 9090}
                    disabled={!config.observability?.prometheus_enabled}
                    onChange={(event) =>
                      setConfig({
                        ...config,
                        observability: {
                          prometheus_enabled:
                            config.observability?.prometheus_enabled ?? false,
                          prometheus_port: Math.min(
                            65535,
                            Math.max(1024, Number(event.target.value) || 9090)
                          ),
                        },
                      })
                    }
                  />
                </label>
              </div>
            </section>

            <section className="setup-section">
              <h2>命令行 omninova（全平台）</h2>
              <p className="setup-embed-sub" style={{ marginTop: 0, marginBottom: "0.75rem" }}>
                将随应用分发的 CLI 安装到用户目录并写入 PATH，无需管理员权限；效果类似 Ollama 安装后可在终端直接使用
                <code style={{ margin: "0 0.2em" }}>omninova</code>。
              </p>
              {cliInstall ? (
                <div className="setup-grid">
                  <div style={{ gridColumn: "1 / -1" }}>
                    <p style={{ margin: "0 0 0.5rem", fontSize: "0.9rem" }}>{cliInstall.hint}</p>
                    <ul
                      style={{
                        margin: "0 0 0.75rem",
                        paddingLeft: "1.25rem",
                        fontSize: "0.85rem",
                        opacity: 0.9,
                      }}
                    >
                      <li>
                        安装目录：<code>{cliInstall.installDir}</code>
                      </li>
                      {cliInstall.bundledAvailable ? (
                        <li>随包 CLI：已检测到</li>
                      ) : (
                        <li>随包 CLI：未检测到（开发构建需先编译 omninova）</li>
                      )}
                      {cliInstall.installedPath ? (
                        <li>
                          当前已安装：<code>{cliInstall.installedPath}</code>
                        </li>
                      ) : null}
                      <li>
                        当前会话 PATH 已包含安装目录：
                        {cliInstall.onPath ? "是" : "否"}
                      </li>
                    </ul>
                    <button
                      type="button"
                      className="setup-btn setup-btn--primary"
                      disabled={cliBusy || !cliInstall.bundledAvailable}
                      onClick={() => void handleCliInstall()}
                    >
                      {cliBusy ? "安装中…" : "安装 / 更新 omninova 到 PATH"}
                    </button>
                  </div>
                </div>
              ) : (
                <p className="setup-action-hint">正在读取 CLI 状态…</p>
              )}
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
        return (
          <>
            <section className="setup-section gateway-runtime-panel">
              <div className="section-heading gateway-runtime-heading">
                <div>
                  <h2>Gateway 运行状态</h2>
                  <div className="section-subtitle">
                    运行态、回调地址和隐私安全信息均来自当前 Gateway 配置。
                  </div>
                </div>
                <span
                  className={`gateway-status-chip ${
                    gatewayStatus.running ? "is-running" : "is-stopped"
                  }`}
                >
                  {gatewayStatus.running ? "运行中" : "已停止"}
                </span>
              </div>
              <div className="gateway-public-config">
                <label>
                  公网入口模式
                  <select
                    value={config.gateway_public?.mode ?? "external_public_url"}
                    onChange={(event) =>
                      setConfig({
                        ...config,
                        gateway_public: {
                          ...initialConfig.gateway_public!,
                          ...config.gateway_public,
                          mode: event.target.value as GatewayPublicMode,
                        },
                      })
                    }
                  >
                    <option value="external_public_url">外部 Public URL</option>
                    <option value="named_cloudflare_tunnel">Cloudflare Named Tunnel</option>
                    <option value="quick_tunnel">Cloudflare Quick Tunnel（临时）</option>
                  </select>
                </label>
                <label>
                  Public Base URL
                  <input
                    value={
                      namedTunnelMode
                        ? draftNamedTunnelBase ?? ""
                        : config.gateway_public?.public_webhook_base_url ?? ""
                    }
                    readOnly={namedTunnelMode}
                    onChange={(event) =>
                      setConfig({
                        ...config,
                        gateway_public: {
                          ...initialConfig.gateway_public!,
                          ...config.gateway_public,
                          public_webhook_base_url: event.target.value,
                        },
                      })
                    }
                    placeholder="https://gateway.example.com"
                  />
                  <small>
                    {namedTunnelMode
                      ? "由 Named Tunnel Hostname 自动生成并保存。"
                      : "只填写域名 Base；保存时会自动移除 webhook 路径后缀。"}
                  </small>
                </label>
                {config.gateway_public?.mode === "named_cloudflare_tunnel" ? (
                  <>
                    <label>
                      Named Tunnel 名称
                      <input
                        value={config.gateway_public.named_tunnel_name ?? ""}
                        onChange={(event) =>
                          setConfig({
                            ...config,
                            gateway_public: {
                              ...initialConfig.gateway_public!,
                              ...config.gateway_public,
                              named_tunnel_name: event.target.value,
                            },
                          })
                        }
                        placeholder="omninova-gateway"
                      />
                    </label>
                    <label>
                      Named Tunnel Hostname
                      <input
                        value={config.gateway_public.named_tunnel_hostname ?? ""}
                        onChange={(event) =>
                          setConfig({
                            ...config,
                            gateway_public: {
                              ...initialConfig.gateway_public!,
                              ...config.gateway_public,
                              named_tunnel_hostname: event.target.value,
                            },
                          })
                        }
                        placeholder="gateway.example.com"
                      />
                    </label>
                  </>
                ) : null}
                {config.gateway_public?.mode === "quick_tunnel" ||
                config.gateway_public?.mode === "named_cloudflare_tunnel" ? (
                  <label>
                    cloudflared 路径
                    <input
                      value={config.gateway_public.cloudflared_path ?? ""}
                      onChange={(event) =>
                        setConfig({
                          ...config,
                          gateway_public: {
                            ...initialConfig.gateway_public!,
                            ...config.gateway_public,
                            cloudflared_path: event.target.value,
                          },
                        })
                      }
                      placeholder="C:\Program Files\cloudflared\cloudflared.exe"
                    />
                    <small>用于检测固定或临时隧道运行环境，不保存隧道凭据。</small>
                  </label>
                ) : null}
              </div>
              <div className="gateway-runtime-grid">
                <div><span>本地地址</span><code>{gatewayStatus.url}</code></div>
                <div>
                  <span>Public Base URL</span>
                  <code>{draftPublicBase || "未配置"}</code>
                </div>
                <div>
                  <span>公网入口模式</span>
                  <strong>
                    {config.gateway_public?.mode ?? gatewayStatus.gateway_public_mode}
                  </strong>
                </div>
                {namedTunnelMode ? (
                  <div>
                    <span>Named Tunnel 配置</span>
                    <strong>{namedTunnelConfigComplete ? "完整" : "缺失"}</strong>
                  </div>
                ) : null}
                {namedTunnelMode ? (
                  <div>
                    <span>固定 Hostname</span>
                    <strong>{draftNamedTunnelHostname || "未配置"}</strong>
                  </div>
                ) : null}
                <div><span>安全模式</span><strong>{gatewayStatus.security_mode || "dev"}</strong></div>
                <div><span>出站模式</span><strong>{gatewayStatus.outbound_mode || "disabled"}</strong></div>
                <div><span>Store</span><strong>{gatewayStatus.store_opened ? "已打开" : "未打开"}</strong></div>
                <div>
                  <span>Retry worker</span>
                  <strong>{gatewayStatus.retry_worker_enabled ? "已启动" : "未启动"}</strong>
                </div>
                <div><span>本地 Health</span><strong>{gatewayStatus.health_ok ? "正常" : "未就绪"}</strong></div>
                <div>
                  <span>公网 Health</span>
                  <strong>
                    {!gatewayStatus.public_health?.configured
                      ? "未配置"
                      : gatewayStatus.public_health.ok
                        ? "正常"
                        : "异常"}
                  </strong>
                </div>
                <div><span>公网检测时间</span><strong>{publicHealthCheckedLabel}</strong></div>
                <div><span>上次启动</span><strong>{lastStartedLabel}</strong></div>
              </div>
              <div className="gateway-runtime-url-list">
                <div>
                  <span>普通事件回调</span>
                  <code>{runtimeWebhookUrl || "未生成"}</code>
                  <button
                    type="button"
                    className="setup-btn setup-btn--secondary"
                    onClick={() => copyGatewayUrl(runtimeWebhookUrl, "普通事件回调 URL")}
                    disabled={!runtimeWebhookUrl}
                  >
                    复制
                  </button>
                </div>
                <div>
                  <span>卡片交互回调</span>
                  <code>{runtimeCardCallbackUrl || "未生成"}</code>
                  <button
                    type="button"
                    className="setup-btn setup-btn--secondary"
                    onClick={() => copyGatewayUrl(runtimeCardCallbackUrl, "卡片交互回调 URL")}
                    disabled={!runtimeCardCallbackUrl}
                  >
                    复制
                  </button>
                </div>
              </div>
              <div className="gateway-runtime-meta">
                已启用频道：{gatewayStatus.enabled_channels?.join("、") || "无"}
                {gatewayStatus.store_path ? ` · Store：${gatewayStatus.store_path}` : ""}
                {` · cloudflared path：${gatewayStatus.cloudflared_configured ? "已配置" : "未配置"}`}
                {` · cloudflared found：${gatewayStatus.cloudflared_found ? "true" : "false"}`}
              </div>
              <div className="gateway-runtime-health-actions">
                <button
                  type="button"
                  className="setup-btn setup-btn--secondary"
                  onClick={handleTestGatewayPublicHealth}
                  disabled={busyAction !== null}
                >
                  {busyAction === "public-health" ? "检测中…" : "测试公网 Health"}
                </button>
                {gatewayStatus.public_health?.error &&
                !["not_checked", "url_not_configured"].includes(
                  gatewayStatus.public_health.error_kind ?? ""
                ) ? (
                  <span className="gateway-public-error">
                    公网检测：{gatewayStatus.public_health.error}
                  </span>
                ) : null}
              </div>
              {(config.gateway_public?.mode === "quick_tunnel" ||
                gatewayStatus.quick_tunnel_non_production) ? (
                <div className="gateway-runtime-warning">
                  Quick Tunnel 地址会变化，只适合临时开发测试，不适合正式环境。
                </div>
              ) : null}
              {namedTunnelMode ? (
                <div
                  className={
                    namedTunnelConfigComplete
                      ? "gateway-runtime-info"
                      : "gateway-runtime-warning"
                  }
                >
                  {namedTunnelConfigComplete
                    ? "Named Tunnel 使用固定公网入口，飞书回调地址不会随重启变化。"
                    : "Named Tunnel 配置不完整，请填写 Tunnel Name 和有效 Hostname。"}
                </div>
              ) : null}
              {(gatewayStatus.security_mode || "dev") === "dev" ? (
                <div className="gateway-runtime-warning">
                  dev 模式允许未校验 webhook，仅适合本地开发，不适合生产环境。
                </div>
              ) : null}
              {gatewayStatus.last_error ? (
                <div className="gateway-status-error">最近错误：{gatewayStatus.last_error}</div>
              ) : null}
            </section>
            <ChannelConfigForm
              value={config.channels}
              onChange={(channels) => setConfig({ ...config, channels })}
              validationError={channelValidationError}
              onValidationChange={setChannelValidationError}
              selectedChannelId={activeChannelId}
              onSelectedChannelChange={setActiveChannelId}
              gatewayUrl={gatewayStatus.running ? gatewayStatus.url : undefined}
              onHealthCheck={async () => {
                const result = await invokeTauri<{
                  ok: boolean;
                  message: string;
                }>("test_gateway_health");
                return result;
              }}
              onCopyWebhookUrl={(url) => {
                void navigator.clipboard.writeText(url);
              }}
            />
          </>
        );
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
          <>
            <button
              type="button"
              className="setup-btn setup-btn--secondary"
              onClick={handleRestartGateway}
              disabled={busyAction !== null}
            >
              {busyAction === "restart" ? "重启中…" : "重启网关"}
            </button>
            <button
              type="button"
              className="setup-btn setup-btn--danger"
              onClick={handleStopGateway}
              disabled={busyAction !== null}
            >
              {busyAction === "stop" ? "停止中…" : "停止网关"}
            </button>
          </>
        )}
        <button
          type="button"
          className="setup-btn setup-btn--secondary"
          onClick={handleTestGatewayHealth}
          disabled={busyAction !== null || !gatewayStatus.running}
        >
          {busyAction === "health" ? "检测中…" : "测试本地 Health"}
        </button>
      </div>
      {actionMessage ? <p className="setup-action-hint">{actionMessage}</p> : null}
      {larkBlockerActions}
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
            <>
              <button
                type="button"
                className="setup-btn setup-btn--secondary setup-btn--block"
                onClick={handleRestartGateway}
                disabled={busyAction !== null}
              >
                {busyAction === "restart" ? "重启中…" : "重启网关"}
              </button>
              <button
                type="button"
                className="setup-btn setup-btn--danger setup-btn--block"
                onClick={handleStopGateway}
                disabled={busyAction !== null}
              >
                {busyAction === "stop" ? "停止中…" : "停止网关"}
              </button>
            </>
          )}
          <button
            type="button"
            className="setup-btn setup-btn--secondary setup-btn--block"
            onClick={handleTestGatewayHealth}
            disabled={busyAction !== null || !gatewayStatus.running}
          >
            {busyAction === "health" ? "检测中…" : "测试本地 Health"}
          </button>
          {actionMessage ? (
            <p className="setup-action-hint">{actionMessage}</p>
          ) : null}
          {larkBlockerActions}
        </div>
      </aside>
      <main className="setup-standalone-main">{setupMainInner}</main>
    </div>
  );
}
