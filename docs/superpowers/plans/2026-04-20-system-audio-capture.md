# System Audio Capture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Record system audio (and optionally mic) on macOS via ScreenCaptureKit + cpal; mix into a single WebM/Opus that drops into ProjectView's staging area.

**Architecture:** A new `capture` Rust module owns SCStream + cpal sessions, writing separate PCM WAVs to a per-session temp dir. Stop triggers ffmpeg to mix and encode to WebM/Opus. Five Tauri commands (`start/pause/resume/stop/discard_call_capture`) drive the lifecycle. A new `RecordCallCard` React component mirrors `RecordCard` with a third slot in ProjectView's Sources row and a "Permission needed" state for Screen Recording grants.

**Tech Stack:** Rust (`screencapturekit` crate for SCStream, `cpal` for mic input, existing `hound` for WAV I/O, existing ffmpeg for mix+encode). React 18 + TypeScript for UI. Tauri 2. macOS-only.

**Spec:** `docs/superpowers/specs/2026-04-20-system-audio-capture-design.md`

---

## File structure

### Rust — new files
- `src-tauri/src/capture/mod.rs` — module root; `CallCaptureRegistry`, `CallCaptureSession`, session-dir helpers, public API.
- `src-tauri/src/capture/system_audio.rs` — SCStream setup + callback that writes PCM to `system.wav`.
- `src-tauri/src/capture/mic_audio.rs` — cpal setup + callback that writes PCM to `mic.wav`.
- `src-tauri/src/capture/mix.rs` — Stop handler: `finalize_session` runs ffmpeg to produce the final WebM/Opus.
- `src-tauri/src/capture/cleanup.rs` — `cleanup_orphaned_call_temps` for startup sweep.

### Rust — modified
- `src-tauri/Cargo.toml` — add `screencapturekit`, `cpal` dependencies.
- `src-tauri/src/commands.rs` — add 5 Tauri commands (`start/pause/resume/stop/discard_call_capture`).
- `src-tauri/src/lib.rs` — register `CallCaptureRegistry` in state, add 5 commands to `invoke_handler!`, call orphan cleanup in `setup`.
- `src-tauri/Info.plist` — add `NSScreenCaptureUsageDescription`.

### Frontend — new
- `src/components/RecordCallCard.tsx` — new component with state machine + media controls + Permission Needed card.

### Frontend — modified
- `src/api/storage.ts` — add wrappers for the 5 call-capture commands.
- `src/components/ProjectView.tsx` — widen sources row grid to 3 columns; render `RecordCallCard`.

---

# Phase 1 — Dependencies

## Task 1: Add `screencapturekit` and `cpal` dependencies

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Verify current latest compatible versions on crates.io**

Before editing, open https://crates.io/crates/screencapturekit and https://crates.io/crates/cpal in a browser (or `cargo search screencapturekit` / `cargo search cpal`) to confirm the current latest versions. At time of writing the plan, `screencapturekit 0.3.x` and `cpal 0.15.x` are current.

- [ ] **Step 2: Add to `src-tauri/Cargo.toml`**

Under `[dependencies]`, append:

```toml
screencapturekit = "0.3"
cpal = "0.15"
```

If Step 1 revealed newer majors (e.g. `0.4` or `1.0`), use those — record the exact version picked in the commit message. API deltas across versions may require small adaptations later; note any here as a concern.

- [ ] **Step 3: Verify compile**

Run: `cd src-tauri && cargo check`
Expected: clean (some fresh warnings from the newly-imported-but-unused crates are fine; they'll be used in Task 2+).

Note: the first build of `screencapturekit` may take a while — it has Apple-framework linkage. That's normal.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "deps: add screencapturekit + cpal for system audio capture"
```

---

# Phase 2 — Capture modules

## Task 2: `capture` module scaffold + session types

**Files:**
- Create: `src-tauri/src/capture/mod.rs`
- Modify: `src-tauri/src/lib.rs` (register module)

- [ ] **Step 1: Create `src-tauri/src/capture/mod.rs`**

```rust
//! Call capture: records system audio (via ScreenCaptureKit) and optionally
//! the default microphone (via cpal), writing two PCM WAV tracks to a
//! per-session temp directory. `finalize_session` runs ffmpeg to mix + encode
//! the final WebM/Opus that the UI drops into staging.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

pub mod cleanup;
pub mod mic_audio;
pub mod mix;
pub mod system_audio;

pub struct CallCaptureSession {
    pub id: String,
    pub session_dir: PathBuf,
    pub system_wav: PathBuf,
    pub mic_wav: Option<PathBuf>,
    pub paused: Arc<AtomicBool>,
    /// Keep-alive handles — dropping these tears the capture down.
    /// Boxed `dyn Any + Send` so the registry is agnostic to the exact
    /// stream-handle types from screencapturekit and cpal.
    pub system_handle: Box<dyn std::any::Any + Send>,
    pub mic_handle: Option<Box<dyn std::any::Any + Send>>,
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Default)]
pub struct CallCaptureRegistry {
    pub sessions: Mutex<HashMap<String, CallCaptureSession>>,
}

impl CallCaptureRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn active_ids(&self) -> Vec<String> {
        self.sessions.lock().unwrap().keys().cloned().collect()
    }
}

/// Build the temp directory for a new session: `<app_data>/tmp/call-captures/<id>/`.
/// Caller creates the dir via `std::fs::create_dir_all`.
pub fn session_temp_dir(app_data_dir: &std::path::Path, session_id: &str) -> PathBuf {
    app_data_dir.join("tmp").join("call-captures").join(session_id)
}

/// Full path of `<app_data>/tmp/call-captures/` — used by the orphan sweep.
pub fn all_sessions_temp_dir(app_data_dir: &std::path::Path) -> PathBuf {
    app_data_dir.join("tmp").join("call-captures")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_starts_empty() {
        let r = CallCaptureRegistry::new();
        assert_eq!(r.active_ids().len(), 0);
    }

    #[test]
    fn session_temp_dir_nests_under_tmp() {
        let base = std::path::PathBuf::from("/fake/data");
        let p = session_temp_dir(&base, "abc");
        assert_eq!(p, std::path::PathBuf::from("/fake/data/tmp/call-captures/abc"));
    }
}
```

- [ ] **Step 2: Create placeholder child modules**

To keep the scaffold compiling before the real implementations land in later tasks, create these three files with a single-line comment each:

`src-tauri/src/capture/system_audio.rs`:
```rust
//! SCStream wiring. Implemented in Task 3.
```

`src-tauri/src/capture/mic_audio.rs`:
```rust
//! cpal mic wiring. Implemented in Task 4.
```

`src-tauri/src/capture/mix.rs`:
```rust
//! ffmpeg mix + encode. Implemented in Task 6.
```

`src-tauri/src/capture/cleanup.rs`:
```rust
//! Orphan temp cleanup. Implemented in Task 7.
```

- [ ] **Step 3: Register the module in `src-tauri/src/lib.rs`**

Near the top of `src-tauri/src/lib.rs` (below `pub mod storage;` and `pub mod orchestrator;`), add:

```rust
pub mod capture;
```

- [ ] **Step 4: Compile + run the two new unit tests**

Run: `cd src-tauri && cargo test --lib capture::tests`
Expected: `registry_starts_empty` and `session_temp_dir_nests_under_tmp` PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/capture/ src-tauri/src/lib.rs
git commit -m "capture: scaffold module with registry + session types"
```

