// Memory, the graph, recall, and the learning preferences the loop records.
import { invoke } from "@tauri-apps/api/core";

export type Memory = {
  id: string;
  content: string;
  kind: string;
  importance: number;
  session_id: string | null;
  created_at: string;
  updated_at: string;
};

export type MemoryNode = {
  id: string;
  label: string;
  kind: string;
};

export type MemoryEdge = {
  from_id: string;
  to_id: string;
  relation: string;
};

export type MemoryGraph = {
  nodes: MemoryNode[];
  edges: MemoryEdge[];
};

export type RecallHit = {
  source: string;
  ref_id: string;
  session_id: string | null;
  snippet: string;
};

export function memoryList(kind?: string): Promise<Memory[]> {
  return invoke<Memory[]>("memory_list", { kind: kind ?? null });
}

export function memorySearch(query: string): Promise<Memory[]> {
  return invoke<Memory[]>("memory_search", { query });
}

export function memoryRecall(query: string): Promise<RecallHit[]> {
  return invoke<RecallHit[]>("memory_recall", { query });
}

export type PastSession = {
  session_id: string;
  title: string;
  updated_at: string;
  score: number;
  snippets: string[];
};

export function memoryRecallSessions(
  query: string,
  exclude?: string,
): Promise<PastSession[]> {
  return invoke<PastSession[]>("memory_recall_sessions", {
    query,
    exclude: exclude ?? null,
  });
}

export type TranscriptTurn = {
  seq: number;
  who: string;
  text: string;
};

export type Transcript = {
  session_id: string;
  title: string;
  turns: TranscriptTurn[];
  next_seq: number;
  more: boolean;
};

export function memoryReadSession(
  sessionId: string,
  afterSeq = 0,
): Promise<Transcript> {
  return invoke<Transcript>("memory_read_session", {
    sessionId,
    afterSeq,
  });
}

export function memoryGraph(): Promise<MemoryGraph> {
  return invoke<MemoryGraph>("memory_graph", {});
}

export function memoryDelete(id: string): Promise<void> {
  return invoke<void>("memory_delete", { id });
}

export type MemoryLink = {
  from_id: string;
  to_id: string;
  relation: string;
  created_at: string;
};

export function memoryLink(
  fromId: string,
  toId: string,
  relation?: string,
): Promise<MemoryLink> {
  return invoke<MemoryLink>("memory_link", {
    fromId,
    toId,
    relation: relation ?? null,
  });
}

export function memoryUnlink(fromId: string, toId: string): Promise<void> {
  return invoke<void>("memory_unlink", { fromId, toId });
}

export type LearningPreference = {
  id: string;
  scope: string;
  category: string;
  key: string;
  value: string;
  confidence: number;
  explicit: boolean;
  evidence_count: number;
  last_confirmed: string;
  created_at: string;
  updated_at: string;
};

const CMD_LEARN_LIST = "learning_list";
const CMD_LEARN_FORGET = "learning_forget";
const CMD_LEARN_CORR = "learning_record_correction";
const CMD_LEARN_FB = "learning_feedback";

export function learningList(): Promise<LearningPreference[]> {
  return invoke<LearningPreference[]>(CMD_LEARN_LIST);
}

export function learningForget(id: string): Promise<void> {
  return invoke<void>(CMD_LEARN_FORGET, { id });
}

export function learningRecordCorrection(
  sessionId: string,
  messageId: string,
  edited: string,
): Promise<void> {
  return invoke<void>(CMD_LEARN_CORR, { sessionId, messageId, edited });
}

export function learningFeedback(
  sessionId: string,
  messageId: string,
  feedback: string,
): Promise<void> {
  return invoke<void>(CMD_LEARN_FB, { sessionId, messageId, feedback });
}
