import { useState } from "react";
import { Input } from "./ui";
import { focusClasses } from "./ui/_shared";
import { cn } from "../lib/cn";

interface Props {
  value: string;
  onRename: (name: string) => void;
  className?: string;
}

export default function EditableTitle({ value, onRename, className }: Props) {
  const [draft, setDraft] = useState<string | null>(null);

  if (draft !== null) {
    const commit = () => {
      const name = draft.trim();
      setDraft(null);
      if (name && name !== value) onRename(name);
    };
    return (
      <Input
        autoFocus
        size="sm"
        aria-label="Name"
        className={cn("flex-1", className)}
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") e.currentTarget.blur();
          if (e.key === "Escape") setDraft(null);
        }}
      />
    );
  }

  return (
    <button
      type="button"
      onClick={() => setDraft(value)}
      title="Rename"
      className={cn(
        "min-w-0 truncate text-left rounded-field cursor-text hover:text-accent transition-colors",
        focusClasses,
        className,
      )}
    >
      {value}
    </button>
  );
}
