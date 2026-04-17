use serde::{Deserialize, Serialize};
use std::process::Command;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

use crate::transcription;

// ── Types ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DepsStatus {
    pub ffmpeg: bool,
    pub ollama: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OllamaModel {
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaGenerateRequest {
    model: String,
    prompt: String,
    stream: bool,
}

// ── Dependency Check ───────────────────────────────────────────────────────────

/// Check that ffmpeg and Ollama are available on the system.
#[tauri::command]
pub async fn check_dependencies(ollama_url: String) -> Result<DepsStatus, String> {
    let ffmpeg = Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let ollama = reqwest::Client::new()
        .get(format!("{}/api/tags", ollama_url))
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false);

    Ok(DepsStatus { ffmpeg, ollama })
}

/// Return the list of locally available Ollama models.
#[tauri::command]
pub async fn list_ollama_models(ollama_url: String) -> Result<Vec<String>, String> {
    let resp = reqwest::Client::new()
        .get(format!("{}/api/tags", ollama_url))
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| format!("Cannot reach Ollama: {e}"))?;

    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

    let models = body["models"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|m| m["name"].as_str().map(String::from))
        .collect();

    Ok(models)
}

// ── Audio Extraction ───────────────────────────────────────────────────────────

/// Build the ffmpeg argv (without the `ffmpeg` binary itself) for extracting
/// audio to a 16 kHz mono WAV. For a single input, uses the simple `-vn`
/// shape; for multiple inputs, uses `filter_complex concat` to stitch their
/// audio streams together in order.
pub fn build_ffmpeg_args(input_paths: &[String], output_wav: &std::path::Path) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    for p in input_paths {
        args.push("-i".to_string());
        args.push(p.clone());
    }
    if input_paths.len() > 1 {
        let mut filter = String::new();
        for i in 0..input_paths.len() {
            filter.push_str(&format!("[{i}:a]"));
        }
        filter.push_str(&format!("concat=n={}:v=0:a=1[out]", input_paths.len()));
        args.push("-filter_complex".to_string());
        args.push(filter);
        args.push("-map".to_string());
        args.push("[out]".to_string());
    } else {
        args.push("-vn".to_string());
    }
    args.push("-acodec".to_string());
    args.push("pcm_s16le".to_string());
    args.push("-ar".to_string());
    args.push("16000".to_string());
    args.push("-ac".to_string());
    args.push("1".to_string());
    args.push("-y".to_string());
    args.push(output_wav.to_string_lossy().to_string());
    args
}

