/**
 * PersonalityPreview 组件单元测试
 *
 * 测试覆盖:
 * - 组件加载状态
 * - 配置数据正确渲染
 * - 错误处理
 * - 不同人格类型的显示
 * - 认知功能栈显示
 * - 示例对话显示
 * - 优势和盲点显示
 * - 推荐应用场景显示
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '../utils';
import { PersonalityPreview } from '@/components/agent/PersonalityPreview';
import { invoke } from '@tauri-apps/api/core';

// Mock Tauri API
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

// Mock data for testing
const mockPersonalityConfig = {
  description: '富有想象力的战略家，雄心勃勃，有长远眼光',
  system_prompt_template: 'You are an INTJ...',
  strengths: ['战略思维', '独立判断', '意志坚定'],
  blind_spots: ['可能过于傲慢', '可能缺乏耐心'],
  recommended_use_cases: ['战略规划', '技术架构设计', '系统分析'],
  theme_color: '#2563EB',
  accent_color: '#787163',
};

const mockPersonalityTraits = {
  function_stack: {
    dominant: 'Ni',
    auxiliary: 'Te',
    tertiary: 'Fi',
    inferior: 'Se',
  },
  behavior_tendency: {
    decision_making: '理性分析',
    information_processing: '直觉洞察',
    social_interaction: '独立自主',
    stress_response: '深思熟虑',
  },
  communication_style: {
    preference: '直接简洁',
    language_traits: ['逻辑性强', '结构化', '注重效率'],
    feedback_style: '客观坦率',
  },
};

/**
 * 创建模拟的 invoke 函数
 * 根据命令名返回对应的模拟数据
 */
function createMockInvoke(config: typeof mockPersonalityConfig, traits: typeof mockPersonalityTraits) {
  return vi.fn().mockImplementation((command: string) => {
    if (command === 'get_mbti_config') {
      return Promise.resolve(config);
    }
    if (command === 'get_mbti_traits') {
      return Promise.resolve(traits);
    }
    return Promise.resolve(null);
  });
}

