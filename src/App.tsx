import { useCallback, useEffect, useState } from "react";
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

  const refreshProjects = useCallback(async () => {
    const list = await api.listProjects();
    setProjects(list);
  }, []);

  const refreshRecordings = useCallback(async (projectId: string) => {
    const list = await api.listRecordings(projectId);
    setRecordingsByProject((prev) => ({ ...prev, [projectId]: list }));
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
    <div className="flex flex-col h-screen bg-surface overflow-hidden">
      <TitleBar />
      <div className="flex-1 flex overflow-hidden">
        <Sidebar
          projects={projects}
          recordingsByProject={recordingsByProject}
          selection={selection}
          onSelect={setSelection}
          onProjectsChanged={refreshProjects}
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
  if (loading) return <div className="flex h-full items-center justify-center text-white/40">Loading…</div>;
  return (
    <div className="flex h-full items-center justify-center">
      <div className="text-center max-w-md px-8">
        <h1 className="text-xl font-semibold mb-2">
          {hasProjects ? "Select a project from the sidebar" : "Create your first project"}
        </h1>
        <p className="text-sm text-white/40">
          Projects group recordings that share a template and default settings.
        </p>
      </div>
    </div>
  );
}
