import React, { useCallback, useEffect, useState } from "react";
import { invokeTauri } from "../../utils/tauri";
import type { PersonalChromeAuthorizationStatusDto } from "./types";
import {
  applyAuthoritativePersonalChromeStatus,
  personalChromeAuthorizationAction,
  personalChromeAuthorizationCopy,
  personalChromeAuthorizationErrorCopy,
  shouldShowPersonalChromeAuthorization,
} from "./personalChromeAuthorizationUi";

interface PersonalChromeAuthorizationCardProps {
  runId: string;
}

export const PersonalChromeAuthorizationCard: React.FC<
  PersonalChromeAuthorizationCardProps
> = ({ runId }) => {
  const [status, setStatus] = useState<PersonalChromeAuthorizationStatusDto | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const queryStatus = useCallback(async () => {
    try {
      const next = await invokeTauri<PersonalChromeAuthorizationStatusDto>(
        "get_personal_chrome_authorization_status",
        { runId }
      );
      setStatus((previous) => applyAuthoritativePersonalChromeStatus(previous, next));
      setError(null);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(personalChromeAuthorizationErrorCopy(message));
    }
  }, [runId]);

  useEffect(() => {
    let disposed = false;
    const refresh = () => {
      if (!disposed) void queryStatus();
    };
    refresh();
    const timer = window.setInterval(refresh, 1500);
    return () => {
      disposed = true;
      window.clearInterval(timer);
    };
  }, [queryStatus]);

  const invokeAction = async (
    command: "approve_personal_chrome_for_run" | "revoke_personal_chrome_for_run"
  ) => {
    setBusy(true);
    try {
      const next = await invokeTauri<PersonalChromeAuthorizationStatusDto>(command, { runId });
      setStatus((previous) => applyAuthoritativePersonalChromeStatus(previous, next));
      setError(null);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(personalChromeAuthorizationErrorCopy(message));
      await queryStatus();
    } finally {
      setBusy(false);
    }
  };

  if (!shouldShowPersonalChromeAuthorization(status) && !error) return null;

  const copy = status ? personalChromeAuthorizationCopy(status) : null;
  const action = status ? personalChromeAuthorizationAction(status) : "none";

  return (
    <div
      className={`agent-run-personal-chrome agent-run-personal-chrome--${copy?.tone ?? "error"}`}
      data-authorization-state={status?.state ?? "unavailable"}
    >
      <div className="agent-run-personal-chrome-copy">
        <strong>{copy?.title ?? "无法读取 Personal Chrome 授权"}</strong>
        <small>{error ?? copy?.detail}</small>
      </div>
      <div className="agent-run-personal-chrome-actions">
        {action === "approve" ? (
          <button
            type="button"
            disabled={busy}
            onClick={() => void invokeAction("approve_personal_chrome_for_run")}
          >
            允许当前任务
          </button>
        ) : null}
        {action === "revoke" ? (
          <button
            type="button"
            className="agent-run-personal-chrome-secondary"
            disabled={busy}
            onClick={() => void invokeAction("revoke_personal_chrome_for_run")}
          >
            撤销访问
          </button>
        ) : null}
      </div>
    </div>
  );
};
