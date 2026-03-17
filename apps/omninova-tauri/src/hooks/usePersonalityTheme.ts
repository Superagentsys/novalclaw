/**
 * 人格主题切换 Hook
 *
 * 提供基于 MBTI 人格类型的动态主题切换功能
 * 通过 CSS 变量实现运行时主题更新，支持 localStorage 持久化
 *
 * [Source: Story 1.3 - 人格自适应色彩系统配置]
 */

import { useCallback, useEffect, useState } from 'react';
import {
  type MBTIType,
  type PersonalityColorConfig,
  personalityColors,
  getPersonalityColors,
} from '@/lib/personality-colors';

/** CSS 变量名称 */
const CSS_VARIABLES = {
  primary: '--personality-primary',
  accent: '--personality-accent',
  secondary: '--personality-secondary',
} as const;

/** localStorage 键名 */
const STORAGE_KEY = 'omninova-personality-theme';

/** Hook 返回类型 */
export interface UsePersonalityThemeReturn {
  /** 当前人格类型 */
  currentType: MBTIType | null;
  /** 当前色彩配置 */
  currentColors: PersonalityColorConfig | null;
  /** 应用人格主题 */
  applyTheme: (type: MBTIType) => void;
  /** 清除当前主题 */
  clearTheme: () => void;
  /** 是否已初始化 */
  isInitialized: boolean;
}

/**
 * 获取 CSS 变量值
 */
function getCSSVariable(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

/**
 * 设置 CSS 变量值
 */
function setCSSVariable(name: string, value: string): void {
  document.documentElement.style.setProperty(name, value);
}

/**
 * 移除 CSS 变量
 */
function removeCSSVariable(name: string): void {
  document.documentElement.style.removeProperty(name);
}

/**
 * 从 localStorage 加载主题
 */
function loadThemeFromStorage(): MBTIType | null {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored && stored in personalityColors) {
      return stored as MBTIType;
    }
  } catch {
    // localStorage 可能不可用（如隐私模式）
    console.warn('Failed to load personality theme from localStorage');
  }
  return null;
}

/**
 * 保存主题到 localStorage
 */
function saveThemeToStorage(type: MBTIType): void {
  try {
    localStorage.setItem(STORAGE_KEY, type);
  } catch {
    console.warn('Failed to save personality theme to localStorage');
  }
}

/**
 * 清除 localStorage 中的主题
 */
function clearThemeFromStorage(): void {
  try {
    localStorage.removeItem(STORAGE_KEY);
  } catch {
    console.warn('Failed to clear personality theme from localStorage');
  }
}

/**
 * 人格主题切换 Hook
 *
 * @param initialType 初始人格类型（可选，优先级低于 localStorage）
 * @returns 主题相关状态和方法
 *
 * @example
 * ```tsx
 * function AgentCard({ agent }) {
 *   const { applyTheme, currentColors } = usePersonalityTheme(agent.mbti_type);
 *
 *   return (
 *     <Card className="border-personality-primary">
 *       <h3 style={{ color: currentColors?.primary }}>{agent.name}</h3>
 *     </Card>
 *   );
 * }
 * ```
 */
export function usePersonalityTheme(
  initialType?: MBTIType
): UsePersonalityThemeReturn {
  const [currentType, setCurrentType] = useState<MBTIType | null>(null);
  const [isInitialized, setIsInitialized] = useState(false);

  // 应用主题到 CSS 变量
  const applyThemeToCSS = useCallback((type: MBTIType) => {
    const colors = getPersonalityColors(type);
    setCSSVariable(CSS_VARIABLES.primary, colors.primary);
    setCSSVariable(CSS_VARIABLES.accent, colors.accent);
    setCSSVariable(CSS_VARIABLES.secondary, colors.secondary);
  }, []);

  // 应用人格主题
  const applyTheme = useCallback(
    (type: MBTIType) => {
      if (!(type in personalityColors)) {
        console.warn(`Invalid MBTI type: ${type}`);
        return;
      }

      applyThemeToCSS(type);
      saveThemeToStorage(type);
      setCurrentType(type);
    },
    [applyThemeToCSS]
  );

  // 清除当前主题
  const clearTheme = useCallback(() => {
    removeCSSVariable(CSS_VARIABLES.primary);
    removeCSSVariable(CSS_VARIABLES.accent);
    removeCSSVariable(CSS_VARIABLES.secondary);
    clearThemeFromStorage();
    setCurrentType(null);
  }, []);

  // 初始化：从 localStorage 或 initialType 加载主题
  useEffect(() => {
    if (isInitialized) return;

    // 优先从 localStorage 加载
    const storedType = loadThemeFromStorage();

    if (storedType) {
      applyThemeToCSS(storedType);
      setCurrentType(storedType);
    } else if (initialType) {
      applyThemeToCSS(initialType);
      setCurrentType(initialType);
    }

    setIsInitialized(true);
  }, [initialType, isInitialized, applyThemeToCSS]);

  // 获取当前色彩配置
  const currentColors = currentType ? getPersonalityColors(currentType) : null;

  return {
    currentType,
    currentColors,
    applyTheme,
    clearTheme,
    isInitialized,
  };
}

/**
 * 直接应用人格主题（非 Hook 版本）
 *
 * 适用于需要在 React 组件外部调用场景
 *
 * @param type MBTI 人格类型
 * @param persist 是否持久化到 localStorage（默认 true）
 */
export function applyPersonalityTheme(type: MBTIType, persist: boolean = true): void {
  if (!(type in personalityColors)) {
    console.warn(`Invalid MBTI type: ${type}`);
    return;
  }

  const colors = getPersonalityColors(type);
  setCSSVariable(CSS_VARIABLES.primary, colors.primary);
  setCSSVariable(CSS_VARIABLES.accent, colors.accent);
  setCSSVariable(CSS_VARIABLES.secondary, colors.secondary);

  if (persist) {
    saveThemeToStorage(type);
  }
}

/**
 * 获取当前应用的人格主题
 */
export function getCurrentPersonalityTheme(): PersonalityColorConfig | null {
  const primary = getCSSVariable(CSS_VARIABLES.primary);

  if (!primary) return null;

  // 通过 primary 颜色反向查找类型
  for (const config of Object.values(personalityColors)) {
    if (config.primary.toLowerCase() === primary.toLowerCase()) {
      return config;
    }
  }

  return null;
}

// 导出类型
export type { MBTIType, PersonalityColorConfig };