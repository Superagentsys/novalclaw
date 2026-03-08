import { invoke } from "@tauri-apps/api/core";

const isTauriEnvironment = () =>
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

  return invoke<T>(command, args);
}
