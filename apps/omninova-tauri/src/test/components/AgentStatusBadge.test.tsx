/**
 * AgentStatusBadge 组件单元测试
 *
 * 测试覆盖:
 * - 三种状态的渲染
 * - 不同尺寸支持
 * - 可访问性支持
 * - 自定义样式
 */

import { describe, it, expect } from 'vitest';
import { render, screen } from '../utils';
import { AgentStatusBadge } from '@/components/agent/AgentStatusBadge';

describe('AgentStatusBadge', () => {
  describe('状态渲染', () => {
    it('应该渲染 active 状态', () => {
      render(<AgentStatusBadge status="active" />);

      expect(screen.getByText('活动')).toBeInTheDocument();
    });

    it('应该渲染 inactive 状态', () => {
      render(<AgentStatusBadge status="inactive" />);

      expect(screen.getByText('停用')).toBeInTheDocument();
    });

    it('应该渲染 archived 状态', () => {
      render(<AgentStatusBadge status="archived" />);

      expect(screen.getByText('已归档')).toBeInTheDocument();
    });
  });

  describe('尺寸支持', () => {
    it('应该支持 sm 尺寸', () => {
      const { container } = render(<AgentStatusBadge status="active" size="sm" />);

      expect(container.firstChild).toHaveClass('text-xs');
    });

    it('应该支持 md 尺寸（默认）', () => {
      const { container } = render(<AgentStatusBadge status="active" size="md" />);

      expect(container.firstChild).toHaveClass('text-sm');
    });

    it('应该支持 lg 尺寸', () => {
      const { container } = render(<AgentStatusBadge status="active" size="lg" />);

      expect(container.firstChild).toHaveClass('text-base');
    });
  });

  describe('可访问性', () => {
    it('应该有 aria-label', () => {
      render(<AgentStatusBadge status="active" />);

      expect(screen.getByLabelText('代理状态: 活动')).toBeInTheDocument();
    });

    it('inactive 状态应该有正确的 aria-label', () => {
      render(<AgentStatusBadge status="inactive" />);

      expect(screen.getByLabelText('代理状态: 停用')).toBeInTheDocument();
    });

    it('archived 状态应该有正确的 aria-label', () => {
      render(<AgentStatusBadge status="archived" />);

      expect(screen.getByLabelText('代理状态: 已归档')).toBeInTheDocument();
    });
  });

  describe('自定义样式', () => {
    it('应该应用自定义 className', () => {
      const { container } = render(
        <AgentStatusBadge status="active" className="custom-class" />
      );

      expect(container.firstChild).toHaveClass('custom-class');
    });
  });

  describe('状态颜色', () => {
    it('active 状态应该有绿色圆点', () => {
      const { container } = render(<AgentStatusBadge status="active" />);

      const dot = container.querySelector('.bg-green-500');
      expect(dot).toBeInTheDocument();
    });

    it('inactive 状态应该有灰色圆点', () => {
      const { container } = render(<AgentStatusBadge status="inactive" />);

      const dot = container.querySelector('.bg-gray-400');
      expect(dot).toBeInTheDocument();
    });

    it('archived 状态应该有琥珀色圆点', () => {
      const { container } = render(<AgentStatusBadge status="archived" />);

      const dot = container.querySelector('.bg-amber-500');
      expect(dot).toBeInTheDocument();
    });
  });
});