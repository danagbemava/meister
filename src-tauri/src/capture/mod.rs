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
