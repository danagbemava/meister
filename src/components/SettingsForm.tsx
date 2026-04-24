import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AppSettings, OutputFormat } from "../types/storage";
import DownloadModelModal from "./DownloadModelModal";

interface Props {
  settings: AppSettings;
  onChange: (s: AppSettings) => void;
}

interface ModelInfo {
  size: string;
  downloaded: boolean;
  size_bytes: number;
}

const OUTPUT_FORMATS: { id: OutputFormat; label: string; desc: string }[] = [
  { id: "md", label: "Markdown", desc: ".md" },
  { id: "txt", label: "Plain text", desc: ".txt" },
  { id: "docx", label: "Word doc", desc: ".docx" },
  { id: "pdf", label: "PDF", desc: ".pdf" },
];

function formatSize(bytes: number) {
  if (bytes >= 1024 ** 3) return (bytes / 1024 ** 3).toFixed(1) + " GB";
  return Math.round(bytes / 1024 ** 2) + " MB";
}

function modelLabel(m: ModelInfo) {
  if (m.size === "base" && m.downloaded) return `${m.size} — bundled ✓`;
  if (m.downloaded) return `${m.size} — downloaded ✓ (${formatSize(m.size_bytes)})`;
  return `${m.size} — ${formatSize(m.size_bytes)} (not downloaded)`;
}

export default function SettingsForm({ settings, onChange }: Props) {
  const [ollamaModels, setOllamaModels] = useState<string[]>([]);
  const [modelInfos, setModelInfos] = useState<ModelInfo[]>([]);
  const [pendingDownload, setPendingDownload] = useState<ModelInfo | null>(null);

  const set = <K extends keyof AppSettings>(key: K, val: AppSettings[K]) =>
    onChange({ ...settings, [key]: val });

  const toggleFormat = (fmt: OutputFormat) => {
    const next = settings.outputFormats.includes(fmt)
      ? settings.outputFormats.filter((f) => f !== fmt)
      : [...settings.outputFormats, fmt];
    if (next.length > 0) set("outputFormats", next);
  };

  useEffect(() => { refresh(); }, []);

  const refresh = async () => {
    try {
      const m = await invoke<string[]>("list_ollama_models", { ollamaUrl: settings.ollamaUrl });
      setOllamaModels(m);
      if (m.length > 0 && !m.includes(settings.model)) set("model", m[0]);
    } catch { /* ignore */ }
    try {
      const infos = await invoke<ModelInfo[]>("list_available_models");
      setModelInfos(infos);
    } catch { /* ignore */ }
  };

  const pickWhisper = (newSize: string) => {
    const info = modelInfos.find((m) => m.size === newSize);
    if (!info) return;
    if (info.downloaded) set("whisperModel", newSize);
    else setPendingDownload(info);
  };

  return (
    <div className="flex flex-col gap-5">
      <Section title="Ollama">
        <Field label="Server URL">
          <input type="text" className="input"
            value={settings.ollamaUrl}
            onChange={(e) => set("ollamaUrl", e.target.value)}
            onBlur={refresh}
            placeholder="http://localhost:11434" />
        </Field>
        <Field label="Model">
          {ollamaModels.length > 0 ? (
            <select className="input" value={settings.model}
              onChange={(e) => set("model", e.target.value)}>
              {ollamaModels.map((m) => <option key={m} value={m}>{m}</option>)}
            </select>
          ) : (
            <input type="text" className="input" value={settings.model}
              onChange={(e) => set("model", e.target.value)} placeholder="gemma3" />
          )}
        </Field>
      </Section>

      <Section title="Transcription">
        <Field label="Model">
          <select className="input" value={settings.whisperModel}
            onChange={(e) => pickWhisper(e.target.value)}>
            {modelInfos.map((m) => (
              <option key={m.size} value={m.size}>{modelLabel(m)}</option>
            ))}
          </select>
        </Field>
        <Field label="Language">
          <input type="text" className="input"
            value={settings.language}
            onChange={(e) => set("language", e.target.value)}
            placeholder="auto  (or e.g. en, de, fr)" />
        </Field>
      </Section>

      <Section title="Output Formats">
        <div className="flex gap-2 flex-wrap">
          {OUTPUT_FORMATS.map(({ id, label, desc }) => {
            const active = settings.outputFormats.includes(id);
            return (
              <button key={id} onClick={() => toggleFormat(id)}
                className={`px-4 py-2 rounded-lg text-body transition-colors flex flex-col items-center gap-0.5
                  ${active
                    ? "bg-accent/20 border border-accent text-accent"
                    : "bg-control-background border border-separator text-label-secondary hover:border-separator"}`}>
                <span className="font-medium">{label}</span>
                <span className="text-[10px] opacity-60">{desc}</span>
              </button>
            );
          })}
        </div>
      </Section>

      {pendingDownload && (
        <DownloadModelModal
          size={pendingDownload.size}
          totalBytesEstimate={pendingDownload.size_bytes}
          onComplete={() => {
            set("whisperModel", pendingDownload.size);
            setPendingDownload(null);
            refresh();
          }}
          onCancel={() => setPendingDownload(null)}
        />
      )}
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-2">
      <h2 className="text-footnote font-semibold text-label-secondary uppercase tracking-wider">{title}</h2>
      {children}
    </div>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1">
      <label className="text-body text-label-secondary">{label}</label>
      {children}
    </div>
  );
}
