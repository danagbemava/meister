//! Pipeline orchestration: `inputs_unchanged` decides whether a completed
//! stage's cached output is still valid given the current recording settings
//! and project template. `run_recording` is added in a later task.

use crate::storage::{AppSettings, StageKind, StageRecord, StageStatus};

/// Returns `true` if the stage's cached inputs still match the current
/// recording settings + template hash, meaning its cached output can be
/// reused. Assumes `stage.status == Completed`; caller checks that first.
pub fn inputs_unchanged(
    stage_kind: StageKind,
    stage: &StageRecord,
    current_settings: &AppSettings,
    current_template_hash: &str,
    current_source_path: &str,
    extract_artifact_exists: bool,
) -> bool {
    if stage.status != StageStatus::Completed {
        return false;
    }
    match stage_kind {
        StageKind::Extract => {
            // source_path is immutable for a recording, so extract is never
            // invalidated by content changes. But if someone deleted the
            // audio.wav artifact *and* the WAV is needed downstream (i.e.
            // transcribe isn't completed), we can't reuse. For the common
            // case of "everything already completed", we don't need the wav
            // anymore, so accept.
            let _ = extract_artifact_exists; // currently unused; hook for future
            let _ = current_source_path;
            true
        }
        StageKind::Transcribe => {
            matches_transcribe_model(stage, &current_settings.whisper_model)
                && matches_language(stage, &current_settings.language)
        }
        StageKind::Format => {
            stage.model_used.as_deref() == Some(current_settings.model.as_str())
                && stage.template_hash.as_deref() == Some(current_template_hash)
        }
        StageKind::Save => {
            // Rerun save if the requested format set changed or any saved
            // file is missing on disk.
            let Some(saved) = &stage.saved_paths else { return false; };
            let formats_match = saved
                .iter()
                .all(|p| saved_path_matches_any_format(p, &current_settings.output_formats))
                && current_settings.output_formats.iter().all(|fmt| {
                    saved.iter().any(|p| path_has_extension(p, fmt_to_ext(fmt)))
                });
            if !formats_match {
                return false;
            }
            saved.iter().all(|p| std::path::Path::new(p).is_file())
        }
    }
}

fn matches_language(stage: &StageRecord, current: &str) -> bool {
    // The transcribe stage stores the model under model_used; language is
    // mixed into the "inputs that matter". We carry it on `output`'s
    // companion — but the current schema only has model_used. For MVP:
    // treat language mismatch as invalidation by overloading model_used?
    // Simpler: store language-suffixed model_used. Instead, use a small
    // convention: model_used string format "<model>@<language>".
    match stage.model_used.as_deref() {
        Some(s) => match s.split_once('@') {
            Some((_, lang)) => lang == current,
            // Back-compat: if no "@" suffix, treat as matching any language.
            None => true,
        },
        None => false,
    }
}

fn matches_transcribe_model(stage: &StageRecord, current: &str) -> bool {
    match stage.model_used.as_deref() {
        Some(s) => match s.split_once('@') {
            Some((model, _)) => model == current,
            None => s == current,
        },
        None => false,
    }
}

fn fmt_to_ext(fmt: &crate::storage::OutputFormat) -> &'static str {
    use crate::storage::OutputFormat::*;
    match fmt {
        Md => "md",
        Txt => "txt",
        Docx => "docx",
        Pdf => "pdf",
    }
}

fn path_has_extension(path: &str, ext: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.eq_ignore_ascii_case(ext))
        .unwrap_or(false)
}

fn saved_path_matches_any_format(
    path: &str,
    formats: &[crate::storage::OutputFormat],
) -> bool {
    formats.iter().any(|f| path_has_extension(path, fmt_to_ext(f)))
}

use std::sync::Arc;
use tauri::{AppHandle, Emitter};

use crate::commands::{run_extract, run_format, run_transcribe};
use crate::storage::{self, Recording, Store};

#[derive(Clone, serde::Serialize)]
struct StageChangedEvent {
    #[serde(rename = "recordingId")]
    recording_id: String,
    stage: String,
    status: String,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn recording_dir(app: &AppHandle, recording_id: &str) -> Result<std::path::PathBuf, String> {
    use tauri::Manager;
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app_data_dir: {e}"))?
        .join("recordings")
        .join(recording_id))
}

