import { useState, useCallback, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import InputStep from "./components/InputStep";
import SettingsStep from "./components/SettingsStep";
import ProcessingStep from "./components/ProcessingStep";
import ResultStep from "./components/ResultStep";
import TitleBar from "./components/TitleBar";

// ── Types ──────────────────────────────────────────────────────────────────────

export type OutputFormat = "md" | "txt" | "docx" | "pdf";

export interface AppSettings {
  ollamaUrl: string;
  model: string;
  whisperModel: string;
  language: string;
  outputFormats: OutputFormat[];
}

export type Stage =
  | "idle"
  | "extracting"
  | "transcribing"
  | "formatting"
  | "saving"
  | "done"
  | "error";

export interface ProcessLog {
  stage: Stage;
  message: string;
  ts: number;
}

const DEFAULT_SETTINGS: AppSettings = {
  ollamaUrl: "http://localhost:11434",
  model: "gemma3",
  whisperModel: "base",
  language: "auto",
  outputFormats: ["md"],
};

// ── App ────────────────────────────────────────────────────────────────────────

export default function App() {
  // Input
  const [recordingPath, setRecordingPath] = useState<string | null>(null);
  const [templatePath, setTemplatePath] = useState<string | null>(null);
  const [templateContent, setTemplateContent] = useState<string>("");

  // Config
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);

  // Step: "input" | "settings" | "processing" | "result"
  const [step, setStep] = useState<"input" | "settings" | "processing" | "result">("input");

  // Processing state
  const [currentStage, setCurrentStage] = useState<Stage>("idle");
  const [logs, setLogs] = useState<ProcessLog[]>([]);
  const [transcript, setTranscript] = useState<string>("");
  const [formattedOutput, setFormattedOutput] = useState<string>("");
  const [savedPaths, setSavedPaths] = useState<string[]>([]);
  const [errorMsg, setErrorMsg] = useState<string>("");
  const [transcribeProgress, setTranscribeProgress] = useState<number>(0);
  const [modelWarming, setModelWarming] = useState<string | null>(null);

  // Listen for transcribe-progress events from the backend
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    listen<{ percent: number; message: string }>("transcribe-progress", (event) => {
      setTranscribeProgress(event.payload.percent);
      setModelWarming(null);
    }).then((fn) => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, []);

  // Listen for model-warming events from the backend
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    listen<{ size: string }>("model-warming", (event) => {
      setModelWarming(event.payload.size);
    }).then((fn) => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, []);

  const addLog = useCallback((stage: Stage, message: string) => {
    setLogs((prev) => [...prev, { stage, message, ts: Date.now() }]);
    setCurrentStage(stage);
  }, []);

  // ── Process pipeline ─────────────────────────────────────────────────────────

  const runPipeline = useCallback(async () => {
    if (!recordingPath) return;

    setStep("processing");
    setLogs([]);
    setErrorMsg("");
    setTranscribeProgress(0);
    setModelWarming(null);
    setTranscript("");
    setFormattedOutput("");
    setSavedPaths([]);

    try {
      // 1. Extract audio
      addLog("extracting", "Extracting audio with ffmpeg…");
      const wavPath: string = await invoke("extract_audio", {
        inputPath: recordingPath,
      });
      addLog("extracting", `Audio extracted → ${wavPath}`);

      // 2. Transcribe
      addLog("transcribing", `Transcribing with Whisper (model: ${settings.whisperModel})…`);
      const rawTranscript: string = await invoke("transcribe_audio", {
        wavPath,
        whisperModel: settings.whisperModel,
        language: settings.language,
      });
      setTranscript(rawTranscript);
      addLog("transcribing", `Transcription complete (${rawTranscript.split(/\s+/).length} words)`);

      // 3. Format with LLM
      addLog("formatting", `Formatting with ${settings.model} via Ollama…`);
      const formatted: string = await invoke("format_transcript", {
        transcript: rawTranscript,
        templateContent,
        ollamaUrl: settings.ollamaUrl,
        model: settings.model,
      });
      setFormattedOutput(formatted);
      addLog("formatting", "Formatting complete.");

      // 4. Save outputs
      addLog("saving", "Saving output files…");
      const paths: string[] = [];

      for (const fmt of settings.outputFormats) {
        const defaultName = `minutes.${fmt}`;
        const savePath = await save({
          defaultPath: defaultName,
          filters: [{ name: fmt.toUpperCase(), extensions: [fmt] }],
        });
        if (!savePath) continue;

        if (fmt === "md" || fmt === "txt") {
          await invoke("save_text_file", { content: formatted, path: savePath });
          paths.push(savePath);
        } else if (fmt === "docx") {
          const bytes = await generateDocx(formatted);
          await invoke("save_binary_file", {
            bytes: Array.from(bytes),
            path: savePath,
          });
          paths.push(savePath);
        } else if (fmt === "pdf") {
          const bytes = await generatePdf(formatted);
          await invoke("save_binary_file", {
            bytes: Array.from(bytes),
            path: savePath,
          });
          paths.push(savePath);
        }
      }

      setSavedPaths(paths);
      addLog("done", `Saved ${paths.length} file(s).`);
      setCurrentStage("done");

      // Cleanup temp files
      await invoke("cleanup_temp");

      setStep("result");
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setErrorMsg(msg);
      addLog("error", msg);
      setCurrentStage("error");
    }
  }, [recordingPath, templateContent, settings, addLog]);

  // ── Reset ────────────────────────────────────────────────────────────────────

  const reset = () => {
    setStep("input");
    setRecordingPath(null);
    setTemplatePath(null);
    setTemplateContent("");
    setTranscript("");
    setFormattedOutput("");
    setSavedPaths([]);
    setLogs([]);
    setErrorMsg("");
    setCurrentStage("idle");
  };

  // ── Template file load ───────────────────────────────────────────────────────

  const loadTemplateFile = useCallback(async (path: string) => {
    setTemplatePath(path);
    try {
      const content: string = await invoke("read_file_as_text", { path });
      setTemplateContent(content);
    } catch (e) {
      console.error("Failed to read template:", e);
    }
  }, []);

  // ── Render ───────────────────────────────────────────────────────────────────

  return (
    <div className="flex flex-col h-screen bg-surface overflow-hidden">
      <TitleBar />

      <div className="flex-1 overflow-hidden">
        {step === "input" && (
          <InputStep
            recordingPath={recordingPath}
            templatePath={templatePath}
            templateContent={templateContent}
            onRecordingDrop={setRecordingPath}
            onTemplateDrop={loadTemplateFile}
            onTemplateChange={setTemplateContent}
            onOpenRecording={async () => {
              const path = await open({
                filters: [
                  {
                    name: "Audio / Video",
                    extensions: ["mp4", "mov", "mkv", "avi", "mp3", "m4a", "wav", "ogg", "webm"],
                  },
                ],
              });
              if (path && typeof path === "string") setRecordingPath(path);
            }}
            onOpenTemplate={async () => {
              const path = await open({
                filters: [{ name: "Document", extensions: ["md", "txt", "docx"] }],
              });
              if (path && typeof path === "string") await loadTemplateFile(path);
            }}
            onNext={() => setStep("settings")}
          />
        )}

        {step === "settings" && (
          <SettingsStep
            settings={settings}
            onChange={setSettings}
            onBack={() => setStep("input")}
            onRun={runPipeline}
          />
        )}

        {step === "processing" && (
          <ProcessingStep
            logs={logs}
            currentStage={currentStage}
            error={errorMsg}
            transcribeProgress={transcribeProgress}
            modelWarming={modelWarming}
            onRetry={() => {
              setStep("input");
            }}
          />
        )}

        {step === "result" && (
          <ResultStep
            formattedOutput={formattedOutput}
            transcript={transcript}
            savedPaths={savedPaths}
            onReset={reset}
          />
        )}
      </div>
    </div>
  );
}

