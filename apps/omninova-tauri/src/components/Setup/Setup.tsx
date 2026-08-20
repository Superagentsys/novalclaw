import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  CHANNEL_PRESETS,
  DEFAULT_PROVIDERS,
  DEFAULT_ROBOT_CONFIG,
  type Config,
  type DingtalkDiagnostics,
  type DingtalkPublicRouteProbe,
  type FeishuDiagnostics,
  type GatewayPublicMode,
  type GatewayStatus,
} from "../../types/config";
import { ChannelConfigForm } from "./ChannelConfigForm";
import { ProviderConfigForm } from "./ProviderConfigForm";
import { SkillsConfigForm } from "./SkillsConfigForm";
import { PersonaConfigForm } from "./PersonaConfigForm";
import { invokeTauri } from "../../utils/tauri";
import omninovalLogo from "../../assets/omninoval-logo.png";
import { open } from "@tauri-apps/plugin-dialog";
import { UiIcon, type UiIconName } from "../UiIcon";
import { notifySetupConfigUpdated } from "../../utils/appEvents";
import { writeClipboardText } from "../../utils/clipboard";

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

type HealthUiStatus = "not_configured" | "not_ready" | "idle" | "ok" | "error";

interface LocalHealthResult {
  ok: boolean;
  status_code?: number | null;
  message: string;
}

const CHANNEL_LABELS = new Map<string, string>(
  CHANNEL_PRESETS.map((preset) => [preset.id, preset.name])
);

function formatEnabledChannels(channelIds: string[]): string {
  return channelIds
    .map((channelId) => CHANNEL_LABELS.get(channelId) ?? channelId)
    .join("、");
}

function channelIdIsKnown(channelId: string | null | undefined): channelId is string {
  return Boolean(channelId && CHANNEL_LABELS.has(channelId));
}

function readRememberedChannelId(): string | null {
  try {
    const remembered = window.sessionStorage.getItem("omninova.setup.activeChannel");
    return channelIdIsKnown(remembered) ? remembered : null;
  } catch {
    return null;
  }
}

