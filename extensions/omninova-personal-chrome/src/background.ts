import {
  attachSession,
  authorizationSnapshot,
  detachSession,
  getAuthorized,
  grantAuthorization,
  listAuthorized,
  revokeAll,
  revokeTab,
  restoreAuthorizationGeneration,
} from "./authorize.js";
import {
  ConnectionStatus,
  PROTOCOL_VERSION,
  APPLICATION_MAX_MESSAGE_BYTES,
  TransportRequest,
  TransportResponse,
  buildHello,
  buildPing,
  isDesktopRequest,
  isProtocolMismatch,
  isRestrictedUrl,
  nativeHostName,
  originPermissionPattern,
} from "./protocol.js";

const RECONNECT_ALARM = "omninova-personal-chrome-reconnect";
const PING_ALARM = "omninova-personal-chrome-ping";

let port: chrome.runtime.Port | null = null;
let status: ConnectionStatus = "disconnected";
let reconnectAttempts = 0;

async function initializeAuthorizationState(): Promise<void> {
  const stored = await chrome.storage.local.get("authorizationGeneration");
  restoreAuthorizationGeneration(Number(stored.authorizationGeneration ?? 0));
  await revokeAuthorization();
}

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
  const encoded = JSON.stringify(message);
  if (encoded.length > APPLICATION_MAX_MESSAGE_BYTES) {
    return;
  }
  port.postMessage(message);
}

function respond(request: TransportRequest, ok: boolean, payload?: Record<string, unknown>, error?: { code: string; message: string }): void {
  const response: TransportResponse = {
    protocol_version: PROTOCOL_VERSION,
    request_id: request.request_id,
    ok,
    payload: ok ? payload ?? {} : undefined,
    error: ok ? undefined : error,
  };
  send(response);
}

function fail(request: TransportRequest, code: string, message: string): void {
  respond(request, false, undefined, { code, message });
}

async function tabInfo(tabId: number): Promise<chrome.tabs.Tab | undefined> {
  try {
    return await chrome.tabs.get(tabId);
  } catch {
    return undefined;
  }
}

async function ensureAuthorized(
  request: TransportRequest
): Promise<{ windowId: number; tabId: number; authorizationGeneration: number; url: string } | null> {
  const tabId = Number(request.payload.tab_id);
  const generation = request.payload.authorization_generation as number | undefined;
  const grant = getAuthorized(tabId, generation);
  if (!grant) {
    fail(request, "PersonalChromeNotAuthorized", "tab is not authorized");
    return null;
  }
  const tab = await tabInfo(tabId);
  if (!tab) {
    fail(request, "TabUnavailable", "authorized tab is gone");
    return null;
  }
  const url = tab.url ?? "";
  if (isRestrictedUrl(url)) {
    fail(request, "OperationUnsupported", "restricted chrome pages cannot be controlled");
    return null;
  }
  return {
    windowId: grant.windowId,
    tabId: grant.tabId,
    authorizationGeneration: grant.authorizationGeneration,
    url,
  };
}

async function sendToTab(tabId: number, message: Record<string, unknown>): Promise<Record<string, unknown>> {
  return await chrome.tabs.sendMessage(tabId, message);
}

async function injectAuthorizedContent(tabId: number, generation: number): Promise<void> {
  await chrome.scripting.executeScript({
    target: { tabId, allFrames: false },
    files: ["dist/dom.js", "dist/content.js"],
  });
  await sendToTab(tabId, {
    kind: "authorization_sync",
    authorization_generation: generation,
  });
}

async function publishAuthorizationState(): Promise<void> {
  const snapshot = authorizationSnapshot();
  await chrome.storage.local.set({
    authorizationEnabled: snapshot.authorized.length === 1,
    authorizationGeneration: snapshot.generation,
  });
}

async function revokeAuthorization(preserveOrigin?: string): Promise<void> {
  const grants = listAuthorized();
  revokeAll();
  for (const grant of grants) {
    try {
      await sendToTab(grant.tabId, {
        kind: "authorization_revoke",
        authorization_generation: grant.authorizationGeneration,
      });
    } catch {
      // A closed or navigated tab is already effectively revoked.
    }
    if (grant.originPermission && grant.originPermission !== preserveOrigin) {
      try {
        await chrome.permissions.remove({ origins: [grant.originPermission] });
      } catch {
        // Authorization is already invalid even if Chrome retains permission.
      }
    }
  }
  await publishAuthorizationState();
}

