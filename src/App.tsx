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
import type { Session, SessionStatus } from "./components/SessionList";
import type { ChatMessage } from "./types/chat";

export type View =
  | "new-agent"
  | "chat"
  | "memory"
  | "skills"
  | "library"
  | "projects"
  | "connectors";

const SESSIONS: Session[] = [
  { id: "s1", title: "Refactor auth flow", status: "live" satisfies SessionStatus },
  { id: "s2", title: "Investigate tauri build", status: "inactive" satisfies SessionStatus },
  { id: "s3", title: "Draft API spec", status: "inactive" satisfies SessionStatus },
];

const SAMPLE_REPLY = `I'll take a look and walk you through what I find.

<h3>Plan</h3>
1. Map the current call graph for the auth module.
2. Spot the worst coupling and isolate it.
3. Propose a clean refactor with <code>tests</code> first.

- Draft an outline in @docs
- Open the build logs in @chrome
- Check the failing test on @github

Pulling the latest run now via @gmail and @meet for context.`;

const STREAM_CHUNK = 8;
const STREAM_TICK_MS = 28;

let mid = 1000;

function App() {
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [view, setView] = useState<View>("new-agent");
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [chatMsgs, setChatMsgs] = useState<ChatMessage[]>([]);
  const [streamId, setStreamId] = useState<string | null>(null);
  const [streamText, setStreamText] = useState("");
  const [settingsOpen, setSettingsOpen] = useState(false);

  const toggleSidebar = () => setSidebarOpen((open) => !open);
  const goToNewAgent = () => {
    setView("new-agent");
    setActiveSessionId(null);
  };
  const openSession = (sessionId: string) => {
    setActiveSessionId(sessionId);
    setView("chat");
  };
  const navigate = (next: View) => setView(next);

  const startStream = (parentId: string | null) => {
    const newId = `a-${++mid}`;
    setChatMsgs((prev) => {
      if (parentId === null) {
        return [...prev, { id: newId, role: "agent", content: "" }];
      }
      const idx = prev.findIndex((m) => m.id === parentId);
      if (idx === -1) return prev;
      const head = prev.slice(0, idx + 1);
      return [...head, { id: newId, role: "agent", content: "" }];
    });
    setStreamText("");
    setStreamId(newId);
  };

  const send = (text: string) => {
    const trimmed = text.trim();
    if (trimmed.length === 0) return;
    const userMsg: ChatMessage = {
      id: `u-${++mid}`,
      role: "user",
      content: trimmed,
      timestamp: Date.now(),
    };
    setChatMsgs((prev) => [...prev, userMsg]);
    setStreamId(null);
    if (view !== "chat") setView("chat");
    startStream(userMsg.id);
  };

  const retry = (userMsgId: string) => {
    const idx = chatMsgs.findIndex((m) => m.id === userMsgId);
    if (idx === -1) return;
    setChatMsgs((prev) => prev.slice(0, idx + 1));
    setStreamId(null);
    startStream(userMsgId);
  };

  useEffect(() => {
    if (streamId === null) return;
    if (streamText.length >= SAMPLE_REPLY.length) {
      const finalText = SAMPLE_REPLY;
      const id = streamId;
      setChatMsgs((prev) =>
        prev.map((m) => (m.id === id ? { ...m, content: finalText } : m))
      );
      setStreamId(null);
      return;
    }
    const t = setTimeout(() => {
      const next = SAMPLE_REPLY.slice(0, streamText.length + STREAM_CHUNK);
      setStreamText(next);
      const id = streamId;
      setChatMsgs((prev) => prev.map((m) => (m.id === id ? { ...m, content: next } : m)));
    }, STREAM_TICK_MS);
    return () => clearTimeout(t);
  }, [streamId, streamText]);

  const activeSession =
    activeSessionId !== null
      ? SESSIONS.find((session) => session.id === activeSessionId)
      : undefined;
  const chatTitle = view === "chat" ? activeSession?.title ?? "New chat" : null;

  return (
    <div className="h-screen w-screen bg-bg-primary">
      <div className="flex h-full w-full overflow-hidden bg-bg-primary">
        <Sidebar
          onToggle={toggleSidebar}
          open={sidebarOpen}
          onNewAgent={goToNewAgent}
          onSelectSession={openSession}
          onNavigate={navigate}
          activeView={view}
          activeSessionId={activeSessionId}
          sessions={SESSIONS}
        />
        <div className="flex min-w-0 min-h-0 flex-1 flex-col">
          <Toolbar
            onToggleSidebar={toggleSidebar}
            sidebarOpen={sidebarOpen}
            chatTitle={chatTitle}
            onSettings={() => setSettingsOpen(true)}
          />
          <div className="min-h-0 min-w-0 flex-1 overflow-hidden">
            {view === "new-agent" && <NewAgentPage onSend={send} />}
            {view === "chat" && (
              <ChatDetailPage
                messages={chatMsgs}
                onSend={send}
                onRetry={retry}
              />
            )}
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
