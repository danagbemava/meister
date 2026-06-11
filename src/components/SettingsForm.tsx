import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { GlobalSettings, OutputFormat } from "../types/storage";
import DownloadModelModal from "./DownloadModelModal";
import { Input, Select } from "./ui";
import { THEMES, applyTheme, getStoredTheme, type ThemeKey } from "../lib/theme";
import { cn } from "../lib/cn";

interface Props {
  settings: GlobalSettings;
  onChange: (s: GlobalSettings) => void;
}

interface ModelInfo {
  size: string;
  downloaded: boolean;
  size_bytes: number;
}

const EXPORT_FORMATS: { id: OutputFormat; label: string; desc: string }[] = [
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
  const [theme, setTheme] = useState<ThemeKey>(() => getStoredTheme());
  const [ollamaModels, setOllamaModels] = useState<string[]>([]);
  const [modelInfos, setModelInfos] = useState<ModelInfo[]>([]);
  const [pendingDownload, setPendingDownload] = useState<ModelInfo | null>(null);

  const set = <K extends keyof GlobalSettings>(key: K, val: GlobalSettings[K]) =>
    onChange({ ...settings, [key]: val });

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
      <Section title="Appearance">
        <div className="flex gap-2 flex-wrap">
          {THEMES.map((t) => (
            <button
              key={t.key}
              type="button"
              onClick={() => { applyTheme(t.key); setTheme(t.key); }}
              aria-pressed={theme === t.key}
              className={cn(
                "flex items-center gap-2 rounded-full border px-3 py-1.5 text-callout",
                theme === t.key
                  ? "border-accent text-label-primary bg-fill-tertiary"
                  : "border-separator text-label-secondary hover:bg-fill-quaternary",
              )}
            >
              <span
                className="inline-block h-4 w-4 rounded-full border border-separator"
                style={{ background: `linear-gradient(135deg, ${t.swatch.bg} 50%, ${t.swatch.accent} 50%)` }}
              />
              {t.name}
            </button>
          ))}
        </div>
      </Section>

      <Section title="Ollama">
        <Field label="Server URL">
          <Input
            value={settings.ollamaUrl}
            onChange={(e) => set("ollamaUrl", e.target.value)}
            onBlur={refresh}
            placeholder="http://localhost:11434" />
        </Field>
        <Field label="Model">
          {ollamaModels.length > 0 ? (
            <Select value={settings.model}
              onChange={(e) => set("model", e.target.value)}>
              {ollamaModels.map((m) => <option key={m} value={m}>{m}</option>)}
            </Select>
          ) : (
            <Input value={settings.model}
              onChange={(e) => set("model", e.target.value)} placeholder="gemma3" />
          )}
        </Field>
      </Section>

      <Section title="Transcription">
        <Field label="Model">
          <Select value={settings.whisperModel}
            onChange={(e) => pickWhisper(e.target.value)}>
            {modelInfos.map((m) => (
              <option key={m.size} value={m.size}>{modelLabel(m)}</option>
            ))}
          </Select>
        </Field>
        <Field label="Default language">
          <Input
            value={settings.language}
            onChange={(e) => set("language", e.target.value)}
            placeholder="auto  (or e.g. en, de, fr)" />
        </Field>
      </Section>

      <Section title="Export">
        <div className="flex flex-col gap-2">
          <label className="text-body text-label-secondary">Default format</label>
          <div className="flex flex-col gap-1">
            {EXPORT_FORMATS.map(({ id, label, desc }) => (
              <label key={id}
                className="flex items-center gap-2 text-body text-label-primary">
                <input
                  type="radio"
                  name="defaultExportFormat"
                  checked={settings.defaultExportFormat === id}
                  onChange={() => set("defaultExportFormat", id)}
                />
                <span className="font-medium">{label}</span>
                <span className="text-footnote text-label-tertiary">{desc}</span>
              </label>
            ))}
          </div>
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
      <div className="bg-fill-quaternary border border-separator rounded-card flex flex-col gap-3 p-4">
        {children}
      </div>
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
