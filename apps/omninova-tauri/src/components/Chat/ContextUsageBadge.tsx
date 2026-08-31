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
import {
  ContextProgressRing,
  type ContextProgressRingState,
  type ContextProgressRingTone,
} from "./ContextProgressRing";

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

function generationLimitSourceLabel(source: string): string {
  switch (source) {
    case "request_override":
      return "本次请求指定";
    case "profile_override":
      return "模型配置";
    case "product_default":
      return "OmniNova 默认策略";
    case "model_maximum_fallback":
      return "保守回退：模型最大输出上限";
    default:
      return source;
  }
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
  if (view.activeStatus === "compaction") return "正在压缩…";
  if (view.activeStatus === "pruning") return "正在裁剪工具结果…";
  if (view.activeStatus === "recovery") return "正在恢复…";
  if (view.refreshing) return "正在刷新…";
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

export function resolveContextProgressRingState(view: ContextUsageView): ContextProgressRingState {
  if (view.unavailable && view.estimatedTokens == null) return "unavailable";
  if (view.activeStatus === "compaction") return "compaction";
  if (view.refreshing) return "refreshing";
  if (view.placeholder) return "calculating";
  if (!view.knownBudget || view.percent == null) return "unknown";
  return "normal";
}

export function resolveContextProgressRingTone(view: ContextUsageView): ContextProgressRingTone {
  if (view.unavailable && view.estimatedTokens == null) return "error";
  if (!view.knownBudget || view.estimatedTokens == null) return "neutral";
  if (view.pressureThresholdTokens == null || view.pressureThresholdTokens <= 0) return "normal";
  if (view.estimatedTokens >= view.pressureThresholdTokens) return "critical";
  if (view.estimatedTokens >= view.pressureThresholdTokens * 0.85) return "warning";
  return "normal";
}

function badgeAriaLabel(view: ContextUsageView): string {
  if (view.unavailable && view.estimatedTokens == null) return "上下文暂不可用";
  if (view.placeholder && view.estimatedTokens == null) return "正在计算上下文输入";

  const estimate = view.estimatedTokens == null
    ? null
    : formatTokenCount(view.estimatedTokens, "exact");
  const estimateQualifier = view.measurementExact ? "" : "约";
  let label: string;

  if (estimate == null) {
    label = "当前上下文输入未知";
  } else if (view.knownBudget && view.maxInputTokens != null && view.percent != null) {
    label = `上下文输入 ${view.percent}%，当前${estimateQualifier} ${estimate} Token，最大输入预算 ${formatTokenCount(view.maxInputTokens, "exact")} Token`;
  } else {
    label = `当前上下文${estimateQualifier} ${estimate} Token，模型输入预算未知`;
  }

  const transient = compactStatusLabel(view);
  return transient ? `${label}，${transient.replace(/…$/, "")}` : label;
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
  const visibleBreakdown = view.breakdown
    ? BREAKDOWN_LABELS.filter((item) => view.breakdown![item.key] > 0)
    : [];
  const ringState = resolveContextProgressRingState(view);
  const ringTone = resolveContextProgressRingTone(view);
  const triggerStatus = compactStatusLabel(view);
  const summary = badgeSummary(view);
  const triggerTitle = `${summary}${triggerStatus && !summary.includes(triggerStatus) ? ` · ${triggerStatus}` : ""}\n点击查看详情`;

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
                {view.modelMaxOutputTokens != null ? (
                  <div className="context-usage-row">
                    <dt>模型最大输出上限</dt>
                    <dd>{formatTokenCount(view.modelMaxOutputTokens, "exact")}</dd>
                  </div>
                ) : null}
                {view.requestOutputReserveTokens != null ? (
                  <div className="context-usage-row">
                    <dt>本次输出预留</dt>
                    <dd>{formatTokenCount(view.requestOutputReserveTokens, "exact")}</dd>
                  </div>
                ) : null}
                {view.requestGenerationLimitSource ? (
                  <div className="context-usage-row">
                    <dt>来源</dt>
                    <dd>{generationLimitSourceLabel(view.requestGenerationLimitSource)}</dd>
                  </div>
                ) : view.requestReserveIsConservativeFallback ? (
                  <div className="context-usage-row">
                    <dt>来源</dt>
                    <dd>保守回退：模型最大输出上限</dd>
                  </div>
                ) : null}
                {view.safetyReserveTokens != null ? (
                  <div className="context-usage-row">
                    <dt>安全预留</dt>
                    <dd>{formatTokenCount(view.safetyReserveTokens, "exact")}</dd>
                  </div>
                ) : null}
                {view.maxInputTokens != null ? (
                  <div className="context-usage-row">
                    <dt>当前输入预算</dt>
                    <dd>{formatTokenCount(view.maxInputTokens, "exact")}</dd>
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
        className={`context-usage-trigger is-${ringState} tone-${ringTone}`}
        data-context-state={ringState}
        data-pressure-tone={ringTone}
        aria-expanded={open}
        aria-haspopup="dialog"
        aria-controls={panelId}
        aria-label={badgeAriaLabel(view)}
        title={triggerTitle}
        onClick={onToggle}
      >
        <ContextProgressRing
          percentage={view.knownBudget ? view.barPercent : null}
          state={ringState}
          tone={ringTone}
        />
        <span className={`context-usage-compact${view.placeholder ? " is-placeholder" : ""}`}>
          {view.knownBudget && view.percent != null ? (
            <span className="context-usage-primary">
              <span className="context-usage-percent">{view.percent}%</span>
            </span>
          ) : view.unavailable && view.estimatedTokens == null ? (
            <span className="context-usage-primary">
              <span className="context-usage-state-text">暂不可用</span>
            </span>
          ) : view.placeholder ? (
            <span className="context-usage-primary">
              <span className="context-usage-state-text">计算中</span>
            </span>
          ) : null}
          {estimateText ? (
            <span className="context-usage-summary">
              <span className="context-usage-separator" aria-hidden="true">·</span>
              <span className="context-usage-current">{estimateText}</span>
              {maxText ? (
                <span className="context-usage-denominator"> / {maxText}</span>
              ) : (
                <span className="context-usage-unknown-label"> · 窗口未知</span>
              )}
            </span>
          ) : null}
          {triggerStatus ? (
            <span className="context-usage-transient" role="status" aria-live="polite">
              {triggerStatus}
            </span>
          ) : null}
        </span>
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
