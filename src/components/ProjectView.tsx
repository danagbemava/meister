import { useCallback, useEffect, useRef, useState } from "react";
import { confirm as dialogConfirm, open as dialogOpen } from "@tauri-apps/plugin-dialog";
import TemplateEditor from "./TemplateEditor";
import RecordCard from "./RecordCard";
import RecordCallCard from "./RecordCallCard";
import StagingList from "./StagingList";
import { api } from "../api/storage";
import { overallStatus } from "../types/storage";
import type { Project, Recording } from "../types/storage";
import { Button, Icon, Input, Textarea, Toolbar } from "./ui";
import { useScrollEdge } from "../lib/useScrollEdge";
import { cn } from "../lib/cn";

interface Props {
  projectId: string;
  onProjectChanged: () => void;
  onRecordingCreated: (r: Recording) => void;
  onSelectRecording: (recordingId: string) => void | Promise<void>;
  initialStagedPaths?: string[];
  onInitialStagedConsumed?: () => void;
  onCaptureActiveChange?: (discard: (() => void) | null) => void;
}

const AUDIO_VIDEO_EXTS = [
  "mp4", "mov", "mkv", "avi", "mp3", "m4a", "wav", "ogg", "webm", "aac", "flac",
];

const AUDIO_VIDEO_EXT_SET = new Set(AUDIO_VIDEO_EXTS);

function isAudioVideo(name: string): boolean {
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  return AUDIO_VIDEO_EXT_SET.has(ext);
}

function basename(p: string): string {
  return p.split(/[\\/]/).pop() ?? p;
}

function deriveName(paths: string[]): string {
  const first = basename(paths[0] ?? "recording");
  return first.replace(/\.[^.]+$/, "");
}

const STAGE_LABELS: Record<string, string> = {
  extract: "Extracting audio",
  transcribe: "Transcribing",
  format: "Formatting",
};

/** Short status line for a recording row. */
function recordingStatus(r: Recording): { text: string; tone: string } {
  const status = overallStatus(r);
  if (status === "completed") {
    return { text: "✓ Transcribed · formatted", tone: "text-status-success" };
  }
  if (status === "failed") {
    return { text: "✗ failed", tone: "text-status-error" };
  }
  if (status === "in_progress") {
    const running = r.stages.find((s) => s.status === "running");
    const label = running ? STAGE_LABELS[running.stage] ?? running.stage : "Processing";
    return { text: `⟳ ${label}…`, tone: "text-accent" };
  }
  return { text: "new", tone: "text-label-tertiary" };
}

/** First non-empty line of the template, for the collapsed-card summary. */
function templateSummary(content: string): string {
  const line = content.split("\n").map((l) => l.trim()).find((l) => l.length > 0);
  return line ?? "";
}

type Affordance = "call" | "audio" | null;

