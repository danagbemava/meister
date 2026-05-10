use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
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
/// one input file's audio to a 16 kHz mono WAV.
pub fn build_ffmpeg_args(input_path: &str, output_wav: &std::path::Path) -> Vec<String> {
    vec![
        "-i".to_string(),
        input_path.to_string(),
        "-vn".to_string(),
        "-acodec".to_string(),
        "pcm_s16le".to_string(),
        "-ar".to_string(),
        "16000".to_string(),
        "-ac".to_string(),
        "1".to_string(),
        "-y".to_string(),
        output_wav.to_string_lossy().to_string(),
    ]
}

/// Extract audio from each input file into its corresponding 16 kHz mono WAV.
/// One ffmpeg invocation per source — sources are kept separate so whisper
/// processes them independently (avoids long-form repetition loops that hit
/// when multiple sources are stitched before transcription).
pub async fn run_extract(
    input_paths: &[String],
    output_wavs: &[std::path::PathBuf],
) -> Result<(), String> {
    if input_paths.is_empty() {
        return Err("no source files provided".to_string());
    }
    if input_paths.len() != output_wavs.len() {
        return Err(format!(
            "input/output count mismatch: {} sources, {} outputs",
            input_paths.len(),
            output_wavs.len()
        ));
    }
    let missing: Vec<&str> = input_paths
        .iter()
        .filter(|p| !std::path::Path::new(p).exists())
        .map(|s| s.as_str())
        .collect();
    if !missing.is_empty() {
        return Err(format!("source file(s) missing:\n  {}", missing.join("\n  ")));
    }
    for (input, output) in input_paths.iter().zip(output_wavs.iter()) {
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create output dir: {e}"))?;
        }
        let args = build_ffmpeg_args(input, output);
        let result = Command::new("ffmpeg")
            .args(&args)
            .output()
            .map_err(|e| format!("ffmpeg not found or failed to start: {e}. Install via: brew install ffmpeg"))?;
        if !result.status.success() {
            return Err(format!(
                "ffmpeg error extracting {input}:\n{}",
                String::from_utf8_lossy(&result.stderr)
            ));
        }
    }
    Ok(())
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
///
/// Emits `transcribe-progress` events with a percent scaled across the whole
/// batch: source `source_idx` of `total_sources` contributes `1/total_sources`
/// of the overall progress. The caller is responsible for the outer loop over
/// sources. The final "complete" event (percent=100) fires only on the last
/// source; intermediate sources leave the bar where they ended.
pub async fn run_transcribe(
    app: &AppHandle,
    wav_path: &std::path::Path,
    whisper_model: &str,
    language: &str,
    source_idx: usize,
    total_sources: usize,
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
    let total = total_sources.max(1) as f32;
    let idx = source_idx as f32;
    let label_idx = source_idx + 1;

    let result = tokio::task::spawn_blocking(move || {
        transcription::transcribe_wav_with_callback(
            &models_dir,
            size,
            &wav,
            lang.as_deref(),
            move |pct| {
                let per_file = pct.clamp(0, 100) as f32;
                let overall = ((idx * 100.0 + per_file) / total).round() as u32;
                let msg = if total > 1.0 {
                    format!("Transcribing source {label_idx}/{total_sources}… {pct}%")
                } else {
                    format!("Transcribing… {pct}%")
                };
                let _ = app_cb.emit("transcribe-progress", TranscribeProgress {
                    percent: overall.min(100),
                    message: msg,
                });
            },
        )
    })
    .await
    .map_err(|e| format!("task join: {e}"))?;

    if source_idx + 1 == total_sources {
        let _ = app.emit("transcribe-progress", TranscribeProgress {
            percent: 100,
            message: "Transcription complete.".to_string(),
        });
    }
    result
}

// ── Formatting pipeline ────────────────────────────────────────────────────────

/// Conservative text cleanup applied before sending a transcript to the
/// formatter. Strips well-known filler tokens and 3+ stutters; normalizes
/// whitespace while preserving paragraph breaks; normalizes em-dashes and
/// ellipses. Idempotent.
pub(crate) fn preprocess(input: &str) -> String {
    use std::sync::OnceLock;
    static FILLER_RE: OnceLock<regex::Regex> = OnceLock::new();
    static PARA_RE: OnceLock<regex::Regex> = OnceLock::new();
    static WS_RE: OnceLock<regex::Regex> = OnceLock::new();

    let mut s = input.replace('—', "-").replace('…', "...");

    // `(?i)\b…\b` boundaries mean "Uber", "umbrella", "Ahmad" are safe.
    let filler = FILLER_RE.get_or_init(|| {
        regex::Regex::new(r"(?i)\b(uh|um|uhm|er|erm|hmm|ah)\b[,.!?]?\s*").unwrap()
    });
    s = filler.replace_all(&s, "").to_string();

    // The regex crate doesn't support backreferences, so stutter collapsing
    // uses a token scanner rather than a single regex.
    s = collapse_stutters(&s);

    let para = PARA_RE.get_or_init(|| regex::Regex::new(r"\n\s*\n\s*").unwrap());
    s = para.replace_all(&s, "\u{0000}").to_string(); // placeholder for para break
    let ws = WS_RE.get_or_init(|| regex::Regex::new(r"\s+").unwrap());
    s = ws.replace_all(&s, " ").to_string();
    s = s.replace('\u{0000}', "\n\n");

    s.trim().to_string()
}