/// Extract audio to a caller-chosen output path. Returns the path on success.
pub async fn run_extract(
    input_path: &str,
    output_wav: &std::path::Path,
) -> Result<(), String> {
    if let Some(parent) = output_wav.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create output dir: {e}"))?;
    }
    let output = Command::new("ffmpeg")
        .args([
            "-i", input_path,
            "-vn",
            "-acodec", "pcm_s16le",
            "-ar", "16000",
            "-ac", "1",
            "-y",
            output_wav.to_str().unwrap(),
        ])
        .output()
        .map_err(|e| format!("ffmpeg not found or failed to start: {e}. Install via: brew install ffmpeg"))?;
    if !output.status.success() {
        return Err(format!(
            "ffmpeg error:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

/// Legacy Tauri command — kept so the current UI keeps working during the
/// transition. Writes to a predictable temp path.
#[tauri::command]
pub async fn extract_audio(
    _app: AppHandle,
    input_path: String,
) -> Result<String, String> {
    let temp_wav = std::env::temp_dir().join("meister_audio.wav");
    run_extract(&input_path, &temp_wav).await?;
    Ok(temp_wav.to_string_lossy().to_string())
}

// ── Transcription ──────────────────────────────────────────────────────────────

#[derive(Clone, serde::Serialize)]
struct TranscribeProgress {
    percent: u32,
    message: String,
}

#[derive(Clone, serde::Serialize)]
struct ModelWarming {
    size: String,
}

/// Run whisper on an already-extracted WAV. Returns the transcript text.
pub async fn run_transcribe(
    app: &AppHandle,
    wav_path: &std::path::Path,
    whisper_model: &str,
    language: &str,
) -> Result<String, String> {
    let size = transcription::ModelSize::from_str(whisper_model)
        .ok_or_else(|| format!("unknown model size: {whisper_model}"))?;
    let models_dir = app.path()
        .app_data_dir()
        .map_err(|e| format!("resolve app_data_dir: {e}"))?
        .join("models");

    if transcription::is_warming(size) {
        let _ = app.emit("model-warming", ModelWarming { size: whisper_model.to_string() });
    }

    let app_cb = app.clone();
    let wav = wav_path.to_path_buf();
    let lang = if language.is_empty() || language == "auto" {
        None
    } else {
        Some(language.to_string())
    };

    let result = tokio::task::spawn_blocking(move || {
        transcription::transcribe_wav_with_callback(
            &models_dir,
            size,
            &wav,
            lang.as_deref(),
            move |pct| {
                let _ = app_cb.emit("transcribe-progress", TranscribeProgress {
                    percent: pct.clamp(0, 100) as u32,
                    message: format!("Transcribing… {pct}%"),
                });
            },
        )
    })
    .await
    .map_err(|e| format!("task join: {e}"))?;

    let _ = app.emit("transcribe-progress", TranscribeProgress {
        percent: 100,
        message: "Transcription complete.".to_string(),
    });
    result
}

#[tauri::command]
pub async fn transcribe_audio(
    app: AppHandle,
    wav_path: String,
    whisper_model: String,
    language: String,
) -> Result<String, String> {
    run_transcribe(&app, std::path::Path::new(&wav_path), &whisper_model, &language).await
}

// ── LLM Formatting ─────────────────────────────────────────────────────────────

#[derive(Clone, serde::Serialize)]
struct FormatProgress {
    chars: usize,
    done: bool,
}

/// Call Ollama to format a transcript against a template. Emits
/// `format-progress` events while streaming.
pub async fn run_format(
    app: &AppHandle,
    transcript: &str,
    template_content: &str,
    ollama_url: &str,
    model: &str,
) -> Result<String, String> {
    use futures_util::StreamExt;

    let prompt = build_prompt(transcript, template_content);
    let client = reqwest::Client::new();
    let body = OllamaGenerateRequest {
        model: model.to_string(),
        prompt,
        stream: true,
    };
    let resp = client
        .post(format!("{}/api/generate", ollama_url))
        .json(&body)
        .timeout(std::time::Duration::from_secs(600))
        .send()
        .await
        .map_err(|e| format!("Cannot reach Ollama at {ollama_url}: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Ollama returned {status}: {text}"));
    }
    let mut stream = resp.bytes_stream();
    let mut buf = String::new();
    let mut accumulated = String::new();
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| format!("Ollama stream error: {e}"))?;
        buf.push_str(&String::from_utf8_lossy(&bytes));
        while let Some(nl) = buf.find('\n') {
            let line = buf[..nl].to_string();
            buf.drain(..=nl);
            if line.trim().is_empty() { continue; }
            let obj: serde_json::Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if let Some(piece) = obj.get("response").and_then(|v| v.as_str()) {
                accumulated.push_str(piece);
                let _ = app.emit("format-progress", FormatProgress {
                    chars: accumulated.chars().count(),
                    done: false,
                });
            }
            if obj.get("done").and_then(|v| v.as_bool()).unwrap_or(false) {
                let _ = app.emit("format-progress", FormatProgress {
                    chars: accumulated.chars().count(),
                    done: true,
                });
            }
        }
    }
    Ok(accumulated)
}

#[tauri::command]
pub async fn format_transcript(
    app: AppHandle,
    transcript: String,
    template_content: String,
    ollama_url: String,
    model: String,
) -> Result<String, String> {
    run_format(&app, &transcript, &template_content, &ollama_url, &model).await
}

fn build_prompt(transcript: &str, template: &str) -> String {
    format!(
        r#"You are an expert document formatter. You will be given a raw meeting/recording transcript and a format template.

Your job:
1. Carefully read the entire transcript.
2. Identify and extract all key information: participants, topics discussed, decisions made, action items, dates, and any other relevant details.
3. Produce a clean, well-structured document that strictly follows the layout and style of the FORMAT TEMPLATE below.
4. Do not invent information not present in the transcript. If a section has no content, write "N/A" or omit it gracefully.
5. Output only the final formatted document — no preamble, no explanation.

---
FORMAT TEMPLATE:
{template}
---

RAW TRANSCRIPT:
{transcript}
---

Produce the formatted document now:"#,
        template = template,
        transcript = transcript,
    )
}

// ── File I/O ───────────────────────────────────────────────────────────────────

/// Write a UTF-8 text file (for .md and .txt outputs).
#[tauri::command]
pub async fn save_text_file(content: String, path: String) -> Result<(), String> {
    std::fs::write(&path, content).map_err(|e| format!("Failed to write {path}: {e}"))
}

/// Write a binary file (for .docx / .pdf blobs generated on the frontend).
#[tauri::command]
pub async fn save_binary_file(bytes: Vec<u8>, path: String) -> Result<(), String> {
    std::fs::write(&path, bytes).map_err(|e| format!("Failed to write {path}: {e}"))
}

/// Read any file as UTF-8 text (used to load the template file the user dropped).
#[tauri::command]
pub async fn read_file_as_text(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| format!("Failed to read {path}: {e}"))
}

// ── Model management ───────────────────────────────────────────────────────────

