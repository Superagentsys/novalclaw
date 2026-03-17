/**
 * AgentEditPage 页面测试
 *
 * 测试代理编辑页面的功能:
 * - 从 URL 参数获取 agent UUID
 * - 加载代理数据
 * - 加载状态（骨架屏）
 * - 代理不存在情况（404）
 * - 保存成功后的导航
 * - 取消后的导航
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, act } from '../utils';
import { AgentEditPage } from '@/pages/AgentEditPage';
import { invoke } from '@tauri-apps/api/core';
import { type MBTIType } from '@/lib/personality-colors';

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

// Mock react-router-dom
const mockNavigate = vi.fn();
const mockParams = { uuid: 'test-uuid-123' };

vi.mock('react-router-dom', () => ({
  useNavigate: () => mockNavigate,
  useParams: () => mockParams,
}));

// Mock data
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

const createMockAgent = (overrides?: Partial<{
  id: number;
  agent_uuid: string;
  name: string;
  description?: string;
  domain?: string;
  mbti_type?: MBTIType;
  status: 'active' | 'inactive' | 'archived';
  created_at: number;
  updated_at: number;
}>) => ({
  id: 1,
  agent_uuid: 'test-uuid-123',
  name: '测试代理',
  description: '这是一个测试代理',
  domain: '测试领域',
  mbti_type: 'INTJ' as MBTIType,
  status: 'active' as const,
  created_at: Date.now(),
  updated_at: Date.now(),
  ...overrides,
});

/**
 * 设置默认的 mock 实现
 */
function setupDefaultMocks() {
  const mockInvoke = vi.mocked(invoke);
  mockInvoke.mockImplementation(async (cmd: string) => {
    if (cmd === 'get_agent_by_id') {
      return JSON.stringify(createMockAgent());
    }
    if (cmd === 'get_mbti_config') {
      return mockMBTIConfig;
    }
    if (cmd === 'get_mbti_traits') {
      return mockMBTITraits;
    }
    if (cmd === 'update_agent') {
      return JSON.stringify(createMockAgent());
    }
    return null;
  });
}

