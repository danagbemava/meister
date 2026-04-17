import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";

interface Props {
  onPicked: (filePath: string) => void;
}

const AUDIO_VIDEO_EXTS = new Set([
  "mp4", "mov", "mkv", "avi", "mp3", "m4a", "wav", "ogg", "webm", "aac", "flac",
]);

export default function RecordingDropZone({ onPicked }: Props) {
  const [dragging, setDragging] = useState(false);

  const openFile = async () => {
    const path = await open({
      filters: [{ name: "Audio / Video", extensions: [...AUDIO_VIDEO_EXTS] }],
    });
    if (path && typeof path === "string") onPicked(path);
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setDragging(false);
    const file = e.dataTransfer.files[0];
    if (!file) return;
    const ext = file.name.split(".").pop()?.toLowerCase() ?? "";
    if (!AUDIO_VIDEO_EXTS.has(ext)) return;
    const path = (file as File & { path?: string }).path ?? file.name;
    onPicked(path);
  };

  return (
    <div
      className={`rounded-xl border-2 border-dashed flex flex-col items-center justify-center gap-2 cursor-pointer py-10 transition-colors
        ${dragging
          ? "border-accent bg-accent/10"
          : "border-white/10 hover:border-white/20 bg-surface-secondary"}`}
      onDragOver={(e) => { e.preventDefault(); setDragging(true); }}
      onDragLeave={() => setDragging(false)}
      onDrop={handleDrop}
      onClick={openFile}
    >
      <span className="text-3xl opacity-30">🎬</span>
      <p className="text-sm text-white/40 text-center px-4">
        Drop a recording here, or click to browse
      </p>
      <p className="text-xs text-white/25">mp4 · mov · mp3 · m4a · wav · …</p>
    </div>
  );
}
