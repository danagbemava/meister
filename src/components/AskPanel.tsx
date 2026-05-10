import { useState } from "react";
import { Button, Input } from "./ui";
import { useAsk } from "../lib/useAsk";
import type { AskCitation } from "../types/ask";

interface Props {
  recordingId: string;
  disabled: boolean;
  onCitationClick: (c: AskCitation) => void;
}

export default function AskPanel({ recordingId, disabled, onCitationClick }: Props) {
  const { threads, ask, cancel, clearAll, removeThread } = useAsk(recordingId);
  const [input, setInput] = useState("");

  const trimmed = input.trim();
  const anyPending = threads.some((t) => t.pending);
  const canSend = !disabled && trimmed.length > 0 && !anyPending;

  const submit = async (threadId?: string) => {
    if (!canSend) return;
    const q = trimmed;
    setInput("");
    await ask(q, threadId);
  };

  return (
    <aside className="w-[360px] flex flex-col gap-3 border-l border-separator pl-4" aria-label="Ask AI">
      <header className="flex items-center justify-between">
        <h3 className="text-headline font-semibold">Ask</h3>
        {threads.length > 0 && (
          <Button variant="secondary" size="sm" onClick={clearAll}>
            Clear all
          </Button>
        )}
      </header>

      {threads.length === 0 && !disabled && (
        <p className="text-footnote text-label-secondary">
          Ask anything about this transcript — e.g. <em>what was said about project financials?</em>
        </p>
      )}

      <ol className="flex flex-col gap-4 max-h-[60vh] overflow-y-auto scrollable">
        {threads.map((thread) => (
          <li key={thread.id} className="flex flex-col gap-2 relative">
            {!thread.pending && (
              <button
                type="button"
                aria-label="Dismiss thread"
                onClick={() => removeThread(thread.id)}
                className="absolute top-0 right-0 text-label-tertiary hover:text-label-primary text-callout"
              >
                ×
              </button>
            )}
            {thread.turns.map((turn, idx) => (
              <div key={idx} className="flex flex-col gap-1">
                {turn.role === "user" ? (
                  <div className="text-callout text-label-secondary">
                    <span className="font-semibold">You:</span> {turn.content}
                  </div>
                ) : (
                  <div className="text-callout text-label-primary whitespace-pre-wrap">
                    {turn.content}
                    {turn.citations && turn.citations.length > 0 && (
                      <div className="mt-2 flex flex-col gap-1">
                        {turn.citations.map((c, i) => {
                          const matched = c.start !== null && c.end !== null;
                          if (matched) {
                            return (
                              <button
                                key={i}
                                type="button"
                                onClick={() => onCitationClick(c)}
                                className="text-left text-footnote bg-control-background border border-separator rounded-md px-2 py-1 hover:border-tint transition-colors"
                              >
                                &#x201C;{c.quote}&#x201D;
                              </button>
                            );
                          }
                          return (
                            <span
                              key={i}
                              title="couldn't locate in transcript"
                              className="text-left text-footnote bg-control-background border border-separator rounded-md px-2 py-1 text-label-tertiary cursor-help"
                            >
                              &#x201C;{c.quote}&#x201D;
                            </span>
                          );
                        })}
                      </div>
                    )}
                  </div>
                )}
              </div>
            ))}
            {thread.pending && (
              <div className="text-footnote text-label-secondary">
                {thread.streamingText ?? "Answering…"}
              </div>
            )}
            {!thread.pending && (
              <div>
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => submit(thread.id)}
                  disabled={!canSend}
                >
                  Follow up
                </Button>
              </div>
            )}
          </li>
        ))}
      </ol>

      <form
        className="mt-auto flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <Input
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder="ask the transcript…"
          disabled={disabled || anyPending}
          aria-label="Ask the transcript"
        />
        {anyPending ? (
          <Button type="button" variant="destructive" size="md" onClick={() => void cancel()}>
            Stop
          </Button>
        ) : (
          <Button type="submit" variant="primary" size="md" disabled={!canSend}>
            Send
          </Button>
        )}
      </form>
    </aside>
  );
}