---

## Task 3: System audio capture — `capture::system_audio`

**Files:**
- Modify: `src-tauri/src/capture/system_audio.rs`

This task wires SCStream to write PCM frames into a WAV file. Because the exact `screencapturekit` crate API surface varies across versions, the code below is the target shape — if the crate version you landed in Task 1 exposes different names (e.g. `SCStreamConfiguration` vs `SCStreamOutputType::Audio`), adapt to the actual API while keeping the public function signature and the WavWriter write loop identical. Commit a short concern if the adaptation was non-trivial.

- [ ] **Step 1: Write the API shape + a unit test**

Replace `src-tauri/src/capture/system_audio.rs` with:

```rust
//! ScreenCaptureKit-backed system-audio capture.
//!
//! `start_system_capture(wav_path, paused)` returns a boxed handle that owns
//! the SCStream. Dropping the handle stops the stream. While `paused` is
//! `true`, incoming audio frames are dropped instead of written.

use hound::{WavSpec, WavWriter};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: u16 = 2;
pub const BITS: u16 = 16;

pub fn wav_spec() -> WavSpec {
    WavSpec {
        channels: CHANNELS,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: BITS,
        sample_format: hound::SampleFormat::Int,
    }
}

/// Try to start an SCStream capturing all system audio, writing interleaved
/// i16 PCM into `wav_path`. Returns an opaque handle whose drop stops the
/// stream. `paused` is shared with the session: when true, frames are dropped.
///
/// Errors:
/// - `"not-authorized"` — Screen Recording permission not granted (caller
///   should surface the Permission Needed UI).
/// - Anything else — propagated from the screencapturekit crate.
pub fn start_system_capture(
    wav_path: &Path,
    paused: Arc<AtomicBool>,
) -> Result<Box<dyn std::any::Any + Send>, String> {
    let writer = WavWriter::create(wav_path, wav_spec())
        .map_err(|e| format!("create system wav: {e}"))?;
    let writer = Arc::new(Mutex::new(Some(writer)));

    // === Construct the SCStream. ===
    //
    // Expected behavior:
    // - Audio-only stream (discard video frames).
    // - Captures ALL system output (not app-filtered).
    // - Audio format: 48 kHz stereo f32 interleaved (SCK default on modern
    //   macOS). Our writer expects i16 — we convert below.
    //
    // Adapt to the actual `screencapturekit` API. A plausible version-0.3
    // outline:
    //
    //   use screencapturekit::{
    //       cm_sample_buffer::CMSampleBuffer,
    //       sc_content_filter::{InitParams, SCContentFilter},
    //       sc_shareable_content::SCShareableContent,
    //       sc_stream::SCStream,
    //       sc_stream_configuration::SCStreamConfiguration,
    //       sc_output_handler::{SCStreamOutputType, StreamOutput},
    //   };
    //
    //   let content = SCShareableContent::current()
    //       .map_err(|e| format!("shareable content: {e}"))?;
    //   let display = content.displays.into_iter().next()
    //       .ok_or_else(|| "no display".to_string())?;
    //   let filter = SCContentFilter::new(InitParams::Display(display));
    //
    //   let mut cfg = SCStreamConfiguration::default();
    //   cfg.captures_audio = true;
    //   cfg.sample_rate = 48_000;
    //   cfg.channel_count = 2;
    //
    //   let mut stream = SCStream::new(&filter, &cfg, AuthError::authorized()?);
    //   stream.add_output(handler, SCStreamOutputType::Audio);
    //   stream.start_capture().map_err(|e| {
    //       if is_auth_error(&e) { "not-authorized".into() } else { format!("{e}") }
    //   })?;
    //
    // The handler is an impl of `StreamOutput` that, for each CMSampleBuffer,
    // extracts interleaved f32 samples and writes them as i16 via the code
    // below.
    //
    // === Audio frame handler (the important invariant) ===
    //
    //   fn on_audio(&self, buf: CMSampleBuffer) {
    //       if self.paused.load(Ordering::Relaxed) { return; }
    //       let Some(samples) = extract_interleaved_f32(&buf) else { return; };
    //       let mut guard = self.writer.lock().unwrap();
    //       let Some(writer) = guard.as_mut() else { return; };
    //       for s in samples {
    //           let clamped = s.clamp(-1.0, 1.0);
    //           let i = (clamped * i16::MAX as f32) as i16;
    //           let _ = writer.write_sample(i);
    //       }
    //   }
    //
    // Return a handle whose Drop closes the stream and finalizes the writer.
    //
    // === End of outline. ===

    let _ = (wav_path, &paused, &writer);  // silence unused-variable warnings while stub
    todo!("implement against the screencapturekit crate API actually present in Cargo.lock")
}

/// Detect whether an SCK start-capture error indicates the app lacks Screen
/// Recording permission. Crate-API-specific; adapt as needed.
fn _is_auth_error(_e: &dyn std::error::Error) -> bool {
    // Implemented as part of Step 2 below.
    false
}
```

- [ ] **Step 2: Implement against the actual crate API**

Open `Cargo.lock` (or run `cargo doc --open -p screencapturekit`) and find the exact types, then replace the `todo!()` with a working implementation that:

