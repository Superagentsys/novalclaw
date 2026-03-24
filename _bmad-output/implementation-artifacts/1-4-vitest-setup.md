# Story 1.4: Vitest 单元测试框架配置

Status: done

## Story

As a 开发者,
I want 配置 Vitest 单元测试框架,
so that 我可以为项目组件和功能编写和运行单元测试.

## Acceptance Criteria

1. **Given** React 项目已设置, **When** 我安装并配置 Vitest, **Then** vitest.config.ts 文件被创建并正确配置

2. 测试工具函数和 mock 辅助模块已创建 (src/test/utils.tsx)

3. package.json 包含测试脚本 (test, test:coverage)

4. 示例测试文件可以成功运行

5. 测试覆盖率报告可以生成

## Tasks / Subtasks

- [x] Task 1: 安装 Vitest 和测试依赖 (AC: 1, 3)
  - [x] 安装 vitest、@testing-library/react、@testing-library/jest-dom、jsdom
  - [x] 安装 @vitest/coverage-v8 用于覆盖率报告
  - [x] 更新 package.json 添加测试脚本

- [x] Task 2: 创建 Vitest 配置文件 (AC: 1)
  - [x] 创建 `vitest.config.ts` 配置文件
  - [x] 配置 jsdom 环境
  - [x] 配置 path alias (@/* 映射)
  - [x] 配置 setup 文件路径

- [x] Task 3: 创建测试工具和辅助模块 (AC: 2)
  - [x] 创建 `src/test/setup.ts` 全局设置文件
  - [x] 创建 `src/test/utils.tsx` 自定义 render 函数
  - [x] 配置 @testing-library/jest-dom matchers

- [x] Task 4: 创建示例测试文件 (AC: 4)
  - [x] 创建 `src/lib/utils.test.ts` 工具函数测试
  - [x] 创建 `src/components/ui/button.test.tsx` 组件测试
  - [x] 验证所有测试通过

- [x] Task 5: 配置覆盖率报告 (AC: 5)
  - [x] 配置覆盖率阈值和报告格式
  - [x] 验证覆盖率报告生成成功
  - [x] 运行 `npm run test:coverage` 确认功能正常

## Dev Notes

### 项目架构约束

- **工作目录**: 所有命令在 `apps/omninova-tauri` 目录下执行
- **前端技术栈**: React 19 + TypeScript 5.9 + Vite
- **样式系统**: Tailwind CSS v4 + Shadcn/UI (已配置)
- **测试框架**: Vitest (本 Story 配置)

### 前一个故事的发现 (Story 1.3)

**重要**: Story 1.3 已完成人格色彩系统实现，有以下发现：

1. **TypeScript verbatimModuleSyntax**: 类型需要使用 `import type { ... }` 语法
2. **Path alias 配置**: `@/*` 映射到 `./src/*`，已在 tsconfig.json 和 vite.config.ts 中配置
3. **Shadcn/UI v4**: 使用 `@base-ui/react` 作为无样式原语，CSS 变量使用 oklch 色彩空间
4. **构建验证**: 使用 `npm run build` 验证构建成功

### Vitest 配置要点

**为什么选择 Vitest:**
- Vite 原生支持，配置简单，启动快速
- 与项目现有技术栈完美集成
- 内置 TypeScript 和 JSX 支持
- 兼容 Jest API，迁移成本低

**核心依赖:**
```json
{
  "vitest": "^3.0.0",
  "@testing-library/react": "^16.0.0",
  "@testing-library/jest-dom": "^6.6.0",
  "@vitest/coverage-v8": "^3.0.0",
  "jsdom": "^26.0.0"
}
```

### Vitest 配置文件示例

```typescript
// vitest.config.ts
import { defineConfig } from 'vitest/config'
import react from '@vitejs/plugin-react'
import path from 'path'

export default defineConfig({
  plugins: [react()],
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/test/setup.ts'],
    include: ['src/**/*.{test,spec}.{js,mjs,cjs,ts,mts,cts,jsx,tsx}'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html'],
      exclude: [
        'node_modules/',
        'src/test/',
      ],
    },
  },
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
})
```

### 测试工具文件示例

**setup.ts:**
```typescript
import '@testing-library/jest-dom'
```

**utils.tsx:**
```typescript
import { ReactElement } from 'react'
import { render, RenderOptions } from '@testing-library/react'

// 自定义 render 函数，可以添加 providers
function customRender(
  ui: ReactElement,
  options?: Omit<RenderOptions, 'wrapper'>
) {
  return render(ui, { ...options })
}

