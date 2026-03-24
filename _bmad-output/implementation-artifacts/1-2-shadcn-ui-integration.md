# Story 1.2: Shadcn/UI 组件库集成

Status: done

## Story

As a 开发者,
I want 集成 Shadcn/UI 组件库,
so that 我可以使用预构建的高质量 UI 组件快速构建界面.

## Acceptance Criteria

1. **Given** Tailwind CSS 已初始化, **When** 我执行 Shadcn/UI 初始化命令, **Then** components.json 配置文件被创建

2. 核心组件已安装（Button, Input, Card, Dialog, Select, Tabs）

3. 组件可以在 React 应用中正确导入和使用

4. 主题系统基础已配置（支持 light/dark 模式变量）

## Tasks / Subtasks

- [x] Task 1: 初始化 Shadcn/UI (AC: 1)
  - [x] 执行 `npx shadcn@latest init` 命令
  - [x] 配置选项：TypeScript, base-nova 样式, CSS变量主题
  - [x] 验证 components.json 配置文件生成

- [x] Task 2: 安装核心组件 (AC: 2)
  - [x] 安装 Button 组件：`npx shadcn@latest add button`
  - [x] 安装 Input 组件：`npx shadcn@latest add input`
  - [x] 安装 Card 组件：`npx shadcn@latest add card`
  - [x] 安装 Dialog 组件：`npx shadcn@latest add dialog`
  - [x] 安装 Select 组件：`npx shadcn@latest add select`
  - [x] 安装 Tabs 组件：`npx shadcn@latest add tabs`
  - [x] 安装 Sonner 组件（替代已废弃的 toast）：`npx shadcn@latest add sonner`
  - [x] 安装 Skeleton 组件：`npx shadcn@latest add skeleton`

- [x] Task 3: 配置主题系统 (AC: 4)
  - [x] 验证 CSS 变量已正确配置（--background, --foreground 等）
  - [x] 配置 dark mode 支持（.dark 类）
  - [x] 更新 index.css 包含 Shadcn/UI 的 CSS 变量（oklch 色彩空间）

- [x] Task 4: 验证组件功能 (AC: 3)
  - [x] 创建统一的组件导出文件 src/components/ui/index.ts
  - [x] 修复 Sonner 导出名称问题（Toaster vs Sonner）
  - [x] 运行 `npm run build` 确保构建成功

## Dev Notes

### 项目架构约束

- **工作目录**: 所有命令在 `apps/omninova-tauri` 目录下执行
- **前端技术栈**: React 19 + TypeScript 5.9 + Vite
- **样式系统**: Tailwind CSS v4 (已在 Story 1.1 配置)
- **Tauri 版本**: 2.x

### 前一个故事的发现 (Story 1.1)

**重要**: Story 1.1 已完成 Tailwind CSS v4 集成，有以下发现：

1. **Tailwind v4 集成方式**: 使用 `@tailwindcss/vite` 插件而非传统 postcss 配置
2. **配置文件格式**: TypeScript (`tailwind.config.ts`)
3. **已安装的依赖**:
   - tailwindcss@4.2.1
   - postcss@8.5.8
   - autoprefixer@10.4.27
   - @tailwindcss/vite

### Shadcn/UI 初始化注意事项

**兼容性检查**: Shadcn/UI 需要与 Tailwind CSS v4 兼容。如果遇到兼容性问题，可能需要调整配置。

**初始化命令选项**:
```
# 在 apps/omninova-tauri 目录下
cd apps/omninova-tauri
npx shadcn@latest init
```

**预期配置选项**:
- Style: Default
- Base color: Slate (或 Neutral)
- CSS variables: Yes
- Tailwind config: tailwind.config.ts
- Components location: src/components/ui

### 组件位置

