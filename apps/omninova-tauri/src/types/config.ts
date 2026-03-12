export interface RobotConfig {
  drive: DriveConfig;
  camera: CameraConfig;
  audio: AudioConfig;
  sensors: SensorsConfig;
  safety: SafetyConfig;
}

export interface DriveConfig {
  backend: string;
  ros2_topic?: string;
  serial_port?: string;
  max_speed: number;
  max_rotation: number;
}

export interface CameraConfig {
  device: string;
  width: number;
  height: number;
  vision_model: string;
  ollama_url: string;
}

export interface AudioConfig {
  mic_device: string;
  speaker_device: string;
  whisper_model: string;
  whisper_path?: string;
  piper_path?: string;
  piper_voice?: string;
}

export interface SensorsConfig {
  lidar_port?: string;
  lidar_type: string;
  motion_pins: number[];
  ultrasonic_pins?: [number, number];
}

export interface SafetyConfig {
  min_obstacle_distance: number;
  slow_zone_multiplier: number;
  approach_speed_limit: number;
  estop_pin?: number;
  bump_sensor_pins: number[];
}

export interface ProviderConfig {
  id: string;
  name: string;
  type: string;
  api_key_env?: string;
  base_url?: string;
  models: string[];
  enabled: boolean;
}

export interface ProviderPreset extends ProviderConfig {
  category: "cloud" | "local";
}

export interface ChannelEntryConfig {
  enabled: boolean;
  token?: string;
  token_env?: string;
  app_id?: string;
  app_secret?: string;
  verification_token?: string;
  encrypt_key?: string;
  webhook_url?: string;
}

export interface ChannelsConfig {
  telegram?: ChannelEntryConfig;
  discord?: ChannelEntryConfig;
  slack?: ChannelEntryConfig;
  whatsapp?: ChannelEntryConfig;
  wechat?: ChannelEntryConfig;
  feishu?: ChannelEntryConfig;
  lark?: ChannelEntryConfig;
  dingtalk?: ChannelEntryConfig;
  matrix?: ChannelEntryConfig;
  email?: ChannelEntryConfig;
  msteams?: ChannelEntryConfig;
  irc?: ChannelEntryConfig;
  webhook?: ChannelEntryConfig;
}

export interface ChannelField {
  key: keyof ChannelEntryConfig;
  label: string;
  placeholder: string;
  type?: "text" | "password";
}

export interface ChannelPreset {
  id: keyof ChannelsConfig;
  name: string;
  category: "im" | "webhook" | "other";
  tokenEnvHint: string;
  fields: ChannelField[];
  isDefault?: boolean;
}

const COMMON_TOKEN_FIELDS: ChannelField[] = [
  { key: "token", label: "Token / Secret", placeholder: "直接填写 token", type: "password" },
  { key: "token_env", label: "Token 环境变量", placeholder: "", type: "text" },
];

