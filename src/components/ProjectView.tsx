import { useCallback, useEffect, useState } from "react";
import TemplateEditor from "./TemplateEditor";
import SettingsForm from "./SettingsForm";
import RecordingDropZone from "./RecordingDropZone";
import RecordCard from "./RecordCard";
import RecordCallCard from "./RecordCallCard";
import StagingList from "./StagingList";
import { api } from "../api/storage";
import type { Project, Recording } from "../types/storage";

interface Props {
  projectId: string;
  onProjectChanged: () => void;
  onRecordingCreated: (r: Recording) => void;
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
    const rec = await api.createRecording(
      projectId,
      name,
      staging,
      project.defaultSettings,
    );
    setStaging([]);
    setStagingName("");
    setRecordings((rs) => [...rs, rec]);
    onRecordingCreated(rec);
  };

  const deleteProject = async () => {
    if (!confirm(`Delete '${project?.name}' and its ${recordings.length} recordings? This cannot be undone.`)) return;
    await api.deleteProject(projectId);
    onProjectChanged();
  };

  if (!project) return <div className="p-8 text-label-tertiary">Loading…</div>;

  return (
    <div className="h-full overflow-y-auto scrollable p-8 flex flex-col gap-8 bg-window-background">
      <div className="flex items-center justify-between">
        {renaming ? (
          <input
            autoFocus
            className="input text-title-2 font-semibold"
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
            className="text-title-2 font-semibold cursor-text hover:text-accent"
            title="Click to rename"
          >
            {project.name}
          </h1>
        )}
        <button
          onClick={deleteProject}
          className="text-footnote text-label-tertiary hover:text-status-error"
        >
          Delete project
        </button>
      </div>

      <TemplateEditor
        template={project.template}
        onChange={(template) => save({ ...project, template })}
      />

      <div>
        <h2 className="text-footnote font-semibold text-label-tertiary uppercase tracking-wider mb-3">
          Default settings
        </h2>
        <SettingsForm
          settings={project.defaultSettings}
          onChange={(defaultSettings) => save({ ...project, defaultSettings })}
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
                <li key={r.id} className="text-body text-label-secondary px-3 py-1.5 bg-control-background rounded hover:bg-fill-tertiary">
                  {r.name}
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
    </div>
  );
}