/// Collapse runs of 3+ identical words (case-insensitive) down to a single
/// instance, preserving the first word's casing. Two consecutive identical
/// words ("very very") are kept as-is (emphasis).
fn collapse_stutters(input: &str) -> String {
    struct Token<'a> {
        word: &'a str,
        gap: &'a str, // whitespace immediately after this word
    }

    let mut tokens: Vec<Token<'_>> = Vec::new();
    let len = input.len();

    let leading_end = input
        .char_indices()
        .find(|(_, c)| !c.is_whitespace())
        .map(|(idx, _)| idx)
        .unwrap_or(len);
    let mut i = leading_end;

    while i < len {
        let word_start = i;
        while i < len {
            if input[i..].starts_with(|c: char| c.is_whitespace()) {
                break;
            }
            let ch_len = input[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
            i += ch_len;
        }
        let word = &input[word_start..i];

        let gap_start = i;
        while i < len && input[i..].starts_with(|c: char| c.is_whitespace()) {
            let ch_len = input[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
            i += ch_len;
        }
        let gap = &input[gap_start..i];

        if !word.is_empty() {
            tokens.push(Token { word, gap });
        }
    }

    let mut result = String::with_capacity(input.len());
    result.push_str(&input[..leading_end]);
    let mut t = 0;
    while t < tokens.len() {
        let word = tokens[t].word;
        let mut run = 1;
        while t + run < tokens.len() && tokens[t + run].word.eq_ignore_ascii_case(word) {
            run += 1;
        }
        if run >= 3 {
            result.push_str(word);
            result.push_str(tokens[t + run - 1].gap);
            t += run;
        } else {
            for tok in &tokens[t..t + run] {
                result.push_str(tok.word);
                result.push_str(tok.gap);
            }
            t += run;
        }
    }

    result
}

/// Token-count thresholds.
pub(crate) const THRESHOLD: usize = 4000;
pub(crate) const CHUNK_TARGET: usize = 3000;
pub(crate) const CHUNK_OVERLAP: usize = 200;
/// Approximation: 1 token ≈ 4 chars.
const CHARS_PER_TOKEN: usize = 4;
const BOUNDARY_SEARCH_WINDOW_CHARS: usize = 500;

fn approx_tokens(s: &str) -> usize {
    s.chars().count() / CHARS_PER_TOKEN
}

/// Re-exports for sibling modules (e.g. `ask::strategy`) that need the same
/// threshold boundary. Keeps the original symbols private to `commands`.
pub(crate) const THRESHOLD_PUB: usize = THRESHOLD;
pub(crate) fn approx_tokens_pub(s: &str) -> usize {
    approx_tokens(s)
}
pub(crate) fn split_into_chunks_pub(s: &str) -> Vec<String> { split_into_chunks(s) }
pub(crate) fn preprocess_pub(s: &str) -> String { preprocess(s) }

/// Advance `i` forward until it lands on a UTF-8 char boundary, then clamp to
/// `s.len()`. Ensures byte-offset slices into `s` never split a codepoint.
fn snap_to_char_boundary(s: &str, mut i: usize) -> usize {
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i.min(s.len())
}

/// Split a cleaned transcript into chunks at sentence boundaries, with a
/// small overlap prefix on each chunk after the first. Single-chunk if the
/// input is below THRESHOLD tokens.
pub(crate) fn split_into_chunks(input: &str) -> Vec<String> {
    if approx_tokens(input) < THRESHOLD {
        return vec![input.to_string()];
    }

    let target_chars = CHUNK_TARGET * CHARS_PER_TOKEN;
    let search_window = BOUNDARY_SEARCH_WINDOW_CHARS;

    let bytes = input.as_bytes();
    let len = bytes.len();

    let mut chunks: Vec<String> = Vec::new();
    let mut start: usize = 0; // byte index

    while start < len {
        debug_assert!(input.is_char_boundary(start));
        let remaining = &input[start..];
        if remaining.len() <= target_chars + search_window {
            chunks.push(prepend_overlap(&chunks, remaining));
            break;
        }

        let target = target_chars.min(remaining.len());

        let lo = snap_to_char_boundary(remaining, target.saturating_sub(search_window));
        let hi = snap_to_char_boundary(remaining, (target + search_window).min(remaining.len()));
        let window = &remaining[lo..hi];

        let split_rel = find_last_sentence_boundary(window)
            .or_else(|| find_last_whitespace(window))
            .unwrap_or(window.len());

        let split_abs = lo + split_rel;
        let body = &remaining[..split_abs];
        chunks.push(prepend_overlap(&chunks, body));

        let mut next = start + split_abs;
        while next < len && matches!(bytes[next], b' ' | b'\t' | b'\n' | b'\r') {
            next += 1;
        }
        start = next;
    }

    chunks
        .into_iter()
        .filter(|c| !c.trim().is_empty())
        .collect()
}

/// Returns the byte index (within `s`) of the character just after the last
/// sentence-terminator+whitespace sequence, or None if none found.
fn find_last_sentence_boundary(s: &str) -> Option<usize> {
    let mut last: Option<usize> = None;
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        let c = bytes[i];
        if (c == b'.' || c == b'?' || c == b'!')
            && matches!(bytes[i + 1], b' ' | b'\t' | b'\n')
        {
            last = Some(i + 2);
        }
        i += 1;
    }
    last
}

fn find_last_whitespace(s: &str) -> Option<usize> {
    s.rfind(|c: char| c.is_whitespace())
        .map(|i| i + s[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1))
}

/// Prepend CHUNK_OVERLAP tokens' worth of tail from the last existing chunk
/// (if any) to `body`, yielding the next chunk's content.
fn prepend_overlap(existing: &[String], body: &str) -> String {
    let Some(prev) = existing.last() else {
        return body.trim_start().to_string();
    };
    let overlap_chars = CHUNK_OVERLAP * CHARS_PER_TOKEN;
    let tail_start = prev.len().saturating_sub(overlap_chars);
    // Snap to a char boundary.
    let tail_start = (tail_start..=prev.len())
        .find(|&i| prev.is_char_boundary(i))
        .unwrap_or(prev.len());
    let tail = &prev[tail_start..];
    format!("{tail}\n\n{}", body.trim_start())
}

