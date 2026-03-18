/**
 * Provider types for OmniNova Claw
 *
 * These types correspond to the Rust types in omninova-core/src/providers/config.rs
 */

/**
 * Provider types matching Rust ProviderType enum
 */
export type ProviderType =
  | 'openai'
  | 'anthropic'
  | 'gemini'
  | 'ollama'
  | 'lmstudio'
  | 'llamacpp'
  | 'vllm'
  | 'sglang'
  | 'openrouter'
  | 'together'
  | 'fireworks'
  | 'novita'
  | 'deepseek'
  | 'qwen'
  | 'moonshot'
  | 'doubao'
  | 'qianfan'
  | 'glm'
  | 'minimax'
  | 'groq'
  | 'xai'
  | 'mistral'
  | 'perplexity'
  | 'cohere'
  | 'nvidia'
  | 'cloudflare'
  | 'mock'
  | 'custom';

/**
 * Provider configuration matching Rust ProviderConfig struct
 */
export interface ProviderConfig {
  id: string;
  name: string;
  providerType: ProviderType;
  apiKeyRef?: string;
  baseUrl?: string;
  defaultModel?: string;
  settings?: string;
  isDefault: boolean;
  createdAt: number;
  updatedAt: number;
}

/**
 * New provider configuration for creation
 */
export interface NewProviderConfig {
  name: string;
  providerType: ProviderType;
  apiKey?: string; // Will be stored in keychain
  baseUrl?: string;
  defaultModel?: string;
  settings?: string;
  isDefault: boolean;
}

/**
 * Provider configuration update
 */
export interface ProviderConfigUpdate {
  name?: string;
  apiKey?: string; // Will update keychain if provided
  baseUrl?: string;
  defaultModel?: string;
  settings?: string;
  isDefault?: boolean;
}

/**
 * Provider connection test result
 * Matches backend response from test_provider_connection
 */
export interface ProviderTestResult {
  provider_type: string;
  name: string;
  model?: string;
  healthy: boolean;
}

/**
 * Connection status type
 */
export type ConnectionStatus = 'untested' | 'testing' | 'connected' | 'failed';

/**
 * Provider with runtime status
 */
export interface ProviderWithStatus extends ProviderConfig {
  connectionStatus: ConnectionStatus;
  lastTested?: number;
  keyExists: boolean;
  storeType?: 'os-keyring' | 'encrypted-file';
}

/**
 * Agent provider validation result
 * Returned by validate_provider_for_agent command
 */
export interface AgentProviderValidation {
  isValid: boolean;
  errors: string[];
  warnings: string[];
  suggestions: string[];
}

/**
 * Provider category for UI grouping
 */
export type ProviderCategory = 'cloud' | 'local' | 'custom';

/**
 * Provider preset for UI display
 */
export interface ProviderPreset {
  id: ProviderType;
  name: string;
  category: ProviderCategory;
  requiresApiKey: boolean;
  defaultBaseUrl?: string;
  popularModels: string[];
}

// ============================================================================
// Tauri Command Invocations
// ============================================================================

import { invokeTauri } from '../utils/tauri';

/**
 * List all providers
 */
export async function listProviders(): Promise<ProviderConfig[]> {
  const json = await invokeTauri<string>('get_provider_configs');
  return JSON.parse(json);
}

/**
 * Create a new provider
 */
export async function createProvider(
  config: NewProviderConfig
): Promise<ProviderConfig> {
  const json = await invokeTauri<string>('create_provider_config', {
    configJson: JSON.stringify(config),
  });
  return JSON.parse(json);
}

/**
 * Update a provider
 */
export async function updateProvider(
  id: string,
  update: ProviderConfigUpdate
): Promise<ProviderConfig> {
  const json = await invokeTauri<string>('update_provider_config', {
    id,
    updatesJson: JSON.stringify(update),
  });
  return JSON.parse(json);
}

/**
 * Delete a provider
 */
export async function deleteProvider(id: string): Promise<void> {
  return invokeTauri<void>('delete_provider_config', { id });
}

/**
 * Set a provider as default
 */
export async function setDefaultProvider(id: string): Promise<void> {
  return invokeTauri<void>('set_default_provider_config', { id });
}

/**
 * Test a provider connection
 * Note: Backend expects full config JSON, not just ID
 */
export async function testProviderConnection(
  config: ProviderConfig
): Promise<ProviderTestResult> {
  const json = await invokeTauri<string>('test_provider_connection', {
    configJson: JSON.stringify(config),
  });
  return JSON.parse(json);
}

