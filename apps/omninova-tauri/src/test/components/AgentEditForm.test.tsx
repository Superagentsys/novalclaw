/**
 * AgentEditForm 组件测试
 *
 * 测试代理编辑表单的功能:
 * - 预填充现有代理数据
 * - MBTI 类型预选和预览
 * - 表单验证
 * - 保存和取消操作
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, act } from '../utils';
import { AgentEditForm } from '@/components/agent/AgentEditForm';
import { type AgentModel } from '@/types/agent';
import { invoke } from '@tauri-apps/api/core';

// Mock Tauri API
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

// Mock sonner toast
vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
  },
}));

const mockInvoke = vi.mocked(invoke);

// 测试数据
const mockAgent: AgentModel = {
  id: 1,
  agent_uuid: 'test-uuid-123',
  name: 'Test Agent',
  description: 'A test agent description',
  domain: 'Testing',
  mbti_type: 'INTJ',
  status: 'active',
  created_at: Date.now(),
  updated_at: Date.now(),
};

const mockOnSuccess = vi.fn();
const mockOnCancel = vi.fn();

// Mock MBTI 配置数据
const mockMBTIConfig = {
  description: '建筑师型人格',
  system_prompt_template: 'You are an INTJ.',
  strengths: ['战略思维', '独立', '坚定'],
  blind_spots: ['可能过于武断'],
  recommended_use_cases: ['战略规划'],
  theme_color: '#2563EB',
  accent_color: '#3B82F6',
};

const mockMBTITraits = {
  function_stack: {
    dominant: 'Ni',
    auxiliary: 'Te',
    tertiary: 'Fi',
    inferior: 'Se',
  },
  behavior_tendency: {
    decision_making: '逻辑分析',
    information_processing: '直觉洞察',
    social_interaction: '独立自主',
    stress_response: '内省',
  },
  communication_style: {
    preference: '直接高效',
    language_traits: ['精确', '结构化'],
    feedback_style: '建设性批评',
  },
};

/**
 * 设置默认的 mock 实现
 */
function setupDefaultMocks() {
  mockInvoke.mockImplementation(async (cmd: string) => {
    if (cmd === 'get_mbti_config') {
      return mockMBTIConfig;
    }
    if (cmd === 'get_mbti_traits') {
      return mockMBTITraits;
    }
    if (cmd === 'update_agent') {
      return JSON.stringify(mockAgent);
    }
    return null;
  });
}

