import { useEffect, useState } from "react";
import { SegmentedControl } from "./ui";

interface Props {
  transcript?: string | null;
  formatted?: string | null;
  savedPaths?: string[] | null;
}

export default function OutputPreview({ transcript, formatted, savedPaths }: Props) {
  const has = (v?: string | null) => (v ?? "").length > 0;
  const hasFormatted = has(formatted);
  const hasTranscript = has(transcript);
  const [tab, setTab] = useState<"formatted" | "transcript">(
    hasFormatted ? "formatted" : "transcript",
  );

  // If the selected tab loses its content (e.g. user re-runs Format and
  // it fails), or the other tab gains content, snap to the populated one.
  useEffect(() => {
    if (tab === "formatted" && !hasFormatted && hasTranscript) setTab("transcript");
    else if (tab === "transcript" && !hasTranscript && hasFormatted) setTab("formatted");
  }, [tab, hasFormatted, hasTranscript]);

  if (!hasTranscript && !hasFormatted) return null;

  return (
    <div className="flex flex-col gap-3">
      <SegmentedControl
        size="sm"
        value={tab}
        onValueChange={(v) => setTab(v as "formatted" | "transcript")}
        aria-label="Output view"
      >
        <SegmentedControl.Item value="formatted" disabled={!hasFormatted}>
          Formatted
        </SegmentedControl.Item>
        <SegmentedControl.Item value="transcript" disabled={!hasTranscript}>
          Transcript
        </SegmentedControl.Item>
      </SegmentedControl>
      <pre className="bg-control-background border border-separator rounded-xl p-4 text-callout font-mono text-label-primary whitespace-pre-wrap max-h-96 overflow-y-auto scrollable">
        {tab === "formatted" ? (formatted ?? "") : (transcript ?? "")}
      </pre>
      {savedPaths && savedPaths.length > 0 && (
        <div className="text-footnote text-label-secondary">
          Saved to:
          <ul className="mt-1">
            {savedPaths.map((p) => <li key={p} className="truncate">· {p}</li>)}
          </ul>
        </div>
      )}
    </div>
  );
}
