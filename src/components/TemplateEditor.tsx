import { useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { Template } from "../types/storage";
import { Button, Checkbox, Textarea } from "./ui";

interface Props {
  template: Template;
  onChange: (t: Template) => void;
  /** When the editor is mounted inside the collapsible template card, the card
   *  header already shows the "Format template" label, so the editor's own
   *  header row is redundant and is hidden. */
  hideHeader?: boolean;
}

const TEMPLATE_EXTS = new Set(["md", "txt", "docx"]);

export default function TemplateEditor({ template, onChange, hideHeader }: Props) {
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
        {hideHeader ? (
          <span />
        ) : (
          <label className="text-footnote font-medium text-label-secondary uppercase tracking-wider">
            Format Template
          </label>
        )}
        <Button
          variant="ghost"
          size="sm"
          onClick={openFile}
          className="text-footnote text-accent hover:text-accent-hover hover:bg-transparent"
        >
          {template.sourcePath ? "Load different file" : "Load from file"}
        </Button>
      </div>
      <div
        className="relative"
        onDragOver={(e) => { e.preventDefault(); setDragging(true); }}
        onDragLeave={() => setDragging(false)}
        onDrop={handleDrop}
      >
        {dragging && (
          <div className="absolute inset-0 flex items-center justify-center rounded-control bg-accent/10 border-2 border-accent z-10 pointer-events-none">
            <p className="text-accent font-medium">Drop template file here</p>
          </div>
        )}
        <Textarea
          ref={textRef}
          className="h-48 p-4 font-mono scrollable"
          placeholder={"# Meeting Minutes\nDate: [DATE]\n..."}
          value={template.content}
          onChange={(e) =>
            onChange({
              source: "inline",
              content: e.target.value,
              sourcePath: null,
              matchStyle: template.matchStyle,
            })
          }
          spellCheck={false}
        />
      </div>
      <div className="mt-1">
        <label className="flex items-center gap-2 text-footnote text-label-secondary select-none">
          <Checkbox
            size="sm"
            checked={template.matchStyle ?? false}
            onCheckedChange={(c) =>
              onChange({ ...template, matchStyle: Boolean(c) })
            }
          />
          Match writing style of this template
        </label>
        <p className="text-footnote text-label-tertiary ml-6">
          When enabled, the LLM also mirrors the template's tone and voice in the output.
        </p>
      </div>
      {template.sourcePath && (
        <p className="text-footnote text-label-tertiary truncate">
          Loaded: {filename(template.sourcePath)}
        </p>
      )}
    </div>
  );
}