describe('AgentEditForm', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
    setupDefaultMocks();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe('Rendering', () => {
    it('should render with prefilled data from agent', async () => {
      await act(async () => {
        render(
          <AgentEditForm
            agent={mockAgent}
            onSuccess={mockOnSuccess}
            onCancel={mockOnCancel}
          />
        );
        vi.advanceTimersByTime(100);
      });

      // 检查名称字段预填充
      const nameInput = screen.getByLabelText(/名称/i);
      expect(nameInput).toHaveValue('Test Agent');

      // 检查描述字段预填充
      const descriptionInput = screen.getByLabelText(/描述/i);
      expect(descriptionInput).toHaveValue('A test agent description');

      // 检查专业领域字段预填充
      const domainInput = screen.getByLabelText(/专业领域/i);
      expect(domainInput).toHaveValue('Testing');
    });

    it('should render MBTI selector with preselected type', async () => {
      await act(async () => {
        render(
          <AgentEditForm
            agent={mockAgent}
            onSuccess={mockOnSuccess}
            onCancel={mockOnCancel}
          />
        );
        vi.advanceTimersByTime(100);
      });

      // 检查 MBTI 类型是否预选 - INTJ 按钮应该有 aria-pressed="true"
      const intjButton = screen.getByRole('button', { name: /INTJ/ });
      expect(intjButton).toHaveAttribute('aria-pressed', 'true');
    });

    it('should show personality preview for selected MBTI type', async () => {
      await act(async () => {
        render(
          <AgentEditForm
            agent={mockAgent}
            onSuccess={mockOnSuccess}
            onCancel={mockOnCancel}
          />
        );
        vi.advanceTimersByTime(100);
      });

      // 人格预览应该显示 INTJ 相关内容
      // 使用 getAllByText 因为有多个元素包含 INTJ
      const intjElements = screen.getAllByText(/INTJ/);
      expect(intjElements.length).toBeGreaterThan(0);
    });

    it('should render save and cancel buttons', async () => {
      await act(async () => {
        render(
          <AgentEditForm
            agent={mockAgent}
            onSuccess={mockOnSuccess}
            onCancel={mockOnCancel}
          />
        );
        vi.advanceTimersByTime(100);
      });

      expect(screen.getByRole('button', { name: /保存/i })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /取消/i })).toBeInTheDocument();
    });
  });

  describe('Form Validation', () => {
    it('should show error when name is cleared', async () => {
      await act(async () => {
        render(
          <AgentEditForm
            agent={mockAgent}
            onSuccess={mockOnSuccess}
            onCancel={mockOnCancel}
          />
        );
        vi.advanceTimersByTime(100);
      });

      const nameInput = screen.getByLabelText(/名称/i);
      fireEvent.change(nameInput, { target: { value: '' } });
      fireEvent.blur(nameInput);

      // 推进时间让状态更新
      await act(async () => {
        vi.advanceTimersByTime(100);
      });

      // 检查错误消息是否存在
      const errorMessage = screen.queryByText(/请输入代理名称/i);
      expect(errorMessage).toBeInTheDocument();
    });

    it('should disable save button when form is invalid', async () => {
      await act(async () => {
        render(
          <AgentEditForm
            agent={mockAgent}
            onSuccess={mockOnSuccess}
            onCancel={mockOnCancel}
          />
        );
        vi.advanceTimersByTime(100);
      });

      const nameInput = screen.getByLabelText(/名称/i);
      fireEvent.change(nameInput, { target: { value: '' } });

      const saveButton = screen.getByRole('button', { name: /保存/i });
      expect(saveButton).toBeDisabled();
    });

    it('should disable save button when no changes made', async () => {
      await act(async () => {
        render(
          <AgentEditForm
            agent={mockAgent}
            onSuccess={mockOnSuccess}
            onCancel={mockOnCancel}
          />
        );
        vi.advanceTimersByTime(100);
      });

      // 没有更改时，保存按钮应该禁用
      const saveButton = screen.getByRole('button', { name: /保存/i });
      expect(saveButton).toBeDisabled();
    });
  });

  describe('MBTI Type Change', () => {
    it('should update selection when MBTI type changes', async () => {
      await act(async () => {
        render(
          <AgentEditForm
            agent={mockAgent}
            onSuccess={mockOnSuccess}
            onCancel={mockOnCancel}
          />
        );
        vi.advanceTimersByTime(100);
      });

      // 点击 ENFP 按钮选择新类型
      const enfpButton = screen.getByRole('button', { name: /ENFP/ });
      fireEvent.click(enfpButton);

      // 推进时间让状态更新
      await act(async () => {
        vi.advanceTimersByTime(100);
      });

      // ENFP 按钮应该被选中
      expect(enfpButton).toHaveAttribute('aria-pressed', 'true');

      // INTJ 按钮应该取消选中
      const intjButton = screen.getByRole('button', { name: /INTJ/ });
      expect(intjButton).toHaveAttribute('aria-pressed', 'false');
    });
  });

  describe('Save Changes', () => {
    it('should call update_agent with correct data on save', async () => {
      const updatedAgent = { ...mockAgent, name: 'Updated Name' };

      mockInvoke.mockImplementation(async (cmd: string, _args?: Record<string, unknown>) => {
        if (cmd === 'get_mbti_config') {
          return mockMBTIConfig;
        }
        if (cmd === 'get_mbti_traits') {
          return mockMBTITraits;
        }
        if (cmd === 'update_agent') {
          return JSON.stringify(updatedAgent);
        }
        return null;
      });

      await act(async () => {
        render(
          <AgentEditForm
            agent={mockAgent}
            onSuccess={mockOnSuccess}
            onCancel={mockOnCancel}
          />
        );
        vi.advanceTimersByTime(100);
      });

      // 修改名称
      const nameInput = screen.getByLabelText(/名称/i);
      fireEvent.change(nameInput, { target: { value: 'Updated Name' } });

      // 点击保存
      const saveButton = screen.getByRole('button', { name: /保存/i });
      await act(async () => {
        fireEvent.click(saveButton);
        vi.advanceTimersByTime(100);
      });

      expect(mockInvoke).toHaveBeenCalledWith('update_agent', {
        uuid: 'test-uuid-123',
        updatesJson: expect.stringContaining('Updated Name'),
      });
    });

    it('should show success toast on successful update', async () => {
      const { toast } = await import('sonner');
      const updatedAgent = { ...mockAgent, name: 'Updated Name' };

      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === 'get_mbti_config') {
          return mockMBTIConfig;
        }
        if (cmd === 'get_mbti_traits') {
          return mockMBTITraits;
        }
        if (cmd === 'update_agent') {
          return JSON.stringify(updatedAgent);
        }
        return null;
      });

      await act(async () => {
        render(
          <AgentEditForm
            agent={mockAgent}
            onSuccess={mockOnSuccess}
            onCancel={mockOnCancel}
          />
        );
        vi.advanceTimersByTime(100);
      });

      const nameInput = screen.getByLabelText(/名称/i);
      fireEvent.change(nameInput, { target: { value: 'Updated Name' } });

      const saveButton = screen.getByRole('button', { name: /保存/i });
      await act(async () => {
        fireEvent.click(saveButton);
        vi.advanceTimersByTime(100);
      });

      expect(toast.success).toHaveBeenCalled();
    });

    it('should call onSuccess callback after successful update', async () => {
      const updatedAgent = { ...mockAgent, name: 'Updated Name' };

      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === 'get_mbti_config') {
          return mockMBTIConfig;
        }
        if (cmd === 'get_mbti_traits') {
          return mockMBTITraits;
        }
        if (cmd === 'update_agent') {
          return JSON.stringify(updatedAgent);
        }
        return null;
      });

      await act(async () => {
        render(
          <AgentEditForm
            agent={mockAgent}
            onSuccess={mockOnSuccess}
            onCancel={mockOnCancel}
          />
        );
        vi.advanceTimersByTime(100);
      });

      const nameInput = screen.getByLabelText(/名称/i);
      fireEvent.change(nameInput, { target: { value: 'Updated Name' } });

      const saveButton = screen.getByRole('button', { name: /保存/i });
      await act(async () => {
        fireEvent.click(saveButton);
        vi.advanceTimersByTime(100);
      });

      expect(mockOnSuccess).toHaveBeenCalled();
    });

    it('should show error toast on update failure', async () => {
      const { toast } = await import('sonner');

      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === 'get_mbti_config') {
          return mockMBTIConfig;
        }
        if (cmd === 'get_mbti_traits') {
          return mockMBTITraits;
        }
        if (cmd === 'update_agent') {
          throw new Error('Update failed');
        }
        return null;
      });

      await act(async () => {
        render(
          <AgentEditForm
            agent={mockAgent}
            onSuccess={mockOnSuccess}
            onCancel={mockOnCancel}
          />
        );
        vi.advanceTimersByTime(100);
      });

      // 修改名称以启用保存按钮
      const nameInput = screen.getByLabelText(/名称/i);
      fireEvent.change(nameInput, { target: { value: 'Updated Name' } });

      const saveButton = screen.getByRole('button', { name: /保存/i });
      await act(async () => {
        fireEvent.click(saveButton);
        vi.advanceTimersByTime(100);
      });

      expect(toast.error).toHaveBeenCalled();
    });
  });

  describe('Cancel Operation', () => {
    it('should call onCancel when cancel button is clicked', async () => {
      await act(async () => {
        render(
          <AgentEditForm
            agent={mockAgent}
            onSuccess={mockOnSuccess}
            onCancel={mockOnCancel}
          />
        );
        vi.advanceTimersByTime(100);
      });

      const cancelButton = screen.getByRole('button', { name: /取消/i });
      fireEvent.click(cancelButton);

      expect(mockOnCancel).toHaveBeenCalled();
    });

    it('should not call update_agent when cancelled', async () => {
      await act(async () => {
        render(
          <AgentEditForm
            agent={mockAgent}
            onSuccess={mockOnSuccess}
            onCancel={mockOnCancel}
          />
        );
        vi.advanceTimersByTime(100);
      });

      // 修改表单
      const nameInput = screen.getByLabelText(/名称/i);
      fireEvent.change(nameInput, { target: { value: 'Changed Name' } });

      // 点击取消
      const cancelButton = screen.getByRole('button', { name: /取消/i });
      fireEvent.click(cancelButton);

      // 不应该调用 update_agent (只应该有 PersonalityPreview 的调用)
      const updateAgentCalls = mockInvoke.mock.calls.filter(
        (call) => call[0] === 'update_agent'
      );
      expect(updateAgentCalls).toHaveLength(0);
    });
  });

  describe('Loading State', () => {
    it('should show loading state during save', async () => {
      let resolvePromise: (value: string) => void;
      const pendingPromise = new Promise<string>((resolve) => {
        resolvePromise = resolve;
      });

      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === 'get_mbti_config') {
          return mockMBTIConfig;
        }
        if (cmd === 'get_mbti_traits') {
          return mockMBTITraits;
        }
        if (cmd === 'update_agent') {
          return pendingPromise;
        }
        return null;
      });

      await act(async () => {
        render(
          <AgentEditForm
            agent={mockAgent}
            onSuccess={mockOnSuccess}
            onCancel={mockOnCancel}
          />
        );
        vi.advanceTimersByTime(100);
      });

      // 修改名称以启用保存按钮
      const nameInput = screen.getByLabelText(/名称/i);
      fireEvent.change(nameInput, { target: { value: 'Updated Name' } });

      const saveButton = screen.getByRole('button', { name: /保存/i });
      await act(async () => {
        fireEvent.click(saveButton);
        vi.advanceTimersByTime(100);
      });

      // 应该显示加载状态
      expect(screen.getByText(/保存中/i)).toBeInTheDocument();

      // Resolve promise
      resolvePromise!(JSON.stringify(mockAgent));

      await act(async () => {
        vi.advanceTimersByTime(100);
      });

      expect(screen.queryByText(/保存中/i)).not.toBeInTheDocument();
    });

    it('should disable form fields during submission', async () => {
      let resolvePromise: (value: string) => void;
      const pendingPromise = new Promise<string>((resolve) => {
        resolvePromise = resolve;
      });

      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === 'get_mbti_config') {
          return mockMBTIConfig;
        }
        if (cmd === 'get_mbti_traits') {
          return mockMBTITraits;
        }
        if (cmd === 'update_agent') {
          return pendingPromise;
        }
        return null;
      });

      await act(async () => {
        render(
          <AgentEditForm
            agent={mockAgent}
            onSuccess={mockOnSuccess}
            onCancel={mockOnCancel}
          />
        );
        vi.advanceTimersByTime(100);
      });

      // 修改名称以启用保存按钮
      const nameInput = screen.getByLabelText(/名称/i);
      fireEvent.change(nameInput, { target: { value: 'Updated Name' } });

      const saveButton = screen.getByRole('button', { name: /保存/i });
      await act(async () => {
        fireEvent.click(saveButton);
        vi.advanceTimersByTime(100);
      });

      // 表单字段应该被禁用
      expect(screen.getByLabelText(/名称/i)).toBeDisabled();
      expect(screen.getByLabelText(/描述/i)).toBeDisabled();
      expect(screen.getByLabelText(/专业领域/i)).toBeDisabled();

      // Resolve promise
      resolvePromise!(JSON.stringify(mockAgent));

      await act(async () => {
        vi.advanceTimersByTime(100);
      });

      expect(screen.getByLabelText(/名称/i)).not.toBeDisabled();
    });
  });
});