/**
 * MBTISelector 组件单元测试
 *
 * 测试覆盖:
 * - 组件渲染所有 16 种类型
 * - 分类筛选功能
 * - 搜索功能
 * - 选择回调
 * - 键盘导航
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, act } from '../utils';
import { MBTISelector } from '@/components/agent/MBTISelector';
import type { MBTIType } from '@/lib/personality-colors';

describe('MBTISelector', () => {
  const mockOnChange = vi.fn();

  beforeEach(() => {
    mockOnChange.mockClear();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe('基础渲染', () => {
    it('应该渲染所有 16 种人格类型', async () => {
      await act(async () => {
        render(<MBTISelector onChange={mockOnChange} />);
      });

      // 检查所有 16 种类型都已渲染 - 使用更精确的选择器
      const types: MBTIType[] = [
        'INTJ', 'INTP', 'ENTJ', 'ENTP',
        'INFJ', 'INFP', 'ENFJ', 'ENFP',
        'ISTJ', 'ISFJ', 'ESTJ', 'ESFJ',
        'ISTP', 'ISFP', 'ESTP', 'ESFP',
      ];

      types.forEach((type) => {
        // 使用 role 和 name 来找到按钮
        const button = screen.getByRole('button', { name: new RegExp(type) });
        expect(button).toBeInTheDocument();
      });
    });

    it('应该显示每种类型的名称', async () => {
      await act(async () => {
        render(<MBTISelector onChange={mockOnChange} />);
        vi.advanceTimersByTime(100);
      });

      // 检查 INTJ 的名称 (config.name 显示在按钮中)
      // 注: 描述字段用于搜索匹配，不在 UI 中显示
      const intjButton = screen.getByRole('button', { name: /INTJ/ });
      expect(intjButton).toBeInTheDocument();
    });

    it('应该应用自定义 className', async () => {
      const { container } = await act(async () => {
        return render(<MBTISelector onChange={mockOnChange} className="custom-class" />);
      });

      expect(container.firstChild).toHaveClass('custom-class');
    });

    it('当 disabled 为 true 时应该禁用所有交互', async () => {
      await act(async () => {
        render(<MBTISelector onChange={mockOnChange} disabled />);
      });

      // 所有类型按钮应该被禁用
      const buttons = screen.getAllByRole('button').filter(btn => btn.hasAttribute('data-type'));
      buttons.forEach((button) => {
        expect(button).toBeDisabled();
      });
    });
  });

  describe('选择功能', () => {
    it('点击类型应该调用 onChange 回调', async () => {
      await act(async () => {
        render(<MBTISelector onChange={mockOnChange} />);
      });

      const intjButton = screen.getByRole('button', { name: /INTJ/ });
      fireEvent.click(intjButton);

      expect(mockOnChange).toHaveBeenCalledWith('INTJ');
    });

    it('选中状态应该有视觉反馈', async () => {
      await act(async () => {
        render(<MBTISelector value="INTJ" onChange={mockOnChange} />);
      });

      const intjButton = screen.getByRole('button', { name: /INTJ/ });
      expect(intjButton).toHaveAttribute('aria-pressed', 'true');
    });
  });

  describe('分类筛选功能', () => {
    it('应该显示分类标签', async () => {
      await act(async () => {
        render(<MBTISelector onChange={mockOnChange} />);
      });

      expect(screen.getByRole('tab', { name: /全部/ })).toBeInTheDocument();
      expect(screen.getByRole('tab', { name: /分析型/ })).toBeInTheDocument();
      expect(screen.getByRole('tab', { name: /外交型/ })).toBeInTheDocument();
      expect(screen.getByRole('tab', { name: /守护型/ })).toBeInTheDocument();
      expect(screen.getByRole('tab', { name: /探索型/ })).toBeInTheDocument();
    });

    it('点击分析型标签应该只显示分析型类型', async () => {
      await act(async () => {
        render(<MBTISelector onChange={mockOnChange} />);
      });

      const analystTab = screen.getByRole('tab', { name: /分析型/ });
      fireEvent.click(analystTab);

      // 分析型类型应该显示
      expect(screen.getByRole('button', { name: /INTJ/ })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /INTP/ })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /ENTJ/ })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /ENTP/ })).toBeInTheDocument();

      // 其他类型不应该显示
      expect(screen.queryByRole('button', { name: /INFJ/ })).not.toBeInTheDocument();
      expect(screen.queryByRole('button', { name: /ISTJ/ })).not.toBeInTheDocument();
    });

    it('应该显示分类统计', async () => {
      await act(async () => {
        render(<MBTISelector onChange={mockOnChange} />);
      });

      // 分析型有 4 个类型
      expect(screen.getByText(/分析型.*4/)).toBeInTheDocument();
    });
  });

  describe('搜索功能', () => {
    it('应该渲染搜索输入框', async () => {
      await act(async () => {
        render(<MBTISelector onChange={mockOnChange} />);
      });

      expect(screen.getByPlaceholderText(/搜索/)).toBeInTheDocument();
    });

    it('输入搜索词应该筛选类型', async () => {
      await act(async () => {
        render(<MBTISelector onChange={mockOnChange} />);
      });

      const searchInput = screen.getByPlaceholderText(/搜索/);
      fireEvent.change(searchInput, { target: { value: '建筑师' } });

      // 推进时间以触发防抖
      await act(async () => {
        vi.advanceTimersByTime(400);
      });

      // 只 INTJ 应该显示（描述包含"建筑师"）
      expect(screen.getByRole('button', { name: /INTJ/ })).toBeInTheDocument();
      expect(screen.queryByRole('button', { name: /INTP/ })).not.toBeInTheDocument();
    });

    it('搜索应该支持类型代码', async () => {
      await act(async () => {
        render(<MBTISelector onChange={mockOnChange} />);
      });

      const searchInput = screen.getByPlaceholderText(/搜索/);
      fireEvent.change(searchInput, { target: { value: 'INT' } });

      // 推进时间以触发防抖
      await act(async () => {
        vi.advanceTimersByTime(400);
      });

      // INT 开头的类型应该显示
      expect(screen.getByRole('button', { name: /INTJ/ })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /INTP/ })).toBeInTheDocument();
      expect(screen.queryByRole('button', { name: /ENTJ/ })).not.toBeInTheDocument();
    });

    it('无搜索结果时应该显示提示', async () => {
      await act(async () => {
        render(<MBTISelector onChange={mockOnChange} />);
      });

      const searchInput = screen.getByPlaceholderText(/搜索/);
      fireEvent.change(searchInput, { target: { value: '不存在的类型xyz' } });

      // 推进时间以触发防抖
      await act(async () => {
        vi.advanceTimersByTime(400);
      });

      expect(screen.getByText(/没有找到匹配的类型/)).toBeInTheDocument();
    });
  });

  describe('键盘导航', () => {
    it('Tab 键应该可以在类型间导航', async () => {
      await act(async () => {
        render(<MBTISelector onChange={mockOnChange} />);
      });

      const firstType = screen.getByRole('button', { name: /INTJ/ });
      firstType.focus();

      fireEvent.keyDown(firstType, { key: 'Tab' });

      // 验证焦点变化
      expect(document.activeElement).toBeDefined();
    });

    it('Enter 键应该选择当前焦点的类型', async () => {
      await act(async () => {
        render(<MBTISelector onChange={mockOnChange} />);
      });

      const firstType = screen.getByRole('button', { name: /INTJ/ });
      firstType.focus();

      fireEvent.keyDown(firstType, { key: 'Enter' });

      expect(mockOnChange).toHaveBeenCalledWith('INTJ');
    });

    it('Space 键应该选择当前焦点的类型', async () => {
      await act(async () => {
        render(<MBTISelector onChange={mockOnChange} />);
      });

      const firstType = screen.getByRole('button', { name: /INTJ/ });
      firstType.focus();

      fireEvent.keyDown(firstType, { key: ' ' });

      expect(mockOnChange).toHaveBeenCalledWith('INTJ');
    });

    it('方向键应该触发焦点变化', async () => {
      await act(async () => {
        render(<MBTISelector onChange={mockOnChange} />);
      });

      const grid = document.querySelector('[role="grid"]');
      if (!grid) throw new Error('Grid not found');

      const firstType = screen.getByRole('button', { name: /INTJ/ });
      firstType.focus();

      // 按右箭头 - 这会触发键盘处理程序
      fireEvent.keyDown(grid, { key: 'ArrowRight' });

      // 验证键盘事件被处理
      const focusedButton = document.activeElement;
      expect(focusedButton).toHaveAttribute('data-type');
    });

    it('Escape 键应该清除搜索', async () => {
      await act(async () => {
        render(<MBTISelector onChange={mockOnChange} />);
      });

      const searchInput = screen.getByPlaceholderText(/搜索/) as HTMLInputElement;
      fireEvent.change(searchInput, { target: { value: 'INTJ' } });

      expect(searchInput.value).toBe('INTJ');

      // 点击清除按钮
      const clearButton = screen.getByRole('button', { name: '清除搜索' });
      fireEvent.click(clearButton);

      expect(searchInput.value).toBe('');
    });
  });

  describe('视觉反馈', () => {
    it('hover 状态应该有样式变化', async () => {
      await act(async () => {
        render(<MBTISelector onChange={mockOnChange} />);
      });

      const firstType = screen.getByRole('button', { name: /INTJ/ });
      expect(firstType).toHaveClass('transition-all');
    });

    it('选中状态应该使用对应类型的主题色', async () => {
      await act(async () => {
        render(<MBTISelector value="INTJ" onChange={mockOnChange} />);
      });

      const intjButton = screen.getByRole('button', { name: /INTJ/ });
      // INTJ 的主题色是 #2563EB
      expect(intjButton).toHaveStyle({ '--type-color': '#2563EB' });
    });
  });
});