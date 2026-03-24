/**
 * API Key Management Hook (Story 8.3)
 *
 * Provides React hooks for managing gateway API keys.
 */

import { useState, useCallback, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type {
  ApiKeyInfo,
  ApiKeyCreated,
  CreateApiKeyRequest,
  ApiKeyPermission,
} from '../types/api-key';

interface UseApiKeysState {
  keys: ApiKeyInfo[];
  loading: boolean;
  error: string | null;
  initialized: boolean;
}

interface UseApiKeysReturn extends UseApiKeysState {
  initStore: () => Promise<void>;
  createKey: (request: CreateApiKeyRequest) => Promise<ApiKeyCreated>;
  revokeKey: (id: number) => Promise<boolean>;
  deleteKey: (id: number) => Promise<boolean>;
  getKey: (id: number) => Promise<ApiKeyInfo | null>;
  refreshKeys: () => Promise<void>;
}

/**
 * Hook for managing gateway API keys
 */
export function useApiKeys(): UseApiKeysReturn {
  const [state, setState] = useState<UseApiKeysState>({
    keys: [],
    loading: false,
    error: null,
    initialized: false,
  });

  const initStore = useCallback(async () => {
    try {
      setState((prev) => ({ ...prev, loading: true, error: null }));
      await invoke('init_api_key_store');
      setState((prev) => ({ ...prev, initialized: true, loading: false }));
    } catch (err) {
      setState((prev) => ({
        ...prev,
        error: err instanceof Error ? err.message : String(err),
        loading: false,
      }));
    }
  }, []);

  const refreshKeys = useCallback(async () => {
    try {
      setState((prev) => ({ ...prev, loading: true, error: null }));
      const keys = await invoke<ApiKeyInfo[]>('list_api_keys');
      setState((prev) => ({ ...prev, keys, loading: false }));
    } catch (err) {
      setState((prev) => ({
        ...prev,
        error: err instanceof Error ? err.message : String(err),
        loading: false,
      }));
    }
  }, []);

  const createKey = useCallback(async (request: CreateApiKeyRequest): Promise<ApiKeyCreated> => {
    try {
      setState((prev) => ({ ...prev, loading: true, error: null }));
      const result = await invoke<ApiKeyCreated>('create_api_key', {
        name: request.name,
        permissions: request.permissions,
        expiresInDays: request.expires_in_days,
      });
      // Refresh keys after creation
      await refreshKeys();
      setState((prev) => ({ ...prev, loading: false }));
      return result;
    } catch (err) {
      const error = err instanceof Error ? err.message : String(err);
      setState((prev) => ({ ...prev, error, loading: false }));
      throw new Error(error);
    }
  }, [refreshKeys]);

  const revokeKey = useCallback(async (id: number): Promise<boolean> => {
    try {
      setState((prev) => ({ ...prev, loading: true, error: null }));
      const result = await invoke<boolean>('revoke_api_key', { id });
      await refreshKeys();
      setState((prev) => ({ ...prev, loading: false }));
      return result;
    } catch (err) {
      const error = err instanceof Error ? err.message : String(err);
      setState((prev) => ({ ...prev, error, loading: false }));
      throw new Error(error);
    }
  }, [refreshKeys]);

  const deleteKey = useCallback(async (id: number): Promise<boolean> => {
    try {
      setState((prev) => ({ ...prev, loading: true, error: null }));
      const result = await invoke<boolean>('delete_gateway_api_key', { id });
      await refreshKeys();
      setState((prev) => ({ ...prev, loading: false }));
      return result;
    } catch (err) {
      const error = err instanceof Error ? err.message : String(err);
      setState((prev) => ({ ...prev, error, loading: false }));
      throw new Error(error);
    }
  }, [refreshKeys]);

  const getKey = useCallback(async (id: number): Promise<ApiKeyInfo | null> => {
    try {
      const result = await invoke<ApiKeyInfo | null>('get_gateway_api_key', { id });
      return result;
    } catch (err) {
      const error = err instanceof Error ? err.message : String(err);
      setState((prev) => ({ ...prev, error }));
      return null;
    }
  }, []);

  // Auto-initialize and load keys on mount
  useEffect(() => {
    if (!state.initialized) {
      initStore().then(() => refreshKeys());
    }
  }, [state.initialized, initStore, refreshKeys]);

  return {
    ...state,
    initStore,
    createKey,
    revokeKey,
    deleteKey,
    getKey,
    refreshKeys,
  };
}

interface UseCreateApiKeyDialogReturn {
  isOpen: boolean;
  name: string;
  permissions: ApiKeyPermission[];
  expiresInDays: number | null;
  createdKey: ApiKeyCreated | null;
  error: string | null;
  isCreating: boolean;
  open: () => void;
  close: () => void;
  setName: (name: string) => void;
  togglePermission: (perm: ApiKeyPermission) => void;
  setExpiresInDays: (days: number | null) => void;
  create: () => Promise<void>;
  clearCreatedKey: () => void;
}

/**
 * Hook for managing the create API key dialog state
 */
export function useCreateApiKeyDialog(
  onCreate: (key: ApiKeyCreated) => void
): UseCreateApiKeyDialogReturn {
  const [isOpen, setIsOpen] = useState(false);
  const [name, setName] = useState('');
  const [permissions, setPermissions] = useState<ApiKeyPermission[]>(['read']);
  const [expiresInDays, setExpiresInDays] = useState<number | null>(null);
  const [createdKey, setCreatedKey] = useState<ApiKeyCreated | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isCreating, setIsCreating] = useState(false);

  const open = () => {
    setIsOpen(true);
    setName('');
    setPermissions(['read']);
    setExpiresInDays(null);
    setError(null);
    setCreatedKey(null);
  };

  const close = () => {
    setIsOpen(false);
    // Don't clear createdKey immediately - let user copy it first
  };

  const togglePermission = (perm: ApiKeyPermission) => {
    setPermissions((prev) => {
      if (prev.includes(perm)) {
        // Can't remove if it's the only permission
        if (prev.length === 1) return prev;
        return prev.filter((p) => p !== perm);
      } else {
        return [...prev, perm];
      }
    });
  };

  const create = async () => {
    if (!name.trim()) {
      setError('请输入 API Key 名称');
      return;
    }

    if (permissions.length === 0) {
      setError('请至少选择一个权限');
      return;
    }

    try {
      setIsCreating(true);
      setError(null);

      const result = await invoke<ApiKeyCreated>('create_api_key', {
        name: name.trim(),
        permissions,
        expiresInDays,
      });

      setCreatedKey(result);
      onCreate(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsCreating(false);
    }
  };

  const clearCreatedKey = () => {
    setCreatedKey(null);
  };

  return {
    isOpen,
    name,
    permissions,
    expiresInDays,
    createdKey,
    error,
    isCreating,
    open,
    close,
    setName,
    togglePermission,
    setExpiresInDays,
    create,
    clearCreatedKey,
  };
}