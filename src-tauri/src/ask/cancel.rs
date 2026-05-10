//! Per-recording cancellation tokens specifically for in-flight Ask queries.
//!
//! Kept separate from `cancellation::RecordingCancelRegistry` so an Ask cancel
//! doesn't trip a concurrent recording-pipeline run, and vice versa.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::cancellation::CancelToken;

#[derive(Default)]
pub struct AskCancelRegistry {
    inner: Mutex<HashMap<String, CancelToken>>,
}

impl AskCancelRegistry {
    pub fn token_for_new_query(&self, recording_id: &str) -> CancelToken {
        let mut g = self.inner.lock().unwrap();
        let tok = CancelToken::default();
        g.insert(recording_id.to_string(), tok.clone());
        tok
    }

    pub fn get(&self, recording_id: &str) -> Option<CancelToken> {
        self.inner.lock().unwrap().get(recording_id).cloned()
    }

    pub fn finish(&self, recording_id: &str) {
        self.inner.lock().unwrap().remove(recording_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_for_new_query_replaces_existing_token() {
        let reg = AskCancelRegistry::default();
        let t1 = reg.token_for_new_query("rec1");
        t1.cancel();
        let t2 = reg.token_for_new_query("rec1");
        assert!(!t2.is_cancelled(), "fresh token must start un-cancelled");
        assert!(t1.is_cancelled());
    }

    #[test]
    fn get_returns_none_after_finish() {
        let reg = AskCancelRegistry::default();
        let _ = reg.token_for_new_query("rec1");
        assert!(reg.get("rec1").is_some());
        reg.finish("rec1");
        assert!(reg.get("rec1").is_none());
    }

    #[test]
    fn cancel_via_registry_propagates_to_held_token() {
        let reg = AskCancelRegistry::default();
        let held = reg.token_for_new_query("rec1");
        reg.get("rec1").unwrap().cancel();
        assert!(held.is_cancelled());
    }
}
