use std::path::Path;

pub fn read_wav_as_f32_mono(path: &Path) -> Result<Vec<f32>, String> {
    let mut reader = hound::WavReader::open(path)
        .map_err(|e| format!("open wav {}: {e}", path.display()))?;
    let spec = reader.spec();
    if spec.channels != 1 {
        return Err(format!("expected mono WAV, got {} channels", spec.channels));
    }
    if spec.sample_rate != 16_000 {
        return Err(format!("expected 16 kHz WAV, got {} Hz", spec.sample_rate));
    }
    if spec.bits_per_sample != 16 {
        return Err(format!("expected 16-bit WAV, got {} bits", spec.bits_per_sample));
    }
    let samples: Result<Vec<i16>, _> = reader.samples::<i16>().collect();
    let samples = samples.map_err(|e| format!("read samples: {e}"))?;
    Ok(samples.into_iter().map(|s| s as f32 / 32768.0).collect())
}

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{Arc, Mutex, OnceLock};

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::transcription::models::{model_paths, ModelSize};

type ContextCache = Mutex<HashMap<String, Arc<WhisperContext>>>;
fn cache() -> &'static ContextCache {
    static C: OnceLock<ContextCache> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Returns true iff this is the first time we're loading the given size in this
/// process — useful for the UI to show the "warming" message before the call.
pub fn is_warming(size: ModelSize) -> bool {
    !cache().lock().unwrap().contains_key(size.as_str())
}

fn load_context(models_dir: &std::path::Path, size: ModelSize) -> Result<Arc<WhisperContext>, String> {
    let mut g = cache().lock().unwrap();
    if let Some(ctx) = g.get(size.as_str()) {
        return Ok(ctx.clone());
    }
    let (bin, _enc) = model_paths(models_dir, size);
    if !bin.is_file() {
        return Err(format!("model file missing: {}", bin.display()));
    }
    // Disable Metal for the decoder: whisper.cpp's encoder runs on the Apple
    // Neural Engine via CoreML (auto-discovered from the sibling `.mlmodelc`
    // directory), which is a separate accelerator from the GPU. The decoder
    // path, when `use_gpu = true`, runs on Metal — and Metal is shared with
    // WindowServer, so on Apple Silicon any heavy Metal load (whisper decoder
    // here, Ollama during format) makes the macOS compositor stutter
    // system-wide. Routing the decoder to CPU keeps the GPU free for the
    // compositor; the encoder is the expensive part and stays on ANE.
    let mut params = WhisperContextParameters::default();
    params.use_gpu = false;
    let ctx = WhisperContext::new_with_params(
        bin.to_str().ok_or("non-utf8 model path")?,
        params,
    ).map_err(|e| format!("load whisper model: {e}"))?;
    let arc = Arc::new(ctx);
    g.insert(size.as_str().to_string(), arc.clone());
    Ok(arc)
}

/// C-compatible trampoline for the progress callback. `user_data` must point to
/// a `Box<dyn FnMut(i32)>` that was previously leaked via `Box::into_raw`.
unsafe extern "C" fn progress_trampoline(
    _ctx: *mut whisper_rs::WhisperSysContext,
    _state: *mut whisper_rs::WhisperSysState,
    progress: std::os::raw::c_int,
    user_data: *mut c_void,
) {
    let cb = &mut *(user_data as *mut Box<dyn FnMut(i32)>);
    cb(progress);
}

