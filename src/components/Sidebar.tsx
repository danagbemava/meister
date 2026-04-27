import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import type { Project, Recording } from "../types/storage";
import { overallStatus } from "../types/storage";
import { api } from "../api/storage";
import type { Selection } from "../App";
import { Button, Input, ContextMenu } from "./ui";

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

  type RenameTarget =
    | { kind: "project"; id: string; value: string }
    | { kind: "recording"; id: string; value: string }
    | null;

  const [renaming, setRenaming] = useState<RenameTarget>(null);

  const beginRenameProject = (p: Project) => {
    setRenaming({ kind: "project", id: p.id, value: p.name });
  };

  const beginRenameRecording = (r: Recording) => {
    setRenaming({ kind: "recording", id: r.id, value: r.name });
  };

  const cancelRename = () => setRenaming(null);

  const commitRename = async () => {
    if (!renaming) return;
    const trimmed = renaming.value.trim();
    if (!trimmed) {
      setRenaming(null);
      return;
    }
    if (renaming.kind === "project") {
      const project = projects.find((p) => p.id === renaming.id);
      if (project && trimmed !== project.name) {
        await api.updateProject({ ...project, name: trimmed });
        onProjectsChanged();
      }
    } else {
      const all = Object.values(recordingsByProject).flat();
      const rec = all.find((r) => r.id === renaming.id);
      if (rec && trimmed !== rec.name) {
        await api.updateRecording({ ...rec, name: trimmed });
        onProjectsChanged();
      }
    }
    setRenaming(null);
  };

  const deleteProject = async (p: Project) => {
    if (!confirm(`Delete project "${p.name}" and all its recordings?`)) return;
    await api.deleteProject(p.id);
    if (selection.kind === "project" && selection.projectId === p.id) {
      onSelect({ kind: "none" });
    }
    if (selection.kind === "recording" && selection.projectId === p.id) {
      onSelect({ kind: "none" });
    }
    onProjectsChanged();
  };

  const deleteRecording = async (r: Recording) => {
    if (!confirm(`Delete recording "${r.name}"?`)) return;
    await api.deleteRecording(r.id);
    if (selection.kind === "recording" && selection.recordingId === r.id) {
      onSelect({ kind: "none" });
    }
    onProjectsChanged();
  };

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
    <aside className="w-60 shrink-0 rounded-xl bg-sidebar-background flex flex-col">
      <div data-tauri-drag-region className="h-9 shrink-0" />
      <div className="p-3 border-b border-separator">
        {!creating ? (
          <Button
            variant="primary"
            size="md"
            onClick={() => setCreating(true)}
            className="w-full"
          >
            + New project
          </Button>
        ) : (
          <div className="flex gap-2">
            <Input
              autoFocus
              size="sm"
              className="flex-1"
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") createProject();
                if (e.key === "Escape") { setCreating(false); setNewName(""); }
              }}
              placeholder="Project name"
            />
            <Button
              variant="primary"
              size="sm"
              onClick={createProject}
            >
              ✓
            </Button>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => { setCreating(false); setNewName(""); }}
            >
              ✕
            </Button>
          </div>
        )}
      </div>

      <div className="flex-1 overflow-y-auto scrollable p-1">
        {projects.length === 0 && (
          <p className="text-footnote text-label-tertiary p-3">No projects yet.</p>
        )}
        {projects.map((p) => {
          const recs = recordingsByProject[p.id] ?? [];
          const isExpanded = expanded[p.id] ?? true;
          return (
            <div key={p.id}>
              <ContextMenu>
                <ContextMenu.Trigger asChild>
                  <div
                    onClick={() => {
                      onSelect({ kind: "project", projectId: p.id });
                      toggleExpand(p.id);
                    }}
                    className={`w-full text-left px-2 py-1.5 rounded flex items-center gap-1.5 text-body cursor-pointer group
                      ${projectSelected(p.id)
                        ? "bg-accent/10 text-accent"
                        : "hover:bg-fill-tertiary text-label-primary"}`}
                  >
                    <span className="text-label-tertiary text-footnote w-3">{isExpanded ? "▾" : "▸"}</span>
                    {renaming?.kind === "project" && renaming.id === p.id ? (
                      <Input
                        autoFocus
                        size="sm"
                        value={renaming.value}
                        onChange={(e) =>
                          setRenaming({ kind: "project", id: p.id, value: e.target.value })
                        }
                        onBlur={commitRename}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") {
                            e.preventDefault();
                            commitRename();
                          } else if (e.key === "Escape") {
                            e.preventDefault();
                            cancelRename();
                          }
                        }}
                        onClick={(e) => e.stopPropagation()}
                        className="flex-1"
                      />
                    ) : (
                      <span className="truncate flex-1">{p.name}</span>
                    )}
                    <button
                      onClick={(e) => handleAddRecording(e, p.id)}
                      className="text-label-tertiary hover:text-accent opacity-0 group-hover:opacity-100 transition-opacity shrink-0 px-1"
                      title="Add a new recording"
                    >
                      +
                    </button>
                  </div>
                </ContextMenu.Trigger>
                <ContextMenu.Content>
                  <ContextMenu.Item onSelect={() => beginRenameProject(p)}>
                    Rename
                  </ContextMenu.Item>
                  <ContextMenu.Separator />
                  <ContextMenu.Item
                    variant="destructive"
                    onSelect={() => deleteProject(p)}
                  >
                    Delete
                  </ContextMenu.Item>
                </ContextMenu.Content>
              </ContextMenu>
              {isExpanded && recs.map((r) => (
                <ContextMenu key={r.id}>
                  <ContextMenu.Trigger asChild>
                    <button
                      onClick={() =>
                        onSelect({
                          kind: "recording",
                          projectId: p.id,
                          recordingId: r.id,
                        })
                      }
                      className={`w-full text-left pl-7 pr-2 py-1 rounded flex items-center gap-2 text-body
                        ${recordingSelected(r.id)
                          ? "bg-accent/10 text-accent"
                          : "hover:bg-fill-tertiary text-label-secondary"}`}
                    >
                      <StatusDot status={overallStatus(r)} />
                      {renaming?.kind === "recording" && renaming.id === r.id ? (
                        <Input
                          autoFocus
                          size="sm"
                          value={renaming.value}
                          onChange={(e) =>
                            setRenaming({ kind: "recording", id: r.id, value: e.target.value })
                          }
                          onBlur={commitRename}
                          onKeyDown={(e) => {
                            if (e.key === "Enter") {
                              e.preventDefault();
                              commitRename();
                            } else if (e.key === "Escape") {
                              e.preventDefault();
                              cancelRename();
                            }
                          }}
                          onClick={(e) => e.stopPropagation()}
                          className="flex-1"
                        />
                      ) : (
                        <span className="truncate flex-1">{r.name}</span>
                      )}
                    </button>
                  </ContextMenu.Trigger>
                  <ContextMenu.Content>
                    <ContextMenu.Item onSelect={() => beginRenameRecording(r)}>
                      Rename
                    </ContextMenu.Item>
                    <ContextMenu.Separator />
                    <ContextMenu.Item
                      variant="destructive"
                      onSelect={() => deleteRecording(r)}
                    >
                      Delete
                    </ContextMenu.Item>
                  </ContextMenu.Content>
                </ContextMenu>
              ))}
              {isExpanded && recs.length === 0 && (
                <p className="pl-7 py-1 text-footnote text-label-tertiary">No recordings</p>
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
