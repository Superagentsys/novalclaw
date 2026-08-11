import { useState } from "react";
import "./App.css";
import "./ui-refresh.css";
import { AppShell, type AppNavId } from "./components/AppShell";
import { Chat } from "./components/Chat/Chat";
import { Setup } from "./components/Setup/Setup";

function App() {
  const [nav, setNav] = useState<AppNavId>("chat");

  return (
    <div className="app-root">
      <AppShell activeNav={nav} onNavigate={setNav}>
        {/*
          Chat 始终挂载，仅用显隐切换：切到设置页时不卸载，
          从而保留正在执行的任务、草稿与进度（bug#3）。
        */}
        <div
          className="app-view"
          style={{ display: nav === "chat" ? "flex" : "none" }}
        >
          <Chat
            isActive={nav === "chat"}
            onOpenSettings={(target) => setNav(target)}
          />
        </div>
        {nav !== "chat" ? (
          <Setup
            embedded
            activeTab={nav}
            onTabChange={(tab) => setNav(tab)}
            onConfigSuccess={() => setNav("chat")}
          />
        ) : null}
      </AppShell>
    </div>
  );
}

export default App;
