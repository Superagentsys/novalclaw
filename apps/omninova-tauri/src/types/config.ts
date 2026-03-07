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

export interface AppConfig {
  api_key?: string;
  api_url?: string;
  default_provider?: string;
  default_model?: string;
  workspace_dir: string;
  omninoval_gateway_url?: string;
  omninoval_config_dir?: string;
  robot?: RobotConfig;
  providers: ProviderConfig[];
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
