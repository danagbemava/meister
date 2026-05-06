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

pub use cleanup::cleanup_orphaned_call_temps;
