import { invoke } from "@tauri-apps/api/core";
import type { AppSettings, Project, Recording, Template } from "../types/storage";

export const api = {
  listProjects: () => invoke<Project[]>("list_projects"),

  createProject: (name: string, defaultSettings: AppSettings, template: Template) =>
    invoke<Project>("create_project", { name, defaultSettings, template }),

  updateProject: (project: Project) => invoke<void>("update_project", { project }),

  deleteProject: (id: string) => invoke<void>("delete_project", { id }),

  listRecordings: (projectId: string) =>
    invoke<Recording[]>("list_recordings", { projectId }),

  getRecording: (id: string) => invoke<Recording | null>("get_recording", { id }),

  createRecording: (
    projectId: string,
    name: string,
    sourcePaths: string[],
    settings: AppSettings,
  ) => invoke<Recording>("create_recording", { projectId, name, sourcePaths, settings }),

  updateRecording: (recording: Recording) =>
    invoke<void>("update_recording", { recording }),

  deleteRecording: (id: string) => invoke<void>("delete_recording", { id }),

  startOverRecording: (id: string) => invoke<void>("start_over_recording", { id }),

  runRecording: (recordingId: string) =>
    invoke<void>("run_recording", { recordingId }),

  recordSavedOutputs: (recordingId: string, savedPaths: string[]) =>
    invoke<void>("record_saved_outputs", { recordingId, savedPaths }),
};
