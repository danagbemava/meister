import type { StageKind, StageStatus } from "../types/storage";

interface Props {
  kind: StageKind;
  status: StageStatus;
  error?: string | null;
  subtitle?: string;
  progress?: number;
}

const LABELS: Record<StageKind, string> = {
  extract: "Extract audio",
  transcribe: "Transcribe",
  format: "Format with LLM",
  save: "Save outputs",
};

const ICONS: Record<StageKind, string> = {
  extract: "🎛",
  transcribe: "✍️",
  format: "🤖",
  save: "💾",
};

export default function StageCard({ kind, status, error, subtitle, progress }: Props) {
  const tone =
    status === "completed" ? "border-green-500/30 bg-green-500/5"
    : status === "running" ? "border-accent/40 bg-accent/10"
    : status === "failed" ? "border-red-500/30 bg-red-500/10"
    : "border-white/10 bg-surface-secondary";

  const statusText =
    status === "completed" ? "Completed"
    : status === "running" ? "Running…"
    : status === "failed" ? "Failed"
    : "Pending";

  return (
    <div className={`rounded-xl border p-4 flex flex-col gap-2 ${tone}`}>
      <div className="flex items-center gap-3">
        <span className="text-xl">{ICONS[kind]}</span>
        <div className="flex-1">
          <div className="text-sm font-medium">{LABELS[kind]}</div>
          {subtitle && <div className="text-xs text-white/40">{subtitle}</div>}
        </div>
        <div className={`text-xs ${
          status === "failed" ? "text-red-400"
          : status === "completed" ? "text-green-400"
          : status === "running" ? "text-accent"
          : "text-white/30"
        }`}>{statusText}</div>
      </div>
      {status === "running" && typeof progress === "number" && progress > 0 && (
        <div className="h-1.5 w-full bg-white/5 rounded-full overflow-hidden">
          <div className="h-full bg-accent rounded-full transition-all"
               style={{ width: `${Math.min(100, Math.max(0, progress))}%` }} />
        </div>
      )}
      {status === "failed" && error && (
        <pre className="text-xs text-red-400 whitespace-pre-wrap font-mono">{error}</pre>
      )}
    </div>
  );
}
