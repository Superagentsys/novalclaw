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

export interface AppConfig {
  api_key?: string;
  api_url?: string;
  default_provider?: string;
  default_model?: string;
  workspace_dir: string;
  openclaw_gateway_url?: string;
  openclaw_config_dir?: string;
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

export const DEFAULT_PROVIDERS: ProviderConfig[] = [
  {
    id: 'anthropic',
    name: 'Anthropic',
    type: 'anthropic',
    api_key_env: 'ANTHROPIC_API_KEY',
    models: ['claude-3-5-sonnet-20241022', 'claude-3-opus-20240229'],
    enabled: false,
  },
  {
    id: 'openai',
    name: 'OpenAI',
    type: 'openai',
    api_key_env: 'OPENAI_API_KEY',
    models: ['gpt-4o', 'gpt-4-turbo', 'gpt-3.5-turbo'],
    enabled: false,
  },
  {
    id: 'ollama',
    name: 'Ollama (Local)',
    type: 'ollama',
    base_url: 'http://localhost:11434',
    models: ['llama3.2', 'llama3.1', 'codellama'],
    enabled: false,
  },
];