function rememberChannelId(channelId: string): void {
  try {
    window.sessionStorage.setItem("omninova.setup.activeChannel", channelId);
  } catch {
    // Storage can be unavailable in privacy-restricted browser contexts.
  }
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
  const normalized = trimmed
    .replace(/\/api\/v1\/gateway\/dingtalk\/events$/i, "")
    .replace(/\/webhook\/dingtalk$/i, "")
    .replace(/\/webhook\/feishu(?:\/card)?$/i, "");
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

function resolveDraftPublicBase(
  gatewayPublic: Config["gateway_public"]
): string | null {
  if (gatewayPublic?.mode === "named_cloudflare_tunnel") {
    const hostname = normalizeNamedTunnelHostname(gatewayPublic.named_tunnel_hostname);
    return hostname ? `https://${hostname}` : null;
  }
  return normalizePublicBaseUrl(gatewayPublic?.public_webhook_base_url);
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
  /** dialog：用于弹出设置，隐藏大标题与 JSON 预览 */
  presentation?: "page" | "dialog";
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
  icon: UiIconName;
};

const setupTabs: SetupTabItem[] = [
  { id: "general", label: "通用设置", icon: "settings" },
  { id: "providers", label: "模型服务", icon: "apps" },
  { id: "channels", label: "渠道接入", icon: "connections" },
  { id: "skills", label: "技能扩展", icon: "tool" },
  { id: "persona", label: "Agent 人设", icon: "agent" },
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
    subtitle: "选择默认模型，再按需接入云端或本地服务。",
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
  presentation = "page",
  activeTab: activeTabProp,
  onTabChange,
}: SetupProps) {
  const dialogMode = presentation === "dialog";
  const [activeTabInternal, setActiveTabInternal] = useState<SetupTab>("general");
  const activeTab = activeTabProp ?? activeTabInternal;
  const setActiveTab = (tab: SetupTab) => {
    onTabChange?.(tab);
    if (activeTabProp === undefined) {
      setActiveTabInternal(tab);
    }
  };
  const [config, setConfig] = useState<Config>(initialConfig);
  const gatewayPublicDraftRef = useRef<string | null>(
    resolveDraftPublicBase(initialConfig.gateway_public)
  );
  gatewayPublicDraftRef.current = resolveDraftPublicBase(config.gateway_public);
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
    "load" | "save" | "start" | "stop" | "restart" | null
  >(null);
  const [actionMessage, setActionMessage] = useState("");
  const [channelValidationError, setChannelValidationError] = useState<string | undefined>();
  const [activeChannelId, setActiveChannelIdState] = useState("");
  const dirtyChannelIdsRef = useRef<Set<string>>(new Set());
  const [localHealthStatus, setLocalHealthStatus] = useState<HealthUiStatus>("not_ready");
  const [localHealthMessage, setLocalHealthMessage] = useState("Gateway 未运行。");
  const [localHealthStatusCode, setLocalHealthStatusCode] = useState<number | null>(null);
  const [localHealthLoading, setLocalHealthLoading] = useState(false);
  const [publicHealthStatus, setPublicHealthStatus] = useState<HealthUiStatus>(
    "not_configured"
  );
  const [publicHealthMessage, setPublicHealthMessage] = useState(
    "Public Base URL 未配置。"
  );
  const [publicHealthStatusCode, setPublicHealthStatusCode] = useState<number | null>(null);
  const [publicHealthLoading, setPublicHealthLoading] = useState(false);
  const [feishuDiagnostics, setFeishuDiagnostics] =
    useState<FeishuDiagnostics | null>(null);
  const [feishuDiagnosticsLoading, setFeishuDiagnosticsLoading] = useState(false);
  const [dingtalkDiagnostics, setDingtalkDiagnostics] =
    useState<DingtalkDiagnostics | null>(null);
  const [dingtalkDiagnosticsLoading, setDingtalkDiagnosticsLoading] = useState(false);
  const [dingtalkRouteProbe, setDingtalkRouteProbe] =
    useState<DingtalkPublicRouteProbe | null>(null);
  const [dingtalkRouteLoading, setDingtalkRouteLoading] = useState(false);
  const [cliInstall, setCliInstall] = useState<CliInstallStatus | null>(null);
  const [cliBusy, setCliBusy] = useState(false);
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

  const setActiveChannelId = useCallback((channelId: string) => {
    if (!channelIdIsKnown(channelId)) {
      return;
    }
    setActiveChannelIdState(channelId);
    rememberChannelId(channelId);
  }, []);

  const syncHealthFromGatewayStatus = useCallback((
    status: GatewayStatus,
    syncPublicHealth = true,
  ) => {
    if (!status.running) {
      setLocalHealthStatus("not_ready");
      setLocalHealthMessage("Gateway 未运行，本地 Health 未就绪。");
      setLocalHealthStatusCode(null);
    } else if (status.health_ok) {
      setLocalHealthStatus("ok");
      setLocalHealthMessage("Gateway 运行中，本地 Health 正常。");
      setLocalHealthStatusCode(200);
    } else {
      setLocalHealthStatus("idle");
      setLocalHealthMessage("Gateway 已启动，尚未完成本地 Health 检测。");
      setLocalHealthStatusCode(null);
    }

    const health = status.public_health;
    if (!status.running) {
      setPublicHealthStatus(health?.configured ? "not_ready" : "not_configured");
      setPublicHealthMessage(
        health?.configured
          ? "Gateway 已停止，之前的公网 Health 结果已失效。"
          : "Public Base URL 未配置。"
      );
      setPublicHealthStatusCode(null);
      return;
    }
    if (!syncPublicHealth) {
      return;
    }
    if (!health?.configured) {
      setPublicHealthStatus("not_configured");
      setPublicHealthMessage("Public Base URL 未配置。");
      setPublicHealthStatusCode(null);
    } else if (health.ok && health.status_code === 200) {
      setPublicHealthStatus("ok");
      setPublicHealthMessage("公网 Health 正常。");
      setPublicHealthStatusCode(200);
    } else if (health.error_kind === "not_checked") {
      setPublicHealthStatus("idle");
      setPublicHealthMessage("公网 Health 尚未检测。");
      setPublicHealthStatusCode(null);
    } else {
      setPublicHealthStatus("error");
      setPublicHealthMessage(health.error ?? "公网 Health 异常。");
      setPublicHealthStatusCode(health.status_code ?? null);
    }
  }, []);

  const refreshGatewayStatus = useCallback(async (): Promise<GatewayStatus> => {
    const status = await invokeTauri<GatewayStatus>("gateway_status");
    const draftBase = gatewayPublicDraftRef.current;
    const statusBase = normalizePublicBaseUrl(status.public_webhook_base_url);
    const syncPublicHealth = !draftBase || draftBase === statusBase;
    setGatewayStatus((current) => syncPublicHealth
      ? status
      : {
          ...status,
          public_webhook_base_url: draftBase,
          public_health: current.public_health,
        });
    syncHealthFromGatewayStatus(status, syncPublicHealth);
    return status;
  }, [syncHealthFromGatewayStatus]);

  const refreshDingtalkDiagnostics = useCallback(async () => {
    const diagnostics = await invokeTauri<DingtalkDiagnostics>("dingtalk_diagnostics");
    setDingtalkDiagnostics(diagnostics);
    return diagnostics;
  }, []);

  const refreshFeishuDiagnostics = useCallback(async () => {
    const diagnostics = await invokeTauri<FeishuDiagnostics>("feishu_diagnostics");
    setFeishuDiagnostics(diagnostics);
    return diagnostics;
  }, []);

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
        if (!disposed) await refreshGatewayStatus();
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
  }, [activeTab, refreshGatewayStatus]);

  useEffect(() => {
    if (activeTab !== "channels" || activeChannelId !== "feishu") {
      return;
    }
    void refreshFeishuDiagnostics().catch(() => {
      // Explicit button actions surface Tauri/browser errors to the user.
    });
  }, [activeTab, activeChannelId, refreshFeishuDiagnostics]);

  useEffect(() => {
    if (activeTab !== "channels" || activeChannelId !== "dingtalk") {
      return;
    }
    void refreshDingtalkDiagnostics().catch(() => {
      // Explicit button actions surface Tauri/browser errors to the user.
    });
  }, [activeTab, activeChannelId, refreshDingtalkDiagnostics]);

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
      syncHealthFromGatewayStatus(nextGatewayStatus);
      dirtyChannelIdsRef.current.clear();
      setActiveChannelIdState((currentChannelId) => {
        if (channelIdIsKnown(currentChannelId)) {
          return currentChannelId;
        }
        const enabledChannels = nextGatewayStatus.enabled_channels.filter(channelIdIsKnown);
        const remembered = readRememberedChannelId();
        const rememberedEnabled = remembered && enabledChannels.includes(remembered)
          ? remembered
          : null;
        const firstEnabled = enabledChannels[0]
          ?? Object.entries(merged.channels).find(([, entry]) => entry?.enabled)?.[0];
        const nextChannelId = rememberedEnabled
          ?? (channelIdIsKnown(firstEnabled) ? firstEnabled : null)
          ?? remembered
          ?? "feishu";
        rememberChannelId(nextChannelId);
        return nextChannelId;
      });
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
    changedChannelIds = [...dirtyChannelIdsRef.current],
  ): Promise<boolean> => {
    const result = await invokeTauri<{ gateway_restarted: boolean }>("save_setup_config", {
      config: configToSave,
      validateAllChannels,
      activeChannelId: channelId,
      changedChannelIds,
    });
    for (const changedChannelId of changedChannelIds) {
      dirtyChannelIdsRef.current.delete(changedChannelId);
    }
    await refreshGatewayStatus();
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
      notifySetupConfigUpdated();
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
      notifySetupConfigUpdated();
      await invokeTauri<GatewayStatus>("start_gateway");
      const nextGatewayStatus = await refreshGatewayStatus();
      if (nextGatewayStatus.running) {
        const enabledChannels = formatEnabledChannels(nextGatewayStatus.enabled_channels);
        const msg = restarted
          ? `Workspace 已切换，网关已重启：${nextGatewayStatus.url}`
          : `网关已启动：${nextGatewayStatus.url}`;
        setActionMessage(`${msg}。已启用频道：${enabledChannels || "无"}`);
        if (onConfigSuccess && !embedded) {
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
        await refreshGatewayStatus();
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
      await saveSetupConfig(false, nextConfig, "lark", ["lark"]);
      notifySetupConfigUpdated();
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
      await invokeTauri<GatewayStatus>("stop_gateway");
      const nextGatewayStatus = await refreshGatewayStatus();
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
        await refreshGatewayStatus();
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
      notifySetupConfigUpdated();
      await invokeTauri<GatewayStatus>("restart_gateway");
      const nextGatewayStatus = await refreshGatewayStatus();
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
        await refreshGatewayStatus();
      } catch {
        // Keep the last known status.
      }
    } finally {
      setBusyAction(null);
    }
  };

  const handleTestGatewayHealth = async () => {
    setLocalHealthLoading(true);
    try {
      const result = await invokeTauri<LocalHealthResult>("test_gateway_health");
      setActionMessage(result.message);
      const status = await refreshGatewayStatus();
      if (!status.running) {
        setLocalHealthStatus("not_ready");
      } else {
        setLocalHealthStatus(result.ok ? "ok" : "error");
      }
      setLocalHealthMessage(result.message);
      setLocalHealthStatusCode(result.status_code ?? null);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setLocalHealthStatus("error");
      setLocalHealthMessage(message);
      setLocalHealthStatusCode(null);
      setActionMessage(`Gateway 健康检查失败：${message}`);
    } finally {
      setLocalHealthLoading(false);
    }
  };

  const handleTestGatewayPublicHealth = async () => {
    setPublicHealthLoading(true);
    try {
      const latestStatus = await refreshGatewayStatus();
      const currentInputBase = resolveDraftPublicBase(config.gateway_public);
      const baseUrl = currentInputBase
        ?? normalizePublicBaseUrl(latestStatus.public_webhook_base_url);
      if (!baseUrl) {
        setPublicHealthStatus("not_configured");
        setPublicHealthMessage("Public Base URL 未配置。");
        setPublicHealthStatusCode(null);
        setActionMessage("Public Base URL 未配置。");
        return;
      }
      const result = await invokeTauri<GatewayStatus["public_health"]>(
        "test_gateway_public_health",
        { baseUrl }
      );
      setPublicHealthStatus(result.ok && result.status_code === 200 ? "ok" : "error");
      setPublicHealthMessage(
        result.ok ? "公网 Health 正常。" : result.error ?? "公网 Health 异常。"
      );
      setPublicHealthStatusCode(result.status_code ?? null);
      setGatewayStatus((current) => ({
        ...current,
        public_webhook_base_url: result.base_url ?? baseUrl,
        public_health: result,
      }));
      setActionMessage(
        result.ok
          ? `公网 Health 检查通过（HTTP ${result.status_code ?? 200}）。`
          : `公网 Health 检查失败：${result.error ?? "未知错误"}`
      );
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setPublicHealthStatus("error");
      setPublicHealthMessage(message);
      setPublicHealthStatusCode(null);
      setActionMessage(`公网 Health 检查失败：${message}`);
    } finally {
      setPublicHealthLoading(false);
    }
  };

  const handleRunDingtalkDiagnostics = async () => {
    setDingtalkDiagnosticsLoading(true);
    try {
      await refreshGatewayStatus();
      const diagnostics = await refreshDingtalkDiagnostics();
      setActionMessage(
        diagnostics.next_steps.length === 0
          ? "钉钉本地诊断通过，建议继续测试公网路由。"
          : `钉钉诊断完成：发现 ${diagnostics.next_steps.length} 项待处理。`
      );
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setActionMessage(`钉钉诊断失败：${message}`);
    } finally {
      setDingtalkDiagnosticsLoading(false);
    }
  };

  const handleRunFeishuDiagnostics = async () => {
    setFeishuDiagnosticsLoading(true);
    try {
      await refreshGatewayStatus();
      const diagnostics = await refreshFeishuDiagnostics();
      setActionMessage(
        diagnostics.next_steps.length === 0
          ? "飞书诊断通过；公网连通性请使用 Public Health 检测。"
          : `飞书诊断完成：发现 ${diagnostics.next_steps.length} 项待处理。`
      );
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setActionMessage(`飞书诊断失败：${message}`);
    } finally {
      setFeishuDiagnosticsLoading(false);
    }
  };

  const handleTestFeishuPublicHealth = async () => {
    await handleTestGatewayPublicHealth();
    await refreshFeishuDiagnostics().catch(() => {
      // The shared Public Health result remains visible even if refresh fails.
    });
  };

  const handleTestDingtalkPublicRoute = async () => {
    const baseUrl = resolveDraftPublicBase(config.gateway_public)
      ?? normalizePublicBaseUrl(gatewayStatus.public_webhook_base_url);
    if (!baseUrl) {
      const result: DingtalkPublicRouteProbe = {
        configured: false,
        reachable: false,
        status_code: null,
        result_kind: "not_configured",
        message: "Public Base URL 未配置。",
      };
      setDingtalkRouteProbe(result);
      setActionMessage(result.message);
      return;
    }

    setDingtalkRouteLoading(true);
    try {
      const result = await invokeTauri<DingtalkPublicRouteProbe>(
        "test_dingtalk_public_route",
        { baseUrl }
      );
      setDingtalkRouteProbe(result);
      setActionMessage(result.message);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setDingtalkRouteProbe({
        configured: true,
        reachable: false,
        status_code: null,
        result_kind: "network_error",
        message,
      });
      setActionMessage(`钉钉公网路由检测失败：${message}`);
    } finally {
      setDingtalkRouteLoading(false);
    }
  };

  const copyGatewayUrl = (url: string | null | undefined, label: string) => {
    if (!url) {
      setActionMessage(`${label}尚未生成。`);
      return;
    }
    void writeClipboardText(url).then(
      () => setActionMessage(`${label}已复制。`),
      () => setActionMessage(`${label}复制失败，请手动复制。`),
    );
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
    : normalizePublicBaseUrl(config.gateway_public?.public_webhook_base_url)
      ?? normalizePublicBaseUrl(gatewayStatus.public_webhook_base_url);
  const callbackBase = draftPublicBase;
  const rawPublicBaseInput = config.gateway_public?.public_webhook_base_url?.trim() ?? "";
  const publicBaseContainsDingtalkPath =
    /\/api\/v1\/gateway\/dingtalk\/events\/?(?:[?#].*)?$/i.test(rawPublicBaseInput)
    || /\/webhook\/dingtalk\/?(?:[?#].*)?$/i.test(rawPublicBaseInput);
  const namedTunnelNameConfigured =
    Boolean(config.gateway_public?.named_tunnel_name?.trim());
  const namedTunnelConfigComplete =
    namedTunnelNameConfigured && Boolean(draftNamedTunnelHostname);
  const runtimeWebhookUrl = callbackBase ? `${callbackBase}/webhook/feishu` : null;
  const runtimeCardCallbackUrl = callbackBase
    ? `${callbackBase}/webhook/feishu/card`
    : null;
  const runtimeDingtalkCallbackUrl = callbackBase
    ? `${callbackBase}/api/v1/gateway/dingtalk/events`
    : null;
  const lastStartedLabel = gatewayStatus.last_started_at
    ? new Date(gatewayStatus.last_started_at * 1000).toLocaleString()
    : "尚未记录";
  const publicHealthCheckedLabel = gatewayStatus.public_health?.checked_at
    ? new Date(gatewayStatus.public_health.checked_at * 1000).toLocaleString()
    : "尚未检测";
  const enabledChannelLabel = formatEnabledChannels(gatewayStatus.enabled_channels);

  useEffect(() => {
    const statusBase = normalizePublicBaseUrl(gatewayStatus.public_webhook_base_url);
    if (!draftPublicBase) {
      setPublicHealthStatus("not_configured");
      setPublicHealthMessage("Public Base URL 未配置。");
      setPublicHealthStatusCode(null);
    } else if (draftPublicBase !== statusBase) {
      setPublicHealthStatus("idle");
      setPublicHealthMessage("Public Base URL 已修改，等待重新检测。");
      setPublicHealthStatusCode(null);
    }
  }, [draftPublicBase, gatewayStatus.public_webhook_base_url]);

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
          </div>
        );
      case "providers":
        return (
          <ProviderConfigForm
            value={config.providers}
            onChange={handleProvidersChange}
            defaultProvider={config.default_provider ?? ""}
            defaultModel={config.default_model ?? ""}
            onDefaultChange={(providerId, model) =>
              setConfig({
                ...config,
                default_provider: providerId,
                default_model: model,
              })
            }
          />
        );
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
                  {publicBaseContainsDingtalkPath ? (
                    <small className="gateway-public-error">
                      请只填写公网根地址；保存时会移除钉钉回调路径。
                    </small>
                  ) : null}
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
                <div>
                  <span>本地 Health</span>
                  <strong>
                    {localHealthLoading
                      ? "检测中…"
                      : localHealthStatus === "ok"
                        ? "正常"
                        : localHealthStatus === "error"
                          ? "异常"
                          : gatewayStatus.running
                            ? "未检测"
                            : "未运行"}
                    {localHealthStatusCode ? ` · HTTP ${localHealthStatusCode}` : ""}
                  </strong>
                </div>
                <div>
                  <span>公网 Health</span>
                  <strong>
                    {publicHealthLoading
                      ? "检测中…"
                      : publicHealthStatus === "not_configured"
                        ? "未配置"
                      : publicHealthStatus === "ok"
                          ? "正常"
                          : publicHealthStatus === "not_ready"
                            ? "未运行"
                          : publicHealthStatus === "error"
                            ? "异常"
                            : "未检测"}
                    {publicHealthStatusCode ? ` · HTTP ${publicHealthStatusCode}` : ""}
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
                已启用频道：{enabledChannelLabel || "无"}
                {gatewayStatus.store_path ? ` · Store：${gatewayStatus.store_path}` : ""}
                {` · cloudflared path：${gatewayStatus.cloudflared_configured ? "已配置" : "未配置"}`}
                {` · cloudflared found：${gatewayStatus.cloudflared_found ? "true" : "false"}`}
              </div>
              <div className="gateway-runtime-health-actions">
                <button
                  type="button"
                  className="setup-btn setup-btn--secondary"
                  onClick={handleTestGatewayPublicHealth}
                  disabled={publicHealthLoading || busyAction !== null}
                >
                  {publicHealthLoading ? "检测中…" : "测试公网 Health"}
                </button>
                {publicHealthMessage ? (
                  <span className="gateway-public-error">
                    公网检测：{publicHealthMessage}
                  </span>
                ) : null}
                {localHealthMessage ? (
                  <span className="gateway-public-error">
                    本地检测：{localHealthMessage}
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
              onChange={(channels) => {
                if (activeChannelId) {
                  dirtyChannelIdsRef.current.add(activeChannelId);
                }
                setConfig({ ...config, channels });
              }}
              validationError={channelValidationError}
              onValidationChange={setChannelValidationError}
              selectedChannelId={activeChannelId}
              onSelectedChannelChange={setActiveChannelId}
              enabledChannelIds={gatewayStatus.enabled_channels}
              publicBaseUrl={draftPublicBase ?? undefined}
              gatewayUrl={gatewayStatus.running ? gatewayStatus.url : undefined}
              onHealthCheck={async () => {
                setLocalHealthLoading(true);
                try {
                  const result = await invokeTauri<LocalHealthResult>("test_gateway_health");
                  setLocalHealthStatus(result.ok ? "ok" : "error");
                  setLocalHealthMessage(result.message);
                  setLocalHealthStatusCode(result.status_code ?? null);
                  return result;
                } catch (error) {
                  const message = error instanceof Error ? error.message : String(error);
                  setLocalHealthStatus("error");
                  setLocalHealthMessage(message);
                  setLocalHealthStatusCode(null);
                  throw error;
                } finally {
                  setLocalHealthLoading(false);
                }
              }}
              onCopyWebhookUrl={(url) => {
                void writeClipboardText(url).then(
                  () => setActionMessage("Webhook 地址已复制。"),
                  () => setActionMessage("Webhook 地址复制失败，请手动复制。"),
                );
              }}
            />
            {activeChannelId === "feishu" ? (
              <section className="setup-section gateway-runtime-panel">
                <div className="section-heading gateway-runtime-heading">
                  <div>
                    <h2>飞书诊断</h2>
                    <div className="section-subtitle">
                      只读检查配置、Gateway、Store、Retry worker 与 Public Health，不模拟飞书签名请求。
                    </div>
                  </div>
                  <button
                    type="button"
                    className="setup-btn setup-btn--secondary"
                    onClick={() => void handleRunFeishuDiagnostics()}
                    disabled={feishuDiagnosticsLoading}
                  >
                    {feishuDiagnosticsLoading ? "诊断中…" : "刷新飞书诊断"}
                  </button>
                </div>

                <div className="gateway-runtime-url-list">
                  <div>
                    <span>普通事件回调</span>
                    <code>{runtimeWebhookUrl || "Public Base URL 未配置"}</code>
                    <button
                      type="button"
                      className="setup-btn setup-btn--secondary"
                      onClick={() => copyGatewayUrl(runtimeWebhookUrl, "飞书普通事件回调 URL")}
                      disabled={!runtimeWebhookUrl}
                    >
                      复制
                    </button>
                  </div>
                  <div>
                    <span>卡片交互回调</span>
                    <code>{runtimeCardCallbackUrl || "Public Base URL 未配置"}</code>
                    <button
                      type="button"
                      className="setup-btn setup-btn--secondary"
                      onClick={() => copyGatewayUrl(runtimeCardCallbackUrl, "飞书卡片交互回调 URL")}
                      disabled={!runtimeCardCallbackUrl}
                    >
                      复制
                    </button>
                  </div>
                </div>

                {feishuDiagnostics ? (
                  <div className="gateway-runtime-grid">
                    <div><span>Feishu</span><strong>{feishuDiagnostics.feishu_enabled ? "enabled" : "disabled"}</strong></div>
                    <div><span>App ID</span><strong>{feishuDiagnostics.app_id_present ? "present" : "missing"}</strong></div>
                    <div><span>App Secret</span><strong>{feishuDiagnostics.app_secret_present ? "present" : "missing"}</strong></div>
                    <div><span>Verification Token</span><strong>{feishuDiagnostics.verification_token_present ? "present" : "missing"}</strong></div>
                    <div><span>Encrypt Key</span><strong>{feishuDiagnostics.encrypt_key_present ? "present" : "missing"}</strong></div>
                    <div><span>Security</span><strong>{feishuDiagnostics.security_mode}</strong></div>
                    <div><span>Outbound</span><strong>{feishuDiagnostics.outbound_mode}</strong></div>
                    <div><span>Gateway</span><strong>{feishuDiagnostics.local_gateway_running ? "running" : "stopped"}</strong></div>
                    <div><span>Local Health</span><strong>{feishuDiagnostics.local_health}</strong></div>
                    <div><span>Public Base URL</span><strong>{feishuDiagnostics.public_base_url_present ? "present" : "missing"}</strong></div>
                    <div>
                      <span>Public Health</span>
                      <strong>
                        {feishuDiagnostics.public_health}
                        {feishuDiagnostics.public_health_status_code
                          ? ` · HTTP ${feishuDiagnostics.public_health_status_code}`
                          : ""}
                      </strong>
                    </div>
                    <div><span>Store</span><strong>{feishuDiagnostics.store_opened ? "opened" : "not_opened"}</strong></div>
                    <div><span>Retry worker</span><strong>{feishuDiagnostics.retry_worker_started ? "started" : "not_started"}</strong></div>
                  </div>
                ) : (
                  <p className="setup-action-hint">正在读取飞书诊断状态…</p>
                )}

                <div className="gateway-runtime-health-actions">
                  <button
                    type="button"
                    className="setup-btn setup-btn--secondary"
                    onClick={() => void handleTestFeishuPublicHealth()}
                    disabled={publicHealthLoading || !draftPublicBase}
                  >
                    {publicHealthLoading ? "检测中…" : "测试 Public Health"}
                  </button>
                  <span className={
                    publicHealthStatus === "ok"
                      ? "gateway-runtime-info"
                      : publicHealthStatus === "error"
                        ? "gateway-public-error"
                        : "setup-action-hint"
                  }>
                    {publicHealthMessage}
                    {publicHealthStatusCode ? `（HTTP ${publicHealthStatusCode}）` : ""}
                  </span>
                </div>

                {(config.gateway_public?.mode === "quick_tunnel"
                  || feishuDiagnostics?.quick_tunnel) ? (
                  <div className="gateway-runtime-warning">
                    Quick Tunnel 地址可能变化；重启 cloudflared 后，需要更新飞书后台两个回调地址。
                  </div>
                ) : null}
                {feishuDiagnostics?.next_steps.length ? (
                  <div className="gateway-runtime-warning">
                    <strong>建议下一步</strong>
                    <ul>
                      {feishuDiagnostics.next_steps.map((step) => (
                        <li key={step}>{step}</li>
                      ))}
                    </ul>
                  </div>
                ) : null}
              </section>
            ) : null}
            {activeChannelId === "dingtalk" ? (
              <section className="setup-section gateway-runtime-panel">
                <div className="section-heading gateway-runtime-heading">
                  <div>
                    <h2>钉钉诊断</h2>
                    <div className="section-subtitle">
                      只读检查本地配置、Gateway、worker 与公网路由，不调用钉钉平台 API。
                    </div>
                  </div>
                  <button
                    type="button"
                    className="setup-btn setup-btn--secondary"
                    onClick={() => void handleRunDingtalkDiagnostics()}
                    disabled={dingtalkDiagnosticsLoading}
                  >
                    {dingtalkDiagnosticsLoading ? "诊断中…" : "刷新钉钉诊断"}
                  </button>
                </div>

                <div className="gateway-runtime-url-list">
                  <div>
                    <span>钉钉消息接收地址</span>
                    <code>{runtimeDingtalkCallbackUrl || "Public Base URL 未配置"}</code>
                    <button
                      type="button"
                      className="setup-btn setup-btn--secondary"
                      onClick={() => copyGatewayUrl(
                        runtimeDingtalkCallbackUrl,
                        "钉钉消息接收地址"
                      )}
                      disabled={!runtimeDingtalkCallbackUrl}
                    >
                      复制
                    </button>
                  </div>
                </div>

                {dingtalkDiagnostics ? (
                  <div className="gateway-runtime-grid">
                    <div>
                      <span>DingTalk</span>
                      <strong>{dingtalkDiagnostics.dingtalk_enabled ? "enabled" : "disabled"}</strong>
                    </div>
                    <div><span>App Key</span><strong>{dingtalkDiagnostics.app_key_present ? "present" : "missing"}</strong></div>
                    <div><span>App Secret</span><strong>{dingtalkDiagnostics.app_secret_present ? "present" : "missing"}</strong></div>
                    <div><span>RobotCode</span><strong>{dingtalkDiagnostics.robot_code_present ? "present" : "missing"}</strong></div>
                    <div><span>Gateway</span><strong>{dingtalkDiagnostics.local_gateway_running ? "running" : "stopped"}</strong></div>
                    <div><span>Local Health</span><strong>{dingtalkDiagnostics.local_health}</strong></div>
                    <div><span>Public Base URL</span><strong>{dingtalkDiagnostics.public_base_url_present ? "present" : "missing"}</strong></div>
                    <div>
                      <span>Public Health</span>
                      <strong>
                        {dingtalkDiagnostics.public_health}
                        {dingtalkDiagnostics.public_health_status_code
                          ? ` · HTTP ${dingtalkDiagnostics.public_health_status_code}`
                          : ""}
                      </strong>
                    </div>
                    <div><span>Worker</span><strong>{dingtalkDiagnostics.worker_started ? "started" : "not_started"}</strong></div>
                    <div><span>Outbound</span><strong>{dingtalkDiagnostics.outbound_mode || "missing"}</strong></div>
                    <div><span>Webhook Path</span><code>{dingtalkDiagnostics.webhook_path}</code></div>
                  </div>
                ) : (
                  <p className="setup-action-hint">正在读取钉钉诊断状态…</p>
                )}

                <div className="gateway-runtime-health-actions">
                  <button
                    type="button"
                    className="setup-btn setup-btn--secondary"
                    onClick={() => void handleTestDingtalkPublicRoute()}
                    disabled={dingtalkRouteLoading}
                  >
                    {dingtalkRouteLoading ? "检测中…" : "测试公网路由"}
                  </button>
                  {dingtalkRouteProbe ? (
                    <span className={
                      dingtalkRouteProbe.reachable
                        ? "gateway-runtime-info"
                        : "gateway-public-error"
                    }>
                      {dingtalkRouteProbe.message}
                      {dingtalkRouteProbe.status_code
                        ? `（HTTP ${dingtalkRouteProbe.status_code}）`
                        : ""}
                    </span>
                  ) : null}
                </div>

                {(config.gateway_public?.mode === "quick_tunnel"
                  || dingtalkDiagnostics?.quick_tunnel) ? (
                  <div className="gateway-runtime-warning">
                    Quick Tunnel 地址可能变化；重启 cloudflared 后，需要更新钉钉后台回调地址并重新发布应用。
                  </div>
                ) : null}
                {dingtalkDiagnostics?.public_health === "failed" ? (
                  <div className="gateway-runtime-warning">
                    本地 Gateway 正常但公网检测失败时，请优先检查 cloudflared 是否运行以及公网地址是否过期。
                  </div>
                ) : null}
                {dingtalkDiagnostics?.next_steps.length ? (
                  <div className="gateway-runtime-warning">
                    <strong>建议下一步</strong>
                    <ul>
                      {dingtalkDiagnostics.next_steps.map((step) => (
                        <li key={step}>{step}</li>
                      ))}
                    </ul>
                  </div>
                ) : null}
              </section>
            ) : null}
          </>
        );
      case "skills":
        return (
          <SkillsConfigForm
            config={config.skills || { open_skills_enabled: true }}
            onChange={(skills) => setConfig({ ...config, skills })}
          />
        );
      case "persona":
        return (
          <div className="setup-section">
            <div className="section-heading">
              <div>
                <h2>行为与人格</h2>
                <div className="section-subtitle">管理名称、工作区、提示词与上下文策略。</div>
              </div>
            </div>
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
          disabled={busyAction !== null || localHealthLoading}
        >
          {localHealthLoading ? "检测中…" : "测试本地 Health"}
        </button>
      </div>
      {actionMessage ? <p className="setup-action-hint">{actionMessage}</p> : null}
      {larkBlockerActions}
    </div>
  );

  const setupPreviewBlock = (
    <div className="setup-preview-wrap">
      <div className={`setup-preview${previewCollapsed ? " setup-preview--collapsed" : ""}`}>
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
                void writeClipboardText(jsonPreview).then(
                  () => setActionMessage("配置已复制到剪贴板。"),
                  () => setActionMessage("配置复制失败，请手动复制。"),
                );
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
      ) : dialogMode ? null : (
        <header className="setup-embed-hero">
          <h1 className="setup-embed-title">{meta.title}</h1>
          <p className="setup-embed-sub">{meta.subtitle}</p>
        </header>
      )}

      {renderTabContent()}

      {embedded ? gatewayActions : null}

      {dialogMode ? null : setupPreviewBlock}
    </>
  );

  if (embedded) {
    return (
      <div
        className={`setup-page setup-page--embedded${
          dialogMode ? " setup-page--dialog" : ""
        }`}
      >
        {setupMainInner}
      </div>
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
              <UiIcon name={tab.icon} size={17} />
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
            disabled={busyAction !== null || localHealthLoading}
          >
            {localHealthLoading ? "检测中…" : "测试本地 Health"}
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
