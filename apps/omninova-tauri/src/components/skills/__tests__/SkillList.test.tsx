/**
 * SkillList Component Tests
 *
 * [Source: Story 7.6 - 技能管理界面]
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { SkillList } from '../SkillList';
import type { SkillMetadata } from '@/types/skill';

// Mock skills
const mockSkills: SkillMetadata[] = [
  {
    id: 'skill-1',
    name: 'Web Search',
    version: '1.0.0',
    description: 'Search the web for information',
    tags: ['productivity'],
    dependencies: [],
    isBuiltin: true,
  },
  {
    id: 'skill-2',
    name: 'File Reader',
    version: '2.0.0',
    description: 'Read files from disk',
    tags: ['automation', 'productivity'],
    dependencies: [],
    isBuiltin: false,
  },
  {
    id: 'skill-3',
    name: 'Email Sender',
    version: '1.5.0',
    description: 'Send emails to recipients',
    author: 'Email Team',
    tags: ['integration'],
    dependencies: ['skill-1'],
    isBuiltin: false,
  },
];

describe('SkillList', () => {
  it('renders all skills', () => {
    render(<SkillList skills={mockSkills} />);

    expect(screen.getByText('Web Search')).toBeInTheDocument();
    expect(screen.getByText('File Reader')).toBeInTheDocument();
    expect(screen.getByText('Email Sender')).toBeInTheDocument();
  });

  it('shows loading spinner when isLoading is true', () => {
    render(<SkillList skills={[]} isLoading={true} />);

    // Look for the spinner with animate-spin class
    expect(document.querySelector('.animate-spin')).toBeInTheDocument();
  });

  it('shows empty state when no skills', () => {
    render(<SkillList skills={[]} />);

    expect(screen.getByText('暂无可用技能')).toBeInTheDocument();
  });

  it('filters skills by search query', () => {
    render(<SkillList skills={mockSkills} />);

    const searchInput = screen.getByPlaceholderText('搜索技能名称、描述、作者...');
    fireEvent.change(searchInput, { target: { value: 'Email' } });

    expect(screen.getByText('Email Sender')).toBeInTheDocument();
    expect(screen.queryByText('Web Search')).not.toBeInTheDocument();
    expect(screen.queryByText('File Reader')).not.toBeInTheDocument();
  });

  it('filters skills by description', () => {
    render(<SkillList skills={mockSkills} />);

    const searchInput = screen.getByPlaceholderText('搜索技能名称、描述、作者...');
    fireEvent.change(searchInput, { target: { value: 'disk' } });

    expect(screen.getByText('File Reader')).toBeInTheDocument();
    expect(screen.queryByText('Web Search')).not.toBeInTheDocument();
  });

  it('filters skills by author', () => {
    render(<SkillList skills={mockSkills} />);

    const searchInput = screen.getByPlaceholderText('搜索技能名称、描述、作者...');
    fireEvent.change(searchInput, { target: { value: 'Email Team' } });

    expect(screen.getByText('Email Sender')).toBeInTheDocument();
    expect(screen.queryByText('Web Search')).not.toBeInTheDocument();
  });

  it('shows no results message when search has no matches', () => {
    render(<SkillList skills={mockSkills} />);

    const searchInput = screen.getByPlaceholderText('搜索技能名称、描述、作者...');
    fireEvent.change(searchInput, { target: { value: 'nonexistent' } });

    expect(screen.getByText('未找到匹配的技能')).toBeInTheDocument();
  });

  it('filters skills by tag', () => {
    render(<SkillList skills={mockSkills} />);

    // Find the tag in the filter bar by looking for badge with cursor-pointer
    const allBadges = document.querySelectorAll('[class*="cursor-pointer"]');
    const productivityTagWithCount = Array.from(allBadges)
      .find(el => el.textContent?.includes('productivity')) as HTMLElement;

    fireEvent.click(productivityTagWithCount);

    // Should show skills with productivity tag
    expect(screen.getByText('Web Search')).toBeInTheDocument();
    expect(screen.getByText('File Reader')).toBeInTheDocument();
    expect(screen.queryByText('Email Sender')).not.toBeInTheDocument();
  });

  it('shows all skills when "全部" tag is selected', () => {
    render(<SkillList skills={mockSkills} />);

    // First select a tag using the filter bar
    const allBadges = document.querySelectorAll('[class*="cursor-pointer"]');
    const productivityTagWithCount = Array.from(allBadges)
      .find(el => el.textContent?.includes('productivity')) as HTMLElement;
    fireEvent.click(productivityTagWithCount);

    // Then select "全部"
    const allTag = Array.from(document.querySelectorAll('[class*="cursor-pointer"]'))
      .find(el => el.textContent?.includes('全部')) as HTMLElement;
    fireEvent.click(allTag);

    expect(screen.getByText('Web Search')).toBeInTheDocument();
    expect(screen.getByText('File Reader')).toBeInTheDocument();
    expect(screen.getByText('Email Sender')).toBeInTheDocument();
  });

  it('shows enabled skill with switch checked', () => {
    const enabledSkillIds = new Set(['skill-1']);
    render(<SkillList skills={mockSkills} enabledSkillIds={enabledSkillIds} />);

    const switches = screen.getAllByRole('switch');
    // First skill should be enabled
    expect(switches[0]).toBeChecked();
  });

  it('calls onSkillToggle when skill is toggled', () => {
    const onSkillToggle = vi.fn();
    render(<SkillList skills={mockSkills} onSkillToggle={onSkillToggle} />);

    const switches = screen.getAllByRole('switch');
    fireEvent.click(switches[0]);

    expect(onSkillToggle).toHaveBeenCalledWith('skill-1', true);
  });

  it('calls onConfigureSkill when configure is clicked', () => {
    const onConfigureSkill = vi.fn();
    const skillWithConfig: SkillMetadata[] = [{
      ...mockSkills[0],
      configSchema: { type: 'object', properties: { key: { type: 'string' } } },
    }];

    render(
      <SkillList
        skills={skillWithConfig}
        onConfigureSkill={onConfigureSkill}
        enabledSkillIds={new Set(['skill-1'])}
      />
    );

    const configureButton = screen.getByText('配置');
    fireEvent.click(configureButton);

    expect(onConfigureSkill).toHaveBeenCalledWith('skill-1');
  });

  it('displays tag counts correctly', () => {
    render(<SkillList skills={mockSkills} />);

    // productivity appears in 2 skills
    expect(screen.getByText(/productivity \(2\)/)).toBeInTheDocument();
    // automation appears in 1 skill
    expect(screen.getByText(/automation \(1\)/)).toBeInTheDocument();
    // integration appears in 1 skill
    expect(screen.getByText(/integration \(1\)/)).toBeInTheDocument();
  });

  it('shows results count when filtered', () => {
    render(<SkillList skills={mockSkills} />);

    const searchInput = screen.getByPlaceholderText('搜索技能名称、描述、作者...');
    fireEvent.change(searchInput, { target: { value: 'Email' } });

    expect(screen.getByText('显示 1 / 3 个技能')).toBeInTheDocument();
  });
});