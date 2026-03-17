/**
 * Agent 组件模块导出
 *
 * 包含所有与 AI 代理相关的组件和类型
 */

// 组件导出
export { MBTISelector } from './MBTISelector';
export type { MBTISelectorProps } from './MBTISelector';

export { PersonalityPreview } from './PersonalityPreview';
export type { PersonalityPreviewProps } from './PersonalityPreview';

export { AgentCreateForm } from './AgentCreateForm';
export type { AgentCreateFormProps } from './AgentCreateForm';

export { AgentStatusBadge } from './AgentStatusBadge';
export type { AgentStatusBadgeProps } from './AgentStatusBadge';

export { AgentCard } from './AgentCard';
export type { AgentCardProps } from './AgentCard';

export { AgentList } from './AgentList';
export type { AgentListProps } from './AgentList';

export { AgentEditForm } from './AgentEditForm';
export type { AgentEditFormProps } from './AgentEditForm';

// 统一类型导出（从 types/agent.ts）
export type {
  AgentModel,
  AgentStatus,
  NewAgent,
  AgentUpdate,
} from '@/types/agent';