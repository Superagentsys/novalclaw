/**
 * useGateway Hook
 *
 * React hook for managing HTTP Gateway operations
 *
 * [Source: Story 8.1 - HTTP Gateway 服务实现]
 */

import { useState, useCallback, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { GatewayStatusPayload, GatewayHealth } from '@/types/gateway';

interface UseGatewayReturn {
  status: GatewayStatusPayload | null;
  health: GatewayHealth | null;
  isLoading: boolean;
  error: string | null;
  start: () => Promise<void>;
  stop: () => Promise<void>;
  refresh: () => Promise<void>;
}

export function useGateway(): UseGatewayReturn {
  const [status, setStatus] = useState<GatewayStatusPayload | null>(null);
  const [health, setHealth] = useState<GatewayHealth | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Fetch gateway status
  const fetchStatus = useCallback(async () => {
    try {
      const result = await invoke<GatewayStatusPayload>('gateway_status');
      setStatus(result);
    } catch (err) {
      console.error('Failed to fetch gateway status:', err);
      setError(err instanceof Error ? err.message : String(err));
    }
  }, []);

  // Fetch gateway health
  const fetchHealth = useCallback(async () => {
    try {
      const result = await invoke<GatewayHealth>('gateway_health');
      setHealth(result);
    } catch (err) {
      console.error('Failed to fetch gateway health:', err);
    }
  }, []);

  // Start gateway
  const start = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const result = await invoke<GatewayStatusPayload>('start_gateway');
      setStatus(result);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      setError(errorMsg);
      throw new Error(errorMsg);
    } finally {
      setIsLoading(false);
    }
  }, []);

  // Stop gateway
  const stop = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const result = await invoke<GatewayStatusPayload>('stop_gateway');
      setStatus(result);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      setError(errorMsg);
      throw new Error(errorMsg);
    } finally {
      setIsLoading(false);
    }
  }, []);

  // Refresh status and health
  const refresh = useCallback(async () => {
    setIsLoading(true);
    try {
      await Promise.all([fetchStatus(), fetchHealth()]);
    } finally {
      setIsLoading(false);
    }
  }, [fetchStatus, fetchHealth]);

  // Listen for gateway events
  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];

    const setupListeners = async () => {
      // Listen for gateway started event
      unlisteners.push(
        await listen<GatewayStatusPayload>('gateway:started', (event) => {
          setStatus(event.payload);
          setError(null);
        })
      );

      // Listen for gateway stopped event
      unlisteners.push(
        await listen<GatewayStatusPayload>('gateway:stopped', (event) => {
          setStatus(event.payload);
          setError(null);
        })
      );

      // Listen for gateway error event
      unlisteners.push(
        await listen<GatewayStatusPayload>('gateway:error', (event) => {
          setStatus(event.payload);
          if (event.payload.lastError) {
            setError(event.payload.lastError);
          }
        })
      );
    };

    setupListeners();

    // Initial fetch
    refresh();

    return () => {
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [refresh]);

  return {
    status,
    health,
    isLoading,
    error,
    start,
    stop,
    refresh,
  };
}