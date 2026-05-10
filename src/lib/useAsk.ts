import { useEffect, useSyncExternalStore } from "react";
import { askApi } from "../api/ask";
import type { AskThread, AskTurn } from "../types/ask";

type Store = Map<string, AskThread[]>;

const store: Store = new Map();
const listeners = new Set<() => void>();
const EMPTY: AskThread[] = [];

function emit() {
  for (const l of listeners) l();
}

function getThreads(recordingId: string): AskThread[] {
  return store.get(recordingId) ?? EMPTY;
}

function setThreads(recordingId: string, threads: AskThread[]) {
  store.set(recordingId, threads);
  emit();
}

function uid(): string {
  return Math.random().toString(36).slice(2) + Date.now().toString(36);
}

function removeThread(recordingId: string, threadId: string) {
  const cur = getThreads(recordingId);
  setThreads(recordingId, cur.filter((t) => t.id !== threadId));
}

async function runQuery(
  recordingId: string,
  question: string,
  threadId: string | undefined,
) {
  const before = getThreads(recordingId);
  let target: AskThread;
  if (threadId) {
    const found = before.find((t) => t.id === threadId);
    if (!found) throw new Error(`thread not found: ${threadId}`);
    target = found;
  } else {
    target = { id: uid(), turns: [], pending: false };
  }

  // Optimistically add the user turn + mark pending.
  target = {
    ...target,
    pending: true,
    turns: [...target.turns, { role: "user", content: question }],
  };
  const next = threadId
    ? before.map((t) => (t.id === target.id ? target : t))
    : [...before, target];
  setThreads(recordingId, next);

  // History = all turns BEFORE this user turn.
  const history: AskTurn[] = target.turns.slice(0, -1);

  let assistant: AskTurn;
  try {
    const ans = await askApi.query(recordingId, question, history);
    assistant = { role: "assistant", content: ans.text, citations: ans.citations };
  } catch (e) {
    const msg = typeof e === "string" ? e : (e as Error)?.message ?? String(e);
    if (msg === "cancelled") {
      // User clicked Stop. Drop the partial turn; user's question remains.
      const after = getThreads(recordingId).map((t) =>
        t.id === target.id ? { ...t, pending: false, streamingText: undefined } : t,
      );
      setThreads(recordingId, after);
      return;
    }
    // Map known connection errors to a friendlier phrasing per spec.
    const friendly = msg.startsWith("Cannot reach Ollama")
      ? "Couldn't reach the model — is Ollama running?"
      : msg;
    assistant = { role: "assistant", content: `Error: ${friendly}` };
  }

  const after = getThreads(recordingId).map((t) =>
    t.id === target.id
      ? { ...t, pending: false, turns: [...t.turns, assistant!], streamingText: undefined }
      : t,
  );
  setThreads(recordingId, after);
}

export function useAsk(recordingId: string) {
  const threads = useSyncExternalStore(
    (cb) => {
      listeners.add(cb);
      return () => { listeners.delete(cb); };
    },
    () => getThreads(recordingId),
    () => getThreads(recordingId),
  );

  // Streaming progress: update the active thread's `streamingText` as
  // ask-progress events arrive. Cleanup on unmount.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    (async () => {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        if (cancelled) return;
        unlisten = await listen<
          | { phase: "search"; chunk: { index: number; total: number } }
          | { phase: "answer"; chars: number }
          | { phase: "done" }
        >("ask-progress", (e) => {
          if (cancelled) return;
          let streamingText: string | undefined;
          if (e.payload.phase === "search") {
            const { index, total } = (e.payload as { phase: "search"; chunk: { index: number; total: number } }).chunk;
            streamingText = `Searching transcript (${index + 1} / ${total})…`;
          } else if (e.payload.phase === "answer") {
            streamingText = `Answering… (${(e.payload as { phase: "answer"; chars: number }).chars} chars)`;
          } else {
            return; // "done" — nothing to render
          }
          const cur = getThreads(recordingId);
          const idx = cur.findIndex((t) => t.pending);
          if (idx === -1) return;
          const updated = { ...cur[idx], streamingText };
          setThreads(recordingId, [...cur.slice(0, idx), updated, ...cur.slice(idx + 1)]);
        });
      } catch {
        // Tauri not available (e.g. in tests or web fallback); streaming progress disabled.
      }
    })();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [recordingId]);

  return {
    threads,
    ask: (question: string, threadId?: string) => runQuery(recordingId, question, threadId),
    cancel: () => askApi.cancel(recordingId),
    clearAll: () => setThreads(recordingId, []),
    removeThread: (threadId: string) => removeThread(recordingId, threadId),
  };
}

/** @internal — for tests */
export function __resetAskStoreForTests() {
  store.clear();
  listeners.clear();
}
