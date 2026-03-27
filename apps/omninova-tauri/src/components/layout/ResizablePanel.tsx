/**
 * Resizable Panel Component
 *
 * A panel that can be resized by dragging its edge.
 *
 * [Source: Story 10.4 - 界面布局自定义]
 */

import * as React from 'react';
import { useState, useCallback, useRef, useEffect, memo } from 'react';
import { cn } from '@/lib/utils';
import { GripVertical } from 'lucide-react';

// ============================================================================
// Types
// ============================================================================

export interface ResizablePanelProps {
  /** Panel content */
  children: React.ReactNode;
  /** Current width in pixels */
  width: number;
  /** Minimum width */
  minWidth: number;
  /** Maximum width */
  maxWidth: number;
  /** Resize callback */
  onResize: (width: number) => void;
  /** Which side the resize handle is on */
  side?: 'left' | 'right';
  /** Whether resizing is enabled */
  resizable?: boolean;
  /** Additional CSS classes */
  className?: string;
}

// ============================================================================
// Component
// ============================================================================

/**
 * Resizable panel component
 *
 * Provides a panel with a draggable resize handle on the specified side.
 *
 * @example
 * ```tsx
 * <ResizablePanel
 *   width={256}
 *   minWidth={180}
 *   maxWidth={400}
 *   onResize={(w) => setSize('sidebarWidth', w)}
 *   side="left"
 * >
 *   <Sidebar />
 * </ResizablePanel>
 * ```
 */
export const ResizablePanel = memo(function ResizablePanel({
  children,
  width,
  minWidth,
  maxWidth,
  onResize,
  side = 'left',
  resizable = true,
  className,
}: ResizablePanelProps): React.ReactElement {
  const [isResizing, setIsResizing] = useState(false);
  const panelRef = useRef<HTMLDivElement>(null);
  const startXRef = useRef(0);
  const startWidthRef = useRef(0);

  // Handle mouse down on resize handle
  const handleMouseDown = useCallback(
    (e: React.MouseEvent) => {
      if (!resizable) return;

      e.preventDefault();
      e.stopPropagation();

      setIsResizing(true);
      startXRef.current = e.clientX;
      startWidthRef.current = width;

      // Add cursor style to body
      document.body.style.cursor = 'col-resize';
      document.body.style.userSelect = 'none';
    },
    [resizable, width]
  );

  // Handle mouse move and up
  useEffect(() => {
    if (!isResizing) return;

    const handleMouseMove = (e: MouseEvent) => {
      const deltaX = e.clientX - startXRef.current;
      const delta = side === 'left' ? deltaX : -deltaX;
      const newWidth = startWidthRef.current + delta;

      // Clamp to constraints
      const clampedWidth = Math.max(minWidth, Math.min(maxWidth, newWidth));

      onResize(clampedWidth);
    };

    const handleMouseUp = () => {
      setIsResizing(false);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };

    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);

    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };
  }, [isResizing, minWidth, maxWidth, onResize, side]);

  return (
    <div
      ref={panelRef}
      className={cn('relative flex flex-col', className)}
      style={{ width: `${width}px`, minWidth: `${minWidth}px`, maxWidth: `${maxWidth}px` }}
    >
      {children}

      {/* Resize handle */}
      {resizable && (
        <div
          className={cn(
            'absolute top-0 bottom-0 w-1 group',
            'transition-colors duration-150',
            isResizing ? 'bg-primary/30' : 'hover:bg-primary/20',
            side === 'left' ? 'right-0' : 'left-0',
            'cursor-col-resize'
          )}
          onMouseDown={handleMouseDown}
          role="separator"
          aria-orientation="vertical"
          tabIndex={0}
          aria-label="拖拽调整面板宽度"
        >
          <GripVertical
            className={cn(
              'w-3 h-4 absolute top-1/2 -translate-y-1/2',
              'text-muted-foreground/50',
              'opacity-0 group-hover:opacity-100 transition-opacity',
              side === 'left' ? '-right-1' : '-left-1'
            )}
          />
        </div>
      )}
    </div>
  );
});

export default ResizablePanel;