// ── Output generators (frontend) ───────────────────────────────────────────────

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
      return new Paragraph({
        children: [new TextRun({ text: line.slice(2, -2), bold: true })],
      });
    } else if (line.startsWith("- ") || line.startsWith("* ")) {
      return new Paragraph({ text: line.slice(2), bullet: { level: 0 } });
    } else if (line.trim() === "") {
      return new Paragraph({ text: "" });
    } else {
      return new Paragraph({ text: line });
    }
  });

  const doc = new Document({ sections: [{ properties: {}, children }] });
  const buffer = await Packer.toBuffer(doc);
  return new Uint8Array(buffer);
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
      doc.text(line.slice(2), margin, y);
      y += 28;
    } else if (line.startsWith("## ")) {
      doc.setFontSize(16).setFont("helvetica", "bold");
      doc.text(line.slice(3), margin, y);
      y += 22;
    } else if (line.startsWith("### ")) {
      doc.setFontSize(13).setFont("helvetica", "bold");
      doc.text(line.slice(4), margin, y);
      y += 18;
    } else if (line.startsWith("- ") || line.startsWith("* ")) {
      doc.setFontSize(11).setFont("helvetica", "normal");
      const wrapped = doc.splitTextToSize(`• ${line.slice(2)}`, maxWidth - 12);
      doc.text(wrapped, margin + 8, y);
      y += wrapped.length * 14 + 2;
    } else if (line.trim() === "") {
      y += 8;
    } else {
      doc.setFontSize(11).setFont("helvetica", "normal");
      const wrapped = doc.splitTextToSize(line, maxWidth);
      doc.text(wrapped, margin, y);
      y += wrapped.length * 14 + 2;
    }
  }

  return new Uint8Array(doc.output("arraybuffer") as ArrayBuffer);
}
