//! Call capture: records system audio (via ScreenCaptureKit) and optionally
//! the default microphone (via cpal), writing two PCM WAV tracks to a
//! per-session temp directory. `finalize_session` runs ffmpeg to mix + encode
//! the final WebM/Opus that the UI drops into staging.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

pub mod amplitude;
pub mod cleanup;
pub mod mic_audio;
pub mod mix;
pub mod power;
pub mod system_audio;

pub struct CallCaptureSession {
    pub id: String,
    pub session_dir: PathBuf,
    pub system_wav: PathBuf,
    pub mic_wav: Option<PathBuf>,
    /// SCStream-native microphone track, captured in parallel with `mic_wav`
    /// (cpal) while we validate that the OS mic tap survives VoIP calls.
    pub sck_mic_wav: Option<PathBuf>,
    pub paused: Arc<AtomicBool>,
    /// Keep-alive handles — dropping these tears the capture down.
    /// Boxed `dyn Any + Send` so the registry is agnostic to the exact
    /// stream-handle types from screencapturekit and cpal.
    pub system_handle: Box<dyn std::any::Any + Send>,
    pub mic_handle: Option<Box<dyn std::any::Any + Send>>,
    pub sample_rate: u32,
    pub channels: u16,
    /// Kept alive for the lifetime of the session; dropped here stops the
    /// 30 fps drain task. Audio callbacks each hold their own `Arc` clone.
    pub _meters: [Arc<amplitude::AmplitudeEmitter>; 2],
    /// Keeps the display awake while recording — ScreenCaptureKit capture is
    /// display-bound and macOS kills the stream when the display sleeps.
    /// `None` when the assertion couldn't be taken (recording still proceeds).
    pub _display_assertion: Option<power::DisplaySleepAssertion>,
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
    app: tauri::AppHandle,
) -> Result<StartCallCaptureResult, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let dir = session_temp_dir(app_data_dir, &id);
    std::fs::create_dir_all(&dir).map_err(|e| format!("create session dir: {e}"))?;

    let system_wav = dir.join("system.wav");
    let mic_wav = dir.join("mic.wav");
    let sck_mic_wav = dir.join("sck_mic.wav");

    let paused = Arc::new(AtomicBool::new(false));

    let meter = |source: &'static str| {
        let session_id = id.clone();
        let app = app.clone();
        Arc::new(amplitude::AmplitudeEmitter::spawn(move |rms| {
            use tauri::Emitter;
            let _ = app.emit(
                "call-capture-amplitude",
                serde_json::json!({ "sessionId": session_id, "source": source, "rms": rms }),
            );
        }))
    };
    let system_meter = meter("system");
    let mic_meter = meter("mic");

    // The ScreenCaptureKit mic tap shares the system-audio stream and clock and
    // keeps working when a call app changes the input device. cpal still runs
    // as the fallback for macOS 14 and for a tap that delivers nothing.
    let use_sck_mic = include_mic && supports_sck_mic();
    let sck_mic_arg = use_sck_mic.then_some(sck_mic_wav.as_path());

    // Best-effort: without the assertion, display sleep can kill the SCStream mid-call.
    let display_assertion = match power::DisplaySleepAssertion::create("Meister is recording a call")
    {
        Ok(a) => Some(a),
        Err(e) => {
            eprintln!(
                "[capture] display-sleep assertion failed ({e}); \
                 recording continues but will stop if the display sleeps"
            );
            None
        }
    };

    // Surface an unexpected SCStream death (display sleep, lock, replayd
    // restart) during the call: an in-app event for the recording card, and a
    // system notification because Meister is usually not frontmost.
    let app_for_stop = app.clone();
    let id_for_stop = id.clone();
    let on_stream_stopped: Box<dyn Fn(Option<String>) + Send + Sync> = Box::new(move |error| {
        use tauri::Emitter;
        use tauri_plugin_notification::NotificationExt;
        let _ = app_for_stop.emit(
            "call-capture-stream-stopped",
            serde_json::json!({ "sessionId": id_for_stop, "error": error }),
        );
        if let Err(e) = app_for_stop
            .notification()
            .builder()
            .title("Recording interrupted")
            .body("System audio capture stopped — audio from this point on is not being recorded. Stop and save now.")
            .show()
        {
            eprintln!("[capture] stream-stopped notification failed: {e}");
        }
    });

    // System audio first — if permission isn't granted, we bail.
    let system_handle = match system_audio::start_system_capture(&system_wav, sck_mic_arg, paused.clone(), system_meter.clone(), mic_meter.clone(), on_stream_stopped) {
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
        match mic_audio::start_mic_capture(&mic_wav, paused.clone(), mic_meter.clone()) {
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
        sck_mic_wav: use_sck_mic.then_some(sck_mic_wav),
        paused,
        system_handle,
        mic_handle,
        sample_rate: system_audio::SAMPLE_RATE,
        channels: system_audio::CHANNELS,
        _meters: [system_meter, mic_meter],
        _display_assertion: display_assertion,
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

    let keep_temp = std::env::var("MEISTER_KEEP_CAPTURE_TEMP").as_deref() == Ok("1");
    if keep_temp {
        diag_wav("system", &session.system_wav);
        for (label, path) in [("mic (cpal)", &session.mic_wav), ("mic (sck)", &session.sck_mic_wav)] {
            if let Some(p) = path {
                diag_wav(label, p);
            }
        }
    }

    let mic = session
        .sck_mic_wav
        .as_deref()
        .filter(|p| wav_frames(p) >= u64::from(system_audio::SAMPLE_RATE))
        .or(session.mic_wav.as_deref());

    // Build output path using the same naming convention as mic-capture.
    std::fs::create_dir_all(captures_dir).map_err(|e| format!("create captures dir: {e}"))?;
    let ts_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let ts = crate::commands::format_utc_timestamp_for_capture(ts_secs);
    let short = &uuid::Uuid::new_v4().simple().to_string()[..8];
    let out_path = captures_dir.join(format!("capture-{ts}-{short}.webm"));

    mix::encode_final(&session.system_wav, mic, &out_path)?;

    // MEISTER_KEEP_CAPTURE_TEMP=1 keeps the per-source WAVs for inspection.
    if keep_temp {
        eprintln!(
            "[capture/finalize] MEISTER_KEEP_CAPTURE_TEMP=1 — preserving source WAVs at {}",
            session.session_dir.display()
        );
    } else {
        let _ = std::fs::remove_dir_all(&session.session_dir);
    }

    Ok(out_path.to_string_lossy().to_string())
}

fn wav_frames(path: &std::path::Path) -> u64 {
    hound::WavReader::open(path).map_or(0, |r| u64::from(r.duration()))
}

/// The ScreenCaptureKit microphone output exists from macOS 15.
fn supports_sck_mic() -> bool {
    std::process::Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|v| v.trim().split('.').next()?.parse::<u32>().ok())
        .is_some_and(|major| major >= 15)
}