#[derive(Clone, serde::Serialize)]
struct ModelDownloadProgress {
    size: String,
    bytes_downloaded: u64,
    bytes_total: u64,
}

#[tauri::command]
pub async fn list_available_models(
    app: AppHandle,
) -> Result<Vec<transcription::ModelInfo>, String> {
    let models_dir = app.path()
        .app_data_dir()
        .map_err(|e| format!("resolve app_data_dir: {e}"))?
        .join("models");
    Ok(transcription::list_available_models(&models_dir))
}

#[tauri::command]
pub async fn download_model(
    app: AppHandle,
    size: String,
) -> Result<(), String> {
    let model_size = transcription::ModelSize::from_str(&size)
        .ok_or_else(|| format!("unknown model size: {size}"))?;

    let models_dir = app.path()
        .app_data_dir()
        .map_err(|e| format!("resolve app_data_dir: {e}"))?
        .join("models");

    let registry = app.state::<Arc<transcription::CancelRegistry>>();
    registry.reset(&size);
    let token = registry.token_for(&size);

    let app_for_cb = app.clone();
    let size_for_cb = size.clone();
    transcription::download_model(
        &models_dir,
        model_size,
        token,
        move |bytes_downloaded, bytes_total| {
            let _ = app_for_cb.emit("model-download-progress", ModelDownloadProgress {
                size: size_for_cb.clone(),
                bytes_downloaded,
                bytes_total,
            });
        },
    ).await
}

#[tauri::command]
pub fn cancel_model_download(
    app: AppHandle,
    size: String,
) -> Result<(), String> {
    let registry = app.state::<Arc<transcription::CancelRegistry>>();
    registry.token_for(&size).cancel();
    Ok(())
}

// ── Storage commands ──────────────────────────────────────────────────────────

