/* eslint-disable react-refresh/only-export-components -- shared model-selection helpers are intentionally colocated with the picker */
import { useLayoutEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { createPortal } from "react-dom";
import { UiIcon } from "../UiIcon";
import "./ModelPicker.css";

export const MODEL_SELECTION_STORAGE_KEY = "omninova.ui.modelSelection.v1";
export const MAX_MODE_STORAGE_KEY = "omninova.ui.maxMode.v1";

export type PickerProvider = {
  id: string;
  label: string;
  type?: string;
  models: string[];
};

export type ModelPickerProps = {
  value: string;
  onChange: (value: string) => void;
  providers: PickerProvider[];
  defaultProvider?: string;
  defaultModel?: string;
  maxMode: boolean;
  onMaxModeChange: (next: boolean) => void;
  onConfigureCustom: () => void;
  disabled?: boolean;
  /** Compact trigger used in the chat toolbar. */
  variant?: "toolbar" | "inline";
};

export function encodeModelSelection(providerId: string, model: string): string {
  return `${providerId}::${model}`;
}

export function parseModelSelection(value: string): {
  providerId?: string;
  model?: string;
} {
  if (!value || value === "auto") return {};
  const idx = value.indexOf("::");
  if (idx <= 0) return { providerId: value };
  return {
    providerId: value.slice(0, idx),
    model: value.slice(idx + 2),
  };
}

export function readStoredModelSelection(): string {
  try {
    return window.localStorage.getItem(MODEL_SELECTION_STORAGE_KEY) ?? "auto";
  } catch {
    return "auto";
  }
}

export function persistModelSelection(value: string) {
  try {
    window.localStorage.setItem(MODEL_SELECTION_STORAGE_KEY, value);
  } catch {
    // Ignore WebView storage failures.
  }
}

export function readStoredMaxMode(): boolean {
  try {
    return window.localStorage.getItem(MAX_MODE_STORAGE_KEY) === "1";
  } catch {
    return false;
  }
}

export function persistMaxMode(value: boolean) {
  try {
    window.localStorage.setItem(MAX_MODE_STORAGE_KEY, value ? "1" : "0");
  } catch {
    // Ignore WebView storage failures.
  }
}

function isLocalProvider(type?: string, id?: string): boolean {
  const key = (type || id || "").toLowerCase();
  return ["ollama", "lmstudio", "vllm", "sglang", "llamacpp", "local"].includes(key);
}

function providerInitial(label: string): string {
  const trimmed = label.trim();
  if (!trimmed) return "M";
  return trimmed.slice(0, 1).toUpperCase();
}

function normalizeSelection(value: string, providers: PickerProvider[]): string {
  if (value === "auto") return "auto";
  const parsed = parseModelSelection(value);
  const provider = providers.find((item) => item.id === parsed.providerId);
  if (!provider) return "auto";
  if (parsed.model && provider.models.includes(parsed.model)) {
    return encodeModelSelection(provider.id, parsed.model);
  }
  if (provider.models[0]) {
    return encodeModelSelection(provider.id, provider.models[0]);
  }
  return encodeModelSelection(provider.id, parsed.model ?? provider.id);
}

export function ModelPicker({
  value,
  onChange,
  providers,
  defaultProvider,
  defaultModel,
  maxMode,
  onMaxModeChange,
  onConfigureCustom,
  disabled = false,
  variant = "toolbar",
}: ModelPickerProps) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [panelStyle, setPanelStyle] = useState<CSSProperties>();
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);

  const normalized = useMemo(
    () => normalizeSelection(value, providers),
    [value, providers]
  );

  const selectedLabel = useMemo(() => {
    if (normalized === "auto") return "自动模型";
    const parsed = parseModelSelection(normalized);
    return parsed.model || parsed.providerId || "自动模型";
  }, [normalized]);

  const rows = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return providers.flatMap((provider) => {
      const models = provider.models.length ? provider.models : [provider.id];
      return models
        .filter((model) => {
          if (!needle) return true;
          return (
            model.toLowerCase().includes(needle) ||
            provider.label.toLowerCase().includes(needle) ||
            provider.id.toLowerCase().includes(needle)
          );
        })
        .map((model) => {
          const selection = encodeModelSelection(provider.id, model);
          const tags: Array<{ label: string; tone: "default" | "local" | "custom" }> = [];
          if (provider.id === defaultProvider && model === defaultModel) {
            tags.push({ label: "默认", tone: "default" });
          }
          if (isLocalProvider(provider.type, provider.id)) {
            tags.push({ label: "本地", tone: "local" });
          }
          if (provider.id.startsWith("custom-") || provider.type === "custom") {
            tags.push({ label: "自定义", tone: "custom" });
          }
          return {
            key: selection,
            providerId: provider.id,
            providerLabel: provider.label,
            model,
            initial: providerInitial(provider.label),
            tags,
          };
        });
    });
  }, [providers, query, defaultProvider, defaultModel]);

  useLayoutEffect(() => {
    if (!open) return;

    const placePanel = () => {
      const rect = triggerRef.current?.getBoundingClientRect();
      if (!rect) return;
      const width = Math.min(320, window.innerWidth - 24);
      let left = variant === "inline" ? rect.right - width : rect.left;
      left = Math.min(Math.max(12, left), window.innerWidth - width - 12);
      if (variant === "inline") {
        setPanelStyle({
          position: "fixed",
          bottom: Math.max(12, window.innerHeight - rect.top + 8),
          left,
          width,
          maxHeight: Math.min(440, Math.max(180, rect.top - 24)),
          zIndex: 80,
        });
      } else {
        setPanelStyle({
          position: "fixed",
          top: rect.bottom + 8,
          left,
          width,
          maxHeight: Math.min(440, window.innerHeight - rect.bottom - 24),
          zIndex: 80,
        });
      }
    };

    const handlePointerDown = (event: MouseEvent) => {
      const target = event.target as Node;
      if (rootRef.current?.contains(target) || panelRef.current?.contains(target)) {
        return;
      }
      setOpen(false);
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };

    placePanel();
    document.addEventListener("mousedown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);
    window.addEventListener("resize", placePanel);
    window.addEventListener("scroll", placePanel, true);
    const timer = window.setTimeout(() => searchRef.current?.focus(), 20);
    return () => {
      document.removeEventListener("mousedown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("resize", placePanel);
      window.removeEventListener("scroll", placePanel, true);
      window.clearTimeout(timer);
    };
  }, [open, variant]);

  const selectValue = (next: string) => {
    onChange(next);
    persistModelSelection(next);
    setOpen(false);
    setQuery("");
  };

  return (
    <div
      ref={rootRef}
      className={`model-picker model-picker--${variant}${open ? " is-open" : ""}`}
    >
      <button
        type="button"
        ref={triggerRef}
        className="model-picker-trigger"
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label="选择任务模型"
        title="选择本次任务优先使用的模型"
        disabled={disabled}
        onClick={() => setOpen((prev) => !prev)}
      >
        <UiIcon name="apps" size={14} />
        <span className="model-picker-trigger-label">{selectedLabel}</span>
        {maxMode ? <span className="model-picker-max-chip">Max</span> : null}
        <span className="model-picker-chevron" aria-hidden />
      </button>

      {open
        ? createPortal(
            <div
              ref={panelRef}
              className="model-picker-panel"
              role="dialog"
              aria-label="选择模型"
              style={{
                ...panelStyle,
                visibility: panelStyle ? "visible" : "hidden",
              }}
            >
              <div className="model-picker-max">
                <div>
                  <strong>Max 模式</strong>
                  <span>请求更强推理，适合复杂任务</span>
                </div>
                <button
                  type="button"
                  className={`model-picker-switch${maxMode ? " is-on" : ""}`}
                  role="switch"
                  aria-checked={maxMode}
                  onClick={() => {
                    const next = !maxMode;
                    onMaxModeChange(next);
                    persistMaxMode(next);
                  }}
                >
                  <span />
                </button>
              </div>

              <label className="model-picker-search">
                <UiIcon name="search" size={13} />
                <input
                  ref={searchRef}
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                  placeholder="搜索模型或服务"
                />
              </label>

              <div className="model-picker-list" role="listbox" aria-label="可用模型">
                <button
                  type="button"
                  role="option"
                  aria-selected={normalized === "auto"}
                  className={`model-picker-row${normalized === "auto" ? " is-selected" : ""}`}
                  onClick={() => selectValue("auto")}
                >
                  <span className="model-picker-avatar model-picker-avatar--auto">A</span>
                  <span className="model-picker-copy">
                    <strong>Auto</strong>
                    <em>按默认服务自动选择</em>
                  </span>
                </button>

                {rows.length === 0 ? (
                  <div className="model-picker-empty">
                    {providers.length === 0
                      ? "还没有可用模型，先配置自定义服务。"
                      : "没有匹配的模型。"}
                  </div>
                ) : (
                  rows.map((row) => (
                    <button
                      key={row.key}
                      type="button"
                      role="option"
                      aria-selected={normalized === row.key}
                      className={`model-picker-row${normalized === row.key ? " is-selected" : ""}`}
                      onClick={() => selectValue(row.key)}
                    >
                      <span className="model-picker-avatar">{row.initial}</span>
                      <span className="model-picker-copy">
                        <strong>
                          {row.model}
                          {row.tags.map((tag) => (
                            <span
                              key={tag.label}
                              className={`model-picker-tag model-picker-tag--${tag.tone}`}
                            >
                              {tag.label}
                            </span>
                          ))}
                        </strong>
                        <em>{row.providerLabel}</em>
                      </span>
                    </button>
                  ))
                )}
              </div>

              <button
                type="button"
                className="model-picker-custom"
                onClick={() => {
                  setOpen(false);
                  onConfigureCustom();
                }}
              >
                <UiIcon name="edit" size={14} />
                配置自定义模型
              </button>
            </div>,
            document.body
          )
        : null}
    </div>
  );
}
