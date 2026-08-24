import { UiIcon } from "../UiIcon";

export interface MessageActionsProps {
  role: "user" | "assistant" | "error";
  copied: boolean;
  onCopy: () => void;
  onEdit?: () => void;
  onRetry?: () => void;
  disabled?: boolean;
}

export function MessageActions({
  role,
  copied,
  onCopy,
  onEdit,
  onRetry,
  disabled = false,
}: MessageActionsProps) {
  return (
    <div className={`message-actions message-actions--${role}`}>
      <button
        type="button"
        className="message-action"
        aria-label="复制消息"
        title="复制消息"
        disabled={disabled}
        onClick={onCopy}
      >
        <UiIcon name={copied ? "check" : "copy"} size={13} />
      </button>
      {role === "user" && onEdit ? (
        <button
          type="button"
          className="message-action"
          aria-label="编辑消息"
          title="编辑消息"
          disabled={disabled}
          onClick={onEdit}
        >
          <UiIcon name="edit" size={13} />
        </button>
      ) : null}
      {role === "assistant" && onRetry ? (
        <button
          type="button"
          className="message-action"
          aria-label="重新生成"
          title="重新生成"
          disabled={disabled}
          onClick={onRetry}
        >
          <UiIcon name="reload" size={13} />
        </button>
      ) : null}
    </div>
  );
}