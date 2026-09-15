import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";

import ChatDetailPage from "./components/ChatDetailPage";
import ConnectorsPage from "./components/ConnectorsPage";
import DocViewer from "./components/DocViewer";
import LibraryPage from "./components/LibraryPage";
import MemoryPage from "./components/MemoryPage";
import NewAgentPage from "./components/NewAgentPage";
import ProjectsPage from "./components/ProjectsPage";
import SettingsModal from "./components/SettingsModal";
import Sidebar from "./components/Sidebar";
import SkillsPage from "./components/SkillsPage";
import Toasts from "./components/Toasts";
import Toolbar from "./components/Toolbar";
import type { Session } from "./components/SessionList";
import { sessExportJson, type ChatModel } from "./lib/ipc";
import { notifyDone } from "./lib/notify";
import { sessionStore, useSessions } from "./stores/sessions";
import { toast } from "./stores/toast";

export type View =
  | "new-agent"
  | "chat"
  | "memory"
  | "skills"
  | "library"
  | "projects"
  | "connectors";

const SKILL_TEMPLATE = `@createskill `;

function App() {
  const { sessions, activeId, turns } = useSessions();
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [view, setView] = useState<View>("new-agent");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [pendingPrompt, setPendingPrompt] = useState("");
  const notified = useRef(new Set<string>());
  const viewRef = useRef(view);
  viewRef.current = view;
  const activeRef = useRef(activeId);
  activeRef.current = activeId;

  useEffect(() => {
    sessionStore.onTurnStart = (sessionId) => {
      notified.current.delete(sessionId);
    };
    sessionStore.onTurnDone = (sessionId, ok, snippet) => {
      if (notified.current.has(sessionId)) return;

      const watching =
        viewRef.current === "chat" && activeRef.current === sessionId;
      if (watching) return;

      notified.current.add(sessionId);
      const title =
        sessionStore.getState().sessions.find((s) => s.id === sessionId)
          ?.title ?? "Argus";
      const body = ok
        ? snippet || "Done."
        : `Needs attention: ${snippet || "something failed."}`;
      void notifyDone(title, body);
    };

    return () => {
      sessionStore.onTurnStart = null;
      sessionStore.onTurnDone = null;
    };
  }, []);

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

  const newSkillChat = () => {
    setPendingPrompt(SKILL_TEMPLATE);
    sessionStore.select(null);
    setView("new-agent");
  };

  const openSession = (sessionId: string) => {
    sessionStore.select(sessionId);
    setView("chat");
  };

  const startNew = async (
    text: string,
    model: ChatModel | null,
    permission: string,
    webSearch: boolean,
  ) => {
    const row = await sessionStore.create(
      "New chat",
      model?.modelId ?? null,
      permission,
      webSearch,
    );
    await sessionStore.select(row.id);
    setView("chat");
    sessionStore.send(row.id, text);
  };

  const activeSession =
    activeId !== null ? sessions.find((s) => s.id === activeId) : undefined;
  const chatTitle = view === "chat" ? activeSession?.title ?? "New chat" : null;

  const exportSession = async (sessionId: string) => {
    try {
      const json = await sessExportJson(sessionId);

      try {
        await navigator.clipboard.writeText(json);
      } catch {}

      const title = sessions.find((s) => s.id === sessionId)?.title ?? "session";
      const blob = new Blob([json], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `${title.replace(/[^\w-]+/g, "-") || "session"}.json`;
      a.click();
      URL.revokeObjectURL(url);
      toast.success("Chat exported");
    } catch {
      toast.error("Export failed");
    }
  };

  const deleteSession = async (id: string) => {
    try {
      await sessionStore.remove(id);
      toast.success("Chat deleted");
    } catch {
      toast.error("Delete failed");
    }
  };

  const archiveSession = async (id: string) => {
    try {
      await sessionStore.archive(id);
      toast.success("Chat archived");
    } catch {
      toast.error("Archive failed");
    }
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
          onArchive={(id) => archiveSession(id)}
          onExport={exportSession}
          onDelete={(id) => deleteSession(id)}
        />
        <div className="flex min-w-0 min-h-0 flex-1 flex-col">
          <Toolbar
            onToggleSidebar={toggleSidebar}
            sidebarOpen={sidebarOpen}
            chatTitle={chatTitle}
            onSettings={() => setSettingsOpen(true)}
          />
          <div className="min-h-0 min-w-0 flex-1 overflow-hidden">
            {view === "new-agent" && (
              <NewAgentPage
                onSend={startNew}
                initialPrompt={pendingPrompt}
                onPromptUsed={() => setPendingPrompt("")}
              />
            )}
            {view === "chat" &&
              (activeId !== null ? (
                <ChatDetailPage sessionId={activeId} />
              ) : (
                <NewAgentPage
                  onSend={startNew}
                  initialPrompt={pendingPrompt}
                  onPromptUsed={() => setPendingPrompt("")}
                />
              ))}
            {view === "memory" && <MemoryPage />}
            {view === "skills" && <SkillsPage onAddSkill={newSkillChat} />}
            {view === "library" && <LibraryPage />}
            {view === "projects" && <ProjectsPage />}
            {view === "connectors" && <ConnectorsPage />}
          </div>
        </div>
      </div>
      <SettingsModal open={settingsOpen} onClose={() => setSettingsOpen(false)} />
      <DocViewer />
      <Toasts />
    </div>
  );
}

export default App;