async function handleDesktopRequest(request: TransportRequest): Promise<void> {
  if (request.protocol_version !== PROTOCOL_VERSION) {
    fail(request, "ProtocolMismatch", "incompatible protocol");
    return;
  }
  switch (request.operation) {
    case "tab_list_authorized": {
      const grants = listAuthorized();
      const tabs = [];
      for (const grant of grants) {
        const tab = await tabInfo(grant.tabId);
        if (!tab) {
          continue;
        }
        tabs.push({
          window_id: grant.windowId,
          tab_id: grant.tabId,
          authorization_generation: grant.authorizationGeneration,
        });
      }
      respond(request, true, { tabs });
      return;
    }
    case "revoke_authorization": {
      await revokeAuthorization();
      respond(request, true, { revoked: true });
      return;
    }
    case "tab_get": {
      const bound = await ensureAuthorized(request);
      if (!bound) {
        return;
      }
      const tab = await tabInfo(bound.tabId);
      respond(request, true, {
        tab_id: bound.tabId,
        url: tab?.url ?? bound.url,
        title: tab?.title ?? "",
      });
      return;
    }
    case "attach_session": {
      const bound = await ensureAuthorized(request);
      if (!bound) {
        return;
      }
      const token = `pcs:${crypto.randomUUID()}`;
      attachSession(token, bound.tabId);
      respond(request, true, { session_token: token, tab_id: bound.tabId });
      return;
    }
    case "detach_session": {
      detachSession(request.session_id);
      respond(request, true, { detached: true });
      return;
    }
    case "session_health": {
      const bound = await ensureAuthorized(request);
      if (!bound) {
        return;
      }
      respond(request, true, { healthy: true, tab_id: bound.tabId, url: bound.url });
      return;
    }
    case "observe": {
      const bound = await ensureAuthorized(request);
      if (!bound) {
        return;
      }
      try {
        const page = await sendToTab(bound.tabId, {
          kind: "observe",
          authorization_generation: bound.authorizationGeneration,
          ref: request.payload.ref,
          selector: request.payload.selector,
          interactive_only: request.payload.interactive_only,
        });
        const pageError = page.error as { code?: string; message?: string } | undefined;
        if (pageError) {
          fail(request, String(pageError.code || "OperationUnsupported"), String(pageError.message || ""));
          return;
        }
        respond(request, true, page);
      } catch {
        fail(request, "OperationUnsupported", "content script is not available on this page");
      }
      return;
    }
    case "act": {
      const bound = await ensureAuthorized(request);
      if (!bound) {
        return;
      }
      if (request.payload.action === "eval") {
        fail(request, "OperationUnsupported", "eval is unsupported");
        return;
      }
      try {
        const page = await sendToTab(bound.tabId, {
          kind: "act",
          authorization_generation: bound.authorizationGeneration,
          ...request.payload,
        });
        const pageError = page.error as { code?: string; message?: string } | undefined;
        if (pageError) {
          fail(request, String(pageError.code || "OperationUnsupported"), String(pageError.message || ""));
          return;
        }
        respond(request, true, page);
      } catch {
        fail(request, "OperationUnsupported", "content script is not available on this page");
      }
      return;
    }
    case "navigate": {
      const bound = await ensureAuthorized(request);
      if (!bound) {
        return;
      }
      const url = String(request.payload.url || "");
      if (isRestrictedUrl(url)) {
        fail(request, "OperationUnsupported", "cannot navigate to a restricted URL");
        return;
      }
      const updated = await chrome.tabs.update(bound.tabId, { url });
      try {
        await injectAuthorizedContent(bound.tabId, bound.authorizationGeneration);
        await sendToTab(bound.tabId, {
          kind: "navigate",
          authorization_generation: bound.authorizationGeneration,
        });
      } catch {
        // Content script will reload with the new document.
      }
      respond(request, true, { url: updated?.url ?? url, title: updated?.title ?? "", tab_id: bound.tabId });
      return;
    }
    case "screenshot":
      fail(
        request,
        "OperationUnsupported",
        "screenshot is not enabled in the B3.5-C v1 permission model"
      );
      return;
    case "eval":
    case "cookies":
    case "storage_get":
    case "debugger":
      fail(request, "OperationUnsupported", `${request.operation} is forbidden`);
      return;
    default:
      fail(request, "OperationUnsupported", `unknown operation ${request.operation}`);
  }
}

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (!message || typeof message !== "object") return false;
  const kind = (message as { kind?: string }).kind;
  if (kind === "popup_authorization_status") {
    void authorizationReady.then(() => {
      const snapshot = authorizationSnapshot();
      sendResponse({
        transportStatus: status,
        authorizationGeneration: snapshot.generation,
        authorizedTabId: snapshot.authorized[0]?.tabId ?? null,
        authorizedWindowId: snapshot.authorized[0]?.windowId ?? null,
      });
    });
    return true;
  }
  if (kind === "popup_authorize_current_tab") {
    void (async () => {
      await authorizationReady;
      const requestedTabId = Number((message as { tabId?: number }).tabId);
      const requestedWindowId = Number((message as { windowId?: number }).windowId);
      const requestedOrigin = String((message as { origin?: string }).origin ?? "");
      const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
      if (
        !tab?.id ||
        tab.windowId === undefined ||
        !tab.url ||
        isRestrictedUrl(tab.url) ||
        tab.id !== requestedTabId ||
        tab.windowId !== requestedWindowId
      ) {
        sendResponse({ ok: false, code: "RestrictedOrUnavailableTab" });
        return;
      }
      const origin = originPermissionPattern(tab.url);
      if (!origin || origin !== requestedOrigin) {
        sendResponse({ ok: false, code: "RestrictedOrUnavailableTab" });
        return;
      }
      const permitted = await chrome.permissions.contains({ origins: [origin] });
      if (!permitted) {
        sendResponse({ ok: false, code: "HostPermissionDenied" });
        return;
      }
      await revokeAuthorization(origin);
      const grant = grantAuthorization(tab.windowId, tab.id, origin);
      try {
        await injectAuthorizedContent(tab.id, grant.authorizationGeneration);
      } catch {
        revokeTab(tab.id);
        if (grant.originPermission) {
          await chrome.permissions.remove({ origins: [grant.originPermission] });
        }
        await publishAuthorizationState();
        sendResponse({ ok: false, code: "ContentScriptInjectionFailed" });
        return;
      }
      await publishAuthorizationState();
      sendResponse({ ok: true, authorizationGeneration: grant.authorizationGeneration });
    })();
    return true;
  }
  if (kind === "popup_revoke_authorization") {
    void authorizationReady
      .then(() => revokeAuthorization())
      .then(() => sendResponse({ ok: true }));
    return true;
  }
  return false;
});

