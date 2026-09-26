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
import SubagentPage from "./components/SubagentPage";
import { notifyDone } from "./lib/notify";
import { isWorking, sessionStore, useSessions } from "./stores/sessions";
import { toast } from "./stores/toast";

export type View =
  | "new-agent"
  | "chat"
  | "subagent"
  | "memory"
  | "skills"
  | "library"
  | "projects"
  | "connectors";

function App() {
  const st = useSessions();
  const { sessions, activeId, agentRuns } = st;
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [view, setView] = useState<View>("new-agent");
  const [subagent, setSubagent] = useState<{
    id: string;
    parent: string;
  } | null>(null);
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

    const unAgent = listen<{ id: string; name: string; state: string }>(
      "agent-done",
      () => {
        if (activeId !== null) {
          sessionStore.loadAgents(activeId);
        }

        sessionStore.loadSessions();
      },
    );

    const unJob = listen<{ label: string; state: string; exit: number | null }>(
      "job-done",
      (e) => {
        const { label, state } = e.payload;
        const ok = state === "done";

        void notifyDone(
          ok ? `Finished: ${label}` : `${label} — ${state}`,
          ok
            ? "The background job you started is done."
            : "The background job did not finish cleanly. Open the session to see why.",
        );
        sessionStore.loadSessions();
      },
    );

    return () => {
      void un.then((f) => f());
      void unJob.then((f) => f());
      void unAgent.then((f) => f());
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

  const openSubagent = (id: string) => {
    if (activeId === null) {
      return;
    }

    setSubagent({ id, parent: activeId });
    setView("subagent");
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
  const subagentRun = subagent !== null ? agentRuns[subagent.id] : undefined;
  const chatTitle =
    view === "subagent"
      ? (subagentRun?.title ?? subagentRun?.name ?? null)
      : view === "chat"
        ? (activeSession?.title ?? "New chat")
        : null;
  const backToChat = () => setView("chat");

  const exportSession = async (sessionId: string) => {
    try {
      const json = await sessExportJson(sessionId);

      try {
        await navigator.clipboard.writeText(json);
      } catch {}

      const title =
        sessions.find((s) => s.id === sessionId)?.title ?? "session";
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
      status: isWorking(st, s.id) ? "live" : "inactive",
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
            onBack={view === "subagent" ? backToChat : undefined}
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
            {view === "subagent" && subagent !== null && (
              <SubagentPage
                agentId={subagent.id}
                parentId={subagent.parent}
                onBack={backToChat}
              />
            )}
            {view === "chat" &&
              (activeId !== null ? (
                <ChatDetailPage
                  sessionId={activeId}
                  onOpenAgent={openSubagent}
                />
              ) : (
                <NewAgentPage
                  onSend={startNew}
                  initialPrompt={pendingPrompt}
                  onPromptUsed={() => setPendingPrompt("")}
                />
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
      <DocViewer />
      <Toasts />
    </div>
  );
}

export default App;
