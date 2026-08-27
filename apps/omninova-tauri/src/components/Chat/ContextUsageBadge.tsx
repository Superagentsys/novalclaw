import React, {
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type RefObject,
} from "react";
import { createPortal } from "react-dom";
import {
  BREAKDOWN_LABELS,
  selectContextUsageView,
  formatParityRelativeError,
  type ContextUsageState,
  type ContextUsageView,
} from "../AgentRun/contextUsageState";
import { formatTokenCount } from "../AgentRun/contextTokens";

export const CONTEXT_USAGE_VIEWPORT_MARGIN = 12;
export const CONTEXT_USAGE_PREFERRED_WIDTH = 360;
const CONTEXT_USAGE_GAP = 8;
const CONTEXT_USAGE_MAX_HEIGHT = 520;

export type ContextUsageRect = {
  top: number;
  left: number;
  right: number;
  bottom: number;
  width: number;
  height: number;
};

export type ContextUsagePopoverPosition = {
  left: number;
  top: number;
  width: number;
  placement: "above" | "below";
};

function clamp(value: number, min: number, max: number): number {
  if (max < min) return min;
  return Math.min(max, Math.max(min, value));
}

export function measureContextUsagePopoverWidth(viewportWidth: number): number {
  return Math.max(0, Math.min(CONTEXT_USAGE_PREFERRED_WIDTH, viewportWidth - CONTEXT_USAGE_VIEWPORT_MARGIN * 2));
}

export function measureContextUsagePopoverMaxHeight(viewportHeight: number): number {
  return Math.max(
    0,
    Math.min(
      CONTEXT_USAGE_MAX_HEIGHT,
      viewportHeight * 0.7,
      viewportHeight - CONTEXT_USAGE_VIEWPORT_MARGIN * 2
    )
  );
}

export function computeContextUsagePopoverPosition(args: {
  trigger: ContextUsageRect;
  viewport: { width: number; height: number };
  popover: { width: number; height: number };
  gap?: number;
  margin?: number;
}): ContextUsagePopoverPosition {
  const margin = args.margin ?? CONTEXT_USAGE_VIEWPORT_MARGIN;
  const gap = args.gap ?? CONTEXT_USAGE_GAP;
  const width = Math.min(args.popover.width, measureContextUsagePopoverWidth(args.viewport.width));
  const height = Math.min(args.popover.height, measureContextUsagePopoverMaxHeight(args.viewport.height));
  const needed = height + gap;
  const canPlaceAbove = args.trigger.top - margin >= needed;
  const placement: "above" | "below" = canPlaceAbove ? "above" : "below";
  const desiredTop =
    placement === "above" ? args.trigger.top - gap - height : args.trigger.bottom + gap;
  const desiredLeft = args.trigger.right - width;
  return {
    left: clamp(desiredLeft, margin, args.viewport.width - width - margin),
    top: clamp(desiredTop, margin, args.viewport.height - height - margin),
    width,
    placement,
  };
}

export function isOutsideContextUsageClick(
  target: { contains?(node: unknown): boolean } | EventTarget | null,
  surfaces: Array<{ contains(node: unknown): boolean } | null | undefined>
): boolean {
  if (!target) return true;
  return surfaces.every((surface) => !surface?.contains(target));
}

const BREAKDOWN_MARKER_CLASS: Record<string, string> = {
  system_tokens: "is-system",
  conversation_tokens: "is-conversation",
  tool_schema_tokens: "is-tool-schema",
  tool_result_tokens: "is-tool-result",
  request_overhead_tokens: "is-overhead",
};

function compactStatusLabel(view: ContextUsageView): string | null {
  if (view.activeStatus === "compaction") return "压缩中";
  if (view.activeStatus === "pruning") return "裁剪中";
  if (view.activeStatus === "recovery") return "恢复中";
  return null;
}

function badgeSummary(view: ContextUsageView): string {
  if (view.unavailable && view.estimatedTokens == null) return "上下文暂不可用";
  if (view.refreshing && view.estimatedTokens == null) return "正在刷新…";
  if (view.placeholder) return "正在计算…";
  const total = formatTokenCount(view.estimatedTokens ?? 0, view.totalFormat);
  const refresh = view.refreshing ? " · 正在刷新…" : "";
  if (!view.knownBudget || view.maxInputTokens == null || view.percent == null) {
    return `${total} · 窗口未知${refresh}`;
  }
  return `${view.percent}% · ${total} / ${formatTokenCount(view.maxInputTokens, "exact")}${refresh}`;
}

