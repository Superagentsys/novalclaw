/**
 * SkillList 组件
 *
 * 显示技能列表，支持标签筛选和搜索功能
 *
 * [Source: Story 7.6 - 技能管理界面]
 */

import { type FC, useState, useMemo } from 'react';
import { Search, Filter, Loader2 } from 'lucide-react';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import { SkillCard } from './SkillCard';
import {
  type SkillMetadata,
  type SkillUsageStatistics,
  type SkillTag,
} from '@/types/skill';

export interface SkillListProps {
  /** List of skills to display */
  skills: SkillMetadata[];
  /** Enabled skill IDs */
  enabledSkillIds?: Set<string>;
  /** Usage statistics map */
  usageStatsMap?: Map<string, SkillUsageStatistics>;
  /** Available tags for filtering */
  availableTags?: SkillTag[];
  /** Callback when skill is toggled */
  onSkillToggle?: (skillId: string, enabled: boolean) => void;
  /** Callback when configure is clicked */
  onConfigureSkill?: (skillId: string) => void;
  /** Loading state */
  isLoading?: boolean;
  /** Show usage statistics */
  showStats?: boolean;
}

/**
 * SkillList component
 */
export const SkillList: FC<SkillListProps> = ({
  skills,
  enabledSkillIds = new Set(),
  usageStatsMap = new Map(),
  availableTags = [],
  onSkillToggle,
  onConfigureSkill,
  isLoading = false,
  showStats = false,
}) => {
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedTag, setSelectedTag] = useState<string | null>(null);

  // Filter skills by search query and tag
  const filteredSkills = useMemo(() => {
    return skills.filter(skill => {
      // Search filter
      const searchLower = searchQuery.toLowerCase();
      const matchesSearch = searchQuery === '' ||
        skill.name.toLowerCase().includes(searchLower) ||
        skill.description.toLowerCase().includes(searchLower) ||
        skill.id.toLowerCase().includes(searchLower) ||
        (skill.author?.toLowerCase().includes(searchLower) ?? false);

      // Tag filter
      const matchesTag = !selectedTag || skill.tags.includes(selectedTag);

      return matchesSearch && matchesTag;
    });
  }, [skills, searchQuery, selectedTag]);

  // Count skills by tag
  const tagCounts = useMemo(() => {
    const counts: Record<string, number> = {};
    skills.forEach(skill => {
      skill.tags.forEach(tag => {
        counts[tag] = (counts[tag] || 0) + 1;
      });
    });
    return counts;
  }, [skills]);

  // Get all unique tags from skills
  const allTags = useMemo(() => {
    const tags = new Set<string>();
    skills.forEach(skill => {
      skill.tags.forEach(tag => tags.add(tag));
    });
    return Array.from(tags).sort();
  }, [skills]);

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-12">
        <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {/* Search and Filter */}
      <div className="flex flex-col sm:flex-row gap-3">
        {/* Search input */}
        <div className="relative flex-1">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
          <Input
            type="text"
            placeholder="搜索技能名称、描述、作者..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="pl-9"
          />
        </div>
      </div>

      {/* Tag filters */}
      <div className="flex items-center gap-2 flex-wrap">
        <Filter className="h-4 w-4 text-muted-foreground" />
        <Badge
          variant={selectedTag === null ? 'default' : 'outline'}
          className="cursor-pointer"
          onClick={() => setSelectedTag(null)}
        >
          全部 ({skills.length})
        </Badge>
        {allTags.map(tag => (
          <Badge
            key={tag}
            variant={selectedTag === tag ? 'default' : 'outline'}
            className="cursor-pointer"
            onClick={() => setSelectedTag(tag)}
          >
            {tag} ({tagCounts[tag] || 0})
          </Badge>
        ))}
      </div>

      {/* Skill grid */}
      {filteredSkills.length === 0 ? (
        <div className="text-center py-12 text-muted-foreground">
          {searchQuery || selectedTag ? (
            <p>未找到匹配的技能</p>
          ) : (
            <p>暂无可用技能</p>
          )}
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {filteredSkills.map(skill => (
            <SkillCard
              key={skill.id}
              skill={skill}
              enabled={enabledSkillIds.has(skill.id)}
              onToggle={(enabled) => onSkillToggle?.(skill.id, enabled)}
              onConfigure={() => onConfigureSkill?.(skill.id)}
              showStats={showStats}
              usageStats={usageStatsMap.get(skill.id)}
              isLoading={isLoading}
            />
          ))}
        </div>
      )}

      {/* Results count */}
      {(searchQuery || selectedTag) && filteredSkills.length > 0 && (
        <div className="text-sm text-muted-foreground text-center">
          显示 {filteredSkills.length} / {skills.length} 个技能
        </div>
      )}
    </div>
  );
};

export default SkillList;