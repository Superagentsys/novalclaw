/**
 * Keyring types for secure API key storage in OmniNova Claw
 *
 * These types correspond to the Rust types in omninova-core/src/security/keyring.rs
 */

/**
 * Key reference for stored secrets
 * Format: keyring://category/name/key_type
 */
export interface KeyReference {
  /** Category (e.g., "providers", "channels") */
  category: string;
  /** Provider or service name */
  name: string;
  /** Key type (e.g., "api_key", "bot_token") */
  key_type: string;
}

/**
 * API key status information
 */
export interface ApiKeyStatus {
  /** Whether the key exists in storage */
  exists: boolean;
  /** Whether there's a reference in config */
  hasReference: boolean;
  /** Last accessed timestamp (if available) */
  lastAccessed?: number;
}

/**
 * Keyring store type
 */
export type KeyringStoreType = 'os-keyring' | 'encrypted-file';

/**
 * Parse a key reference URL string
 * Format: keyring://category/name/key_type
 */
export function parseKeyReference(url: string): KeyReference | null {
  const prefix = 'keyring://';
  if (!url.startsWith(prefix)) {
    return null;
  }

  const path = url.slice(prefix.length);
  const parts = path.split('/');

  if (parts.length !== 3) {
    return null;
  }

  return {
    category: parts[0],
    name: parts[1],
    key_type: parts[2],
  };
}

/**
 * Create a key reference URL string
 */
export function createKeyReference(ref: KeyReference): string {
  return `keyring://${ref.category}/${ref.name}/${ref.key_type}`;
}

/**
 * Create a provider API key reference
 */
export function createProviderKeyReference(providerName: string): KeyReference {
  return {
    category: 'providers',
    name: providerName,
    key_type: 'api_key',
  };
}

// ============================================================================
// Tauri Command Invocations
// ============================================================================

import { invokeTauri } from '../utils/tauri';

/**
 * Initialize the keyring service
 * @returns The store type being used ('os-keyring' or 'encrypted-file')
 */
export async function initKeyringService(): Promise<KeyringStoreType> {
  return invokeTauri<KeyringStoreType>('init_keyring_service');
}

/**
 * Save an API key for a provider
 * @param provider Provider name (e.g., 'openai', 'anthropic')
 * @param apiKey The API key to store
 * @returns The key reference URL
 */
export async function saveApiKey(provider: string, apiKey: string): Promise<string> {
  return invokeTauri<string>('save_api_key', { provider, apiKey });
}

/**
 * Get an API key for a provider
 * @param provider Provider name
 * @returns The stored API key
 */
export async function getApiKey(provider: string): Promise<string> {
  return invokeTauri<string>('get_api_key', { provider });
}

/**
 * Delete an API key for a provider
 * @param provider Provider name
 */
export async function deleteApiKey(provider: string): Promise<void> {
  return invokeTauri<void>('delete_api_key', { provider });
}

/**
 * Check if an API key exists for a provider
 * @param provider Provider name
 * @returns Whether the key exists
 */
export async function apiKeyExists(provider: string): Promise<boolean> {
  return invokeTauri<boolean>('api_key_exists', { provider });
}

/**
 * Get the type of keyring storage being used
 * @returns The store type
 */
export async function getKeyringStoreType(): Promise<KeyringStoreType> {
  return invokeTauri<KeyringStoreType>('get_keyring_store_type');
}