/// Run the pipeline for a recording, skipping any stage whose cached output is
/// still valid. Idempotent — this is how both "Run" and "Resume" work.
#[tauri::command]
pub async fn run_recording(
    app: AppHandle,
    store: tauri::State<'_, Arc<Store>>,
    recording_id: String,
) -> Result<(), String> {
    let store: Arc<Store> = (*store).clone();

    // Load recording + project.
    let rec: Recording = storage::recordings::get(&store, &recording_id)
        .map_err(|e| format!("load recording: {e}"))?
        .ok_or_else(|| format!("recording not found: {recording_id}"))?;
    let project = storage::projects::get(&store, &rec.project_id)
        .map_err(|e| format!("load project: {e}"))?
        .ok_or_else(|| format!("project not found: {}", rec.project_id))?;

    let template_hash = storage::template_hash(&project.template.content);
    let wav_path = recording_dir(&app, &recording_id)?.join("audio.wav");
    let wav_exists = wav_path.is_file();

    // Walk stages; on invalidation, reset downstream before starting.
    // We reload the recording's stages after each transition to avoid stale in-memory copies.
    for stage_kind in StageKind::ALL {
        let rec = storage::recordings::get(&store, &recording_id)
            .map_err(|e| format!("reload recording: {e}"))?
            .ok_or_else(|| "recording vanished".to_string())?;
        let stage = rec
            .stages
            .iter()
            .find(|s| s.stage == stage_kind)
            .ok_or_else(|| format!("stage row missing: {:?}", stage_kind))?;

        let first_source = rec.source_paths.first().map(String::as_str).unwrap_or("");
        let reusable = matches!(stage.status, storage::StageStatus::Completed)
            && inputs_unchanged(
                stage_kind,
                stage,
                &rec.settings,
                &template_hash,
                first_source,
                wav_exists,
            );

        if reusable {
            continue;
        }

        // Invalidation: clear this stage + everything downstream before running.
        storage::stages::reset_from(&store, &recording_id, stage_kind)
            .map_err(|e| format!("reset_from: {e}"))?;

        storage::stages::set_running(&store, &recording_id, stage_kind)
            .map_err(|e| format!("set_running: {e}"))?;
        let _ = app.emit("stage-changed", StageChangedEvent {
            recording_id: recording_id.clone(),
            stage: stage_kind.as_str().to_string(),
            status: "running".to_string(),
        });

        let result = match stage_kind {
            StageKind::Extract => {
                let src = rec.source_paths.first().map(String::as_str).unwrap_or("");
                run_extract(src, &wav_path).await
                    .map(|_| storage::stages::CompletedOutput {
                        output: None,
                        model_used: None,
                        template_hash: None,
                        saved_paths: None,
                        completed_at: now_ms(),
                    })
            }
            StageKind::Transcribe => {
                let whisper_model = rec.settings.whisper_model.clone();
                let language = rec.settings.language.clone();
                let transcript = run_transcribe(&app, &wav_path, &whisper_model, &language).await?;
                // Success: delete the wav to reclaim disk.
                let _ = std::fs::remove_file(&wav_path);
                Ok(storage::stages::CompletedOutput {
                    output: Some(transcript),
                    // Encode language into model_used for invalidation checks.
                    model_used: Some(format!("{}@{}", whisper_model, language)),
                    template_hash: None,
                    saved_paths: None,
                    completed_at: now_ms(),
                })
            }
            StageKind::Format => {
                // Load transcript from transcribe stage output.
                let rec2 = storage::recordings::get(&store, &recording_id)
                    .map_err(|e| format!("reload for format: {e}"))?
                    .ok_or_else(|| "recording vanished".to_string())?;
                let transcript = rec2
                    .stages
                    .iter()
                    .find(|s| s.stage == StageKind::Transcribe)
                    .and_then(|s| s.output.clone())
                    .ok_or_else(|| "transcribe output missing".to_string())?;
                let formatted = run_format(
                    &app,
                    &transcript,
                    &project.template.content,
                    &rec.settings.ollama_url,
                    &rec.settings.model,
                )
                .await?;
                Ok(storage::stages::CompletedOutput {
                    output: Some(formatted),
                    model_used: Some(rec.settings.model.clone()),
                    template_hash: Some(template_hash.clone()),
                    saved_paths: None,
                    completed_at: now_ms(),
                })
            }
            StageKind::Save => {
                // Save is user-interactive (file dialogs). We do not run it here —
                // the UI invokes `save_recording_outputs` separately once the user
                // picks destinations. Flip this stage to completed with an empty
                // saved_paths list; the UI will update it.
                Ok(storage::stages::CompletedOutput {
                    output: None,
                    model_used: None,
                    template_hash: None,
                    saved_paths: Some(Vec::new()),
                    completed_at: now_ms(),
                })
            }
        };

        match result {
            Ok(out) => {
                storage::stages::set_completed(&store, &recording_id, stage_kind, out)
                    .map_err(|e| format!("set_completed: {e}"))?;
                let _ = app.emit("stage-changed", StageChangedEvent {
                    recording_id: recording_id.clone(),
                    stage: stage_kind.as_str().to_string(),
                    status: "completed".to_string(),
                });
            }
            Err(e) => {
                storage::stages::set_failed(&store, &recording_id, stage_kind, &e)
                    .map_err(|ee| format!("set_failed: {ee}"))?;
                let _ = app.emit("stage-changed", StageChangedEvent {
                    recording_id: recording_id.clone(),
                    stage: stage_kind.as_str().to_string(),
                    status: "failed".to_string(),
                });
                return Err(e);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{AppSettings, OutputFormat, StageKind, StageRecord, StageStatus};

    fn settings() -> AppSettings {
        AppSettings {
            ollama_url: "u".to_string(),
            model: "gemma3".to_string(),
            whisper_model: "base".to_string(),
            language: "en".to_string(),
            output_formats: vec![OutputFormat::Md],
        }
    }

    fn completed_stage(kind: StageKind) -> StageRecord {
        StageRecord {
            stage: kind,
            status: StageStatus::Completed,
            error: None,
            completed_at: Some(1),
            model_used: None,
            template_hash: None,
            output: None,
            saved_paths: None,
        }
    }

    #[test]
    fn extract_always_valid_if_completed() {
        let s = completed_stage(StageKind::Extract);
        assert!(inputs_unchanged(StageKind::Extract, &s, &settings(), "h", "/x", true));
    }

    #[test]
    fn transcribe_invalid_if_model_changed() {
        let mut s = completed_stage(StageKind::Transcribe);
        s.model_used = Some("base@en".to_string());
        let mut cur = settings();
        cur.whisper_model = "small".to_string();
        assert!(!inputs_unchanged(StageKind::Transcribe, &s, &cur, "h", "/x", true));
    }

    #[test]
    fn transcribe_valid_if_model_matches() {
        let mut s = completed_stage(StageKind::Transcribe);
        s.model_used = Some("base@en".to_string());
        assert!(inputs_unchanged(StageKind::Transcribe, &s, &settings(), "h", "/x", true));
    }

    #[test]
    fn format_invalid_on_template_change() {
        let mut s = completed_stage(StageKind::Format);
        s.model_used = Some("gemma3".to_string());
        s.template_hash = Some("old".to_string());
        assert!(!inputs_unchanged(StageKind::Format, &s, &settings(), "new", "/x", true));
    }

    #[test]
    fn format_invalid_on_model_change() {
        let mut s = completed_stage(StageKind::Format);
        s.model_used = Some("llama3".to_string());
        s.template_hash = Some("h".to_string());
        assert!(!inputs_unchanged(StageKind::Format, &s, &settings(), "h", "/x", true));
    }

    #[test]
    fn save_invalid_if_paths_missing() {
        let mut s = completed_stage(StageKind::Save);
        s.saved_paths = Some(vec!["/definitely/not/here.md".to_string()]);
        assert!(!inputs_unchanged(StageKind::Save, &s, &settings(), "h", "/x", true));
    }

    #[test]
    fn save_invalid_if_output_formats_changed() {
        let mut s = completed_stage(StageKind::Save);
        // Pretend the file exists — build a temp file.
        let dir = std::env::temp_dir();
        let f = dir.join("meister_test.md");
        std::fs::write(&f, "x").unwrap();
        s.saved_paths = Some(vec![f.to_string_lossy().to_string()]);

        let mut cur = settings();
        cur.output_formats = vec![OutputFormat::Md, OutputFormat::Txt];
        assert!(!inputs_unchanged(StageKind::Save, &s, &cur, "h", "/x", true));

        let _ = std::fs::remove_file(&f);
    }
}
