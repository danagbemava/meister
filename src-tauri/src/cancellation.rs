//! Per-recording cancellation tokens for in-flight pipeline runs.

use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

#[derive(Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Default)]
pub struct RecordingCancelRegistry {
    inner: Mutex<HashMap<String, CancelToken>>,
}

impl RecordingCancelRegistry {
    /// Get-or-create a fresh token for a new run. If a previous token
    /// exists for this recording, it is replaced with a fresh one (so the
    /// new run starts un-cancelled).
    pub fn token_for_new_run(&self, recording_id: &str) -> CancelToken {
        let mut g = self.inner.lock().unwrap();
        let token = CancelToken::default();
        g.insert(recording_id.to_string(), token.clone());
        token
    }

    /// Look up the token for an in-progress run, if any.
    pub fn get(&self, recording_id: &str) -> Option<CancelToken> {
        let g = self.inner.lock().unwrap();
        g.get(recording_id).cloned()
    }

    /// Remove the token for a finished run.
    pub fn finish(&self, recording_id: &str) {
        let mut g = self.inner.lock().unwrap();
        g.remove(recording_id);
    }
}
