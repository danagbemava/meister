import { invoke } from "@tauri-apps/api/core";
import type { GlobalSettings, Project, Recording, Template } from "../types/storage";

export const api = {
  listProjects: () => invoke<Project[]>("list_projects"),

  createProject: (name: string, template: Template) =>
    invoke<Project>("create_project", { name, template }),

  updateProject: (project: Project) => invoke<void>("update_project", { project }),

  deleteProject: (id: string) => invoke<void>("delete_project", { id }),

  listRecordings: (projectId: string) =>
    invoke<Recording[]>("list_recordings", { projectId }),

  getRecording: (id: string) => invoke<Recording | null>("get_recording", { id }),

  createRecording: (
    projectId: string,
    name: string,
    sourcePaths: string[],
  ) => invoke<Recording>("create_recording", { projectId, name, sourcePaths }),

  updateRecording: (recording: Recording) =>
    invoke<void>("update_recording", { recording }),

  deleteRecording: (id: string) => invoke<void>("delete_recording", { id }),

  startOverRecording: (id: string) => invoke<void>("start_over_recording", { id }),

  runRecording: (recordingId: string) =>
    invoke<void>("run_recording", { recordingId }),

  getGlobalSettings: () => invoke<GlobalSettings>("get_global_settings"),

  setGlobalSettings: (settings: GlobalSettings) =>
    invoke<void>("set_global_settings", { settings }),

  openSettingsWindow: () => invoke<void>("open_settings_window"),

  setFormatOutput: (recordingId: string, output: string) =>
    invoke<void>("set_format_output", { recordingId, output }),

  isDemoMode: () => invoke<boolean>("is_demo_mode"),

  setDemoMode: (enabled: boolean) =>
    invoke<void>("set_demo_mode", { enabled }),

  startCallCapture: async (includeMic: boolean) => {
    const raw = await invoke<{
      session_id: string;
      system_ok: boolean;
      mic_ok: boolean;
      mic_error?: string | null;
    }>("start_call_capture", { includeMic });
    return {
      sessionId: raw.session_id,
      systemOk: raw.system_ok,
      micOk: raw.mic_ok,
      micError: raw.mic_error ?? null,
    };
  },

  pauseCallCapture: (sessionId: string) =>
    invoke<void>("pause_call_capture", { sessionId }),

  resumeCallCapture: (sessionId: string) =>
    invoke<void>("resume_call_capture", { sessionId }),

  stopCallCapture: (sessionId: string) =>
    invoke<string>("stop_call_capture", { sessionId }),

  discardCallCapture: (sessionId: string) =>
    invoke<void>("discard_call_capture", { sessionId }),
};
