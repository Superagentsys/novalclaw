/**
 * SkillCard Component Tests
 *
 * [Source: Story 7.6 - 技能管理界面]
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { SkillCard } from '../SkillCard';
import type { SkillMetadata, SkillUsageStatistics } from '@/types/skill';

// Mock skill metadata
const mockSkill: SkillMetadata = {
  id: 'test-skill',
  name: 'Test Skill',
  version: '1.0.0',
  description: 'A test skill for testing',
  author: 'Test Author',
  tags: ['productivity', 'automation'],
  dependencies: [],
  isBuiltin: false,
};

const mockSkillWithConfig: SkillMetadata = {
  ...mockSkill,
  id: 'configurable-skill',
  name: 'Configurable Skill',
  configSchema: {
    type: 'object',
    properties: {
      apiKey: { type: 'string', title: 'API Key' },
      maxResults: { type: 'integer', title: 'Max Results', default: 10 },
    },
    required: ['apiKey'],
  },
};

const mockUsageStats: SkillUsageStatistics = {
  skillId: 'test-skill',
  totalExecutions: 100,
  successCount: 90,
  failureCount: 10,
  avgDurationMs: 250,
  lastExecutedAt: '2026-03-23T10:00:00Z',
};

describe('SkillCard', () => {
  it('renders skill basic information', () => {
    render(<SkillCard skill={mockSkill} />);

    expect(screen.getByText('Test Skill')).toBeInTheDocument();
    expect(screen.getByText('A test skill for testing')).toBeInTheDocument();
    expect(screen.getByText('v1.0.0')).toBeInTheDocument();
    expect(screen.getByText('Test Author')).toBeInTheDocument();
  });

  it('renders skill tags', () => {
    render(<SkillCard skill={mockSkill} />);

    expect(screen.getByText('productivity')).toBeInTheDocument();
    expect(screen.getByText('automation')).toBeInTheDocument();
  });

  it('shows builtin badge for builtin skills', () => {
    const builtinSkill = { ...mockSkill, isBuiltin: true };
    render(<SkillCard skill={builtinSkill} />);

    expect(screen.getByText('内置')).toBeInTheDocument();
  });

  it('renders enabled switch when onToggle is provided', () => {
    const onToggle = vi.fn();
    render(<SkillCard skill={mockSkill} onToggle={onToggle} />);

    // Switch should be present
    const switchElement = screen.getByRole('switch');
    expect(switchElement).toBeInTheDocument();
  });

  it('calls onToggle when switch is clicked', async () => {
    const onToggle = vi.fn();
    render(<SkillCard skill={mockSkill} onToggle={onToggle} enabled={false} />);

    const switchElement = screen.getByRole('switch');
    // Click the switch to toggle
    fireEvent.click(switchElement);

    // The switch should call onToggle with true (toggling from false to true)
    expect(onToggle).toHaveBeenCalled();
  });

  it('shows configure button when skill has config schema and onConfigure is provided', () => {
    const onConfigure = vi.fn();
    render(
      <SkillCard
        skill={mockSkillWithConfig}
        onConfigure={onConfigure}
        enabled={true}
      />
    );

    expect(screen.getByText('配置')).toBeInTheDocument();
  });

  it('disables configure button when skill is not enabled', () => {
    const onConfigure = vi.fn();
    render(
      <SkillCard
        skill={mockSkillWithConfig}
        onConfigure={onConfigure}
        enabled={false}
      />
    );

    const configureButton = screen.getByText('配置').closest('button');
    expect(configureButton).toBeDisabled();
  });

  it('calls onConfigure when configure button is clicked', () => {
    const onConfigure = vi.fn();
    render(
      <SkillCard
        skill={mockSkillWithConfig}
        onConfigure={onConfigure}
        enabled={true}
      />
    );

    const configureButton = screen.getByText('配置');
    fireEvent.click(configureButton);

    expect(onConfigure).toHaveBeenCalled();
  });

  it('shows usage statistics when showStats is true', () => {
    render(
      <SkillCard
        skill={mockSkill}
        showStats={true}
        usageStats={mockUsageStats}
      />
    );

    expect(screen.getByText('执行次数')).toBeInTheDocument();
    expect(screen.getByText('100')).toBeInTheDocument();
    expect(screen.getByText('成功率')).toBeInTheDocument();
    expect(screen.getByText('90%')).toBeInTheDocument();
  });

  it('does not show statistics when showStats is false', () => {
    render(
      <SkillCard
        skill={mockSkill}
        showStats={false}
        usageStats={mockUsageStats}
      />
    );

    expect(screen.queryByText('执行次数')).not.toBeInTheDocument();
  });

  it('renders homepage link when homepage is provided', () => {
    const skillWithHomepage = { ...mockSkill, homepage: 'https://example.com' };
    render(<SkillCard skill={skillWithHomepage} />);

    const docLink = screen.getByText('文档');
    expect(docLink).toBeInTheDocument();
    expect(docLink.closest('a')).toHaveAttribute('href', 'https://example.com');
  });

  it('shows dependencies count when skill has dependencies', () => {
    const skillWithDeps = { ...mockSkill, dependencies: ['dep1', 'dep2'] };
    render(<SkillCard skill={skillWithDeps} />);

    expect(screen.getByText('依赖: 2 个技能')).toBeInTheDocument();
  });
});