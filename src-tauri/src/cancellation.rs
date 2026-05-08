//! Per-recording cancellation tokens for in-flight pipeline runs.

use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tokio::sync::Notify;

#[derive(Clone, Default)]
pub struct CancelToken {
    flag: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl CancelToken {
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
        // Wake any task currently awaiting `cancelled()` so the cancel
        // signal interrupts an in-flight `await`, not just a poll between
        // awaits.
        self.notify.notify_waiters();
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    /// Resolves when this token is cancelled. Safe to call on a token that
    /// is already cancelled (returns immediately).
    pub async fn cancelled(&self) {
        loop {
            if self.is_cancelled() {
                return;
            }
            // Register interest in the next `notify_waiters()` BEFORE
            // re-checking the flag — otherwise we could race with a
            // `cancel()` that fires between the flag check and the await,
            // and miss the wake-up.
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.is_cancelled() {
                return;
            }
            notified.await;
            // Spurious wake-ups from `notify_waiters()` are theoretically
            // possible; loop back and re-check the flag rather than
            // assuming the wake-up means cancellation.
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancelled_returns_immediately_when_already_cancelled() {
        let token = CancelToken::default();
        token.cancel();
        // Should resolve essentially instantly; the timeout is a safety net.
        tokio::time::timeout(std::time::Duration::from_millis(50), token.cancelled())
            .await
            .expect("cancelled() did not return for an already-cancelled token");
    }

    #[tokio::test]
    async fn cancelled_wakes_when_cancel_fires_during_await() {
        let token = CancelToken::default();
        let waiter = token.clone();
        let handle = tokio::spawn(async move {
            waiter.cancelled().await;
        });

        // Give the spawned task a chance to enter `cancelled().await`.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        token.cancel();

        tokio::time::timeout(std::time::Duration::from_millis(100), handle)
            .await
            .expect("cancelled() did not wake within 100ms of cancel()")
            .expect("spawned task panicked");
    }

    #[tokio::test]
    async fn cancelled_wakes_multiple_concurrent_waiters() {
        let token = CancelToken::default();
        let mut handles = Vec::new();
        for _ in 0..5 {
            let w = token.clone();
            handles.push(tokio::spawn(async move { w.cancelled().await }));
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        token.cancel();

        for h in handles {
            tokio::time::timeout(std::time::Duration::from_millis(100), h)
                .await
                .expect("a waiter did not wake")
                .expect("waiter panicked");
        }
    }
}
