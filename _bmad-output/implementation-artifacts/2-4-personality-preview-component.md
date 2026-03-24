# Story 2.4: 人格预览组件

Status: done

## Story

As a 用户,
I want 预览所选人格类型的行为特征,
so that 我可以确认这是我希望 AI 代理具有的人格特征.

## Acceptance Criteria

1. **Given** 用户已选择一个 MBTI 类型, **When** 我查看 PersonalityPreview 组件, **Then** 组件显示该类型的详细特征描述

2. **Given** 人格特征已显示, **When** 我查看示例对话部分, **Then** 显示示例对话响应风格

3. **Given** 人格预览已加载, **When** 我查看优势部分, **Then** 显示该类型的优势和潜在盲点

4. **Given** 人格预览已加载, **When** 我查看应用场景部分, **Then** 显示建议的应用场景

## Tasks / Subtasks

- [x] Task 1: 创建 PersonalityPreview 组件文件 (AC: 1)
  - [x] 创建 `apps/omninova-tauri/src/components/agent/PersonalityPreview.tsx` 文件
  - [x] 定义 `PersonalityPreviewProps` 接口（mbtiType, className）
  - [x] 使用 Tauri `invoke` 调用 `get_mbti_config` 获取完整配置
  - [x] 实现加载状态和错误处理
  - [x] 显示人格类型名称和描述

- [x] Task 2: 实现认知功能栈显示 (AC: 1)
  - [x] 调用 `get_mbti_traits` 获取人格特征
  - [x] 显示功能栈（主导、辅助、第三、劣势功能）
  - [x] 使用可视化方式展示认知功能顺序
  - [x] 添加功能图标或颜色标识

- [x] Task 3: 实现示例对话响应风格展示 (AC: 2)
  - [x] 创建示例对话展示区域
  - [x] 根据人格类型生成或展示预设的示例对话
  - [x] 使用对应人格类型的主题色样式
  - [x] 展示该类型的沟通风格特点

- [x] Task 4: 实现优势和盲点展示 (AC: 3)
  - [x] 创建优势列表展示区域
  - [x] 创建潜在盲点列表展示区域
  - [x] 使用不同的视觉样式区分优势和盲点（如不同图标或颜色）
  - [x] 实现 `PersonalityConfig.strengths` 和 `PersonalityConfig.blind_spots` 数据渲染

- [x] Task 5: 实现应用场景推荐 (AC: 4)
  - [x] 创建应用场景展示区域
  - [x] 渲染 `PersonalityConfig.recommended_use_cases` 数据
  - [x] 使用标签或卡片样式展示每个场景
  - [x] 添加场景图标增强视觉效果

- [x] Task 6: 增强视觉设计 (AC: All)
  - [x] 使用人格类型对应的主题色作为组件主色调
  - [x] 实现 hover 和过渡动画效果
  - [x] 确保响应式布局适配
  - [x] 复用 `PersonalityIndicator` 组件作为头部展示

- [x] Task 7: 添加单元测试 (AC: All)
  - [x] 测试组件加载状态
  - [x] 测试配置数据正确渲染
  - [x] 测试错误处理
  - [x] 测试不同人格类型的显示

- [x] Task 8: 文档和导出 (AC: All)
  - [x] 添加组件 JSDoc 注释
  - [x] 更新 `components/agent/index.ts` 导出
  - [x] 运行 `npm run lint` 确保无警告（组件本身无警告）

## Dev Notes

### 前置依赖

**Story 2-2 已完成：**
- `MbtiType` 枚举定义了 16 种人格类型
- `PersonalityConfig` 结构体包含完整配置数据
- `PersonalityTraits` 结构体包含认知功能和行为倾向
- Tauri 命令已暴露：`get_mbti_types`, `get_mbti_traits`, `get_mbti_config`

**Story 2-3 已完成：**
- `MBTISelector` 组件已实现类型选择功能
- 本组件将作为选择器的预览面板使用

### 现有组件复用

**PersonalityIndicator 组件：**
- 位于 `apps/omninova-tauri/src/components/ui/personality-indicator.tsx`
- 支持多种变体：badge, chip, card, minimal
- 可作为预览组件的头部展示

```tsx
// 使用示例
<PersonalityIndicator
  type={mbtiType}
  variant="card"
  size="lg"
  showDescription
  showCategory
/>
```

### Tauri 命令接口

**获取人格配置：**
```typescript
interface PersonalityConfig {
  description: string;
  system_prompt_template: string;
  strengths: string[];
  blind_spots: string[];
  recommended_use_cases: string[];
  theme_color: string;
  accent_color: string;
}

const config = await invoke<PersonalityConfig>('get_mbti_config', {
  mbtiType: 'INTJ'
});
```

**获取人格特征：**
```typescript
interface PersonalityTraits {
  function_stack: FunctionStack;
  behavior_tendency: BehaviorTendency;
  communication_style: CommunicationStyle;
}

interface FunctionStack {
  dominant: CognitiveFunction;
  auxiliary: CognitiveFunction;
  tertiary: CognitiveFunction;
  inferior: CognitiveFunction;
}

interface BehaviorTendency {
  decision_making: string;
  information_processing: string;
  social_interaction: string;
  stress_response: string;
}

interface CommunicationStyle {
  preference: string;
  language_traits: string[];
  feedback_style: string;
}

const traits = await invoke<PersonalityTraits>('get_mbti_traits', {
  mbtiType: 'INTJ'
});
```

### 组件设计建议

