import { useState } from "react";
import { Button, Icon, Input } from "./ui";

interface Props {
  paths: string[];
  onReorder: (next: string[]) => void;
  onRemove: (index: number) => void;
  onCancel: () => void;
  onCreate: () => void;
  name: string;
  onNameChange: (name: string) => void;
}

function basename(p: string): string {
  return p.split(/[\\/]/).pop() ?? p;
}

export default function StagingList({
  paths,
  onReorder,
  onRemove,
  onCancel,
  onCreate,
  name,
  onNameChange,
}: Props) {
  const [draggingIdx, setDraggingIdx] = useState<number | null>(null);
  const [overIdx, setOverIdx] = useState<number | null>(null);

  const handleDragStart = (idx: number) => (e: React.DragEvent) => {
    setDraggingIdx(idx);
    e.dataTransfer.effectAllowed = "move";
  };

  const handleDragOver = (idx: number) => (e: React.DragEvent) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
    if (idx !== overIdx) setOverIdx(idx);
  };

  const handleDrop = (dropIdx: number) => (e: React.DragEvent) => {
    e.preventDefault();
    if (draggingIdx === null || draggingIdx === dropIdx) {
      setDraggingIdx(null);
      setOverIdx(null);
      return;
    }
    const next = [...paths];
    const [moved] = next.splice(draggingIdx, 1);
    next.splice(dropIdx, 0, moved);
    onReorder(next);
    setDraggingIdx(null);
    setOverIdx(null);
  };

  const handleDragEnd = () => {
    setDraggingIdx(null);
    setOverIdx(null);
  };

  return (
    <div className="rounded-xl border border-separator bg-control-background p-3 flex flex-col gap-2">
      <ul className="flex flex-col">
        {paths.map((p, idx) => (
          <li
            key={`${p}-${idx}`}
            draggable
            onDragStart={handleDragStart(idx)}
            onDragOver={handleDragOver(idx)}
            onDrop={handleDrop(idx)}
            onDragEnd={handleDragEnd}
            className={`flex items-center gap-2 px-2 py-1.5 rounded text-body
              ${overIdx === idx ? "bg-accent/10" : ""}
              ${draggingIdx === idx ? "opacity-40" : ""}`}
          >
            <span className="text-label-tertiary cursor-grab select-none" title="Drag to reorder">⋮⋮</span>
            <span className="text-label-tertiary tabular-nums w-5 text-right">{idx + 1}.</span>
            <span className="flex-1 truncate text-label-primary" title={p}>
              {basename(p)}
            </span>
            <button
              type="button"
              onClick={() => onRemove(idx)}
              aria-label="Remove from staging"
              title="Remove"
              className="w-6 h-6 inline-flex items-center justify-center rounded-full text-label-tertiary hover:bg-fill-tertiary hover:text-status-error transition-colors"
            >
              <Icon name="x" size="sm" />
            </button>
          </li>
        ))}
      </ul>
      <div className="flex items-center gap-2 pt-2 border-t border-separator">
        <label className="text-footnote text-label-secondary shrink-0">Name</label>
        <Input
          size="sm"
          className="flex-1"
          value={name}
          onChange={(e) => onNameChange(e.target.value)}
          placeholder="Recording name"
        />
      </div>
      <div className="flex items-center justify-end gap-2">
        <Button onClick={onCancel} variant="ghost" size="sm">
          Cancel
        </Button>
        <Button
          onClick={onCreate}
          disabled={paths.length === 0}
          variant="primary"
          size="sm"
        >
          Create
        </Button>
      </div>
    </div>
  );
}
