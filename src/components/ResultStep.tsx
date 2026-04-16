import { useState } from "react";
import { open } from "@tauri-apps/plugin-shell";

interface Props {
  formattedOutput: string;
  transcript: string;
  savedPaths: string[];
  onReset: () => void;
}

type Tab = "output" | "transcript";

export default function ResultStep({ formattedOutput, transcript, savedPaths, onReset }: Props) {
  const [tab, setTab] = useState<Tab>("output");
  const [copied, setCopied] = useState(false);

  // Reveal the file's containing folder in Finder
  const openInFinder = async (filePath: string) => {
    try {
      const dir = filePath.substring(0, filePath.lastIndexOf("/"));
      await open(dir);
    } catch {
      // ignore — best effort
    }
  };

  const copyToClipboard = async () => {
    await navigator.clipboard.writeText(
      tab === "output" ? formattedOutput : transcript
    );
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const content = tab === "output" ? formattedOutput : transcript;

  return (
    <div className="flex flex-col h-full px-8 py-6 gap-4">
      <div className="flex items-center justify-between shrink-0">
        <h1 className="text-xl font-semibold">Results</h1>
        <button
          onClick={onReset}
          className="px-4 py-1.5 rounded-lg bg-surface-secondary hover:bg-surface-tertiary text-sm transition-colors"
        >
          + New recording
        </button>
      </div>

      {/* Saved files */}
      {savedPaths.length > 0 && (
        <div className="shrink-0 bg-green-500/10 border border-green-500/20 rounded-xl p-4 flex flex-col gap-2">
          <p className="text-sm font-medium text-green-400">
            {savedPaths.length} file{savedPaths.length !== 1 ? "s" : ""} saved
          </p>
          {savedPaths.map((p, i) => (
            <div key={i} className="flex items-center gap-2">
              <span className="text-xs font-mono text-white/50 truncate flex-1">{p}</span>
              <button
                onClick={() => openInFinder(p)}
                className="text-xs text-accent hover:text-accent-hover shrink-0"
              >
                Show in Finder
              </button>
            </div>
          ))}
        </div>
      )}

      {/* Tab switcher */}
      <div className="flex gap-1 bg-surface-secondary rounded-lg p-1 w-fit shrink-0">
        {(["output", "transcript"] as Tab[]).map((t) => (
          <button
            key={t}
            onClick={() => setTab(t)}
            className={`px-4 py-1.5 rounded-md text-sm font-medium transition-colors capitalize
              ${tab === t ? "bg-surface text-white" : "text-white/40 hover:text-white/60"}`}
          >
            {t === "output" ? "Formatted output" : "Raw transcript"}
          </button>
        ))}
      </div>

      {/* Content */}
      <div className="flex-1 relative min-h-0">
        <textarea
          readOnly
          className="w-full h-full bg-surface-secondary rounded-xl p-4 text-sm font-mono text-white/80 resize-none outline-none scrollable"
          value={content}
        />
        <button
          onClick={copyToClipboard}
          className="absolute top-3 right-3 px-3 py-1 rounded-md bg-surface text-xs text-white/50 hover:text-white/80 transition-colors"
        >
          {copied ? "Copied!" : "Copy"}
        </button>
      </div>

      {/* Word count */}
      <p className="text-xs text-white/25 shrink-0">
        {content.split(/\s+/).filter(Boolean).length} words ·{" "}
        {content.length} characters
      </p>
    </div>
  );
}
