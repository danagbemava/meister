import { useCallback, useEffect, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { save as saveDialog, open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import StageCard from "./StageCard";
import OutputPreview from "./OutputPreview";
import SettingsForm from "./SettingsForm";
import { api } from "../api/storage";
import {
  overallStatus,
  type Recording,
  type StageRecord,
  type StageKind,
} from "../types/storage";

interface Props {
  recordingId: string;
  onRecordingChanged: () => void;
  onRecordingDeleted: () => void;
}

const STAGE_ORDER: StageKind[] = ["extract", "transcribe", "format", "save"];

export default function RecordingView({
  recordingId,
  onRecordingChanged,
  onRecordingDeleted,
}: Props) {
  const [rec, setRec] = useState<Recording | null>(null);
  const [running, setRunning] = useState(false);
  const [transcribePct, setTranscribePct] = useState(0);
  const [formatChars, setFormatChars] = useState(0);
  const [missingByPath, setMissingByPath] = useState<Record<string, boolean>>({});
  const [renamingRec, setRenamingRec] = useState(false);
  const [draftRecName, setDraftRecName] = useState("");

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
    listen<{ chars: number }>("format-progress", (e) =>
      setFormatChars(e.payload.chars)
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

  if (!rec) return <div className="p-8 text-white/40">Loading…</div>;

  const stageOf = (k: StageKind): StageRecord | undefined =>
    rec.stages.find((s) => s.stage === k);
  const transcript = stageOf("transcribe")?.output ?? "";
  const formatted = stageOf("format")?.output ?? "";
  const savedPaths = stageOf("save")?.savedPaths ?? [];
  const anyFailed = rec.stages.some((s) => s.status === "failed");
  const allCompleted = rec.stages.every((s) => s.status === "completed");
  const canRun = !running && !allCompleted; // re-run happens via Start over + Run

  const run = async () => {
    setRunning(true);
    setTranscribePct(0);
    setFormatChars(0);
    try {
      await api.runRecording(recordingId);
      await reload();
      // If save stage is "completed" with empty savedPaths, ask the user to pick paths.
      const updated = await api.getRecording(recordingId);
      if (updated) {
        const s = updated.stages.find((x) => x.stage === "save");
        if (s && s.status === "completed" && (s.savedPaths ?? []).length === 0) {
          await runSaveFlow(updated);
        }
      }
      await reload();
    } catch (err) {
      console.error("run_recording failed:", err);
      await reload();
    } finally {
      setRunning(false);
      onRecordingChanged();
    }
  };

  const runSaveFlow = async (r: Recording) => {
    const formatted = r.stages.find((s) => s.stage === "format")?.output ?? "";
    const paths: string[] = [];
    for (const fmt of r.settings.outputFormats) {
      const defaultName = `${r.name}.${fmt}`;
      const dest = await saveDialog({
        defaultPath: defaultName,
        filters: [{ name: fmt.toUpperCase(), extensions: [fmt] }],
      });
      if (!dest) continue;
      if (fmt === "md" || fmt === "txt") {
        await invoke("save_text_file", { content: formatted, path: dest });
        paths.push(dest);
      } else if (fmt === "docx") {
        const bytes = await generateDocx(formatted);
        await invoke("save_binary_file", { bytes: Array.from(bytes), path: dest });
        paths.push(dest);
      } else if (fmt === "pdf") {
        const bytes = await generatePdf(formatted);
        await invoke("save_binary_file", { bytes: Array.from(bytes), path: dest });
        paths.push(dest);
      }
    }
    await api.recordSavedOutputs(r.id, paths);
  };

  const startOver = async () => {
    if (!confirm("Start over — discard all progress for this recording?")) return;
    await api.startOverRecording(recordingId);
    await reload();
    onRecordingChanged();
  };

  const deleteMe = async () => {
    if (!confirm(`Delete recording '${rec.name}'?`)) return;
    await api.deleteRecording(recordingId);
    onRecordingDeleted();
  };

  return (
    <div className="h-full flex flex-col">
      <div className="flex-1 overflow-y-auto scrollable p-8 flex flex-col gap-6">
        <div className="flex items-center justify-between">
          {renamingRec ? (
            <input
              autoFocus
              className="input text-xl font-semibold flex-1 mr-4"
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
              className="text-xl font-semibold cursor-text hover:text-accent"
              title="Click to rename"
            >
              {rec.name}
            </h1>
          )}
          <div className="flex items-center gap-2 text-xs">
            <span className="text-white/40">{overallStatus(rec)}</span>
            <button onClick={deleteMe} className="text-white/40 hover:text-red-400">Delete</button>
          </div>
        </div>
        <div className="flex flex-col gap-1">
          <span className="text-xs font-medium text-white/50 uppercase tracking-wider">
            {rec.sourcePaths.length > 1 ? "Sources" : "Source"}
          </span>
          <ul className="flex flex-col gap-1">
            {rec.sourcePaths.map((p, idx) => {
              const missing = !!missingByPath[p];
              return (
                <li
                  key={`${p}-${idx}`}
                  className={`flex items-center gap-3 px-3 py-1.5 rounded text-xs font-mono
                    ${missing
                      ? "bg-red-500/10 border border-red-500/20"
                      : "bg-surface-secondary"}`}
                >
                  <span className="text-white/30 tabular-nums w-5 shrink-0">
                    {rec.sourcePaths.length > 1 ? `${idx + 1}.` : ""}
                  </span>
                  <span className={`flex-1 truncate ${missing ? "text-red-400" : "text-white/60"}`} title={p}>
                    {p}
                  </span>
                  {missing && (
                    <button
                      onClick={() => locateSource(idx)}
                      className="text-accent hover:text-accent-hover shrink-0"
                    >
                      Locate…
                    </button>
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
              k === "transcribe" ? `model: ${rec.settings.whisperModel}`
              : k === "format" ? `model: ${rec.settings.model}`
              : k === "save" ? `formats: ${rec.settings.outputFormats.join(", ")}`
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

        <details className="bg-surface-secondary rounded-xl p-4">
          <summary className="cursor-pointer text-sm text-white/60">Recording settings</summary>
          <div className="mt-4 flex flex-col gap-3">
            {(() => {
              const warning = completedStageInvalidation(rec);
              return warning ? (
                <p className="text-xs bg-amber-500/10 border border-amber-500/20 text-amber-300 rounded px-3 py-2">
                  {warning}
                </p>
              ) : null;
            })()}
            <SettingsForm
              settings={rec.settings}
              onChange={async (settings) => {
                const next = { ...rec, settings };
                setRec(next);
                await api.updateRecording(next);
                onRecordingChanged();
              }}
            />
          </div>
        </details>

        <OutputPreview
          transcript={transcript}
          formatted={formatted}
          savedPaths={savedPaths}
        />

        {rec.stages.find((s) => s.stage === "format")?.status === "running" && formatChars > 0 && (
          <p className="text-xs text-white/40">Formatting… {formatChars.toLocaleString()} chars</p>
        )}
      </div>

      <div className="shrink-0 border-t border-white/5 p-4 flex items-center justify-between bg-surface">
        <button
          onClick={startOver}
          className="text-sm text-white/50 hover:text-white"
        >
          Start over
        </button>
        <button
          disabled={!canRun}
          onClick={run}
          className="px-6 py-2 rounded-lg bg-accent hover:bg-accent-hover disabled:opacity-30 disabled:cursor-not-allowed text-sm font-semibold transition-colors"
        >
          {running ? "Running…" : anyFailed ? "Resume" : "Run"}
        </button>
      </div>
    </div>
  );
}

// ── Output generators (copied verbatim from old App.tsx) ─────────────────────

async function generateDocx(markdown: string): Promise<Uint8Array> {
  const { Document, Packer, Paragraph, TextRun, HeadingLevel } = await import("docx");
  const lines = markdown.split("\n");
  const children = lines.map((line) => {
    if (line.startsWith("### ")) {
      return new Paragraph({ text: line.slice(4), heading: HeadingLevel.HEADING_3 });
    } else if (line.startsWith("## ")) {
      return new Paragraph({ text: line.slice(3), heading: HeadingLevel.HEADING_2 });
    } else if (line.startsWith("# ")) {
      return new Paragraph({ text: line.slice(2), heading: HeadingLevel.HEADING_1 });
    } else if (line.startsWith("**") && line.endsWith("**") && line.length > 4) {
      return new Paragraph({ children: [new TextRun({ text: line.slice(2, -2), bold: true })] });
    } else if (line.startsWith("- ") || line.startsWith("* ")) {
      return new Paragraph({ text: line.slice(2), bullet: { level: 0 } });
    } else if (line.trim() === "") {
      return new Paragraph({ text: "" });
    } else {
      return new Paragraph({ text: line });
    }
  });
  const doc = new Document({ sections: [{ properties: {}, children }] });
  const blob = await Packer.toBlob(doc);
  const ab = await blob.arrayBuffer();
  return new Uint8Array(ab);
}

async function generatePdf(markdown: string): Promise<Uint8Array> {
  const { jsPDF } = await import("jspdf");
  const doc = new jsPDF({ unit: "pt", format: "a4" });
  const pageWidth = doc.internal.pageSize.getWidth();
  const margin = 60;
  const maxWidth = pageWidth - margin * 2;
  let y = 80;
  const lines = markdown.split("\n");
  for (const raw of lines) {
    const line = raw.trimEnd();
    if (y > doc.internal.pageSize.getHeight() - 60) {
      doc.addPage();
      y = 60;
    }
    if (line.startsWith("# ")) {
      doc.setFontSize(20).setFont("helvetica", "bold");
      doc.text(line.slice(2), margin, y); y += 28;
    } else if (line.startsWith("## ")) {
      doc.setFontSize(16).setFont("helvetica", "bold");
      doc.text(line.slice(3), margin, y); y += 22;
    } else if (line.startsWith("### ")) {
      doc.setFontSize(13).setFont("helvetica", "bold");
      doc.text(line.slice(4), margin, y); y += 18;
    } else if (line.startsWith("- ") || line.startsWith("* ")) {
      doc.setFontSize(11).setFont("helvetica", "normal");
      const wrapped = doc.splitTextToSize(`• ${line.slice(2)}`, maxWidth - 12);
      doc.text(wrapped, margin + 8, y); y += wrapped.length * 14 + 2;
    } else if (line.trim() === "") {
      y += 8;
    } else {
      doc.setFontSize(11).setFont("helvetica", "normal");
      const wrapped = doc.splitTextToSize(line, maxWidth);
      doc.text(wrapped, margin, y); y += wrapped.length * 14 + 2;
    }
  }
  return new Uint8Array(doc.output("arraybuffer") as ArrayBuffer);
}

function completedStageInvalidation(r: Recording): string | null {
  const affected: string[] = [];
  const stage = (k: StageKind) => r.stages.find((s) => s.stage === k);

  const t = stage("transcribe");
  if (t?.status === "completed") {
    const encoded = `${r.settings.whisperModel}@${r.settings.language}`;
    if (t.modelUsed !== encoded) affected.push("transcribe");
  }

  const f = stage("format");
  if (f?.status === "completed" && f.modelUsed !== r.settings.model) {
    affected.push("format");
  }

  const s = stage("save");
  if (s?.status === "completed") {
    const saved = (s.savedPaths ?? []).map((p) => p.split(".").pop() ?? "");
    const want = [...r.settings.outputFormats].sort().join(",");
    const have = [...new Set(saved)].sort().join(",");
    if (want !== have) affected.push("save");
  }

  if (affected.length === 0) return null;
  const downstream = affected.includes("transcribe")
    ? " (+ format, save)"
    : affected.includes("format") ? " (+ save)"
    : "";
  return `Changing settings will re-run: ${affected.join(", ")}${downstream} on next Run.`;
}
