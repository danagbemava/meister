//! RMS-based silence gate for 16 kHz mono f32 samples.
//!
//! Strips silent regions so whisper only processes speech, avoiding the
//! 30-second windows of silence that produce lock-in loops and YouTube
//! boilerplate hallucinations.

const FRAME_MS: u32 = 20;
const ATTACK_MS: u32 = 100;
const RELEASE_MS: u32 = 300;
const PAD_MS: u32 = 200;
const MIN_REGION_MS: u32 = 200;
const MIN_INPUT_MS: u32 = 100;
const NOISE_FLOOR_PERCENTILE: f32 = 0.20;
const THRESHOLD_MULTIPLIER: f32 = 4.0;
const MIN_THRESHOLD: f32 = 0.003;

pub(crate) fn gate_silence(samples: &[f32], sample_rate: u32) -> Vec<f32> {
    if samples.is_empty() {
        return Vec::new();
    }
    let min_len = ms_to_samples(MIN_INPUT_MS, sample_rate);
    if samples.len() < min_len {
        return samples.to_vec();
    }

    let frame_len = ms_to_samples(FRAME_MS, sample_rate).max(1);
    let attack_frames = (ATTACK_MS / FRAME_MS).max(1) as usize;
    let release_frames = (RELEASE_MS / FRAME_MS).max(1) as usize;
    let pad_samples = ms_to_samples(PAD_MS, sample_rate);
    let min_region_samples = ms_to_samples(MIN_REGION_MS, sample_rate);

    // Per-frame RMS.
    let mut rms: Vec<f32> = Vec::with_capacity(samples.len() / frame_len + 1);
    let mut i = 0;
    while i + frame_len <= samples.len() {
        let frame = &samples[i..i + frame_len];
        let sum_sq: f32 = frame.iter().map(|s| s * s).sum();
        rms.push((sum_sq / frame_len as f32).sqrt());
        i += frame_len;
    }
    if rms.is_empty() {
        return samples.to_vec();
    }

    // Noise floor = 20th percentile of frame RMS. Threshold = max(floor × 4, MIN).
    // When the 20th and 80th percentiles are close (ratio > 0.5), the signal is
    // uniformly loud with no silent baseline — noise floor estimate is unreliable,
    // so fall back to MIN_THRESHOLD to avoid gating out clean speech.
    let mut sorted = rms.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p20_idx = ((sorted.len() as f32) * NOISE_FLOOR_PERCENTILE) as usize;
    let p80_idx = ((sorted.len() as f32) * 0.80) as usize;
    let noise_floor = sorted[p20_idx.min(sorted.len() - 1)];
    let p80 = sorted[p80_idx.min(sorted.len() - 1)];
    let uniform_signal = p80 > 0.0 && (noise_floor / p80) > 0.5;
    let threshold = if uniform_signal {
        MIN_THRESHOLD
    } else {
        (noise_floor * THRESHOLD_MULTIPLIER).max(MIN_THRESHOLD)
    };

    // Walk frames, emit (start_frame, end_frame_exclusive) regions in SPEECH state.
    let mut regions: Vec<(usize, usize)> = Vec::new();
    let mut in_speech = false;
    let mut region_start: usize = 0;
    let mut below_run: usize = 0;
    let mut above_run: usize = 0;
    let mut last_above: usize = 0;

    for (fi, &r) in rms.iter().enumerate() {
        let above = r >= threshold;
        if in_speech {
            if above {
                last_above = fi;
                below_run = 0;
            } else {
                below_run += 1;
                if below_run >= release_frames {
                    regions.push((region_start, last_above + 1));
                    in_speech = false;
                    below_run = 0;
                    above_run = 0;
                }
            }
        } else if above {
            above_run += 1;
            if above_run >= attack_frames {
                region_start = fi + 1 - above_run;
                in_speech = true;
                last_above = fi;
                below_run = 0;
            }
        } else {
            above_run = 0;
        }
    }
    if in_speech {
        regions.push((region_start, last_above + 1));
    }

    // Convert frame regions to sample regions, apply padding, drop tiny regions.
    let total = samples.len();
    let mut padded: Vec<(usize, usize)> = regions
        .into_iter()
        .map(|(a, b)| {
            let start = (a * frame_len).saturating_sub(pad_samples);
            let end = ((b * frame_len) + pad_samples).min(total);
            (start, end)
        })
        .filter(|(a, b)| b.saturating_sub(*a) >= min_region_samples)
        .collect();

    // Merge overlapping/adjacent regions after padding.
    padded.sort();
    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(padded.len());
    for region in padded {
        match merged.last_mut() {
            Some(last) if last.1 >= region.0 => last.1 = last.1.max(region.1),
            _ => merged.push(region),
        }
    }

    let mut out: Vec<f32> = Vec::with_capacity(merged.iter().map(|(a, b)| b - a).sum());
    for (a, b) in merged {
        out.extend_from_slice(&samples[a..b]);
    }
    out
}

