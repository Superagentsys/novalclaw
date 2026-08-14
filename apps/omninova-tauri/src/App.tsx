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
          style={{ display: nav === "chat" ? "flex" : "none" }}
        >
          <Chat
            isActive={nav === "chat"}
            onOpenSettings={openSettings}
          />
        </div>
        {nav === "automation" ? (
          <div className="app-view">
            <Automation />
          </div>
        ) : null}
        {nav === "knowledge" ? (
          <div className="app-view">
            <Knowledge />
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
