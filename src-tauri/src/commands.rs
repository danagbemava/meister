use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use tauri::{AppHandle, Emitter};

// ── Types ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DepsStatus {
    pub ffmpeg: bool,
    pub whisper: bool,
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

#[derive(Debug, Serialize, Deserialize)]
struct OllamaGenerateResponse {
    response: String,
}

// ── Dependency Check ───────────────────────────────────────────────────────────

/// Check that ffmpeg, whisper, and Ollama are available on the system.
#[tauri::command]
pub async fn check_dependencies(ollama_url: String) -> Result<DepsStatus, String> {
    let ffmpeg = Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    // whisper can be invoked as `whisper` (openai-whisper) or `whisper-cpp`
    let whisper = Command::new("whisper")
        .arg("--help")
        .output()
        .map(|_| true)
        .unwrap_or(false);

    let ollama = reqwest::Client::new()
        .get(format!("{}/api/tags", ollama_url))
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false);

    Ok(DepsStatus { ffmpeg, whisper, ollama })
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

/// Use ffmpeg to extract/convert the input file to a 16 kHz mono WAV suitable
/// for Whisper. Returns the path to the temporary WAV file.
#[tauri::command]
pub async fn extract_audio(
    _app: AppHandle,
    input_path: String,
) -> Result<String, String> {
    // Write to a predictable temp location so we can clean it up later.
    let temp_wav = std::env::temp_dir().join("meister_audio.wav");

    let output = Command::new("ffmpeg")
        .args([
            "-i",
            &input_path,
            "-vn",                        // drop video stream
            "-acodec", "pcm_s16le",       // 16-bit PCM
            "-ar", "16000",               // 16 kHz
            "-ac", "1",                   // mono
            "-y",                         // overwrite if exists
            temp_wav.to_str().unwrap(),
        ])
        .output()
        .map_err(|e| format!("ffmpeg not found or failed to start: {e}. Install via: brew install ffmpeg"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffmpeg error:\n{stderr}"));
    }

    Ok(temp_wav.to_string_lossy().to_string())
}

// ── Transcription ──────────────────────────────────────────────────────────────

/// Run openai-whisper on the WAV file and return the raw transcript text.
/// Streams progress events (`transcribe-progress`) to the frontend as Whisper
/// processes each audio chunk.
#[tauri::command]
pub async fn transcribe_audio(
    app: AppHandle,
    wav_path: String,
    whisper_model: String,
    language: String,
) -> Result<String, String> {
    // Output goes to a temp folder we control.
    let out_dir = std::env::temp_dir().join("meister_whisper");
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;

    let mut cmd = Command::new("whisper");
    // Force Python to flush stdout/stderr immediately so we get real-time output.
    // Without this, Python block-buffers stdout when piped (no TTY).
    cmd.env("PYTHONUNBUFFERED", "1");
    cmd.args([
        &wav_path,
        "--model", &whisper_model,
        "--output_format", "txt",
        "--output_dir", out_dir.to_str().unwrap(),
        // Verbose mode prints timestamps for each segment, which we parse for progress.
        "--verbose", "True",
    ]);
    if !language.is_empty() && language != "auto" {
        cmd.args(["--language", &language]);
    }

    // Spawn the process so we can stream stdout + stderr in real time.
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("whisper not found: {e}. Install via: pip install openai-whisper"))?;

    let stderr = child.stderr.take().expect("stderr was piped");
    let stdout = child.stdout.take().expect("stdout was piped");

    let re_timestamp = regex::Regex::new(
        r"\[(\d{2}):(\d{2})\.(\d{3})\s*-->\s*(\d{2}):(\d{2})\.(\d{3})\]"
    ).unwrap();
    let re_percent = regex::Regex::new(r"(\d+)%\|").unwrap();

    let audio_duration_ms = get_wav_duration_ms();

    // Read stdout in a thread — whisper --verbose True prints timestamp lines here.
    let app_stdout = app.clone();
    let re_ts = re_timestamp.clone();
    let stdout_handle = std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().flatten() {
            if let Some(caps) = re_ts.captures(&line) {
                let end_min: u64 = caps[4].parse().unwrap_or(0);
                let end_sec: u64 = caps[5].parse().unwrap_or(0);
                let end_ms: u64 = caps[6].parse().unwrap_or(0);
                let end_total_ms = end_min * 60_000 + end_sec * 1_000 + end_ms;

                if let Some(total) = audio_duration_ms {
                    if total > 0 {
                        let pct = ((end_total_ms as f64 / total as f64) * 100.0)
                            .min(99.0) as u32;
                        let _ = app_stdout.emit("transcribe-progress", TranscribeProgress {
                            percent: pct,
                            message: format!("Transcribing… {pct}%"),
                        });
                    }
                }
            }
        }
    });

    // Read stderr byte-by-byte — tqdm uses \r to overwrite progress in-place,
    // so we split on both \r and \n to catch each update.
    let app_stderr = app.clone();
    let stderr_handle = std::thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        let mut collected = String::new();
        let mut buf = [0u8; 1];
        let mut line = String::new();

        loop {
            match std::io::Read::read(&mut reader, &mut buf) {
                Ok(0) => break, // EOF
                Ok(_) => {
                    let ch = buf[0] as char;
                    if ch == '\r' || ch == '\n' {
                        if !line.is_empty() {
                            // Check for tqdm percentage
                            if let Some(caps) = re_percent.captures(&line) {
                                if let Ok(pct) = caps[1].parse::<u32>() {
                                    let _ = app_stderr.emit("transcribe-progress", TranscribeProgress {
                                        percent: pct,
                                        message: format!("Processing audio… {pct}%"),
                                    });
                                }
                            }
                            // Also check for timestamps in stderr (some whisper versions)
                            if let Some(caps) = re_timestamp.captures(&line) {
                                let end_min: u64 = caps[4].parse().unwrap_or(0);
                                let end_sec: u64 = caps[5].parse().unwrap_or(0);
                                let end_ms: u64 = caps[6].parse().unwrap_or(0);
                                let end_total_ms = end_min * 60_000 + end_sec * 1_000 + end_ms;
                                if let Some(total) = audio_duration_ms {
                                    if total > 0 {
                                        let pct = ((end_total_ms as f64 / total as f64) * 100.0)
                                            .min(99.0) as u32;
                                        let _ = app_stderr.emit("transcribe-progress", TranscribeProgress {
                                            percent: pct,
                                            message: format!("Transcribing… {pct}%"),
                                        });
                                    }
                                }
                            }
                            collected.push_str(&line);
                            collected.push('\n');
                            line.clear();
                        }
                    } else {
                        line.push(ch);
                    }
                }
                Err(_) => break,
            }
        }

        collected
    });

    // Use spawn_blocking so we don't block the Tokio async runtime.
    let (status, stderr_output) = tokio::task::spawn_blocking(move || {
        let status = child.wait();
        let _ = stdout_handle.join();
        let stderr_output = stderr_handle.join().unwrap_or_default();
        (status, stderr_output)
    })
    .await
    .map_err(|e| format!("task join error: {e}"))?;

    let status = status.map_err(|e| format!("whisper process error: {e}"))?;

    // Emit 100% on completion
    let _ = app.emit("transcribe-progress", TranscribeProgress {
        percent: 100,
        message: "Transcription complete.".to_string(),
    });

    if !status.success() {
        return Err(format!("Whisper error:\n{stderr_output}"));
    }

    // Whisper names the output file after the input file, with .txt extension.
    let stem = std::path::Path::new(&wav_path)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let txt_path = out_dir.join(format!("{stem}.txt"));

    if !txt_path.exists() {
        let mut found = None;
        for entry in std::fs::read_dir(&out_dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.path().extension().map_or(false, |x| x == "txt") {
                found = Some(entry.path());
                break;
            }
        }
        match found {
            Some(p) => return std::fs::read_to_string(p).map_err(|e| e.to_string()),
            None => return Err("Whisper finished but no .txt transcript was found.".to_string()),
        }
    }

    std::fs::read_to_string(&txt_path).map_err(|e| e.to_string())
}