fn ms_to_samples(ms: u32, sample_rate: u32) -> usize {
    (sample_rate as usize * ms as usize) / 1000
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 16_000;

    fn ms_to_samples(ms: u32) -> usize {
        (SR as usize * ms as usize) / 1000
    }

    /// Build a silence buffer of the given length.
    fn silence(ms: u32) -> Vec<f32> {
        vec![0.0; ms_to_samples(ms)]
    }

    /// Build a "speech-like" buffer: a 200 Hz sine with ~0.1 amplitude.
    /// Loud enough to be well above the noise-floor threshold.
    fn speech(ms: u32) -> Vec<f32> {
        let n = ms_to_samples(ms);
        (0..n)
            .map(|i| {
                let t = i as f32 / SR as f32;
                0.1 * (2.0 * std::f32::consts::PI * 200.0 * t).sin()
            })
            .collect()
    }

    #[test]
    fn empty_input_returns_empty() {
        let out = gate_silence(&[], SR);
        assert!(out.is_empty());
    }

    #[test]
    fn all_silence_returns_empty() {
        let out = gate_silence(&silence(2000), SR);
        assert!(out.is_empty(), "expected empty, got {} samples", out.len());
    }

    #[test]
    fn all_speech_returned_close_to_unchanged() {
        let input = speech(2000);
        let out = gate_silence(&input, SR);
        // May trim a tiny bit at edges; allow ±300 ms of padding/attack slack.
        let tolerance = ms_to_samples(300);
        assert!(
            out.len() + tolerance >= input.len(),
            "output too short: got {}, expected ≥ {}",
            out.len(),
            input.len() - tolerance
        );
        assert!(
            out.len() <= input.len() + tolerance,
            "output too long: got {}, expected ≤ {}",
            out.len(),
            input.len() + tolerance
        );
    }

    #[test]
    fn below_min_input_length_bypasses() {
        let input = speech(50); // 50 ms < MIN_INPUT_MS
        let out = gate_silence(&input, SR);
        assert_eq!(out.len(), input.len(), "short clips must bypass the gate");
    }

    #[test]
    fn speech_silence_speech_keeps_speech_drops_silence() {
        let mut input: Vec<f32> = Vec::new();
        input.extend_from_slice(&speech(500));
        input.extend_from_slice(&silence(2000));
        input.extend_from_slice(&speech(500));
        let out = gate_silence(&input, SR);
        // Speech total is 1000 ms; allow up to PAD_MS*2 per region of padding.
        let speech_ms = 1000;
        let max_padding_ms = 200 * 4; // two regions × pre+post pad
        let max_expected = ms_to_samples(speech_ms + max_padding_ms);
        let min_expected = ms_to_samples(speech_ms); // at minimum, recover speech
        assert!(
            out.len() >= min_expected,
            "too much trimmed: got {}, expected ≥ {}",
            out.len(),
            min_expected
        );
        assert!(
            out.len() <= max_expected,
            "not enough silence removed: got {}, expected ≤ {}",
            out.len(),
            max_expected
        );
        // And the result must be strictly shorter than the input.
        assert!(out.len() < input.len(), "gate should have removed the 2 s silence");
    }

    #[test]
    fn deterministic() {
        let input = {
            let mut v = Vec::new();
            v.extend_from_slice(&silence(500));
            v.extend_from_slice(&speech(1000));
            v.extend_from_slice(&silence(500));
            v.extend_from_slice(&speech(500));
            v
        };
        let a = gate_silence(&input, SR);
        let b = gate_silence(&input, SR);
        assert_eq!(a, b);
    }
}