interface ContextUsageBadgeContentProps {
  view: ContextUsageView;
  open: boolean;
  panelId: string;
  triggerRef?: RefObject<HTMLButtonElement | null>;
  panelRef?: RefObject<HTMLDivElement | null>;
  panelStyle?: CSSProperties;
  placement?: "above" | "below";
  portal?: boolean;
  onToggle: () => void;
}

/** Presentation-only content, exported so truth-focused UI output can be tested without a DOM. */
export function ContextUsageBadgeContent({
  view,
  open,
  panelId,
  triggerRef,
  panelRef,
  panelStyle,
  placement,
  portal = false,
  onToggle,
}: ContextUsageBadgeContentProps) {
  const estimateText =
    view.estimatedTokens == null
      ? null
      : formatTokenCount(view.estimatedTokens, view.totalFormat);
  const maxText =
    view.knownBudget && view.maxInputTokens != null
      ? formatTokenCount(view.maxInputTokens, "exact")
      : null;
  const statusLabel = compactStatusLabel(view);
  const visibleBreakdown = view.breakdown
    ? BREAKDOWN_LABELS.filter((item) => view.breakdown![item.key] > 0)
    : [];

  const panel = open ? (
        <div
          ref={panelRef}
          id={panelId}
          className="context-usage-panel"
          role="dialog"
          aria-label="上下文用量详情"
          data-placement={placement}
          style={panelStyle}
        >
          <header className="context-usage-header">
            <div className="context-usage-heading">
              <span className="context-usage-title">
                {view.unavailable && view.estimatedTokens == null
                  ? "上下文暂不可用"
                  : view.placeholder
                    ? "上下文"
                    : view.knownBudget && view.percent != null
                      ? `上下文输入 ${view.percent}%`
                      : "上下文估算"}
              </span>
              {!view.placeholder && !view.unavailable ? (
                <span className="context-usage-kind">{view.measurementLabel}</span>
              ) : view.refreshing ? (
                <span className="context-usage-kind">正在刷新…</span>
              ) : null}
            </div>
            <span className="context-usage-total">
              {view.unavailable && view.estimatedTokens == null
                ? "—"
                : view.refreshing && view.estimatedTokens == null
                  ? "正在刷新…"
                  : view.placeholder
                    ? "正在计算…"
                    : view.knownBudget && estimateText && maxText
                      ? `${estimateText} / ${maxText}`
                      : estimateText}
            </span>
          </header>

          {view.knownBudget && view.barPercent != null ? (
            <div className="context-usage-meter-wrap">
              <div
                className="context-usage-bar"
                role="progressbar"
                aria-label="上下文输入预算使用率"
                aria-valuemin={0}
                aria-valuemax={100}
                aria-valuenow={view.barPercent}
                aria-valuetext={`${view.percent}% · ${estimateText} / ${maxText}`}
              >
                {visibleBreakdown.length > 0 && view.estimatedTokens && !view.breakdownIndependent ? (
                  <span
                    className="context-usage-segments"
                    style={{ width: `${view.barPercent}%` }}
                    aria-hidden="true"
                  >
                    {visibleBreakdown.map((item) => (
                      <span
                        key={item.key}
                        className={`context-usage-segment ${BREAKDOWN_MARKER_CLASS[item.key]}`}
                        style={{
                          width: `${Math.max(
                            0,
                            Math.min(100, (view.breakdown![item.key] / view.estimatedTokens!) * 100)
                          )}%`,
                        }}
                      />
                    ))}
                  </span>
                ) : (
                  <span
                    className="context-usage-bar-fill"
                    style={{ width: `${view.barPercent}%` }}
                    aria-hidden="true"
                  />
                )}
                {view.thresholdPercent != null ? (
                  <span
                    className="context-usage-bar-threshold"
                    style={{ left: `${view.thresholdPercent}%` }}
                    aria-hidden="true"
                  />
                ) : null}
              </div>
              {view.thresholdPercent != null ? (
                <div className="context-usage-threshold-note">
                  自动维护阈值 {formatTokenCount(view.pressureThresholdTokens!, "exact")}
                </div>
              ) : null}
            </div>
          ) : null}

          {!view.placeholder ? (
            <p className="context-usage-hint">当前模型可见上下文，不含输入框未发送草稿。</p>
          ) : null}

          {visibleBreakdown.length > 0 ? (
            <section className="context-usage-section" aria-labelledby={`${panelId}-breakdown`}>
              <div className="context-usage-section-heading">
                <span id={`${panelId}-breakdown`}>估算构成</span>
                {view.revision != null ? <span>Revision {view.revision}</span> : null}
              </div>
              {view.breakdownCaption ? (
                <p className="context-usage-hint">{view.breakdownCaption}</p>
              ) : null}
              <dl className="context-usage-rows context-usage-rows--breakdown">
                {visibleBreakdown.map((item) => (
                  <div key={item.key} className="context-usage-row">
                    <dt>
                      <span
                        className={`context-usage-marker ${BREAKDOWN_MARKER_CLASS[item.key]}`}
                        aria-hidden="true"
                      />
                      <span className="context-usage-row-label">{item.label}</span>
                    </dt>
                    <dd>{formatTokenCount(view.breakdown![item.key], "estimate")}</dd>
                  </div>
                ))}
              </dl>
            </section>
          ) : null}

          {view.knownBudget ? (
            <section className="context-usage-section" aria-labelledby={`${panelId}-budget`}>
              <div className="context-usage-section-heading">
                <span id={`${panelId}-budget`}>预算边界</span>
              </div>
              <dl className="context-usage-rows context-usage-rows--secondary">
                {view.contextWindowTokens != null ? (
                  <div className="context-usage-row">
                    <dt>模型上下文窗口</dt>
                    <dd>{formatTokenCount(view.contextWindowTokens, "exact")}</dd>
                  </div>
                ) : null}
                {view.maxInputTokens != null ? (
                  <div className="context-usage-row">
                    <dt>最大输入预算</dt>
                    <dd>{formatTokenCount(view.maxInputTokens, "exact")}</dd>
                  </div>
                ) : null}
                {view.outputReserveTokens != null ? (
                  <div className="context-usage-row">
                    <dt>输出预留</dt>
                    <dd>{formatTokenCount(view.outputReserveTokens, "exact")}</dd>
                  </div>
                ) : null}
                {view.pressureThresholdTokens != null ? (
                  <div className="context-usage-row">
                    <dt>自动维护阈值</dt>
                    <dd>{formatTokenCount(view.pressureThresholdTokens, "exact")}</dd>
                  </div>
                ) : null}
              </dl>
            </section>
          ) : !view.placeholder ? (
            <div className="context-usage-unknown">模型上下文窗口未知，暂不显示使用比例。</div>
          ) : null}

          {!view.placeholder ? (
            <section className="context-usage-section" aria-labelledby={`${panelId}-measurement`}>
              <div className="context-usage-section-heading">
                <span id={`${panelId}-measurement`}>请求测量</span>
              </div>
              <dl className="context-usage-rows context-usage-rows--secondary">
                <div className="context-usage-row">
                  <dt>{view.measurementLabel}</dt>
                  <dd>
                    {view.estimatedTokens == null
                      ? "—"
                      : `${formatTokenCount(view.estimatedTokens, view.totalFormat)} · ${view.estimatedTokens.toLocaleString("zh-CN")}`}
                  </dd>
                </div>
                {view.lastActualTokens != null && view.actualLabel ? (
                  <div className="context-usage-row">
                    <dt>{view.actualLabel}</dt>
                    <dd>
                      {formatTokenCount(view.lastActualTokens, "exact")} · {view.lastActualTokens.toLocaleString("zh-CN")}
                    </dd>
                  </div>
                ) : null}
                {view.parity ? (
                  <>
                    <div className="context-usage-row">
                      <dt>本地 Tokenizer</dt>
                      <dd>
                        {formatTokenCount(view.parity.localTokens, "exact")} · {view.parity.localTokens.toLocaleString("zh-CN")}
                      </dd>
                    </div>
                    <div className="context-usage-row">
                      <dt>Provider 实际</dt>
                      <dd>
                        {formatTokenCount(view.parity.actualTokens, "exact")} · {view.parity.actualTokens.toLocaleString("zh-CN")}
                      </dd>
                    </div>
                    <div className="context-usage-row">
                      <dt>差值</dt>
                      <dd>{view.parity.delta.toLocaleString("zh-CN")}</dd>
                    </div>
                    <div className="context-usage-row">
                      <dt>相对误差</dt>
                      <dd>{formatParityRelativeError(view.parity.relativeErrorPercent)}</dd>
                    </div>
                  </>
                ) : null}
              </dl>
            </section>
          ) : null}

          {view.activeStatusLabel ? (
            <div className="context-usage-live" role="status" aria-live="polite">
              <span className="context-usage-live-dot" aria-hidden="true" />
              {view.activeStatusLabel}
            </div>
          ) : null}
        </div>
  ) : null;

  const portaled =
    portal && typeof document !== "undefined" && document.body
      ? panel
        ? createPortal(panel, document.body)
        : null
      : panel;

  return (
    <React.Fragment>
      <button
        ref={triggerRef}
        type="button"
        className="context-usage-trigger"
        aria-expanded={open}
        aria-haspopup="dialog"
        aria-controls={panelId}
        aria-label={`${view.compactText}${view.activeStatusLabel ? `，${view.activeStatusLabel}` : ""}`}
        onClick={onToggle}
      >
        <span className="context-usage-kicker">上下文</span>
        <span className={`context-usage-value${view.placeholder ? " is-placeholder" : ""}`}>
          {badgeSummary(view)}
        </span>
        {statusLabel ? <span className="context-usage-status">{statusLabel}</span> : null}
      </button>
      {portaled}
    </React.Fragment>
  );
}