#[derive(Clone, Serialize)]
struct TranscribeProgress {
    percent: u32,
    message: String,
}

/// Try to get the duration of the meister temp WAV by reading the RIFF header.
fn get_wav_duration_ms() -> Option<u64> {
    let path = std::env::temp_dir().join("meister_audio.wav");
    let data = std::fs::read(&path).ok()?;
    // WAV header: bytes 24-27 = sample rate (u32 LE), bytes 34-35 = bits per sample (u16 LE)
    // Total data size is file size minus 44 bytes of header (standard PCM WAV).
    if data.len() < 44 { return None; }
    let sample_rate = u32::from_le_bytes(data[24..28].try_into().ok()?) as u64;
    let bits_per_sample = u16::from_le_bytes(data[34..36].try_into().ok()?) as u64;
    let channels = u16::from_le_bytes(data[22..24].try_into().ok()?) as u64;
    if sample_rate == 0 || bits_per_sample == 0 || channels == 0 { return None; }
    let data_size = (data.len() as u64).saturating_sub(44);
    let bytes_per_sample = bits_per_sample / 8;
    let total_samples = data_size / (bytes_per_sample * channels);
    Some(total_samples * 1000 / sample_rate)
}

// ── LLM Formatting ─────────────────────────────────────────────────────────────

/// Send the transcript + template to an Ollama model and return the formatted output.
#[tauri::command]
pub async fn format_transcript(
    transcript: String,
    template_content: String,
    ollama_url: String,
    model: String,
) -> Result<String, String> {
    let prompt = build_prompt(&transcript, &template_content);

    let client = reqwest::Client::new();
    let body = OllamaGenerateRequest {
        model: model.clone(),
        prompt,
        stream: false,
    };

    let resp = client
        .post(format!("{}/api/generate", ollama_url))
        .json(&body)
        .timeout(std::time::Duration::from_secs(300))
        .send()
        .await
        .map_err(|e| format!("Cannot reach Ollama at {ollama_url}: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Ollama returned {status}: {text}"));
    }

    let result: OllamaGenerateResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse Ollama response: {e}"))?;

    Ok(result.response)
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

/// Clean up temp files created during processing.
#[tauri::command]
pub async fn cleanup_temp() -> Result<(), String> {
    let wav = std::env::temp_dir().join("meister_audio.wav");
    let whisper_dir = std::env::temp_dir().join("meister_whisper");
    let _ = std::fs::remove_file(wav);
    let _ = std::fs::remove_dir_all(whisper_dir);
    Ok(())
}
