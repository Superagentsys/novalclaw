/**
 * AgentCreateForm 组件单元测试
 *
 * 测试覆盖:
 * - 组件渲染所有字段
 * - 必填字段验证
 * - MBTISelector 集成
 * - PersonalityPreview 显示条件
 * - 表单提交成功流程
 * - 表单提交失败处理
 * - 取消按钮行为
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '../utils';
import { AgentCreateForm, type AgentModel } from '@/components/agent/AgentCreateForm';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';

// Mock Tauri API
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

// Mock sonner toast
vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}));

// Mock data for testing
const mockAgentModel: AgentModel = {
  id: 1,
  agent_uuid: 'test-uuid-123',
  name: '测试代理',
  description: '这是一个测试代理',
  domain: '代码审查',
  mbti_type: 'INTJ',
  status: 'active',
  system_prompt: undefined,
  created_at: Date.now(),
  updated_at: Date.now(),
};

describe('AgentCreateForm', () => {
  const mockInvoke = vi.mocked(invoke);
  const mockToastSuccess = vi.mocked(toast.success);
  const mockToastError = vi.mocked(toast.error);

  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  describe('字段渲染', () => {
    it('应该渲染名称输入框', () => {
      render(<AgentCreateForm />);

      expect(screen.getByLabelText(/名称/)).toBeInTheDocument();
    });

    it('应该渲染描述文本域', () => {
      render(<AgentCreateForm />);

      expect(screen.getByLabelText(/描述/)).toBeInTheDocument();
    });

    it('应该渲染专业领域输入框', () => {
      render(<AgentCreateForm />);

      expect(screen.getByLabelText(/专业领域/)).toBeInTheDocument();
    });

    it('应该渲染人格类型选择器', () => {
      render(<AgentCreateForm />);

      // 检查人格类型标签存在（使用更具体的 label 选择器）
      expect(screen.getByText('人格类型').closest('label')).toBeInTheDocument();
    });

    it('应该渲染创建按钮', () => {
      render(<AgentCreateForm />);

      expect(screen.getByRole('button', { name: /创建代理/ })).toBeInTheDocument();
    });

    it('应该显示字符计数', () => {
      render(<AgentCreateForm />);

      // 名称默认显示 0/50
      expect(screen.getByText('0/50')).toBeInTheDocument();
      // 描述默认显示 0/500
      expect(screen.getByText('0/500')).toBeInTheDocument();
    });
  });

  describe('必填字段验证', () => {
    it('名称为空时应该显示错误（失焦验证）', async () => {
      render(<AgentCreateForm />);

      // 输入名称然后清空
      const nameInput = screen.getByLabelText(/名称/);
      fireEvent.change(nameInput, { target: { value: '测试' } });
      fireEvent.change(nameInput, { target: { value: '' } });

      // 触发失焦验证
      fireEvent.blur(nameInput);

      await waitFor(() => {
        expect(screen.getByRole('alert')).toHaveTextContent('请输入代理名称');
      });
    });

    it('名称超过最大长度时应该显示错误', async () => {
      render(<AgentCreateForm />);

      const nameInput = screen.getByLabelText(/名称/);
      // 输入超过 50 字符的内容
      fireEvent.change(nameInput, {
        target: { value: 'a'.repeat(51) },
      });

      // 触发失焦验证
      fireEvent.blur(nameInput);

      await waitFor(() => {
        expect(screen.getByRole('alert')).toHaveTextContent('名称不能超过50个字符');
      });
    });

    it('描述超过最大长度时应该显示错误', async () => {
      render(<AgentCreateForm />);

      const descriptionInput = screen.getByLabelText(/描述/);
      // 输入超过 500 字符的内容
      fireEvent.change(descriptionInput, {
        target: { value: 'a'.repeat(501) },
      });

      // 触发失焦验证
      fireEvent.blur(descriptionInput);

      await waitFor(() => {
        expect(screen.getByRole('alert')).toHaveTextContent('描述不能超过500个字符');
      });
    });

    it('专业领域超过最大长度时应该显示错误', async () => {
      render(<AgentCreateForm />);

      const domainInput = screen.getByLabelText(/专业领域/);
      // 输入超过 100 字符的内容
      fireEvent.change(domainInput, {
        target: { value: 'a'.repeat(101) },
      });

      // 触发失焦验证
      fireEvent.blur(domainInput);

      await waitFor(() => {
        expect(screen.getByRole('alert')).toHaveTextContent('专业领域不能超过100个字符');
      });
    });

    it('输入内容后应该清除错误', async () => {
      render(<AgentCreateForm />);

      const nameInput = screen.getByLabelText(/名称/);

      // 触发错误：输入后清空并失焦
      fireEvent.change(nameInput, { target: { value: '测试' } });
      fireEvent.change(nameInput, { target: { value: '' } });
      fireEvent.blur(nameInput);

      await waitFor(() => {
        expect(screen.getByRole('alert')).toBeInTheDocument();
      });

      // 输入名称
      fireEvent.change(nameInput, { target: { value: '测试代理' } });

      await waitFor(() => {
        expect(screen.queryByRole('alert')).not.toBeInTheDocument();
      });
    });
  });

  describe('MBTISelector 集成', () => {
    it('应该渲染 MBTI 选择器', () => {
      render(<AgentCreateForm />);

      // 检查 MBTI 选择器是否渲染（通过检查分类标签）
      expect(screen.getByText('全部 (16)')).toBeInTheDocument();
    });

    it('选择人格类型应该更新预览', async () => {
      // Mock get_mbti_config 和 get_mbti_traits
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === 'get_mbti_config') {
          return Promise.resolve({
            description: '富有想象力的战略家',
            strengths: ['战略思维'],
            blind_spots: ['可能过于傲慢'],
            recommended_use_cases: ['战略规划'],
            theme_color: '#2563EB',
            accent_color: '#787163',
          });
        }
        if (cmd === 'get_mbti_traits') {
          return Promise.resolve({
            function_stack: {
              dominant: 'Ni',
              auxiliary: 'Te',
              tertiary: 'Fi',
              inferior: 'Se',
            },
          });
        }
        return Promise.resolve(null);
      });

      render(<AgentCreateForm />);

      // 初始状态：显示占位提示（使用更具体的选择器）
      const placeholder = screen.getByText('选择人格类型后').closest('div');
      expect(placeholder).toBeInTheDocument();

      // 点击 INTJ 类型按钮
      const intjButton = screen.getByRole('button', { name: /INTJ/i });
      fireEvent.click(intjButton);

      // 预览应该显示（检查认知功能栈标题）
      await waitFor(() => {
        expect(screen.getByText(/认知功能栈/)).toBeInTheDocument();
      });
    });
  });

  describe('PersonalityPreview 显示', () => {
    it('未选择人格类型时应该显示占位提示', () => {
      render(<AgentCreateForm />);

      expect(screen.getByText('选择人格类型后')).toBeInTheDocument();
      expect(screen.getByText('将显示预览')).toBeInTheDocument();
    });

    it('选择人格类型后应该显示预览', async () => {
      // Mock get_mbti_config 和 get_mbti_traits
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === 'get_mbti_config') {
          return Promise.resolve({
            description: '富有想象力的战略家',
            strengths: ['战略思维'],
            blind_spots: ['可能过于傲慢'],
            recommended_use_cases: ['战略规划'],
            theme_color: '#2563EB',
            accent_color: '#787163',
          });
        }
        if (cmd === 'get_mbti_traits') {
          return Promise.resolve({
            function_stack: {
              dominant: 'Ni',
              auxiliary: 'Te',
              tertiary: 'Fi',
              inferior: 'Se',
            },
          });
        }
        return Promise.resolve(null);
      });

      render(<AgentCreateForm />);

      // 选择 INTJ
      const intjButton = screen.getByRole('button', { name: /INTJ/i });
      fireEvent.click(intjButton);

      await waitFor(() => {
        expect(screen.getByText(/认知功能栈/)).toBeInTheDocument();
      });
    });
  });

  describe('表单提交成功', () => {
    it('成功创建代理应该调用 onSuccess 回调', async () => {
      mockInvoke.mockResolvedValueOnce(mockAgentModel);

      const onSuccess = vi.fn();
      render(<AgentCreateForm onSuccess={onSuccess} />);

      // 填写表单
      const nameInput = screen.getByLabelText(/名称/);
      fireEvent.change(nameInput, { target: { value: '测试代理' } });

      // 提交
      const submitButton = screen.getByRole('button', { name: /创建代理/ });
      fireEvent.click(submitButton);

      await waitFor(() => {
        expect(onSuccess).toHaveBeenCalledWith(mockAgentModel);
      });
    });

    it('成功创建代理应该显示成功通知', async () => {
      mockInvoke.mockResolvedValueOnce(mockAgentModel);

      render(<AgentCreateForm />);

      // 填写表单
      const nameInput = screen.getByLabelText(/名称/);
      fireEvent.change(nameInput, { target: { value: '测试代理' } });

      // 提交
      const submitButton = screen.getByRole('button', { name: /创建代理/ });
      fireEvent.click(submitButton);

      await waitFor(() => {
        expect(mockToastSuccess).toHaveBeenCalledWith(
          expect.stringContaining('测试代理')
        );
      });
    });

    it('提交时应该显示加载状态', async () => {
      // 模拟慢速响应
      mockInvoke.mockImplementation(
        () => new Promise((resolve) => setTimeout(() => resolve(mockAgentModel), 100))
      );

      render(<AgentCreateForm />);

      // 填写表单
      const nameInput = screen.getByLabelText(/名称/);
      fireEvent.change(nameInput, { target: { value: '测试代理' } });

      // 提交
      const submitButton = screen.getByRole('button', { name: /创建代理/ });
      fireEvent.click(submitButton);

      // 应该显示加载状态
      expect(screen.getByText('创建中...')).toBeInTheDocument();
      expect(submitButton).toBeDisabled();
    });
  });

  describe('表单提交失败', () => {
    it('失败时应该显示错误通知', async () => {
      mockInvoke.mockRejectedValueOnce(new Error('Network error'));

      render(<AgentCreateForm />);

      // 填写表单
      const nameInput = screen.getByLabelText(/名称/);
      fireEvent.change(nameInput, { target: { value: '测试代理' } });

      // 提交
      const submitButton = screen.getByRole('button', { name: /创建代理/ });
      fireEvent.click(submitButton);

      await waitFor(() => {
        expect(mockToastError).toHaveBeenCalledWith(
          '创建代理失败',
          expect.objectContaining({
            description: 'Network error',
          })
        );
      });
    });
  });

  describe('取消按钮', () => {
    it('应该渲染取消按钮（当提供 onCancel 时）', () => {
      render(<AgentCreateForm onCancel={() => {}} />);

      expect(screen.getByRole('button', { name: '取消' })).toBeInTheDocument();
    });

    it('不提供 onCancel 时不应该渲染取消按钮', () => {
      render(<AgentCreateForm />);

      expect(screen.queryByRole('button', { name: '取消' })).not.toBeInTheDocument();
    });

    it('点击取消应该调用 onCancel 回调', () => {
      const onCancel = vi.fn();
      render(<AgentCreateForm onCancel={onCancel} />);

      const cancelButton = screen.getByRole('button', { name: '取消' });
      fireEvent.click(cancelButton);

      expect(onCancel).toHaveBeenCalled();
    });
  });

  describe('自定义样式', () => {
    it('应该应用自定义 className', () => {
      const { container } = render(
        <AgentCreateForm className="custom-class" />
      );

      expect(container.firstChild).toHaveClass('custom-class');
    });
  });

  describe('禁用状态', () => {
    it('提交中应该禁用所有输入', async () => {
      // 模拟慢速响应
      mockInvoke.mockImplementation(
        () => new Promise((resolve) => setTimeout(() => resolve(mockAgentModel), 100))
      );

      render(<AgentCreateForm />);

      // 填写表单
      const nameInput = screen.getByLabelText(/名称/) as HTMLInputElement;
      fireEvent.change(nameInput, { target: { value: '测试代理' } });

      // 提交
      const submitButton = screen.getByRole('button', { name: /创建代理/ });
      fireEvent.click(submitButton);

      // 所有输入应该被禁用
      expect(nameInput.disabled).toBe(true);
    });
  });
});