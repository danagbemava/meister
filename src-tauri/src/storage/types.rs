//! Data types shared by storage, orchestrator, and Tauri commands.
//!
//! `settings` and `default_settings` are stored as JSON strings in SQLite
//! and round-tripped through serde when loaded.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Md,
    Txt,
    Docx,
    Pdf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppSettings {
    #[serde(rename = "ollamaUrl")]
    pub ollama_url: String,
    pub model: String,
    #[serde(rename = "whisperModel")]
    pub whisper_model: String,
    pub language: String,
    #[serde(rename = "outputFormats")]
    pub output_formats: Vec<OutputFormat>,
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
    #[serde(rename = "defaultSettings")]
    pub default_settings: AppSettings,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum StageKind {
    Extract,
    Transcribe,
    Format,
    Save,
}

impl StageKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            StageKind::Extract => "extract",
            StageKind::Transcribe => "transcribe",
            StageKind::Format => "format",
            StageKind::Save => "save",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "extract" => Some(StageKind::Extract),
            "transcribe" => Some(StageKind::Transcribe),
            "format" => Some(StageKind::Format),
            "save" => Some(StageKind::Save),
            _ => None,
        }
    }

    pub const ALL: [StageKind; 4] = [
        StageKind::Extract,
        StageKind::Transcribe,
        StageKind::Format,
        StageKind::Save,
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
    #[serde(rename = "savedPaths")]
    pub saved_paths: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recording {
    pub id: String,
    #[serde(rename = "projectId")]
    pub project_id: String,
    pub name: String,
    #[serde(rename = "sourcePath")]
    pub source_path: String,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
    pub settings: AppSettings,
    pub stages: Vec<StageRecord>,
}
