/**
 * AgentCard 组件单元测试
 *
 * 测试覆盖:
 * - 组件渲染所有信息
 * - 状态指示器显示正确状态
 * - 人格主题色应用
 * - 点击导航行为
 * - 键盘可访问性
 * - 编辑按钮功能
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '../utils';
import { AgentCard, type AgentModel } from '@/components/agent/AgentCard';
import { type MBTIType } from '@/lib/personality-colors';

// Mock data
const createMockAgent = (overrides?: Partial<AgentModel>): AgentModel => ({
  id: 1,
  agent_uuid: 'test-uuid-123',
  name: '测试代理',
  description: '这是一个测试代理的描述',
  domain: '代码审查',
  mbti_type: 'INTJ' as MBTIType,
  status: 'active',
  created_at: Date.now(),
  updated_at: Date.now(),
  ...overrides,
});

describe('AgentCard', () => {
  describe('信息渲染', () => {
    it('应该渲染代理名称', () => {
      render(<AgentCard agent={createMockAgent()} />);

      expect(screen.getByText('测试代理')).toBeInTheDocument();
    });

    it('应该渲染代理描述', () => {
      render(<AgentCard agent={createMockAgent()} />);

      expect(screen.getByText(/这是一个测试代理的描述/)).toBeInTheDocument();
    });

    it('应该渲染专业领域', () => {
      render(<AgentCard agent={createMockAgent()} />);

      expect(screen.getByText('代码审查')).toBeInTheDocument();
    });

    it('应该渲染 MBTI 类型徽章', () => {
      render(<AgentCard agent={createMockAgent()} />);

      expect(screen.getByText('INTJ')).toBeInTheDocument();
    });

    it('无描述时不渲染描述区域', () => {
      render(<AgentCard agent={createMockAgent({ description: undefined })} />);

      // 不应该有描述文本
      expect(screen.queryByText(/这是一个测试代理的描述/)).not.toBeInTheDocument();
    });

    it('无专业领域时不渲染领域标签', () => {
      render(<AgentCard agent={createMockAgent({ domain: undefined })} />);

      expect(screen.queryByText('代码审查')).not.toBeInTheDocument();
    });

    it('无 MBTI 类型时不渲染类型徽章', () => {
      render(<AgentCard agent={createMockAgent({ mbti_type: undefined })} />);

      expect(screen.queryByText('INTJ')).not.toBeInTheDocument();
    });
  });

  describe('状态指示器', () => {
    it('应该显示 active 状态', () => {
      render(<AgentCard agent={createMockAgent({ status: 'active' })} />);

      expect(screen.getByText('活动')).toBeInTheDocument();
    });

    it('应该显示 inactive 状态', () => {
      render(<AgentCard agent={createMockAgent({ status: 'inactive' })} />);

      expect(screen.getByText('停用')).toBeInTheDocument();
    });

    it('应该显示 archived 状态', () => {
      render(<AgentCard agent={createMockAgent({ status: 'archived' })} />);

      expect(screen.getByText('已归档')).toBeInTheDocument();
    });
  });

  describe('人格主题色', () => {
    it('应该应用 INTJ 主题色到边框', () => {
      const { container } = render(
        <AgentCard agent={createMockAgent({ mbti_type: 'INTJ' })} />
      );

      const card = container.firstChild as HTMLElement;
      expect(card.style.borderLeftColor).toBe('rgb(37, 99, 235)'); // #2563EB
    });

    it('应该应用 INFP 主题色到边框', () => {
      const { container } = render(
        <AgentCard agent={createMockAgent({ mbti_type: 'INFP' })} />
      );

      const card = container.firstChild as HTMLElement;
      expect(card.style.borderLeftColor).toBe('rgb(249, 115, 22)'); // #F97316
    });

    it('无 MBTI 类型时使用默认边框', () => {
      const { container } = render(
        <AgentCard agent={createMockAgent({ mbti_type: undefined })} />
      );

      const card = container.firstChild as HTMLElement;
      expect(card.style.borderLeftColor).toBeFalsy();
    });
  });

  describe('点击交互', () => {
    it('点击卡片应该调用 onClick 回调', () => {
      const onClick = vi.fn();
      const agent = createMockAgent();
      render(<AgentCard agent={agent} onClick={onClick} />);

      fireEvent.click(screen.getByRole('button'));

      expect(onClick).toHaveBeenCalledWith(
        expect.objectContaining({
          id: agent.id,
          agent_uuid: agent.agent_uuid,
          name: agent.name,
          description: agent.description,
          domain: agent.domain,
          mbti_type: agent.mbti_type,
          status: agent.status,
        })
      );
    });

    it('未提供 onClick 时不应该抛出错误', () => {
      expect(() => {
        render(<AgentCard agent={createMockAgent()} />);
        fireEvent.click(screen.getByRole('button'));
      }).not.toThrow();
    });
  });

  describe('可访问性', () => {
    it('应该有正确的 role 属性', () => {
      render(<AgentCard agent={createMockAgent()} />);

      expect(screen.getByRole('button')).toBeInTheDocument();
    });

    it('应该有 aria-label', () => {
      render(<AgentCard agent={createMockAgent()} />);

      expect(screen.getByLabelText(/查看代理: 测试代理/)).toBeInTheDocument();
    });

    it('应该支持键盘导航（Enter 键）', () => {
      const onClick = vi.fn();
      render(<AgentCard agent={createMockAgent()} onClick={onClick} />);

      const card = screen.getByRole('button');
      fireEvent.keyDown(card, { key: 'Enter' });

      expect(onClick).toHaveBeenCalled();
    });

    it('应该支持键盘导航（Space 键）', () => {
      const onClick = vi.fn();
      render(<AgentCard agent={createMockAgent()} onClick={onClick} />);

      const card = screen.getByRole('button');
      fireEvent.keyDown(card, { key: ' ' });

      expect(onClick).toHaveBeenCalled();
    });
  });

  describe('自定义样式', () => {
    it('应该应用自定义 className', () => {
      const { container } = render(
        <AgentCard agent={createMockAgent()} className="custom-class" />
      );

      expect(container.firstChild).toHaveClass('custom-class');
    });
  });

  describe('文本截断', () => {
    it('长名称应该被截断', () => {
      const longName = '这是一个非常非常非常非常非常非常长的代理名称';
      render(<AgentCard agent={createMockAgent({ name: longName })} />);

      const nameElement = screen.getByText(longName);
      expect(nameElement).toHaveClass('truncate');
    });

    it('长描述应该被截断', () => {
      const longDescription = '这是一个非常非常非常非常非常非常非常非常非常非常非常非常非常非常非常长的描述文本';
      render(<AgentCard agent={createMockAgent({ description: longDescription })} />);

      const descElement = screen.getByText(longDescription);
      expect(descElement).toHaveClass('line-clamp-2');
    });
  });

  describe('编辑按钮', () => {
    it('当 showEditButton 为 true 时应显示编辑按钮', () => {
      render(<AgentCard agent={createMockAgent()} showEditButton />);

      expect(screen.getByRole('button', { name: /编辑代理/i })).toBeInTheDocument();
    });

    it('当 showEditButton 未设置时不显示编辑按钮', () => {
      render(<AgentCard agent={createMockAgent()} />);

      expect(screen.queryByRole('button', { name: /编辑代理/i })).not.toBeInTheDocument();
    });

    it('当 showEditButton 为 false 时不显示编辑按钮', () => {
      render(<AgentCard agent={createMockAgent()} showEditButton={false} />);

      expect(screen.queryByRole('button', { name: /编辑代理/i })).not.toBeInTheDocument();
    });

    it('点击编辑按钮应调用 onEdit 回调', () => {
      const onEdit = vi.fn();
      const agent = createMockAgent();
      render(<AgentCard agent={agent} showEditButton onEdit={onEdit} />);

      fireEvent.click(screen.getByRole('button', { name: /编辑代理/i }));

      expect(onEdit).toHaveBeenCalledWith(agent);
    });

    it('点击编辑按钮不应触发卡片的 onClick', () => {
      const onClick = vi.fn();
      const onEdit = vi.fn();
      render(
        <AgentCard
          agent={createMockAgent()}
          showEditButton
          onEdit={onEdit}
          onClick={onClick}
        />
      );

      fireEvent.click(screen.getByRole('button', { name: /编辑代理/i }));

      expect(onEdit).toHaveBeenCalled();
      expect(onClick).not.toHaveBeenCalled();
    });

    it('编辑按钮应使用人格主题色', () => {
      render(
        <AgentCard agent={createMockAgent({ mbti_type: 'INTJ' })} showEditButton />
      );

      // 查找编辑按钮
      const editButton = screen.getByRole('button', { name: /编辑代理/i });
      expect(editButton).toBeInTheDocument();
    });
  });

  describe('复制按钮', () => {
    it('当 showDuplicateButton 为 true 时应显示复制按钮', () => {
      render(<AgentCard agent={createMockAgent()} showDuplicateButton />);

      expect(screen.getByRole('button', { name: /复制代理/i })).toBeInTheDocument();
    });

    it('当 showDuplicateButton 未设置时不显示复制按钮', () => {
      render(<AgentCard agent={createMockAgent()} />);

      expect(screen.queryByRole('button', { name: /复制代理/i })).not.toBeInTheDocument();
    });

    it('当 showDuplicateButton 为 false 时不显示复制按钮', () => {
      render(<AgentCard agent={createMockAgent()} showDuplicateButton={false} />);

      expect(screen.queryByRole('button', { name: /复制代理/i })).not.toBeInTheDocument();
    });

    it('点击复制按钮应调用 onDuplicate 回调', () => {
      const onDuplicate = vi.fn();
      const agent = createMockAgent();
      render(<AgentCard agent={agent} showDuplicateButton onDuplicate={onDuplicate} />);

      fireEvent.click(screen.getByRole('button', { name: /复制代理/i }));

      expect(onDuplicate).toHaveBeenCalledWith(agent);
    });

    it('点击复制按钮不应触发卡片的 onClick', () => {
      const onClick = vi.fn();
      const onDuplicate = vi.fn();
      render(
        <AgentCard
          agent={createMockAgent()}
          showDuplicateButton
          onDuplicate={onDuplicate}
          onClick={onClick}
        />
      );

      fireEvent.click(screen.getByRole('button', { name: /复制代理/i }));

      expect(onDuplicate).toHaveBeenCalled();
      expect(onClick).not.toHaveBeenCalled();
    });

    it('复制按钮应使用人格主题色', () => {
      render(
        <AgentCard agent={createMockAgent({ mbti_type: 'INTJ' })} showDuplicateButton />
      );

      const duplicateButton = screen.getByRole('button', { name: /复制代理/i });
      expect(duplicateButton).toBeInTheDocument();
    });

    it('编辑按钮和复制按钮可以同时显示', () => {
      render(
        <AgentCard
          agent={createMockAgent()}
          showEditButton
          showDuplicateButton
        />
      );

      expect(screen.getByRole('button', { name: /编辑代理/i })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /复制代理/i })).toBeInTheDocument();
    });
  });

  describe('状态切换按钮', () => {
    it('当 showToggleButton 为 true 时应显示切换按钮', () => {
      render(<AgentCard agent={createMockAgent()} showToggleButton />);

      expect(screen.getByRole('button', { name: /切换代理状态/i })).toBeInTheDocument();
    });

    it('当 showToggleButton 未设置时不显示切换按钮', () => {
      render(<AgentCard agent={createMockAgent()} />);

      expect(screen.queryByRole('button', { name: /切换代理状态/i })).not.toBeInTheDocument();
    });

    it('当 showToggleButton 为 false 时不显示切换按钮', () => {
      render(<AgentCard agent={createMockAgent()} showToggleButton={false} />);

      expect(screen.queryByRole('button', { name: /切换代理状态/i })).not.toBeInTheDocument();
    });

    it('点击切换按钮应调用 onToggle 回调', () => {
      const onToggle = vi.fn();
      const agent = createMockAgent();
      render(<AgentCard agent={agent} showToggleButton onToggle={onToggle} />);

      fireEvent.click(screen.getByRole('button', { name: /切换代理状态/i }));

      expect(onToggle).toHaveBeenCalledWith(agent);
    });

    it('点击切换按钮不应触发卡片的 onClick', () => {
      const onClick = vi.fn();
      const onToggle = vi.fn();
      render(
        <AgentCard
          agent={createMockAgent()}
          showToggleButton
          onToggle={onToggle}
          onClick={onClick}
        />
      );

      fireEvent.click(screen.getByRole('button', { name: /切换代理状态/i }));

      expect(onToggle).toHaveBeenCalled();
      expect(onClick).not.toHaveBeenCalled();
    });

    it('停用状态的代理卡片应有 opacity-60 类', () => {
      const { container } = render(
        <AgentCard agent={createMockAgent({ status: 'inactive' })} />
      );

      const card = container.firstChild as HTMLElement;
      expect(card).toHaveClass('opacity-60');
    });

    it('活动状态的代理卡片不应有 opacity-60 类', () => {
      const { container } = render(
        <AgentCard agent={createMockAgent({ status: 'active' })} />
      );

      const card = container.firstChild as HTMLElement;
      expect(card).not.toHaveClass('opacity-60');
    });

    it('切换按钮应使用人格主题色', () => {
      render(
        <AgentCard agent={createMockAgent({ mbti_type: 'INTJ' })} showToggleButton />
      );

      const toggleButton = screen.getByRole('button', { name: /切换代理状态/i });
      expect(toggleButton).toBeInTheDocument();
    });

    it('停用状态的切换按钮应有 opacity-50 样式', () => {
      render(
        <AgentCard agent={createMockAgent({ status: 'inactive' })} showToggleButton />
      );

      const toggleButton = screen.getByRole('button', { name: /切换代理状态/i });
      expect(toggleButton).toHaveClass('opacity-50');
    });

    it('切换、复制、编辑按钮可以同时显示', () => {
      render(
        <AgentCard
          agent={createMockAgent()}
          showToggleButton
          showDuplicateButton
          showEditButton
        />
      );

      expect(screen.getByRole('button', { name: /切换代理状态/i })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /复制代理/i })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /编辑代理/i })).toBeInTheDocument();
    });

    it('已归档状态的代理不应显示切换按钮', () => {
      render(
        <AgentCard agent={createMockAgent({ status: 'archived' })} showToggleButton />
      );

      expect(screen.queryByRole('button', { name: /切换代理状态/i })).not.toBeInTheDocument();
    });
  });

  describe('删除按钮', () => {
    it('当 showDeleteButton 为 true 时应显示删除按钮', () => {
      render(<AgentCard agent={createMockAgent()} showDeleteButton />);

      expect(screen.getByRole('button', { name: /删除代理/i })).toBeInTheDocument();
    });

    it('当 showDeleteButton 未设置时不显示删除按钮', () => {
      render(<AgentCard agent={createMockAgent()} />);

      expect(screen.queryByRole('button', { name: /删除代理/i })).not.toBeInTheDocument();
    });

    it('当 showDeleteButton 为 false 时不显示删除按钮', () => {
      render(<AgentCard agent={createMockAgent()} showDeleteButton={false} />);

      expect(screen.queryByRole('button', { name: /删除代理/i })).not.toBeInTheDocument();
    });

    it('点击删除按钮应调用 onDelete 回调', () => {
      const onDelete = vi.fn();
      const agent = createMockAgent();
      render(<AgentCard agent={agent} showDeleteButton onDelete={onDelete} />);

      fireEvent.click(screen.getByRole('button', { name: /删除代理/i }));

      expect(onDelete).toHaveBeenCalledWith(agent);
    });

    it('点击删除按钮不应触发卡片的 onClick', () => {
      const onClick = vi.fn();
      const onDelete = vi.fn();
      render(
        <AgentCard
          agent={createMockAgent()}
          showDeleteButton
          onDelete={onDelete}
          onClick={onClick}
        />
      );

      fireEvent.click(screen.getByRole('button', { name: /删除代理/i }));

      expect(onDelete).toHaveBeenCalled();
      expect(onClick).not.toHaveBeenCalled();
    });

    it('删除按钮应有 destructive 样式', () => {
      render(<AgentCard agent={createMockAgent()} showDeleteButton />);

      const deleteButton = screen.getByRole('button', { name: /删除代理/i });
      expect(deleteButton).toHaveClass('hover:bg-destructive/10');
      expect(deleteButton).toHaveClass('hover:text-destructive');
    });

    it('删除、切换、复制、编辑按钮可以同时显示', () => {
      render(
        <AgentCard
          agent={createMockAgent()}
          showDeleteButton
          showToggleButton
          showDuplicateButton
          showEditButton
        />
      );

      expect(screen.getByRole('button', { name: /删除代理/i })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /切换代理状态/i })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /复制代理/i })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /编辑代理/i })).toBeInTheDocument();
    });
  });
});