1. Enumerates a display via `SCShareableContent`.
2. Builds an `SCContentFilter` that captures everything from that display.
3. Configures the `SCStream` with `captures_audio = true` (or the crate's equivalent), `sample_rate = 48_000`, `channel_count = 2`.
4. Registers a `StreamOutput` handler for audio that:
   - Returns immediately if `paused.load(Ordering::Relaxed)` is true.
   - Extracts interleaved f32 samples from the `CMSampleBuffer`.
   - Acquires the writer mutex and writes converted i16 samples.
5. Starts the stream. On start error that looks like an authorization failure, map to `Err("not-authorized".into())`.
6. Returns a handle struct that wraps the `SCStream` + `Arc<Mutex<Option<WavWriter>>>` and whose `Drop` stops the stream and takes+finalizes the writer.

Boxed as `Box<dyn std::any::Any + Send>` so the registry stays generic.

If the crate API fundamentally blocks audio capture (e.g. forces a full screen video config), stop and report BLOCKED — we'd need a different crate version or a custom wrapper.

- [ ] **Step 3: Unit test (no live audio — shape-only)**

Because this calls into macOS and requires permissions, we can't unit-test it in CI. Instead, append a compilation smoke test below the public function in `system_audio.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_spec_is_48k_stereo_s16() {
        let s = wav_spec();
        assert_eq!(s.sample_rate, 48_000);
        assert_eq!(s.channels, 2);
        assert_eq!(s.bits_per_sample, 16);
        assert_eq!(s.sample_format, hound::SampleFormat::Int);
    }
}
```

- [ ] **Step 4: Compile + run the test**

Run: `cd src-tauri && cargo test --lib capture::system_audio::tests`
Expected: PASS.

Run: `cd src-tauri && cargo check`
Expected: clean (the actual SCStream code compiles — the `todo!()` is gone).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/capture/system_audio.rs
git commit -m "capture/system_audio: SCStream-backed system audio capture"
```

---

## Task 4: Mic capture — `capture::mic_audio`

**Files:**
- Modify: `src-tauri/src/capture/mic_audio.rs`

- [ ] **Step 1: Write the module**

Replace `src-tauri/src/capture/mic_audio.rs` with:

```rust
//! cpal-backed mic capture. Writes interleaved i16 PCM to a WAV file at the
//! same sample rate and channel count as `system_audio`. Resamples/channel-
//! fixes on the fly if the mic's native format differs.

use super::system_audio::{CHANNELS, SAMPLE_RATE, wav_spec};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use hound::WavWriter;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub fn start_mic_capture(
    wav_path: &Path,
    paused: Arc<AtomicBool>,
) -> Result<Box<dyn std::any::Any + Send>, String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "no default input device".to_string())?;
    let supported = device
        .default_input_config()
        .map_err(|e| format!("default input config: {e}"))?;

    let dev_channels = supported.channels();
    let dev_rate = supported.sample_rate().0;
    let dev_format = supported.sample_format();
    let config: StreamConfig = supported.into();

    let writer = WavWriter::create(wav_path, wav_spec())
        .map_err(|e| format!("create mic wav: {e}"))?;
    let writer = Arc::new(Mutex::new(Some(writer)));

    let paused_cb = paused.clone();
    let writer_cb = writer.clone();

    let err_fn = |err| eprintln!("mic cpal stream error: {err}");

    let stream = match dev_format {
        SampleFormat::F32 => device.build_input_stream(
            &config,
            move |data: &[f32], _| {
                if paused_cb.load(Ordering::Relaxed) { return; }
                let mut guard = writer_cb.lock().unwrap();
                let Some(w) = guard.as_mut() else { return; };
                write_resampled_f32(w, data, dev_channels, dev_rate);
            },
            err_fn,
            None,
        ),
        SampleFormat::I16 => device.build_input_stream(
            &config,
            move |data: &[i16], _| {
                if paused_cb.load(Ordering::Relaxed) { return; }
                let mut guard = writer_cb.lock().unwrap();
                let Some(w) = guard.as_mut() else { return; };
                write_resampled_i16(w, data, dev_channels, dev_rate);
            },
            err_fn,
            None,
        ),
        SampleFormat::U16 => device.build_input_stream(
            &config,
            move |data: &[u16], _| {
                if paused_cb.load(Ordering::Relaxed) { return; }
                let mut guard = writer_cb.lock().unwrap();
                let Some(w) = guard.as_mut() else { return; };
                let i16s: Vec<i16> = data.iter().map(|&u| (u as i32 - 32_768) as i16).collect();
                write_resampled_i16(w, &i16s, dev_channels, dev_rate);
            },
            err_fn,
            None,
        ),
        other => return Err(format!("unsupported mic sample format: {other:?}")),
    }
    .map_err(|e| format!("build mic input stream: {e}"))?;

    stream.play().map_err(|e| format!("start mic stream: {e}"))?;

    struct Handle {
        _stream: cpal::Stream,
        writer: Arc<Mutex<Option<WavWriter<std::io::BufWriter<std::fs::File>>>>>,
    }
    impl Drop for Handle {
        fn drop(&mut self) {
            let mut guard = self.writer.lock().unwrap();
            if let Some(w) = guard.take() {
                let _ = w.finalize();
            }
            // _stream drops here, stopping the cpal input.
        }
    }
    Ok(Box::new(Handle { _stream: stream, writer }))
}

/// Write i16 data to the WAV after resampling (linear) + channel fix.
fn write_resampled_i16<W: std::io::Write + std::io::Seek>(
    writer: &mut WavWriter<W>,
    data: &[i16],
    src_channels: u16,
    src_rate: u32,
) {
    let f32s: Vec<f32> = data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
    write_resampled_f32(writer, &f32s, src_channels, src_rate);
}

/// Write f32 data to the WAV after resampling (linear) to 48 kHz + forcing to
/// `CHANNELS` channels (2). For a typical 48 kHz stereo mic, this is a no-op.
fn write_resampled_f32<W: std::io::Write + std::io::Seek>(
    writer: &mut WavWriter<W>,
    data: &[f32],
    src_channels: u16,
    src_rate: u32,
) {
    let mono: Vec<f32> = if src_channels == 1 {
        data.to_vec()
    } else {
        // Downmix first src_channels into mono.
        data.chunks_exact(src_channels as usize)
            .map(|frame| frame.iter().sum::<f32>() / src_channels as f32)
            .collect()
    };
    let resampled: Vec<f32> = if src_rate == SAMPLE_RATE {
        mono
    } else {
        linear_resample(&mono, src_rate, SAMPLE_RATE)
    };
    for s in resampled {
        let clamped = s.clamp(-1.0, 1.0);
        let i = (clamped * i16::MAX as f32) as i16;
        // Duplicate mono mic into both channels of the 48k stereo WAV.
        for _ in 0..CHANNELS {
            let _ = writer.write_sample(i);
        }
    }
}

/// Cheap linear-interpolation resampler. Good enough for speech (Whisper is
/// robust to small quality degradation); we are not shipping a DAW.
fn linear_resample(src: &[f32], src_rate: u32, dst_rate: u32) -> Vec<f32> {
    if src.is_empty() { return Vec::new(); }
    let ratio = dst_rate as f64 / src_rate as f64;
    let dst_len = (src.len() as f64 * ratio).round() as usize;
    let mut out = Vec::with_capacity(dst_len);
    for i in 0..dst_len {
        let src_idx_f = i as f64 / ratio;
        let idx0 = src_idx_f.floor() as usize;
        let idx1 = (idx0 + 1).min(src.len() - 1);
        let frac = (src_idx_f - idx0 as f64) as f32;
        let s = src[idx0] * (1.0 - frac) + src[idx1] * frac;
        out.push(s);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_resample_double_rate_doubles_length() {
        let src: Vec<f32> = (0..100).map(|i| (i as f32) / 100.0).collect();
        let out = linear_resample(&src, 24_000, 48_000);
        assert_eq!(out.len(), 200);
    }

    #[test]
    fn linear_resample_passthrough_at_same_rate() {
        let src: Vec<f32> = vec![0.1, 0.2, 0.3];
        let out = linear_resample(&src, 48_000, 48_000);
        assert_eq!(out, src);
    }
}
```

