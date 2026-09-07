(() => {
interface DomApi {
  bumpSnapshotGeneration(): number;
  click(el: Element): void;
  currentSnapshotGeneration(): number;
  fill(el: Element, value: string): void;
  hover(el: Element): void;
  lookupRef(ref: string): Element | undefined;
  press(key: string): void;
  readValue(el: Element): string;
  scroll(direction: string, pixels?: number): void;
  selectValue(el: Element, value: string): void;
  takeSnapshot(root: ParentNode, interactiveOnly?: boolean): Record<string, unknown>;
}

interface ContentState {
  authorizedGeneration: number | null;
  listenerInstalled: boolean;
}

const globalState = globalThis as typeof globalThis & {
  __omninovaPersonalChromeDom?: DomApi;
  __omninovaPersonalChromeContent?: ContentState;
};
const installedDom = globalState.__omninovaPersonalChromeDom;
if (!installedDom) return;
const dom: DomApi = installedDom;
const contentState = globalState.__omninovaPersonalChromeContent ?? {
  authorizedGeneration: null,
  listenerInstalled: false,
};
globalState.__omninovaPersonalChromeContent = contentState;

function handlePageMessage(
  message: { kind?: string; ref?: string; selector?: string; value?: string; text?: string; key?: string; direction?: string; pixels?: number; interactive_only?: boolean; authorization_generation?: number }
): Record<string, unknown> {
  if (message.kind === "authorization_sync") {
    contentState.authorizedGeneration = Number(message.authorization_generation);
    return { authorized: Number.isFinite(contentState.authorizedGeneration), authorization_generation: contentState.authorizedGeneration };
  }
  if (message.kind === "authorization_revoke") {
    contentState.authorizedGeneration = null;
    dom.bumpSnapshotGeneration();
    return { authorized: false };
  }
  if (
    contentState.authorizedGeneration === null ||
    message.authorization_generation !== contentState.authorizedGeneration
  ) {
    return { error: { code: "PersonalChromeNotAuthorized", message: "authorization generation is stale" } };
  }
  if (message.kind === "navigate") {
    dom.bumpSnapshotGeneration();
    return { snapshot_generation: dom.currentSnapshotGeneration() };
  }
  if (message.kind === "observe") {
    const snapshot = dom.takeSnapshot(document, Boolean(message.interactive_only));
    let value: string | undefined;
    if (message.ref || message.selector) {
      const el = resolve(message.ref, message.selector);
      if (!el) {
        return { error: { code: "StaleReference", message: "element reference is stale" } };
      }
      value = dom.readValue(el);
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
      if (action === "click") dom.click(el);
      if (action === "fill" && message.value !== undefined) dom.fill(el, message.value);
      if (action === "type" && (message.text || message.value)) dom.fill(el, String(message.text || message.value));
      if (action === "hover") dom.hover(el);
      if (action === "select" && message.value !== undefined) dom.selectValue(el, message.value);
    }
    if ((message as { action?: string }).action === "press" && message.key) {
      dom.press(message.key);
    }
    if ((message as { action?: string }).action === "scroll") {
      dom.scroll(message.direction || "down", message.pixels);
    }
    return { url: location.href, title: document.title };
  }
  return { error: { code: "OperationUnsupported", message: "unknown page op" } };
}

function resolve(ref?: string, selector?: string): Element | undefined {
  if (ref) {
    return dom.lookupRef(ref);
  }
  if (selector) {
    return document.querySelector(selector) ?? undefined;
  }
  return undefined;
}

if (!contentState.listenerInstalled) {
  chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
    sendResponse(handlePageMessage(message));
    return true;
  });
  contentState.listenerInstalled = true;
}

window.addEventListener("pagehide", () => {
  dom.bumpSnapshotGeneration();
});
})();