// ============================================================================
// Agent Provider Assignment Commands (Story 3.7)
// ============================================================================

/**
 * Set the default provider for an agent
 */
export async function setAgentDefaultProvider(
  agentUuid: string,
  providerId: string
): Promise<void> {
  await invokeTauri<string>('set_agent_default_provider', {
    agentUuid,
    providerId,
  });
}

/**
 * Get the provider configuration for an agent
 * Returns the agent's default provider if set, otherwise the global default.
 * Returns null if no provider is configured.
 */
export async function getAgentProvider(
  agentUuid: string
): Promise<ProviderConfig | null> {
  const json = await invokeTauri<string | null>('get_agent_provider', {
    agentUuid,
  });
  return json ? JSON.parse(json) : null;
}

/**
 * Validate a provider for agent assignment
 * Checks if a provider is suitable for use with an agent.
 */
export async function validateProviderForAgent(
  providerId: string
): Promise<AgentProviderValidation> {
  const json = await invokeTauri<string>('validate_provider_for_agent', {
    providerId,
  });
  return JSON.parse(json);
}

// ============================================================================
// Provider Presets
// ============================================================================

/**
 * Provider presets for UI display
 * Based on PROVIDER_PRESETS from config.ts
 */
export const PROVIDER_PRESETS: ProviderPreset[] = [
  // Cloud providers
  {
    id: 'anthropic',
    name: 'Anthropic',
    category: 'cloud',
    requiresApiKey: true,
    popularModels: ['claude-sonnet-4-20250514', 'claude-opus-4-20250514', 'claude-3-5-sonnet-20241022'],
  },
  {
    id: 'openai',
    name: 'OpenAI',
    category: 'cloud',
    requiresApiKey: true,
    popularModels: ['gpt-4.1', 'gpt-4.1-mini', 'o3', 'o4-mini'],
  },
  {
    id: 'gemini',
    name: 'Google Gemini',
    category: 'cloud',
    requiresApiKey: true,
    popularModels: ['gemini-2.5-pro-preview-06-05', 'gemini-2.0-flash'],
  },
  {
    id: 'deepseek',
    name: 'DeepSeek',
    category: 'cloud',
    requiresApiKey: true,
    popularModels: ['deepseek-chat', 'deepseek-reasoner'],
  },
  {
    id: 'qwen',
    name: '通义千问 (Qwen)',
    category: 'cloud',
    requiresApiKey: true,
    popularModels: ['qwen-turbo', 'qwen-plus', 'qwen-max'],
  },
  {
    id: 'moonshot',
    name: 'Moonshot (月之暗面)',
    category: 'cloud',
    requiresApiKey: true,
    popularModels: ['moonshot-v1-8k', 'moonshot-v1-32k', 'moonshot-v1-128k'],
  },
  {
    id: 'xai',
    name: 'xAI (Grok)',
    category: 'cloud',
    requiresApiKey: true,
    popularModels: ['grok-3', 'grok-3-fast'],
  },
  {
    id: 'mistral',
    name: 'Mistral AI',
    category: 'cloud',
    requiresApiKey: true,
    popularModels: ['mistral-large-latest', 'codestral-latest'],
  },
  {
    id: 'groq',
    name: 'Groq',
    category: 'cloud',
    requiresApiKey: true,
    popularModels: ['llama-3.3-70b-versatile', 'llama-3.1-8b-instant'],
  },
  {
    id: 'openrouter',
    name: 'OpenRouter',
    category: 'cloud',
    requiresApiKey: true,
    defaultBaseUrl: 'https://openrouter.ai/api/v1',
    popularModels: ['anthropic/claude-sonnet-4', 'openai/gpt-4.1'],
  },
  // Local providers
  {
    id: 'ollama',
    name: 'Ollama',
    category: 'local',
    requiresApiKey: false,
    defaultBaseUrl: 'http://localhost:11434',
    popularModels: ['llama3.2', 'llama3.1', 'qwen2.5', 'deepseek-r1'],
  },
  {
    id: 'lmstudio',
    name: 'LM Studio',
    category: 'local',
    requiresApiKey: false,
    defaultBaseUrl: 'http://localhost:1234/v1',
    popularModels: [],
  },
];

/**
 * Get provider preset by type
 */
export function getProviderPreset(type: ProviderType): ProviderPreset | undefined {
  return PROVIDER_PRESETS.find((p) => p.id === type);
}

/**
 * Get providers by category
 */
export function getProvidersByCategory(
  category: ProviderCategory
): ProviderPreset[] {
  return PROVIDER_PRESETS.filter((p) => p.category === category);
}