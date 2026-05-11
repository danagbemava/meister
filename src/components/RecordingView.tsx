import { forwardRef, useCallback, useEffect, useImperativeHandle, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open, confirm as dialogConfirm } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import StageCard from "./StageCard";
import OutputPreview from "./OutputPreview";
import AskPanel from "./AskPanel";
import { Button, Icon, Input, Toolbar } from "./ui";
import { api } from "../api/storage";
import {
  overallStatus,
  type Recording,
  type StageRecord,
  type StageKind,
} from "../types/storage";
import { exportRecording } from "../lib/exportRecording";
import { useGlobalSettings } from "../lib/useGlobalSettings";

export interface RecordingViewHandle {
  runPipeline: () => void;
  exportFile: () => void;
}

interface Props {
  recordingId: string;
  onRecordingChanged: () => void;
  onRecordingDeleted: () => void;
}

const STAGE_ORDER: StageKind[] = ["extract", "transcribe", "format"];

const RecordingView = forwardRef<RecordingViewHandle, Props>(function RecordingView(
  { recordingId, onRecordingChanged, onRecordingDeleted },
  ref,
) {
  const [rec, setRec] = useState<Recording | null>(null);
  const [running, setRunning] = useState(false);
  const [transcribePct, setTranscribePct] = useState(0);
  type FormatPhase = "single" | "map" | "reduce";
  type FormatProgressPayload = {
    phase: FormatPhase;
    chunk: { index: number; total: number; completed: number } | null;
    chars: number;
    done: boolean;
  };
  const [formatProgress, setFormatProgress] = useState<FormatProgressPayload | null>(null);
  const [missingByPath, setMissingByPath] = useState<Record<string, boolean>>({});
  const [renamingRec, setRenamingRec] = useState(false);
  const [draftRecName, setDraftRecName] = useState("");
  const [askOpen, setAskOpen] = useState(false);
  const [highlightRange, setHighlightRange] = useState<
    { start: number; end: number; nonce: number } | undefined
  >(undefined);

  const { settings: globalSettings } = useGlobalSettings();

  useEffect(() => {
    if (!rec) return;
    let cancelled = false;
    (async () => {
      const entries: [string, boolean][] = [];
      for (const p of rec.sourcePaths) {
        try {
          const ok = await invoke<boolean>("path_exists", { path: p });
          entries.push([p, !ok]);
        } catch {
          entries.push([p, false]);
        }
      }
      if (!cancelled) {
        setMissingByPath(Object.fromEntries(entries));
      }
    })();
    return () => { cancelled = true; };
  }, [rec?.id, rec?.sourcePaths?.join("|")]);

  const locateSource = async (idx: number) => {
    if (!rec) return;
    const path = await open({ multiple: false });
    if (typeof path !== "string") return;
    const nextPaths = [...rec.sourcePaths];
    nextPaths[idx] = path;
    const next = { ...rec, sourcePaths: nextPaths };
    setRec(next);
    await api.updateRecording(next);
    onRecordingChanged();
  };

  const reload = useCallback(async () => {
    const r = await api.getRecording(recordingId);
    setRec(r);
  }, [recordingId]);

  useEffect(() => { reload(); }, [reload]);

  useEffect(() => {
    let unlisten1: UnlistenFn | undefined;
    let unlisten2: UnlistenFn | undefined;
    listen<{ percent: number }>("transcribe-progress", (e) =>
      setTranscribePct(e.payload.percent)
    ).then((fn) => { unlisten1 = fn; });
    listen<FormatProgressPayload>("format-progress", (e) =>
      setFormatProgress(e.payload)
    ).then((fn) => { unlisten2 = fn; });
    return () => { unlisten1?.(); unlisten2?.(); };
  }, []);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    listen<{ recordingId: string; stage: string; status: string }>(
      "stage-changed",
      (e) => {
        if (e.payload.recordingId === recordingId) reload();
      },
    ).then((fn) => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, [recordingId, reload]);

  const runRef = useRef<() => void>(() => {});
  const exportFlowRef = useRef<() => void>(() => {});

  useImperativeHandle(ref, () => ({
    runPipeline: () => { runRef.current(); },
    exportFile: () => { exportFlowRef.current(); },
  }));

  const hasTranscript =
    rec?.stages.find((s) => s.stage === "transcribe")?.status === "completed" &&
    (rec?.stages.find((s) => s.stage === "transcribe")?.output?.length ?? 0) > 0;

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey && e.key.toLowerCase() === "k") {
        if (!hasTranscript) return;
        e.preventDefault();
        setAskOpen((v) => !v);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [hasTranscript]);

  useEffect(() => {
    setHighlightRange(undefined);
  }, [recordingId]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey && e.key.toLowerCase() === "e") {
        e.preventDefault();
        exportFlowRef.current();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  if (!rec) return <div className="p-8 text-label-tertiary">Loading…</div>;

  const stageOf = (k: StageKind): StageRecord | undefined =>
    rec.stages.find((s) => s.stage === k);
  const transcript = stageOf("transcribe")?.output ?? "";
  const formatted = stageOf("format")?.output ?? "";
  const anyFailed = rec.stages.some((s) => s.status === "failed");
  const allCompleted = rec.stages.every((s) => s.status === "completed");
  const backendRunning = rec.stages.some((s) => s.status === "running");
  const canRun = !running && !backendRunning && !allCompleted; // re-run happens via Start over + Run

  const exportFlow = async () => {
    if (!rec || !globalSettings) return;
    const fmtOutput = rec.stages.find((s) => s.stage === "format")?.output ?? "";
    if (!fmtOutput) return;
    await exportRecording({
      formatted: fmtOutput,
      recordingName: rec.name,
      defaultFormat: globalSettings.defaultExportFormat,
    });
  };

  exportFlowRef.current = () => { void exportFlow(); };

  const run = async () => {
    setRunning(true);
    setTranscribePct(0);
    setFormatProgress(null);
    try {
      await api.runRecording(recordingId);
      await reload();
    } catch (err) {
      console.error("run_recording failed:", err);
      await reload();
    } finally {
      setRunning(false);
      onRecordingChanged();
    }
  };

  runRef.current = () => {
    if (canRun) run();
  };

  const startOver = async () => {
    const ok = await dialogConfirm(
      "Start over — discard all progress for this recording?",
      { title: "Start over", kind: "warning" },
    );
    if (!ok) return;
    await api.startOverRecording(recordingId);
    await reload();
    onRecordingChanged();
  };

  const deleteMe = async () => {
    const ok = await dialogConfirm(
      `Delete recording '${rec.name}'?`,
      { title: "Delete recording", kind: "warning" },
    );
    if (!ok) return;
    await api.deleteRecording(recordingId);
    onRecordingDeleted();
  };

  return (
    <div className="h-full flex flex-col">
      <Toolbar data-tauri-drag-region>
        {renamingRec ? (
          <Input
            autoFocus
            size="sm"
            className="font-semibold flex-1 mr-2"
            value={draftRecName}
            onChange={(e) => setDraftRecName(e.target.value)}
            onBlur={async () => {
              const name = draftRecName.trim() || rec.name;
              setRenamingRec(false);
              if (name !== rec.name) {
                const next = { ...rec, name };
                setRec(next);
                await api.updateRecording(next);
                onRecordingChanged();
              }
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") (e.target as HTMLInputElement).blur();
              if (e.key === "Escape") { setDraftRecName(rec.name); setRenamingRec(false); }
            }}
          />
        ) : (
          <h1
            onClick={() => { setDraftRecName(rec.name); setRenamingRec(true); }}
            className="text-body font-semibold cursor-text hover:text-accent truncate"
            title="Click to rename"
          >
            {rec.name}
          </h1>
        )}
        <Toolbar.Spacer />
        <span className="text-footnote text-label-tertiary">{overallStatus(rec)}</span>
        <Button variant="ghost" size="sm" onClick={deleteMe} className="text-status-error">
          Delete
        </Button>
      </Toolbar>
      <div className="flex-1 overflow-y-auto scrollable p-8 flex flex-col gap-6">
        <div className="flex flex-col gap-1">
          <span className="text-footnote font-medium text-label-secondary uppercase tracking-wider">
            {rec.sourcePaths.length > 1 ? "Sources" : "Source"}
          </span>
          <ul className="flex flex-col gap-1">
            {rec.sourcePaths.map((p, idx) => {
              const missing = !!missingByPath[p];
              return (
                <li
                  key={`${p}-${idx}`}
                  className={`flex items-center gap-3 px-3 py-1.5 rounded text-footnote font-mono
                    ${missing
                      ? "bg-status-error/10 border border-status-error/20"
                      : "bg-control-background"}`}
                >
                  <span className="text-label-tertiary tabular-nums w-5 shrink-0">
                    {rec.sourcePaths.length > 1 ? `${idx + 1}.` : ""}
                  </span>
                  <span className={`flex-1 truncate ${missing ? "text-status-error" : "text-label-secondary"}`} title={p}>
                    {p}
                  </span>
                  {missing && (
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => locateSource(idx)}
                      className="text-accent shrink-0"
                    >
                      Locate…
                    </Button>
                  )}
                </li>
              );
            })}
          </ul>
        </div>

        <div className="flex flex-col gap-2">
          {STAGE_ORDER.map((k) => {
            const s = stageOf(k)!;
            const subtitle =
              k === "transcribe" ? `model: ${globalSettings?.whisperModel ?? "?"}`
              : k === "format" ? `model: ${globalSettings?.model ?? "?"}`
              : undefined;
            const progress =
              s.status === "running" && k === "transcribe" ? transcribePct
              : undefined;
            return (
              <StageCard
                key={k}
                kind={k}
                status={s.status}
                error={s.error}
                subtitle={subtitle}
                progress={progress}
              />
            );
          })}
        </div>

        <OutputPreview
          transcript={transcript}
          formatted={formatted}
          highlightRange={highlightRange}
        />
        {askOpen && rec && (
          <AskPanel
            recordingId={rec.id}
            disabled={!hasTranscript}
            onCitationClick={(c) => {
              if (c.start === null || c.end === null) return;
              setHighlightRange({ start: c.start, end: c.end, nonce: Date.now() });
            }}
            onClose={() => setAskOpen(false)}
          />
        )}

        {rec.stages.find((s) => s.stage === "format")?.status === "running" && formatProgress && (
          <div className="text-footnote text-label-tertiary space-y-1">
            {formatProgress.phase === "single" && (
              <p>Formatting… {formatProgress.chars.toLocaleString()} chars</p>
            )}
            {formatProgress.phase === "map" && formatProgress.chunk && (
              <>
                <p>
                  Analyzing chunk {Math.min(formatProgress.chunk.completed + 1, formatProgress.chunk.total)}/
                  {formatProgress.chunk.total}…
                  {formatProgress.chars > 0 && (
                    <span className="text-label-tertiary"> · {formatProgress.chars.toLocaleString()} chars streamed</span>
                  )}
                </p>
                <div className="h-1 w-full bg-fill-tertiary rounded">
                  <div
                    className="h-1 bg-label-tertiary rounded"
                    style={{
                      width: `${(formatProgress.chunk.completed / formatProgress.chunk.total) * 100}%`,
                    }}
                  />
                </div>
              </>
            )}
            {formatProgress.phase === "reduce" && (
              <p>Composing output… {formatProgress.chars.toLocaleString()} chars</p>
            )}
          </div>
        )}
      </div>

      <div className="shrink-0 border-t border-separator p-4 flex items-center justify-between bg-window-background">
        <div className="flex items-center gap-2">
          <Button
            variant="ghost"
            size="md"
            onClick={startOver}
          >
            Start over
          </Button>
          <Button
            variant="secondary"
            size="md"
            disabled={!formatted}
            title="Export the formatted output to a file (⌘E)"
            onClick={() => void exportFlow()}
          >
            Export…
          </Button>
          <Button
            variant="secondary"
            size="md"
            disabled={!hasTranscript}
            title={hasTranscript ? "Ask about this transcript (⌘K)" : "Transcribe first"}
            onClick={() => setAskOpen((v) => !v)}
          >
            <Icon name="chat" size="md" /> Ask
          </Button>
        </div>
        {(running || backendRunning) ? (
          <Button
            variant="destructive"
            size="lg"
            onClick={async () => {
              try {
                await invoke("cancel_recording", { recordingId });
              } catch (e) {
                console.warn("cancel failed", e);
              }
            }}
          >
            Stop
          </Button>
        ) : (
          <Button
            variant="primary"
            size="lg"
            disabled={!canRun}
            onClick={run}
          >
            {anyFailed ? "Resume" : "Run"}
          </Button>
        )}
      </div>
    </div>
  );
});

export default RecordingView;