export function ContextUsageBadge({ state }: { state: ContextUsageState }) {
  const view = useMemo(() => selectContextUsageView(state), [state]);
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState<ContextUsagePopoverPosition | null>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const panelId = useId();

  const updatePosition = useCallback(() => {
    const trigger = triggerRef.current;
    const panel = panelRef.current;
    if (!trigger || !panel) return;
    const rect = trigger.getBoundingClientRect();
    const next = computeContextUsagePopoverPosition({
      trigger: {
        top: rect.top,
        left: rect.left,
        right: rect.right,
        bottom: rect.bottom,
        width: rect.width,
        height: rect.height,
      },
      viewport: { width: window.innerWidth, height: window.innerHeight },
      popover: {
        width: measureContextUsagePopoverWidth(window.innerWidth),
        height: Math.min(panel.offsetHeight, measureContextUsagePopoverMaxHeight(window.innerHeight)),
      },
    });
    setPosition(next);
  }, []);

  useLayoutEffect(() => {
    if (!open) {
      setPosition(null);
      return;
    }
    updatePosition();
    let frame = 0;
    const schedule = () => {
      if (frame) return;
      frame = window.requestAnimationFrame(() => {
        frame = 0;
        updatePosition();
      });
    };
    if (!panelRef.current) schedule();
    window.addEventListener("resize", schedule);
    window.addEventListener("scroll", schedule, true);
    const observer =
      typeof ResizeObserver === "undefined" ? null : new ResizeObserver(schedule);
    if (panelRef.current) observer?.observe(panelRef.current);
    return () => {
      window.removeEventListener("resize", schedule);
      window.removeEventListener("scroll", schedule, true);
      observer?.disconnect();
      if (frame) window.cancelAnimationFrame(frame);
    };
  }, [open, updatePosition, view]);

  useEffect(() => {
    if (!open) return;
    const onPointer = (event: PointerEvent) => {
      if (isOutsideContextUsageClick(event.target, [triggerRef.current, panelRef.current])) {
        setOpen(false);
      }
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      setOpen(false);
      triggerRef.current?.focus();
    };
    window.addEventListener("pointerdown", onPointer);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("pointerdown", onPointer);
      window.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const panelStyle: CSSProperties | undefined = open
    ? {
        left: position?.left ?? 0,
        top: position?.top ?? 0,
        width: position?.width ?? measureContextUsagePopoverWidth(
          typeof window === "undefined" ? CONTEXT_USAGE_PREFERRED_WIDTH : window.innerWidth
        ),
        visibility: position ? "visible" : "hidden",
      }
    : undefined;

  return (
    <div className={`context-usage${open ? " is-open" : ""}`}>
      <ContextUsageBadgeContent
        view={view}
        open={open}
        panelId={panelId}
        triggerRef={triggerRef}
        panelRef={panelRef}
        panelStyle={panelStyle}
        placement={position?.placement}
        portal
        onToggle={() => setOpen((value) => !value)}
      />
    </div>
  );
}
