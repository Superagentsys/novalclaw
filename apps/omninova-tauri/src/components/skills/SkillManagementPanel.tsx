/**
 * SkillManagementPanel 组件
 *
 * 技能管理主面板，整合技能列表、筛选、配置等功能
 *
 * [Source: Story 7.6 - 技能管理界面]
 */

import { type FC, useState, useEffect, useCallback, useMemo } from 'react';
import { Loader2, Settings2, BarChart3 } from 'lucide-react';
import { toast } from 'sonner';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { SkillList } from './SkillList';
import { SkillConfigDialog } from './SkillConfigDialog';
import { SkillUsageStats } from './SkillUsageStats';
import { useSkills, useAgentSkillConfig, useSkillUsageStats } from '@/hooks/useSkills';
import {
  type SkillMetadata,
  type AgentSkillConfig,
  type SkillUsageStatistics,
} from '@/types/skill';

export interface SkillManagementPanelProps {
  /** Agent ID to manage skills for */
  agentId?: string;
  /** Initial skill configuration */
  initialConfig?: AgentSkillConfig;
  /** Callback when skill configuration changes */
  onConfigChange?: (config: AgentSkillConfig) => void;
}

/**
 * SkillManagementPanel component
 */
export const SkillManagementPanel: FC<SkillManagementPanelProps> = ({
  agentId,
  initialConfig,
  onConfigChange,
}) => {
  const {
    skills,
    tags,
    isLoading,
    error,
    refresh,
    validateConfig,
  } = useSkills();

  // Usage statistics
  const { stats: usageStatsMap, refresh: refreshStats } = useSkillUsageStats();

  // State for enabled skills
  const [enabledSkillIds, setEnabledSkillIds] = useState<Set<string>>(() => {
    return new Set(initialConfig?.enabledSkills || []);
  });

  // State for skill configurations
  const [skillConfigs, setSkillConfigs] = useState<Record<string, Record<string, unknown>>>(
    initialConfig?.skillConfigs || {}
  );

  // State for config dialog
  const [configDialogOpen, setConfigDialogOpen] = useState(false);
  const [selectedSkillId, setSelectedSkillId] = useState<string | null>(null);

  // Selected skill for config dialog
  const selectedSkill = useMemo(() => {
    return skills.find(s => s.id === selectedSkillId) || null;
  }, [skills, selectedSkillId]);

  // Sync enabled skills with initial config
  useEffect(() => {
    if (initialConfig?.enabledSkills) {
      setEnabledSkillIds(new Set(initialConfig.enabledSkills));
    }
    if (initialConfig?.skillConfigs) {
      setSkillConfigs(initialConfig.skillConfigs);
    }
  }, [initialConfig]);

  // Handle skill toggle
  const handleSkillToggle = useCallback((skillId: string, enabled: boolean) => {
    setEnabledSkillIds(prev => {
      const next = new Set(prev);
      if (enabled) {
        next.add(skillId);
      } else {
        next.delete(skillId);
      }
      return next;
    });

    // Notify parent of config change
    if (onConfigChange && agentId) {
      const newConfig: AgentSkillConfig = {
        agentId,
        enabledSkills: enabled
          ? [...Array.from(enabledSkillIds), skillId]
          : Array.from(enabledSkillIds).filter(id => id !== skillId),
        skillConfigs,
      };
      onConfigChange(newConfig);
    }
  }, [agentId, enabledSkillIds, skillConfigs, onConfigChange]);

  // Handle configure skill
  const handleConfigureSkill = useCallback((skillId: string) => {
    setSelectedSkillId(skillId);
    setConfigDialogOpen(true);
  }, []);

  // Handle save config
  const handleSaveConfig = useCallback(async (config: Record<string, unknown>) => {
    if (!selectedSkillId) return;

    // Validate config
    const isValid = await validateConfig(selectedSkillId, config);
    if (!isValid) {
      toast.error('配置验证失败');
      return;
    }

    // Save config
    setSkillConfigs(prev => ({
      ...prev,
      [selectedSkillId]: config,
    }));

    // Notify parent
    if (onConfigChange && agentId) {
      const newConfig: AgentSkillConfig = {
        agentId,
        enabledSkills: Array.from(enabledSkillIds),
        skillConfigs: {
          ...skillConfigs,
          [selectedSkillId]: config,
        },
      };
      onConfigChange(newConfig);
    }

    toast.success('配置已保存');
    setConfigDialogOpen(false);
  }, [selectedSkillId, validateConfig, agentId, enabledSkillIds, skillConfigs, onConfigChange]);

  // Get current config for selected skill
  const currentConfig = useMemo(() => {
    if (!selectedSkillId) return {};
    return skillConfigs[selectedSkillId] || {};
  }, [skillConfigs, selectedSkillId]);

  // Error handling
  useEffect(() => {
    if (error) {
      toast.error(error);
    }
  }, [error]);

  return (
    <div className="space-y-6">
      <Tabs defaultValue="skills" className="w-full">
        <TabsList>
          <TabsTrigger value="skills" className="gap-1.5">
            <Settings2 className="h-4 w-4" />
            技能管理
          </TabsTrigger>
          <TabsTrigger value="stats" className="gap-1.5">
            <BarChart3 className="h-4 w-4" />
            使用统计
          </TabsTrigger>
        </TabsList>

        <TabsContent value="skills" className="mt-4">
          {isLoading && skills.length === 0 ? (
            <div className="flex items-center justify-center py-12">
              <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
            </div>
          ) : (
            <SkillList
              skills={skills}
              enabledSkillIds={enabledSkillIds}
              usageStatsMap={usageStatsMap}
              availableTags={tags}
              onSkillToggle={handleSkillToggle}
              onConfigureSkill={handleConfigureSkill}
              isLoading={isLoading}
              showStats={false}
            />
          )}

          {/* Summary */}
          <div className="mt-4 pt-4 border-t text-sm text-muted-foreground">
            已启用 {enabledSkillIds.size} / {skills.length} 个技能
          </div>
        </TabsContent>

        <TabsContent value="stats" className="mt-4">
          <SkillUsageStats
            skills={skills}
            enabledSkillIds={enabledSkillIds}
            usageStatsMap={usageStatsMap}
          />
        </TabsContent>
      </Tabs>

      {/* Config Dialog */}
      {selectedSkill && (
        <SkillConfigDialog
          skill={selectedSkill}
          config={currentConfig}
          open={configDialogOpen}
          onOpenChange={setConfigDialogOpen}
          onSave={handleSaveConfig}
        />
      )}
    </div>
  );
};

export default SkillManagementPanel;