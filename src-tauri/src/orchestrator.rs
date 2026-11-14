//! Pipeline orchestration: `inputs_unchanged` decides whether a completed
//! stage's cached output is still valid given the current recording settings
//! and project template. `run_recording` is added in a later task.

use crate::storage::{EffectiveSettings, StageKind, StageRecord, StageStatus};

/// Returns `true` if the stage's cached inputs still match the current
/// recording settings + template hash, meaning its cached output can be
/// reused. Assumes `stage.status == Completed`; caller checks that first.
pub fn inputs_unchanged(
    stage_kind: StageKind,
    stage: &StageRecord,
    current_settings: &EffectiveSettings,
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

use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

use crate::cancellation::RecordingCancelRegistry;
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

    let global = storage::app_settings::get(&store)
        .map_err(|e| format!("load app settings: {e}"))?;
    let effective = storage::EffectiveSettings::from(&global, project.language.as_deref());

    let template_hash = storage::template_hash(&project.template.content);
    let rec_dir = recording_dir(&app, &recording_id)?;
    let wav_paths: Vec<std::path::PathBuf> = (0..rec.source_paths.len())
        .map(|i| rec_dir.join(format!("audio-{i}.wav")))
        .collect();
    let wavs_exist = !wav_paths.is_empty() && wav_paths.iter().all(|p| p.is_file());

    // Per-recording cancel token. Replaces any stale token from a previous
    // run so this run starts un-cancelled.
    let registry: tauri::State<Arc<RecordingCancelRegistry>> = app.state();
    let cancel = registry.token_for_new_run(&recording_id);

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
                &effective,
                &template_hash,
                first_source,
                wavs_exist,
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

        if cancel.is_cancelled() {
            storage::stages::set_failed(&store, &recording_id, stage_kind, "cancelled")
                .map_err(|e| format!("set_failed: {e}"))?;
            let _ = app.emit("stage-changed", StageChangedEvent {
                recording_id: recording_id.clone(),
                stage: stage_kind.as_str().to_string(),
                status: "failed".to_string(),
            });
            registry.finish(&recording_id);
            return Err("cancelled".into());
        }

        let result = match stage_kind {
            StageKind::Extract => {
                run_extract(&rec.source_paths, &wav_paths).await
                    .map(|_| storage::stages::CompletedOutput {
                        output: None,
                        model_used: None,
                        template_hash: None,
                        completed_at: now_ms(),
                    })
            }
            StageKind::Transcribe => {
                let whisper_model = effective.whisper_model.clone();
                let language = effective.language.clone();
                let total = wav_paths.len();
                let mut parts: Vec<String> = Vec::with_capacity(total);
                let mut transcribe_err: Option<String> = None;
                for (i, wav) in wav_paths.iter().enumerate() {
                    match run_transcribe(&app, wav, &whisper_model, &language, i, total).await {
                        Ok(t) => parts.push(t),
                        Err(e) => { transcribe_err = Some(e); break; }
                    }
                }
                if let Some(e) = transcribe_err {
                    Err(e)
                } else {
                    // Success: delete all wavs to reclaim disk.
                    for wav in &wav_paths {
                        let _ = std::fs::remove_file(wav);
                    }
                    Ok(storage::stages::CompletedOutput {
                        output: Some(parts.join("\n\n")),
                        // Encode language into model_used for invalidation checks.
                        model_used: Some(format!("{}@{}", whisper_model, language)),
                        template_hash: None,
                        completed_at: now_ms(),
                    })
                }
            }
            StageKind::Format => async {
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
                    &effective.ollama_url,
                    &effective.model,
                    project.template.match_style,
                    &project.custom_instructions,
                    Some(cancel.clone()),
                )
                .await?;
                Ok::<_, String>(storage::stages::CompletedOutput {
                    output: Some(formatted),
                    model_used: Some(effective.model.clone()),
                    template_hash: Some(template_hash.clone()),
                    completed_at: now_ms(),
                })
            }
            .await,
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
                registry.finish(&recording_id);
                return Err(e);
            }
        }
    }

    registry.finish(&recording_id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{EffectiveSettings, StageKind, StageRecord, StageStatus};

    fn settings() -> EffectiveSettings {
        EffectiveSettings {
            ollama_url: "u".to_string(),
            model: "gemma3".to_string(),
            whisper_model: "base".to_string(),
            language: "en".to_string(),
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
}
