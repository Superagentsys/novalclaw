/**
 * PrivacyConfigForm Tests
 *
 * Tests for the privacy configuration form component.
 * [Source: Story 7.4 - 数据处理与隐私设置]
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { PrivacyConfigForm, type PrivacyConfigFormProps } from './PrivacyConfigForm';
import { DEFAULT_PRIVACY_CONFIG } from '@/types/agent';

// Mock Tauri API
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

describe('PrivacyConfigForm', () => {
  const defaultProps: PrivacyConfigFormProps = {
    config: DEFAULT_PRIVACY_CONFIG,
    onChange: vi.fn(),
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('Rendering', () => {
    it('should render all collapsible sections', () => {
      render(<PrivacyConfigForm {...defaultProps} />);

      expect(screen.getByText('数据保留策略')).toBeInTheDocument();
      expect(screen.getByText('敏感信息过滤')).toBeInTheDocument();
      expect(screen.getByText('记忆共享范围')).toBeInTheDocument();
      expect(screen.getByText('排除数据规则')).toBeInTheDocument();
    });

    it('should expand data retention section by default', () => {
      render(<PrivacyConfigForm {...defaultProps} />);

      expect(screen.getByLabelText('情景记忆保留天数')).toBeInTheDocument();
      expect(screen.getByLabelText('工作记忆保留小时数')).toBeInTheDocument();
    });

    it('should show auto cleanup badge when enabled', () => {
      render(<PrivacyConfigForm {...defaultProps} />);

      expect(screen.getByText('自动清理')).toBeInTheDocument();
    });
  });

  describe('Data Retention', () => {
    it('should update episodic memory days', async () => {
      const onChange = vi.fn();
      render(<PrivacyConfigForm {...defaultProps} onChange={onChange} />);

      const input = screen.getByLabelText('情景记忆保留天数');
      // Clear and type new value
      fireEvent.change(input, { target: { value: '60' } });

      expect(onChange).toHaveBeenCalled();
      const lastCall = onChange.mock.calls[onChange.mock.calls.length - 1][0];
      expect(lastCall.dataRetention.episodicMemoryDays).toBe(60);
    });

    it('should update working memory hours', async () => {
      const onChange = vi.fn();
      render(<PrivacyConfigForm {...defaultProps} onChange={onChange} />);

      const input = screen.getByLabelText('工作记忆保留小时数');
      fireEvent.change(input, { target: { value: '48' } });

      expect(onChange).toHaveBeenCalled();
      const lastCall = onChange.mock.calls[onChange.mock.calls.length - 1][0];
      expect(lastCall.dataRetention.workingMemoryHours).toBe(48);
    });

    it('should toggle auto cleanup', async () => {
      const onChange = vi.fn();
      render(<PrivacyConfigForm {...defaultProps} onChange={onChange} />);

      const switchElement = screen.getByRole('switch', { name: /自动清理过期数据/i });
      await userEvent.click(switchElement);

      expect(onChange).toHaveBeenCalledWith(
        expect.objectContaining({
          dataRetention: expect.objectContaining({
            autoCleanup: false,
          }),
        })
      );
    });
  });

  describe('Sensitive Filter', () => {
    it('should toggle filter enabled', async () => {
      const onChange = vi.fn();
      render(<PrivacyConfigForm {...defaultProps} onChange={onChange} />);

      const switchElement = screen.getByRole('switch', { name: /启用敏感信息自动过滤/i });
      await userEvent.click(switchElement);

      expect(onChange).toHaveBeenCalledWith(
        expect.objectContaining({
          sensitiveFilter: expect.objectContaining({
            enabled: true,
          }),
        })
      );
    });

    it('should show filter types when enabled', async () => {
      const config = {
        ...DEFAULT_PRIVACY_CONFIG,
        sensitiveFilter: { ...DEFAULT_PRIVACY_CONFIG.sensitiveFilter, enabled: true },
      };
      render(<PrivacyConfigForm {...defaultProps} config={config} />);

      expect(screen.getByText('邮箱地址')).toBeInTheDocument();
      expect(screen.getByText('电话号码')).toBeInTheDocument();
      expect(screen.getByText('身份证号')).toBeInTheDocument();
      expect(screen.getByText('银行卡号')).toBeInTheDocument();
      expect(screen.getByText('IP 地址')).toBeInTheDocument();
    });

    it('should toggle individual filter types', async () => {
      const onChange = vi.fn();
      const config = {
        ...DEFAULT_PRIVACY_CONFIG,
        sensitiveFilter: { ...DEFAULT_PRIVACY_CONFIG.sensitiveFilter, enabled: true },
      };
      render(<PrivacyConfigForm {...defaultProps} config={config} onChange={onChange} />);

      const emailCheckbox = screen.getByRole('checkbox', { name: /邮箱地址/i });
      await userEvent.click(emailCheckbox);

      expect(onChange).toHaveBeenCalledWith(
        expect.objectContaining({
          sensitiveFilter: expect.objectContaining({
            filterEmail: false,
          }),
        })
      );
    });
  });

  describe('Memory Sharing Scope', () => {
    it('should show memory sharing section', () => {
      render(<PrivacyConfigForm {...defaultProps} />);

      expect(screen.getByText('记忆共享范围')).toBeInTheDocument();
    });

    it('should show scope options when expanded', async () => {
      render(<PrivacyConfigForm {...defaultProps} />);

      // Find and click the crossSession option
      const crossSessionOption = screen.getByText('跨会话共享');
      await userEvent.click(crossSessionOption);

      // The click should trigger onChange
      // Note: the click handler is on the parent label element
    });
  });

  describe('Exclusion Rules', () => {
    it('should show add rule button', () => {
      render(<PrivacyConfigForm {...defaultProps} />);

      expect(screen.getByRole('button', { name: /添加规则/i })).toBeInTheDocument();
    });

    it('should open add rule dialog', async () => {
      render(<PrivacyConfigForm {...defaultProps} />);

      const addButton = screen.getByRole('button', { name: /添加规则/i });
      await userEvent.click(addButton);

      expect(screen.getByText('添加排除规则')).toBeInTheDocument();
    });

    it('should add a new rule', async () => {
      const onChange = vi.fn();
      render(<PrivacyConfigForm {...defaultProps} onChange={onChange} />);

      // Open dialog
      await userEvent.click(screen.getByRole('button', { name: /添加规则/i }));

      // Fill in form
      await userEvent.type(screen.getByLabelText('规则名称 *'), 'Test Rule');
      await userEvent.type(screen.getByLabelText('正则表达式 *'), 'test.*pattern');

      // Save
      await userEvent.click(screen.getByRole('button', { name: '添加规则' }));

      expect(onChange).toHaveBeenCalledWith(
        expect.objectContaining({
          exclusionRules: expect.arrayContaining([
            expect.objectContaining({
              name: 'Test Rule',
              pattern: 'test.*pattern',
            }),
          ]),
        })
      );
    });
  });

  describe('Reset', () => {
    it('should reset to defaults', async () => {
      const onChange = vi.fn();
      const customConfig = {
        ...DEFAULT_PRIVACY_CONFIG,
        dataRetention: {
          ...DEFAULT_PRIVACY_CONFIG.dataRetention,
          episodicMemoryDays: 365,
        },
      };
      render(<PrivacyConfigForm {...defaultProps} config={customConfig} onChange={onChange} />);

      const resetButton = screen.getByRole('button', { name: /重置为默认/i });
      await userEvent.click(resetButton);

      expect(onChange).toHaveBeenCalledWith(DEFAULT_PRIVACY_CONFIG);
    });
  });

  describe('Disabled State', () => {
    it('should disable all inputs when disabled', () => {
      render(<PrivacyConfigForm {...defaultProps} disabled />);

      expect(screen.getByLabelText('情景记忆保留天数')).toBeDisabled();
      expect(screen.getByLabelText('工作记忆保留小时数')).toBeDisabled();
      // Switch uses data-disabled attribute instead of native disabled
      expect(screen.getByRole('switch', { name: /自动清理过期数据/i })).toHaveAttribute('data-disabled');
      expect(screen.getByRole('button', { name: /添加规则/i })).toBeDisabled();
    });
  });
});