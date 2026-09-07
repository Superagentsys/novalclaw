export const PROTOCOL_VERSION = 1;
export const APPLICATION_MAX_MESSAGE_BYTES = 1_048_576;
export const NATIVE_HOST_NAME = "com.omninova.browser_host";

export const MAX_OBSERVE_ELEMENTS = 80;
export const MAX_ELEMENT_TEXT = 400;

export type ConnectionStatus =
  | "disconnected"
  | "connecting"
  | "connected"
  | "protocol_mismatch";

export interface TransportRequest {
  protocol_version: number;
  request_id: string;
  session_id?: string;
  operation: string;
  payload: Record<string, unknown>;
}

export interface TransportResponse {
  protocol_version: number;
  request_id: string;
  ok: boolean;
  payload?: Record<string, unknown>;
  error?: { code: string; message: string };
}

export const BACKEND_OPERATIONS = [
  "tab_get",
  "tab_list_authorized",
  "revoke_authorization",
  "attach_session",
  "detach_session",
  "session_health",
  "observe",
  "act",
  "navigate",
  "screenshot",
] as const;

export function nativeHostName(): string {
  return NATIVE_HOST_NAME;
}

export function buildHello(requestId: string, extensionVersion: string): TransportRequest {
  return {
    protocol_version: PROTOCOL_VERSION,
    request_id: requestId,
    session_id: "",
    operation: "hello",
    payload: {
      protocol_version: PROTOCOL_VERSION,
      extension_version: extensionVersion,
    },
  };
}

export function buildPing(requestId: string, echo: string): TransportRequest {
  return {
    protocol_version: PROTOCOL_VERSION,
    request_id: requestId,
    operation: "ping",
    payload: { echo },
  };
}

export function isProtocolMismatch(message: unknown): boolean {
  if (!message || typeof message !== "object") {
    return false;
  }
  const error = (message as { error?: { code?: string } }).error;
  return error?.code === "ProtocolMismatch";
}

export function shouldReconnect(status: ConnectionStatus): boolean {
  return status !== "protocol_mismatch";
}

export function isDesktopRequest(message: unknown): message is TransportRequest {
  return Boolean(
    message &&
      typeof message === "object" &&
      "operation" in message &&
      !("ok" in message)
  );
}

export function isRestrictedUrl(url: string): boolean {
  const lower = url.toLowerCase();
  return (
    lower.startsWith("chrome://") ||
    lower.startsWith("chrome-extension://") ||
    lower.startsWith("edge://") ||
    lower.startsWith("about:") ||
    lower.includes("chromewebstore.google.com")
  );
}

export function originPermissionPattern(url: string): string | undefined {
  try {
    const parsed = new URL(url);
    if (parsed.protocol !== "http:" && parsed.protocol !== "https:") return undefined;
    return `${parsed.protocol}//${parsed.host}/*`;
  } catch {
    return undefined;
  }
}

export function redactInputValue(inputType: string, value: string): string | undefined {
  if (inputType.toLowerCase() === "password") {
    return undefined;
  }
  return value;
}
