//! Data types shared by storage, orchestrator, and Tauri commands.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Md,
    Txt,
    Docx,
    Pdf,
}

impl OutputFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            OutputFormat::Md => "md",
            OutputFormat::Txt => "txt",
            OutputFormat::Docx => "docx",
            OutputFormat::Pdf => "pdf",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "md" => Some(OutputFormat::Md),
            "txt" => Some(OutputFormat::Txt),
            "docx" => Some(OutputFormat::Docx),
            "pdf" => Some(OutputFormat::Pdf),
            _ => None,
        }
    }
}

/// App-wide settings. Persisted in the singleton `app_settings` table.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GlobalSettings {
    #[serde(rename = "ollamaUrl")]
    pub ollama_url: String,
    pub model: String,
    #[serde(rename = "whisperModel")]
    pub whisper_model: String,
    pub language: String,
    #[serde(rename = "defaultExportFormat")]
    pub default_export_format: OutputFormat,
}

/// The effective settings the orchestrator runs a recording with, computed
/// from `(GlobalSettings, Project.language)` at run time.
#[derive(Debug, Clone)]
pub struct EffectiveSettings {
    pub ollama_url: String,
    pub model: String,
    pub whisper_model: String,
    pub language: String,
}

impl EffectiveSettings {
    pub fn from(global: &GlobalSettings, project_language: Option<&str>) -> Self {
        let language = project_language
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&global.language)
            .to_string();
        Self {
            ollama_url: global.ollama_url.clone(),
            model: global.model.clone(),
            whisper_model: global.whisper_model.clone(),
            language,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TemplateSource {
    Inline,
    File,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    pub source: TemplateSource,
    pub content: String,
    #[serde(rename = "sourcePath")]
    pub source_path: Option<String>,
    #[serde(rename = "matchStyle", default, skip_serializing_if = "is_false")]
    pub match_style: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
    pub template: Template,
    #[serde(rename = "customInstructions", default, skip_serializing_if = "String::is_empty")]
    pub custom_instructions: String,
    /// Optional per-project language override. `None`/empty means "use the
    /// global default language".
    #[serde(default)]
    pub language: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum StageKind {
    Extract,
    Transcribe,
    Format,
}

impl StageKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            StageKind::Extract => "extract",
            StageKind::Transcribe => "transcribe",
            StageKind::Format => "format",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "extract" => Some(StageKind::Extract),
            "transcribe" => Some(StageKind::Transcribe),
            "format" => Some(StageKind::Format),
            _ => None,
        }
    }

    pub const ALL: [StageKind; 3] = [
        StageKind::Extract,
        StageKind::Transcribe,
        StageKind::Format,
    ];
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StageStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

impl StageStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            StageStatus::Pending => "pending",
            StageStatus::Running => "running",
            StageStatus::Completed => "completed",
            StageStatus::Failed => "failed",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(StageStatus::Pending),
            "running" => Some(StageStatus::Running),
            "completed" => Some(StageStatus::Completed),
            "failed" => Some(StageStatus::Failed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageRecord {
    pub stage: StageKind,
    pub status: StageStatus,
    pub error: Option<String>,
    #[serde(rename = "completedAt")]
    pub completed_at: Option<i64>,
    #[serde(rename = "modelUsed")]
    pub model_used: Option<String>,
    #[serde(rename = "templateHash")]
    pub template_hash: Option<String>,
    pub output: Option<String>,
}

fn is_false(b: &bool) -> bool { !*b }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recording {
    pub id: String,
    #[serde(rename = "projectId")]
    pub project_id: String,
    pub name: String,
    #[serde(rename = "sourcePaths")]
    pub source_paths: Vec<String>,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
    pub stages: Vec<StageRecord>,
}