export const CHANNEL_PRESETS: ChannelPreset[] = [
  {
    id: "feishu",
    name: "飞书 Feishu",
    category: "im",
    tokenEnvHint: "FEISHU_APP_SECRET",
    isDefault: true,
    fields: [
      { key: "app_id", label: "App ID", placeholder: "cli_xxxxxxxxxx", type: "text" },
      { key: "app_secret", label: "App Secret", placeholder: "飞书应用密钥", type: "password" },
      { key: "verification_token", label: "Verification Token", placeholder: "事件订阅验证 Token", type: "text" },
      { key: "encrypt_key", label: "Encrypt Key", placeholder: "事件加密密钥（可选）", type: "password" },
      { key: "webhook_url", label: "Webhook 回调地址", placeholder: "https://your-domain/webhook/feishu", type: "text" },
      { key: "token_env", label: "Secret 环境变量", placeholder: "FEISHU_APP_SECRET", type: "text" },
    ],
  },
  {
    id: "telegram",
    name: "Telegram",
    category: "im",
    tokenEnvHint: "TELEGRAM_BOT_TOKEN",
    fields: [...COMMON_TOKEN_FIELDS],
  },
  {
    id: "discord",
    name: "Discord",
    category: "im",
    tokenEnvHint: "DISCORD_BOT_TOKEN",
    fields: [...COMMON_TOKEN_FIELDS],
  },
  {
    id: "slack",
    name: "Slack",
    category: "im",
    tokenEnvHint: "SLACK_BOT_TOKEN",
    fields: [...COMMON_TOKEN_FIELDS],
  },
  {
    id: "whatsapp",
    name: "WhatsApp",
    category: "im",
    tokenEnvHint: "WHATSAPP_TOKEN",
    fields: [...COMMON_TOKEN_FIELDS],
  },
  {
    id: "wechat",
    name: "WeChat / 企业微信",
    category: "im",
    tokenEnvHint: "WECHAT_TOKEN",
    fields: [
      { key: "app_id", label: "Corp ID / App ID", placeholder: "企业 ID 或应用 ID", type: "text" },
      { key: "app_secret", label: "App Secret", placeholder: "应用密钥", type: "password" },
      { key: "token", label: "Token", placeholder: "回调 Token", type: "password" },
      { key: "encrypt_key", label: "EncodingAESKey", placeholder: "消息加解密密钥", type: "password" },
      { key: "token_env", label: "Secret 环境变量", placeholder: "WECHAT_TOKEN", type: "text" },
    ],
  },
  {
    id: "lark",
    name: "Lark (国际版飞书)",
    category: "im",
    tokenEnvHint: "LARK_APP_SECRET",
    fields: [
      { key: "app_id", label: "App ID", placeholder: "cli_xxxxxxxxxx", type: "text" },
      { key: "app_secret", label: "App Secret", placeholder: "Lark 应用密钥", type: "password" },
      { key: "verification_token", label: "Verification Token", placeholder: "事件验证 Token", type: "text" },
      { key: "token_env", label: "Secret 环境变量", placeholder: "LARK_APP_SECRET", type: "text" },
    ],
  },
  {
    id: "dingtalk",
    name: "钉钉 DingTalk",
    category: "im",
    tokenEnvHint: "DINGTALK_TOKEN",
    fields: [
      { key: "app_id", label: "App Key", placeholder: "钉钉应用 AppKey", type: "text" },
      { key: "app_secret", label: "App Secret", placeholder: "钉钉应用 AppSecret", type: "password" },
      { key: "token", label: "签名密钥", placeholder: "自定义机器人签名密钥", type: "password" },
      { key: "token_env", label: "Secret 环境变量", placeholder: "DINGTALK_TOKEN", type: "text" },
    ],
  },
  {
    id: "matrix",
    name: "Matrix",
    category: "im",
    tokenEnvHint: "MATRIX_ACCESS_TOKEN",
    fields: [...COMMON_TOKEN_FIELDS],
  },
  {
    id: "msteams",
    name: "Microsoft Teams",
    category: "im",
    tokenEnvHint: "MSTEAMS_TOKEN",
    fields: [...COMMON_TOKEN_FIELDS],
  },
  {
    id: "email",
    name: "Email",
    category: "other",
    tokenEnvHint: "EMAIL_SMTP_PASSWORD",
    fields: [...COMMON_TOKEN_FIELDS],
  },
  {
    id: "irc",
    name: "IRC",
    category: "other",
    tokenEnvHint: "IRC_PASSWORD",
    fields: [...COMMON_TOKEN_FIELDS],
  },
  {
    id: "webhook",
    name: "通用 Webhook",
    category: "webhook",
    tokenEnvHint: "WEBHOOK_SECRET",
    fields: [
      { key: "token", label: "Signing Secret", placeholder: "Webhook 签名密钥", type: "password" },
      { key: "webhook_url", label: "回调地址", placeholder: "https://your-domain/webhook", type: "text" },
      { key: "token_env", label: "Secret 环境变量", placeholder: "WEBHOOK_SECRET", type: "text" },
    ],
  },
];

export interface SkillsConfig {
  open_skills_enabled: boolean;
  open_skills_dir?: string;
  prompt_injection_mode?: string;
}

export interface AgentPersonaConfig {
  name: string;
  system_prompt?: string;
  compact_context?: boolean;
  max_tool_iterations?: number;
  max_history_messages?: number;
}

// Main configuration interface
export interface Config {
  api_key?: string;
  api_url?: string;
  default_provider?: string;
  default_model?: string;
  default_temperature?: number;
  workspace_dir?: string;
  omninoval_gateway_url?: string;
  omninoval_config_dir?: string;
  provider_api?: string;
  robot: RobotConfig;
  providers: ProviderConfig[];
  channels: ChannelsConfig;
  skills?: SkillsConfig;
  agent?: AgentPersonaConfig;
}

