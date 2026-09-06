document.addEventListener("DOMContentLoaded", async () => {
  const statusEl = document.getElementById("status");
  const hostEl = document.getElementById("host");
  if (!statusEl || !hostEl) {
    return;
  }
  const stored = await chrome.storage.local.get(["transportStatus", "nativeHostName"]);
  statusEl.textContent = String(stored.transportStatus ?? "disconnected");
  hostEl.textContent = String(stored.nativeHostName ?? "");
});
