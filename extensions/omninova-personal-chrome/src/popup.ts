export interface PopupAuthorizationView {
  title: string;
  detail: string;
  canAuthorize: boolean;
  canRevoke: boolean;
}

function permissionOrigin(url: string): string | undefined {
  try {
    const parsed = new URL(url);
    if (parsed.protocol !== "http:" && parsed.protocol !== "https:") return undefined;
    return `${parsed.protocol}//${parsed.host}/*`;
  } catch {
    return undefined;
  }
}

export function popupAuthorizationView(
  transportStatus: string,
  authorizedTabId: number | null,
  currentTabId: number | null = null
): PopupAuthorizationView {
  if (transportStatus === "protocol_mismatch") {
    return { title: "协议不兼容", detail: "请更新 OmniNova 与扩展。", canAuthorize: false, canRevoke: authorizedTabId !== null };
  }
  if (transportStatus !== "connected") {
    return { title: "扩展尚未连接", detail: "请先启动 OmniNova Desktop 并检查 Native Host。", canAuthorize: false, canRevoke: authorizedTabId !== null };
  }
  if (authorizedTabId !== null) {
    if (authorizedTabId === currentTabId) {
      return { title: "当前标签页已授权", detail: "OmniNova 仅能访问这个明确授权的标签页。", canAuthorize: true, canRevoke: true };
    }
    return { title: "已授权其他标签页", detail: "当前标签页尚未授权。你可以明确切换授权到这里。", canAuthorize: true, canRevoke: true };
  }
  return { title: "当前未授权", detail: "扩展已连接，但 OmniNova 不能读取或操作任何页面。", canAuthorize: true, canRevoke: false };
}

if (typeof document !== "undefined") document.addEventListener("DOMContentLoaded", async () => {
  const statusEl = document.getElementById("status");
  const hostEl = document.getElementById("host");
  const detailEl = document.getElementById("detail");
  const allowButton = document.getElementById("allow") as HTMLButtonElement | null;
  const revokeButton = document.getElementById("revoke") as HTMLButtonElement | null;
  if (!statusEl || !hostEl || !detailEl || !allowButton || !revokeButton) {
    return;
  }
  const stored = await chrome.storage.local.get(["transportStatus", "nativeHostName"]);
  hostEl.textContent = String(stored.nativeHostName ?? "");
  const refresh = async () => {
    const state = await chrome.runtime.sendMessage({ kind: "popup_authorization_status" });
    const [activeTab] = await chrome.tabs.query({ active: true, currentWindow: true });
    const view = popupAuthorizationView(
      String(state?.transportStatus ?? stored.transportStatus ?? "disconnected"),
      typeof state?.authorizedTabId === "number" ? state.authorizedTabId : null,
      activeTab?.id ?? null
    );
    statusEl.textContent = view.title;
    detailEl.textContent = view.detail;
    allowButton.disabled = !view.canAuthorize;
    revokeButton.disabled = !view.canRevoke;
  };
  allowButton.addEventListener("click", async () => {
    allowButton.disabled = true;
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
    const origin = tab?.url ? permissionOrigin(tab.url) : undefined;
    if (!tab?.id || tab.windowId === undefined || !origin) {
      await refresh();
      detailEl.textContent = "当前页面不支持授权。";
      return;
    }
    const permitted = await chrome.permissions.request({ origins: [origin] });
    if (!permitted) {
      await refresh();
      detailEl.textContent = "未授予当前网站权限。";
      return;
    }
    const result = await chrome.runtime.sendMessage({
      kind: "popup_authorize_current_tab",
      tabId: tab.id,
      windowId: tab.windowId,
      origin,
    });
    await refresh();
    if (!result?.ok) detailEl.textContent = `无法授权：${String(result?.code ?? "Unknown")}`;
  });
  revokeButton.addEventListener("click", async () => {
    revokeButton.disabled = true;
    await chrome.runtime.sendMessage({ kind: "popup_revoke_authorization" });
    await refresh();
    detailEl.textContent = "访问已撤销；Chrome 和标签页保持打开。";
  });
  await refresh();
});
