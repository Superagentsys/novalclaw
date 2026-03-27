/**
 * Layout Presets Component
 *
 * UI for managing layout presets.
 *
 * [Source: Story 10.4 - 界面布局自定义]
 */

import * as React from 'react';
import { memo, useState, useCallback } from 'react';
import { cn } from '@/lib/utils';
import { useLayoutStore } from '@/stores/layoutStore';
import { PANEL_NAMES, LAYOUT_CONSTRAINTS } from '@/types/layout';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  Layout,
  Save,
  Trash2,
  RotateCcw,
  Plus,
  MoreHorizontal,
  Eye,
  EyeOff,
} from 'lucide-react';

// ============================================================================
// Types
// ============================================================================

export interface LayoutPresetsProps {
  /** Additional CSS classes */
  className?: string;
}

// ============================================================================
// Component
// ============================================================================

/**
 * Layout presets component
 *
 * Provides UI for:
 * - Toggling panel visibility
 * - Saving/loading layout presets
 * - Resetting to default layout
 */
export const LayoutPresets = memo(function LayoutPresets({
  className,
}: LayoutPresetsProps): React.ReactElement {
  // Store state
  const currentLayout = useLayoutStore((s) => s.currentLayout);
  const presets = useLayoutStore((s) => s.presets);
  const togglePanel = useLayoutStore((s) => s.togglePanel);
  const savePreset = useLayoutStore((s) => s.savePreset);
  const loadPreset = useLayoutStore((s) => s.loadPreset);
  const deletePreset = useLayoutStore((s) => s.deletePreset);
  const resetToDefault = useLayoutStore((s) => s.resetToDefault);

  // Local state
  const [isSaveDialogOpen, setIsSaveDialogOpen] = useState(false);
  const [newPresetName, setNewPresetName] = useState('');

  // Handle save preset
  const handleSavePreset = useCallback(() => {
    if (newPresetName.trim()) {
      savePreset(newPresetName.trim());
      setNewPresetName('');
      setIsSaveDialogOpen(false);
    }
  }, [newPresetName, savePreset]);

  return (
    <div className={cn('space-y-4', className)}>
      {/* Panel toggles */}
      <div className="space-y-2">
        <h4 className="text-sm font-medium text-muted-foreground">面板显示</h4>
        {(Object.keys(PANEL_NAMES) as Array<keyof typeof PANEL_NAMES>).map(
          (panel) => (
            <button
              key={panel}
              type="button"
              onClick={() => togglePanel(panel)}
              className={cn(
                'w-full flex items-center justify-between px-3 py-2 rounded-lg',
                'text-sm transition-colors',
                'hover:bg-accent'
              )}
            >
              <span>{PANEL_NAMES[panel]}</span>
              {currentLayout.visibility[panel] ? (
                <Eye className="w-4 h-4 text-primary" />
              ) : (
                <EyeOff className="w-4 h-4 text-muted-foreground" />
              )}
            </button>
          )
        )}
      </div>

      {/* Presets */}
      <div className="space-y-2">
        <h4 className="text-sm font-medium text-muted-foreground">布局预设</h4>

        <div className="space-y-1">
          {presets.map((preset) => (
            <div
              key={preset.id}
              className={cn(
                'flex items-center justify-between px-3 py-2 rounded-lg',
                'text-sm transition-colors',
                preset.id === currentLayout.id
                  ? 'bg-primary/10 text-primary'
                  : 'hover:bg-accent'
              )}
            >
              <button
                type="button"
                onClick={() => loadPreset(preset.id)}
                className="flex-1 text-left flex items-center gap-2"
              >
                <Layout className="w-4 h-4" />
                {preset.name}
              </button>

              {preset.id !== 'default' && (
                <DropdownMenu>
                  <DropdownMenuTrigger asChild>
                    <Button variant="ghost" size="icon" className="h-6 w-6">
                      <MoreHorizontal className="w-4 h-4" />
                    </Button>
                  </DropdownMenuTrigger>
                  <DropdownMenuContent align="end">
                    <DropdownMenuItem onClick={() => deletePreset(preset.id)}>
                      <Trash2 className="w-4 h-4 mr-2" />
                      删除预设
                    </DropdownMenuItem>
                  </DropdownMenuContent>
                </DropdownMenu>
              )}
            </div>
          ))}
        </div>

        {/* Save preset button */}
        <Dialog open={isSaveDialogOpen} onOpenChange={setIsSaveDialogOpen}>
          <DialogTrigger asChild>
            <Button variant="outline" size="sm" className="w-full">
              <Save className="w-4 h-4 mr-2" />
              保存当前布局
            </Button>
          </DialogTrigger>
          <DialogContent className="sm:max-w-md">
            <DialogHeader>
              <DialogTitle>保存布局预设</DialogTitle>
            </DialogHeader>
            <div className="flex gap-2 mt-4">
              <Input
                value={newPresetName}
                onChange={(e) => setNewPresetName(e.target.value)}
                placeholder="输入预设名称..."
                onKeyDown={(e) => {
                  if (e.key === 'Enter') {
                    handleSavePreset();
                  }
                }}
              />
              <Button onClick={handleSavePreset} disabled={!newPresetName.trim()}>
                保存
              </Button>
            </div>
          </DialogContent>
        </Dialog>

        {/* Reset button */}
        <Button
          variant="ghost"
          size="sm"
          className="w-full text-muted-foreground"
          onClick={resetToDefault}
        >
          <RotateCcw className="w-4 h-4 mr-2" />
          重置为默认布局
        </Button>
      </div>
    </div>
  );
});

export default LayoutPresets;