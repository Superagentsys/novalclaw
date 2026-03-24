/**
 * ConfigExportDialog Component
 *
 * Dialog for exporting agent configurations.
 * [Source: Story 7.8 - 配置导入导出功能]
 */

import { useState, useEffect } from 'react';
import { toast } from 'sonner';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import { Checkbox } from '@/components/ui/checkbox';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Download, Info, Loader2 } from 'lucide-react';
import {
  type ExportFormat,
  type ExportOptions,
  DEFAULT_EXPORT_OPTIONS,
  EXPORT_FORMAT_LABELS,
} from '@/types/config-import-export';
import type { AgentModel } from '@/types/agent';
import { useConfigImportExport } from '@/hooks/useConfigImportExport';

export interface ConfigExportDialogProps {
  /** Dialog open state */
  open: boolean;
  /** Open state change callback */
  onOpenChange: (open: boolean) => void;
  /** Agents available for export */
  agents: AgentModel[];
  /** Pre-selected agent ID for single export */
  agentId?: string;
}

export function ConfigExportDialog({
  open,
  onOpenChange,
  agents,
  agentId,
}: ConfigExportDialogProps) {
  const { exportState, exportAgent, exportAllAgents, resetExportState } =
    useConfigImportExport();

  const [exportScope, setExportScope] = useState<'single' | 'all'>(
    agentId ? 'single' : 'all'
  );
  const [selectedAgentId, setSelectedAgentId] = useState<string | undefined>(agentId);
  const [options, setOptions] = useState<ExportOptions>(DEFAULT_EXPORT_OPTIONS);

  // Reset state when dialog opens
  useEffect(() => {
    if (open) {
      resetExportState();
      setExportScope(agentId ? 'single' : 'all');
      setSelectedAgentId(agentId);
      setOptions(DEFAULT_EXPORT_OPTIONS);
    }
  }, [open, agentId, resetExportState]);

  const handleExport = async () => {
    let success = false;

    if (exportScope === 'single' && selectedAgentId) {
      success = await exportAgent(selectedAgentId, options);
    } else {
      success = await exportAllAgents(agents, options);
    }

    if (success) {
      toast.success('导出成功', {
        description: `配置已导出为 ${options.format.toUpperCase()} 格式`,
      });
      onOpenChange(false);
    }
  };

  const handleFormatChange = (format: string) => {
    setOptions(prev => ({ ...prev, format: format as ExportFormat }));
  };

  const selectedAgent = agents.find(a => a.agent_uuid === selectedAgentId);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[500px]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Download className="h-5 w-5" />
            导出配置
          </DialogTitle>
          <DialogDescription>
            导出代理配置以备份或迁移到其他设备
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-6 py-4">
          {/* Export Scope */}
          <div className="space-y-3">
            <Label className="text-base">导出范围</Label>
            <RadioGroup
              value={exportScope}
              onValueChange={(value) => setExportScope(value as 'single' | 'all')}
              disabled={exportState.isExporting}
            >
              {agentId && selectedAgent && (
                <div className="flex items-center space-x-2">
                  <RadioGroupItem value="single" id="single" />
                  <Label htmlFor="single" className="font-normal">
                    当前代理 ({selectedAgent.name})
                  </Label>
                </div>
              )}
              <div className="flex items-center space-x-2">
                <RadioGroupItem value="all" id="all" />
                <Label htmlFor="all" className="font-normal">
                  所有代理 ({agents.length}个)
                </Label>
              </div>
            </RadioGroup>
          </div>

          {/* Export Format */}
          <div className="space-y-3">
            <Label className="text-base">导出格式</Label>
            <RadioGroup
              value={options.format}
              onValueChange={handleFormatChange}
              disabled={exportState.isExporting}
              className="flex gap-4"
            >
              {(Object.keys(EXPORT_FORMAT_LABELS) as ExportFormat[]).map(format => (
                <div key={format} className="flex items-center space-x-2">
                  <RadioGroupItem value={format} id={`format-${format}`} />
                  <Label htmlFor={`format-${format}`} className="font-normal">
                    {EXPORT_FORMAT_LABELS[format]}
                  </Label>
                </div>
              ))}
            </RadioGroup>
          </div>

          {/* Export Options */}
          <div className="space-y-3">
            <Label className="text-base">包含内容</Label>
            <div className="space-y-2">
              <div className="flex items-center space-x-2">
                <Checkbox
                  id="include-skills"
                  checked={options.includeSkills}
                  onCheckedChange={(checked) =>
                    setOptions(prev => ({ ...prev, includeSkills: !!checked }))
                  }
                  disabled={exportState.isExporting}
                />
                <Label htmlFor="include-skills" className="font-normal">
                  技能配置
                </Label>
              </div>
              <div className="flex items-center space-x-2">
                <Checkbox
                  id="include-history"
                  checked={options.includeHistory}
                  onCheckedChange={(checked) =>
                    setOptions(prev => ({ ...prev, includeHistory: !!checked }))
                  }
                  disabled={exportState.isExporting}
                />
                <Label htmlFor="include-history" className="font-normal">
                  对话历史
                </Label>
              </div>
              <div className="flex items-center space-x-2">
                <Checkbox
                  id="include-memory"
                  checked={options.includeMemory}
                  onCheckedChange={(checked) =>
                    setOptions(prev => ({ ...prev, includeMemory: !!checked }))
                  }
                  disabled={exportState.isExporting}
                />
                <Label htmlFor="include-memory" className="font-normal">
                  记忆数据
                </Label>
              </div>
            </div>
          </div>

          {/* Security Notice */}
          <Alert>
            <Info className="h-4 w-4" />
            <AlertDescription>
              敏感信息（API密钥等）不会被导出，请确保在目标设备上重新配置。
            </AlertDescription>
          </Alert>

          {/* Error Display */}
          {exportState.error && (
            <Alert variant="destructive">
              <AlertDescription>{exportState.error}</AlertDescription>
            </Alert>
          )}
        </div>

        <DialogFooter>
          <Button
            variant="outline"
            onClick={() => onOpenChange(false)}
            disabled={exportState.isExporting}
          >
            取消
          </Button>
          <Button
            onClick={handleExport}
            disabled={exportState.isExporting || (exportScope === 'all' && agents.length === 0)}
          >
            {exportState.isExporting ? (
              <>
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                导出中...
              </>
            ) : (
              <>
                <Download className="mr-2 h-4 w-4" />
                导出
              </>
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}