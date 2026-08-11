import { useCallback, useEffect, useRef, useState } from "react";
import { THEME_OPTIONS, useTheme } from "../theme/themeState";
import { UiIcon } from "./UiIcon";

export function ThemeSwitcher({ collapsed = false }: { collapsed?: boolean }) {
  const { preference, resolvedTheme, setPreference } = useTheme();
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const optionRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const current = THEME_OPTIONS.find((option) => option.id === preference) ?? THEME_OPTIONS[0];
  const menuId = "omninova-theme-menu";

  const closeMenu = useCallback((restoreFocus = false) => {
    setOpen(false);
    if (restoreFocus) {
      window.requestAnimationFrame(() => triggerRef.current?.focus());
    }
  }, []);

  useEffect(() => {
    if (!open) return;
    const selectedIndex = Math.max(
      0,
      THEME_OPTIONS.findIndex((option) => option.id === preference),
    );
    window.requestAnimationFrame(() => optionRefs.current[selectedIndex]?.focus());
  }, [open, preference]);

  useEffect(() => {
    if (!open) return;
    const handlePointerDown = (event: MouseEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) closeMenu(false);
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") closeMenu(true);
    };
    document.addEventListener("mousedown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("mousedown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [open, closeMenu]);

  const handleMenuKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    const currentIndex = optionRefs.current.findIndex(
      (item) => item === document.activeElement,
    );
    let nextIndex = currentIndex;
    if (event.key === "ArrowDown") {
      nextIndex = currentIndex < 0 ? 0 : (currentIndex + 1) % THEME_OPTIONS.length;
    } else if (event.key === "ArrowUp") {
      nextIndex =
        currentIndex < 0
          ? THEME_OPTIONS.length - 1
          : (currentIndex - 1 + THEME_OPTIONS.length) % THEME_OPTIONS.length;
    } else if (event.key === "Home") {
      nextIndex = 0;
    } else if (event.key === "End") {
      nextIndex = THEME_OPTIONS.length - 1;
    } else {
      return;
    }
    event.preventDefault();
    optionRefs.current[nextIndex]?.focus();
  };

  return (
    <div className="theme-switcher" ref={rootRef}>
      <button
        ref={triggerRef}
        type="button"
        className={`app-shell-nav-item theme-switcher-trigger${open ? " is-open" : ""}`}
        onClick={() => setOpen((value) => !value)}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={menuId}
        aria-label={`外观主题：${current.label}`}
        title={collapsed ? `外观主题：${current.label}` : undefined}
      >
        <span className="app-shell-nav-icon">
          <UiIcon name="palette" />
        </span>
        {!collapsed ? (
          <span className="theme-switcher-trigger-copy">
            <span className="app-shell-nav-label">外观主题</span>
            <span className="theme-switcher-current">{current.label}</span>
          </span>
        ) : null}
      </button>

      {open ? (
        <div
          id={menuId}
          className="theme-switcher-popover"
          role="menu"
          aria-label="选择界面主题"
          onKeyDown={handleMenuKeyDown}
        >
          <div className="theme-switcher-popover-head">
            <div>
              <strong>界面主题</strong>
              <span>选择会自动保存</span>
            </div>
            <span className="theme-switcher-resolved">
              当前 {resolvedTheme === "midnight" || resolvedTheme === "graphite" ? "夜间" : "日间"}
            </span>
          </div>
          <div className="theme-switcher-options">
            {THEME_OPTIONS.map((option) => {
              const selected = option.id === preference;
              return (
                <button
                  ref={(node) => {
                    optionRefs.current[THEME_OPTIONS.indexOf(option)] = node;
                  }}
                  key={option.id}
                  type="button"
                  className={`theme-option${selected ? " is-selected" : ""}`}
                  role="menuitemradio"
                  aria-checked={selected}
                  tabIndex={selected ? 0 : -1}
                  onClick={() => {
                    setPreference(option.id);
                    closeMenu(true);
                  }}
                >
                  <span className="theme-option-icon" aria-hidden>
                    <UiIcon name={option.icon} />
                  </span>
                  <span className="theme-option-copy">
                    <strong>{option.label}</strong>
                    <span>{option.description}</span>
                  </span>
                  <span className="theme-option-swatches" aria-hidden>
                    {option.swatches.map((color) => (
                      <i key={color} style={{ background: color }} />
                    ))}
                  </span>
                  {selected ? (
                    <span className="theme-option-check" aria-hidden>
                      <UiIcon name="check" size={14} />
                    </span>
                  ) : null}
                </button>
              );
            })}
          </div>
        </div>
      ) : null}
    </div>
  );
}