- [ ] **Step 2: Compile + run tests**

Run: `cd src-tauri && cargo test --lib capture::mic_audio::tests`
Expected: both tests PASS.

Run: `cd src-tauri && cargo check`
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/capture/mic_audio.rs
git commit -m "capture/mic_audio: cpal-backed mic capture with linear resample"
```

---

# Phase 3 — Session + commands

## Task 5: Session lifecycle helpers + `start_call_capture` / `discard_call_capture`

**Files:**
- Modify: `src-tauri/src/capture/mod.rs` (add session start helper)
- Modify: `src-tauri/src/commands.rs` (add 2 Tauri commands)
- Modify: `src-tauri/src/lib.rs` (register registry + commands)

- [ ] **Step 1: Append session-start helper to `src-tauri/src/capture/mod.rs`**

Append below the existing content of `src-tauri/src/capture/mod.rs`:

```rust
use std::sync::atomic::Ordering;

#[derive(serde::Serialize, Clone)]
pub struct StartCallCaptureResult {
    pub session_id: String,
    pub system_ok: bool,
    pub mic_ok: bool,
    pub mic_error: Option<String>,
}

/// Create a new capture session. Writes to `<app_data>/tmp/call-captures/<id>/`.
/// Adds the session to the registry on success. On `not-authorized` system
/// start, returns a result with `system_ok = false` and does NOT add the
/// session (the UI will surface Permission Needed).
pub fn begin_session(
    registry: &CallCaptureRegistry,
    app_data_dir: &std::path::Path,
    include_mic: bool,
) -> Result<StartCallCaptureResult, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let dir = session_temp_dir(app_data_dir, &id);
    std::fs::create_dir_all(&dir).map_err(|e| format!("create session dir: {e}"))?;

    let system_wav = dir.join("system.wav");
    let mic_wav = dir.join("mic.wav");

    let paused = Arc::new(AtomicBool::new(false));

    // System audio first — if permission isn't granted, we bail.
    let system_handle = match system_audio::start_system_capture(&system_wav, paused.clone()) {
        Ok(h) => h,
        Err(e) if e == "not-authorized" => {
            let _ = std::fs::remove_dir_all(&dir);
            return Ok(StartCallCaptureResult {
                session_id: id,
                system_ok: false,
                mic_ok: false,
                mic_error: None,
            });
        }
        Err(e) => {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(format!("start system capture: {e}"));
        }
    };

    // Mic is best-effort when requested.
    let (mic_handle, mic_path, mic_ok, mic_error) = if include_mic {
        match mic_audio::start_mic_capture(&mic_wav, paused.clone()) {
            Ok(h) => (Some(h), Some(mic_wav.clone()), true, None),
            Err(e) => (None, None, false, Some(e)),
        }
    } else {
        (None, None, true, None)
    };

    let session = CallCaptureSession {
        id: id.clone(),
        session_dir: dir,
        system_wav,
        mic_wav: mic_path,
        paused,
        system_handle,
        mic_handle,
        sample_rate: system_audio::SAMPLE_RATE,
        channels: system_audio::CHANNELS,
    };

    registry.sessions.lock().unwrap().insert(id.clone(), session);

    Ok(StartCallCaptureResult {
        session_id: id,
        system_ok: true,
        mic_ok,
        mic_error,
    })
}

/// Drop a session without producing a final file. Deletes the temp dir.
pub fn discard_session(registry: &CallCaptureRegistry, id: &str) -> Result<(), String> {
    let session = registry.sessions.lock().unwrap().remove(id);
    if let Some(s) = session {
        // Dropping the handles stops the streams and finalizes writers first.
        drop(s.system_handle);
        if let Some(h) = s.mic_handle { drop(h); }
        let _ = std::fs::remove_dir_all(&s.session_dir);
    }
    Ok(())
}