/// Run whisper.cpp on a WAV file. Synchronous (CPU/GPU bound). Caller is
/// responsible for invoking inside `tokio::task::spawn_blocking` if used from
/// async context.
///
/// # Workaround note
/// whisper-rs 0.11.1's `set_progress_callback_safe` has a use-after-move bug
/// (the user_data pointer is set to the stack address of the closure before it
/// is moved into its heap Box). We work around this by using the lower-level
/// unsafe API: we box the closure ourselves, leak the pointer into the
/// `progress_callback_user_data` slot, and reconstruct + drop the Box after
/// `state.full()` returns.
pub fn transcribe_wav_with_callback(
    models_dir: &std::path::Path,
    size: ModelSize,
    wav_path: &std::path::Path,
    language: Option<&str>,
    progress_cb: impl Fn(i32) + Send + Sync + 'static,
) -> Result<String, String> {
    let samples = read_wav_as_f32_mono(wav_path)?;
    let samples_before = samples.len();
    let tag = wav_path.file_name().and_then(|s| s.to_str()).unwrap_or("?");

    let samples = if std::env::var("MEISTER_NO_SILENCE_GATE").as_deref() == Ok("1") {
        eprintln!(
            "[transcribe:{tag}] silence gate BYPASSED (MEISTER_NO_SILENCE_GATE=1); {samples_before} samples"
        );
        samples
    } else {
        let gated = crate::transcription::silence::gate_silence(&samples, 16_000);
        let pct = if samples_before == 0 {
            0.0
        } else {
            (gated.len() as f64 / samples_before as f64) * 100.0
        };
        eprintln!(
            "[transcribe:{tag}] silence gate: {samples_before} → {} samples ({pct:.1}% kept)",
            gated.len()
        );
        gated
    };
    if samples.is_empty() {
        eprintln!("[transcribe:{tag}] empty after gate; returning empty transcript");
        return Ok(String::new());
    }
    let ctx = load_context(models_dir, size)?;
    let mut state = ctx.create_state().map_err(|e| format!("create state: {e}"))?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    if let Some(lang) = language.filter(|l| !l.is_empty() && *l != "auto") {
        params.set_language(Some(lang));
    }
    // Prevent the classic long-form repetition loop: with no_context=false
    // (the default), whisper feeds its previous output back as the prompt for
    // the next 30-second window — so a single hallucinated `[inaudible]` can
    // perpetuate itself through the tail of the audio.
    params.set_no_context(true);
    params.set_suppress_non_speech_tokens(true);
    params.set_temperature(0.0);
    params.set_temperature_inc(0.2);
    params.set_no_speech_thold(0.6);
    params.set_logprob_thold(-1.0);
    params.set_entropy_thold(2.4);
    params.set_suppress_blank(true);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_special(false);
    params.set_print_timestamps(false);

    // Box the callback as a trait object, then box that so we have a stable
    // fat-pointer address we can hand to C as `*mut c_void`.
    let cb_box: Box<dyn FnMut(i32)> = Box::new(progress_cb);
    let cb_ptr = Box::into_raw(Box::new(cb_box)); // *mut Box<dyn FnMut(i32)>

    unsafe {
        params.set_progress_callback(Some(progress_trampoline));
        params.set_progress_callback_user_data(cb_ptr as *mut c_void);
    }

    let result = state.full(params, &samples).map_err(|e| format!("whisper full: {e}"));

    // Reconstruct and drop the Box to avoid leaking.
    // SAFETY: cb_ptr was created by Box::into_raw above and whisper.cpp will
    // not call the callback after state.full() returns.
    unsafe { drop(Box::from_raw(cb_ptr)); }

    result?;

    let n = state.full_n_segments().map_err(|e| format!("n_segments: {e}"))?;
    let mut segments: Vec<String> = Vec::with_capacity(n as usize);
    for i in 0..n {
        segments.push(
            state.full_get_segment_text(i).map_err(|e| format!("segment {i}: {e}"))?,
        );
    }

    let mut lengths: Vec<usize> = segments.iter().map(|s| s.trim().len()).collect();
    lengths.sort_unstable_by(|a, b| b.cmp(a));
    let top5: Vec<usize> = lengths.iter().take(5).copied().collect();
    eprintln!(
        "[transcribe:{tag}] raw whisper segments: {} (top-5 lengths: {:?})",
        segments.len(),
        top5
    );
    for (i, seg) in segments.iter().take(20).enumerate() {
        let trimmed = seg.trim();
        let preview: String = trimmed.chars().take(160).collect();
        let suffix = if preview.chars().count() < trimmed.chars().count() { "…" } else { "" };
        eprintln!(
            "[transcribe:{tag}]   raw[{i}] len={}: {preview:?}{suffix}",
            trimmed.len()
        );
    }

    let before_collapse = segments.len();
    let segments = crate::transcription::postprocess::collapse_repetitions(&segments);
    eprintln!(
        "[transcribe:{tag}] collapse_repetitions: {before_collapse} → {} (dropped {})",
        segments.len(),
        before_collapse - segments.len()
    );

    let before_filter = segments.len();
    let mut kept: Vec<String> = Vec::with_capacity(segments.len());
    let mut filter_drops: Vec<String> = Vec::new();
    for seg in segments {
        if crate::transcription::postprocess::is_hallucination(&seg) {
            if filter_drops.len() < 10 {
                filter_drops.push(seg);
            }
        } else {
            kept.push(seg);
        }
    }
    eprintln!(
        "[transcribe:{tag}] filter_hallucinations: {before_filter} → {} (dropped {}){}",
        kept.len(),
        before_filter - kept.len(),
        if filter_drops.is_empty() { "".to_string() } else { format!("; first drops:") }
    );
    for d in &filter_drops {
        eprintln!("[transcribe:{tag}]   filtered: {:?}", d.trim());
    }

    Ok(kept.join(" ").trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_fixture_wav() {
        let path = std::path::Path::new("tests/fixtures/hello.wav");
        let samples = read_wav_as_f32_mono(path).expect("fixture should be readable");
        assert!(!samples.is_empty(), "should produce samples");
        // 16 kHz mono, ~1–3 seconds of speech → 16k–48k samples
        assert!(samples.len() > 8_000, "too few samples: {}", samples.len());
        assert!(samples.iter().all(|s| s.abs() <= 1.0), "samples should be in [-1, 1]");
    }

    #[test]
    fn rejects_missing_file() {
        let err = read_wav_as_f32_mono(std::path::Path::new("/nonexistent.wav")).unwrap_err();
        assert!(err.contains("open wav"));
    }
}
