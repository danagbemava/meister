import { useCallback, useEffect, useState } from "react";
import TemplateEditor from "./TemplateEditor";
import SettingsForm from "./SettingsForm";
import RecordingDropZone from "./RecordingDropZone";
import { api } from "../api/storage";
import type { Project, Recording } from "../types/storage";

interface Props {
  projectId: string;
  onProjectChanged: () => void;
  onRecordingCreated: (r: Recording) => void;
}

export default function ProjectView({
  projectId,
  onProjectChanged,
  onRecordingCreated,
}: Props) {
  const [project, setProject] = useState<Project | null>(null);
  const [recordings, setRecordings] = useState<Recording[]>([]);
  const [renaming, setRenaming] = useState(false);
  const [draftName, setDraftName] = useState("");

  const load = useCallback(async () => {
    const list = await api.listProjects();
    const p = list.find((x) => x.id === projectId) ?? null;
    setProject(p);
    if (p) setDraftName(p.name);
    const recs = await api.listRecordings(projectId);
    setRecordings(recs);
  }, [projectId]);

  useEffect(() => { load(); }, [load]);

  const save = async (next: Project) => {
    await api.updateProject(next);
    setProject(next);
    onProjectChanged();
  };

  const addRecording = async (sourcePath: string) => {
    if (!project) return;
    const filename = sourcePath.split(/[\\/]/).pop() ?? sourcePath;
    const name = filename.replace(/\.[^.]+$/, "");
    const rec = await api.createRecording(
      projectId,
      name,
      sourcePath,
      project.defaultSettings,
    );
    setRecordings((rs) => [...rs, rec]);
    onRecordingCreated(rec);
  };

  const deleteProject = async () => {
    if (!confirm(`Delete '${project?.name}' and its ${recordings.length} recordings? This cannot be undone.`)) return;
    await api.deleteProject(projectId);
    onProjectChanged();
  };

  if (!project) return <div className="p-8 text-white/40">Loading…</div>;

  return (
    <div className="h-full overflow-y-auto scrollable p-8 flex flex-col gap-8">
      <div className="flex items-center justify-between">
        {renaming ? (
          <input
            autoFocus
            className="input text-xl font-semibold"
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
            className="text-xl font-semibold cursor-text hover:text-accent"
            title="Click to rename"
          >
            {project.name}
          </h1>
        )}
        <button
          onClick={deleteProject}
          className="text-xs text-white/40 hover:text-red-400"
        >
          Delete project
        </button>
      </div>

      <TemplateEditor
        template={project.template}
        onChange={(template) => save({ ...project, template })}
      />

      <div>
        <h2 className="text-xs font-semibold text-white/40 uppercase tracking-wider mb-3">
          Default settings
        </h2>
        <SettingsForm
          settings={project.defaultSettings}
          onChange={(defaultSettings) => save({ ...project, defaultSettings })}
        />
      </div>

      <div>
        <h2 className="text-xs font-semibold text-white/40 uppercase tracking-wider mb-3">
          Recordings
        </h2>
        <div className="flex flex-col gap-3">
          {recordings.length > 0 && (
            <ul className="flex flex-col gap-1">
              {recordings.map((r) => (
                <li key={r.id} className="text-sm text-white/70 px-3 py-1.5 bg-surface-secondary rounded">
                  {r.name}
                </li>
              ))}
            </ul>
          )}
          <RecordingDropZone onPicked={addRecording} />
        </div>
      </div>
    </div>
  );
}
