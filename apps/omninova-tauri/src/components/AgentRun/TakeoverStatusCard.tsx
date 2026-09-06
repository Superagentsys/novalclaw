import React, { useCallback, useEffect, useState } from "react";
import { invokeTauri } from "../../utils/tauri";
import { listenAgentRunEvents } from "../../utils/events";
import type { AgentRunEvent, BrowserTakeoverStateDto } from "./types";
import {
  applyAuthoritativeTakeoverState,
  shouldRefreshTakeoverFromEvent,
  shouldShowTakeoverCard,
  takeoverErrorCopy,
  takeoverPrimaryAction,
  takeoverStatusCopy,
} from "./takeoverUi";

interface TakeoverStatusCardProps {
  runId: string;
}

export const TakeoverStatusCard: React.FC<TakeoverStatusCardProps> = ({ runId }) => {
  const [state, setState] = useState<BrowserTakeoverStateDto | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const queryState = useCallback(async () => {
    try {
      const next = await invokeTauri<BrowserTakeoverStateDto>("get_browser_takeover_state", {
        runId,
      });
      setState((previous) => applyAuthoritativeTakeoverState(previous, next));
      setError(null);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setState(null);
      setError(takeoverErrorCopy(message));
    }
  }, [runId]);

  useEffect(() => {
    let disposed = false;
    void queryState();
    const listen = listenAgentRunEvents<AgentRunEvent>("agent-run-event", (event) => {
      if (disposed) return;
      const payload = event.payload;
      if (!shouldRefreshTakeoverFromEvent(payload.type, payload.run_id, runId)) return;
      void queryState();
    });
    return () => {
      disposed = true;
      void listen.then((unlisten) => unlisten());
    };
  }, [queryState, runId]);

  const invokeAction = async (command: "request_browser_takeover" | "release_browser_takeover" | "cancel_browser_takeover") => {
    setBusy(true);
    try {
      const next = await invokeTauri<BrowserTakeoverStateDto>(command, { runId });
      setState((previous) => applyAuthoritativeTakeoverState(previous, next));
      setError(null);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(takeoverErrorCopy(message));
      await queryState();
    } finally {
      setBusy(false);
    }
  };

  if (!shouldShowTakeoverCard(state) && !error) {
    return null;
  }

  const copy = state ? takeoverStatusCopy(state) : null;
  const action = state ? takeoverPrimaryAction(state) : "none";
  const showCancel =
    state?.phase === "human_controlled" || state?.phase === "timed_out";

  return (
    <div
      className={`agent-run-takeover agent-run-takeover--${copy?.tone ?? "warn"}`}
      data-phase={state?.phase ?? "unavailable"}
    >
      <div className="agent-run-takeover-copy">
        <strong>{copy?.title ?? "无法读取接管状态"}</strong>
        <small>{error ?? copy?.detail}</small>
      </div>
      <div className="agent-run-takeover-actions">
        {action === "take" ? (
          <button
            type="button"
            disabled={busy}
            onClick={() => {
              void invokeAction("request_browser_takeover");
            }}
          >
            接管浏览器
          </button>
        ) : null}
        {action === "release" ? (
          <button
            type="button"
            disabled={busy}
            onClick={() => {
              void invokeAction("release_browser_takeover");
            }}
          >
            交还控制权
          </button>
        ) : null}
        {showCancel ? (
          <button
            type="button"
            className="agent-run-takeover-secondary"
            disabled={busy}
            onClick={() => {
              void invokeAction("cancel_browser_takeover");
            }}
          >
            取消接管
          </button>
        ) : null}
      </div>
    </div>
  );
};
