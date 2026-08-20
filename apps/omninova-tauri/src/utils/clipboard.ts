/**
 * Copies text in both secure browser contexts and the Tauri WebView.
 * Older WebViews can reject navigator.clipboard, so keep a DOM fallback.
 */
export async function writeClipboardText(text: string): Promise<void> {
  if (navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(text);
      return;
    } catch {
      // Continue with the selection-based fallback below.
    }
  }

  const textarea = document.createElement("textarea");
  textarea.value = text;
  textarea.setAttribute("readonly", "");
  textarea.style.position = "fixed";
  textarea.style.inset = "-9999px auto auto -9999px";
  textarea.style.opacity = "0";
  document.body.appendChild(textarea);
  textarea.select();
  textarea.setSelectionRange(0, textarea.value.length);

  try {
    if (!document.execCommand("copy")) {
      throw new Error("当前系统拒绝了剪贴板写入，请手动复制。");
    }
  } finally {
    textarea.remove();
  }
}