use crate::storage::{self, AppSettings, Project, Recording, Store, Template};

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[tauri::command]
pub async fn list_projects(
    store: tauri::State<'_, Arc<Store>>,
) -> Result<Vec<Project>, String> {
    let store = (*store).clone();
    tokio::task::spawn_blocking(move || storage::projects::list(&store))
        .await
        .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn create_project(
    store: tauri::State<'_, Arc<Store>>,
    name: String,
    default_settings: AppSettings,
    template: Template,
) -> Result<Project, String> {
    let store = (*store).clone();
    tokio::task::spawn_blocking(move || {
        let id = uuid::Uuid::new_v4().to_string();
        let now = now_ms();
        let p = Project {
            id,
            name,
            created_at: now,
            updated_at: now,
            template,
            default_settings,
        };
        storage::projects::insert(&store, &p)?;
        Ok::<Project, String>(p)
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn update_project(
    store: tauri::State<'_, Arc<Store>>,
    project: Project,
) -> Result<(), String> {
    let store = (*store).clone();
    let mut p = project;
    p.updated_at = now_ms();
    tokio::task::spawn_blocking(move || storage::projects::update(&store, &p))
        .await
        .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn delete_project(
    app: AppHandle,
    store: tauri::State<'_, Arc<Store>>,
    id: String,
) -> Result<(), String> {
    let store = (*store).clone();
    let recordings_root = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app_data_dir: {e}"))?
        .join("recordings");
    let id2 = id.clone();
    tokio::task::spawn_blocking(move || {
        // Collect recording IDs before the cascade deletes them, then remove
        // their on-disk folders after the DB delete succeeds.
        let recs = storage::recordings::list_by_project(&store, &id2)?;
        storage::projects::delete(&store, &id2)?;
        for r in recs {
            let _ = std::fs::remove_dir_all(recordings_root.join(&r.id));
        }
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn list_recordings(
    store: tauri::State<'_, Arc<Store>>,
    project_id: String,
) -> Result<Vec<Recording>, String> {
    let store = (*store).clone();
    tokio::task::spawn_blocking(move || storage::recordings::list_by_project(&store, &project_id))
        .await
        .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn get_recording(
    store: tauri::State<'_, Arc<Store>>,
    id: String,
) -> Result<Option<Recording>, String> {
    let store = (*store).clone();
    tokio::task::spawn_blocking(move || storage::recordings::get(&store, &id))
        .await
        .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn create_recording(
    store: tauri::State<'_, Arc<Store>>,
    project_id: String,
    name: String,
    source_paths: Vec<String>,
    settings: AppSettings,
) -> Result<Recording, String> {
    let store = (*store).clone();
    tokio::task::spawn_blocking(move || {
        let id = uuid::Uuid::new_v4().to_string();
        let now = now_ms();
        let r = Recording {
            id,
            project_id,
            name,
            source_paths,
            created_at: now,
            updated_at: now,
            settings,
            stages: vec![],
        };
        storage::recordings::insert(&store, &r)?;
        storage::recordings::get(&store, &r.id)?
            .ok_or_else(|| "recording vanished after insert".to_string())
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn update_recording(
    store: tauri::State<'_, Arc<Store>>,
    recording: Recording,
) -> Result<(), String> {
    let store = (*store).clone();
    let mut r = recording;
    r.updated_at = now_ms();
    tokio::task::spawn_blocking(move || storage::recordings::update(&store, &r))
        .await
        .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn delete_recording(
    app: AppHandle,
    store: tauri::State<'_, Arc<Store>>,
    id: String,
) -> Result<(), String> {
    let store = (*store).clone();
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app_data_dir: {e}"))?
        .join("recordings")
        .join(&id);
    let id2 = id.clone();
    tokio::task::spawn_blocking(move || storage::recordings::delete(&store, &id2))
        .await
        .map_err(|e| format!("join: {e}"))??;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[tauri::command]
pub async fn start_over_recording(
    app: AppHandle,
    store: tauri::State<'_, Arc<Store>>,
    id: String,
) -> Result<(), String> {
    let store = (*store).clone();
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app_data_dir: {e}"))?
        .join("recordings")
        .join(&id);
    let id2 = id.clone();
    tokio::task::spawn_blocking(move || {
        storage::stages::reset_from(&store, &id2, storage::StageKind::Extract)
    })
    .await
    .map_err(|e| format!("join: {e}"))??;
    let _ = std::fs::remove_file(dir.join("audio.wav"));
    Ok(())
}

// ── Save helpers ───────────────────────────────────────────────────────────

/// Placeholder helper kept so the orchestrator has a symbol to import — the
/// actual save step runs on the React side via file dialogs. No-op.
pub async fn run_save_outputs() -> Result<(), String> {
    Ok(())
}

/// Called by the UI after the user finishes picking save paths and writing
/// the output files. Appends to the save stage's saved_paths and marks it
/// completed.
#[tauri::command]
pub async fn record_saved_outputs(
    store: tauri::State<'_, Arc<Store>>,
    recording_id: String,
    saved_paths: Vec<String>,
) -> Result<(), String> {
    let store = (*store).clone();
    tokio::task::spawn_blocking(move || {
        storage::stages::set_completed(
            &store,
            &recording_id,
            storage::StageKind::Save,
            storage::stages::CompletedOutput {
                output: None,
                model_used: None,
                template_hash: None,
                saved_paths: Some(saved_paths),
                completed_at: now_ms(),
            },
        )
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub fn path_exists(path: String) -> bool {
    std::path::Path::new(&path).exists()
}

#[cfg(test)]
mod build_ffmpeg_args_tests {
    use super::build_ffmpeg_args;
    use std::path::PathBuf;

    #[test]
    fn single_file_uses_simple_vn_shape() {
        let paths = vec!["/a.mp4".to_string()];
        let out = PathBuf::from("/tmp/out.wav");
        let args = build_ffmpeg_args(&paths, &out);
        assert_eq!(
            args,
            vec![
                "-i", "/a.mp4",
                "-vn",
                "-acodec", "pcm_s16le",
                "-ar", "16000",
                "-ac", "1",
                "-y",
                "/tmp/out.wav",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
        );
        assert!(!args.iter().any(|a| a == "-filter_complex"));
    }

    #[test]
    fn multi_file_uses_concat_filter() {
        let paths = vec![
            "/a.mp4".to_string(),
            "/b.mov".to_string(),
            "/c.mp3".to_string(),
        ];
        let out = PathBuf::from("/tmp/out.wav");
        let args = build_ffmpeg_args(&paths, &out);

        let input_count = args.iter().filter(|a| *a == "-i").count();
        assert_eq!(input_count, 3);

        let filter_idx = args
            .iter()
            .position(|a| a == "-filter_complex")
            .expect("should have filter_complex");
        assert_eq!(
            args[filter_idx + 1],
            "[0:a][1:a][2:a]concat=n=3:v=0:a=1[out]"
        );

        let map_idx = args.iter().position(|a| a == "-map").expect("should have -map");
        assert_eq!(args[map_idx + 1], "[out]");

        assert!(args.windows(2).any(|w| w[0] == "-acodec" && w[1] == "pcm_s16le"));
        assert!(args.windows(2).any(|w| w[0] == "-ar" && w[1] == "16000"));
        assert!(args.windows(2).any(|w| w[0] == "-ac" && w[1] == "1"));
        assert_eq!(args.last().unwrap(), "/tmp/out.wav");
    }

    #[test]
    fn two_file_concat_string_is_well_formed() {
        let paths = vec!["/a.mp4".to_string(), "/b.mp4".to_string()];
        let out = PathBuf::from("/tmp/out.wav");
        let args = build_ffmpeg_args(&paths, &out);
        let filter_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        assert_eq!(args[filter_idx + 1], "[0:a][1:a]concat=n=2:v=0:a=1[out]");
    }
}
