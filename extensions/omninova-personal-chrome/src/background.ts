import {
  ConnectionStatus,
  PROTOCOL_VERSION,
  buildHello,
  buildPing,
  isProtocolMismatch,
  nativeHostName,
} from "./protocol.js";

const RECONNECT_ALARM = "omninova-personal-chrome-reconnect";
const PING_ALARM = "omninova-personal-chrome-ping";

let port: chrome.runtime.Port | null = null;
let status: ConnectionStatus = "disconnected";
let reconnectAttempts = 0;

async function setStatus(next: ConnectionStatus): Promise<void> {
  status = next;
  await chrome.storage.local.set({
    transportStatus: next,
    nativeHostName: nativeHostName(),
    protocolVersion: PROTOCOL_VERSION,
  });
}

function send(message: unknown): void {
  if (!port) {
    return;
  }
  port.postMessage(message);
}

function connect(): void {
  if (port) {
    return;
  }
  void setStatus("connecting");
  try {
    port = chrome.runtime.connectNative(nativeHostName());
  } catch {
    port = null;
    void setStatus("disconnected");
    scheduleReconnect();
    return;
  }
  port.onMessage.addListener((message) => {
    if (isProtocolMismatch(message)) {
      void setStatus("protocol_mismatch");
      port?.disconnect();
      port = null;
      return;
    }
    if (message && typeof message === "object" && "ok" in message && (message as { ok?: boolean }).ok) {
      reconnectAttempts = 0;
      void setStatus("connected");
    }
  });
  port.onDisconnect.addListener(() => {
    port = null;
    if (status !== "protocol_mismatch") {
      void setStatus("disconnected");
      scheduleReconnect();
    }
  });
  send(buildHello(crypto.randomUUID(), chrome.runtime.getManifest().version ?? "0.1.0"));
}

function scheduleReconnect(): void {
  if (status === "protocol_mismatch") {
    return;
  }
  reconnectAttempts += 1;
  const delayMinutes = Math.min(1, 0.05 * reconnectAttempts);
  chrome.alarms.create(RECONNECT_ALARM, { delayInMinutes: Math.max(delayMinutes, 0.05) });
}

function schedulePing(): void {
  chrome.alarms.create(PING_ALARM, { periodInMinutes: 0.5 });
}

chrome.runtime.onInstalled.addListener(() => {
  connect();
  schedulePing();
});

chrome.runtime.onStartup.addListener(() => {
  connect();
  schedulePing();
});

chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === RECONNECT_ALARM) {
    connect();
  }
  if (alarm.name === PING_ALARM && port && status === "connected") {
    send(buildPing(crypto.randomUUID(), "keepalive"));
  }
});

connect();
schedulePing();
