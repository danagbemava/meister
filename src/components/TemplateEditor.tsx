import { useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { Template } from "../types/storage";

interface Props {
  template: Template;
  onChange: (t: Template) => void;
}

const TEMPLATE_EXTS = new Set(["md", "txt", "docx"]);

export default function TemplateEditor({ template, onChange }: Props) {
  const [dragging, setDragging] = useState(false);
  const textRef = useRef<HTMLTextAreaElement>(null);

  const loadFile = async (path: string) => {
    const content = await invoke<string>("read_file_as_text", { path });
    onChange({ source: "file", content, sourcePath: path });
  };

  const openFile = async () => {
    const path = await open({
      filters: [{ name: "Document", extensions: ["md", "txt", "docx"] }],
    });
    if (path && typeof path === "string") await loadFile(path);
  };

  const handleDrop = async (e: React.DragEvent) => {
    e.preventDefault();
    setDragging(false);
    const file = e.dataTransfer.files[0];
    if (!file) return;
    const ext = file.name.split(".").pop()?.toLowerCase() ?? "";
    if (!TEMPLATE_EXTS.has(ext)) return;
    const path = (file as File & { path?: string }).path ?? file.name;
    await loadFile(path);
  };

  const filename = (p?: string | null) => p ? p.split(/[\\/]/).pop() ?? p : "";

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between">
        <label className="text-xs font-medium text-white/50 uppercase tracking-wider">
          Format Template
        </label>
        <button
          onClick={openFile}
          className="text-xs text-accent hover:text-accent-hover transition-colors"
        >
          {template.sourcePath ? "Load different file" : "Load from file"}
        </button>
      </div>
      <div
        className={`relative rounded-xl border transition-colors
          ${dragging ? "border-accent bg-accent/10" : "border-white/10"}`}
        onDragOver={(e) => { e.preventDefault(); setDragging(true); }}
        onDragLeave={() => setDragging(false)}
        onDrop={handleDrop}
      >
        {dragging && (
          <div className="absolute inset-0 flex items-center justify-center rounded-xl bg-accent/10 z-10 pointer-events-none">
            <p className="text-accent font-medium">Drop template file here</p>
          </div>
        )}
        <textarea
          ref={textRef}
          className="w-full h-48 bg-surface-secondary rounded-xl p-4 text-sm font-mono text-white/80 resize-y outline-none focus:ring-1 focus:ring-accent/50 placeholder-white/20 scrollable"
          placeholder={"# Meeting Minutes\nDate: [DATE]\n..."}
          value={template.content}
          onChange={(e) =>
            onChange({ source: "inline", content: e.target.value, sourcePath: null })
          }
          spellCheck={false}
        />
      </div>
      {template.sourcePath && (
        <p className="text-xs text-white/30 truncate">
          Loaded: {filename(template.sourcePath)}
        </p>
      )}
    </div>
  );
}
