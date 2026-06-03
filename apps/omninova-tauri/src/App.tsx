import { useState } from "react";
import "./App.css";
import { AppShell, type AppNavId } from "./components/AppShell";
import { Chat } from "./components/Chat/Chat";
import { KnowledgeBase } from "./components/Knowledge/KnowledgeBase";
import { Employees } from "./components/Employees/Employees";
import { Setup } from "./components/Setup/Setup";

function App() {
  const [nav, setNav] = useState<AppNavId>("chat");
  const [employeeContext, setEmployeeContext] = useState<{ id: string; name: string } | null>(
    null
  );

  return (
    <div className="app-root">
      <AppShell activeNav={nav} onNavigate={setNav}>
        {nav === "chat" || nav === "cron" ? (
          <Chat
            initialSidebarTab={nav === "cron" ? "scheduled" : "avatars"}
            employeeContext={employeeContext}
          />
        ) : nav === "knowledge" ? (
          <KnowledgeBase />
        ) : nav === "employees" ? (
          <Employees
            onOpenSession={(emp) => {
              setEmployeeContext({ ...emp });
              setNav("chat");
            }}
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
