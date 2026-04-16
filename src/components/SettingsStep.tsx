import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AppSettings, OutputFormat } from "../App";

interface Props {
  settings: AppSettings;
  onChange: (s: AppSettings) => void;
  onBack: () => void;
  onRun: () => void;
}

const WHISPER_MODELS = ["tiny", "base", "small", "medium", "large"];
const OUTPUT_FORMATS: { id: OutputFormat; label: string; desc: string }[] = [
  { id: "md", label: "Markdown", desc: ".md" },
  { id: "txt", label: "Plain text", desc: ".txt" },
  { id: "docx", label: "Word doc", desc: ".docx" },
  { id: "pdf", label: "PDF", desc: ".pdf" },
];

export default function SettingsStep({ settings, onChange, onBack, onRun }: Props) {
  const [models, setModels] = useState<string[]>([]);
  const [checking, setChecking] = useState(false);
  const [deps, setDeps] = useState<{ ffmpeg: boolean; whisper: boolean; ollama: boolean } | null>(null);

  const set = <K extends keyof AppSettings>(key: K, val: AppSettings[K]) =>
    onChange({ ...settings, [key]: val });

  const toggleFormat = (fmt: OutputFormat) => {
    const next = settings.outputFormats.includes(fmt)
      ? settings.outputFormats.filter((f) => f !== fmt)
      : [...settings.outputFormats, fmt];
    if (next.length > 0) set("outputFormats", next);
  };

  useEffect(() => {
    checkDeps();
  }, []);

  const checkDeps = async () => {
    setChecking(true);
    try {
      const result = await invoke<{ ffmpeg: boolean; whisper: boolean; ollama: boolean }>(
        "check_dependencies",
        { ollamaUrl: settings.ollamaUrl }
      );
      setDeps(result);

      if (result.ollama) {
        const m = await invoke<string[]>("list_ollama_models", { ollamaUrl: settings.ollamaUrl });
        setModels(m);
      }
    } catch {
      // silently fail — user can still proceed
    } finally {
      setChecking(false);
    }
  };

  const canRun = settings.outputFormats.length > 0;

  return (
    <div className="flex flex-col h-full px-8 py-6 gap-6">
      <h1 className="text-xl font-semibold">Configure</h1>

      <div className="flex-1 overflow-y-auto scrollable flex flex-col gap-6">

        {/* Dependency status */}
        <Section title="System Dependencies">
          <div className="flex gap-3 flex-wrap">
            {(["ffmpeg", "whisper", "ollama"] as const).map((dep) => (
              <div
                key={dep}
                className={`flex items-center gap-2 px-3 py-1.5 rounded-lg text-sm
                  ${checking
                    ? "bg-surface-tertiary text-white/40"
                    : deps?.[dep]
                      ? "bg-green-500/15 text-green-400"
                      : "bg-red-500/15 text-red-400"
                  }`}
              >
                {checking ? (
                  <span className="spinner" />
                ) : deps?.[dep] ? (
                  <span>✓</span>
                ) : (
                  <span>✗</span>
                )}
                <span className="font-mono">{dep}</span>
              </div>
            ))}
            <button
              onClick={checkDeps}
              className="px-3 py-1.5 rounded-lg text-sm text-white/40 hover:text-white/70 bg-surface-secondary transition-colors"
            >
              Recheck
            </button>
          </div>
          {deps && !deps.ollama && (
            <p className="text-xs text-red-400 mt-2">
              Ollama not reachable at <code className="font-mono">{settings.ollamaUrl}</code>.
              Check the URL below or start Ollama.
            </p>
          )}
          {deps && !deps.ffmpeg && (
            <p className="text-xs text-red-400 mt-1">
              ffmpeg not found. Install with: <code className="font-mono">brew install ffmpeg</code>
            </p>
          )}
          {deps && !deps.whisper && (
            <p className="text-xs text-red-400 mt-1">
              Whisper not found. Install with:{" "}
              <code className="font-mono">pip install openai-whisper</code>
            </p>
          )}
        </Section>

        {/* Ollama settings */}
        <Section title="Ollama">
          <div className="flex flex-col gap-3">
            <Field label="Server URL">
              <input
                type="text"
                className="input"
                value={settings.ollamaUrl}
                onChange={(e) => set("ollamaUrl", e.target.value)}
                onBlur={checkDeps}
                placeholder="http://localhost:11434"
              />
            </Field>
            <Field label="Model">
              {models.length > 0 ? (
                <select
                  className="input"
                  value={settings.model}
                  onChange={(e) => set("model", e.target.value)}
                >
                  {models.map((m) => (
                    <option key={m} value={m}>{m}</option>
                  ))}
                </select>
              ) : (
                <input
                  type="text"
                  className="input"
                  value={settings.model}
                  onChange={(e) => set("model", e.target.value)}
                  placeholder="gemma3"
                />
              )}
              <p className="text-xs text-white/30 mt-1">
                Pull a model first: <code className="font-mono">ollama pull gemma3</code>
              </p>
            </Field>
          </div>
        </Section>

        {/* Whisper settings */}
        <Section title="Transcription (Whisper)">
          <div className="flex flex-col gap-3">
            <Field label="Model size">
              <select
                className="input"
                value={settings.whisperModel}
                onChange={(e) => set("whisperModel", e.target.value)}
              >
                {WHISPER_MODELS.map((m) => (
                  <option key={m} value={m}>{m}</option>
                ))}
              </select>
              <p className="text-xs text-white/30 mt-1">
                Larger = more accurate, slower. <strong>base</strong> is a good default.
              </p>
            </Field>
            <Field label="Language (optional)">
              <input
                type="text"
                className="input"
                value={settings.language}
                onChange={(e) => set("language", e.target.value)}
                placeholder="auto  (or e.g. en, de, fr)"
              />
            </Field>
          </div>
        </Section>

        {/* Output formats */}
        <Section title="Output Formats">
          <div className="flex gap-2 flex-wrap">
            {OUTPUT_FORMATS.map(({ id, label, desc }) => {
              const active = settings.outputFormats.includes(id);
              return (
                <button
                  key={id}
                  onClick={() => toggleFormat(id)}
                  className={`px-4 py-2 rounded-lg text-sm transition-colors flex flex-col items-center gap-0.5
                    ${active
                      ? "bg-accent/20 border border-accent text-accent"
                      : "bg-surface-secondary border border-white/10 text-white/50 hover:border-white/20"
                    }`}
                >
                  <span className="font-medium">{label}</span>
                  <span className="text-[10px] opacity-60">{desc}</span>
                </button>
              );
            })}
          </div>
        </Section>
      </div>

      {/* Footer */}
      <div className="flex items-center justify-between shrink-0">
        <button
          onClick={onBack}
          className="px-5 py-2 rounded-lg bg-surface-secondary hover:bg-surface-tertiary text-sm font-medium transition-colors"
        >
          ← Back
        </button>
        <button
          disabled={!canRun}
          onClick={onRun}
          className="px-6 py-2 rounded-lg bg-accent hover:bg-accent-hover disabled:opacity-30 disabled:cursor-not-allowed text-sm font-semibold transition-colors"
        >
          Run →
        </button>
      </div>
    </div>
  );
}

// ── Small layout helpers ───────────────────────────────────────────────────────

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-3">
      <h2 className="text-xs font-semibold text-white/40 uppercase tracking-wider">{title}</h2>
      {children}
    </div>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1">
      <label className="text-sm text-white/60">{label}</label>
      {children}
    </div>
  );
}
