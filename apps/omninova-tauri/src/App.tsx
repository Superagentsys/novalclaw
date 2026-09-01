import { useCallback, useState } from "react";
import "./App.css";
import "./ui-refresh.css";
import { AppShell, type AppNavId } from "./components/AppShell";
import { Automation } from "./components/Automation/Automation";
import { Knowledge } from "./components/Knowledge/Knowledge";
import { Chat } from "./components/Chat/Chat";
import { SettingsDialog } from "./components/Setup/SettingsDialog";
import { type SetupTab } from "./components/Setup/Setup";

const SETUP_TABS: SetupTab[] = ["general", "providers", "channels", "skills", "persona"];

function isSetupTab(id: AppNavId): id is SetupTab {
  return SETUP_TABS.includes(id as SetupTab);
}

function App() {
  const [nav, setNav] = useState<AppNavId>("chat");
  const [settingsTab, setSettingsTab] = useState<SetupTab | null>(null);
  const [mountedViews, setMountedViews] = useState<Set<AppNavId>>(
    () => new Set<AppNavId>(["chat"])
  );

  const openSettings = useCallback((tab: SetupTab) => {
    setSettingsTab(tab);
  }, []);

  const handleNavigate = useCallback((id: AppNavId) => {
    if (isSetupTab(id)) {
      setSettingsTab((current) => {
        if (id === "general") {
          return current ? null : "general";
        }
        return current === id ? null : id;
      });
      return;
    }
    if (id === "automation" || id === "knowledge") {
      setMountedViews((current) => {
        if (current.has(id)) return current;
        const next = new Set(current);
        next.add(id);
        return next;
      });
    }
    setNav(id);
    setSettingsTab(null);
  }, []);

  return (
    <div className="app-root">
      <AppShell
        activeNav={nav}
        settingsTab={settingsTab}
        onNavigate={handleNavigate}
      >
        {/*
          Chat 始终挂载，仅用显隐切换：打开设置对话框时不卸载，
          从而保留正在执行的任务、草稿与进度（bug#3）。
        */}
        <div
          className="app-view"
          hidden={nav !== "chat"}
          aria-hidden={nav !== "chat"}
        >
          <Chat
            isActive={nav === "chat"}
            onOpenSettings={openSettings}
          />
        </div>
        {mountedViews.has("automation") ? (
          <div
            className="app-view"
            hidden={nav !== "automation"}
            aria-hidden={nav !== "automation"}
          >
            <Automation />
          </div>
        ) : null}
        {mountedViews.has("knowledge") ? (
          <div
            className="app-view"
            hidden={nav !== "knowledge"}
            aria-hidden={nav !== "knowledge"}
          >
            <Knowledge isActive={nav === "knowledge"} />
          </div>
        ) : null}
      </AppShell>
      {settingsTab ? (
        <SettingsDialog
          activeTab={settingsTab}
          onTabChange={setSettingsTab}
          onClose={() => setSettingsTab(null)}
        />
      ) : null}
    </div>
  );
}

export default App;
