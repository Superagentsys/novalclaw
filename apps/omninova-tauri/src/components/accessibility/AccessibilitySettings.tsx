/**
 * Accessibility Settings Component
 *
 * Settings panel for accessibility options.
 *
 * [Source: Story 10.7 - 无障碍访问]
 */

import * as React from 'react';
import { memo } from 'react';
import { cn } from '@/lib/utils';
import { useAccessibilityStore } from '@/stores/accessibilityStore';
import { ZOOM_LEVELS } from '@/types/accessibility';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog';
import {
  Accessibility,
  ZoomIn,
  ZoomOut,
  RotateCcw,
  Contrast,
  Type,
  Eye,
  Volume2,
} from 'lucide-react';

// ============================================================================
// Types
// ============================================================================

export interface AccessibilitySettingsProps {
  /** Additional CSS classes */
  className?: string;
  /** Whether dialog is open (controlled) */
  open?: boolean;
  /** Called when open state changes */
  onOpenChange?: (open: boolean) => void;
}

// ============================================================================
// Component
// ============================================================================

/**
 * Accessibility settings component
 *
 * Provides UI for:
 * - High contrast mode toggle
 * - Large text mode toggle
 * - Reduce motion toggle
 * - Screen reader mode toggle
 * - Zoom level adjustment
 */
export const AccessibilitySettings = memo(function AccessibilitySettings({
  className,
  open,
  onOpenChange,
}: AccessibilitySettingsProps): React.ReactElement {
  // Store state
  const highContrast = useAccessibilityStore((s) => s.highContrast);
  const largeText = useAccessibilityStore((s) => s.largeText);
  const reduceMotion = useAccessibilityStore((s) => s.reduceMotion);
  const zoomLevel = useAccessibilityStore((s) => s.zoomLevel);
  const screenReaderMode = useAccessibilityStore((s) => s.screenReaderMode);

  const toggleHighContrast = useAccessibilityStore((s) => s.toggleHighContrast);
  const toggleLargeText = useAccessibilityStore((s) => s.toggleLargeText);
  const toggleReduceMotion = useAccessibilityStore((s) => s.toggleReduceMotion);
  const toggleScreenReaderMode = useAccessibilityStore(
    (s) => s.toggleScreenReaderMode
  );
  const setZoomLevel = useAccessibilityStore((s) => s.setZoomLevel);
  const zoomIn = useAccessibilityStore((s) => s.zoomIn);
  const zoomOut = useAccessibilityStore((s) => s.zoomOut);
  const resetToDefaults = useAccessibilityStore((s) => s.resetToDefaults);

  const currentIndex = ZOOM_LEVELS.indexOf(zoomLevel as never);
  const canZoomIn = currentIndex < ZOOM_LEVELS.length - 1;
  const canZoomOut = currentIndex > 0;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          className={cn('gap-2', className)}
        >
          <Accessibility className="w-4 h-4" />
          无障碍
        </Button>
      </DialogTrigger>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>无障碍设置</DialogTitle>
        </DialogHeader>
        <div className="mt-4 space-y-6">
          {/* High Contrast */}
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <Contrast className="w-5 h-5 text-muted-foreground" />
              <Label htmlFor="high-contrast">高对比度模式</Label>
            </div>
            <Switch
              id="high-contrast"
              checked={highContrast}
              onCheckedChange={toggleHighContrast}
            />
          </div>

          {/* Large Text */}
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <Type className="w-5 h-5 text-muted-foreground" />
              <Label htmlFor="large-text">大字体模式</Label>
            </div>
            <Switch
              id="large-text"
              checked={largeText}
              onCheckedChange={toggleLargeText}
            />
          </div>

          {/* Reduce Motion */}
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <Eye className="w-5 h-5 text-muted-foreground" />
              <Label htmlFor="reduce-motion">减少动画</Label>
            </div>
            <Switch
              id="reduce-motion"
              checked={reduceMotion}
              onCheckedChange={toggleReduceMotion}
            />
          </div>

          {/* Screen Reader Mode */}
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <Volume2 className="w-5 h-5 text-muted-foreground" />
              <Label htmlFor="screen-reader">屏幕阅读器优化</Label>
            </div>
            <Switch
              id="screen-reader"
              checked={screenReaderMode}
              onCheckedChange={toggleScreenReaderMode}
            />
          </div>

          {/* Zoom Level */}
          <div className="space-y-2">
            <Label>界面缩放</Label>
            <div className="flex items-center gap-2">
              <Button
                variant="outline"
                size="icon"
                onClick={zoomOut}
                disabled={!canZoomOut}
              >
                <ZoomOut className="w-4 h-4" />
              </Button>
              <Select
                value={String(zoomLevel)}
                onValueChange={(v) => setZoomLevel(Number(v))}
              >
                <SelectTrigger className="w-24">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {ZOOM_LEVELS.map((level) => (
                    <SelectItem key={level} value={String(level)}>
                      {level}%
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <Button
                variant="outline"
                size="icon"
                onClick={zoomIn}
                disabled={!canZoomIn}
              >
                <ZoomIn className="w-4 h-4" />
              </Button>
            </div>
          </div>

          {/* Reset */}
          <div className="flex justify-end">
            <Button
              variant="outline"
              size="sm"
              onClick={resetToDefaults}
              className="gap-2"
            >
              <RotateCcw className="w-4 h-4" />
              重置为默认
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
});

export default AccessibilitySettings;