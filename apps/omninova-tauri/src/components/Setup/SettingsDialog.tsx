import { useEffect } from "react";
import { UiIcon } from "../UiIcon";
import { Setup, type SetupTab } from "./Setup";
import "./SettingsDialog.css";

const TABS: Array<{ id: SetupTab; label: string }> = [
  { id: "general", label: "通用" },
  { id: "providers", label: "模型" },
  { id: "channels", label: "渠道" },
  { id: "skills", label: "技能" },
  { id: "persona", label: "Agents" },
];

type SettingsDialogProps = {
  activeTab: SetupTab;
  onTabChange: (tab: SetupTab) => void;
  onClose: () => void;
};

export function SettingsDialog({
  activeTab,
  onTabChange,
  onClose,
}: SettingsDialogProps) {
  useEffect(() => {
    const previous = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.body.style.overflow = previous;
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [onClose]);

  return (
    <div
      className="settings-dialog-overlay"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        className="settings-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="settings-dialog-title"
      >
        <header className="settings-dialog-head">
          <div>
            <h2 id="settings-dialog-title">设置</h2>
            <p>网关、编排与安全域。保存后立即作用于本机任务。</p>
          </div>
          <button
            type="button"
            className="settings-dialog-close"
            onClick={onClose}
            aria-label="关闭设置"
          >
            <UiIcon name="close" size={16} />
          </button>
        </header>

        <nav className="settings-dialog-tabs" aria-label="设置分区">
          {TABS.map((tab) => (
            <button
              key={tab.id}
              type="button"
              className={activeTab === tab.id ? "is-active" : undefined}
              onClick={() => onTabChange(tab.id)}
            >
              {tab.label}
            </button>
          ))}
        </nav>

        <div className="settings-dialog-body">
          <Setup
            embedded
            presentation="dialog"
            activeTab={activeTab}
            onTabChange={onTabChange}
          />
        </div>
      </div>
    </div>
  );
}
