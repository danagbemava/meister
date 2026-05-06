//! Per-frame RMS amplitude computation + per-session emitter that pushes
//! samples to the JS side at ~30 fps.

use std::time::Duration;
use tokio::sync::mpsc;

/// Per-session emitter that buffers RMS samples on a bounded channel and
/// drains them at ~30 fps via a spawned task. Each tick, the running max
/// of buffered samples is forwarded to the user callback.
///
/// The audio callback path uses `try_send` and silently drops on a full
/// channel — the drainer already coalesces, so the only effect of a drop
/// is a slightly less peaky max for that tick.
pub struct AmplitudeEmitter {
    tx: mpsc::Sender<f32>,
    handle: tokio::task::JoinHandle<()>,
}

impl AmplitudeEmitter {
    /// Spawn the drain task. `on_tick` is called at most once per ~33ms with
    /// the running max RMS observed during that window.
    pub fn spawn<F>(on_tick: F) -> Self
    where
        F: Fn(f32) + Send + 'static,
    {
        let (tx, mut rx) = mpsc::channel::<f32>(8);
        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(33));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            // Skip the first immediate tick so we always have a window of data.
            interval.tick().await;

            let mut running_max: f32 = 0.0;
            let mut have_data = false;

            loop {
                tokio::select! {
                    biased;
                    maybe = rx.recv() => {
                        match maybe {
                            Some(v) => {
                                if v > running_max { running_max = v; }
                                have_data = true;
                            }
                            None => break,
                        }
                    }
                    _ = interval.tick() => {
                        if have_data {
                            on_tick(running_max);
                            running_max = 0.0;
                            have_data = false;
                        }
                    }
                }
            }
        });
        Self { tx, handle }
    }

    /// Non-blocking push. Drops the value if the channel is full.
    pub fn push(&self, rms: f32) {
        let _ = self.tx.try_send(rms);
    }
}

impl Drop for AmplitudeEmitter {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

/// Compute RMS over an interleaved or mono float frame, clamped to [0.0, 1.0].
pub fn compute_rms(data: &[f32]) -> f32 {
    if data.is_empty() {
        return 0.0;
    }
    let sum: f32 = data.iter().map(|&s| s * s).sum();
    let rms = (sum / data.len() as f32).sqrt();
    rms.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rms_silence_is_zero() {
        let data = vec![0.0_f32; 1024];
        assert_eq!(compute_rms(&data), 0.0);
    }

    #[test]
    fn rms_unit_sine_is_one_over_sqrt_two() {
        let n = 1024;
        let data: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * i as f32 / 16.0).sin())
            .collect();
        let r = compute_rms(&data);
        assert!((r - 1.0 / 2.0_f32.sqrt()).abs() < 1e-3, "got {r}");
    }

    #[test]
    fn rms_clamps_above_one() {
        let data = vec![2.0_f32; 8];
        assert_eq!(compute_rms(&data), 1.0);
    }

    #[test]
    fn rms_empty_is_zero() {
        let data: Vec<f32> = Vec::new();
        assert_eq!(compute_rms(&data), 0.0);
    }

    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn emitter_coalesces_and_emits_max() {
        let collected: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
        let collected_cb = collected.clone();
        let emitter = AmplitudeEmitter::spawn(move |rms| {
            collected_cb.lock().unwrap().push(rms);
        });

        // Push a burst within a single ~33ms tick window.
        emitter.push(0.1);
        emitter.push(0.5);
        emitter.push(0.3);

        // Wait long enough for at least one tick to fire.
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        drop(emitter);

        let got = collected.lock().unwrap().clone();
        assert!(!got.is_empty(), "emitter never fired");
        // The first emitted value should be the running max (0.5), not 0.1 or 0.3.
        assert_eq!(got[0], 0.5);
    }

    #[tokio::test]
    async fn emitter_drops_when_channel_full() {
        // Allocate emitter and immediately stress with more values than the
        // channel can hold; we should not panic and should still emit the max.
        let collected: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
        let collected_cb = collected.clone();
        let emitter = AmplitudeEmitter::spawn(move |rms| {
            collected_cb.lock().unwrap().push(rms);
        });

        for _ in 0..1000 {
            emitter.push(0.7);
        }

        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        drop(emitter);

        let got = collected.lock().unwrap().clone();
        assert!(got.iter().any(|&v| (v - 0.7).abs() < 1e-3), "expected 0.7 in {got:?}");
    }
}
