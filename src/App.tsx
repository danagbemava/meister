import { useCallback, useEffect, useRef, useState } from "react";
import Sidebar, { type SidebarHandle } from "./components/Sidebar";
import ProjectView from "./components/ProjectView";
import RecordingView, { type RecordingViewHandle } from "./components/RecordingView";
import { confirm as dialogConfirm, open as openDialog } from "@tauri-apps/plugin-dialog";
import { api } from "./api/storage";
import { initMenuEvents, setMenuHandler } from "./lib/menuEvents";
import type { Project, Recording } from "./types/storage";

export type Selection =
  | { kind: "none" }
  | { kind: "project"; projectId: string }
  | { kind: "recording"; projectId: string; recordingId: string };

export default function App() {
  const [projects, setProjects] = useState<Project[]>([]);
  const [recordingsByProject, setRecordingsByProject] = useState<Record<string, Recording[]>>({});
  const [selection, setSelection] = useState<Selection>({ kind: "none" });
  const [loadingProjects, setLoadingProjects] = useState(true);
  const [pendingStagedPaths, setPendingStagedPaths] = useState<{ projectId: string; paths: string[] } | null>(null);
  const [sidebarVisible, setSidebarVisible] = useState(true);
  const sidebarRef = useRef<SidebarHandle>(null);
  const recordingViewRef = useRef<RecordingViewHandle>(null);

  const refreshProjects = useCallback(async () => {
    const list = await api.listProjects();
    setProjects(list);
  }, []);

  const refreshRecordings = useCallback(async (projectId: string) => {
    const list = await api.listRecordings(projectId);
    setRecordingsByProject((prev) => ({ ...prev, [projectId]: list }));
  }, []);

  const activeCaptureRef = useRef<null | (() => void)>(null);

  const registerActiveCapture = useCallback(
    (discard: (() => void) | null) => {
      activeCaptureRef.current = discard;
    },
    [],
  );

  const guardedSelect = useCallback(async (next: Selection) => {
    if (activeCaptureRef.current) {
      const ok = await dialogConfirm(
        "Recording in progress — stop and discard?",
        { title: "Stop recording", kind: "warning" },
      );
      if (!ok) return;
      activeCaptureRef.current();
      activeCaptureRef.current = null;
    }
    setSelection(next);
  }, []);

  const AUDIO_VIDEO_EXTS = [
    "mp4", "mov", "mkv", "avi", "mp3", "m4a", "wav", "ogg", "webm", "aac", "flac",
  ];

  const importAudioToActiveProject = useCallback(async () => {
    let projectId: string | null = null;
    if (selection.kind === "project") projectId = selection.projectId;
    else if (selection.kind === "recording") projectId = selection.projectId;
    if (!projectId) return;

    const picked = await openDialog({
      multiple: true,
      filters: [{ name: "Audio / Video", extensions: AUDIO_VIDEO_EXTS }],
    });
    if (!picked) return;
    const list = Array.isArray(picked) ? picked : [picked];
    if (list.length === 0) return;
    void guardedSelect({ kind: "project", projectId });
    setPendingStagedPaths({ projectId, paths: list });
  }, [selection, guardedSelect]);

  useEffect(() => {
    refreshProjects().finally(() => setLoadingProjects(false));
  }, [refreshProjects]);

  useEffect(() => {
    void initMenuEvents();
  }, []);

  useEffect(() => {
    setMenuHandler("menu:new-project", () => {
      sidebarRef.current?.beginNewProject();
    });
    setMenuHandler("menu:import-audio", () => {
      void importAudioToActiveProject();
    });
    setMenuHandler("menu:run-pipeline", () => {
      recordingViewRef.current?.runPipeline();
    });
    setMenuHandler("menu:toggle-sidebar", () => {
      setSidebarVisible((v) => !v);
    });
  }, [importAudioToActiveProject]);

  useEffect(() => {
    for (const p of projects) {
      if (!(p.id in recordingsByProject)) {
        refreshRecordings(p.id);
      }
    }
  }, [projects, recordingsByProject, refreshRecordings]);

  return (
    <div className="h-screen flex gap-2 p-2 overflow-hidden">
      <div
        aria-hidden={!sidebarVisible}
        className={`transition-all duration-200 overflow-hidden shrink-0 ${
          sidebarVisible ? "w-60 opacity-100" : "w-0 opacity-0"
        }`}
      >
        <Sidebar
          ref={sidebarRef}
          projects={projects}
          recordingsByProject={recordingsByProject}
          selection={selection}
          onSelect={guardedSelect}
          onProjectsChanged={refreshProjects}
          onRecordingsChanged={(projectId) => {
            void refreshRecordings(projectId);
          }}
          onPickFilesForProject={(projectId, paths) => {
            void guardedSelect({ kind: "project", projectId });
            setPendingStagedPaths({ projectId, paths });
          }}
        />
      </div>
      <div className="flex-1 rounded-xl bg-control-background overflow-hidden">
        {selection.kind === "none" && (
          <EmptyState loading={loadingProjects} hasProjects={projects.length > 0} />
        )}
        {selection.kind === "project" && (
          <ProjectView
            projectId={selection.projectId}
            onProjectChanged={refreshProjects}
            onRecordingCreated={(r) => {
              refreshRecordings(r.projectId);
              setSelection({ kind: "recording", projectId: r.projectId, recordingId: r.id });
            }}
            onSelectRecording={(recordingId) =>
              guardedSelect({
                kind: "recording",
                projectId: selection.projectId,
                recordingId,
              })
            }
            initialStagedPaths={
              pendingStagedPaths?.projectId === selection.projectId
                ? pendingStagedPaths.paths
                : undefined
            }
            onInitialStagedConsumed={() => setPendingStagedPaths(null)}
            onCaptureActiveChange={registerActiveCapture}
          />
        )}
        {selection.kind === "recording" && (
          <RecordingView
            ref={recordingViewRef}
            recordingId={selection.recordingId}
            onRecordingChanged={() => refreshRecordings(selection.projectId)}
            onRecordingDeleted={() => {
              refreshRecordings(selection.projectId);
              setSelection({ kind: "project", projectId: selection.projectId });
            }}
          />
        )}
      </div>
    </div>
  );
}

function EmptyState({ loading, hasProjects }: { loading: boolean; hasProjects: boolean }) {
  if (loading) return <div className="flex h-full items-center justify-center text-label-tertiary">Loading…</div>;
  return (
    <div className="flex h-full items-center justify-center">
      <div className="text-center max-w-md px-8">
        <h1 className="text-title-2 font-semibold mb-2">
          {hasProjects ? "Select a project from the sidebar" : "Create your first project"}
        </h1>
        <p className="text-body text-label-tertiary">
          Projects group recordings that share a template and default settings.
        </p>
      </div>
    </div>
  );
}