export interface GatewayStatus {
  running: boolean;
  url: string;
  last_error?: string | null;
}

export type ChannelKindValue =
  | "cli"
  | "web"
  | "webchat"
  | "telegram"
  | "discord"
  | "slack"
  | "whatsapp"
  | "google_chat"
  | "signal"
  | "bluebubbles"
  | "imessage"
  | "irc"
  | "msteams"
  | "matrix"
  | "feishu"
  | "line"
  | "mattermost"
  | "nextcloud_talk"
  | "nostr"
  | "synology_chat"
  | "tlon"
  | "twitch"
  | "wechat"
  | "zalo"
  | "zalo_personal"
  | "lark"
  | "dingtalk"
  | "email"
  | "webhook";

export interface RouteDecision {
  agent_name: string;
  provider?: string | null;
  model?: string | null;
}

export interface GatewayInboundResponse {
  route: RouteDecision;
  reply: string;
}

export interface GatewayHealth {
  ok: boolean;
  provider: string;
  provider_healthy: boolean;
  memory_healthy: boolean;
}

export interface ProviderHealthSummary {
  id: string;
  name: string;
  enabled: boolean;
  is_default: boolean;
  model?: string | null;
  base_url?: string | null;
  healthy?: boolean | null;
}

export interface SessionTreeNode {
  session_key?: string | null;
  channel?: string | null;
  session_id?: string | null;
  parent_session_key?: string | null;
  parent_agent_id?: string | null;
  agent_name?: string | null;
  spawn_depth: number;
  updated_at: number;
  source: string;
}

export interface SessionTreeStats {
  unique_agents: number;
  unique_parent_agents: number;
  max_spawn_depth: number;
  min_updated_at: number;
  max_updated_at: number;
}

export interface SessionTreeResponse {
  sessions: SessionTreeNode[];
  active_children_by_parent: Record<string, number>;
  total_before_filter: number;
  total_after_filter: number;
  returned: number;
  offset: number;
  limit?: number | null;
  has_more: boolean;
  next_offset?: number | null;
  prev_offset?: number | null;
  next_cursor?: number | null;
  prev_cursor?: number | null;
  source_counts_after_filter: Record<string, number>;
  stats_after_filter: SessionTreeStats;
}

export const DEFAULT_ROBOT_CONFIG: RobotConfig = {
  drive: {
    backend: 'mock',
    max_speed: 0.5,
    max_rotation: 1.0,
  },
  camera: {
    device: '/dev/video0',
    width: 640,
    height: 480,
    vision_model: 'moondream',
    ollama_url: 'http://localhost:11434',
  },
  audio: {
    mic_device: 'default',
    speaker_device: 'default',
    whisper_model: 'base',
  },
  sensors: {
    lidar_type: 'mock',
    motion_pins: [],
  },
  safety: {
    min_obstacle_distance: 0.3,
    slow_zone_multiplier: 3.0,
    approach_speed_limit: 0.3,
    bump_sensor_pins: [],
  },
};

