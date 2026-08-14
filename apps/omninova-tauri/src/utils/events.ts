import { gatewayOrigin, isTauriEnvironment } from "./tauri";

export type AgentEventHandler<T> = (event: { payload: T }) => void;

/**
 * Subscribe to live agent-run events.
 * Desktop uses the Tauri event bus; the browser UI uses Gateway SSE.
 */
export async function listenAgentRunEvents<T = unknown>(
  _event: string,
  handler: AgentEventHandler<T>
): Promise<() => void> {
  if (isTauriEnvironment()) {
    const { listen } = await import("@tauri-apps/api/event");
    return listen<T>("agent-run-event", handler);
  }

  const source = new EventSource(`${gatewayOrigin()}/api/v1/events`);
  source.onmessage = (message) => {
    try {
      handler({ payload: JSON.parse(message.data) as T });
    } catch {
      // ignore malformed frames
    }
  };
  source.onerror = () => {
    // EventSource reconnects automatically; keep the subscription alive.
  };
  return () => source.close();
}