export default function ProjectView({
  projectId,
  onProjectChanged,
  onRecordingCreated,
  onSelectRecording,
  initialStagedPaths,
  onInitialStagedConsumed,
  onCaptureActiveChange,
}: Props) {
  const [project, setProject] = useState<Project | null>(null);
  const [recordings, setRecordings] = useState<Recording[]>([]);
  const [renaming, setRenaming] = useState(false);
  const [draftName, setDraftName] = useState("");
  const [staging, setStaging] = useState<string[]>([]);
  const [stagingName, setStagingName] = useState("");
  // null → auto (resolved from template content once loaded); true/false → user override.
  const [templateOpen, setTemplateOpen] = useState<boolean | null>(null);
  const [affordance, setAffordance] = useState<Affordance>(null);
  const [dragging, setDragging] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  const scrolled = useScrollEdge(scrollRef);

  const refreshRecordings = useCallback(async () => {
    setRecordings(await api.listRecordings(projectId));
  }, [projectId]);

  const load = useCallback(async () => {
    const list = await api.listProjects();
    const p = list.find((x) => x.id === projectId) ?? null;
    setProject(p);
    if (p) setDraftName(p.name);
    await refreshRecordings();
  }, [projectId, refreshRecordings]);

  useEffect(() => { load(); }, [load]);

  // Clear staging + transient affordance when switching projects.
  useEffect(() => {
    setStaging([]);
    setStagingName("");
    setTemplateOpen(null);
    setAffordance(null);
  }, [projectId]);

  const save = async (next: Project) => {
    await api.updateProject(next);
    setProject(next);
    onProjectChanged();
  };

  const onFilesPicked = (picked: string[]) => {
    setStaging((prev) => {
      const seen = new Set(prev);
      const next = [...prev];
      for (const p of picked) {
        if (!seen.has(p)) {
          next.push(p);
          seen.add(p);
        }
      }
      // Auto-fill the name on first files dropped, but don't overwrite a user edit.
      if (prev.length === 0 && next.length > 0) {
        setStagingName(deriveName(next));
      }
      return next;
    });
  };

  // Merge in paths dropped from the sidebar "+" button.
  useEffect(() => {
    if (!initialStagedPaths || initialStagedPaths.length === 0) return;
    onFilesPicked(initialStagedPaths);
    onInitialStagedConsumed?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [initialStagedPaths]);

  const removeAt = (idx: number) =>
    setStaging((prev) => prev.filter((_, i) => i !== idx));

  const cancelStaging = () => {
    setStaging([]);
    setStagingName("");
  };

  const createFromStaging = async () => {
    if (!project || staging.length === 0) return;
    const name = stagingName.trim() || deriveName(staging);
    const rec = await api.createRecording(projectId, name, staging);
    setStaging([]);
    setStagingName("");
    setRecordings((rs) => [...rs, rec]);
    onRecordingCreated(rec);
  };

  const importFiles = async () => {
    const paths = await dialogOpen({
      multiple: true,
      filters: [{ name: "Audio / Video", extensions: AUDIO_VIDEO_EXTS }],
    });
    if (!paths) return;
    const list = Array.isArray(paths) ? paths : [paths];
    if (list.length > 0) onFilesPicked(list);
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
    if (picked.length > 0) onFilesPicked(picked);
  };

  const deleteProject = async () => {
    const ok = await dialogConfirm(
      `Delete '${project?.name}' and its ${recordings.length} recordings? This cannot be undone.`,
      { title: "Delete project", kind: "warning" },
    );
    if (!ok) return;
    await api.deleteProject(projectId);
    onProjectChanged();
  };

  if (!project) return <div className="p-8 text-label-tertiary">Loading…</div>;

  const hasTemplate = project.template.content.trim().length > 0;
  // Resolve auto state: configured → collapsed, empty → expanded.
  const isOpen = templateOpen ?? !hasTemplate;
  const summary = templateSummary(project.template.content);
  const matchStyleOn = project.template.matchStyle ?? false;

  const metaParts = [
    `${recordings.length} recording${recordings.length === 1 ? "" : "s"}`,
    `template ${hasTemplate ? "configured" : "not set"}`,
  ];
  if (project.updatedAt) {
    metaParts.push(`updated ${new Date(project.updatedAt).toLocaleDateString()}`);
  }

  return (
    <div className="h-full flex flex-col">
      <Toolbar data-tauri-drag-region scrolled={scrolled}>
        {renaming ? (
          <Input
            autoFocus
            size="sm"
            className="font-semibold"
            value={draftName}
            onChange={(e) => setDraftName(e.target.value)}
            onBlur={async () => {
              const name = draftName.trim() || project.name;
              setRenaming(false);
              if (name !== project.name) await save({ ...project, name });
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") (e.target as HTMLInputElement).blur();
              if (e.key === "Escape") { setDraftName(project.name); setRenaming(false); }
            }}
          />
        ) : (
          <h1
            onClick={() => setRenaming(true)}
            className="text-body font-semibold cursor-text hover:text-accent truncate"
            title="Click to rename"
          >
            {project.name}
          </h1>
        )}
        <Toolbar.Spacer />
      </Toolbar>
      <div ref={scrollRef} className="flex-1 overflow-y-auto scrollable p-8 flex flex-col gap-8">
        {/* Editorial header */}
        <header className="flex flex-col gap-1">
          <h2 className="text-[22px] font-display font-semibold leading-tight truncate">
            {project.name}
          </h2>
          <p className="text-callout italic text-label-tertiary">
            {metaParts.join(" · ")}
          </p>
        </header>

        {/* Collapsible template card */}
        <section className="bg-control-background border border-separator rounded-card shadow-card overflow-hidden">
          <button
            type="button"
            onClick={() => setTemplateOpen(!isOpen)}
            aria-expanded={isOpen}
            className="w-full flex items-center gap-2 px-4 py-3 text-left hover:bg-fill-tertiary transition-colors"
          >
            <Icon
              name="chevron-right"
              size="sm"
              className={cn(
                "text-label-tertiary transition-transform shrink-0",
                isOpen && "rotate-90",
              )}
            />
            <span className="text-footnote font-semibold uppercase tracking-wider text-label-secondary">
              Format template
            </span>
            <span
              className={cn(
                "text-footnote font-medium",
                hasTemplate ? "text-accent" : "text-label-tertiary",
              )}
            >
              · {hasTemplate ? "configured" : "not set"}
            </span>
            {!isOpen && hasTemplate && (
              <span className="flex-1 truncate text-footnote text-label-tertiary italic">
                {summary}
                {matchStyleOn ? " · match style ON" : ""}
              </span>
            )}
          </button>
          {isOpen && (
            <div className="px-4 pb-4 pt-1 border-t border-separator">
              <TemplateEditor
                hideHeader
                template={project.template}
                onChange={(template) => save({ ...project, template })}
              />
            </div>
          )}
        </section>

        <div className="flex flex-col gap-2">
          <label className="text-footnote font-medium text-label-secondary uppercase tracking-wider">
            Additional Instructions
          </label>
          <Textarea
            className="h-24 p-3 scrollable"
            placeholder="e.g., Use British English. Keep under 500 words. Skip empty sections."
            value={project.customInstructions ?? ""}
            onChange={(e) =>
              save({ ...project, customInstructions: e.target.value })
            }
            spellCheck
          />
        </div>

        <div className="flex flex-col gap-2">
          <label className="text-footnote font-medium text-label-secondary uppercase tracking-wider">
            Language override (optional)
          </label>
          <Input
            value={project.language ?? ""}
            onChange={(e) =>
              save({ ...project, language: e.target.value.trim() === "" ? null : e.target.value })
            }
            placeholder="Leave empty to use the global default"
          />
        </div>

        {/* Recordings — drop target wraps the whole section */}
        <section
          onDragOver={(e) => { e.preventDefault(); setDragging(true); }}
          onDragLeave={() => setDragging(false)}
          onDrop={handleDrop}
          className={cn(
            "flex flex-col gap-3 rounded-card -m-1 p-1 transition-colors",
            dragging && "border-2 border-dashed border-accent bg-accent/10",
          )}
        >
          <h2 className="text-footnote font-semibold text-label-tertiary uppercase tracking-wider">
            Recordings
          </h2>

          {recordings.length > 0 ? (
            <ul className="flex flex-col gap-2">
              {recordings.map((r) => {
                const st = recordingStatus(r);
                return (
                  <li key={r.id}>
                    <button
                      type="button"
                      onClick={() => void onSelectRecording(r.id)}
                      className="w-full flex items-center gap-3 text-left bg-control-background border border-separator rounded-card shadow-card px-4 py-3 hover:bg-fill-tertiary transition-colors"
                    >
                      <Icon name="recording" size="md" className="text-label-tertiary shrink-0" />
                      <span className="flex-1 min-w-0">
                        <span className="block font-semibold text-label-primary truncate">
                          {r.name}
                        </span>
                        <span className={cn("block text-footnote", st.tone)}>
                          {st.text}
                        </span>
                      </span>
                      <span className="shrink-0 bg-fill-tertiary rounded-full px-2 py-0.5 text-caption-1 text-label-secondary">
                        {new Date(r.createdAt).toLocaleDateString()}
                      </span>
                    </button>
                  </li>
                );
              })}
            </ul>
          ) : (
            <p className="text-footnote italic text-label-tertiary">
              No recordings yet — record a call or drop a file below.
            </p>
          )}

          {staging.length > 0 && (
            <StagingList
              paths={staging}
              onReorder={setStaging}
              onRemove={removeAt}
              onCancel={cancelStaging}
              onCreate={createFromStaging}
              name={stagingName}
              onNameChange={setStagingName}
            />
          )}

          {/* Active capture surfaces, revealed by the pills below */}
          {affordance === "call" && (
            <RecordCallCard
              onPicked={(paths) => { onFilesPicked(paths); setAffordance(null); }}
              onActiveChange={onCaptureActiveChange}
            />
          )}
          {affordance === "audio" && (
            <RecordCard
              autoStart
              onPicked={(paths) => { onFilesPicked(paths); setAffordance(null); }}
              onActiveChange={onCaptureActiveChange}
              onDone={() => setAffordance(null)}
            />
          )}

          {/* Pill actions */}
          {affordance === null && (
            <div className="flex flex-wrap gap-2">
              <Button variant="primary" size="md" onClick={() => setAffordance("call")}>
                ● Record call
              </Button>
              <Button variant="secondary" size="md" onClick={() => setAffordance("audio")}>
                Record audio
              </Button>
              <Button variant="secondary" size="md" onClick={() => void importFiles()}>
                Import files…
              </Button>
            </div>
          )}
        </section>

        {/* Quiet, low-emphasis destructive action at the very bottom */}
        <div className="pt-2">
          <Button
            variant="ghost"
            size="sm"
            onClick={deleteProject}
            className="text-status-error hover:bg-status-error/10"
          >
            Delete project
          </Button>
        </div>
      </div>
    </div>
  );
}