export const PROVIDER_PRESETS: ProviderPreset[] = [
  {
    id: 'anthropic',
    name: 'Anthropic',
    type: 'anthropic',
    api_key_env: 'ANTHROPIC_API_KEY',
    models: [
      'claude-sonnet-4-20250514',
      'claude-opus-4-20250514',
      'claude-3-7-sonnet-latest',
      'claude-3-5-sonnet-latest',
      'claude-3-5-haiku-latest',
    ],
    enabled: false,
    category: 'cloud',
  },
  {
    id: 'openai',
    name: 'OpenAI',
    type: 'openai',
    api_key_env: 'OPENAI_API_KEY',
    models: [
      'gpt-5',
      'gpt-5-mini',
      'gpt-4.1',
      'gpt-4.1-mini',
      'gpt-4o',
      'gpt-4o-mini',
      'o3',
      'o4-mini',
    ],
    enabled: false,
    category: 'cloud',
  },
  {
    id: 'gemini',
    name: 'Google Gemini',
    type: 'gemini',
    api_key_env: 'GEMINI_API_KEY',
    base_url: 'https://generativelanguage.googleapis.com',
    models: [
      'gemini-2.5-pro',
      'gemini-2.5-flash',
      'gemini-2.0-flash',
      'gemini-1.5-pro',
      'gemini-1.5-flash',
    ],
    enabled: false,
    category: 'cloud',
  },
  {
    id: 'deepseek',
    name: 'DeepSeek',
    type: 'deepseek',
    api_key_env: 'DEEPSEEK_API_KEY',
    base_url: 'https://api.deepseek.com',
    models: [
      'deepseek-chat',
      'deepseek-reasoner',
      'deepseek-v3',
      'deepseek-r1',
    ],
    enabled: false,
    category: 'cloud',
  },
  {
    id: 'qwen',
    name: 'Qwen / DashScope',
    type: 'qwen',
    api_key_env: 'DASHSCOPE_API_KEY',
    base_url: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
    models: [
      'qwen-max',
      'qwen-plus',
      'qwen-turbo',
      'qwen2.5-72b-instruct',
      'qwen2.5-coder-32b-instruct',
    ],
    enabled: false,
    category: 'cloud',
  },
  {
    id: 'moonshot',
    name: 'Moonshot',
    type: 'moonshot',
    api_key_env: 'MOONSHOT_API_KEY',
    base_url: 'https://api.moonshot.cn/v1',
    models: [
      'moonshot-v1-8k',
      'moonshot-v1-32k',
      'moonshot-v1-128k',
      'kimi-k2-0711-preview',
    ],
    enabled: false,
    category: 'cloud',
  },
  {
    id: 'xai',
    name: 'xAI',
    type: 'xai',
    api_key_env: 'XAI_API_KEY',
    base_url: 'https://api.x.ai/v1',
    models: [
      'grok-4',
      'grok-3',
      'grok-3-mini',
      'grok-beta',
    ],
    enabled: false,
    category: 'cloud',
  },
  {
    id: 'mistral',
    name: 'Mistral',
    type: 'mistral',
    api_key_env: 'MISTRAL_API_KEY',
    base_url: 'https://api.mistral.ai/v1',
    models: [
      'mistral-large-latest',
      'mistral-medium-latest',
      'ministral-8b-latest',
      'codestral-latest',
      'open-mixtral-8x22b',
    ],
    enabled: false,
    category: 'cloud',
  },
  {
    id: 'groq',
    name: 'Groq',
    type: 'groq',
    api_key_env: 'GROQ_API_KEY',
    base_url: 'https://api.groq.com/openai/v1',
    models: [
      'llama-3.3-70b-versatile',
      'llama-3.1-8b-instant',
      'mixtral-8x7b-32768',
      'gemma2-9b-it',
      'deepseek-r1-distill-llama-70b',
    ],
    enabled: false,
    category: 'cloud',
  },
  {
    id: 'openrouter',
    name: 'OpenRouter',
    type: 'openrouter',
    api_key_env: 'OPENROUTER_API_KEY',
    base_url: 'https://openrouter.ai/api/v1',
    models: [
      'openai/gpt-5',
      'openai/gpt-4.1',
      'anthropic/claude-sonnet-4',
      'google/gemini-2.5-pro',
      'deepseek/deepseek-r1',
      'meta-llama/llama-3.3-70b-instruct',
    ],
    enabled: false,
    category: 'cloud',
  },
  {
    id: 'ollama',
    name: 'Ollama (Local)',
    type: 'ollama',
    base_url: 'http://localhost:11434',
    models: [
      'llama3.2',
      'llama3.1',
      'qwen2.5',
      'qwen2.5-coder',
      'deepseek-r1',
      'mistral',
      'gemma3',
      'codellama',
    ],
    enabled: false,
    category: 'local',
  },
  {
    id: 'lmstudio',
    name: 'LM Studio (Local)',
    type: 'lmstudio',
    base_url: 'http://localhost:1234/v1',
    models: [
      'qwen2.5-coder-7b-instruct',
      'qwen2.5-coder-32b-instruct',
      'llama-3.1-8b-instruct',
      'llama-3.3-70b-instruct',
      'mistral-small-3.1',
      'gemma-3-12b-it',
    ],
    enabled: false,
    category: 'local',
  },
];

export const cloneProviderPreset = (id: string): ProviderConfig | undefined => {
  const preset = PROVIDER_PRESETS.find((item) => item.id === id);

  if (!preset) {
    return undefined;
  }

  return {
    id: preset.id,
    name: preset.name,
    type: preset.type,
    api_key_env: preset.api_key_env,
    base_url: preset.base_url,
    models: [...preset.models],
    enabled: preset.enabled,
  };
};

export const DEFAULT_PROVIDERS: ProviderConfig[] = [];
