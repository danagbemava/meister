import { forwardRef, useCallback, useEffect, useImperativeHandle, useRef, useState } from "react";
import { subscribe } from "../lib/subscribe";
import { open, confirm as dialogConfirm } from "@tauri-apps/plugin-dialog";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
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
import { useScrollEdge } from "../lib/useScrollEdge";

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
  // null = adopt whatever the DB says. Becomes a string once the user
  // starts editing the formatted output.
  const [formattedDraft, setFormattedDraft] = useState<string | null>(null);
  const persistTimerRef = useRef<number | null>(null);

  const [expandedSources, setExpandedSources] = useState<Set<number>>(() => new Set());

  const toggleSourceExpanded = useCallback((idx: number) => {
    setExpandedSources((prev) => {
      const next = new Set(prev);
      if (next.has(idx)) next.delete(idx);
      else next.add(idx);
      return next;
    });
  }, []);

  const [playerErrors, setPlayerErrors] = useState<Record<number, string>>({});

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
    const offTranscribe = subscribe<{ percent: number }>("transcribe-progress", (e) =>
      setTranscribePct(e.payload.percent)
    );
    const offFormat = subscribe<FormatProgressPayload>("format-progress", (e) =>
      setFormatProgress(e.payload)
    );
    return () => { offTranscribe(); offFormat(); };
  }, []);

  useEffect(
    () =>
      subscribe<{ recordingId: string }>("stage-changed", (e) => {
        if (e.payload.recordingId === recordingId) reload();
      }),
    [recordingId, reload],
  );

  const runRef = useRef<() => void>(() => {});
  const exportFlowRef = useRef<() => void>(() => {});
  const scrollRef = useRef<HTMLDivElement>(null);
  const scrolled = useScrollEdge(scrollRef);

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
    setFormattedDraft(null);
    setExpandedSources(new Set());
    setPlayerErrors({});
  }, [recordingId]);

  // Adopt the DB value whenever it changes externally (re-run completes,
  // app loads). User edits in `formattedDraft` are reconciled into the DB
  // on a 500ms debounce; once the persist round-trips, dbFormatted will
  // match the draft and the draft can be cleared so subsequent external
  // updates win.
  const dbFormattedForEffect =
    rec?.stages.find((s) => s.stage === "format")?.output ?? "";
  useEffect(() => {
    setFormattedDraft((draft) =>
      draft === null || draft === dbFormattedForEffect ? null : draft,
    );
  }, [dbFormattedForEffect]);

  const onFormattedChange = useCallback(
    (next: string) => {
      setFormattedDraft(next);
      if (persistTimerRef.current !== null) {
        window.clearTimeout(persistTimerRef.current);
      }
      persistTimerRef.current = window.setTimeout(() => {
        void api.setFormatOutput(recordingId, next).catch((err) => {
          console.warn("setFormatOutput failed:", err);
        });
      }, 500);
    },
    [recordingId],
  );

  useEffect(() => {
    return () => {
      if (persistTimerRef.current !== null) {
        window.clearTimeout(persistTimerRef.current);
      }
    };
  }, []);

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
  const dbFormatted = stageOf("format")?.output ?? "";
  const formatStatus = stageOf("format")?.status;
  const formatted = formattedDraft ?? dbFormatted;
  const anyFailed = rec.stages.some((s) => s.status === "failed");
  const allCompleted = rec.stages.every((s) => s.status === "completed");
  const backendRunning = rec.stages.some((s) => s.status === "running");
  const canRun = !running && !backendRunning && !allCompleted; // re-run happens via Start over + Run

  const exportFlow = async () => {
    if (!rec || !globalSettings) return;
    if (!formatted) return;
    await exportRecording({
      formatted,
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

  const redoStage = async (stage: "transcribe" | "format") => {
    const label = stage === "transcribe" ? "Transcribe" : "Format";
    const downstreamNote = stage === "transcribe"
      ? " The current transcript and formatted output will be discarded."
      : " The current formatted output will be discarded.";
    const ok = await dialogConfirm(
      `Re-run ${label}?${downstreamNote}`,
      { title: `Re-run ${label}`, kind: "warning" },
    );
    if (!ok) return;
    // Transcribe deletes the extracted WAVs on success to reclaim disk, so
    // a true "reset transcribe" would fail on missing input. Re-extract
    // first (it's cheap — just ffmpeg on the source) so transcribe has
    // fresh WAVs to chew on.
    const resetFrom = stage === "transcribe" ? "extract" : "format";
    await api.resetRecordingFrom(recordingId, resetFrom);
    await reload();
    onRecordingChanged();
    run();
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
      <Toolbar data-tauri-drag-region scrolled={scrolled}>
        {renamingRec ? (
          <Input
            autoFocus
            size="sm"
            className="font-display text-[22px] font-semibold flex-1 mr-2"
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
            className="font-display text-[22px] font-semibold cursor-text hover:text-accent truncate"
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
      <div className="flex-1 min-h-0 flex">
      <div ref={scrollRef} className="flex-1 min-w-0 min-h-0 overflow-y-auto scrollable p-8 flex flex-col gap-6">
        <div className="shrink-0 flex flex-col gap-1 bg-control-background border border-separator rounded-card shadow-card p-3">
          <span className="text-footnote font-medium text-label-secondary uppercase tracking-wider">
            {rec.sourcePaths.length > 1 ? "Sources" : "Source"}
          </span>
          <ul className="flex flex-col gap-1 max-h-[30vh] overflow-y-auto scrollable">
            {rec.sourcePaths.map((p, idx) => {
              const missing = !!missingByPath[p];
              return (
                <li
                  key={`${p}-${idx}`}
                  className={`flex flex-col gap-1.5 px-3 py-1.5 rounded text-footnote font-mono
                    ${missing
                      ? "bg-status-error/10 border border-status-error/20"
                      : "bg-fill-tertiary"}`}
                >
                  <div className="flex items-center gap-3">
                    {!missing ? (
                      <button
                        type="button"
                        aria-label={expandedSources.has(idx) ? "Collapse player" : "Expand player"}
                        aria-expanded={expandedSources.has(idx)}
                        onClick={() => toggleSourceExpanded(idx)}
                        className="shrink-0 inline-flex items-center justify-center w-4 h-4 text-label-tertiary hover:text-label-primary transition-transform"
                        style={{ transform: expandedSources.has(idx) ? "rotate(90deg)" : "rotate(0deg)" }}
                      >
                        <Icon name="chevron-right" size="sm" />
                      </button>
                    ) : (
                      <span className="shrink-0 w-4" aria-hidden />
                    )}
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
                  </div>
                  {!missing && expandedSources.has(idx) && (
                    <div className="pl-7 pr-1 pb-1">
                      <audio
                        controls
                        preload="metadata"
                        src={convertFileSrc(p)}
                        className="w-full h-8"
                        onError={(e) => {
                          const el = e.currentTarget;
                          const msg = el.error?.message || `Cannot play this file (code ${el.error?.code ?? "?"}).`;
                          setPlayerErrors((prev) => ({ ...prev, [idx]: msg }));
                        }}
                        onLoadedMetadata={() => {
                          setPlayerErrors((prev) => {
                            if (!(idx in prev)) return prev;
                            const { [idx]: _, ...rest } = prev;
                            return rest;
                          });
                        }}
                      />
                      {playerErrors[idx] && (
                        <p className="mt-1 text-status-error text-footnote font-sans">{playerErrors[idx]}</p>
                      )}
                    </div>
                  )}
                </li>
              );
            })}
          </ul>
        </div>

        <div className="shrink-0 flex flex-col gap-2">
          {STAGE_ORDER.map((k) => {
            const s = stageOf(k)!;
            const subtitle =
              k === "transcribe" ? `model: ${globalSettings?.whisperModel ?? "?"}`
              : k === "format" ? `model: ${globalSettings?.model ?? "?"}`
              : undefined;
            const progress =
              s.status === "running" && k === "transcribe" ? transcribePct
              : undefined;
            const onRedo =
              k === "transcribe" || k === "format"
                ? () => void redoStage(k)
                : undefined;
            return (
              <StageCard
                key={k}
                kind={k}
                status={s.status}
                error={s.error}
                subtitle={subtitle}
                progress={progress}
                onRedo={onRedo}
                redoDisabled={running || backendRunning}
              />
            );
          })}
        </div>

        <OutputPreview
          transcript={transcript}
          formatted={formatted}
          highlightRange={highlightRange}
          onFormattedChange={onFormattedChange}
          formattedReadOnly={formatStatus !== "completed"}
        />

        {rec.stages.find((s) => s.stage === "format")?.status === "running" && formatProgress && (
          <div className="shrink-0 text-footnote text-label-tertiary space-y-1">
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
        {askOpen && rec && (
          <div className="w-[420px] shrink-0 border-s border-separator flex flex-col">
            <AskPanel
              recordingId={rec.id}
              disabled={!hasTranscript}
              onCitationClick={(c) => {
                if (c.start === null || c.end === null) return;
                setHighlightRange({ start: c.start, end: c.end, nonce: Date.now() });
              }}
              onClose={() => setAskOpen(false)}
            />
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
