export type OutputFormat = "md" | "txt" | "docx" | "pdf";

export interface GlobalSettings {
  ollamaUrl: string;
  model: string;
  whisperModel: string;
  language: string;
  defaultExportFormat: OutputFormat;
}

export type TemplateSource = "inline" | "file";

export interface Template {
  source: TemplateSource;
  content: string;
  sourcePath?: string | null;
  matchStyle?: boolean;
}

export interface Project {
  id: string;
  name: string;
  createdAt: number;
  updatedAt: number;
  template: Template;
  customInstructions?: string;
  language?: string | null;
}

export type StageKind = "extract" | "transcribe" | "format";
export type StageStatus = "pending" | "running" | "completed" | "failed";

export interface StageRecord {
  stage: StageKind;
  status: StageStatus;
  error?: string | null;
  completedAt?: number | null;
  modelUsed?: string | null;
  templateHash?: string | null;
  output?: string | null;
}

export interface Recording {
  id: string;
  projectId: string;
  name: string;
  sourcePaths: string[];
  createdAt: number;
  updatedAt: number;
  stages: StageRecord[];
}

export const STAGES: StageKind[] = ["extract", "transcribe", "format"];

export function overallStatus(r: Recording): "new" | "in_progress" | "completed" | "failed" {
  if (r.stages.length === 0) return "new";
  if (r.stages.some((s) => s.status === "failed")) return "failed";
  if (r.stages.some((s) => s.status === "running")) return "in_progress";
  if (r.stages.every((s) => s.status === "completed")) return "completed";
  if (r.stages.some((s) => s.status !== "pending")) return "in_progress";
  return "new";
}
