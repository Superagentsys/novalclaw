import { useState } from "react";
import "./App.css";
import { AppShell, type AppNavId } from "./components/AppShell";
import { Chat } from "./components/Chat/Chat";
import { Setup } from "./components/Setup/Setup";

function App() {
  const [nav, setNav] = useState<AppNavId>("chat");

  return (
    <div className="app-root">
      <AppShell activeNav={nav} onNavigate={setNav}>
        {nav === "chat" || nav === "cron" ? (
          <Chat
            initialSidebarTab={nav === "cron" ? "scheduled" : "avatars"}
          />
        ) : (
          <Setup
            embedded
            activeTab={nav}
            onTabChange={(tab) => setNav(tab)}
            onConfigSuccess={() => setNav("chat")}
          />
        )}
      </AppShell>
    </div>
  );
}

export default App;