pub fn set_paused(registry: &CallCaptureRegistry, id: &str, paused: bool) -> Result<(), String> {
    let sessions = registry.sessions.lock().unwrap();
    let s = sessions.get(id).ok_or_else(|| format!("unknown session: {id}"))?;
    s.paused.store(paused, Ordering::Relaxed);
    Ok(())
}
```

- [ ] **Step 2: Add Tauri commands in `src-tauri/src/commands.rs`**

Append to the end of `src-tauri/src/commands.rs`:

```rust
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
    tokio::task::spawn_blocking(move || {
        capture::begin_session(&registry, &app_data_dir, include_mic)
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
```

(`stop_call_capture` lands in Task 6 after the mix module exists.)

- [ ] **Step 3: Register in `src-tauri/src/lib.rs`**

In `src-tauri/src/lib.rs`:

1. Register the registry in setup, next to `app.manage(Arc::new(store))`:
   ```rust
   app.manage(std::sync::Arc::new(capture::CallCaptureRegistry::new()));
   ```

2. Add to `invoke_handler![…]`:
   ```rust
   commands::start_call_capture,
   commands::pause_call_capture,
   commands::resume_call_capture,
   commands::discard_call_capture,
   ```

- [ ] **Step 4: Compile**

Run: `cd src-tauri && cargo check`
Expected: clean.

- [ ] **Step 5: Run existing test suite**

Run: `cd src-tauri && cargo test`
Expected: all tests still pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/capture/mod.rs src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "capture: session start/discard/pause/resume + Tauri commands"
```

---

## Task 6: Mix + encode on Stop — `capture::mix` + `stop_call_capture`

**Files:**
- Modify: `src-tauri/src/capture/mix.rs`
- Modify: `src-tauri/src/capture/mod.rs` (add `finalize_session` helper)
- Modify: `src-tauri/src/commands.rs` (add `stop_call_capture`)
- Modify: `src-tauri/src/lib.rs` (register command)

- [ ] **Step 1: Write `capture::mix`**

Replace `src-tauri/src/capture/mix.rs` with:

```rust
//! ffmpeg-based mix + encode. Produces the final WebM/Opus at a caller-chosen
//! output path. When only system audio was captured, re-encodes to opus; when
//! both are present, runs amix.

use std::path::Path;
use std::process::Command;

pub fn encode_final(
    system_wav: &Path,
    mic_wav: Option<&Path>,
    output_webm: &Path,
) -> Result<(), String> {
    if let Some(parent) = output_webm.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create output dir: {e}"))?;
    }
    let args: Vec<String> = if let Some(mic) = mic_wav {
        vec![
            "-i".into(), system_wav.to_string_lossy().into_owned(),
            "-i".into(), mic.to_string_lossy().into_owned(),
            "-filter_complex".into(),
            "[0:a][1:a]amix=inputs=2:duration=longest:dropout_transition=0,aresample=48000[out]".into(),
            "-map".into(), "[out]".into(),
            "-c:a".into(), "libopus".into(),
            "-b:a".into(), "96k".into(),
            "-y".into(),
            output_webm.to_string_lossy().into_owned(),
        ]
    } else {
        vec![
            "-i".into(), system_wav.to_string_lossy().into_owned(),
            "-c:a".into(), "libopus".into(),
            "-b:a".into(), "96k".into(),
            "-y".into(),
            output_webm.to_string_lossy().into_owned(),
        ]
    };

    let output = Command::new("ffmpeg")
        .args(&args)
        .output()
        .map_err(|e| format!("ffmpeg not found or failed to start: {e}. Install via: brew install ffmpeg"))?;
    if !output.status.success() {
        return Err(format!("ffmpeg error:\n{}", String::from_utf8_lossy(&output.stderr)));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hound::{SampleFormat, WavSpec, WavWriter};
    use tempfile::tempdir;

    fn write_sine(path: &Path, secs: u32, freq: f32) {
        let spec = WavSpec {
            channels: 2,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        let mut w = WavWriter::create(path, spec).unwrap();
        for i in 0..48_000 * secs {
            let t = i as f32 / 48_000.0;
            let s = (2.0 * std::f32::consts::PI * freq * t).sin();
            let v = (s * 16_000.0) as i16;
            w.write_sample(v).unwrap();
            w.write_sample(v).unwrap();
        }
        w.finalize().unwrap();
    }

    #[test]
    fn encode_single_source_produces_nonempty_webm() {
        let dir = tempdir().unwrap();
        let sys = dir.path().join("system.wav");
        write_sine(&sys, 1, 440.0);
        let out = dir.path().join("out.webm");
        encode_final(&sys, None, &out).unwrap();
        let meta = std::fs::metadata(&out).unwrap();
        assert!(meta.len() > 0, "output webm should be non-empty");
    }

    #[test]
    fn encode_two_sources_amix_produces_nonempty_webm() {
        let dir = tempdir().unwrap();
        let sys = dir.path().join("system.wav");
        let mic = dir.path().join("mic.wav");
        write_sine(&sys, 1, 440.0);
        write_sine(&mic, 1, 880.0);
        let out = dir.path().join("out.webm");
        encode_final(&sys, Some(&mic), &out).unwrap();
        let meta = std::fs::metadata(&out).unwrap();
        assert!(meta.len() > 0);
    }
}
```

- [ ] **Step 2: Append `finalize_session` to `src-tauri/src/capture/mod.rs`**

Append:

```rust
/// Stop a session: drop handles to flush writers, encode via ffmpeg, delete
/// temp dir, return the final output path as a String.
pub fn finalize_session(
    registry: &CallCaptureRegistry,
    session_id: &str,
    captures_dir: &std::path::Path,
) -> Result<String, String> {
    let session = registry
        .sessions
        .lock()
        .unwrap()
        .remove(session_id)
        .ok_or_else(|| format!("unknown session: {session_id}"))?;

    // Dropping handles finalizes the WAV writers.
    drop(session.system_handle);
    if let Some(h) = session.mic_handle { drop(h); }

    // Build output path using the same naming convention as mic-capture.
    std::fs::create_dir_all(captures_dir).map_err(|e| format!("create captures dir: {e}"))?;
    let ts_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let ts = crate::commands::format_utc_timestamp_for_capture(ts_secs);
    let short = &uuid::Uuid::new_v4().simple().to_string()[..8];
    let out_path = captures_dir.join(format!("capture-{ts}-{short}.webm"));

    mix::encode_final(&session.system_wav, session.mic_wav.as_deref(), &out_path)?;

    let _ = std::fs::remove_dir_all(&session.session_dir);

    Ok(out_path.to_string_lossy().to_string())
}
```

Note the call to `crate::commands::format_utc_timestamp_for_capture` — this exposes the existing private `format_utc_timestamp` from the mic-capture feature. In Step 3 we make it accessible.

- [ ] **Step 3: Expose the timestamp helper in `commands.rs`**

In `src-tauri/src/commands.rs`, locate the existing private `fn format_utc_timestamp` (added by the mic-capture feature). Rename it (or re-export) so the capture module can call it:

Change its definition from:
```rust
fn format_utc_timestamp(secs: u64) -> String {
```
to:
```rust
pub(crate) fn format_utc_timestamp_for_capture(secs: u64) -> String {
```

And update its caller inside `write_capture_to` accordingly:
```rust
// inside write_capture_to:
format_utc_timestamp_for_capture(secs_since_epoch)
```

- [ ] **Step 4: Add `stop_call_capture` in `commands.rs`**

Append to the Call capture commands block:

```rust
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
```

- [ ] **Step 5: Register `stop_call_capture` in `lib.rs` `invoke_handler![…]`**

Add:
```rust
commands::stop_call_capture,
```

- [ ] **Step 6: Compile + run tests**

Run: `cd src-tauri && cargo check`
Expected: clean.

Run: `cd src-tauri && cargo test --lib capture::mix::tests`
Expected: both mix tests PASS (they require ffmpeg on PATH; ffmpeg is already an app dep).

Run: `cd src-tauri && cargo test`
Expected: full suite passes.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/capture/mix.rs \
        src-tauri/src/capture/mod.rs \
        src-tauri/src/commands.rs \
        src-tauri/src/lib.rs
git commit -m "capture: stop+mix+encode to final webm; add stop_call_capture"
```

---

## Task 7: Orphan temp cleanup — `capture::cleanup`

**Files:**
- Modify: `src-tauri/src/capture/cleanup.rs`
- Modify: `src-tauri/src/lib.rs` (call in setup)

- [ ] **Step 1: Write the cleanup module + unit tests**

Replace `src-tauri/src/capture/cleanup.rs` with:

```rust
//! Orphan call-capture temp directories.
//!
//! Call at app startup, BEFORE creating any new sessions. Removes every
//! directory under `<app_data>/tmp/call-captures/` whose name is not in
//! `active_ids`. At boot time `active_ids` is empty, so every directory
//! gets removed — any temp state that survived a crash is garbage.

use std::collections::HashSet;
use std::path::Path;

pub fn cleanup_orphaned_call_temps(
    captures_tmp_dir: &Path,
    active_ids: &HashSet<String>,
) -> Result<usize, String> {
    let entries = match std::fs::read_dir(captures_tmp_dir) {
        Ok(e) => e,
        Err(_) => return Ok(0),
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = match path.file_name().and_then(|s| s.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if !active_ids.contains(&name) {
            if std::fs::remove_dir_all(&path).is_ok() {
                removed += 1;
            }
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn deletes_inactive_session_dirs_keeps_active() {
        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join("keep-me")).unwrap();
        std::fs::create_dir(dir.path().join("drop-1")).unwrap();
        std::fs::create_dir(dir.path().join("drop-2")).unwrap();

        let mut active = HashSet::new();
        active.insert("keep-me".to_string());

        let n = cleanup_orphaned_call_temps(dir.path(), &active).unwrap();
        assert_eq!(n, 2);
        assert!(dir.path().join("keep-me").exists());
        assert!(!dir.path().join("drop-1").exists());
        assert!(!dir.path().join("drop-2").exists());
    }

    #[test]
    fn handles_missing_dir_gracefully() {
        let bogus = std::path::PathBuf::from("/definitely/not/a/real/xyz");
        let active = HashSet::new();
        let n = cleanup_orphaned_call_temps(&bogus, &active).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn ignores_non_directory_entries() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("not-a-dir.txt"), b"x").unwrap();
        let active = HashSet::new();
        let n = cleanup_orphaned_call_temps(dir.path(), &active).unwrap();
        assert_eq!(n, 0);
        assert!(dir.path().join("not-a-dir.txt").exists());
    }
}
```

- [ ] **Step 2: Re-export from `capture::mod`**

At the bottom of `src-tauri/src/capture/mod.rs`, add:

```rust
pub use cleanup::cleanup_orphaned_call_temps;
```

- [ ] **Step 3: Wire into `lib.rs::setup`**

In `src-tauri/src/lib.rs::setup`, after the existing `cleanup_orphaned_captures` call (from the mic-capture feature), add:

```rust
            let call_tmp = capture::all_sessions_temp_dir(&app_data_dir);
            match capture::cleanup_orphaned_call_temps(&call_tmp, &std::collections::HashSet::new()) {
                Ok(n) if n > 0 => eprintln!("capture: removed {n} orphaned call-capture dir(s)"),
                Ok(_) => {}
                Err(e) => eprintln!("capture: call temp cleanup failed (non-fatal): {e}"),
            }
```

- [ ] **Step 4: Compile + run tests**

Run: `cd src-tauri && cargo test --lib capture::cleanup::tests`
Expected: all 3 PASS.

Run: `cd src-tauri && cargo test`
Expected: full suite passes.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/capture/cleanup.rs \
        src-tauri/src/capture/mod.rs \
        src-tauri/src/lib.rs
git commit -m "capture: orphan temp cleanup + wire into app setup"
```

---

# Phase 4 — Permissions

## Task 8: `NSScreenCaptureUsageDescription` in `Info.plist`

**Files:**
- Modify: `src-tauri/Info.plist`

- [ ] **Step 1: Add the key**

In `src-tauri/Info.plist`, add a new key/string pair alongside the existing `NSMicrophoneUsageDescription`:

```xml
	<key>NSScreenCaptureUsageDescription</key>
	<string>Meister captures system audio to record calls and meetings for transcription.</string>
```

The final file should have both keys in the top-level `<dict>`.

- [ ] **Step 2: Compile check**

Run: `cd src-tauri && cargo check`
Expected: clean (Tauri validates the Info.plist reference at build).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/Info.plist
git commit -m "bundle: add NSScreenCaptureUsageDescription for system audio"
```

---

# Phase 5 — Frontend

## Task 9: TS API wrappers

**Files:**
- Modify: `src/api/storage.ts`

- [ ] **Step 1: Append call-capture wrappers to `src/api/storage.ts`**

At the bottom of the `api` object in `src/api/storage.ts`, before the closing `};`, add:

```ts
  startCallCapture: (includeMic: boolean) =>
    invoke<{
      sessionId: string;
      systemOk: boolean;
      micOk: boolean;
      micError?: string | null;
    }>("start_call_capture", { includeMic }),

  pauseCallCapture: (sessionId: string) =>
    invoke<void>("pause_call_capture", { sessionId }),

  resumeCallCapture: (sessionId: string) =>
    invoke<void>("resume_call_capture", { sessionId }),

  stopCallCapture: (sessionId: string) =>
    invoke<string>("stop_call_capture", { sessionId }),

  discardCallCapture: (sessionId: string) =>
    invoke<void>("discard_call_capture", { sessionId }),
```

Rust-side `StartCallCaptureResult` uses `#[serde(rename_all = ...)]`? Let's double-check: it does NOT, so `session_id`, `system_ok`, `mic_ok`, `mic_error` are the JSON keys. Adjust the wrapper to decode those names:

Replace the first wrapper with:

```ts
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
```

- [ ] **Step 2: Build**

Run: `cd /Users/nexus/projects/meister/.worktrees/system-audio && npm run build`
Expected: clean — nothing calls these yet.

- [ ] **Step 3: Commit**

```bash
git add src/api/storage.ts
git commit -m "frontend/api: add call-capture command wrappers"
```

---

## Task 10: `RecordCallCard` component

**Files:**
- Create: `src/components/RecordCallCard.tsx`

- [ ] **Step 1: Write the component**

Create `src/components/RecordCallCard.tsx`:

```tsx
import { useEffect, useRef, useState } from "react";
import { api } from "../api/storage";

type CaptureState =
  | "idle"
  | "permission-needed"
  | "recording"
  | "paused"
  | "saving"
  | "error";

interface Props {
  onPicked: (filePaths: string[]) => void;
  onActiveChange?: (discard: (() => void) | null) => void;
}

function formatElapsed(ms: number): string {
  const total = Math.floor(ms / 1000);
  const hh = Math.floor(total / 3600);
  const mm = Math.floor((total % 3600) / 60);
  const ss = total % 60;
  const pad = (n: number) => n.toString().padStart(2, "0");
  return hh > 0 ? `${pad(hh)}:${pad(mm)}:${pad(ss)}` : `${pad(mm)}:${pad(ss)}`;
}

export default function RecordCallCard({ onPicked, onActiveChange }: Props) {
  const [state, setState] = useState<CaptureState>("idle");
  const [errorMsg, setErrorMsg] = useState<string>("");
  const [micError, setMicError] = useState<string>("");
  const [includeMic, setIncludeMic] = useState(true);
  const [elapsedMs, setElapsedMs] = useState(0);

  const sessionRef = useRef<string | null>(null);
  const startedAtRef = useRef<number>(0);
  const accumulatedRef = useRef<number>(0);
  const tickerRef = useRef<number | null>(null);

  const stopTicker = () => {
    if (tickerRef.current !== null) {
      window.clearInterval(tickerRef.current);
      tickerRef.current = null;
    }
  };

  const startTicker = () => {
    stopTicker();
    tickerRef.current = window.setInterval(() => {
      setElapsedMs(accumulatedRef.current + (Date.now() - startedAtRef.current));
    }, 200);
  };

  useEffect(() => {
    return () => { stopTicker(); };
  }, []);

  const discard = async () => {
    const id = sessionRef.current;
    stopTicker();
    sessionRef.current = null;
    onActiveChange?.(null);
    if (id) {
      try { await api.discardCallCapture(id); } catch { /* best-effort */ }
    }
    setState("idle");
    setElapsedMs(0);
    accumulatedRef.current = 0;
  };

  const start = async () => {
    setErrorMsg("");
    setMicError("");
    setElapsedMs(0);
    accumulatedRef.current = 0;
    try {
      const result = await api.startCallCapture(includeMic);
      if (!result.systemOk) {
        setState("permission-needed");
        return;
      }
      if (includeMic && !result.micOk) {
        setMicError(result.micError ?? "Microphone unavailable — recording system audio only.");
      }
      sessionRef.current = result.sessionId;
      startedAtRef.current = Date.now();
      startTicker();
      setState("recording");
      onActiveChange?.(() => { void discard(); });
    } catch (e) {
      setState("error");
      setErrorMsg(String(e));
    }
  };

  const pause = async () => {
    const id = sessionRef.current;
    if (!id) return;
    try {
      await api.pauseCallCapture(id);
      stopTicker();
      accumulatedRef.current += Date.now() - startedAtRef.current;
      setState("paused");
    } catch (e) {
      setErrorMsg(String(e));
      setState("error");
    }
  };

  const resume = async () => {
    const id = sessionRef.current;
    if (!id) return;
    try {
      await api.resumeCallCapture(id);
      startedAtRef.current = Date.now();
      startTicker();
      setState("recording");
    } catch (e) {
      setErrorMsg(String(e));
      setState("error");
    }
  };

  const stop = async () => {
    const id = sessionRef.current;
    if (!id) return;
    stopTicker();
    setState("saving");
    try {
      const path = await api.stopCallCapture(id);
      sessionRef.current = null;
      onActiveChange?.(null);
      setElapsedMs(0);
      accumulatedRef.current = 0;
      setState("idle");
      onPicked([path]);
    } catch (e) {
      setErrorMsg(String(e));
      setState("error");
    }
  };

  const openSettings = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-shell");
      await open(
        "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
      );
    } catch { /* ignore */ }
  };

  const retry = () => {
    setState("idle");
    setErrorMsg("");
    setMicError("");
  };

  const cardBase =
    "rounded-xl border-2 flex flex-col items-center justify-center gap-2 py-10 px-4 transition-colors";

  if (state === "idle") {
    return (
      <div
        className={`${cardBase} border-dashed border-white/10 hover:border-white/20 bg-surface-secondary cursor-pointer`}
        onClick={start}
      >
        <span className="text-3xl opacity-30">🔊</span>
        <p className="text-sm text-white/40">Record call</p>
        <label
          className="flex items-center gap-2 text-xs text-white/50 cursor-pointer select-none"
          onClick={(e) => e.stopPropagation()}
        >
          <input
            type="checkbox"
            checked={includeMic}
            onChange={(e) => setIncludeMic(e.target.checked)}
          />
          Include microphone
        </label>
      </div>
    );
  }

  if (state === "permission-needed") {
    return (
      <div className={`${cardBase} border-solid border-amber-500/40 bg-amber-500/10`}>
        <span className="text-3xl opacity-60">🔒</span>
        <p className="text-sm text-amber-300 text-center px-2">
          Screen Recording permission required.
        </p>
        <p className="text-xs text-white/40 text-center px-2">
          Grant permission in System Settings, then relaunch Meister.
        </p>
        <div className="flex gap-2 mt-2">
          <button onClick={openSettings} className="px-3 py-1 rounded bg-accent text-surface text-sm font-medium">
            Open Settings
          </button>
          <button onClick={retry} className="px-3 py-1 rounded bg-surface hover:bg-white/10 text-sm">
            Retry
          </button>
        </div>
      </div>
    );
  }

  if (state === "recording" || state === "paused") {
    const dotColor = state === "recording" ? "bg-red-500 animate-pulse" : "bg-amber-400";
    const badge = includeMic ? "System + Mic" : "System only";
    return (
      <div className={`${cardBase} border-solid border-accent/40 bg-accent/10`}>
        <div className="flex items-center gap-2">
          <span className={`w-2.5 h-2.5 rounded-full ${dotColor}`} />
          <span className="text-2xl font-mono tabular-nums">{formatElapsed(elapsedMs)}</span>
        </div>
        <p className="text-xs text-white/50">{badge}</p>
        {micError && (
          <p className="text-xs text-amber-400 text-center px-2">{micError}</p>
        )}
        <div className="flex gap-2 mt-2">
          {state === "recording" ? (
            <button onClick={pause} className="px-3 py-1 rounded bg-surface hover:bg-white/10 text-sm">⏸ Pause</button>
          ) : (
            <button onClick={resume} className="px-3 py-1 rounded bg-surface hover:bg-white/10 text-sm">▶ Resume</button>
          )}
          <button onClick={stop} className="px-3 py-1 rounded bg-accent text-surface text-sm font-medium">⏹ Stop</button>
          <button onClick={discard} className="px-3 py-1 rounded bg-surface hover:bg-red-500/20 text-sm text-white/70">✕ Discard</button>
        </div>
      </div>
    );
  }

  if (state === "saving") {
    return (
      <div className={`${cardBase} border-solid border-white/10 bg-surface-secondary`}>
        <span className="spinner" />
        <p className="text-sm text-white/50">Mixing tracks…</p>
      </div>
    );
  }

  return (
    <div className={`${cardBase} border-solid border-red-500/30 bg-red-500/10`}>
      <span className="text-3xl opacity-50">⚠</span>
      <p className="text-sm text-red-300 text-center px-2">{errorMsg}</p>
      <button onClick={retry} className="mt-2 px-3 py-1 rounded bg-surface hover:bg-white/10 text-sm">Retry</button>
    </div>
  );
}
```

- [ ] **Step 2: Build**

Run: `npm run build`
Expected: clean — nothing uses this component yet.

- [ ] **Step 3: Commit**

```bash
git add src/components/RecordCallCard.tsx
git commit -m "frontend: add RecordCallCard with permission-needed state"
```

---

## Task 11: Integrate `RecordCallCard` into ProjectView (3-column sources row)

**Files:**
- Modify: `src/components/ProjectView.tsx`

- [ ] **Step 1: Import the new component**

At the top of `src/components/ProjectView.tsx`, add alongside the existing `RecordCard` import:

```tsx
import RecordCallCard from "./RecordCallCard";
```

- [ ] **Step 2: Widen the grid and add the third card**

Find the existing two-column sources row:

```tsx
<div className="grid grid-cols-1 md:grid-cols-2 gap-3">
  <RecordingDropZone onPicked={onFilesPicked} />
  <RecordCard
    onPicked={onFilesPicked}
    onActiveChange={onCaptureActiveChange}
  />
</div>
```

Replace with:

```tsx
<div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
  <RecordingDropZone onPicked={onFilesPicked} />
  <RecordCard
    onPicked={onFilesPicked}
    onActiveChange={onCaptureActiveChange}
  />
  <RecordCallCard
    onPicked={onFilesPicked}
    onActiveChange={onCaptureActiveChange}
  />
</div>
```

- [ ] **Step 3: Build**

Run: `npm run build`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add src/components/ProjectView.tsx
git commit -m "frontend: add RecordCallCard to sources row (3 columns)"
```

---

# Phase 6 — Verification

## Task 12: End-to-end verification

**Files:** none — verification only.

- [ ] **Step 1: Rust tests**

Run: `cd src-tauri && cargo test`
Expected: all pass. New tests: `capture::tests::registry_starts_empty`, `capture::tests::session_temp_dir_nests_under_tmp`, `capture::system_audio::tests::wav_spec_is_48k_stereo_s16`, `capture::mic_audio::tests::linear_resample_*`, `capture::mix::tests::encode_*_produces_nonempty_webm` (×2), `capture::cleanup::tests::*` (×3).

- [ ] **Step 2: Frontend build**

Run: `cd /Users/nexus/projects/meister/.worktrees/system-audio && npm run build`
Expected: tsc + vite clean.

- [ ] **Step 3: Manual runtime tests**

Launch: `npm run tauri dev` (ensure port 1420 is free).

Walk through each:

1. **Screen Recording permission flow.** Fresh install OR revoke permission first. Click "Record call". Card flips to **Permission needed**. Click **Open Settings** → macOS System Settings opens on the Screen Recording pane. Add Meister (dev binary) to the list. Relaunch the app (required for TCC). Click "Record call" again — capture starts.

2. **Podcast capture (system-only).** Uncheck "Include microphone". Play a podcast in a browser. Click Record call → timer starts. Stop at ~15s. Card shows "Mixing tracks…" briefly, then returns to Idle. A `capture-YYYYMMDD-HHMMSS-xxxxxxxx.webm` row appears in staging. Click Create → Run pipeline → transcript matches podcast content.

3. **1-on-1 call (system + mic).** Use your phone as a second participant on Zoom/Meet/WhatsApp. Keep "Include microphone" checked. Record call. Both parties talk. Stop → mix runs → staging row. Create → Run → transcript contains both voices.

4. **Pause/Resume.** Start a capture, pause at ~5s, speak for a few seconds (silence should be dropped), resume, speak. Stop. The final file should contain audio from before the pause and after the resume, but not the paused interval.

5. **Discard.** Start, wait 3s, click Discard. No `capture-...webm` in `<app_data>/captures/`. Temp dir at `<app_data>/tmp/call-captures/` is empty.

6. **Mic-unavailable graceful degrade.** If possible, disconnect mic OR revoke mic permission. Start with "Include microphone" on. Capture still proceeds system-only; an amber sub-banner appears on the Recording card.

7. **Mid-capture navigation.** Start capture. Click another project in the sidebar. `confirm("Recording in progress — stop and discard?")` appears (existing guard from mic-capture). Cancel keeps capture; OK discards and navigates.

8. **Crash recovery.** Start capture. `kill -9` the app. Relaunch. `<app_data>/tmp/call-captures/` is empty (orphan sweep ran).

- [ ] **Step 4: If anything fails, STOP and report.**

Do not mark the plan complete on partial success.

---

## Notes for the implementer

- **`screencapturekit` crate API risk.** The Task 3 outline is speculative — the actual API may name things differently. Keep the public function signature in `system_audio.rs` stable (`start_system_capture(wav_path, paused) -> Result<Box<dyn Any + Send>, String>`); freely adapt the internals. If the crate cannot deliver system audio (no `captures_audio` equivalent, or audio output stream lacks a way to hook in), STOP and escalate — we'll need to write a Rust-Swift FFI layer, which is out of scope for this plan.

- **`cpal` stability.** Mic capture via cpal is well-tested territory. The code as written should work for typical macOS mics (48 kHz stereo f32); the resampler handles mismatched rates.

- **Linear resampling is intentionally cheap.** This is speech for Whisper, not music production. Don't substitute `rubato` or another high-quality resampler — it adds a large dep for no audible gain.

- **Audio/video sync is not a concern** because we only capture audio.

- **macOS TCC relaunch requirement** for Screen Recording is a platform limitation (just like Accessibility permissions). We surface it in the Permission Needed copy. Don't try to work around it.

- **ffmpeg is an existing dependency.** Don't introduce alternative encoders.

- **Session IDs are UUID strings.** They're surfaced to the frontend as-is — no need for an abstraction.

- **Dropping a session handle finalizes its writer via `Drop`.** The order matters: `drop(session.system_handle)` MUST run before `mix::encode_final` reads `system.wav`, otherwise the WAV header won't have a correct length. The `finalize_session` code handles this correctly.

- **Do not** bundle a `stop` + new capture into the same UI click. Each start is a new session.
