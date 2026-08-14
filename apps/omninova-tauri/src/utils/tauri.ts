import { invoke } from "@tauri-apps/api/core";

const VERBOSE_INVOKE_LOG_COMMANDS = new Set([
  "process_inbound_message_streaming",
  "route_inbound_message",
]);

export const isTauriEnvironment = () =>
  typeof window !== "undefined" &&
  Boolean(
    (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__
  );

export function gatewayOrigin(): string {
  const env = (import.meta as { env?: Record<string, string> }).env ?? {};
  const fromEnv = env.VITE_GATEWAY_URL;
  if (fromEnv && fromEnv.trim()) {
    return fromEnv.replace(/\/$/, "");
  }
  return "";
}

export async function invokeTauri<T>(
  command: string,
  args?: Record<string, unknown>
): Promise<T> {
  if (!isTauriEnvironment()) {
    return invokeGateway<T>(command, args);
  }

  const shouldLogPayload = import.meta.env.DEV && VERBOSE_INVOKE_LOG_COMMANDS.has(command);
  if (shouldLogPayload) {
    console.log("[invokeTauri:start]", command, args);
  }
  try {
    const result = await invoke<T>(command, args);
    if (shouldLogPayload) {
      console.log("[invokeTauri:resolved]", command, result);
    }
    return result;
  } catch (error) {
    if (import.meta.env.DEV) {
      console.error("[invokeTauri:error]", command, error);
    }
    throw error;
  }
}

async function invokeGateway<T>(
  command: string,
  args?: Record<string, unknown>
): Promise<T> {
  const shouldLogPayload = import.meta.env.DEV && VERBOSE_INVOKE_LOG_COMMANDS.has(command);
  if (shouldLogPayload) {
    console.log("[invokeGateway:start]", command, args);
  }
  const response = await fetch(`${gatewayOrigin()}/api/v1/invoke`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ command, args: args ?? {} }),
  });
  let body: { ok?: boolean; result?: T; error?: string } = {};
  try {
    body = (await response.json()) as { ok?: boolean; result?: T; error?: string };
  } catch {
    throw new Error(`Gateway invoke ${command} failed (${response.status})`);
  }
  if (!response.ok || body.ok === false) {
    throw new Error(body.error || `Gateway invoke ${command} failed (${response.status})`);
  }
  if (shouldLogPayload) {
    console.log("[invokeGateway:resolved]", command, body.result);
  }
  return body.result as T;
}
