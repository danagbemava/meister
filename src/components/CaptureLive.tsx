import { confirm as dialogConfirm } from "@tauri-apps/plugin-dialog";
import { Button } from "./ui";
import RecordingWaveform from "./RecordingWaveform";

interface Props {
  live: boolean;
  elapsedMs: number;
  meters: { label: string; samples: Float32Array }[];
  warning?: string;
  onStop: () => void;
  onPause: () => void;
  onResume: () => void;
  onDiscard: () => void;
}

export function formatElapsed(ms: number): string {
  const total = Math.floor(ms / 1000);
  const hh = Math.floor(total / 3600);
  const mm = Math.floor((total % 3600) / 60);
  const ss = total % 60;
  const pad = (n: number) => n.toString().padStart(2, "0");
  return hh > 0 ? `${pad(hh)}:${pad(mm)}:${pad(ss)}` : `${pad(mm)}:${pad(ss)}`;
}

export default function CaptureLive({ live, elapsedMs, meters, warning, onStop, onPause, onResume, onDiscard }: Props) {
  const state = live ? "recording" : "paused";

  const discard = async () => {
    if (elapsedMs > 5000) {
      const ok = await dialogConfirm("Discard this recording? The audio will be deleted.", {
        title: "Discard recording",
        kind: "warning",
      });
      if (!ok) return;
    }
    onDiscard();
  };

  return (
    <div
      role="region"
      aria-label="Recording in progress"
      className="rounded-card border border-accent/40 bg-control-background shadow-card p-5 flex flex-col gap-4"
    >
      <div className="flex items-center gap-3">
        <span
          className={`inline-flex items-center gap-2 rounded-full px-2.5 py-1 text-footnote font-medium ${
            live ? "bg-status-error/15 text-status-error" : "bg-status-warning/15 text-status-warning"
          }`}
        >
          <span className={`w-2 h-2 rounded-full ${live ? "bg-status-error animate-pulse" : "bg-status-warning"}`} />
          {live ? "Recording" : "Paused"}
        </span>
        <span className="ml-auto font-display text-[34px] leading-none tabular-nums text-label-primary">
          {formatElapsed(elapsedMs)}
        </span>
      </div>

      <div className="grid gap-2">
        {meters.map((m) => (
          <div key={m.label} className="grid grid-cols-[8.5rem_1fr] items-center gap-3">
            <span className="text-footnote text-label-secondary">{m.label}</span>
            <RecordingWaveform samples={m.samples} state={state} className="h-8" />
          </div>
        ))}
      </div>

      {warning && <p className="text-footnote text-status-warning">{warning}</p>}

      <div className="flex items-center gap-2">
        <Button onClick={onStop} variant="primary" size="md" iconLeft="stop">
          Stop and save
        </Button>
        {live ? (
          <Button onClick={onPause} variant="secondary" size="md" iconLeft="pause">Pause</Button>
        ) : (
          <Button onClick={onResume} variant="secondary" size="md" iconLeft="play">Resume</Button>
        )}
        <Button
          onClick={() => void discard()}
          variant="ghost"
          size="md"
          iconLeft="trash"
          className="ml-auto text-label-secondary hover:text-status-error hover:bg-status-error/10"
        >
          Discard
        </Button>
      </div>
    </div>
  );
}
