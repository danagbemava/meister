import { useCallback, useEffect, useRef, useState } from "react";
import TitleBar from "./components/TitleBar";
import Sidebar from "./components/Sidebar";
import ProjectView from "./components/ProjectView";
import RecordingView from "./components/RecordingView";
import { api } from "./api/storage";
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

  const guardedSelect = useCallback((next: Selection) => {
    if (activeCaptureRef.current) {
      if (!confirm("Recording in progress — stop and discard?")) return;
      activeCaptureRef.current();
      activeCaptureRef.current = null;
    }
    setSelection(next);
  }, []);

  useEffect(() => {
    refreshProjects().finally(() => setLoadingProjects(false));
  }, [refreshProjects]);

  useEffect(() => {
    for (const p of projects) {
      if (!(p.id in recordingsByProject)) {
        refreshRecordings(p.id);
      }
    }
  }, [projects, recordingsByProject, refreshRecordings]);

  return (
    <div className="flex flex-col h-screen bg-window-background overflow-hidden">
      <TitleBar />
      <div className="flex-1 flex overflow-hidden">
        <Sidebar
          projects={projects}
          recordingsByProject={recordingsByProject}
          selection={selection}
          onSelect={guardedSelect}
          onProjectsChanged={refreshProjects}
          onPickFilesForProject={(projectId, paths) => {
            guardedSelect({ kind: "project", projectId });
            setPendingStagedPaths({ projectId, paths });
          }}
        />
        <div className="flex-1 overflow-hidden">
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
    </div>
  );
}

function EmptyState({ loading, hasProjects }: { loading: boolean; hasProjects: boolean }) {
  if (loading) return <div className="flex h-full items-center justify-center text-label-tertiary">Loading…</div>;
  return (
    <div className="flex h-full items-center justify-center">
      <div className="text-center max-w-md px-8">
        <h1 className="text-xl font-semibold mb-2">
          {hasProjects ? "Select a project from the sidebar" : "Create your first project"}
        </h1>
        <p className="text-sm text-label-tertiary">
          Projects group recordings that share a template and default settings.
        </p>
      </div>
    </div>
  );
}