/// SHA-256 over `transcript || 0x00 || template || 0x00 || model || 0x00 ||
/// match_style_byte || 0x00 || custom_instructions` → hex.
pub(crate) fn cache_key(
    transcript: &str,
    template: &str,
    model: &str,
    match_style: bool,
    custom_instructions: &str,
) -> String {
    let mut h = Sha256::new();
    h.update(transcript.as_bytes());
    h.update([0u8]);
    h.update(template.as_bytes());
    h.update([0u8]);
    h.update(model.as_bytes());
    h.update([0u8]);
    h.update([if match_style { 1u8 } else { 0u8 }]);
    h.update([0u8]);
    h.update(custom_instructions.as_bytes());
    let digest = h.finalize();
    let mut out = String::with_capacity(64);
    for b in digest.iter() {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

pub(crate) fn cache_read(cache_dir: &Path, key: &str) -> Option<String> {
    let path = cache_dir.join(format!("{key}.md"));
    std::fs::read_to_string(&path).ok()
}

pub(crate) fn cache_write(cache_dir: &Path, key: &str, content: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(cache_dir)?;
    std::fs::write(cache_dir.join(format!("{key}.md")), content)
}

/// Generic Ollama `/api/generate` streaming helper. Accumulates the response
/// body, invoking `on_token(piece)` for every streamed `response` field.
///
/// Honors `keep_alive: "30m"`, fixed REQUEST_TIMEOUT_SECS / INACTIVITY_TIMEOUT_SECS,
/// and races the read against `cancel.cancelled()` (returns `Err("cancelled")`
/// promptly if tripped during an await).
///
/// `on_token` is `Send` so this future can be polled inside a Tauri command
/// (Tauri requires command futures to be `Send`).
pub(crate) async fn ollama_stream(
    ollama_url: &str,
    model: &str,
    prompt: String,
    num_ctx: u32,
    on_token: &mut (dyn FnMut(&str) + Send),
    cancel: Option<&crate::cancellation::CancelToken>,
) -> Result<String, String> {
    use futures_util::StreamExt;

    #[derive(serde::Serialize)]
    struct Options {
        num_ctx: u32,
        num_predict: i32,
    }
    #[derive(serde::Serialize)]
    struct Req<'a> {
        model: &'a str,
        prompt: String,
        stream: bool,
        keep_alive: &'a str,
        options: Options,
    }

    const REQUEST_TIMEOUT_SECS: u64 = 480;
    const INACTIVITY_TIMEOUT_SECS: u64 = 120;

    let body = Req {
        model,
        prompt,
        stream: true,
        keep_alive: "30m",
        options: Options { num_ctx, num_predict: -1 },
    };

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/generate", ollama_url))
        .json(&body)
        .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
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
    loop {
        let cancel_fut = async {
            match cancel {
                Some(c) => c.cancelled().await,
                None => std::future::pending::<()>().await,
            }
        };
        let next_fut = tokio::time::timeout(
            std::time::Duration::from_secs(INACTIVITY_TIMEOUT_SECS),
            stream.next(),
        );
        let next = tokio::select! {
            biased;
            _ = cancel_fut => return Err("cancelled".into()),
            r = next_fut => r,
        };
        let chunk_bytes = match next {
            Ok(Some(c)) => c,
            Ok(None) => break,
            Err(_) => {
                return Err(format!(
                    "Ollama at {ollama_url} stopped responding (no data for {INACTIVITY_TIMEOUT_SECS}s). The model may have been unloaded or the server crashed. Try: pkill ollama && OLLAMA_NUM_PARALLEL=3 ollama serve, then re-run."
                ));
            }
        };
        let bytes = chunk_bytes.map_err(|e| format!("Ollama stream error: {e}"))?;
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
                on_token(piece);
            }
        }
    }
    Ok(accumulated)
}

/// Call Ollama /api/generate with streaming, forward tokens to `on_progress`
/// with the given phase (and optional chunk info). Returns the accumulated
/// output. Honors `keep_alive: "30m"` and explicit `num_ctx` / `num_predict`.
async fn ollama_generate_stream(
    ollama_url: &str,
    model: &str,
    prompt: String,
    num_ctx: u32,
    phase: FormatPhase,
    chunk: Option<ChunkInfo>,
    on_progress: &(dyn Fn(FormatProgress) + Send + Sync),
    cancel: Option<&crate::cancellation::CancelToken>,
) -> Result<String, String> {
    let mut accumulated_chars: usize = 0;
    let mut on_token = |piece: &str| {
        accumulated_chars += piece.chars().count();
        on_progress(FormatProgress {
            phase,
            chunk: chunk.clone(),
            chars: accumulated_chars,
            done: false,
        });
    };
    ollama_stream(ollama_url, model, prompt, num_ctx, &mut on_token, cancel).await
}

fn build_map_prompt(chunk: &str, index: usize, total: usize) -> String {
    format!(
        r#"You are an expert note-taker. You will be given ONE SEGMENT of a longer transcript. This is part {part_num} of {total} parts.

Your job:
1. Read the segment carefully.
2. Produce concise, structured markdown notes that capture everything a downstream document formatter will need to reconstruct what was said: participants mentioned, topics discussed, decisions, action items, dates, specific numbers, and short verbatim quotes worth preserving.
3. Do NOT invent information not present in the segment. If the segment is mostly silence/filler, output "(no substantive content in this segment)".
4. Do NOT write a finished document. These notes are intermediate.
5. Where relevant, flag references to earlier or later segments (e.g. "as discussed earlier", "we'll come back to X").

---
SEGMENT (part {part_num} of {total}):
{chunk}
---

Produce the notes now:"#,
        part_num = index + 1,
        total = total,
        chunk = chunk,
    )
}

/// Run the map-pass prompt against Ollama for one chunk. Emits phase=Map
/// tokens with `chars` set to a running total via `chars_running` (shared
/// across all concurrent chunks).
async fn map_pass(
    ollama_url: &str,
    model: &str,
    chunk: &str,
    index: usize,
    total: usize,
    completed: Arc<std::sync::atomic::AtomicUsize>,
    chars_running: Arc<std::sync::atomic::AtomicUsize>,
    on_progress: Arc<dyn Fn(FormatProgress) + Send + Sync>,
    cancel: Option<crate::cancellation::CancelToken>,
) -> Result<(usize, String), String> {
    use std::sync::atomic::Ordering;

    let prompt = build_map_prompt(chunk, index, total);
    let on_progress_inner = on_progress.clone();
    let chars_running_inner = chars_running.clone();
    let completed_inner = completed.clone();
    let emit = move |p: FormatProgress| {
        let this_chars = p.chars;
        let prev = chars_running_inner.swap(
            chars_running_inner.load(Ordering::Relaxed).max(this_chars),
            Ordering::Relaxed,
        );
        let total_chars = prev.max(this_chars);
        on_progress_inner(FormatProgress {
            phase: FormatPhase::Map,
            chunk: Some(ChunkInfo {
                index,
                total,
                completed: completed_inner.load(Ordering::Relaxed),
            }),
            chars: total_chars,
            done: false,
        });
    };

    let emit_boxed: Box<dyn Fn(FormatProgress) + Send + Sync> = Box::new(emit);
    let out = ollama_generate_stream(
        ollama_url,
        model,
        prompt,
        6144,
        FormatPhase::Map,
        Some(ChunkInfo { index, total, completed: completed.load(Ordering::Relaxed) }),
        &*emit_boxed,
        cancel.as_ref(),
    )
    .await?;

    let now_completed = completed.fetch_add(1, Ordering::Relaxed) + 1;
    on_progress(FormatProgress {
        phase: FormatPhase::Map,
        chunk: Some(ChunkInfo { index, total, completed: now_completed }),
        chars: chars_running.load(Ordering::Relaxed),
        done: false,
    });

    Ok((index, out))
}

