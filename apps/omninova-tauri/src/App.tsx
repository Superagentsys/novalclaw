import { useState, useEffect } from "react";
import "./App.css";
import { Chat } from "./components/Chat/Chat";
import { Setup } from "./components/Setup/Setup";
import { LoginDialog } from "./components/account/LoginDialog";
import { invoke } from "@tauri-apps/api/core";

type AppView = "setup" | "chat";

type AuthState =
  | { status: "checking" }
  | { status: "no-account" }
  | { status: "require-password" }
  | { status: "authenticated" };

function App() {
  const [view, setView] = useState<AppView>("setup");
  const [authState, setAuthState] = useState<AuthState>({ status: "checking" });

  // Check if password is required on startup
  useEffect(() => {
    const checkAuth = async () => {
      try {
        // Initialize account store
        await invoke("init_account_store");

        // Check if account exists
        const hasAccount = await invoke<boolean>("has_account");

        if (!hasAccount) {
          setAuthState({ status: "no-account" });
          return;
        }

        // Check if password is required on startup
        const requirePassword = await invoke<boolean>("get_require_password_on_startup");

        if (requirePassword) {
          setAuthState({ status: "require-password" });
        } else {
          setAuthState({ status: "authenticated" });
        }
      } catch (error) {
        console.error("Failed to check auth state:", error);
        // Default to no account on error
        setAuthState({ status: "no-account" });
      }
    };

    checkAuth();
  }, []);

  const handleLoginSuccess = () => {
    setAuthState({ status: "authenticated" });
  };

  // Show nothing while checking auth state
  if (authState.status === "checking") {
    return (
      <div className="flex items-center justify-center min-h-screen bg-background">
        <div className="animate-pulse text-muted-foreground">加载中...</div>
      </div>
    );
  }

  // Show login dialog if password is required
  if (authState.status === "require-password") {
    return <LoginDialog onLoginSuccess={handleLoginSuccess} />;
  }

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
