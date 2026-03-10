import { useState } from "react";
import "./App.css";
import { Chat } from "./components/Chat/Chat";
import { Setup } from "./components/Setup/Setup";

type AppView = "setup" | "chat";

function App() {
  const [view, setView] = useState<AppView>("setup");

  return (
    <div className={`app-shell ${view === "chat" ? "app-shell--chat" : ""}`}>
      {view === "setup" ? (
        <Setup
          onConfigSuccess={() => setView("chat")}
        />
      ) : (
        <Chat onBack={() => setView("setup")} />
      )}
    </div>
  );
}

export default App;
