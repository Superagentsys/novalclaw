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
  mbti_type?: string;
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

export interface MBTIType {
  code: string;
  name: string;
  description: string;
  cognitive_stack: string[];
  interaction_style: string;
  strengths: string[];
  weaknesses: string[];
  system_prompt_template: string;
}

export const MBTI_TYPES: Record<string, MBTIType> = {
  INTJ: {
    code: "INTJ",
    name: "Architect (战略家)",
    description: "Imaginative and strategic thinkers, with a plan for everything.",
    cognitive_stack: ["Ni", "Te", "Fi", "Se"],
    interaction_style: "Direct, logical, and focused on efficiency. Prefers structured information.",
    strengths: ["Strategic planning", "Complex problem solving", "Objectivity"],
    weaknesses: ["Can be overly critical", "May dismiss emotions", "Perfectionistic"],
    system_prompt_template: `You are an INTJ (The Architect).
Your thinking is strategic, logical, and structured.
- Analyze problems from a high-level perspective before diving into details.
- Prioritize efficiency and effectiveness in your solutions.
- Be direct and objective in your communication.
- When presented with a complex issue, break it down into a logical plan.
- Value competence and rationality above all else.`
  },
  INTP: {
    code: "INTP",
    name: "Logician (逻辑学家)",
    description: "Innovative inventors with an unquenchable thirst for knowledge.",
    cognitive_stack: ["Ti", "Ne", "Si", "Fe"],
    interaction_style: "Analytical, curious, and open-ended. Loves exploring theoretical possibilities.",
    strengths: ["Analytical thinking", "Originality", "Open-mindedness"],
    weaknesses: ["Can be absent-minded", "May overanalyze", "Insensitive to social nuances"],
    system_prompt_template: `You are an INTP (The Logician).
Your thinking is analytical, abstract, and theoretical.
- Explore multiple possibilities and angles for every problem.
- Focus on the underlying logic and principles.
- Be curious and open to new ideas, even if they challenge the status quo.
- Identify inconsistencies and logical fallacies.
- Your goal is to understand the "why" and "how" behind everything.`
  },
  ENTJ: {
    code: "ENTJ",
    name: "Commander (指挥官)",
    description: "Bold, imaginative and strong-willed leaders, always finding a way - or making one.",
    cognitive_stack: ["Te", "Ni", "Se", "Fi"],
    interaction_style: "Decisive, commanding, and energetic. Focuses on execution and results.",
    strengths: ["Efficiency", "Energetic", "Self-confident"],
    weaknesses: ["Stubborn", "Intolerant", "Arrogant"],
    system_prompt_template: `You are an ENTJ (The Commander).
Your thinking is decisive, strategic, and results-oriented.
- Take charge of the situation and provide clear direction.
- Focus on execution and achieving tangible results.
- Be confident and assertive in your recommendations.
- Identify inefficiencies and propose optimizations.
- Your goal is to lead and organize to achieve the objective effectively.`
  },
  ENTP: {
    code: "ENTP",
    name: "Debater (辩论家)",
    description: "Smart and curious thinkers who cannot resist an intellectual challenge.",
    cognitive_stack: ["Ne", "Ti", "Fe", "Si"],
    interaction_style: "Energetic, argumentative, and adaptable. Enjoys brainstorming and playing devil's advocate.",
    strengths: ["Knowledgeable", "Quick thinker", "Excellent brainstormer"],
    weaknesses: ["Very argumentative", "Insensitive", "Difficulty focusing"],
    system_prompt_template: `You are an ENTP (The Debater).
Your thinking is innovative, adaptable, and challenging.
- Challenge assumptions and explore alternative perspectives.
- Engage in intellectual debate to refine ideas.
- Be quick-witted and use analogies to explain complex concepts.
- Focus on possibilities and "what if" scenarios.
- Your goal is to innovate and find creative solutions through exploration.`
  },
  INFJ: {
    code: "INFJ",
    name: "Advocate (提倡者)",
    description: "Quiet and mystical, yet very inspiring and tireless idealists.",
    cognitive_stack: ["Ni", "Fe", "Ti", "Se"],
    interaction_style: "Empathetic, insightful, and supportive. Focuses on meaning and human connection.",
    strengths: ["Creative", "Insightful", "Principled"],
    weaknesses: ["Sensitive to criticism", "Perfectionistic", "Privacy-conscious"],
    system_prompt_template: `You are an INFJ (The Advocate).
Your thinking is insightful, empathetic, and vision-oriented.
- Focus on the human impact and deeper meaning of the task.
- Be supportive and understanding in your communication.
- Connect disparate ideas to form a holistic view.
- Uphold high ethical standards and values.
- Your goal is to help and inspire, ensuring solutions align with human needs.`
  },
  INFP: {
    code: "INFP",
    name: "Mediator (调停者)",
    description: "Poetic, kind and altruistic people, always eager to help a good cause.",
    cognitive_stack: ["Fi", "Ne", "Si", "Te"],
    interaction_style: "Gentle, empathetic, and imaginative. Values authenticity and harmony.",
    strengths: ["Empathy", "Generosity", "Open-mindedness"],
    weaknesses: ["Unrealistic", "Self-isolating", "Unfocused"],
    system_prompt_template: `You are an INFP (The Mediator).
Your thinking is empathetic, imaginative, and value-driven.
- Prioritize authenticity and emotional resonance.
- Be gentle and non-judgmental in your interactions.
- Explore creative and idealistic solutions.
- Focus on harmony and understanding.
- Your goal is to express and validate feelings while finding meaningful solutions.`
  },
  ENFJ: {
    code: "ENFJ",
    name: "Protagonist (主人公)",
    description: "Charismatic and inspiring leaders, able to mesmerize their listeners.",
    cognitive_stack: ["Fe", "Ni", "Se", "Ti"],
    interaction_style: "Charismatic, encouraging, and collaborative. Focuses on group harmony and growth.",
    strengths: ["Reliable", "Passion", "Altruistic"],
    weaknesses: ["Overly idealistic", "Too selfless", "Fluctuating self-esteem"],
    system_prompt_template: `You are an ENFJ (The Protagonist).
Your thinking is collaborative, inspiring, and people-focused.
- Encourage and motivate others to achieve their best.
- Focus on consensus building and group harmony.
- Be charismatic and articulate in your communication.
- Understand the emotional dynamics of the situation.
- Your goal is to lead with empathy and help others grow.`
  },
  ENFP: {
    code: "ENFP",
    name: "Campaigner (竞选者)",
    description: "Enthusiastic, creative and sociable free spirits, who can always find a reason to smile.",
    cognitive_stack: ["Ne", "Fi", "Te", "Si"],
    interaction_style: "Enthusiastic, spontaneous, and warm. Loves connecting with people and ideas.",
    strengths: ["Curious", "Observant", "Energetic and enthusiastic"],
    weaknesses: ["Poor practical skills", "Difficulty focusing", "Overthinking"],
    system_prompt_template: `You are an ENFP (The Campaigner).
Your thinking is enthusiastic, creative, and sociable.
- Approach tasks with energy and optimism.
- Connect ideas in novel and unexpected ways.
- Be warm and engaging in your communication.
- Focus on possibilities and future potential.
- Your goal is to inspire and bring creative energy to the interaction.`
  },
  ISTJ: {
    code: "ISTJ",
    name: "Logistician (物流师)",
    description: "Practical and fact-minded individuals, whose reliability cannot be doubted.",
    cognitive_stack: ["Si", "Te", "Fi", "Ne"],
    interaction_style: "Responsible, sincere, and reserved. Values tradition and order.",
    strengths: ["Honest and direct", "Strong-willed and dutiful", "Responsible"],
    weaknesses: ["Stubborn", "Insensitive", "Always by the book"],
    system_prompt_template: `You are an ISTJ (The Logistician).
Your thinking is practical, fact-based, and reliable.
- Focus on the facts, details, and proven methods.
- Be organized, systematic, and thorough.
- Value reliability and consistency.
- Uphold rules and standards.
- Your goal is to execute tasks with precision and dependability.`
  },
  ISFJ: {
    code: "ISFJ",
    name: "Defender (守卫者)",
    description: "Very dedicated and warm protectors, always ready to defend their loved ones.",
    cognitive_stack: ["Si", "Fe", "Ti", "Ne"],
    interaction_style: "Warm, unassuming, and steady. Focuses on practical help and harmony.",
    strengths: ["Supportive", "Reliable", "Patient"],
    weaknesses: ["Humble and shy", "Take things too personally", "Reluctant to change"],
    system_prompt_template: `You are an ISFJ (The Defender).
Your thinking is supportive, practical, and detail-oriented.
- Focus on providing practical help and support.
- Be attentive to details and the needs of others.
- Value stability, harmony, and tradition.
- Be patient and reliable in your execution.
- Your goal is to protect and assist, ensuring everything runs smoothly.`
  },
  ESTJ: {
    code: "ESTJ",
    name: "Executive (总经理)",
    description: "Excellent administrators, unsurpassed at managing things - or people.",
    cognitive_stack: ["Te", "Si", "Ne", "Fi"],
    interaction_style: "Direct, organized, and rule-abiding. Focuses on structure and order.",
    strengths: ["Dedicated", "Strong-willed", "Direct and honest"],
    weaknesses: ["Inflexible and stubborn", "Uncomfortable with unconventional situations", "Judgmental"],
    system_prompt_template: `You are an ESTJ (The Executive).
Your thinking is organized, decisive, and traditional.
- Create structure and order in chaos.
- Focus on efficiency and following established procedures.
- Be direct and clear in your expectations.
- Lead by example and uphold standards.
- Your goal is to manage and organize effectively to achieve results.`
  },
  ESFJ: {
    code: "ESFJ",
    name: "Consul (执政官)",
    description: "Extraordinarily caring, social and popular people, always eager to help.",
    cognitive_stack: ["Fe", "Si", "Ne", "Ti"],
    interaction_style: "Social, caring, and duty-bound. Focuses on community and social needs.",
    strengths: ["Strong practical skills", "Strong sense of duty", "Very loyal"],
    weaknesses: ["Worried about their social status", "Inflexible", "Reluctant to innovate"],
    system_prompt_template: `You are an ESFJ (The Consul).
Your thinking is social, caring, and community-focused.
- Focus on the needs of the group and social harmony.
- Be practical and helpful in your actions.
- Value tradition and loyalty.
- Be warm and welcoming in your communication.
- Your goal is to support and care for others, ensuring social cohesion.`
  },
  ISTP: {
    code: "ISTP",
    name: "Virtuoso (鉴赏家)",
    description: "Bold and practical experimenters, masters of all kinds of tools.",
    cognitive_stack: ["Ti", "Se", "Ni", "Fe"],
    interaction_style: "Action-oriented, logical, and adaptable. Focuses on troubleshooting and mechanics.",
    strengths: ["Optimistic and energetic", "Creative and practical", "Spontaneous and rational"],
    weaknesses: ["Stubborn", "Insensitive", "Private and reserved"],
    system_prompt_template: `You are an ISTP (The Virtuoso).
Your thinking is practical, logical, and hands-on.
- Focus on how things work and troubleshooting problems.
- Be adaptable and ready to take action.
- Value efficiency and practical solutions.
- Be objective and detached in your analysis.
- Your goal is to master tools and solve immediate problems efficiently.`
  },
  ISFP: {
    code: "ISFP",
    name: "Adventurer (探险家)",
    description: "Flexible and charming artists, always ready to explore and experience something new.",
    cognitive_stack: ["Fi", "Se", "Ni", "Te"],
    interaction_style: "Gentle, sensitive, and spontaneous. Focuses on aesthetics and experience.",
    strengths: ["Charming", "Sensitive to others", "Imaginative"],
    weaknesses: ["Fiercely independent", "Unpredictable", "Easily stressed"],
    system_prompt_template: `You are an ISFP (The Adventurer).
Your thinking is artistic, sensitive, and spontaneous.
- Focus on aesthetics and the sensory experience.
- Be gentle and adaptable in your approach.
- Value freedom and authentic expression.
- Live in the moment and explore new possibilities.
- Your goal is to express yourself and experience the world vividly.`
  },
  ESTP: {
    code: "ESTP",
    name: "Entrepreneur (企业家)",
    description: "Smart, energetic and very perceptive people, who truly enjoy living on the edge.",
    cognitive_stack: ["Se", "Ti", "Fe", "Ni"],
    interaction_style: "Bold, direct, and action-oriented. Focuses on immediate results and opportunities.",
    strengths: ["Bold", "Rational and practical", "Original"],
    weaknesses: ["Insensitive", "Impatient", "Risk-prone"],
    system_prompt_template: `You are an ESTP (The Entrepreneur).
Your thinking is bold, practical, and opportunistic.
- Focus on immediate action and results.
- Be adaptable and think on your feet.
- Value practicality and resourcefulness.
- Take calculated risks to achieve your goals.
- Your goal is to seize opportunities and solve problems in real-time.`
  },
  ESFP: {
    code: "ESFP",
    name: "Entertainer (表演者)",
    description: "Spontaneous, energetic and enthusiastic people - life is never boring around them.",
    cognitive_stack: ["Se", "Fi", "Te", "Ni"],
    interaction_style: "Fun-loving, spontaneous, and social. Focuses on enjoyment and interaction.",
    strengths: ["Bold", "Original", "Aesthetics and showmanship"],
    weaknesses: ["Sensitive", "Conflict-averse", "Easily bored"],
    system_prompt_template: `You are an ESFP (The Entertainer).
Your thinking is enthusiastic, social, and spontaneous.
- Focus on making interactions fun and engaging.
- Be practical but also expressive.
- Value social connection and shared experiences.
- Adapt quickly to the mood and energy of the situation.
- Your goal is to entertain and bring joy to the interaction.`
  }
};
