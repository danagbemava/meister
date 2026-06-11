import { useCallback, useEffect, useRef, useState } from "react";
import { confirm as dialogConfirm } from "@tauri-apps/plugin-dialog";
import TemplateEditor from "./TemplateEditor";
import RecordingDropZone from "./RecordingDropZone";
import RecordCard from "./RecordCard";
import RecordCallCard from "./RecordCallCard";
import StagingList from "./StagingList";
import { api } from "../api/storage";
import type { Project, Recording } from "../types/storage";
import { Button, Input, Textarea, Toolbar } from "./ui";
import { useScrollEdge } from "../lib/useScrollEdge";

interface Props {
  projectId: string;
  onProjectChanged: () => void;
  onRecordingCreated: (r: Recording) => void;
  onSelectRecording: (recordingId: string) => void | Promise<void>;
  initialStagedPaths?: string[];
  onInitialStagedConsumed?: () => void;
  onCaptureActiveChange?: (discard: (() => void) | null) => void;
}

function basename(p: string): string {
  return p.split(/[\\/]/).pop() ?? p;
}

function deriveName(paths: string[]): string {
  const first = basename(paths[0] ?? "recording");
  return first.replace(/\.[^.]+$/, "");
}

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
  const scrollRef = useRef<HTMLDivElement>(null);
  const scrolled = useScrollEdge(scrollRef);

  const load = useCallback(async () => {
    const list = await api.listProjects();
    const p = list.find((x) => x.id === projectId) ?? null;
    setProject(p);
    if (p) setDraftName(p.name);
    const recs = await api.listRecordings(projectId);
    setRecordings(recs);
  }, [projectId]);

  useEffect(() => { load(); }, [load]);

  // Clear staging when switching projects.
  useEffect(() => {
    setStaging([]);
    setStagingName("");
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
        <Button
          variant="ghost"
          size="sm"
          onClick={deleteProject}
          className="text-status-error"
        >
          Delete project
        </Button>
      </Toolbar>
      <div ref={scrollRef} className="flex-1 overflow-y-auto scrollable p-8 flex flex-col gap-8">
      <TemplateEditor
        template={project.template}
        onChange={(template) => save({ ...project, template })}
      />

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

      <div>
        <h2 className="text-footnote font-semibold text-label-tertiary uppercase tracking-wider mb-3">
          Recordings
        </h2>
        <div className="flex flex-col gap-3">
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
            <RecordingDropZone onPicked={onFilesPicked} />
            <RecordCard
              onPicked={onFilesPicked}
              onActiveChange={onCaptureActiveChange}
            />
            <RecordCallCard
              onPicked={onFilesPicked}
              onActiveChange={onCaptureActiveChange}
            />
          </div>
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
          {recordings.length > 0 && (
            <ul className="flex flex-col gap-1">
              {recordings.map((r) => (
                <li key={r.id}>
                  <button
                    type="button"
                    onClick={() => void onSelectRecording(r.id)}
                    className="w-full text-left text-body text-label-secondary px-3 py-1.5 rounded hover:bg-fill-tertiary cursor-pointer"
                  >
                    {r.name}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
      </div>
    </div>
  );
}
