import { useState } from "react";
import omninovalLogo from "../assets/omninoval-logo.png";

export type AppNavId =
  | "chat"
  | "cron"
  | "general"
  | "providers"
  | "channels"
  | "skills"
  | "persona";

interface NavItem {
  id: AppNavId;
  label: string;
  icon: "chat" | "grid" | "bot" | "nodes" | "puzzle" | "clock" | "gear";
}

const PRIMARY_NAV: NavItem[] = [
  { id: "chat", label: "新对话", icon: "chat" },
  { id: "providers", label: "模型", icon: "grid" },
  { id: "persona", label: "Agents", icon: "bot" },
  { id: "channels", label: "频道", icon: "nodes" },
  { id: "skills", label: "技能", icon: "puzzle" },
  { id: "cron", label: "定时任务", icon: "clock" },
];

function NavIcon({ name }: { name: NavItem["icon"] }) {
  const common = { width: 18, height: 18, strokeWidth: 1.6, stroke: "currentColor", fill: "none" };
  switch (name) {
    case "chat":
      return (
        <svg viewBox="0 0 24 24" aria-hidden {...common}>
          <path d="M4 6h16v10H8l-4 4v-14z" strokeLinejoin="round" />
        </svg>
      );
    case "grid":
      return (
        <svg viewBox="0 0 24 24" aria-hidden {...common}>
          <rect x="4" y="4" width="6" height="6" rx="1" />
          <rect x="14" y="4" width="6" height="6" rx="1" />
          <rect x="4" y="14" width="6" height="6" rx="1" />
          <rect x="14" y="14" width="6" height="6" rx="1" />
        </svg>
      );
    case "bot":
      return (
        <svg viewBox="0 0 24 24" aria-hidden {...common}>
          <rect x="5" y="8" width="14" height="10" rx="2" />
          <circle cx="9" cy="12" r="1" fill="currentColor" stroke="none" />
          <circle cx="15" cy="12" r="1" fill="currentColor" stroke="none" />
          <path d="M9 5v3M15 5v3" strokeLinecap="round" />
        </svg>
      );
    case "nodes":
      return (
        <svg viewBox="0 0 24 24" aria-hidden {...common}>
          <circle cx="6" cy="6" r="2" />
          <circle cx="18" cy="6" r="2" />
          <circle cx="12" cy="18" r="2" />
          <path d="M6 8v6l6 4M18 8v6l-6 4" />
        </svg>
      );
    case "puzzle":
      return (
        <svg viewBox="0 0 24 24" aria-hidden {...common}>
          <path
            d="M8 4h4a2 2 0 012 2v2h2a2 2 0 012 2v4h-2a2 2 0 00-2 2v2H8v-2a2 2 0 00-2-2H4V8h2a2 2 0 002-2V4z"
            strokeLinejoin="round"
          />
        </svg>
      );
    case "clock":
      return (
        <svg viewBox="0 0 24 24" aria-hidden {...common}>
          <circle cx="12" cy="12" r="8" />
          <path d="M12 8v4l3 2" strokeLinecap="round" />
        </svg>
      );
    case "gear":
      return (
        <svg viewBox="0 0 24 24" aria-hidden {...common}>
          <circle cx="12" cy="12" r="3" />
          <path
            strokeLinecap="round"
            d="M12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42"
          />
        </svg>
      );
    default:
      return null;
  }
}

interface AppShellProps {
  activeNav: AppNavId;
  onNavigate: (id: AppNavId) => void;
  children: React.ReactNode;
}

export function AppShell({ activeNav, onNavigate, children }: AppShellProps) {
  const [collapsed, setCollapsed] = useState(false);

  return (
    <div className={`app-shell-root ${collapsed ? "app-shell-root--collapsed" : ""}`}>
      <aside className="app-shell-sidebar">
        <div className="app-shell-sidebar-head">
          <div className="app-shell-brand">
            <img src={omninovalLogo} alt="" className="app-shell-logo" />
            {!collapsed && (
              <span className="app-shell-brand-text">
                OmniNova <strong>Claw</strong>
              </span>
            )}
          </div>
          <button
            type="button"
            className="app-shell-collapse"
            onClick={() => setCollapsed((c) => !c)}
            title={collapsed ? "展开侧栏" : "收起侧栏"}
            aria-label="切换侧栏"
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              {collapsed ? <path d="M9 18l6-6-6-6" /> : <path d="M15 18l-6-6 6-6" />}
            </svg>
          </button>
        </div>

        <nav className="app-shell-nav" aria-label="主导航">
          {PRIMARY_NAV.map((item) => (
            <button
              key={item.id}
              type="button"
              className={`app-shell-nav-item ${activeNav === item.id ? "is-active" : ""}`}
              onClick={() => onNavigate(item.id)}
            >
              <span className="app-shell-nav-icon">
                <NavIcon name={item.icon} />
              </span>
              {!collapsed && <span className="app-shell-nav-label">{item.label}</span>}
            </button>
          ))}
        </nav>

        <div className="app-shell-sidebar-foot">
          <button
            type="button"
            className={`app-shell-nav-item ${activeNav === "general" ? "is-active" : ""}`}
            onClick={() => onNavigate("general")}
          >
            <span className="app-shell-nav-icon">
              <NavIcon name="gear" />
            </span>
            {!collapsed && <span className="app-shell-nav-label">设置</span>}
          </button>
        </div>
      </aside>

      <div className="app-shell-main">{children}</div>
    </div>
  );
}
