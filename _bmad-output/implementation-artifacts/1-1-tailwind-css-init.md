# Story 1.1: Tailwind CSS 样式系统初始化

Status: done

## Story

As a 开发者,
I want 初始化并配置 Tailwind CSS 样式系统,
so that 我可以拥有一个统一的、可定制的设计基础来构建用户界面.

## Acceptance Criteria

1. **Given** Tauri + React 项目已创建, **When** 我执行 Tailwind CSS 初始化命令, **Then** tailwind.config.js 文件被创建并包含正确的 content 路径配置 ✅

2. tailwind.config.js 包含响应式断点配置（sm: 640px, md: 768px, lg: 1024px, xl: 1280px） ✅

3. 基础设计 tokens 已定义（颜色、间距、排版 scale） ✅

4. 全局 CSS 文件正确引入 Tailwind directives ✅

## Tasks / Subtasks

- [x] Task 1: 安装 Tailwind CSS 依赖 (AC: 1)
  - [x] 安装 tailwindcss, postcss, autoprefixer 作为开发依赖
  - [x] 验证依赖版本兼容性（React 19, TypeScript 5.9）

- [x] Task 2: 生成 Tailwind 配置文件 (AC: 1, 2)
  - [x] 执行初始化配置（使用 Tailwind v4 + @tailwindcss/vite 插件）
  - [x] 配置 content 路径指向 `./src/**/*.{js,ts,jsx,tsx}`
  - [x] 添加响应式断点配置（sm: 640px, md: 768px, lg: 1024px, xl: 1280px）

- [x] Task 3: 定义基础设计 tokens (AC: 3)
  - [x] 扩展颜色系统（主蓝色 #2563EB, 中性灰, 强调青色 #0D9488）
  - [x] 配置间距系统（基础单位 4px）
  - [x] 配置排版 scale（h1-h4, body sizes）
  - [x] 添加语义色彩（success, warning, error, info）
  - [x] 添加人格自适应色彩占位（INTJ, ENFP, ISTJ, ESFP）

- [x] Task 4: 配置全局 CSS (AC: 4)
  - [x] 在 src/index.css 中添加 `@import 'tailwindcss'` 指令
  - [x] 更新 vite.config.ts 添加 @tailwindcss/vite 插件

- [x] Task 5: 验证配置 (AC: 1-4)
  - [x] 运行 `npm run build` 验证构建成功
  - [x] 验证 Tailwind CSS 变量正确输出到构建产物
  - [x] 验证配置文件内容正确

## Dev Notes

### 项目架构约束

- **工作目录**: 所有命令在 `apps/omninova-tauri` 目录下执行
- **前端技术栈**: React 19 + TypeScript 5.9 + Vite
- **设计系统**: Shadcn/UI + Tailwind CSS（本故事仅完成 Tailwind 部分）
- **Tauri 版本**: 2.x

### 初始化命令 [Source: architecture.md#Gap Analysis]

```bash
# 在 apps/omninova-tauri 目录下
cd apps/omninova-tauri
npm install -D tailwindcss postcss autoprefixer
npx tailwindcss init -p
```

### 响应式断点配置 [Source: ux-design-specification.md#响应式断点]

```
sm: 640px   # 移动端小屏
md: 768px   # 平板/移动端
lg: 1024px  # 平板横屏/小桌面
xl: 1280px  # 桌面端
```

### 色彩系统基础 [Source: ux-design-specification.md#色彩系统]

**主色调:**
- 主蓝色: #2563EB（主要操作和可信赖的交互）
- 中性灰: Gray-100 到 Gray-900（背景和文本层次）
- 强调青色: #0D9488（正面反馈和成功状态）

**语义色彩:**
- 成功: #22C55E (Green-500)
- 警告: #F59E0B (Amber-500)
- 错误: #EF4444 (Red-500)
- 信息: #3B82F6 (Blue-500)

### 排版系统 [Source: ux-design-specification.md#排版系统]

**类型比例:**
- h1: 2.5rem (40px)
- h2: 2rem (32px)
- h3: 1.75rem (28px)
- h4: 1.5rem (24px)
- 正文大: 1.125rem (18px)
- 正文常规: 1rem (16px)
- 正文小: 0.875rem (14px)
- 说明文字: 0.75rem (12px)

**行高:** 正文 1.5, 标题 1.25

### 间距系统 [Source: ux-design-specification.md#间距与布局基础]

- 基础单位: 4px
- 常见倍数: 8px, 12px, 16px, 24px, 32px, 40px, 48px, 64px

### Project Structure Notes

**实际文件结构:**

```
apps/omninova-tauri/
├── tailwind.config.ts    # 新建 (TypeScript 配置)
├── vite.config.ts        # 修改（添加 tailwindcss 插件）
├── src/
│   └── index.css         # 修改（添加 @import 'tailwindcss'）
```

**注意:** 使用 Tailwind CSS v4，不需要 postcss.config.js 文件，而是使用 @tailwindcss/vite 插件。

### Testing Standards

- 本项目使用 Vitest 进行单元测试（Story 1.4 配置）
- 本故事无需编写测试文件
- 验证方式：运行 `npm run build` 检查构建成功

### References

- [Source: architecture.md#Gap Analysis] - Tailwind 初始化命令
- [Source: architecture.md#项目结构] - 文件路径配置
- [Source: ux-design-specification.md#色彩系统] - 颜色 tokens
- [Source: ux-design-specification.md#排版系统] - 字体 scale
- [Source: ux-design-specification.md#间距与布局基础] - 间距系统
- [Source: ux-design-specification.md#响应式断点] - 断点配置

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (glm-5)

### Debug Log References

无

### Completion Notes List

1. **Tailwind CSS v4 集成方式变更**: 原计划使用 `npx tailwindcss init -p` 生成配置文件，但 Tailwind v4 推荐使用 `@tailwindcss/vite` 插件集成方式，更简洁高效。

2. **配置文件格式**: 使用 TypeScript 格式 (`tailwind.config.ts`) 而非 JavaScript 格式，与项目整体 TypeScript 配置保持一致。

3. **构建验证**: `npm run build` 成功，生成的 CSS 文件 (34.87 kB) 包含所有 Tailwind CSS 变量和主题配置。

4. **依赖版本**:
   - tailwindcss@4.2.1
   - postcss@8.5.8
   - autoprefixer@10.4.27
   - @tailwindcss/vite (新增)

### File List

**新增文件:**
- `apps/omninova-tauri/tailwind.config.ts` - Tailwind 配置文件

**修改文件:**
- `apps/omninova-tauri/vite.config.ts` - 添加 tailwindcss 插件
- `apps/omninova-tauri/src/index.css` - 添加 Tailwind 导入
- `apps/omninova-tauri/package.json` - 添加 tailwindcss 相关依赖 (自动更新)
- `apps/omninova-tauri/package-lock.json` - 依赖锁定文件 (自动更新)
- `.gitignore` - 添加 AI 相关和临时文件忽略规则

## Change Log

- 2026-03-15: 完成 Tailwind CSS v4 样式系统初始化，所有验收标准满足
- 2026-03-15: Code Review 通过，修复 File List 文档完整性问题

## Senior Developer Review (AI)

**Review Date:** 2026-03-15
**Review Outcome:** ✅ Approved
**Reviewer Model:** Claude Opus 4.6 (glm-5)

### Action Items

- [x] [MEDIUM] 更新 File List 记录 .gitignore 修改

### Review Summary

所有验收标准已正确实现，代码质量良好。Tailwind CSS v4 集成方式选择合理，配置文件结构清晰。构建验证通过，无安全问题发现。