chrome.tabs.onRemoved.addListener((tabId) => {
  const grant = getAuthorized(tabId);
  if (!grant) return;
  revokeTab(tabId);
  if (grant.originPermission) {
    void chrome.permissions.remove({ origins: [grant.originPermission] });
  }
  void publishAuthorizationState();
});

chrome.tabs.onUpdated.addListener((tabId, changeInfo, tab) => {
  if (changeInfo.status !== "complete") return;
  const grant = getAuthorized(tabId);
  if (!grant) return;
  if (!tab.url || isRestrictedUrl(tab.url)) return;
  void injectAuthorizedContent(tabId, grant.authorizationGeneration).catch(async () => {
    revokeTab(tabId);
    if (grant.originPermission) {
      await chrome.permissions.remove({ origins: [grant.originPermission] });
    }
    await publishAuthorizationState();
  });
});

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
    if (isDesktopRequest(message)) {
      void handleDesktopRequest(message);
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
  void authorizationReady.then(() => {
    connect();
    schedulePing();
  });
});

chrome.runtime.onStartup.addListener(() => {
  void authorizationReady.then(() => {
    connect();
    schedulePing();
  });
});

chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === RECONNECT_ALARM) {
    connect();
  }
  if (alarm.name === PING_ALARM && port && status === "connected") {
    send(buildPing(crypto.randomUUID(), "keepalive"));
  }
});

const authorizationReady = initializeAuthorizationState();
void authorizationReady.then(() => {
  connect();
  schedulePing();
});
