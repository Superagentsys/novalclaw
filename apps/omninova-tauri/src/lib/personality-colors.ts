/**
 * MBTI 人格类型色彩配置
 *
 * 根据 MBTI 理论，16 种人格类型分为 4 大类别：
 * - 分析型 (Analysts): INTJ, INTP, ENTJ, ENTP - 深蓝/灰色系，理性冷静
 * - 外交型 (Diplomats): INFJ, INFP, ENFJ, ENFP - 暖橙/青色系，温暖创意
 * - 守护型 (Sentinels): ISTJ, ISFJ, ESTJ, ESFJ - 海军蓝/白色系，稳重可靠
 * - 探索型 (Explorers): ISTP, ISFP, ESTP, ESFP - 紫色/暖色系，活力热情
 *
 * [Source: ux-design-specification.md#色彩系统]
 */

/**
 * MBTI 人格类型
 */
export type MBTIType =
  | 'INTJ' | 'INTP' | 'ENTJ' | 'ENTP'
  | 'INFJ' | 'INFP' | 'ENFJ' | 'ENFP'
  | 'ISTJ' | 'ISFJ' | 'ESTJ' | 'ESFJ'
  | 'ISTP' | 'ISFP' | 'ESTP' | 'ESFP';

/**
 * 人格类型色调风格
 */
export type PersonalityTone = 'analytical' | 'creative' | 'structured' | 'energetic';

/**
 * 人格类型分类
 */
export type PersonalityCategory = 'analysts' | 'diplomats' | 'sentinels' | 'explorers';

/**
 * 人格类型色彩配置
 */
export interface PersonalityColorConfig {
  /** 主色调 */
  primary: string;
  /** 强调色 */
  accent: string;
  /** 次要色 */
  secondary: string;
  /** 色调风格 */
  tone: PersonalityTone;
  /** 分类 */
  category: PersonalityCategory;
  /** 类型名称 */
  name: string;
  /** 简短描述 */
  description: string;
}

/**
 * 16 种 MBTI 类型的完整色彩映射
 */
export const personalityColors: Record<MBTIType, PersonalityColorConfig> = {
  // ===== 分析型 (Analysts) - 深蓝色系 =====
  INTJ: {
    primary: '#2563EB',
    accent: '#787163',
    secondary: '#1E40AF',
    tone: 'analytical',
    category: 'analysts',
    name: 'INTJ',
    description: '建筑师 - 富有想象力的战略家',
  },
  INTP: {
    primary: '#1E40AF',
    accent: '#6B7280',
    secondary: '#1E3A8A',
    tone: 'analytical',
    category: 'analysts',
    name: 'INTP',
    description: '逻辑学家 - 创新的发明家',
  },
  ENTJ: {
    primary: '#1E3A8A',
    accent: '#787163',
    secondary: '#172554',
    tone: 'analytical',
    category: 'analysts',
    name: 'ENTJ',
    description: '指挥官 - 大胆的领导者',
  },
  ENTP: {
    primary: '#3B82F6',
    accent: '#9CA3AF',
    secondary: '#2563EB',
    tone: 'analytical',
    category: 'analysts',
    name: 'ENTP',
    description: '辩论家 - 聪明的探索者',
  },

  // ===== 外交型 (Diplomats) - 暖橙色系 =====
  INFJ: {
    primary: '#EA580C',
    accent: '#0D9488',
    secondary: '#C2410C',
    tone: 'creative',
    category: 'diplomats',
    name: 'INFJ',
    description: '提倡者 - 安静的理想主义者',
  },
  INFP: {
    primary: '#F97316',
    accent: '#14B8A6',
    secondary: '#EA580C',
    tone: 'creative',
    category: 'diplomats',
    name: 'INFP',
    description: '调停者 - 诗意的理想主义者',
  },
  ENFJ: {
    primary: '#C2410C',
    accent: '#0D9488',
    secondary: '#EA580C',
    tone: 'creative',
    category: 'diplomats',
    name: 'ENFJ',
    description: '主人公 - 富有魅力的领导者',
  },
  ENFP: {
    primary: '#EA580C',
    accent: '#0D9488',
    secondary: '#F97316',
    tone: 'creative',
    category: 'diplomats',
    name: 'ENFP',
    description: '竞选者 - 热情的探索者',
  },

  // ===== 守护型 (Sentinels) - 海军蓝色系 =====
  ISTJ: {
    primary: '#1E3A8A',
    accent: '#374151',
    secondary: '#172554',
    tone: 'structured',
    category: 'sentinels',
    name: 'ISTJ',
    description: '物流师 - 可靠的实干家',
  },
  ISFJ: {
    primary: '#1E40AF',
    accent: '#4B5563',
    secondary: '#1E3A8A',
    tone: 'structured',
    category: 'sentinels',
    name: 'ISFJ',
    description: '守卫者 - 忠诚的保护者',
  },
  ESTJ: {
    primary: '#172554',
    accent: '#374151',
    secondary: '#0F172A',
    tone: 'structured',
    category: 'sentinels',
    name: 'ESTJ',
    description: '总经理 - 高效的管理者',
  },
  ESFJ: {
    primary: '#1E3A8A',
    accent: '#6B7280',
    secondary: '#1E40AF',
    tone: 'structured',
    category: 'sentinels',
    name: 'ESFJ',
    description: '执政官 - 热心的助人者',
  },

  // ===== 探索型 (Explorers) - 紫色系 =====
  ISTP: {
    primary: '#7C3AED',
    accent: '#F97316',
    secondary: '#6D28D9',
    tone: 'energetic',
    category: 'explorers',
    name: 'ISTP',
    description: '鉴赏家 - 大胆的实验家',
  },
  ISFP: {
    primary: '#8B5CF6',
    accent: '#FB923C',
    secondary: '#7C3AED',
    tone: 'energetic',
    category: 'explorers',
    name: 'ISFP',
    description: '探险家 - 灵活的艺术家',
  },
  ESTP: {
    primary: '#6D28D9',
    accent: '#F97316',
    secondary: '#5B21B6',
    tone: 'energetic',
    category: 'explorers',
    name: 'ESTP',
    description: '企业家 - 精明的冒险家',
  },
  ESFP: {
    primary: '#A855F7',
    accent: '#F97316',
    secondary: '#8B5CF6',
    tone: 'energetic',
    category: 'explorers',
    name: 'ESFP',
    description: '表演者 - 自发的娱乐者',
  },
};

/**
 * 获取人格类型的色彩配置
 */
export function getPersonalityColors(type: MBTIType): PersonalityColorConfig {
  return personalityColors[type];
}

/**
 * 获取分类下的所有类型
 */
export function getTypesByCategory(category: PersonalityCategory): MBTIType[] {
  return Object.entries(personalityColors)
    .filter(([, config]) => config.category === category)
    .map(([type]) => type as MBTIType);
}

/**
 * 所有 MBTI 类型列表
 */
export const allMBTITypes: MBTIType[] = Object.keys(personalityColors) as MBTIType[];

/**
 * 分类信息
 */
export const personalityCategories: Record<PersonalityCategory, { name: string; description: string }> = {
  analysts: {
    name: '分析型',
    description: '深蓝/灰色系，理性冷静',
  },
  diplomats: {
    name: '外交型',
    description: '暖橙/青色系，温暖创意',
  },
  sentinels: {
    name: '守护型',
    description: '海军蓝/白色系，稳重可靠',
  },
  explorers: {
    name: '探索型',
    description: '紫色/暖色系，活力热情',
  },
};