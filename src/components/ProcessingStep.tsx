import type { Stage, ProcessLog } from "../App";
import { useEffect, useRef } from "react";

interface Props {
  logs: ProcessLog[];
  currentStage: Stage;
  error: string;
  transcribeProgress: number;
  onRetry: () => void;
}

const STAGES: Stage[] = ["extracting", "transcribing", "formatting", "saving", "done"];

const STAGE_LABELS: Record<Stage, string> = {
  idle: "Idle",
  extracting: "Extracting audio",
  transcribing: "Transcribing",
  formatting: "Formatting",
  saving: "Saving files",
  done: "Done",
  error: "Error",
};

const STAGE_ICONS: Record<Stage, string> = {
  idle: "·",
  extracting: "🎛",
  transcribing: "✍️",
  formatting: "🤖",
  saving: "💾",
  done: "✅",
  error: "❌",
};

function stageIndex(s: Stage) {
  return STAGES.indexOf(s);
}

export default function ProcessingStep({ logs, currentStage, error, transcribeProgress, onRetry }: Props) {
  const logRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (logRef.current) {
      logRef.current.scrollTop = logRef.current.scrollHeight;
    }
  }, [logs]);

  const isError = currentStage === "error";
  const isDone = currentStage === "done";
  const currentIdx = stageIndex(currentStage);

  return (
    <div className="flex flex-col h-full px-8 py-6 gap-6">
      <h1 className="text-xl font-semibold">
        {isError ? "Something went wrong" : isDone ? "Complete" : "Processing…"}
      </h1>

      {/* Stage pipeline */}
      <div className="flex items-center gap-0 shrink-0">
        {STAGES.filter((s) => s !== "done").map((stage, i) => {
          const idx = stageIndex(stage);
          const done = isDone || currentIdx > idx;
          const active = !isDone && currentIdx === idx;

          return (
            <div key={stage} className="flex items-center">
              <div
                className={`flex items-center gap-2 px-3 py-1.5 rounded-lg text-sm transition-all
                  ${isError && active ? "bg-red-500/15 text-red-400"
                  : done ? "bg-green-500/10 text-green-400"
                  : active ? "bg-accent/15 text-accent"
                  : "bg-surface-secondary text-white/30"
                }`}
              >
                {active && !isError ? (
                  <span className="spinner" />
                ) : (
                  <span>{done ? "✓" : isError && active ? "✗" : String(i + 1)}</span>
                )}
                <span>{STAGE_LABELS[stage]}</span>
              </div>
              {i < STAGES.filter((s) => s !== "done").length - 1 && (
                <div
                  className={`h-px w-4 mx-1 ${
                    done || (isDone && i < STAGES.length - 1) ? "bg-green-500/40" : "bg-white/10"
                  }`}
                />
              )}
            </div>
          );
        })}
      </div>

      {/* Log panel */}
      <div
        ref={logRef}
        className="flex-1 bg-surface-secondary rounded-xl p-4 overflow-y-auto font-mono text-xs scrollable"
      >
        {logs.length === 0 && (
          <p className="text-white/20">Starting…</p>
        )}
        {logs.map((log, i) => (
          <div key={i} className="flex gap-3 mb-1">
            <span className="text-white/25 shrink-0 tabular-nums">
              {new Date(log.ts).toLocaleTimeString("en-US", { hour12: false })}
            </span>
            <span className="shrink-0">{STAGE_ICONS[log.stage]}</span>
            <span
              className={
                log.stage === "error"
                  ? "text-red-400"
                  : log.stage === "done"
                  ? "text-green-400"
                  : "text-white/70"
              }
            >
              {log.message}
            </span>
          </div>
        ))}
        {!isDone && !isError && (
          <div className="mt-2">
            <div className="flex gap-2 items-center text-white/30">
              <span className="spinner" />
              <span>
                {currentStage === "transcribing" && transcribeProgress > 0
                  ? `Transcribing… ${transcribeProgress}%`
                  : `${STAGE_LABELS[currentStage]}…`}
              </span>
            </div>
            {currentStage === "transcribing" && transcribeProgress > 0 && (
              <div className="mt-3 h-1.5 w-full bg-white/5 rounded-full overflow-hidden">
                <div
                  className="h-full bg-accent rounded-full transition-all duration-500 ease-out"
                  style={{ width: `${transcribeProgress}%` }}
                />
              </div>
            )}
          </div>
        )}
      </div>

      {/* Error state */}
      {isError && (
        <div className="shrink-0 flex flex-col gap-3">
          <div className="bg-red-500/10 border border-red-500/20 rounded-xl p-4">
            <p className="text-sm text-red-400 font-mono whitespace-pre-wrap">{error}</p>
          </div>
          <button
            onClick={onRetry}
            className="self-start px-5 py-2 rounded-lg bg-surface-secondary hover:bg-surface-tertiary text-sm font-medium transition-colors"
          >
            ← Try again
          </button>
        </div>
      )}
    </div>
  );
}
