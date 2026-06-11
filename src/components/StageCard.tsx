import type { StageKind, StageStatus } from "../types/storage";
import { Button, Icon, type IconName } from "./ui";

interface Props {
  kind: StageKind;
  status: StageStatus;
  error?: string | null;
  subtitle?: string;
  progress?: number;
  /** If provided and the stage is completed, render a Redo button that
   *  fires this callback. The parent is responsible for resetting +
   *  re-running the pipeline. */
  onRedo?: () => void;
  /** Disable the Redo affordance — e.g. while a run is in flight. */
  redoDisabled?: boolean;
}

const LABELS: Record<StageKind, string> = {
  extract: "Extract audio",
  transcribe: "Transcribe",
  format: "Format with LLM",
};

const ICONS: Record<StageKind, IconName> = {
  extract: "equalizer",
  transcribe: "doc-text",
  format: "sparkles",
};

export default function StageCard({ kind, status, error, subtitle, progress, onRedo, redoDisabled }: Props) {
  const tone =
    status === "completed" ? "border-status-success/30 bg-status-success/10"
    : status === "running" ? "border-accent/40 bg-accent/10"
    : status === "failed" ? "border-status-error/30 bg-status-error/10"
    : "border-separator bg-fill-quaternary";

  const statusText =
    status === "completed" ? "Completed"
    : status === "running" ? "Running…"
    : status === "failed" ? "Failed"
    : "Pending";

  return (
    <div className={`rounded-card border p-4 flex flex-col gap-2 ${tone}`}>
      <div className="flex items-center gap-3">
        <Icon name={ICONS[kind]} size="md" className="text-label-secondary" />
        <div className="flex-1">
          <div className="text-body font-medium">{LABELS[kind]}</div>
          {subtitle && <div className="text-footnote text-label-secondary">{subtitle}</div>}
        </div>
        <div className={`text-footnote ${
          status === "failed" ? "text-status-error"
          : status === "completed" ? "text-status-success"
          : status === "running" ? "text-accent"
          : "text-label-tertiary"
        }`}>{statusText}</div>
        {onRedo && status === "completed" && (
          <Button
            variant="ghost"
            size="sm"
            disabled={redoDisabled}
            onClick={onRedo}
            title={`Re-run ${LABELS[kind]} (and discard downstream output)`}
            className="shrink-0"
          >
            Redo
          </Button>
        )}
      </div>
      {status === "running" && typeof progress === "number" && progress > 0 && (
        <div className="h-1.5 w-full bg-fill-tertiary rounded-full overflow-hidden">
          <div className="h-full bg-accent rounded-full transition-all"
               style={{ width: `${Math.min(100, Math.max(0, progress))}%` }} />
        </div>
      )}
      {status === "failed" && error && (
        <pre className="text-footnote text-status-error whitespace-pre-wrap font-mono">{error}</pre>
      )}
    </div>
  );
}
