import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { confirm as dialogConfirm } from "@tauri-apps/plugin-dialog";
import type { Project, Recording } from "../types/storage";
import { api } from "../api/storage";
import type { Selection } from "../App";
import { Button, ContextMenu, Icon, Input } from "./ui";
import { folderColorFor } from "../lib/folderColor";

export interface SidebarHandle {
  beginNewProject: () => void;
}

interface Props {
  projects: Project[];
  recordingsByProject: Record<string, Recording[]>;
  selection: Selection;
  onSelect: (s: Selection) => void | Promise<void>;
  onProjectsChanged: () => void;
  onRecordingsChanged: (projectId: string) => void;
  onPickFilesForProject: (projectId: string, paths: string[]) => void;
}

const EXPANDED_KEY = (projectId: string) => `sidebar-expanded-${projectId}`;

function readExpanded(projectId: string): boolean {
  try {
    const v = localStorage.getItem(EXPANDED_KEY(projectId));
    if (v === null) return true;
    return v === "true";
  } catch {
    return true;
  }
}

function writeExpanded(projectId: string, value: boolean): void {
  try {
    localStorage.setItem(EXPANDED_KEY(projectId), value ? "true" : "false");
  } catch {
    /* localStorage unavailable; not fatal */
  }
}

const Sidebar = forwardRef<SidebarHandle, Props>(function Sidebar(
  {
    projects,
    recordingsByProject,
    selection,
    onSelect,
    onProjectsChanged,
    onRecordingsChanged,
    onPickFilesForProject: _onPickFilesForProject,
  },
  ref,
) {
  const [creating, setCreating] = useState(false);
  const [newName, setNewName] = useState("");
  const [renamingProject, setRenamingProject] = useState<string | null>(null);
  const [renamingRecording, setRenamingRecording] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [search, setSearch] = useState("");
  const [expandedTick, setExpandedTick] = useState(0);

  const isExpanded = useCallback(
    (projectId: string) => readExpanded(projectId),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [expandedTick],
  );
  const toggleExpanded = useCallback((projectId: string) => {
    writeExpanded(projectId, !readExpanded(projectId));
    setExpandedTick((n) => n + 1);
  }, []);

  useImperativeHandle(ref, () => ({
    beginNewProject: () => {
      setCreating(true);
      setNewName("");
    },
  }));

  // Frame reporting → Rust for the native glass view.
  const sidebarRef = useRef<HTMLElement | null>(null);
  useEffect(() => {
    const el = sidebarRef.current;
    if (!el) return;
    const report = () => {
      const r = el.getBoundingClientRect();
      try {
        const maybe = invoke("report_sidebar_frame", {
          x: r.x,
          y: r.y,
          width: r.width,
          height: r.height,
        }) as Promise<unknown> | undefined;
        if (maybe && typeof (maybe as Promise<unknown>).catch === "function") {
          (maybe as Promise<unknown>).catch(() => { /* command unavailable on non-macOS or before setup */ });
        }
      } catch {
        /* command unavailable on non-macOS or before setup */
      }
    };
    report();
    const ro = new ResizeObserver(report);
    ro.observe(el);
    window.addEventListener("resize", report);
    return () => {
      ro.disconnect();
      window.removeEventListener("resize", report);
    };
  }, []);

  // Search filtering.
  const filterText = search.trim().toLowerCase();
  const filtered = useMemo(() => {
    if (!filterText) {
      return projects.map((p) => ({
        project: p,
        recordings: recordingsByProject[p.id] ?? [],
        forceExpand: false,
      }));
    }
    return projects
      .map((p) => {
        const recs = recordingsByProject[p.id] ?? [];
        const projectMatches = p.name.toLowerCase().includes(filterText);
        const matchingRecs = recs.filter((r) => r.name.toLowerCase().includes(filterText));
        if (projectMatches) return { project: p, recordings: recs, forceExpand: false };
        if (matchingRecs.length > 0) return { project: p, recordings: matchingRecs, forceExpand: true };
        return null;
      })
      .filter((x): x is NonNullable<typeof x> => x !== null);
  }, [projects, recordingsByProject, filterText]);

  const createProject = async () => {
    const name = newName.trim();
    if (!name) {
      setCreating(false);
      return;
    }
    await api.createProject(name, { source: "inline", content: "", sourcePath: null });
    setCreating(false);
    setNewName("");
    onProjectsChanged();
  };

  const deleteProject = async (id: string, name: string) => {
    const ok = await dialogConfirm(`Delete project "${name}"? Its recordings stay on disk.`, {
      title: "Delete project",
      kind: "warning",
    });
    if (!ok) return;
    await api.deleteProject(id);
    try { localStorage.removeItem(EXPANDED_KEY(id)); } catch { /* ignore */ }
    onProjectsChanged();
  };

  const renameProject = async (id: string) => {
    const newNameTrimmed = renameValue.trim();
    if (!newNameTrimmed) {
      setRenamingProject(null);
      return;
    }
    const project = projects.find((p) => p.id === id);
    if (!project) {
      setRenamingProject(null);
      return;
    }
    await api.updateProject({ ...project, name: newNameTrimmed });
    setRenamingProject(null);
    onProjectsChanged();
  };

  const deleteRecording = async (projectId: string, recordingId: string, name: string) => {
    const ok = await dialogConfirm(`Delete recording "${name}"?`, {
      title: "Delete recording",
      kind: "warning",
    });
    if (!ok) return;
    await api.deleteRecording(recordingId);
    onRecordingsChanged(projectId);
  };

  const renameRecording = async (projectId: string, recordingId: string) => {
    const newNameTrimmed = renameValue.trim();
    if (!newNameTrimmed) {
      setRenamingRecording(null);
      return;
    }
    const recording = (recordingsByProject[projectId] ?? []).find((r) => r.id === recordingId);
    if (!recording) {
      setRenamingRecording(null);
      return;
    }
    await api.updateRecording({ ...recording, name: newNameTrimmed });
    setRenamingRecording(null);
    onRecordingsChanged(projectId);
  };

  const projectSelected = (id: string) =>
    selection.kind === "project" && selection.projectId === id;
  const recordingSelected = (id: string) =>
    selection.kind === "recording" && selection.recordingId === id;

  return (
    <aside
      ref={sidebarRef}
      className="w-60 h-full shrink-0 flex flex-col bg-transparent rounded-window border border-separator shadow-island overflow-hidden"
    >
      <div data-tauri-drag-region className="h-9 shrink-0" />

      <div className="px-3 pb-2 shrink-0">
        <div
          className="flex items-center gap-2 h-8 px-2.5 rounded-control"
          style={{
            background: "var(--sidebar-search-bg)",
            border: "1px solid var(--sidebar-search-border)",
          }}
        >
          <Icon name="magnifying-glass" size="sm" className="text-label-tertiary" />
          <input
            type="search"
            aria-label="Search projects and recordings"
            placeholder="Search"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="flex-1 bg-transparent outline-none text-callout text-label-primary placeholder:text-label-tertiary"
          />
          {search && (
            <button
              type="button"
              onClick={() => setSearch("")}
              aria-label="Clear search"
              className="text-label-tertiary hover:text-label-primary"
            >
              <Icon name="x" size="sm" />
            </button>
          )}
        </div>
      </div>

      <div className="flex-1 overflow-y-auto scrollable px-2 pb-1">
        <div className="flex items-center justify-between pl-2 pr-0.5 pt-1 pb-0.5">
          <span
            className="text-caption-1 font-semibold uppercase"
            style={{ color: "var(--sidebar-section-label)", letterSpacing: "0.6px" }}
          >
            Projects
          </span>
          {!creating && (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => { setCreating(true); setNewName(""); }}
              iconLeft="plus"
              aria-label="New project"
              className="text-label-tertiary hover:text-label-primary"
            />
          )}
        </div>

        {creating && (
          <div className="flex gap-1 px-1 pb-2 pt-1">
            <Input
              autoFocus
              size="sm"
              className="flex-1"
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void createProject();
                if (e.key === "Escape") { setCreating(false); setNewName(""); }
              }}
              placeholder="Project name"
            />
            <Button variant="primary" size="sm" onClick={createProject} iconLeft="check" aria-label="Create project" />
            <Button
              variant="ghost"
              size="sm"
              onClick={() => { setCreating(false); setNewName(""); }}
              iconLeft="x"
              aria-label="Cancel"
            />
          </div>
        )}

        {projects.length === 0 && !creating && (
          <div className="flex flex-col items-center gap-2 py-6">
            <p className="text-footnote text-label-tertiary">No projects yet.</p>
            <Button
              variant="primary"
              size="sm"
              onClick={() => { setCreating(true); setNewName(""); }}
            >
              New project
            </Button>
          </div>
        )}

        {filtered.map(({ project: p, recordings: recs, forceExpand }) => {
          const expanded = forceExpand || isExpanded(p.id);
          const folderClass = `sidebar-folder sidebar-folder-${folderColorFor(p.id)}`;
          const isSel = projectSelected(p.id);
          return (
            <div key={p.id}>
              <ContextMenu>
                <ContextMenu.Trigger asChild>
                  <div
                    role="button"
                    tabIndex={0}
                    onClick={() => {
                      void onSelect({ kind: "project", projectId: p.id });
                      toggleExpanded(p.id);
                    }}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" || e.key === " ") {
                        e.preventDefault();
                        void onSelect({ kind: "project", projectId: p.id });
                        toggleExpanded(p.id);
                      }
                    }}
                    aria-label={`${p.name}, project, ${recs.length} recording${recs.length === 1 ? "" : "s"}`}
                    aria-current={isSel ? "page" : undefined}
                    aria-expanded={expanded}
                    className="group mx-2 my-px h-9 px-2.5 rounded-control flex items-center gap-2 text-body outline-none focus-visible:ring-2 focus-visible:ring-focus-ring"
                    style={
                      isSel
                        ? { background: "var(--sidebar-row-selected)", color: "var(--sidebar-row-selected-text)" }
                        : undefined
                    }
                    onMouseEnter={(e) => {
                      if (!isSel) e.currentTarget.style.background = "var(--sidebar-row-hover)";
                    }}
                    onMouseLeave={(e) => {
                      if (!isSel) e.currentTarget.style.background = "";
                    }}
                  >
                    <span
                      aria-hidden
                      className="inline-block w-2.5 transition-transform"
                      style={{
                        transform: expanded ? "rotate(90deg)" : "rotate(0deg)",
                        color: isSel ? "var(--sidebar-row-selected-text)" : "var(--color-label-tertiary)",
                      }}
                    >
                      ›
                    </span>
                    <span className={folderClass} aria-hidden />
                    {renamingProject === p.id ? (
                      <Input
                        autoFocus
                        size="sm"
                        className="flex-1 h-6 -my-1"
                        value={renameValue}
                        onChange={(e) => setRenameValue(e.target.value)}
                        onClick={(e) => e.stopPropagation()}
                        onKeyDown={(e) => {
                          e.stopPropagation();
                          if (e.key === "Enter") void renameProject(p.id);
                          if (e.key === "Escape") setRenamingProject(null);
                        }}
                        onBlur={() => void renameProject(p.id)}
                      />
                    ) : (
                      <span className="flex-1 truncate font-medium">{p.name}</span>
                    )}
                  </div>
                </ContextMenu.Trigger>
                <ContextMenu.Content>
                  <ContextMenu.Item onSelect={() => { setRenameValue(p.name); setRenamingProject(p.id); }}>
                    Rename
                  </ContextMenu.Item>
                  <ContextMenu.Item onSelect={() => void deleteProject(p.id, p.name)}>
                    Delete
                  </ContextMenu.Item>
                </ContextMenu.Content>
              </ContextMenu>

              {expanded && recs.map((rec) => {
                const recSel = recordingSelected(rec.id);
                return (
                  <ContextMenu key={rec.id}>
                    <ContextMenu.Trigger asChild>
                      <div
                        role="button"
                        tabIndex={0}
                        onClick={() => void onSelect({ kind: "recording", projectId: p.id, recordingId: rec.id })}
                        onKeyDown={(e) => {
                          if (e.key === "Enter" || e.key === " ") {
                            e.preventDefault();
                            void onSelect({ kind: "recording", projectId: p.id, recordingId: rec.id });
                          }
                        }}
                        aria-label={`${rec.name}, recording`}
                        aria-current={recSel ? "page" : undefined}
                        className="mx-2 my-px h-[34px] pl-[30px] pr-2.5 rounded-control flex items-center gap-2 text-body outline-none focus-visible:ring-2 focus-visible:ring-focus-ring"
                        style={
                          recSel
                            ? { background: "var(--sidebar-row-selected)", color: "var(--sidebar-row-selected-text)" }
                            : undefined
                        }
                        onMouseEnter={(e) => {
                          if (!recSel) e.currentTarget.style.background = "var(--sidebar-row-hover)";
                        }}
                        onMouseLeave={(e) => {
                          if (!recSel) e.currentTarget.style.background = "";
                        }}
                      >
                        <Icon
                          name="recording"
                          size="sm"
                          className={recSel ? "" : "text-label-tertiary"}
                        />
                        {renamingRecording === rec.id ? (
                          <Input
                            autoFocus
                            size="sm"
                            className="flex-1 h-6 -my-1"
                            value={renameValue}
                            onChange={(e) => setRenameValue(e.target.value)}
                            onClick={(e) => e.stopPropagation()}
                            onKeyDown={(e) => {
                              e.stopPropagation();
                              if (e.key === "Enter") void renameRecording(p.id, rec.id);
                              if (e.key === "Escape") setRenamingRecording(null);
                            }}
                            onBlur={() => void renameRecording(p.id, rec.id)}
                          />
                        ) : (
                          <span className="flex-1 truncate">{rec.name}</span>
                        )}
                      </div>
                    </ContextMenu.Trigger>
                    <ContextMenu.Content>
                      <ContextMenu.Item onSelect={() => { setRenameValue(rec.name); setRenamingRecording(rec.id); }}>
                        Rename
                      </ContextMenu.Item>
                      <ContextMenu.Item onSelect={() => void deleteRecording(p.id, rec.id, rec.name)}>
                        Delete
                      </ContextMenu.Item>
                    </ContextMenu.Content>
                  </ContextMenu>
                );
              })}
            </div>
          );
        })}
      </div>
    </aside>
  );
});

export default Sidebar;