describe('PersonalityPreview', () => {
  const mockInvoke = vi.mocked(invoke);

  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  describe('加载状态', () => {
    it('当加载数据时应该显示加载指示器', async () => {
      // 设置永不 resolve 的 promise
      mockInvoke.mockImplementation(() => new Promise(() => {}));

      render(<PersonalityPreview mbtiType="INTJ" />);

      expect(screen.getByText(/加载中/i)).toBeInTheDocument();
    });
  });

  describe('错误处理', () => {
    it('当加载失败时应该显示错误信息', async () => {
      mockInvoke.mockRejectedValue(new Error('Network error'));

      render(<PersonalityPreview mbtiType="INTJ" />);

      await waitFor(() => {
        expect(screen.getByText(/加载失败/i)).toBeInTheDocument();
      });
    });

    it('当加载失败时应该显示重试按钮', async () => {
      mockInvoke.mockRejectedValue(new Error('Network error'));

      render(<PersonalityPreview mbtiType="INTJ" />);

      await waitFor(() => {
        expect(screen.getByRole('button', { name: /重试/i })).toBeInTheDocument();
      });
    });

    it('点击重试按钮应该重新加载数据', async () => {
      mockInvoke
        .mockRejectedValueOnce(new Error('Network error'))
        .mockImplementation((cmd: string) => {
          if (cmd === 'get_mbti_config') return Promise.resolve(mockPersonalityConfig);
          if (cmd === 'get_mbti_traits') return Promise.resolve(mockPersonalityTraits);
          return Promise.resolve(null);
        });

      render(<PersonalityPreview mbtiType="INTJ" />);

      await waitFor(() => {
        expect(screen.getByRole('button', { name: /重试/i })).toBeInTheDocument();
      });

      const retryButton = screen.getByRole('button', { name: /重试/i });
      fireEvent.click(retryButton);

      await waitFor(() => {
        expect(screen.queryByText(/加载失败/i)).not.toBeInTheDocument();
      });
    });
  });

  describe('基础渲染', () => {
    beforeEach(() => {
      mockInvoke.mockImplementation(createMockInvoke(mockPersonalityConfig, mockPersonalityTraits));
    });

    it('应该显示人格类型名称', async () => {
      render(<PersonalityPreview mbtiType="INTJ" />);

      await waitFor(() => {
        expect(screen.getByText('INTJ')).toBeInTheDocument();
      });
    });

    it('应该显示人格类型描述', async () => {
      render(<PersonalityPreview mbtiType="INTJ" />);

      await waitFor(() => {
        expect(screen.getByText(/富有想象力的战略家/)).toBeInTheDocument();
      });
    });

    it('应该应用自定义 className', async () => {
      const { container } = render(<PersonalityPreview mbtiType="INTJ" className="custom-class" />);

      await waitFor(() => {
        expect(container.firstChild).toHaveClass('custom-class');
      });
    });
  });

  describe('认知功能栈显示', () => {
    beforeEach(() => {
      mockInvoke.mockImplementation(createMockInvoke(mockPersonalityConfig, mockPersonalityTraits));
    });

    it('应该显示认知功能栈标题', async () => {
      render(<PersonalityPreview mbtiType="INTJ" />);

      await waitFor(() => {
        expect(screen.getByText(/认知功能栈/i)).toBeInTheDocument();
      });
    });

    it('应该显示四个认知功能', async () => {
      render(<PersonalityPreview mbtiType="INTJ" />);

      await waitFor(() => {
        expect(screen.getByText('Ni')).toBeInTheDocument();
        expect(screen.getByText('Te')).toBeInTheDocument();
        expect(screen.getByText('Fi')).toBeInTheDocument();
        expect(screen.getByText('Se')).toBeInTheDocument();
      });
    });

    it('应该显示功能角色标签（主导、辅助等）', async () => {
      render(<PersonalityPreview mbtiType="INTJ" />);

      await waitFor(() => {
        expect(screen.getByText('主导')).toBeInTheDocument();
        expect(screen.getByText('辅助')).toBeInTheDocument();
        expect(screen.getByText('第三')).toBeInTheDocument();
        expect(screen.getByText('劣势')).toBeInTheDocument();
      });
    });
  });

  describe('示例对话显示', () => {
    beforeEach(() => {
      mockInvoke.mockImplementation(createMockInvoke(mockPersonalityConfig, mockPersonalityTraits));
    });

    it('应该显示示例对话标题', async () => {
      render(<PersonalityPreview mbtiType="INTJ" />);

      await waitFor(() => {
        expect(screen.getByText(/示例对话风格/i)).toBeInTheDocument();
      });
    });

    it('应该显示示例对话内容', async () => {
      render(<PersonalityPreview mbtiType="INTJ" />);

      await waitFor(() => {
        // INTJ 的示例对话内容
        expect(screen.getByText(/基于您的需求/i)).toBeInTheDocument();
      });
    });

    it('应该显示沟通风格特点', async () => {
      render(<PersonalityPreview mbtiType="INTJ" />);

      await waitFor(() => {
        expect(screen.getByText(/直接|结构化|注重效率/i)).toBeInTheDocument();
      });
    });
  });

  describe('优势和盲点显示', () => {
    beforeEach(() => {
      mockInvoke.mockImplementation(createMockInvoke(mockPersonalityConfig, mockPersonalityTraits));
    });

    it('应该显示优势标题', async () => {
      render(<PersonalityPreview mbtiType="INTJ" />);

      await waitFor(() => {
        expect(screen.getByText(/优势/i)).toBeInTheDocument();
      });
    });

    it('应该显示所有优势项', async () => {
      render(<PersonalityPreview mbtiType="INTJ" />);

      await waitFor(() => {
        expect(screen.getByText('战略思维')).toBeInTheDocument();
        expect(screen.getByText('独立判断')).toBeInTheDocument();
        expect(screen.getByText('意志坚定')).toBeInTheDocument();
      });
    });

    it('应该显示盲点标题', async () => {
      render(<PersonalityPreview mbtiType="INTJ" />);

      await waitFor(() => {
        expect(screen.getByText(/潜在盲点/i)).toBeInTheDocument();
      });
    });

    it('应该显示所有盲点项', async () => {
      render(<PersonalityPreview mbtiType="INTJ" />);

      await waitFor(() => {
        expect(screen.getByText(/可能过于傲慢/)).toBeInTheDocument();
        expect(screen.getByText(/可能缺乏耐心/)).toBeInTheDocument();
      });
    });
  });

  describe('应用场景推荐', () => {
    beforeEach(() => {
      mockInvoke.mockImplementation(createMockInvoke(mockPersonalityConfig, mockPersonalityTraits));
    });

    it('应该显示应用场景标题', async () => {
      render(<PersonalityPreview mbtiType="INTJ" />);

      await waitFor(() => {
        expect(screen.getByText(/建议应用场景/i)).toBeInTheDocument();
      });
    });

    it('应该显示所有推荐场景', async () => {
      render(<PersonalityPreview mbtiType="INTJ" />);

      await waitFor(() => {
        expect(screen.getByText('战略规划')).toBeInTheDocument();
        expect(screen.getByText('技术架构设计')).toBeInTheDocument();
        expect(screen.getByText('系统分析')).toBeInTheDocument();
      });
    });
  });

  describe('不同人格类型', () => {
    it('应该正确显示 ENFP 类型的数据', async () => {
      const enfpConfig = {
        description: '热情的探索者，富有创造力和感染力',
        strengths: ['热情洋溢', '创意丰富', '善于鼓励'],
        blind_spots: ['可能过于理想化', '可能难以专注'],
        recommended_use_cases: ['团队激励', '创意策划', '产品探索'],
        theme_color: '#EA580C',
        accent_color: '#0D9488',
        system_prompt_template: 'You are an ENFP...',
      };

      const enfpTraits = {
        function_stack: {
          dominant: 'Ne',
          auxiliary: 'Fi',
          tertiary: 'Te',
          inferior: 'Si',
        },
        behavior_tendency: {
          decision_making: '价值驱动',
          information_processing: '探索可能',
          social_interaction: '热情互动',
          stress_response: '积极应对',
        },
        communication_style: {
          preference: '热情生动',
          language_traits: ['富有感染力', '创意表达', '关注人际'],
          feedback_style: '积极鼓励',
        },
      };

      mockInvoke.mockImplementation(createMockInvoke(enfpConfig, enfpTraits));

      render(<PersonalityPreview mbtiType="ENFP" />);

      await waitFor(() => {
        expect(screen.getByText('ENFP')).toBeInTheDocument();
        expect(screen.getByText(/热情的探索者/)).toBeInTheDocument();
        expect(screen.getByText('Ne')).toBeInTheDocument();
        expect(screen.getByText('热情洋溢')).toBeInTheDocument();
      });
    });
  });

  describe('视觉设计', () => {
    beforeEach(() => {
      mockInvoke.mockImplementation(createMockInvoke(mockPersonalityConfig, mockPersonalityTraits));
    });

    it('应该使用人格类型的主题色', async () => {
      render(<PersonalityPreview mbtiType="INTJ" />);

      await waitFor(() => {
        // 检查是否有使用主题色的元素
        const coloredElements = document.querySelectorAll('[style*="#2563EB"]');
        expect(coloredElements.length).toBeGreaterThan(0);
      });
    });

    it('hover 状态应该有过渡动画', async () => {
      const { container } = render(<PersonalityPreview mbtiType="INTJ" />);

      await waitFor(() => {
        const transitionElements = container.querySelectorAll('[class*="transition"]');
        expect(transitionElements.length).toBeGreaterThan(0);
      });
    });
  });
});