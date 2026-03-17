/**
 * AgentList 组件单元测试
 *
 * 测试覆盖:
 * - 列表渲染
 * - 空状态显示
 * - 加载状态（骨架屏）
 * - 点击导航行为
 * - 响应式布局
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '../utils';
import { AgentList, type AgentModel } from '@/components/agent/AgentList';
import { type MBTIType } from '@/lib/personality-colors';

// Mock data
const createMockAgent = (id: number, overrides?: Partial<AgentModel>): AgentModel => ({
  id,
  agent_uuid: `uuid-${id}`,
  name: `代理 ${id}`,
  description: `这是代理 ${id} 的描述`,
  domain: '代码审查',
  mbti_type: 'INTJ' as MBTIType,
  status: 'active',
  created_at: Date.now(),
  updated_at: Date.now(),
  ...overrides,
});

describe('AgentList', () => {
  describe('列表渲染', () => {
    it('应该渲染代理列表', () => {
      const agents = [
        createMockAgent(1),
        createMockAgent(2),
        createMockAgent(3),
      ];

      render(<AgentList agents={agents} />);

      expect(screen.getByText('代理 1')).toBeInTheDocument();
      expect(screen.getByText('代理 2')).toBeInTheDocument();
      expect(screen.getByText('代理 3')).toBeInTheDocument();
    });

    it('应该使用网格布局', () => {
      const agents = [createMockAgent(1)];
      const { container } = render(<AgentList agents={agents} />);

      expect(container.firstChild).toHaveClass('grid');
    });

    it('应该渲染响应式网格', () => {
      const agents = [createMockAgent(1)];
      const { container } = render(<AgentList agents={agents} />);

      expect(container.firstChild).toHaveClass('grid-cols-1');
      expect(container.firstChild).toHaveClass('md:grid-cols-2');
      expect(container.firstChild).toHaveClass('lg:grid-cols-3');
    });
  });

  describe('空状态', () => {
    it('应该显示空状态提示', () => {
      render(<AgentList agents={[]} />);

      expect(screen.getByText('还没有创建代理')).toBeInTheDocument();
    });

    it('应该显示创建引导', () => {
      render(<AgentList agents={[]} />);

      expect(screen.getByText('创建你的第一个 AI 代理开始对话')).toBeInTheDocument();
    });

    it('有 onCreateAgent 时应该显示创建按钮', () => {
      const onCreateAgent = vi.fn();
      render(<AgentList agents={[]} onCreateAgent={onCreateAgent} />);

      expect(screen.getByRole('button', { name: /创建代理/ })).toBeInTheDocument();
    });

    it('无 onCreateAgent 时不应该显示创建按钮', () => {
      render(<AgentList agents={[]} />);

      expect(screen.queryByRole('button', { name: /创建代理/ })).not.toBeInTheDocument();
    });

    it('点击创建按钮应该调用 onCreateAgent', () => {
      const onCreateAgent = vi.fn();
      render(<AgentList agents={[]} onCreateAgent={onCreateAgent} />);

      fireEvent.click(screen.getByRole('button', { name: /创建代理/ }));

      expect(onCreateAgent).toHaveBeenCalled();
    });
  });

  describe('加载状态', () => {
    it('加载中应该显示骨架屏', () => {
      const { container } = render(<AgentList agents={[]} isLoading />);

      // 骨架屏应该有多个 animate-pulse 元素
      const skeletons = container.querySelectorAll('.animate-pulse');
      expect(skeletons.length).toBeGreaterThan(0);
    });

    it('有代理数据时不显示骨架屏', () => {
      const agents = [createMockAgent(1)];
      const { container } = render(<AgentList agents={agents} isLoading />);

      expect(container.querySelectorAll('.animate-pulse')).toHaveLength(0);
    });
  });

  describe('点击交互', () => {
    it('点击卡片应该调用 onAgentClick', () => {
      const agents = [createMockAgent(1)];
      const onAgentClick = vi.fn();

      render(<AgentList agents={agents} onAgentClick={onAgentClick} />);

      fireEvent.click(screen.getByRole('button', { name: /查看代理: 代理 1/ }));

      expect(onAgentClick).toHaveBeenCalledWith(agents[0]);
    });
  });

  describe('自定义样式', () => {
    it('应该应用自定义 className', () => {
      const agents = [createMockAgent(1)];
      const { container } = render(
        <AgentList agents={agents} className="custom-class" />
      );

      expect(container.firstChild).toHaveClass('custom-class');
    });
  });
});