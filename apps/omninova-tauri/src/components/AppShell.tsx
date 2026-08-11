import { useEffect, useState, type ReactNode } from "react";
import omninovalLogo from "../assets/omninoval-logo.png";
import { ThemeSwitcher } from "./ThemeSwitcher";
import { UiIcon, type UiIconName } from "./UiIcon";

export type AppNavId =
  | "chat"
  | "general"
  | "providers"
  | "channels"
  | "skills"
  | "persona";

interface NavItem {
  id: AppNavId;
  label: string;
  description: string;
  icon: UiIconName;
}

const WORKSPACE_NAV: NavItem[] = [
  { id: "chat", label: "任务中心", description: "对话、执行与任务历史", icon: "message" },
];

const RESOURCE_NAV: NavItem[] = [
  { id: "providers", label: "模型服务", description: "Provider 与默认模型", icon: "apps" },
  { id: "persona", label: "Agents", description: "人设、Workspace 与行为", icon: "agent" },
  { id: "skills", label: "技能市场", description: "安装与管理技能", icon: "tool" },
  { id: "channels", label: "渠道连接", description: "飞书、Webhook 等入口", icon: "connections" },
];

const SIDEBAR_STORAGE_KEY = "omninova.ui.sidebarCollapsed.v1";

interface AppShellProps {
  activeNav: AppNavId;
  onNavigate: (id: AppNavId) => void;
  children: ReactNode;
}

function readSidebarPreference(): boolean {
  try {
    return window.localStorage.getItem(SIDEBAR_STORAGE_KEY) === "1";
  } catch {
    return false;
  }
}

export function AppShell({ activeNav, onNavigate, children }: AppShellProps) {
  const [collapsed, setCollapsed] = useState(readSidebarPreference);

  useEffect(() => {
    try {
      window.localStorage.setItem(SIDEBAR_STORAGE_KEY, collapsed ? "1" : "0");
    } catch {
      // Keep the in-memory preference when WebView storage is unavailable.
    }
  }, [collapsed]);

  const renderNavGroup = (heading: string, items: NavItem[]) => (
    <section className="app-shell-nav-section" aria-label={heading}>
      {!collapsed ? <h2 className="app-shell-nav-heading">{heading}</h2> : null}
      <div className="app-shell-nav-list">
        {items.map((item) => (
          <button
            key={item.id}
            type="button"
            className={`app-shell-nav-item ${activeNav === item.id ? "is-active" : ""}`}
            onClick={() => onNavigate(item.id)}
            aria-current={activeNav === item.id ? "page" : undefined}
            aria-label={collapsed ? item.label : undefined}
            title={collapsed ? item.label : item.description}
          >
            <span className="app-shell-nav-icon">
              <UiIcon name={item.icon} />
            </span>
            {!collapsed ? (
              <span className="app-shell-nav-copy">
                <span className="app-shell-nav-label">{item.label}</span>
                <span className="app-shell-nav-description">{item.description}</span>
              </span>
            ) : null}
          </button>
        ))}
      </div>
    </section>
  );

  return (
    <div className={`app-shell-root ${collapsed ? "app-shell-root--collapsed" : ""}`}>
      <aside className="app-shell-sidebar">
        <div className="app-shell-sidebar-head">
          <div className="app-shell-brand">
            <img src={omninovalLogo} alt="" className="app-shell-logo" />
            {!collapsed ? (
              <span className="app-shell-brand-copy">
                <span className="app-shell-brand-kicker">LOCAL AGENT DESKTOP</span>
                <span className="app-shell-brand-text">
                  OmniNova <strong>Claw</strong>
                </span>
              </span>
            ) : null}
          </div>
          <button
            type="button"
            className="app-shell-collapse"
            onClick={() => setCollapsed((value) => !value)}
            title={collapsed ? "展开侧栏" : "收起侧栏"}
            aria-label={collapsed ? "展开侧栏" : "收起侧栏"}
            aria-expanded={!collapsed}
          >
            <UiIcon name={collapsed ? "menuUnfold" : "menuFold"} size={16} />
          </button>
        </div>

        <nav className="app-shell-nav" aria-label="主导航">
          {renderNavGroup("工作台", WORKSPACE_NAV)}
          {renderNavGroup("能力与连接", RESOURCE_NAV)}
        </nav>

        <div className="app-shell-sidebar-foot">
          {!collapsed ? (
            <div className="app-shell-local-status" role="status">
              <span aria-hidden />
              <div>
                <strong>本地优先</strong>
                <small>配置与任务保存在本机</small>
              </div>
            </div>
          ) : null}
          <ThemeSwitcher collapsed={collapsed} />
          <button
            type="button"
            className={`app-shell-nav-item ${activeNav === "general" ? "is-active" : ""}`}
            onClick={() => onNavigate("general")}
            aria-current={activeNav === "general" ? "page" : undefined}
            aria-label={collapsed ? "设置" : undefined}
            title={collapsed ? "设置" : "应用、网关与隐私设置"}
          >
            <span className="app-shell-nav-icon">
              <UiIcon name="settings" />
            </span>
            {!collapsed ? (
              <span className="app-shell-nav-copy">
                <span className="app-shell-nav-label">设置</span>
                <span className="app-shell-nav-description">应用、网关与隐私</span>
              </span>
            ) : null}
          </button>
        </div>
      </aside>

      <div className="app-shell-main">{children}</div>
    </div>
  );
}