/// Read a finalized capture WAV and log its declared format plus measured
/// duration, peak, and RMS (in dBFS). Used at finalize to spot a silent or
/// missing source track. Best-effort: any read error is logged, never fatal.
fn diag_wav(label: &str, path: &std::path::Path) {
    let mut reader = match hound::WavReader::open(path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[capture/finalize] {label}: cannot open {} ({e})", path.display());
            return;
        }
    };
    let spec = reader.spec();
    let mut peak: i32 = 0;
    let mut sum_sq: f64 = 0.0;
    let mut count: u64 = 0;
    for s in reader.samples::<i32>().flatten() {
        let a = s.unsigned_abs() as i32;
        if a > peak {
            peak = a;
        }
        sum_sq += (s as f64) * (s as f64);
        count += 1;
    }
    let full = f64::from(i16::MAX);
    let frames = if spec.channels > 0 { count / u64::from(spec.channels) } else { 0 };
    let secs = if spec.sample_rate > 0 { frames as f64 / f64::from(spec.sample_rate) } else { 0.0 };
    let rms = if count > 0 { (sum_sq / count as f64).sqrt() } else { 0.0 };
    let db = |v: f64| if v <= 0.0 { f64::NEG_INFINITY } else { 20.0 * (v / full).log10() };
    eprintln!(
        "[capture/finalize] {label}: {} Hz × {} ch, {:.1}s, {} samples | peak {:.1} dBFS, rms {:.1} dBFS{}",
        spec.sample_rate,
        spec.channels,
        secs,
        count,
        db(f64::from(peak)),
        db(rms),
        if peak == 0 { "  <-- SILENT" } else { "" },
    );
}

pub use cleanup::cleanup_orphaned_call_temps;
