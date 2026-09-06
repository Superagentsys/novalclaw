(() => {
const MAX_ELEMENT_TEXT = 400;
const MAX_OBSERVE_ELEMENTS = 80;

interface SnapshotElement {
  id: string;
  role: string;
  name: string;
  interactive: boolean;
  input_type?: string;
}

interface SnapshotResult {
  snapshot_generation: number;
  text: string;
  elements: SnapshotElement[];
}

let snapshotGeneration = randomGeneration();
const refs = new Map<string, Element>();

function randomGeneration(): number {
  const bytes = new Uint32Array(2);
  crypto.getRandomValues(bytes);
  return (bytes[0] % 0x3fffffff) * 100000 + (bytes[1] % 100000) + 1;
}

function bumpSnapshotGeneration(): number {
  snapshotGeneration += 1;
  refs.clear();
  return snapshotGeneration;
}

function currentSnapshotGeneration(): number {
  return snapshotGeneration;
}

function lookupRef(ref: string): Element | undefined {
  const prefix = `pc:${snapshotGeneration}:`;
  if (!ref.startsWith(prefix)) {
    return undefined;
  }
  return refs.get(ref);
}

function roleFor(el: Element): string {
  const explicit = el.getAttribute("role");
  if (explicit) {
    return explicit;
  }
  const tag = el.tagName.toLowerCase();
  if (tag === "a") return "link";
  if (tag === "button") return "button";
  if (tag === "input" || tag === "textarea") return "textbox";
  if (tag === "select") return "combobox";
  return tag;
}

function isInteractive(el: Element): boolean {
  const tag = el.tagName.toLowerCase();
  if (["a", "button", "input", "select", "textarea"].includes(tag)) {
    return true;
  }
  const role = el.getAttribute("role");
  return Boolean(role && ["button", "link", "textbox", "checkbox"].includes(role));
}

function visible(el: Element): boolean {
  const html = el as HTMLElement;
  if (!html.getBoundingClientRect) {
    return true;
  }
  const rect = html.getBoundingClientRect();
  return rect.width > 0 && rect.height > 0;
}

function takeSnapshot(root: ParentNode, interactiveOnly = false): SnapshotResult {
  snapshotGeneration = randomGeneration();
  refs.clear();
  const nodes = [...root.querySelectorAll("a, button, input, select, textarea, [role]")];
  const elements: SnapshotElement[] = [];
  const names: string[] = [];
  let id = 0;
  for (const el of nodes) {
    if (elements.length >= MAX_OBSERVE_ELEMENTS) {
      break;
    }
    if (!visible(el)) {
      continue;
    }
    const interactive = isInteractive(el);
    if (interactiveOnly && !interactive) {
      continue;
    }
    id += 1;
    const inputType = (el as HTMLInputElement).type || "";
    let name =
      (el as HTMLInputElement).getAttribute?.("aria-label") ||
      (el as HTMLInputElement).placeholder ||
      el.textContent?.trim() ||
      (el as HTMLInputElement).name ||
      "";
    if (name.length > MAX_ELEMENT_TEXT) {
      name = name.slice(0, MAX_ELEMENT_TEXT);
    }
    if (inputType.toLowerCase() === "password") {
      name = name || "password";
    }
    const ref = `pc:${snapshotGeneration}:${id}`;
    refs.set(ref, el);
    elements.push({
      id: String(id),
      role: roleFor(el),
      name,
      interactive,
      input_type: inputType || undefined,
    });
    if (inputType.toLowerCase() === "password") {
      names.push(`${name} [password]`);
    } else {
      names.push(name);
    }
  }
  return {
    snapshot_generation: snapshotGeneration,
    text: names.join("\n"),
    elements,
  };
}

function readValue(el: Element): string {
  const input = el as HTMLInputElement;
  const inputType = input.type || "";
  const raw = input.value ?? "";
  return inputType.toLowerCase() === "password" ? "" : raw;
}

function click(el: Element): void {
  (el as HTMLElement).click();
}

function fill(el: Element, value: string): void {
  const input = el as HTMLInputElement;
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  input.dispatchEvent(new Event("change", { bubbles: true }));
}

function hover(el: Element): void {
  el.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
}

function press(key: string): void {
  document.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
}

function scroll(direction: string, pixels = 400): void {
  const dy = direction === "up" ? -pixels : direction === "down" ? pixels : 0;
  const dx = direction === "left" ? -pixels : direction === "right" ? pixels : 0;
  window.scrollBy(dx, dy);
}

function selectValue(el: Element, value: string): void {
  const select = el as HTMLSelectElement;
  select.value = value;
  select.dispatchEvent(new Event("change", { bubbles: true }));
}

(globalThis as typeof globalThis & { __omninovaPersonalChromeDom?: unknown })
  .__omninovaPersonalChromeDom = {
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
  };
})();
