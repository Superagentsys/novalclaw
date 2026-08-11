import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import {
  ThemeContext,
  applyTheme,
  isThemePreference,
  persistThemePreference,
  readPreference,
  resolveTheme,
  systemPrefersDark,
  type ThemeContextValue,
  type ThemePreference,
} from "./themeState";

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [preference, setPreferenceState] = useState<ThemePreference>(readPreference);
  const [systemDark, setSystemDark] = useState(systemPrefersDark);
  const resolvedTheme = resolveTheme(preference, systemDark);

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const handleChange = (event: MediaQueryListEvent) => setSystemDark(event.matches);
    media.addEventListener("change", handleChange);
    return () => media.removeEventListener("change", handleChange);
  }, []);

  useEffect(() => {
    applyTheme(preference, systemDark);
  }, [preference, systemDark]);

  useEffect(() => {
    const inTauri = Boolean(
      (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__
    );
    if (!inTauri) return;

    let disposed = false;
    void import("@tauri-apps/api/window")
      .then(({ getCurrentWindow }) => {
        if (disposed) return;
        const nativeTheme = preference === "system"
          ? null
          : resolvedTheme === "midnight" || resolvedTheme === "graphite"
            ? "dark"
            : "light";
        return getCurrentWindow().setTheme(nativeTheme);
      })
      .catch(() => {
        // CSS theming remains functional when native window theming is unavailable.
      });

    return () => {
      disposed = true;
    };
  }, [preference, resolvedTheme]);

  const setPreference = useCallback((next: ThemePreference) => {
    if (!isThemePreference(next)) return;
    setPreferenceState(next);
    try {
      persistThemePreference(next);
    } catch {
      // A read-only WebView profile should still change theme in memory.
    }
  }, []);

  const value = useMemo<ThemeContextValue>(
    () => ({ preference, resolvedTheme, setPreference }),
    [preference, resolvedTheme, setPreference]
  );

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}
