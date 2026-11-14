import { useEffect, useRef, useState } from "react";
import { Button, SegmentedControl } from "./ui";
import Notes from "./Notes";

interface Props {
  transcript?: string | null;
  formatted?: string | null;
  highlightRange?: { start: number; end: number; nonce?: number };
  /** If provided, the formatted view becomes an editable textarea controlled
   *  by the parent. Each keystroke fires this. The parent decides when to
   *  persist (typically on blur). */
  onFormattedChange?: (next: string) => void;
  /** Hides the formatted-editing affordance when true (e.g. while the format
   *  stage is still running). */
  formattedReadOnly?: boolean;
}

export default function OutputPreview({
  transcript,
  formatted,
  highlightRange,
  onFormattedChange,
  formattedReadOnly,
}: Props) {
  const has = (v?: string | null) => (v ?? "").length > 0;
  const hasFormatted = has(formatted);
  const hasTranscript = has(transcript);
  const [tab, setTab] = useState<"formatted" | "transcript">(
    hasFormatted ? "formatted" : "transcript",
  );
  const [editing, setEditing] = useState(false);
  const canEdit = !!onFormattedChange && !formattedReadOnly;

  useEffect(() => {
    if (tab === "formatted" && !hasFormatted && hasTranscript) setTab("transcript");
    else if (tab === "transcript" && !hasTranscript && hasFormatted) setTab("formatted");
  }, [tab, hasFormatted, hasTranscript]);

  // When a citation comes in, force the transcript tab and scroll to it.
  const highlightRef = useRef<HTMLSpanElement | null>(null);
  const [flash, setFlash] = useState(false);
  useEffect(() => {
    if (!highlightRange) return;
    setTab("transcript");
    setFlash(true);
    const t = window.setTimeout(() => setFlash(false), 1200);
    // Defer scroll until after layout in case the tab just changed.
    const r = window.requestAnimationFrame(() => {
      highlightRef.current?.scrollIntoView({ block: "center", behavior: "smooth" });
    });
    return () => {
      window.clearTimeout(t);
      window.cancelAnimationFrame(r);
    };
  }, [highlightRange?.start, highlightRange?.end, highlightRange?.nonce]);

  if (!hasTranscript && !hasFormatted) return null;

  const renderTranscript = () => {
    const text = transcript ?? "";
    if (!highlightRange || tab !== "transcript") return text;
    const { start, end } = highlightRange;
    if (start < 0 || end > text.length || start >= end) return text;
    return (
      <>
        {text.slice(0, start)}
        <span
          ref={highlightRef}
          className={
            flash
              ? "bg-status-info/30 transition-colors duration-300"
              : "bg-status-info/10 transition-colors duration-300"
          }
          data-testid="ask-citation-highlight"
        >
          {text.slice(start, end)}
        </span>
        {text.slice(end)}
      </>
    );
  };

  const frame = "flex-1 min-h-0 border border-separator rounded-card overflow-y-auto scrollable";
  const surface = `${frame} bg-control-background shadow-card`;

  return (
    <div className="flex-1 min-h-0 flex flex-col gap-3">
      <div className="flex items-center gap-2">
        <SegmentedControl
          size="sm"
          value={tab}
          onValueChange={(v) => setTab(v as "formatted" | "transcript")}
          aria-label="Output view"
        >
          <SegmentedControl.Item value="formatted" disabled={!hasFormatted}>
            Notes
          </SegmentedControl.Item>
          <SegmentedControl.Item value="transcript" disabled={!hasTranscript}>
            Transcript
          </SegmentedControl.Item>
        </SegmentedControl>
        {tab === "formatted" && canEdit && (
          <Button
            variant="ghost"
            size="sm"
            className="ml-auto"
            iconLeft={editing ? "check" : undefined}
            onClick={() => setEditing((v) => !v)}
          >
            {editing ? "Done" : "Edit"}
          </Button>
        )}
      </div>
      {tab === "formatted" ? (
        canEdit && editing ? (
          <textarea
            autoFocus
            value={formatted ?? ""}
            onChange={(e) => onFormattedChange!(e.target.value)}
            spellCheck
            className={`${surface} px-6 py-5 text-body leading-relaxed text-label-primary resize-none outline-none focus-visible:ring-2 focus-visible:ring-focus-ring focus-visible:border-accent`}
            aria-label="Edit notes (markdown)"
          />
        ) : (
          <div className={`${surface} px-8 py-7`}>
            <Notes markdown={formatted ?? ""} />
          </div>
        )
      ) : (
        <div className={`${frame} bg-fill-quaternary px-8 py-7`}>
          <p className="max-w-[75ch] text-body leading-relaxed text-label-primary whitespace-pre-wrap">
            {renderTranscript()}
          </p>
        </div>
      )}
    </div>
  );
}
