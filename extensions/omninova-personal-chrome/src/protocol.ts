export const PROTOCOL_VERSION = 1;
export const APPLICATION_MAX_MESSAGE_BYTES = 1_048_576;
export const NATIVE_HOST_NAME = "com.omninova.browser_host";

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
