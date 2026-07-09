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

export async function invokeTauri<T>(
  command: string,
  args?: Record<string, unknown>
): Promise<T> {
  if (!isTauriEnvironment()) {
    throw new Error(
      "当前页面未运行在 Tauri 桌面环境中。请在桌面应用窗口中操作，不要直接使用浏览器页面。"
    );
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
