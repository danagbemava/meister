//! Silence gate for 16 kHz mono f32 samples.
//!
//! Drops only LONG stretches of near-silence (≥ MIN_SILENCE_MS below an
//! absolute RMS threshold). Short pauses, breaths, and quiet speech are
//! preserved so whisper still sees the context it needs. This is a
//! deliberately conservative replacement for the earlier percentile-based
//! gate, which over-cut real speech on recordings with a low noise floor.

const FRAME_MS: u32 = 20;
const MIN_SILENCE_MS: u32 = 3000;
const PAD_MS: u32 = 500;
const ABSOLUTE_THRESHOLD: f32 = 0.003;
const MIN_INPUT_MS: u32 = 100;

pub(crate) fn gate_silence(samples: &[f32], sample_rate: u32) -> Vec<f32> {
    if samples.is_empty() {
        return Vec::new();
    }
    let min_len = ms_to_samples(MIN_INPUT_MS, sample_rate);
    if samples.len() < min_len {
        return samples.to_vec();
    }

    let frame_len = ms_to_samples(FRAME_MS, sample_rate).max(1);
    let min_silence_samples = ms_to_samples(MIN_SILENCE_MS, sample_rate);
    let pad_samples = ms_to_samples(PAD_MS, sample_rate);

    if !any_audible(samples, frame_len) {
        return Vec::new();
    }

    let mut out: Vec<f32> = Vec::with_capacity(samples.len());
    let mut cursor: usize = 0;
    let mut silence_start: Option<usize> = None;

    let mut i = 0;
    while i + frame_len <= samples.len() {
        let frame = &samples[i..i + frame_len];
        let is_silent = frame_rms(frame, frame_len) < ABSOLUTE_THRESHOLD;

        if is_silent {
            if silence_start.is_none() {
                silence_start = Some(i);
            }
        } else if let Some(start) = silence_start.take() {
            let end = i;
            if end - start >= min_silence_samples {
                let keep_until = (start + pad_samples).min(samples.len());
                out.extend_from_slice(&samples[cursor..keep_until]);
                cursor = end.saturating_sub(pad_samples).max(keep_until);
            }
        }
        i += frame_len;
    }

    if let Some(start) = silence_start {
        let end = samples.len();
        if end - start >= min_silence_samples {
            let keep_until = (start + pad_samples).min(samples.len());
            out.extend_from_slice(&samples[cursor..keep_until]);
            cursor = samples.len();
        }
    }
    out.extend_from_slice(&samples[cursor..]);

    out
}

fn any_audible(samples: &[f32], frame_len: usize) -> bool {
    let mut i = 0;
    while i + frame_len <= samples.len() {
        if frame_rms(&samples[i..i + frame_len], frame_len) >= ABSOLUTE_THRESHOLD {
            return true;
        }
        i += frame_len;
    }
    false
}

fn frame_rms(frame: &[f32], frame_len: usize) -> f32 {
    let sum_sq: f32 = frame.iter().map(|s| s * s).sum();
    (sum_sq / frame_len as f32).sqrt()
}

fn ms_to_samples(ms: u32, sample_rate: u32) -> usize {
    (sample_rate as usize * ms as usize) / 1000
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 16_000;

    fn ms_samples(ms: u32) -> usize {
        (SR as usize * ms as usize) / 1000
    }

    fn silence(ms: u32) -> Vec<f32> {
        vec![0.0; ms_samples(ms)]
    }

    fn speech(ms: u32) -> Vec<f32> {
        let n = ms_samples(ms);
        (0..n)
            .map(|i| {
                let t = i as f32 / SR as f32;
                0.1 * (2.0 * std::f32::consts::PI * 200.0 * t).sin()
            })
            .collect()
    }

    #[test]
    fn empty_input_returns_empty() {
        assert!(gate_silence(&[], SR).is_empty());
    }

    #[test]
    fn all_silence_returns_empty() {
        assert!(gate_silence(&silence(4000), SR).is_empty());
    }

    #[test]
    fn all_speech_passes_through_unchanged() {
        let input = speech(2000);
        let out = gate_silence(&input, SR);
        assert_eq!(out.len(), input.len(), "no silence → bit-exact passthrough");
    }

    #[test]
    fn below_min_input_length_bypasses() {
        let input = speech(50);
        let out = gate_silence(&input, SR);
        assert_eq!(out.len(), input.len());
    }

    #[test]
    fn short_silence_is_preserved() {
        let mut input: Vec<f32> = Vec::new();
        input.extend_from_slice(&speech(500));
        input.extend_from_slice(&silence(1000));
        input.extend_from_slice(&speech(500));
        let out = gate_silence(&input, SR);
        assert_eq!(out.len(), input.len(), "sub-MIN_SILENCE_MS gaps must pass through");
    }

    #[test]
    fn silence_just_under_min_is_preserved() {
        let mut input: Vec<f32> = Vec::new();
        input.extend_from_slice(&speech(500));
        input.extend_from_slice(&silence(MIN_SILENCE_MS - 100));
        input.extend_from_slice(&speech(500));
        let out = gate_silence(&input, SR);
        assert_eq!(out.len(), input.len());
    }

    #[test]
    fn long_silence_middle_is_trimmed() {
        let mut input: Vec<f32> = Vec::new();
        input.extend_from_slice(&speech(500));
        input.extend_from_slice(&silence(5000));
        input.extend_from_slice(&speech(500));
        let out = gate_silence(&input, SR);
        let expected_ms = 500 + PAD_MS + PAD_MS + 500;
        let expected = ms_samples(expected_ms);
        let tol = ms_samples(FRAME_MS * 2);
        assert!(
            (out.len() as i64 - expected as i64).abs() < tol as i64,
            "got {} samples, expected ~{}",
            out.len(),
            expected
        );
        assert!(out.len() < input.len());
    }

    #[test]
    fn trailing_long_silence_is_trimmed() {
        let mut input: Vec<f32> = Vec::new();
        input.extend_from_slice(&speech(500));
        input.extend_from_slice(&silence(5000));
        let out = gate_silence(&input, SR);
        let expected_ms = 500 + PAD_MS;
        let expected = ms_samples(expected_ms);
        let tol = ms_samples(FRAME_MS * 2);
        assert!(
            (out.len() as i64 - expected as i64).abs() < tol as i64,
            "got {} samples, expected ~{}",
            out.len(),
            expected
        );
    }

    #[test]
    fn multiple_long_silences_all_trimmed() {
        let mut input: Vec<f32> = Vec::new();
        input.extend_from_slice(&speech(300));
        input.extend_from_slice(&silence(4000));
        input.extend_from_slice(&speech(300));
        input.extend_from_slice(&silence(4000));
        input.extend_from_slice(&speech(300));
        let out = gate_silence(&input, SR);
        let expected_ms = 300 * 3 + PAD_MS * 4;
        let expected = ms_samples(expected_ms);
        let tol = ms_samples(FRAME_MS * 4);
        assert!(
            (out.len() as i64 - expected as i64).abs() < tol as i64,
            "got {} samples, expected ~{}",
            out.len(),
            expected
        );
    }

    #[test]
    fn deterministic() {
        let mut input: Vec<f32> = Vec::new();
        input.extend_from_slice(&silence(500));
        input.extend_from_slice(&speech(1000));
        input.extend_from_slice(&silence(4000));
        input.extend_from_slice(&speech(500));
        let a = gate_silence(&input, SR);
        let b = gate_silence(&input, SR);
        assert_eq!(a, b);
    }
}
