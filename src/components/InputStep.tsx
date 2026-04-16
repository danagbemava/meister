import { useRef, useState } from "react";

interface Props {
  recordingPath: string | null;
  templatePath: string | null;
  templateContent: string;
  onRecordingDrop: (path: string) => void;
  onTemplateDrop: (path: string) => void;
  onTemplateChange: (content: string) => void;
  onOpenRecording: () => void;
  onOpenTemplate: () => void;
  onNext: () => void;
}

const AUDIO_VIDEO_EXTS = new Set([
  "mp4", "mov", "mkv", "avi", "mp3", "m4a", "wav", "ogg", "webm", "aac", "flac",
]);

const TEMPLATE_EXTS = new Set(["md", "txt", "docx"]);

export default function InputStep({
  recordingPath,
  templatePath,
  templateContent,
  onRecordingDrop,
  onTemplateDrop,
  onTemplateChange,
  onOpenRecording,
  onOpenTemplate,
  onNext,
}: Props) {
  const [recDragging, setRecDragging] = useState(false);
  const [tplDragging, setTplDragging] = useState(false);
  const templateRef = useRef<HTMLTextAreaElement>(null);

  const canProceed = !!recordingPath;

  // ── Drag helpers ─────────────────────────────────────────────────────────────

  const handleRecordingDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setRecDragging(false);
    const file = e.dataTransfer.files[0];
    if (!file) return;
    const ext = file.name.split(".").pop()?.toLowerCase() ?? "";
    if (AUDIO_VIDEO_EXTS.has(ext)) {
      // Tauri exposes the real file path via the non-standard `path` property
      const filePath = (file as File & { path?: string }).path ?? file.name;
      onRecordingDrop(filePath);
    }
  };

  const handleTemplateDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setTplDragging(false);
    const file = e.dataTransfer.files[0];
    if (!file) return;
    const ext = file.name.split(".").pop()?.toLowerCase() ?? "";
    if (TEMPLATE_EXTS.has(ext)) {
      const filePath = (file as File & { path?: string }).path ?? file.name;
      onTemplateDrop(filePath);
    }
  };

  const filename = (p: string) => p.split(/[\\/]/).pop() ?? p;

  return (
    <div className="flex flex-col h-full px-8 py-6 gap-6">
      <h1 className="text-xl font-semibold">New Recording</h1>

      {/* Two-column layout */}
      <div className="flex-1 flex gap-5 min-h-0">

        {/* Left: Recording drop zone */}
        <div className="flex flex-col gap-3 w-64 shrink-0">
          <label className="text-xs font-medium text-white/50 uppercase tracking-wider">
            Recording
          </label>

          <div
            className={`flex-1 rounded-xl border-2 border-dashed flex flex-col items-center justify-center gap-3 cursor-pointer transition-colors
              ${recDragging
                ? "border-accent bg-accent/10 drop-active"
                : recordingPath
                  ? "border-green-500/60 bg-green-500/5"
                  : "border-white/10 hover:border-white/20 bg-surface-secondary"
              }`}
            onDragOver={(e) => { e.preventDefault(); setRecDragging(true); }}
            onDragLeave={() => setRecDragging(false)}
            onDrop={handleRecordingDrop}
            onClick={onOpenRecording}
          >
            {recordingPath ? (
              <>
                <span className="text-3xl">🎙️</span>
                <span className="text-sm font-medium text-green-400 text-center px-3 break-all">
                  {filename(recordingPath)}
                </span>
                <span className="text-xs text-white/30">Click to change</span>
              </>
            ) : (
              <>
                <span className="text-4xl opacity-30">🎬</span>
                <p className="text-sm text-white/40 text-center px-4">
                  Drop a video or audio file, or click to browse
                </p>
                <p className="text-xs text-white/25">mp4 · mov · mp3 · m4a · wav · …</p>
              </>
            )}
          </div>
        </div>

        {/* Right: Template */}
        <div className="flex flex-col flex-1 gap-3 min-h-0">
          <div className="flex items-center justify-between">
            <label className="text-xs font-medium text-white/50 uppercase tracking-wider">
              Format Template
            </label>
            <button
              onClick={onOpenTemplate}
              className="text-xs text-accent hover:text-accent-hover transition-colors"
            >
              {templatePath ? `Load different file` : "Load from file"}
            </button>
          </div>

          {/* Template drop overlay + textarea */}
          <div
            className={`flex-1 relative rounded-xl border transition-colors min-h-0
              ${tplDragging
                ? "border-accent bg-accent/10 drop-active"
                : "border-white/10"
              }`}
            onDragOver={(e) => { e.preventDefault(); setTplDragging(true); }}
            onDragLeave={() => setTplDragging(false)}
            onDrop={handleTemplateDrop}
          >
            {tplDragging && (
              <div className="absolute inset-0 flex items-center justify-center rounded-xl bg-accent/10 z-10 pointer-events-none">
                <p className="text-accent font-medium">Drop template file here</p>
              </div>
            )}

            <textarea
              ref={templateRef}
              className="w-full h-full bg-surface-secondary rounded-xl p-4 text-sm font-mono text-white/80 resize-none outline-none focus:ring-1 focus:ring-accent/50 placeholder-white/20 scrollable"
              placeholder={`Paste or type your format template here.\n\nExample:\n# Meeting Minutes\nDate: [DATE]\nAttendees: [NAMES]\n\n## Agenda\n...\n\n## Action Items\n- [ ] ...`}
              value={templateContent}
              onChange={(e) => onTemplateChange(e.target.value)}
              spellCheck={false}
            />
          </div>

          {templatePath && (
            <p className="text-xs text-white/30 truncate">
              Loaded: {filename(templatePath)}
            </p>
          )}
        </div>
      </div>

      {/* Footer */}
      <div className="flex items-center justify-between shrink-0">
        <p className="text-xs text-white/25">
          {!recordingPath && "Drop a recording to get started."}
          {recordingPath && !templateContent.trim() && "Template is optional — add one to guide formatting."}
          {recordingPath && templateContent.trim().length > 0 && "Ready to configure and run."}
        </p>
        <button
          disabled={!canProceed}
          onClick={onNext}
          className="px-5 py-2 rounded-lg bg-accent hover:bg-accent-hover disabled:opacity-30 disabled:cursor-not-allowed text-sm font-medium transition-colors"
        >
          Configure →
        </button>
      </div>
    </div>
  );
}
