import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";

interface Props {
  onPicked: (filePaths: string[]) => void;
}

const AUDIO_VIDEO_EXTS = new Set([
  "mp4", "mov", "mkv", "avi", "mp3", "m4a", "wav", "ogg", "webm", "aac", "flac",
]);

function isAudioVideo(name: string): boolean {
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  return AUDIO_VIDEO_EXTS.has(ext);
}

export default function RecordingDropZone({ onPicked }: Props) {
  const [dragging, setDragging] = useState(false);

  const openFile = async () => {
    const paths = await open({
      multiple: true,
      filters: [{ name: "Audio / Video", extensions: [...AUDIO_VIDEO_EXTS] }],
    });
    if (!paths) return;
    const list = Array.isArray(paths) ? paths : [paths];
    if (list.length > 0) onPicked(list);
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setDragging(false);
    const files = Array.from(e.dataTransfer.files);
    const picked: string[] = [];
    for (const file of files) {
      if (!isAudioVideo(file.name)) continue;
      const path = (file as File & { path?: string }).path ?? file.name;
      picked.push(path);
    }
    if (picked.length > 0) onPicked(picked);
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
        Drop one or more files here, or click to browse
      </p>
      <p className="text-xs text-white/25">mp4 · mov · mp3 · m4a · wav · …</p>
    </div>
  );
}
