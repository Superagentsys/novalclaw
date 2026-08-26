import React, {
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type RefObject,
} from "react";
import {
  BREAKDOWN_LABELS,
  selectContextUsageView,
  type ContextUsageState,
  type ContextUsageView,
} from "../AgentRun/contextUsageState";
import { formatTokenCount } from "../AgentRun/contextTokens";

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
  if (view.placeholder) return "正在计算…";
  const estimate = formatTokenCount(view.estimatedTokens ?? 0, "estimate");
  if (!view.knownBudget || view.maxInputTokens == null || view.percent == null) {
    return `${estimate} · 窗口未知`;
  }
  return `${view.percent}% · ${estimate} / ${formatTokenCount(view.maxInputTokens, "exact")}`;
}

interface ContextUsageBadgeContentProps {
  view: ContextUsageView;
  open: boolean;
  panelId: string;
  triggerRef?: RefObject<HTMLButtonElement | null>;
  onToggle: () => void;
}

/** Presentation-only content, exported so truth-focused UI output can be tested without a DOM. */
export function ContextUsageBadgeContent({
  view,
  open,
  panelId,
  triggerRef,
  onToggle,
}: ContextUsageBadgeContentProps) {
  const estimateText =
    view.estimatedTokens == null ? null : formatTokenCount(view.estimatedTokens, "estimate");
  const maxText =
    view.knownBudget && view.maxInputTokens != null
      ? formatTokenCount(view.maxInputTokens, "exact")
      : null;
  const statusLabel = compactStatusLabel(view);
  const visibleBreakdown = view.breakdown
    ? BREAKDOWN_LABELS.filter((item) => view.breakdown![item.key] > 0)
    : [];

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

      {open ? (
        <div
          id={panelId}
          className="context-usage-panel"
          role="dialog"
          aria-label="上下文用量详情"
        >
          <header className="context-usage-header">
            <div className="context-usage-heading">
              <span className="context-usage-title">
                {view.placeholder
                  ? "上下文"
                  : view.knownBudget && view.percent != null
                    ? `上下文已用 ${view.percent}%`
                    : "上下文估算"}
              </span>
              {!view.placeholder ? (
                <span className="context-usage-kind">{view.measurementLabel}</span>
              ) : null}
            </div>
            <span className="context-usage-total">
              {view.placeholder
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
                {visibleBreakdown.length > 0 && view.estimatedTokens ? (
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

          {visibleBreakdown.length > 0 ? (
            <section className="context-usage-section" aria-labelledby={`${panelId}-breakdown`}>
              <div className="context-usage-section-heading">
                <span id={`${panelId}-breakdown`}>输入构成</span>
                {view.revision != null ? <span>Revision {view.revision}</span> : null}
              </div>
              <dl className="context-usage-rows context-usage-rows--breakdown">
                {visibleBreakdown.map((item) => (
                  <div key={item.key} className="context-usage-row">
                    <dt>
                      <span
                        className={`context-usage-marker ${BREAKDOWN_MARKER_CLASS[item.key]}`}
                        aria-hidden="true"
                      />
                      {item.label}
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
                      : `${formatTokenCount(view.estimatedTokens, "estimate")} · ${view.estimatedTokens.toLocaleString("zh-CN")}`}
                  </dd>
                </div>
                {view.lastActualTokens != null ? (
                  <div className="context-usage-row">
                    <dt>上次请求实际</dt>
                    <dd>
                      {formatTokenCount(view.lastActualTokens, "exact")} · {view.lastActualTokens.toLocaleString("zh-CN")}
                    </dd>
                  </div>
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
      ) : null}
    </React.Fragment>
  );
}

export function ContextUsageBadge({ state }: { state: ContextUsageState }) {
  const view = useMemo(() => selectContextUsageView(state), [state]);
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelId = useId();

  useEffect(() => {
    if (!open) return;
    const onPointer = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
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

  return (
    <div ref={rootRef} className={`context-usage${open ? " is-open" : ""}`}>
      <ContextUsageBadgeContent
        view={view}
        open={open}
        panelId={panelId}
        triggerRef={triggerRef}
        onToggle={() => setOpen((value) => !value)}
      />
    </div>
  );
}
