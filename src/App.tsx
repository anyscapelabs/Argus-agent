import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import ChatDetailPage from "./components/ChatDetailPage";
import ConnectorsPage from "./components/ConnectorsPage";
import LibraryPage from "./components/LibraryPage";
import MemoryPage from "./components/MemoryPage";
import NewAgentPage from "./components/NewAgentPage";
import ProjectsPage from "./components/ProjectsPage";
import SettingsModal from "./components/SettingsModal";
import Sidebar from "./components/Sidebar";
import SkillsPage from "./components/SkillsPage";
import Toolbar from "./components/Toolbar";
import type { Session } from "./components/SessionList";
import { sessExportJson, type ChatModel } from "./lib/ipc";
import { sessionStore, useSessions } from "./stores/sessions";

export type View =
  | "new-agent"
  | "chat"
  | "memory"
  | "skills"
  | "library"
  | "projects"
  | "connectors";

function App() {
  const { sessions, activeId, turns } = useSessions();
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [view, setView] = useState<View>("new-agent");
  const [settingsOpen, setSettingsOpen] = useState(false);

  useEffect(() => {
    sessionStore.loadSessions();
    const un = listen("sessions-changed", () => {
      sessionStore.loadSessions();
    });
    return () => {
      void un.then((f) => f());
    };
  }, []);

  const toggleSidebar = () => setSidebarOpen((open) => !open);

  const goToNewAgent = () => {
    sessionStore.select(null);
    setView("new-agent");
  };

  const openSession = (sessionId: string) => {
    sessionStore.select(sessionId);
    setView("chat");
  };

  const startNew = async (text: string, model: ChatModel | null, permission: string) => {
    const row = await sessionStore.create("New chat", model?.modelId ?? null, permission);
    await sessionStore.select(row.id);
    setView("chat");
    sessionStore.send(row.id, text);
  };

  const activeSession = activeId !== null ? sessions.find((s) => s.id === activeId) : undefined;
  const chatTitle = view === "chat" ? activeSession?.title ?? "New chat" : null;

  const exportSession = async (sessionId: string) => {
    const json = await sessExportJson(sessionId);
    try {
      await navigator.clipboard.writeText(json);
    } catch {
      // clipboard can be blocked; the download below still runs
    }
    const title = sessions.find((s) => s.id === sessionId)?.title ?? "session";
    const blob = new Blob([json], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `${title.replace(/[^\w-]+/g, "-") || "session"}.json`;
    a.click();
    URL.revokeObjectURL(url);
  };

  const sidebarSessions: Session[] = sessions
    .filter((s) => s.status !== "archived")
    .map((s) => ({
      id: s.id,
      title: s.title,
      status: turns[s.id] !== undefined ? "live" : "inactive",
    }));

  return (
    <div className="h-screen w-screen bg-bg-primary">
      <div className="flex h-full w-full overflow-hidden bg-bg-primary">
        <Sidebar
          onToggle={toggleSidebar}
          open={sidebarOpen}
          onNewAgent={goToNewAgent}
          onSelectSession={openSession}
          onNavigate={setView}
          activeView={view}
          activeSessionId={activeId}
          sessions={sidebarSessions}
          onArchive={(id) => sessionStore.archive(id)}
          onExport={exportSession}
          onDelete={(id) => sessionStore.remove(id)}
        />
        <div className="flex min-w-0 min-h-0 flex-1 flex-col">
          <Toolbar
            onToggleSidebar={toggleSidebar}
            sidebarOpen={sidebarOpen}
            chatTitle={chatTitle}
            onSettings={() => setSettingsOpen(true)}
          />
          <div className="min-h-0 min-w-0 flex-1 overflow-hidden">
            {view === "new-agent" && <NewAgentPage onSend={startNew} />}
            {view === "chat" &&
              (activeId !== null ? (
                <ChatDetailPage sessionId={activeId} />
              ) : (
                <NewAgentPage onSend={startNew} />
              ))}
            {view === "memory" && <MemoryPage />}
            {view === "skills" && <SkillsPage />}
            {view === "library" && <LibraryPage />}
            {view === "projects" && <ProjectsPage />}
            {view === "connectors" && <ConnectorsPage />}
          </div>
        </div>
      </div>
      <SettingsModal
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
      />
    </div>
  );
}

export default App;
