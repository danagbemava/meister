import { useState } from "react";

interface Props {
  transcript?: string | null;
  formatted?: string | null;
  savedPaths?: string[] | null;
}

export default function OutputPreview({ transcript, formatted, savedPaths }: Props) {
  const [tab, setTab] = useState<"formatted" | "transcript">("formatted");
  const has = (v?: string | null) => (v ?? "").length > 0;

  if (!has(transcript) && !has(formatted)) return null;

  return (
    <div className="flex flex-col gap-3">
      <div className="flex gap-2 text-xs">
        <button
          onClick={() => setTab("formatted")}
          className={`px-3 py-1 rounded ${
            tab === "formatted" ? "bg-accent/10 text-accent" : "text-label-secondary hover:bg-fill-tertiary"
          }`}
        >
          Formatted
        </button>
        <button
          onClick={() => setTab("transcript")}
          className={`px-3 py-1 rounded ${
            tab === "transcript" ? "bg-accent/10 text-accent" : "text-label-secondary hover:bg-fill-tertiary"
          }`}
        >
          Transcript
        </button>
      </div>
      <pre className="bg-control-background border border-separator rounded-xl p-4 text-xs font-mono text-label-primary whitespace-pre-wrap max-h-96 overflow-y-auto scrollable">
        {tab === "formatted" ? (formatted ?? "") : (transcript ?? "")}
      </pre>
      {savedPaths && savedPaths.length > 0 && (
        <div className="text-xs text-label-secondary">
          Saved to:
          <ul className="mt-1">
            {savedPaths.map((p) => <li key={p} className="truncate">· {p}</li>)}
          </ul>
        </div>
      )}
    </div>
  );
}