fn stitch_notes(notes: &[(usize, String)]) -> String {
    let total = notes.len();
    let mut out = String::new();
    for (i, body) in notes {
        out.push_str(&format!("### Notes from part {} of {}\n\n", i + 1, total));
        out.push_str(body.trim());
        out.push_str("\n\n");
    }
    out.trim_end().to_string()
}

async fn reduce_pass(
    ollama_url: &str,
    model: &str,
    stitched_notes: &str,
    template: &str,
    match_style: bool,
    custom_instructions: &str,
    on_progress: Arc<dyn Fn(FormatProgress) + Send + Sync>,
    cancel: Option<crate::cancellation::CancelToken>,
) -> Result<String, String> {
    let prompt = build_prompt(stitched_notes, template, match_style, custom_instructions);

    // Emit an immediate Reduce-phase event so the UI flips from
    // "Analyzing chunk N/N" to "Composing output…" while we wait for
    // Ollama's first byte (cold model + large prompt can take 30s+).
    on_progress(FormatProgress {
        phase: FormatPhase::Reduce,
        chunk: None,
        chars: 0,
        done: false,
    });

    let emit = {
        let on_progress = on_progress.clone();
        move |p: FormatProgress| {
            on_progress(FormatProgress {
                phase: FormatPhase::Reduce,
                chunk: None,
                chars: p.chars,
                done: false,
            });
        }
    };
    let emit_boxed: Box<dyn Fn(FormatProgress) + Send + Sync> = Box::new(emit);
    ollama_generate_stream(
        ollama_url,
        model,
        prompt,
        12288,
        FormatPhase::Reduce,
        None,
        &*emit_boxed,
        cancel.as_ref(),
    )
    .await
}

// ── LLM Formatting ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, serde::Serialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub(crate) enum FormatPhase {
    Single,
    Map,
    Reduce,
}

#[derive(Clone, serde::Serialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChunkInfo {
    pub index: usize,
    pub total: usize,
    pub completed: usize,
}

#[derive(Clone, serde::Serialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FormatProgress {
    pub phase: FormatPhase,
    pub chunk: Option<ChunkInfo>,
    pub chars: usize,
    pub done: bool,
}

/// Core formatting logic, testable without an `AppHandle`. Owns the
/// cache lookup, preprocessing, threshold-based branching, map-reduce
/// orchestration, and final cache write.
pub(crate) async fn run_format_core(
    transcript: &str,
    template: &str,
    ollama_url: &str,
    model: &str,
    match_style: bool,
    custom_instructions: &str,
    cache_dir: &Path,
    on_progress: Arc<dyn Fn(FormatProgress) + Send + Sync>,
    cancel: Option<crate::cancellation::CancelToken>,
) -> Result<String, String> {
    use futures_util::{stream, StreamExt};
    use std::sync::atomic::AtomicUsize;

    let key = cache_key(transcript, template, model, match_style, custom_instructions);
    if let Some(hit) = cache_read(cache_dir, &key) {
        on_progress(FormatProgress {
            phase: FormatPhase::Single,
            chunk: None,
            chars: hit.chars().count(),
            done: true,
        });
        return Ok(hit);
    }

    let cleaned = preprocess(transcript);

    let final_output = if approx_tokens(&cleaned) < THRESHOLD {
        let prompt = build_prompt(&cleaned, template, match_style, custom_instructions);
        let single_cb = {
            let on_progress = on_progress.clone();
            move |p: FormatProgress| on_progress(p)
        };
        let boxed: Box<dyn Fn(FormatProgress) + Send + Sync> = Box::new(single_cb);
        let out = ollama_generate_stream(
            ollama_url, model, prompt, 8192,
            FormatPhase::Single, None, &*boxed,
            cancel.as_ref(),
        ).await?;
        on_progress(FormatProgress {
            phase: FormatPhase::Single,
            chunk: None,
            chars: out.chars().count(),
            done: true,
        });
        out
    } else {
        let chunks = split_into_chunks(&cleaned);
        let total = chunks.len();
        let completed = Arc::new(AtomicUsize::new(0));
        let chars_running = Arc::new(AtomicUsize::new(0));

        let url = ollama_url.to_string();
        let model_s = model.to_string();

        // With default Ollama (OLLAMA_NUM_PARALLEL=1), parallel requests queue
        // at the server. Each queued request sits for ~T seconds where T is
        // the per-chunk processing time; if T * (concurrency-1) exceeds the
        // 120s watchdog, queued requests fail with "stopped responding".
        //
        // Heuristic: only attempt parallelism if every chunk is small enough
        // that T * (concurrency-1) stays under the watchdog. We approximate T
        // from chunk char count (rough: <2000 chars ≈ 30s on warm Gemma).
        let avg_chunk_chars = chunks.iter().map(|c| c.len()).sum::<usize>() / chunks.len().max(1);
        let concurrency = if chunks.len() == 1 {
            1
        } else if chunks.len() <= 3 && avg_chunk_chars < 2000 {
            chunks.len()
        } else {
            1
        };

        let results: Vec<Result<(usize, String), String>> = stream::iter(
            chunks.into_iter().enumerate()
        )
        .map(|(i, chunk_text)| {
            let url = url.clone();
            let model_s = model_s.clone();
            let completed = completed.clone();
            let chars_running = chars_running.clone();
            let on_progress = on_progress.clone();
            let cancel = cancel.clone();
            async move {
                map_pass(&url, &model_s, &chunk_text, i, total, completed, chars_running, on_progress, cancel).await
            }
        })
        .buffer_unordered(concurrency)
        .collect()
        .await;

        let mut notes: Vec<(usize, String)> = Vec::with_capacity(total);
        for r in results {
            notes.push(r?);
        }
        notes.sort_by_key(|(i, _)| *i);

        let stitched = stitch_notes(&notes);
        let reduced = reduce_pass(ollama_url, model, &stitched, template, match_style, custom_instructions, on_progress.clone(), cancel.clone()).await?;

        on_progress(FormatProgress {
            phase: FormatPhase::Reduce,
            chunk: None,
            chars: reduced.chars().count(),
            done: true,
        });
        reduced
    };

    if let Err(e) = cache_write(cache_dir, &key, &final_output) {
        eprintln!("format cache write failed: {e}");
    }

    Ok(final_output)
}