describe('AgentEditPage', () => {
  const mockInvoke = vi.mocked(invoke);

  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
    mockParams.uuid = 'test-uuid-123';
    setupDefaultMocks();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe('数据加载', () => {
    it('应该在加载时显示加载状态', async () => {
      // 让 invoke 永不 resolve
      mockInvoke.mockImplementation(() => new Promise(() => {}));

      render(<AgentEditPage />);

      // 应该显示骨架屏
      expect(document.querySelector('.animate-pulse')).toBeInTheDocument();
    });

    it('应该调用 get_agent_by_id 命令获取代理数据', async () => {
      await act(async () => {
        render(<AgentEditPage />);
        vi.advanceTimersByTime(100);
      });

      expect(mockInvoke).toHaveBeenCalledWith('get_agent_by_id', {
        uuid: 'test-uuid-123',
      });
    });

    it('应该在加载完成后显示代理名称', async () => {
      await act(async () => {
        render(<AgentEditPage />);
        vi.advanceTimersByTime(100);
      });

      expect(screen.getByDisplayValue('测试代理')).toBeInTheDocument();
    });
  });

  describe('404 处理', () => {
    it('代理不存在时应显示 404 页面', async () => {
      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === 'get_agent_by_id') {
          return null; // 代理不存在
        }
        return null;
      });

      await act(async () => {
        render(<AgentEditPage />);
        vi.advanceTimersByTime(100);
      });

      expect(screen.getByText(/代理不存在/i)).toBeInTheDocument();
    });

    it('404 页面应有返回列表按钮', async () => {
      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === 'get_agent_by_id') {
          return null;
        }
        return null;
      });

      await act(async () => {
        render(<AgentEditPage />);
        vi.advanceTimersByTime(100);
      });

      expect(screen.getByRole('button', { name: /返回列表/i })).toBeInTheDocument();
    });

    it('点击返回列表按钮应导航到代理列表', async () => {
      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === 'get_agent_by_id') {
          return null;
        }
        return null;
      });

      await act(async () => {
        render(<AgentEditPage />);
        vi.advanceTimersByTime(100);
      });

      fireEvent.click(screen.getByRole('button', { name: /返回列表/i }));

      expect(mockNavigate).toHaveBeenCalledWith('/agents');
    });
  });

  describe('错误处理', () => {
    it('加载失败时应显示错误信息', async () => {
      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === 'get_agent_by_id') {
          throw new Error('加载失败');
        }
        return null;
      });

      await act(async () => {
        render(<AgentEditPage />);
        vi.advanceTimersByTime(100);
      });

      // 检查错误信息存在（可能多个元素包含"加载失败"）
      const errorElements = screen.getAllByText(/加载失败/i);
      expect(errorElements.length).toBeGreaterThan(0);
    });

    it('错误状态应有重试按钮', async () => {
      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === 'get_agent_by_id') {
          throw new Error('加载失败');
        }
        return null;
      });

      await act(async () => {
        render(<AgentEditPage />);
        vi.advanceTimersByTime(100);
      });

      expect(screen.getByRole('button', { name: /重试/i })).toBeInTheDocument();
    });
  });

  describe('页面头部', () => {
    it('应显示编辑代理标题', async () => {
      await act(async () => {
        render(<AgentEditPage />);
        vi.advanceTimersByTime(100);
      });

      expect(screen.getByRole('heading', { name: /编辑代理/i })).toBeInTheDocument();
    });

    it('应显示返回按钮', async () => {
      await act(async () => {
        render(<AgentEditPage />);
        vi.advanceTimersByTime(100);
      });

      expect(screen.getByRole('button', { name: /返回/i })).toBeInTheDocument();
    });

    it('点击返回按钮应调用 navigate(-1)', async () => {
      await act(async () => {
        render(<AgentEditPage />);
        vi.advanceTimersByTime(100);
      });

      fireEvent.click(screen.getByRole('button', { name: /返回/i }));

      expect(mockNavigate).toHaveBeenCalledWith(-1);
    });
  });

  describe('表单集成', () => {
    it('应渲染 AgentEditForm 组件', async () => {
      await act(async () => {
        render(<AgentEditPage />);
        vi.advanceTimersByTime(100);
      });

      // 检查表单元素
      expect(screen.getByLabelText(/名称/i)).toBeInTheDocument();
    });

    it('表单应预填充代理数据', async () => {
      await act(async () => {
        render(<AgentEditPage />);
        vi.advanceTimersByTime(100);
      });

      expect(screen.getByDisplayValue('测试代理')).toBeInTheDocument();
      expect(screen.getByDisplayValue('这是一个测试代理')).toBeInTheDocument();
      expect(screen.getByDisplayValue('测试领域')).toBeInTheDocument();
    });
  });

  describe('保存成功导航', () => {
    it('保存成功后应导航到代理列表', async () => {
      await act(async () => {
        render(<AgentEditPage />);
        vi.advanceTimersByTime(100);
      });

      // 修改名称以启用保存按钮
      const nameInput = screen.getByLabelText(/名称/i);
      fireEvent.change(nameInput, { target: { value: '更新后的名称' } });

      await act(async () => {
        vi.advanceTimersByTime(100);
      });

      const saveButton = screen.getByRole('button', { name: /保存/i });
      fireEvent.click(saveButton);

      await act(async () => {
        vi.advanceTimersByTime(100);
      });

      expect(mockNavigate).toHaveBeenCalledWith('/agents');
    });
  });

  describe('取消导航', () => {
    it('点击取消应调用 navigate(-1)', async () => {
      await act(async () => {
        render(<AgentEditPage />);
        vi.advanceTimersByTime(100);
      });

      fireEvent.click(screen.getByRole('button', { name: /取消/i }));

      expect(mockNavigate).toHaveBeenCalledWith(-1);
    });
  });
});