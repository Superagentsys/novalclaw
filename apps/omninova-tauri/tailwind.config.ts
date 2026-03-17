import type { Config } from 'tailwindcss'

const config: Config = {
  content: [
    './index.html',
    './src/**/*.{js,ts,jsx,tsx}',
  ],
  theme: {
    extend: {
      // 响应式断点配置 [Source: ux-design-specification.md#响应式断点]
      screens: {
        'sm': '640px',
        'md': '768px',
        'lg': '1024px',
        'xl': '1280px',
      },
      // 色彩系统 [Source: ux-design-specification.md#色彩系统]
      colors: {
        primary: {
          DEFAULT: '#2563EB',
        },
        accent: {
          DEFAULT: '#0D9488',
        },
        semantic: {
          success: '#22C55E',
          warning: '#F59E0B',
          error: '#EF4444',
          info: '#3B82F6',
        },
        // 人格自适应色彩系统 (Story 1.3)
        // 动态主题色 - 通过 CSS 变量实现运行时切换
        personality: {
          primary: 'var(--personality-primary)',
          accent: 'var(--personality-accent)',
          secondary: 'var(--personality-secondary)',
          // 静态类型色 - 作为 fallback 和直接使用
          // 分析型 (Analysts)
          intj: {
            primary: '#2563EB',
            accent: '#787163',
            secondary: '#1E40AF',
          },
          intp: {
            primary: '#1E40AF',
            accent: '#6B7280',
            secondary: '#1E3A8A',
          },
          entj: {
            primary: '#1E3A8A',
            accent: '#787163',
            secondary: '#172554',
          },
          entp: {
            primary: '#3B82F6',
            accent: '#9CA3AF',
            secondary: '#2563EB',
          },
          // 外交型 (Diplomats)
          infj: {
            primary: '#EA580C',
            accent: '#0D9488',
            secondary: '#C2410C',
          },
          infp: {
            primary: '#F97316',
            accent: '#14B8A6',
            secondary: '#EA580C',
          },
          enfj: {
            primary: '#C2410C',
            accent: '#0D9488',
            secondary: '#EA580C',
          },
          enfp: {
            primary: '#EA580C',
            accent: '#0D9488',
            secondary: '#F97316',
          },
          // 守护型 (Sentinels)
          istj: {
            primary: '#1E3A8A',
            accent: '#374151',
            secondary: '#172554',
          },
          isfj: {
            primary: '#1E40AF',
            accent: '#4B5563',
            secondary: '#1E3A8A',
          },
          estj: {
            primary: '#172554',
            accent: '#374151',
            secondary: '#0F172A',
          },
          esfj: {
            primary: '#1E3A8A',
            accent: '#6B7280',
            secondary: '#1E40AF',
          },
          // 探索型 (Explorers)
          istp: {
            primary: '#7C3AED',
            accent: '#F97316',
            secondary: '#6D28D9',
          },
          isfp: {
            primary: '#8B5CF6',
            accent: '#FB923C',
            secondary: '#7C3AED',
          },
          estp: {
            primary: '#6D28D9',
            accent: '#F97316',
            secondary: '#5B21B6',
          },
          esfp: {
            primary: '#A855F7',
            accent: '#F97316',
            secondary: '#8B5CF6',
          },
        },
      },
      // 排版系统 [Source: ux-design-specification.md#排版系统]
      fontSize: {
        'h1': ['2.5rem', { lineHeight: '1.25' }],
        'h2': ['2rem', { lineHeight: '1.25' }],
        'h3': ['1.75rem', { lineHeight: '1.25' }],
        'h4': ['1.5rem', { lineHeight: '1.25' }],
        'body-lg': ['1.125rem', { lineHeight: '1.5' }],
        'body': ['1rem', { lineHeight: '1.5' }],
        'body-sm': ['0.875rem', { lineHeight: '1.5' }],
        'caption': ['0.75rem', { lineHeight: '1.5' }],
      },
      // 间距系统 [Source: ux-design-specification.md#间距与布局基础]
      // 基础单位 4px，Tailwind 默认支持
      spacing: {
        '18': '4.5rem',
        '22': '5.5rem',
      },
      // 字体栈
      fontFamily: {
        sans: [
          '-apple-system',
          'BlinkMacSystemFont',
          '"Segoe UI"',
          'Roboto',
          'Helvetica',
          'Arial',
          'sans-serif',
        ],
      },
    },
  },
  plugins: [],
}

export default config