// 重新导出所有 testing-library 方法
export * from '@testing-library/react'
export { customRender as render }
```

### package.json 脚本配置

```json
{
  "scripts": {
    "test": "vitest",
    "test:run": "vitest run",
    "test:coverage": "vitest run --coverage"
  }
}
```

### 测试文件结构

```
apps/omninova-tauri/src/
├── test/
│   ├── setup.ts           # 全局设置文件
│   └── utils.tsx          # 测试工具函数
├── lib/
│   ├── utils.ts
│   └── utils.test.ts      # 工具函数测试
└── components/
    └── ui/
        ├── button.tsx
        └── button.test.tsx # 组件测试
```

### 测试命名约定

| 类型 | 规则 | 示例 |
|------|------|------|
| 前端单元测试 | `{name}.test.ts` | `utils.test.ts` |
| 前端组件测试 | `{name}.test.tsx` | `button.test.tsx` |
| 测试文件位置 | 与源文件同目录或 `src/__tests__/` | `src/lib/utils.test.ts` |

### 组件测试最佳实践

```typescript
// button.test.tsx
import { describe, it, expect } from 'vitest'
import { render, screen } from '@/test/utils'
import { Button } from './button'

describe('Button', () => {
  it('renders correctly', () => {
    render(<Button>Click me</Button>)
    expect(screen.getByRole('button', { name: /click me/i })).toBeInTheDocument()
  })

  it('applies variant styles', () => {
    render(<Button variant="destructive">Delete</Button>)
    const button = screen.getByRole('button')
    expect(button).toHaveClass('bg-destructive')
  })
})
```

### 与 Shadcn/UI 组件测试

**注意事项:**
1. Shadcn/UI 组件使用 `@base-ui/react` 原语
2. 某些组件可能需要 mock Tauri API
3. CSS 变量在 jsdom 中可能需要特殊处理

**Tauri API Mock (如需要):**
```typescript
// src/test/__mocks__/tauri.ts
export const invoke = vi.fn()
export const listen = vi.fn()
```

### Testing Standards

- 测试框架: Vitest + React Testing Library
- 测试覆盖率目标: 不设硬性要求，鼓励关键路径覆盖
- 运行测试: `npm run test`
- 验证方式: 确保所有测试通过，覆盖率报告可生成

### Project Structure Notes

**实际文件结构预期:**

```
apps/omninova-tauri/
├── vitest.config.ts          # 新建
├── package.json              # 修改（添加依赖和脚本）
├── src/
│   ├── test/
│   │   ├── setup.ts          # 新建
│   │   └── utils.tsx         # 新建
│   ├── lib/
│   │   └── utils.test.ts     # 新建
│   └── components/
│       └── ui/
│           └── button.test.tsx # 新建
```

### References

- [Source: architecture.md#测试组织] - 测试文件命名和位置
- [Source: architecture.md#Starter Template Evaluation] - Vitest 添加要求
- [Source: epics.md#Story 1.4] - 验收标准
- [Source: 1-3-personality-color-system.md] - 前一个故事的实现记录

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (claude-opus-4-6)

### Debug Log References

1. **TypeScript verbatimModuleSyntax 错误**
   - 构建时报错：`'RenderOptions' is a type and must be imported using a type-only import`
   - 解决方案：将 `RenderOptions` 的导入改为 `import type { RenderOptions }`

### Completion Notes List

1. 完整配置了 Vitest 测试框架，包括：
   - 安装 vitest、@testing-library/react、@testing-library/jest-dom、jsdom、@vitest/coverage-v8
   - 创建 vitest.config.ts 配置文件，配置 jsdom 环境、path alias、setup 文件

2. 创建了测试工具文件：
   - src/test/setup.ts：导入 @testing-library/jest-dom matchers
   - src/test/utils.tsx：自定义 render 函数，方便后续添加 providers

3. 创建了示例测试文件：
   - src/lib/utils.test.ts：测试 cn 工具函数（6 个测试用例）
   - src/components/ui/button.test.tsx：测试 Button 组件（7 个测试用例）

4. 测试结果：
   - 所有 13 个测试通过
   - 覆盖率报告成功生成（100% 覆盖率）

5. 添加的 npm 脚本：
   - `npm run test`：运行 vitest 监听模式
   - `npm run test:run`：运行测试一次
   - `npm run test:coverage`：运行测试并生成覆盖率报告

### File List

**新建文件:**
- `apps/omninova-tauri/vitest.config.ts` - Vitest 配置文件
- `apps/omninova-tauri/src/test/setup.ts` - 全局测试设置文件
- `apps/omninova-tauri/src/test/utils.tsx` - 测试工具函数
- `apps/omninova-tauri/src/lib/utils.test.ts` - utils 工具函数测试
- `apps/omninova-tauri/src/components/ui/button.test.tsx` - Button 组件测试

**修改文件:**
- `apps/omninova-tauri/package.json` - 添加测试依赖和脚本

## Change Log

- 2026-03-15: Story 创建，状态设为 ready-for-dev
- 2026-03-15: Story 实现完成，状态更新为 review
- 2026-03-15: Code review 通过，无问题发现，状态更新为 done