/// Call Ollama to format a transcript against a template. Emits
/// `format-progress` events while streaming. Thin wrapper over
/// `run_format_core` that resolves the cache directory from the AppHandle.
pub async fn run_format(
    app: &AppHandle,
    transcript: &str,
    template_content: &str,
    ollama_url: &str,
    model: &str,
    match_style: bool,
    custom_instructions: &str,
    cancel: Option<crate::cancellation::CancelToken>,
) -> Result<String, String> {
    let cache_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app_data_dir: {e}"))?
        .join("format-cache");

    let app_handle = app.clone();
    let on_progress: Arc<dyn Fn(FormatProgress) + Send + Sync> = Arc::new(move |p| {
        let _ = app_handle.emit("format-progress", p);
    });

    run_format_core(
        transcript,
        template_content,
        ollama_url,
        model,
        match_style,
        custom_instructions,
        &cache_dir,
        on_progress,
        cancel,
    )
    .await
}


fn build_prompt(
    transcript: &str,
    template: &str,
    match_style: bool,
    custom_instructions: &str,
) -> String {
    let instructions_trimmed = custom_instructions.trim();

    // Build the numbered job steps in order. Steps with optional clauses
    // get appended conditionally so step numbers stay contiguous.
    let mut steps: Vec<String> = vec![
        "Carefully read the entire transcript.".into(),
        "Identify and extract all key information: participants, topics discussed, decisions made, action items, dates, and any other relevant details.".into(),
        "Produce a clean, well-structured document that strictly follows the layout of the FORMAT TEMPLATE below.".into(),
    ];
    if match_style {
        steps.push(
            "Match the writing style of the FORMAT TEMPLATE. Mirror its tone (formal/casual), sentence rhythm, vocabulary, and any voice quirks (first-person/third-person, verbosity, jargon level). Apply this style to all prose in your output, while still drawing factual content only from the transcript.".into(),
        );
    }
    steps.push(
        "Do not invent information not present in the transcript. If a section has no content, write \"N/A\" or omit it gracefully.".into(),
    );
    steps.push(
        "Output only the final formatted document — no preamble, no explanation.".into(),
    );
    if !instructions_trimmed.is_empty() {
        steps.push(
            "Follow the ADDITIONAL INSTRUCTIONS strictly. They override defaults where they conflict (e.g., length, voice, locale). Do not introduce facts not present in the transcript.".into(),
        );
    }
    let numbered_steps = steps
        .iter()
        .enumerate()
        .map(|(i, s)| format!("{}. {}", i + 1, s))
        .collect::<Vec<_>>()
        .join("\n");

    let instructions_block = if instructions_trimmed.is_empty() {
        String::new()
    } else {
        format!(
            "\n---\nADDITIONAL INSTRUCTIONS:\n{instructions_trimmed}\n",
        )
    };

    format!(
        r#"You are an expert document formatter. You will be given a raw meeting/recording transcript and a format template.

Your job:
{numbered_steps}

---
FORMAT TEMPLATE:
{template}
---{instructions_block}
RAW TRANSCRIPT:
{transcript}
---

Produce the formatted document now:"#,
        numbered_steps = numbered_steps,
        template = template,
        instructions_block = instructions_block,
        transcript = transcript,
    )
}

// ── File I/O ───────────────────────────────────────────────────────────────────

/// Write a capture blob to the given directory. Filename format:
/// `capture-YYYYMMDD-HHMMSS-<8-hex>.webm`. Returns the full path.
pub fn write_capture_to(
    dir: &std::path::Path,
    bytes: &[u8],
) -> Result<std::path::PathBuf, String> {
    std::fs::create_dir_all(dir)
        .map_err(|e| format!("create captures dir: {e}"))?;

    let ts = {
        let secs_since_epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        format_utc_timestamp_for_capture(secs_since_epoch)
    };
    let full = uuid::Uuid::new_v4().simple().to_string();
    let short = &full[..8];
    let filename = format!("capture-{ts}-{short}.webm");
    let path = dir.join(filename);
    std::fs::write(&path, bytes)
        .map_err(|e| format!("write capture: {e}"))?;
    Ok(path)
}

/// Return `YYYYMMDD-HHMMSS` for a given unix timestamp (UTC).
pub(crate) fn format_utc_timestamp_for_capture(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let sod = secs % 86_400;
    let hh = sod / 3_600;
    let mm = (sod % 3_600) / 60;
    let ss = sod % 60;

    // Civil calendar from days-since-epoch (Howard Hinnant).
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}{m:02}{d:02}-{hh:02}{mm:02}{ss:02}")
}

/// Save a captured audio Blob (raw bytes) into the app data dir.
/// Returns the absolute path of the written file.
#[tauri::command]
pub async fn save_capture(
    app: AppHandle,
    bytes: Vec<u8>,
) -> Result<String, String> {
    use tauri::Manager;
    let dir = app.path()
        .app_data_dir()
        .map_err(|e| format!("app_data_dir: {e}"))?
        .join("captures");
    let path = write_capture_to(&dir, &bytes)?;
    Ok(path.to_string_lossy().to_string())
}

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
            custom_instructions: String::new(),
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

// ── Pipeline cancellation ──────────────────────────────────────────────────

