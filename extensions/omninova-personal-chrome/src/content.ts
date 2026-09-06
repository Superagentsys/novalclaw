import {
  bumpSnapshotGeneration,
  click,
  currentSnapshotGeneration,
  fill,
  hover,
  lookupRef,
  press,
  readValue,
  scroll,
  selectValue,
  takeSnapshot,
} from "./dom.js";

export function handlePageMessage(
  message: { kind?: string; ref?: string; selector?: string; value?: string; text?: string; key?: string; direction?: string; pixels?: number; interactive_only?: boolean }
): Record<string, unknown> {
  if (message.kind === "navigate") {
    bumpSnapshotGeneration();
    return { snapshot_generation: currentSnapshotGeneration() };
  }
  if (message.kind === "observe") {
    const snapshot = takeSnapshot(document, Boolean(message.interactive_only));
    let value: string | undefined;
    if (message.ref || message.selector) {
      const el = resolve(message.ref, message.selector);
      if (!el) {
        return { error: { code: "StaleReference", message: "element reference is stale" } };
      }
      value = readValue(el);
    }
    return { ...snapshot, url: location.href, title: document.title, value };
  }
  if (message.kind === "act") {
    if (message.ref || message.selector) {
      const el = resolve(message.ref, message.selector);
      if (!el) {
        return { error: { code: "StaleReference", message: "element reference is stale" } };
      }
      const action = (message as { action?: string }).action;
      if (action === "click") click(el);
      if (action === "fill" && message.value !== undefined) fill(el, message.value);
      if (action === "type" && (message.text || message.value)) fill(el, String(message.text || message.value));
      if (action === "hover") hover(el);
      if (action === "select" && message.value !== undefined) selectValue(el, message.value);
    }
    if ((message as { action?: string }).action === "press" && message.key) {
      press(message.key);
    }
    if ((message as { action?: string }).action === "scroll") {
      scroll(message.direction || "down", message.pixels);
    }
    return { url: location.href, title: document.title };
  }
  return { error: { code: "OperationUnsupported", message: "unknown page op" } };
}

function resolve(ref?: string, selector?: string): Element | undefined {
  if (ref) {
    return lookupRef(ref);
  }
  if (selector) {
    return document.querySelector(selector) ?? undefined;
  }
  return undefined;
}

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  sendResponse(handlePageMessage(message));
  return true;
});

window.addEventListener("pagehide", () => {
  bumpSnapshotGeneration();
});