**布局结构：**
```
┌─────────────────────────────────────────┐
│  [类型徽章] INTJ - 建筑师               │
│  分析型 · 战略思维                      │
├─────────────────────────────────────────┤
│  认知功能栈                             │
│  ┌──────┬──────┬──────┬──────┐         │
│  │ Ni   │ Te   │ Fi   │ Se   │         │
│  │ 主导 │ 辅助 │ 第三 │ 劣势 │         │
│  └──────┴──────┴──────┴──────┘         │
├─────────────────────────────────────────┤
│  示例对话风格                           │
│  "基于您的需求，我建议采用..."          │
│  (直接、结构化、注重效率)               │
├─────────────────────────────────────────┤
│  优势                                   │
│  ✓ 战略思维  ✓ 独立判断  ✓ 意志坚定    │
├─────────────────────────────────────────┤
│  潜在盲点                               │
│  ⚠ 可能过于傲慢  ⚠ 可能缺乏耐心        │
├─────────────────────────────────────────┤
│  建议应用场景                           │
│  [战略规划] [技术架构设计] [系统分析]   │
└─────────────────────────────────────────┘
```

### 认知功能图标建议

```typescript
const functionIcons: Record<CognitiveFunction, string> = {
  Ni: '🔮', // 内倾直觉 - 洞察
  Ne: '💡', // 外倾直觉 - 创意
  Si: '📚', // 内倾感觉 - 经验
  Se: '🎯', // 外倾感觉 - 行动
  Ti: '🧠', // 内倾思考 - 分析
  Te: '📊', // 外倾思考 - 效率
  Fi: '💚', // 内倾情感 - 价值
  Fe: '🤝', // 外倾情感 - 和谐
};
```

### 示例对话生成策略

可以根据人格类型的 `CommunicationStyle` 和 `system_prompt_template` 生成示例对话：

1. **分析型 (INTJ, INTP, ENTJ, ENTP)**：展示逻辑性强、直接、注重效率的回应
2. **外交型 (INFJ, INFP, ENFJ, ENFP)**：展示富有同理心、关注人际的回应
3. **守护型 (ISTJ, ISFJ, ESTJ, ESFJ)**：展示注重细节、可靠的回应
4. **探索型 (ISTP, ISFP, ESTP, ESFP)**：展示灵活、实用的回应

### 项目架构约束

- **组件位置**: `apps/omninova-tauri/src/components/agent/PersonalityPreview.tsx`
- **样式系统**: Tailwind CSS + Shadcn/UI
- **状态管理**: React useState/useEffect（组件内部状态）
- **命名约定**:
  - 组件文件: PascalCase (`PersonalityPreview.tsx`)
  - Props 接口: 组件名 + Props (`PersonalityPreviewProps`)
  - CSS 类: kebab-case via Tailwind

### 可访问性要求

- WCAG 2.1 AA 颜色对比度标准
- 使用语义化 HTML 结构
- 添加适当的 aria-label
- 键盘可访问的交互元素

### 测试策略

**单元测试（Vitest）：**
- 组件渲染测试
- 加载状态测试
- 错误处理测试
- 不同人格类型渲染测试

**测试文件位置：**
- `apps/omninova-tauri/src/test/components/PersonalityPreview.test.tsx`

### 依赖项

无需添加新的依赖项，现有依赖已足够：
- React 19
- Tailwind CSS
- Shadcn/UI 组件
- @tauri-apps/api

### References

- [Source: epics.md#Story 2.4] - 验收标准
- [Source: architecture.md#前端组件位置] - 组件位置规范
- [Source: ux-design-specification.md#核心组件] - UX 设计要求
- [Source: ux-design-specification.md#色彩系统] - 人格自适应色彩
- [Source: 2-2-mbti-personality-system.md] - Tauri 命令接口和数据结构
- [Source: 2-3-mbti-selector-component.md] - 选择器组件参考
- [Source: personality-indicator.tsx] - 现有组件复用

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (claude-opus-4-6)

### Debug Log References

N/A

### Completion Notes List

**实现完成日期:** 2026-03-16

**主要决策:**
1. 使用 `Promise.all` 并行请求 `get_mbti_config` 和 `get_mbti_traits` 提高加载效率
2. 为 16 种人格类型预定义了示例对话内容，便于展示各类型的沟通风格
3. 使用 emoji 图标表示 8 种认知功能，提升视觉辨识度
4. 创建 `TraitList` 子组件统一渲染优势和盲点列表
5. 创建 `FunctionStackItem` 子组件展示认知功能栈

**测试覆盖:**
- 22 个单元测试全部通过
- 测试文件: `apps/omninova-tauri/src/test/components/PersonalityPreview.test.tsx`
- 覆盖: 加载状态、错误处理、基础渲染、认知功能栈、示例对话、优势盲点、应用场景、视觉设计、不同人格类型

**代码质量:**
- 组件通过 lint 检查（无新增 lint 错误）
- 包含完整的 JSDoc 注释
- 遵循项目命名和架构规范
- 响应式布局适配 (grid-cols-1 md:grid-cols-2)

**实现特点:**
- 加载状态显示 Loader2 旋转动画
- 错误状态显示错误信息和重试按钮
- 复用 `PersonalityIndicator` 组件作为头部展示
- 使用人格类型主题色作为组件主色调
- hover 状态有 scale-105 过渡动画

### File List

**新建文件:**
- `apps/omninova-tauri/src/components/agent/PersonalityPreview.tsx` - 人格预览组件
- `apps/omninova-tauri/src/test/components/PersonalityPreview.test.tsx` - 单元测试（22 个测试用例）

**修改文件:**
- `apps/omninova-tauri/src/components/agent/index.ts` - 添加组件导出