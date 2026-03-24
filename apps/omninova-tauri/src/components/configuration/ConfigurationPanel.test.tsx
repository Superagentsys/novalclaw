/**
 * ConfigurationPanel Tests
 *
 * Tests for the main configuration panel component.
 * [Source: Story 7.7 - ConfigurationPanel 组件]
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ConfigurationPanel } from './ConfigurationPanel';
import { invoke } from '@tauri-apps/api/core';

// Mock the invoke function
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

// Mock child components
vi.mock('@/components/agent/AgentStyleConfigForm', () => ({
  AgentStyleConfigForm: ({ config, onChange }: { config: Record<string, unknown>; onChange: (c: Record<string, unknown>) => void }) => (
    <div data-testid="style-config-form">
      <span data-testid="style-config-value">{JSON.stringify(config)}</span>
      <button
        data-testid="change-style"
        onClick={() => onChange({ ...config, formality: 'changed' })}
      >
        Change Style
      </button>
    </div>
  ),
}));

vi.mock('@/components/agent/ContextWindowConfigForm', () => ({
  ContextWindowConfigForm: ({ config, onChange }: { config: Record<string, unknown>; onChange: (c: Record<string, unknown>) => void }) => (
    <div data-testid="context-config-form">
      <span data-testid="context-config-value">{JSON.stringify(config)}</span>
      <button
        data-testid="change-context"
        onClick={() => onChange({ ...config, maxTokens: 9999 })}
      >
        Change Context
      </button>
    </div>
  ),
}));

vi.mock('@/components/agent/TriggerKeywordsConfigForm', () => ({
  TriggerKeywordsConfigForm: ({ config }: { config: Record<string, unknown> }) => (
    <div data-testid="trigger-config-form">
      <span data-testid="trigger-config-value">{JSON.stringify(config)}</span>
    </div>
  ),
}));

vi.mock('@/components/agent/PrivacyConfigForm', () => ({
  PrivacyConfigForm: ({ config }: { config: Record<string, unknown> }) => (
    <div data-testid="privacy-config-form">
      <span data-testid="privacy-config-value">{JSON.stringify(config)}</span>
    </div>
  ),
}));

vi.mock('@/components/skills/SkillManagementPanel', () => ({
  SkillManagementPanel: ({ agentId }: { agentId: string }) => (
    <div data-testid="skill-management-panel">
      <span data-testid="skill-agent-id">{agentId}</span>
    </div>
  ),
}));

describe('ConfigurationPanel', () => {
  const mockInvoke = vi.mocked(invoke);
  const defaultProps = {
    agentId: 'test-agent-id',
  };

  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockResolvedValue({
      style_config: JSON.stringify({ formality: 'professional', verbosity: 0.5 }),
      context_window_config: JSON.stringify({ maxTokens: 4096 }),
      trigger_keywords_config: JSON.stringify({ keywords: [] }),
      privacy_config: JSON.stringify({ dataRetention: { episodicMemoryDays: 30 } }),
    });
  });

  describe('AC1: 选项卡式界面', () => {
    it('should render tabs for basic, advanced, and skills settings', async () => {
      render(<ConfigurationPanel {...defaultProps} />);

      // Wait for loading to complete
      await waitFor(() => {
        expect(screen.getByRole('tab', { name: /基础设置/ })).toBeInTheDocument();
      });

      expect(screen.getByRole('tab', { name: /高级设置/ })).toBeInTheDocument();
      expect(screen.getByRole('tab', { name: /技能管理/ })).toBeInTheDocument();
    });

    it('should switch between tabs', async () => {
      render(<ConfigurationPanel {...defaultProps} />);

      await waitFor(() => {
        expect(screen.getByRole('tab', { name: /基础设置/ })).toBeInTheDocument();
      });

      // Click on Advanced tab
      await userEvent.click(screen.getByRole('tab', { name: /高级设置/ }));

      // Should show context window config in advanced tab
      expect(screen.getByTestId('context-config-form')).toBeInTheDocument();
    });
  });

  describe('AC2: 渐进披露', () => {
    it('should show advanced settings collapsed by default in basic tab', async () => {
      render(<ConfigurationPanel {...defaultProps} />);

      await waitFor(() => {
        expect(screen.getByRole('tab', { name: /基础设置/ })).toBeInTheDocument();
      });

      // Find the expandable advanced section - look for the badge text
      expect(screen.getByText(/上下文窗口/)).toBeInTheDocument();
    });

    it('should expand advanced settings when clicked in basic tab', async () => {
      render(<ConfigurationPanel {...defaultProps} />);

      await waitFor(() => {
        expect(screen.getByTestId('style-config-form')).toBeInTheDocument();
      });

      // Find the expand button by the badge text container
      const expandButton = screen.getByRole('button', { name: /上下文窗口/ });
      await userEvent.click(expandButton);

      // Should show context window config
      expect(screen.getByTestId('context-config-form')).toBeInTheDocument();
    });
  });

  describe('AC3: 保存/取消', () => {
    it('should show save and cancel buttons when there are changes', async () => {
      render(<ConfigurationPanel {...defaultProps} />);

      await waitFor(() => {
        expect(screen.getByTestId('style-config-form')).toBeInTheDocument();
      });

      // Make a change
      await userEvent.click(screen.getByTestId('change-style'));

      // Buttons should be enabled
      expect(screen.getByText('保存')).not.toBeDisabled();
      expect(screen.getByText('取消')).not.toBeDisabled();
    });

    it('should cancel changes and revert to original', async () => {
      render(<ConfigurationPanel {...defaultProps} />);

      await waitFor(() => {
        expect(screen.getByTestId('style-config-form')).toBeInTheDocument();
      });

      // Get initial value
      const initialValue = screen.getByTestId('style-config-value').textContent;

      // Make a change
      await userEvent.click(screen.getByTestId('change-style'));

      // Value should change
      expect(screen.getByTestId('style-config-value').textContent).not.toBe(initialValue);

      // Click cancel
      await userEvent.click(screen.getByText('取消'));

      // Value should revert
      expect(screen.getByTestId('style-config-value').textContent).toBe(initialValue);
    });

    it('should save configuration successfully', async () => {
      const onSave = vi.fn().mockResolvedValue(undefined);
      render(<ConfigurationPanel {...defaultProps} onSave={onSave} />);

      await waitFor(() => {
        expect(screen.getByTestId('style-config-form')).toBeInTheDocument();
      });

      // Make a change
      await userEvent.click(screen.getByTestId('change-style'));

      // Click save
      await userEvent.click(screen.getByText('保存'));

      // Should call onSave
      await waitFor(() => {
        expect(onSave).toHaveBeenCalled();
      });
    });
  });

  describe('AC4: 配置预览', () => {
    it('should show preview dialog when preview button is clicked', async () => {
      render(<ConfigurationPanel {...defaultProps} />);

      await waitFor(() => {
        expect(screen.getByTestId('style-config-form')).toBeInTheDocument();
      });

      // Make a change first
      await userEvent.click(screen.getByTestId('change-style'));

      // Click preview button
      await userEvent.click(screen.getByText('预览更改'));

      // Should show preview dialog
      expect(screen.getByText('配置变更预览')).toBeInTheDocument();
    });

    it('should show changes in preview dialog', async () => {
      render(<ConfigurationPanel {...defaultProps} />);

      await waitFor(() => {
        expect(screen.getByTestId('style-config-form')).toBeInTheDocument();
      });

      // Make a change
      await userEvent.click(screen.getByTestId('change-style'));

      // Click preview
      await userEvent.click(screen.getByText('预览更改'));

      // Should show the changed path
      await waitFor(() => {
        expect(screen.getByText(/styleConfig/)).toBeInTheDocument();
      });
    });
  });

  describe('AC5: 错误提示', () => {
    it('should show error when save fails', async () => {
      mockInvoke.mockRejectedValue(new Error('Save failed'));

      render(<ConfigurationPanel {...defaultProps} />);

      await waitFor(() => {
        expect(screen.getByTestId('style-config-form')).toBeInTheDocument();
      });

      // Make a change
      await userEvent.click(screen.getByTestId('change-style'));

      // Click save
      await userEvent.click(screen.getByText('保存'));

      // Should show error
      await waitFor(() => {
        expect(screen.getByText('Save failed')).toBeInTheDocument();
      });
    });
  });

  describe('AC6: 重置默认', () => {
    it('should show reset confirmation dialog', async () => {
      render(<ConfigurationPanel {...defaultProps} />);

      await waitFor(() => {
        expect(screen.getByRole('button', { name: /重置默认/ })).toBeInTheDocument();
      });

      // Click reset button
      await userEvent.click(screen.getByRole('button', { name: /重置默认/ }));

      // Should show confirmation dialog
      expect(screen.getByText('重置配置')).toBeInTheDocument();
      expect(screen.getByText(/确定要将所有配置重置为默认值吗/)).toBeInTheDocument();
    });

    it('should reset to defaults when confirmed', async () => {
      render(<ConfigurationPanel {...defaultProps} />);

      await waitFor(() => {
        expect(screen.getByTestId('style-config-form')).toBeInTheDocument();
      });

      // Make a change first
      await userEvent.click(screen.getByTestId('change-style'));

      // Open reset dialog
      await userEvent.click(screen.getByRole('button', { name: /重置默认/ }));

      // Confirm reset
      await userEvent.click(screen.getByRole('button', { name: /确认重置/ }));

      // Configuration should be reset
      await waitFor(() => {
        expect(screen.getByTestId('style-config-value').textContent).not.toContain('changed');
      });
    });
  });

  describe('Loading state', () => {
    it('should show loading spinner while loading', () => {
      // Make invoke return a promise that doesn't resolve immediately
      mockInvoke.mockImplementation(() => new Promise(() => {}));

      render(<ConfigurationPanel {...defaultProps} />);

      // Should show loading spinner
      expect(document.querySelector('.animate-spin')).toBeInTheDocument();
    });
  });

  describe('Disabled state', () => {
    it('should disable all interactive elements when disabled', async () => {
      render(<ConfigurationPanel {...defaultProps} disabled />);

      await waitFor(() => {
        expect(screen.getByText('重置默认')).toBeInTheDocument();
      });

      // All buttons should be disabled
      expect(screen.getByText('重置默认')).toBeDisabled();
      expect(screen.getByText('预览更改')).toBeDisabled();
      expect(screen.getByText('保存')).toBeDisabled();
    });
  });
});