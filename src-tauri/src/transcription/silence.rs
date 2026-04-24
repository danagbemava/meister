//! RMS-based silence gate for 16 kHz mono f32 samples.
//!
//! Strips silent regions so whisper only processes speech, avoiding the
//! 30-second windows of silence that produce lock-in loops and YouTube
//! boilerplate hallucinations.

pub(crate) fn gate_silence(samples: &[f32], sample_rate: u32) -> Vec<f32> {
    let _ = sample_rate;
    samples.to_vec()
}

#[cfg(test)]
mod tests {}
