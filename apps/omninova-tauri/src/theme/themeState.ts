import { createContext, useContext } from "react";
import type { UiIconName } from "../components/UiIcon";

export type ThemeId = "daylight" | "midnight" | "aurora" | "paper" | "graphite";
export type ThemePreference = "system" | ThemeId;

export interface ThemeOption {
  id: ThemePreference;
  label: string;
  description: string;
  icon: UiIconName;
  swatches: readonly [string, string, string];
}

export const THEME_OPTIONS: readonly ThemeOption[] = [
  { id: "system", label: "跟随系统", description: "自动匹配 Windows，采用中性系统配色", icon: "desktop", swatches: ["#f5f7fb", "#171b20", "#6fa8e8"] },
  { id: "daylight", label: "澄明白昼", description: "高可读的冷白工作台", icon: "sun", swatches: ["#f5f7fb", "#ffffff", "#4f6bed"] },
  { id: "midnight", label: "深海夜航", description: "低眩光的深蓝夜间界面", icon: "moon", swatches: ["#0c1220", "#151e30", "#69a7ff"] },
  { id: "aurora", label: "极光实验室", description: "冷矿物色与清晰状态仪表", icon: "experiment", swatches: ["#edf2ef", "#fbfdfc", "#63a432"] },
  { id: "paper", label: "纸墨档案", description: "温和纸色、墨色与克制朱红", icon: "fileText", swatches: ["#f4f0e7", "#fffdf7", "#b5483b"] },
  { id: "graphite", label: "石墨工坊", description: "中性深灰与琥珀操作信号", icon: "tool", swatches: ["#111315", "#1c2024", "#e2ad3b"] },
] as const;

const STORAGE_KEY = "omninova.ui.theme.v1";
const VALID_PREFERENCES = new Set<ThemePreference>(THEME_OPTIONS.map((option) => option.id));

export interface ThemeContextValue {
  preference: ThemePreference;
  resolvedTheme: ThemeId;
  setPreference: (preference: ThemePreference) => void;
}

export const ThemeContext = createContext<ThemeContextValue | null>(null);

export function systemPrefersDark(): boolean {
  return typeof window !== "undefined" &&
    window.matchMedia?.("(prefers-color-scheme: dark)").matches === true;
}

export function readPreference(): ThemePreference {
  if (typeof window === "undefined") return "system";
  try {
    const stored = window.localStorage.getItem(STORAGE_KEY) as ThemePreference | null;
    return stored && VALID_PREFERENCES.has(stored) ? stored : "system";
  } catch {
    return "system";
  }
}

export function resolveTheme(preference: ThemePreference, dark = systemPrefersDark()): ThemeId {
  return preference === "system" ? (dark ? "midnight" : "daylight") : preference;
}

export function applyTheme(preference: ThemePreference, dark = systemPrefersDark()): ThemeId {
  const resolved = resolveTheme(preference, dark);
  if (typeof document !== "undefined") {
    document.documentElement.dataset.theme = resolved;
    document.documentElement.dataset.themePreference = preference;
    document.documentElement.style.colorScheme =
      resolved === "midnight" || resolved === "graphite" ? "dark" : "light";
  }
  return resolved;
}

export function isThemePreference(value: ThemePreference): boolean {
  return VALID_PREFERENCES.has(value);
}

export function persistThemePreference(value: ThemePreference) {
  window.localStorage.setItem(STORAGE_KEY, value);
}

/** Apply the saved theme before React mounts to avoid a bright startup flash. */
export function initializeTheme() {
  applyTheme(readPreference());
}

export function useTheme(): ThemeContextValue {
  const value = useContext(ThemeContext);
  if (!value) throw new Error("useTheme must be used inside ThemeProvider");
  return value;
}
