import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import type { Project, Recording } from "../types/storage";
import { overallStatus } from "../types/storage";
import { api } from "../api/storage";
import type { Selection } from "../App";

interface Props {
  projects: Project[];
  recordingsByProject: Record<string, Recording[]>;
  selection: Selection;
  onSelect: (s: Selection) => void;
  onProjectsChanged: () => void;
  onPickFilesForProject: (projectId: string, paths: string[]) => void;
}

const AUDIO_VIDEO_EXTS = [
  "mp4", "mov", "mkv", "avi", "mp3", "m4a", "wav", "ogg", "webm", "aac", "flac",
];

const DEFAULTS = {
  ollamaUrl: "http://localhost:11434",
  model: "gemma3",
  whisperModel: "base",
  language: "auto",
  outputFormats: ["md"] as const,
};

export default function Sidebar({
  projects,
  recordingsByProject,
  selection,
  onSelect,
  onProjectsChanged,
  onPickFilesForProject,
}: Props) {
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [creating, setCreating] = useState(false);
  const [newName, setNewName] = useState("");

  const toggleExpand = (id: string) =>
    setExpanded((e) => ({ ...e, [id]: !e[id] }));

  const createProject = async () => {
    const name = newName.trim();
    if (!name) return;
    const p = await api.createProject(
      name,
      {
        ollamaUrl: DEFAULTS.ollamaUrl,
        model: DEFAULTS.model,
        whisperModel: DEFAULTS.whisperModel,
        language: DEFAULTS.language,
        outputFormats: [...DEFAULTS.outputFormats],
      },
      { source: "inline", content: "", sourcePath: null },
    );
    setNewName("");
    setCreating(false);
    onProjectsChanged();
    onSelect({ kind: "project", projectId: p.id });
    setExpanded((e) => ({ ...e, [p.id]: true }));
  };

  const projectSelected = (id: string) =>
    selection.kind === "project" && selection.projectId === id;
  const recordingSelected = (id: string) =>
    selection.kind === "recording" && selection.recordingId === id;

  const handleAddRecording = async (e: React.MouseEvent, projectId: string) => {
    e.stopPropagation();
    const picked = await open({
      multiple: true,
      filters: [{ name: "Audio / Video", extensions: AUDIO_VIDEO_EXTS }],
    });
    if (!picked) return;
    const list = Array.isArray(picked) ? picked : [picked];
    if (list.length === 0) return;
    onPickFilesForProject(projectId, list);
  };

  return (
    <aside className="w-60 shrink-0 border-r border-separator bg-sidebar-background flex flex-col">
      <div className="p-3 border-b border-separator">
        {!creating ? (
          <button
            onClick={() => setCreating(true)}
            className="w-full px-3 py-2 rounded-lg text-sm bg-accent/10 hover:bg-accent/20 text-accent"
          >
            + New project
          </button>
        ) : (
          <div className="flex gap-2">
            <input
              autoFocus
              className="input flex-1"
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") createProject();
                if (e.key === "Escape") { setCreating(false); setNewName(""); }
              }}
              placeholder="Project name"
            />
            <button
              onClick={createProject}
              className="px-2 py-1 text-xs rounded bg-accent text-label-primary"
            >
              ✓
            </button>
          </div>
        )}
      </div>

      <div className="flex-1 overflow-y-auto scrollable p-1">
        {projects.length === 0 && (
          <p className="text-xs text-label-tertiary p-3">No projects yet.</p>
        )}
        {projects.map((p) => {
          const recs = recordingsByProject[p.id] ?? [];
          const isExpanded = expanded[p.id] ?? true;
          return (
            <div key={p.id}>
              <div
                onClick={() => {
                  onSelect({ kind: "project", projectId: p.id });
                  toggleExpand(p.id);
                }}
                className={`w-full text-left px-2 py-1.5 rounded flex items-center gap-1.5 text-sm cursor-pointer group
                  ${projectSelected(p.id)
                    ? "bg-accent/10 text-accent"
                    : "hover:bg-fill-tertiary text-label-primary"}`}
              >
                <span className="text-label-tertiary text-xs w-3">{isExpanded ? "▾" : "▸"}</span>
                <span className="truncate flex-1">{p.name}</span>
                <button
                  onClick={(e) => handleAddRecording(e, p.id)}
                  className="text-label-tertiary hover:text-accent opacity-0 group-hover:opacity-100 transition-opacity shrink-0 px-1"
                  title="Add a new recording"
                >
                  +
                </button>
              </div>
              {isExpanded && recs.map((r) => (
                <button
                  key={r.id}
                  onClick={() =>
                    onSelect({ kind: "recording", projectId: p.id, recordingId: r.id })
                  }
                  className={`w-full text-left pl-7 pr-2 py-1 rounded flex items-center gap-2 text-sm
                    ${recordingSelected(r.id)
                      ? "bg-accent/10 text-accent"
                      : "hover:bg-fill-tertiary text-label-secondary"}`}
                >
                  <StatusDot status={overallStatus(r)} />
                  <span className="truncate flex-1">{r.name}</span>
                </button>
              ))}
              {isExpanded && recs.length === 0 && (
                <p className="pl-7 py-1 text-xs text-label-tertiary">No recordings</p>
              )}
            </div>
          );
        })}
      </div>
    </aside>
  );
}

function StatusDot({ status }: { status: "new" | "in_progress" | "completed" | "failed" }) {
  const color =
    status === "failed" ? "bg-status-error"
    : status === "completed" ? "bg-status-success"
    : status === "in_progress" ? "bg-accent"
    : "bg-fill-tertiary";
  return <span className={`w-1.5 h-1.5 rounded-full shrink-0 ${color}`} />;
}