/// Cancel the in-flight pipeline run for a recording, if any. Flips the
/// per-recording cancel token so the format step's stream loop bails on
/// next poll.
#[tauri::command]
pub async fn cancel_recording(
    registry: tauri::State<'_, std::sync::Arc<crate::cancellation::RecordingCancelRegistry>>,
    recording_id: String,
) -> Result<(), String> {
    if let Some(token) = registry.get(&recording_id) {
        token.cancel();
        Ok(())
    } else {
        Err(format!("no in-flight run for recording {recording_id}"))
    }
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

// ── Call capture commands ─────────────────────────────────────────────────

use crate::capture::{self, CallCaptureRegistry, StartCallCaptureResult};

#[tauri::command]
pub async fn start_call_capture(
    app: AppHandle,
    registry: tauri::State<'_, Arc<CallCaptureRegistry>>,
    include_mic: bool,
) -> Result<StartCallCaptureResult, String> {
    use tauri::Manager;
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app_data_dir: {e}"))?;
    let registry = (*registry).clone();
    let app_for_session = app.clone();
    tokio::task::spawn_blocking(move || {
        capture::begin_session(&registry, &app_data_dir, include_mic, app_for_session)
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn discard_call_capture(
    registry: tauri::State<'_, Arc<CallCaptureRegistry>>,
    session_id: String,
) -> Result<(), String> {
    let registry = (*registry).clone();
    tokio::task::spawn_blocking(move || capture::discard_session(&registry, &session_id))
        .await
        .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn pause_call_capture(
    registry: tauri::State<'_, Arc<CallCaptureRegistry>>,
    session_id: String,
) -> Result<(), String> {
    let registry = (*registry).clone();
    tokio::task::spawn_blocking(move || capture::set_paused(&registry, &session_id, true))
        .await
        .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn resume_call_capture(
    registry: tauri::State<'_, Arc<CallCaptureRegistry>>,
    session_id: String,
) -> Result<(), String> {
    let registry = (*registry).clone();
    tokio::task::spawn_blocking(move || capture::set_paused(&registry, &session_id, false))
        .await
        .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn stop_call_capture(
    app: AppHandle,
    registry: tauri::State<'_, Arc<CallCaptureRegistry>>,
    session_id: String,
) -> Result<String, String> {
    use tauri::Manager;
    let captures_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app_data_dir: {e}"))?
        .join("captures");
    let registry = (*registry).clone();
    tokio::task::spawn_blocking(move || {
        capture::finalize_session(&registry, &session_id, &captures_dir)
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use crate::test_support::MockOllama;

    #[test]
    fn preprocess_strips_standalone_fillers() {
        let input = "Um, so I think, uh, we should go. Ah, okay.";
        let out = preprocess(input);
        assert_eq!(out, "so I think, we should go. okay.");
    }

    #[test]
    fn preprocess_preserves_substrings_that_look_like_fillers() {
        let input = "Uber driver brought an umbrella. Also: Ahmad came.";
        let out = preprocess(input);
        assert!(out.contains("Uber"));
        assert!(out.contains("umbrella"));
        assert!(out.contains("Ahmad"));
    }

    #[test]
    fn preprocess_collapses_3plus_stutters() {
        assert_eq!(preprocess("I I I went"), "I went");
        assert_eq!(preprocess("the the the the end"), "the end");
    }

    #[test]
    fn preprocess_keeps_2x_repeats_as_emphasis() {
        assert_eq!(preprocess("very very good"), "very very good");
    }

    #[test]
    fn preprocess_normalizes_whitespace_preserving_paragraph_breaks() {
        let input = "line one\nstill one.\n\n\nnew para.";
        let out = preprocess(input);
        assert_eq!(out, "line one still one.\n\nnew para.");
    }

    #[test]
    fn preprocess_normalizes_dashes_and_ellipsis() {
        assert_eq!(preprocess("first — second … third"), "first - second ... third");
    }

    #[test]
    fn preprocess_is_idempotent() {
        let input = "Um, so — I I I said, uhm, maybe…\n\n\nThat's it.";
        let once = preprocess(input);
        let twice = preprocess(&once);
        assert_eq!(once, twice);
    }

    #[test]
    fn split_short_input_returns_single_chunk() {
        let input = "Short transcript. Not much here.";
        let chunks = split_into_chunks(input);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], input);
    }

    #[test]
    fn split_long_input_produces_multiple_chunks() {
        // ~24k chars ≈ 6000 tokens → ≥ 2 chunks given CHUNK_TARGET=3000.
        let sentence = "This is a sentence with some length to it. ";
        let input: String = sentence.repeat(550); // ~24k chars
        let chunks = split_into_chunks(&input);
        assert!(chunks.len() >= 2, "expected ≥2 chunks, got {}", chunks.len());
        // Each chunk should be within target+search-window+overlap slack.
        for (i, c) in chunks.iter().enumerate() {
            assert!(
                c.len() <= (CHUNK_TARGET + CHUNK_OVERLAP + 500) * 4 + 50,
                "chunk {i} too large: {} chars", c.len()
            );
        }
    }

    #[test]
    fn split_prefers_sentence_boundaries() {
        // Build input where the target split lands mid-sentence, with a
        // clean '. ' a few hundred chars before the target.
        let head = "A".repeat(3000 * 4 - 300); // well before target (chars, not tokens)
        let tail = ". Then more content here to fill space. ".repeat(200);
        let input = format!("{head}. Clean boundary here. {tail}");
        let chunks = split_into_chunks(&input);
        assert!(chunks.len() >= 2);
        // Chunk 0 should end with sentence-ending punctuation + space (or be trimmed to end with '.').
        let c0 = chunks[0].trim_end();
        assert!(
            c0.ends_with('.') || c0.ends_with('?') || c0.ends_with('!'),
            "chunk 0 ends with: {:?}",
            &c0[c0.len().saturating_sub(40)..],
        );
    }

    #[test]
    fn split_wall_of_text_falls_back_to_whitespace() {
        // No sentence punctuation at all; split should still find a space.
        let word = "word ";
        let input = word.repeat(4000); // ~20k chars, no punctuation
        let chunks = split_into_chunks(&input);
        assert!(chunks.len() >= 2);
        // No chunk should end mid-word.
        for (i, c) in chunks.iter().enumerate() {
            let trimmed = c.trim_end();
            assert!(trimmed.ends_with("word"), "chunk {i} ends mid-word: {:?}", &trimmed[trimmed.len().saturating_sub(20)..]);
        }
    }

    #[test]
    fn split_overlap_prefix_present_on_later_chunks() {
        let sentence = "Sentence number X is here. ";
        let input: String = (0..1200).map(|i| sentence.replace('X', &i.to_string())).collect();
        let chunks = split_into_chunks(&input);
        assert!(chunks.len() >= 2);
        // Chunk 1 should start with some content that appears near the end of chunk 0.
        let tail0_end = &chunks[0][chunks[0].len().saturating_sub(CHUNK_OVERLAP * 4)..];
        // Pick a distinctive word near the end of chunk 0 and check it appears at the start of chunk 1.
        let words: Vec<&str> = tail0_end.split_whitespace().collect();
        if let Some(marker) = words.iter().rev().find(|w| w.len() > 3).copied() {
            let head1 = &chunks[1][..chunks[1].len().min(CHUNK_OVERLAP * 4 + 100)];
            assert!(head1.contains(marker), "chunk 1 head missing overlap marker {marker:?}");
        } else {
            panic!("test setup should produce a distinctive marker word");
        }
    }

    #[test]
    fn split_handles_multibyte_chars_without_panic() {
        // A long input whose char positions around CHUNK_TARGET fall on multi-byte
        // codepoints (em-dash, smart quote, accented letters). Must not panic.
        let chunk = "Café — \u{201C}résumé\u{201D} naïve déjà vu. ";
        let input: String = chunk.repeat(1000); // > THRESHOLD in chars
        let chunks = split_into_chunks(&input);
        assert!(chunks.len() >= 2);
        // Reconstruct lossily and confirm no char was cut in half.
        for c in chunks.iter() {
            assert!(c.is_char_boundary(0) && c.is_char_boundary(c.len()));
            assert!(std::str::from_utf8(c.as_bytes()).is_ok());
        }
    }

    #[test]
    fn cache_key_is_stable_for_same_inputs() {
        let a = cache_key("transcript", "template", "gemma3", false, "");
        let b = cache_key("transcript", "template", "gemma3", false, "");
        assert_eq!(a, b);
        assert_eq!(a.len(), 64, "sha256 hex should be 64 chars");
    }

    #[test]
    fn cache_key_differs_on_any_input_change() {
        let base = cache_key("t", "tpl", "m", false, "");
        assert_ne!(base, cache_key("t2", "tpl", "m", false, ""));
        assert_ne!(base, cache_key("t", "tpl2", "m", false, ""));
        assert_ne!(base, cache_key("t", "tpl", "m2", false, ""));
        assert_ne!(base, cache_key("t", "tpl", "m", true, ""));
        assert_ne!(base, cache_key("t", "tpl", "m", false, "extra"));
    }

    #[test]
    fn cache_read_missing_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let out = cache_read(tmp.path(), "nope");
        assert!(out.is_none());
    }

    #[test]
    fn cache_write_then_read_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        cache_write(tmp.path(), "abc123", "hello world").unwrap();
        let got = cache_read(tmp.path(), "abc123").unwrap();
        assert_eq!(got, "hello world");
    }

    fn noop_progress() -> Arc<dyn Fn(FormatProgress) + Send + Sync> {
        Arc::new(|_p| {})
    }

    fn collect_progress() -> (Arc<Mutex<Vec<FormatProgress>>>, Arc<dyn Fn(FormatProgress) + Send + Sync>) {
        let v: Arc<Mutex<Vec<FormatProgress>>> = Arc::new(Mutex::new(Vec::new()));
        let v2 = v.clone();
        let cb: Arc<dyn Fn(FormatProgress) + Send + Sync> = Arc::new(move |p| {
            v2.lock().unwrap().push(p);
        });
        (v, cb)
    }

    #[tokio::test]
    async fn single_call_path_makes_one_upstream_call() {
        let mock = MockOllama::spawn(|_body| "FORMATTED\n".to_string()).await;
        let tmp = tempfile::tempdir().unwrap();

        let out = run_format_core(
            "short transcript",
            "# Template",
            &mock.base_url,
            "gemma3",
            false,
            "",
            tmp.path(),
            noop_progress(),
            None,
        )
        .await
        .unwrap();

        assert!(out.trim().contains("FORMATTED"));
        let calls = mock.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0]["keep_alive"], "30m");
        assert_eq!(calls[0]["options"]["num_ctx"], 8192);
        assert_eq!(calls[0]["options"]["num_predict"], -1);
    }

    #[tokio::test]
    async fn map_reduce_path_issues_N_plus_1_calls() {
        // Real chunks (~12k chars) trigger the conservative concurrency=1
        // branch of the heuristic. The map/reduce *shape* is what we assert
        // here — concurrency tuning is an internal detail covered separately.
        let sentence = "This is a long sentence that will get split eventually. ";
        let transcript: String = sentence.repeat(500);

        let mock = MockOllama::spawn(|body| {
            let prompt = body["prompt"].as_str().unwrap_or("");
            if prompt.contains("You are an expert note-taker") {
                "NOTES for this segment. ".to_string()
            } else {
                "REDUCED OUTPUT. ".to_string()
            }
        })
        .await;

        let tmp = tempfile::tempdir().unwrap();
        let out = run_format_core(
            &transcript,
            "# Template",
            &mock.base_url,
            "gemma3",
            false,
            "",
            tmp.path(),
            noop_progress(),
            None,
        )
        .await
        .unwrap();

        assert!(out.contains("REDUCED OUTPUT"));
        let calls = mock.calls.lock().unwrap();
        assert!(calls.len() >= 3, "expected N+1 (N≥2 map + 1 reduce) calls, got {}", calls.len());
        let last_prompt = calls.last().unwrap()["prompt"].as_str().unwrap();
        assert!(last_prompt.contains("expert document formatter"), "last call was not reduce");

        let map_call = calls.iter().find(|c| c["prompt"].as_str().unwrap().contains("note-taker")).unwrap();
        assert_eq!(map_call["options"]["num_ctx"], 6144);
        let reduce_call = calls.iter().rev().find(|c| c["prompt"].as_str().unwrap().contains("expert document formatter")).unwrap();
        assert_eq!(reduce_call["options"]["num_ctx"], 12288);
    }

    #[tokio::test]
    async fn cache_hit_short_circuits_second_call() {
        let mock = MockOllama::spawn(|_b| "FORMATTED".to_string()).await;
        let tmp = tempfile::tempdir().unwrap();

        run_format_core("t", "tpl", &mock.base_url, "gemma3", false, "", tmp.path(), noop_progress(), None).await.unwrap();
        run_format_core("t", "tpl", &mock.base_url, "gemma3", false, "", tmp.path(), noop_progress(), None).await.unwrap();

        let calls = mock.calls.lock().unwrap();
        assert_eq!(calls.len(), 1, "second run should have hit the cache");
    }

    #[tokio::test]
    async fn cache_miss_on_model_change() {
        let mock = MockOllama::spawn(|_b| "FORMATTED".to_string()).await;
        let tmp = tempfile::tempdir().unwrap();

        run_format_core("t", "tpl", &mock.base_url, "modelA", false, "", tmp.path(), noop_progress(), None).await.unwrap();
        run_format_core("t", "tpl", &mock.base_url, "modelB", false, "", tmp.path(), noop_progress(), None).await.unwrap();

        let calls = mock.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
    }

    #[tokio::test]
    async fn progress_events_order_map_then_reduce_then_done() {
        let sentence = "This is a long sentence that will get split eventually. ";
        let transcript: String = sentence.repeat(500);

        let mock = MockOllama::spawn(|b| {
            if b["prompt"].as_str().unwrap().contains("note-taker") {
                "NOTES. ".to_string()
            } else {
                "REDUCED. ".to_string()
            }
        })
        .await;

        let tmp = tempfile::tempdir().unwrap();
        let (events, cb) = collect_progress();

        run_format_core(&transcript, "# Template", &mock.base_url, "gemma3", false, "", tmp.path(), cb, None)
            .await
            .unwrap();

        let events = events.lock().unwrap().clone();
        assert!(!events.is_empty());

        let phases: Vec<FormatPhase> = events.iter().map(|e| e.phase).collect();
        let first_reduce_idx = phases.iter().position(|p| *p == FormatPhase::Reduce).expect("no reduce phase");
        assert!(phases[..first_reduce_idx].iter().all(|p| *p == FormatPhase::Map));

        let mut last_completed = 0;
        for e in events.iter().take(first_reduce_idx) {
            let c = e.chunk.as_ref().expect("map event missing chunk info");
            assert!(c.completed >= last_completed);
            last_completed = c.completed;
        }
        let last_map = &events[first_reduce_idx - 1];
        let c = last_map.chunk.as_ref().unwrap();
        assert_eq!(c.completed, c.total);

        let last = events.last().unwrap();
        assert!(last.done);
        assert_eq!(last.phase, FormatPhase::Reduce);
    }

    #[test]
    fn build_prompt_omits_style_clause_when_flag_off() {
        let p = build_prompt("transcript", "tpl", false, "");
        assert!(!p.contains("Match the writing style"), "should not include style clause");
    }

    #[test]
    fn build_prompt_includes_style_clause_when_flag_on() {
        let p = build_prompt("transcript", "tpl", true, "");
        assert!(
            p.contains("Match the writing style of the FORMAT TEMPLATE"),
            "missing style clause:\n{p}"
        );
    }

    #[test]
    fn build_prompt_omits_instructions_section_when_empty() {
        let p = build_prompt("transcript", "tpl", false, "");
        assert!(!p.contains("ADDITIONAL INSTRUCTIONS"), "should not include instructions block");
    }

    #[test]
    fn build_prompt_strips_whitespace_only_instructions() {
        let p = build_prompt("transcript", "tpl", false, "   \n  \t");
        assert!(!p.contains("ADDITIONAL INSTRUCTIONS"), "whitespace-only must be treated as empty");
    }

    #[test]
    fn build_prompt_includes_instructions_section_when_set() {
        let p = build_prompt("transcript", "tpl", false, "Use British English.");
        assert!(p.contains("ADDITIONAL INSTRUCTIONS:"), "missing instructions header");
        assert!(p.contains("Use British English."), "missing instructions body");
        assert!(
            p.contains("Follow the ADDITIONAL INSTRUCTIONS strictly"),
            "missing instructions step"
        );
    }

    #[test]
    fn build_prompt_base_uses_layout_only_phrasing() {
        // The base prompt should say "follow the layout" (singular), not "layout and style".
        let p = build_prompt("t", "tpl", false, "");
        assert!(p.contains("strictly follows the layout of the FORMAT TEMPLATE"), "base phrasing");
        assert!(
            !p.contains("layout and style of the FORMAT TEMPLATE"),
            "old conflated phrasing must be removed"
        );
    }

    #[tokio::test]
    async fn ollama_stream_collects_tokens_and_calls_back() {
        let mock = MockOllama::spawn(|_| "hello world.".to_string()).await;
        let collected: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let cb = {
            let collected = collected.clone();
            move |tok: &str| collected.lock().unwrap().push(tok.to_string())
        };
        let out = ollama_stream(
            &mock.base_url, "gemma3", "PROMPT".into(), 8192,
            &mut { cb }, None,
        )
        .await
        .unwrap();
        assert_eq!(out.trim(), "hello world.");
        let calls = mock.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0]["prompt"], "PROMPT");
        let toks = collected.lock().unwrap();
        assert!(toks.len() >= 2, "expected multiple token callbacks, got {}", toks.len());
        let reconstructed: String = toks.iter().map(|s| s.as_str()).collect();
        assert_eq!(reconstructed, "hello world.");
    }
}

#[cfg(test)]
mod build_ffmpeg_args_tests {
    use super::build_ffmpeg_args;
    use std::path::PathBuf;

    #[test]
    fn single_input_uses_simple_vn_shape() {
        let out = PathBuf::from("/tmp/out.wav");
        let args = build_ffmpeg_args("/a.mp4", &out);
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
    fn input_path_with_spaces_is_passed_as_single_arg() {
        let out = PathBuf::from("/tmp/out.wav");
        let args = build_ffmpeg_args("/Users/n/Movies/2026-04-16 20-06-30.mov", &out);
        let i_idx = args.iter().position(|a| a == "-i").unwrap();
        assert_eq!(args[i_idx + 1], "/Users/n/Movies/2026-04-16 20-06-30.mov");
    }
}

#[cfg(test)]
mod write_capture_tests {
    use super::write_capture_to;
    use tempfile::tempdir;

    #[test]
    fn writes_bytes_and_returns_webm_path_under_dir() {
        let dir = tempdir().unwrap();
        let bytes = b"opus-bytes-here".to_vec();
        let path = write_capture_to(dir.path(), &bytes).unwrap();

        assert!(path.starts_with(dir.path()));
        assert_eq!(path.extension().and_then(|s| s.to_str()), Some("webm"));

        let on_disk = std::fs::read(&path).unwrap();
        assert_eq!(on_disk, bytes);

        let name = path.file_name().and_then(|s| s.to_str()).unwrap();
        assert!(name.starts_with("capture-"), "name should start with 'capture-': {name}");
    }

    #[test]
    fn returns_unique_paths_on_repeated_calls() {
        let dir = tempdir().unwrap();
        let p1 = write_capture_to(dir.path(), b"a").unwrap();
        let p2 = write_capture_to(dir.path(), b"b").unwrap();
        assert_ne!(p1, p2, "same dir + separate calls should produce different paths");
    }
}