根据架构规范 [Source: architecture.md#前端架构]，组件应放置在：

```
apps/omninova-tauri/src/
├── components/
│   └── ui/                    # Shadcn/UI 基础组件
│       ├── button.tsx
│       ├── card.tsx
│       ├── dialog.tsx
│       ├── input.tsx
│       ├── select.tsx
│       ├── tabs.tsx
│       ├── toast.tsx
│       ├── skeleton.tsx
│       └── index.ts           # 统一导出
```

### 主题系统配置

**CSS 变量结构** [Source: ux-design-specification.md#色彩系统]:

```css
:root {
  --background: 0 0% 100%;
  --foreground: 222.2 84% 4.9%;
  --primary: 221.2 83.2% 53.3%;  /* #2563EB */
  --secondary: 210 40% 96.1%;
  --muted: 210 40% 96.1%;
  --accent: 172 66% 50%;         /* #0D9488 */
  --destructive: 0 84.2% 60.2%;  /* #EF4444 */
  --border: 214.3 31.8% 91.4%;
  --ring: 221.2 83.2% 53.3%;
}

.dark {
  --background: 222.2 84% 4.9%;
  --foreground: 210 40% 98%;
  /* ... dark mode values */
}
```

**人格自适应主题** [Source: ux-design-specification.md#人格自适应主题]:
- 本故事仅配置基础主题系统
- 人格自适应色彩已在 tailwind.config.ts 中预定义
- Story 1.3 将实现完整的人格自适应主题切换

### Testing Standards

- 本项目使用 Vitest 进行单元测试（Story 1.4 配置）
- 本故事无需编写测试文件
- 验证方式：运行 `npm run build` 检查构建成功

### Project Structure Notes

**实际文件结构预期**:

```
apps/omninova-tauri/
├── components.json             # 新建 (Shadcn/UI 配置)
├── src/
│   ├── index.css              # 修改（添加 CSS 变量）
│   ├── lib/
│   │   └── utils.ts           # 新建（Shadcn/UI 工具函数）
│   └── components/
│       └── ui/                # 新建目录
│           ├── button.tsx
│           ├── card.tsx
│           ├── dialog.tsx
│           ├── input.tsx
│           ├── select.tsx
│           ├── tabs.tsx
│           ├── toast.tsx
│           ├── skeleton.tsx
│           └── index.ts
```

### References

- [Source: architecture.md#Starter Template Evaluation] - Shadcn/UI 初始化命令
- [Source: architecture.md#前端架构] - 组件结构和 CSS 变量配置
- [Source: architecture.md#项目结构] - 文件路径配置
- [Source: ux-design-specification.md#色彩系统] - 主题色彩配置
- [Source: ux-design-specification.md#设计系统基础] - Shadcn/UI + Tailwind CSS 设计系统
- [Source: epics.md#Story 1.2] - 验收标准
- [Source: 1-1-tailwind-css-init.md] - 前一个故事的实现记录

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (claude-opus-4-6)

### Debug Log References

1. **TypeScript path alias 配置问题**
   - 初始运行 `npx shadcn@latest init` 失败，报错 "No import alias found in your tsconfig.json file"
   - 解决方案：在 tsconfig.json 和 tsconfig.app.json 中添加 `baseUrl` 和 `paths` 配置
   - 同时需要在 vite.config.ts 中添加 `resolve.alias` 配置

2. **Toast 组件已废弃**
   - 运行 `npx shadcn@latest add toast` 时提示组件已废弃
   - 解决方案：使用 `npx shadcn@latest add sonner` 替代

3. **Sonner 导出名称不匹配**
   - index.ts 自动生成 `export { Sonner } from './sonner'`
   - 但 sonner.tsx 实际导出的是 `Toaster`
   - 解决方案：修改 index.ts 为 `export { Toaster } from './sonner'`

### Completion Notes List

1. Shadcn/UI v4 使用 `base-nova` 样式，与 Tailwind CSS v4 完全兼容
2. CSS 变量使用 oklch 色彩空间，提供更好的色彩感知一致性
3. Dark mode 通过 `.dark` 类实现，配合 `@custom-variant dark (&:is(.dark *));` CSS 规则
4. 组件使用 `@base-ui/react` 作为无样式原语（而非 Radix UI）
5. 需要安装额外依赖：clsx, tailwind-merge, class-variance-authority

### File List

**新建文件:**
- `apps/omninova-tauri/components.json` - Shadcn/UI 配置文件
- `apps/omninova-tauri/src/lib/utils.ts` - cn() 工具函数
- `apps/omninova-tauri/src/components/ui/button.tsx` - Button 组件
- `apps/omninova-tauri/src/components/ui/input.tsx` - Input 组件
- `apps/omninova-tauri/src/components/ui/card.tsx` - Card 组件
- `apps/omninova-tauri/src/components/ui/dialog.tsx` - Dialog 组件
- `apps/omninova-tauri/src/components/ui/select.tsx` - Select 组件
- `apps/omninova-tauri/src/components/ui/tabs.tsx` - Tabs 组件
- `apps/omninova-tauri/src/components/ui/skeleton.tsx` - Skeleton 组件
- `apps/omninova-tauri/src/components/ui/sonner.tsx` - Toaster 组件（Sonner 库）
- `apps/omninova-tauri/src/components/ui/index.ts` - 统一导出文件

**修改文件:**
- `apps/omninova-tauri/tsconfig.json` - 添加 path alias 配置
- `apps/omninova-tauri/tsconfig.app.json` - 添加 baseUrl 和 paths 配置
- `apps/omninova-tauri/vite.config.ts` - 添加 resolve.alias 配置
- `apps/omninova-tauri/src/index.css` - 添加 Shadcn/UI CSS 变量（oklch）
- `apps/omninova-tauri/package.json` - 添加依赖（shadcn 安装自动更新）
- `apps/omninova-tauri/package-lock.json` - 依赖锁文件更新

## Change Log

- 2026-03-15: Story 创建，状态设为 ready-for-dev
- 2026-03-15: Story 实现完成，状态更新为 review
- 2026-03-15: 代码审查通过，修复 dialog.tsx 中的 "use client" 指令，状态更新为 done

## Code Review Record

### Review Date: 2026-03-15

### Reviewer: Claude Opus 4.6

### Findings

| 级别 | 问题 | 状态 |
|------|------|------|
| LOW | dialog.tsx 包含不必要的 "use client" 指令（Next.js RSC 指令，Tauri 应用不需要） | ✅ 已修复 |

### Fixes Applied

1. 删除 `src/components/ui/dialog.tsx` 中的 `"use client"` 指令

### Verification

- `npm run build` 构建成功
- 所有 AC 验证通过