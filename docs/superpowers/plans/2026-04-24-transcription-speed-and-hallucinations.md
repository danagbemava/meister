# Transcription Speed & Hallucinations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate whisper's repetition-loop and YouTube-boilerplate hallucinations on real recordings, and shave 20–40 % off transcription wall time, by stripping silence before whisper, activating whisper.cpp's internal threshold-based fallback loop, and post-processing segments to collapse lock-in loops and drop known boilerplate.

**Architecture:** Two new pure-function modules (`transcription/silence.rs`, `transcription/postprocess.rs`) plus small additions to `FullParams` inside `transcription/engine.rs`. The whole thing is wrapped around the existing `state.full()` call: samples get gated before, segments get filtered after. No changes to public API, orchestrator, storage, or frontend.

**Tech Stack:** Rust (whisper-rs 0.11.1, hound, regex), no new crate dependencies.

**Spec:** `docs/superpowers/specs/2026-04-24-transcription-speed-and-hallucinations-design.md`

---

## File Structure

**Created:**
- `src-tauri/src/transcription/silence.rs` — `gate_silence(samples: &[f32], sample_rate: u32) -> Vec<f32>` plus internal helpers and `#[cfg(test)] mod tests`.
- `src-tauri/src/transcription/postprocess.rs` — `collapse_repetitions(segments: &[String]) -> Vec<String>`, `filter_hallucinations(segments: &[String]) -> Vec<String>`, `is_hallucination(segment: &str) -> bool`, and `#[cfg(test)] mod tests`.

**Modified:**
- `src-tauri/src/transcription/mod.rs` — declare `silence` and `postprocess` submodules.
- `src-tauri/src/transcription/engine.rs` — wire the silence gate pre-`state.full()`, add six whisper `FullParams` setters, wire the two post-process layers pre-concatenation.

**Not touched:** orchestrator, commands, storage, frontend, `models.rs`.

---

## Task 1: Scaffold `silence.rs` module and wire it into `mod.rs`

**Files:**
- Create: `src-tauri/src/transcription/silence.rs`
- Modify: `src-tauri/src/transcription/mod.rs`

- [ ] **Step 1: Create the empty module**

Create `src-tauri/src/transcription/silence.rs` with just the signature and a `todo!()`:

```rust
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
```

- [ ] **Step 2: Add module declaration**

Edit `src-tauri/src/transcription/mod.rs`. After `pub mod models;` add:

```rust
pub(crate) mod silence;
pub(crate) mod postprocess;
```

(We add both now so a later task doesn't revisit `mod.rs`.)

- [ ] **Step 3: Create the stub `postprocess.rs` so the `mod` declaration compiles**

Create `src-tauri/src/transcription/postprocess.rs`:

```rust
//! Post-processing of whisper segment output: collapses lock-in loops
//! and drops known hallucinated boilerplate.

pub(crate) fn collapse_repetitions(segments: &[String]) -> Vec<String> {
    segments.to_vec()
}

pub(crate) fn filter_hallucinations(segments: &[String]) -> Vec<String> {
    segments.to_vec()
}

#[cfg(test)]
mod tests {}
```

- [ ] **Step 4: Verify the crate still builds**

Run: `cargo build -p meister --manifest-path src-tauri/Cargo.toml`
Expected: clean build.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/transcription/silence.rs src-tauri/src/transcription/postprocess.rs src-tauri/src/transcription/mod.rs
git commit -m "transcription: scaffold silence + postprocess modules"
```

---

## Task 2: Implement `collapse_repetitions` (the highest-value post-process layer)

**Files:**
- Modify: `src-tauri/src/transcription/postprocess.rs`

- [ ] **Step 1: Write the failing tests**

Replace the contents of `#[cfg(test)] mod tests {}` in `postprocess.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn segs(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn collapse_triple_run_to_one() {
        let input = segs(&["A", "A", "A", "B"]);
        let out = collapse_repetitions(&input);
        assert_eq!(out, segs(&["A", "B"]));
    }

    #[test]
    fn collapse_keeps_doubles_as_emphasis() {
        let input = segs(&["A", "A", "B"]);
        let out = collapse_repetitions(&input);
        assert_eq!(out, input);
    }

    #[test]
    fn collapse_long_run_to_one() {
        let input: Vec<String> = std::iter::repeat("hello".to_string()).take(10).collect();
        let out = collapse_repetitions(&input);
        assert_eq!(out, segs(&["hello"]));
    }

    #[test]
    fn collapse_preserves_alternation() {
        let input = segs(&["A", "B", "A", "B", "A", "B"]);
        let out = collapse_repetitions(&input);
        assert_eq!(out, input);
    }

    #[test]
    fn collapse_interleaved_runs() {
        // A A A B A A A A → A B A
        let input = segs(&["A", "A", "A", "B", "A", "A", "A", "A"]);
        let out = collapse_repetitions(&input);
        assert_eq!(out, segs(&["A", "B", "A"]));
    }

    #[test]
    fn collapse_normalizes_case_and_punctuation() {
        // "Yes." and "yes" should be considered equal for run detection.
        let input = segs(&["Yes.", "yes", "YES", "yes!", "no."]);
        let out = collapse_repetitions(&input);
        // First two still kept (threshold = 3), third and fourth dropped.
        assert_eq!(out, segs(&["Yes.", "yes", "no."]));
    }

    #[test]
    fn collapse_iron_rod_fixture() {
        // Reproduces the real-world failure: 23 copies of one loop
        // followed by 34 copies of another. Expected: 2 surviving segments.
        let mut input: Vec<String> = Vec::new();
        for _ in 0..23 {
            input.push("And the iron rod and cement are growing up.".to_string());
        }
        for _ in 0..34 {
            input.push("I think it's a little different.".to_string());
        }
        let out = collapse_repetitions(&input);
        assert_eq!(out, segs(&[
            "And the iron rod and cement are growing up.",
            "I think it's a little different.",
        ]));
    }

    #[test]
    fn collapse_empty_input_returns_empty() {
        let out = collapse_repetitions(&[]);
        assert!(out.is_empty());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p meister --manifest-path src-tauri/Cargo.toml --lib postprocess::tests::collapse_`
Expected: the 7 non-empty-input tests fail (the stub `collapse_repetitions` just clones input). `collapse_empty_input_returns_empty` passes vacuously.

- [ ] **Step 3: Implement `collapse_repetitions` + `normalize`**

Replace the stub `collapse_repetitions` body in `postprocess.rs` with:

```rust
/// Runs of ≥ REPETITION_THRESHOLD consecutive segments that normalize to
/// the same string collapse to a single occurrence. The first and second
/// are kept (two-in-a-row is plausible emphasis); from the third onward
/// they are dropped until the run ends.
const REPETITION_THRESHOLD: usize = 3;

pub(crate) fn collapse_repetitions(segments: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(segments.len());
    let mut last_norm: Option<String> = None;
    let mut run_len: usize = 0;

    for seg in segments {
        let n = normalize(seg);
        let same_as_last = last_norm.as_deref() == Some(n.as_str());
        if same_as_last {
            run_len += 1;
            if run_len < REPETITION_THRESHOLD {
                out.push(seg.clone());
            }
            // run_len >= THRESHOLD → drop
        } else {
            run_len = 1;
            last_norm = Some(n);
            out.push(seg.clone());
        }
    }
    out
}

/// Lowercase, trim, collapse internal whitespace, strip a single run of
/// trailing punctuation (.,!?…). Matches "Yes." and "yes" as equal.
fn normalize(s: &str) -> String {
    let trimmed = s.trim();
    let mut collapsed = String::with_capacity(trimmed.len());
    let mut prev_ws = false;
    for ch in trimmed.chars() {
        if ch.is_whitespace() {
            if !prev_ws { collapsed.push(' '); }
            prev_ws = true;
        } else {
            collapsed.push(ch.to_ascii_lowercase());
            prev_ws = false;
        }
    }
    let mut result: &str = &collapsed;
    while let Some(last) = result.chars().next_back() {
        if matches!(last, '.' | ',' | '!' | '?' | '…') {
            result = &result[..result.len() - last.len_utf8()];
        } else {
            break;
        }
    }
    result.to_string()
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p meister --manifest-path src-tauri/Cargo.toml --lib postprocess::tests::collapse_`
Expected: all 8 `collapse_*` tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/transcription/postprocess.rs
git commit -m "transcription: implement collapse_repetitions (kill whisper lock-in loops)"
```

---

## Task 3: Implement `is_hallucination` + `filter_hallucinations` (the boilerplate layer)

**Files:**
- Modify: `src-tauri/src/transcription/postprocess.rs`

- [ ] **Step 1: Write the failing tests**

Append to `#[cfg(test)] mod tests` in `postprocess.rs`:

```rust
    #[test]
    fn hallucination_matches_youtube_boilerplate() {
        assert!(is_hallucination("Thank you for watching!"));
        assert!(is_hallucination("thanks for watching"));
        assert!(is_hallucination("Please subscribe and hit the bell."));
        assert!(is_hallucination("like and subscribe"));
        assert!(is_hallucination("Don't forget to subscribe."));
        assert!(is_hallucination("See you in the next video."));
        assert!(is_hallucination("See you next time."));
    }

    #[test]
    fn hallucination_matches_bracket_markers() {
        assert!(is_hallucination("[Music]"));
        assert!(is_hallucination("[MUSIC]"));
        assert!(is_hallucination("[Applause]"));
        assert!(is_hallucination("[Laughter]"));
        assert!(is_hallucination("♪"));
    }

    #[test]
    fn hallucination_matches_non_english_boilerplate() {
        assert!(is_hallucination("ご視聴ありがとうございました"));
        assert!(is_hallucination("字幕 by someone"));
        assert!(is_hallucination("Translated by fan-sub team"));
    }

    #[test]
    fn hallucination_matches_short_forms() {
        assert!(is_hallucination("you"));
        assert!(is_hallucination("You."));
        assert!(is_hallucination("Thank you."));
        assert!(is_hallucination("Thanks."));
        assert!(is_hallucination(""));
        assert!(is_hallucination("   "));
        assert!(is_hallucination("..."));
        assert!(is_hallucination("[inaudible]"));
        assert!(is_hallucination("inaudible"));
    }

    #[test]
    fn hallucination_preserves_legit_content() {
        assert!(!is_hallucination("I think we should ship next week."));
        assert!(!is_hallucination("Thanks, John."));
        assert!(!is_hallucination("This is a video about Kubernetes."));
        assert!(!is_hallucination("[inaudible speech at 2:34]"));
        assert!(!is_hallucination("Yes, exactly."));
        assert!(!is_hallucination("You should try the new API."));
    }

    #[test]
    fn filter_drops_hallucinations_preserves_order() {
        let input = segs(&[
            "Good morning.",
            "[Music]",
            "Let's start the review.",
            "Thank you for watching.",
            "Next item on the agenda.",
        ]);
        let out = filter_hallucinations(&input);
        assert_eq!(out, segs(&[
            "Good morning.",
            "Let's start the review.",
            "Next item on the agenda.",
        ]));
    }

    #[test]
    fn compound_collapse_then_filter_real_sample() {
        let mut input: Vec<String> = Vec::new();
        for _ in 0..23 {
            input.push("And the iron rod and cement are growing up.".to_string());
        }
        input.push("Thank you for watching!".to_string());
        for _ in 0..34 {
            input.push("I think it's a little different.".to_string());
        }
        let collapsed = collapse_repetitions(&input);
        let out = filter_hallucinations(&collapsed);
        assert_eq!(out, segs(&[
            "And the iron rod and cement are growing up.",
            "I think it's a little different.",
        ]));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p meister --manifest-path src-tauri/Cargo.toml --lib postprocess::tests`
Expected: the 7 new `hallucination_*` / `filter_*` / `compound_*` tests fail. `collapse_*` still pass.

- [ ] **Step 3: Add the `regex` use + implement `is_hallucination` and `filter_hallucinations`**

At the top of `postprocess.rs` (the non-test module), add:

```rust
use std::sync::OnceLock;
```

Replace the stub `filter_hallucinations` body and add `is_hallucination`:

```rust
const LITERAL_BLOCKLIST: &[&str] = &[
    "thank you for watching",
    "thanks for watching",
    "please subscribe",
    "like and subscribe",
    "don't forget to subscribe",
    "see you in the next video",
    "see you next time",
    "[music]",
    "[applause]",
    "[laughter]",
    "♪",
    "ご視聴ありがとうございました",
    "字幕 by",
    "translated by",
];

fn full_segment_patterns() -> &'static [regex::Regex] {
    static PATTERNS: OnceLock<Vec<regex::Regex>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        [
            r"^(?i)(you|thank you|thanks)[.!\s]*$",
            r"^\s*\.*\s*$",
            r"^\[?\s*(?i)inaudible\s*\]?$",
        ]
        .iter()
        .map(|p| regex::Regex::new(p).expect("hallucination pattern should compile"))
        .collect()
    })
}

/// True if a segment matches any known hallucination pattern. Matched
/// case-insensitively against the trimmed segment. Literal entries are
/// substring matches; regex entries are full-segment matches.
pub(crate) fn is_hallucination(segment: &str) -> bool {
    let trimmed = segment.trim();
    if trimmed.is_empty() {
        return true;
    }
    let lower = trimmed.to_lowercase();
    if LITERAL_BLOCKLIST.iter().any(|needle| lower.contains(needle)) {
        return true;
    }
    full_segment_patterns().iter().any(|re| re.is_match(trimmed))
}

pub(crate) fn filter_hallucinations(segments: &[String]) -> Vec<String> {
    segments
        .iter()
        .filter(|s| !is_hallucination(s))
        .cloned()
        .collect()
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p meister --manifest-path src-tauri/Cargo.toml --lib postprocess::tests`
Expected: all postprocess tests PASS (8 collapse + 5 hallucination + 1 filter + 1 compound = 15 total).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/transcription/postprocess.rs
git commit -m "transcription: implement boilerplate hallucination filter"
```

---

## Task 4: Implement `gate_silence` RMS energy gate

**Files:**
- Modify: `src-tauri/src/transcription/silence.rs`

- [ ] **Step 1: Write the failing tests**

Replace the `#[cfg(test)] mod tests {}` in `silence.rs` with:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p meister --manifest-path src-tauri/Cargo.toml --lib silence::tests`
Expected: `all_silence_returns_empty` and `speech_silence_speech_keeps_speech_drops_silence` fail (the stub returns input unchanged). `empty_input_returns_empty`, `below_min_input_length_bypasses`, `all_speech_returned_close_to_unchanged`, and `deterministic` pass vacuously.

- [ ] **Step 3: Implement `gate_silence`**

Replace the stub body in `silence.rs` with:

```rust
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
    let mut sorted = rms.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((sorted.len() as f32) * NOISE_FLOOR_PERCENTILE) as usize;
    let noise_floor = sorted[idx.min(sorted.len() - 1)];
    let threshold = (noise_floor * THRESHOLD_MULTIPLIER).max(MIN_THRESHOLD);

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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p meister --manifest-path src-tauri/Cargo.toml --lib silence::tests`
Expected: all 6 silence tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/transcription/silence.rs
git commit -m "transcription: implement RMS-based silence gate"
```

---

## Task 5: Wire silence gate + whisper param tuning + post-process into `engine.rs`

**Files:**
- Modify: `src-tauri/src/transcription/engine.rs`

- [ ] **Step 1: Add the silence gate + early-return**

Find (at the top of `transcribe_wav_with_callback` in `engine.rs`):

```rust
    let samples = read_wav_as_f32_mono(wav_path)?;
    let ctx = load_context(models_dir, size)?;
```

Replace with:

```rust
    let samples = read_wav_as_f32_mono(wav_path)?;
    let samples = crate::transcription::silence::gate_silence(&samples, 16_000);
    if samples.is_empty() {
        return Ok(String::new());
    }
    let ctx = load_context(models_dir, size)?;
```

- [ ] **Step 2: Add the six whisper threshold / fallback params**

Find the existing params block that ends with:

```rust
    params.set_no_context(true);
    params.set_suppress_non_speech_tokens(true);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_special(false);
    params.set_print_timestamps(false);
```

Insert immediately before `params.set_print_progress(false);`:

```rust
    params.set_temperature(0.0);
    params.set_temperature_inc(0.2);
    params.set_no_speech_thold(0.6);
    params.set_logprob_thold(-1.0);
    params.set_entropy_thold(2.4);
    params.set_suppress_blank(true);
```

- [ ] **Step 3: Replace the segment-concat loop with collect + post-process**

Find (at the bottom of `transcribe_wav_with_callback`):

```rust
    let n = state.full_n_segments().map_err(|e| format!("n_segments: {e}"))?;
    let mut out = String::new();
    for i in 0..n {
        out.push_str(&state.full_get_segment_text(i).map_err(|e| format!("segment {i}: {e}"))?);
    }
    Ok(out.trim().to_string())
```

Replace with:

```rust
    let n = state.full_n_segments().map_err(|e| format!("n_segments: {e}"))?;
    let mut segments: Vec<String> = Vec::with_capacity(n as usize);
    for i in 0..n {
        segments.push(
            state.full_get_segment_text(i).map_err(|e| format!("segment {i}: {e}"))?,
        );
    }
    let segments = crate::transcription::postprocess::collapse_repetitions(&segments);
    let segments = crate::transcription::postprocess::filter_hallucinations(&segments);
    Ok(segments.join(" ").trim().to_string())
```

- [ ] **Step 4: Build and run the existing test suite**

Run: `cargo build -p meister --manifest-path src-tauri/Cargo.toml`
Expected: clean build.

Run: `cargo test -p meister --manifest-path src-tauri/Cargo.toml --lib`
Expected: all existing tests pass plus the new silence + postprocess tests added in earlier tasks. No regressions in the format, storage, or orchestrator tests.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/transcription/engine.rs
git commit -m "transcription: gate silence, tune whisper thresholds, post-process segments"
```

---

## Task 6: Manual verification on a real recording

**Files:** none

- [ ] **Step 1: Pick a known-problematic recording**

Use a recording that previously produced repetition-loop hallucinations (the iron-rod/cement one from the user's reports is ideal). If unavailable, any recording with long silent stretches will do.

- [ ] **Step 2: Clear the recording's cached output**

From the app, navigate to the recording and click `Start over` to clear the stage outputs. This forces Transcribe to re-run with the new pipeline.

- [ ] **Step 3: Run end-to-end**

Click `Run`. Let Extract → Transcribe → Format → Save complete.

Expected:

- Transcribe stage wall time is noticeably shorter than before (20–40 % on recordings with silence).
- The produced Transcript tab shows no 20+ repetitions of any phrase.
- No "Thank you for watching" / "Please subscribe" / "[Music]" lines unless they were truly spoken.
- Legit content reads as expected — no missing sentences in speech-heavy regions.

- [ ] **Step 4: Record before/after timings**

Note the Transcribe stage wall time in the recording UI (or in logs if exposed) and compare to the last known pre-change timing for the same recording. Paste the numbers into the PR description when merging.

- [ ] **Step 5: (If anything regresses) file a follow-up**

If legit speech is being trimmed, the most likely culprit is an aggressive noise-floor estimate on a particularly clean recording. The constants in `silence.rs` (`NOISE_FLOOR_PERCENTILE`, `THRESHOLD_MULTIPLIER`, `MIN_THRESHOLD`, `PAD_MS`) are the tuning knobs; nudge `MIN_THRESHOLD` down or `PAD_MS` up in a follow-up PR.

---

## Self-review

- **Spec coverage**:
  - Silence gate algorithm + constants → Task 4.
  - Whisper threshold params → Task 5 (Step 2).
  - `collapse_repetitions` + `REPETITION_THRESHOLD = 3` → Task 2.
  - `filter_hallucinations` with literal + regex layers → Task 3.
  - Integration diff in `engine.rs` → Task 5 (Steps 1, 3).
  - Module declarations → Task 1.
  - `silence.rs` unit tests → Task 4 (Step 1).
  - `postprocess.rs` unit tests, including iron-rod fixture → Tasks 2, 3.
  - Manual verification on a real recording → Task 6.
  - Optional `#[ignore]`d real-audio fixture → dropped (the unit tests on pasted fixture already catch the specific bug; the spec marked this as optional).

- **Placeholders**: none. Each code step contains the full function body or the exact block to insert.

- **Type consistency**: `collapse_repetitions`, `filter_hallucinations`, `is_hallucination`, `gate_silence` signatures match exactly across Tasks 1 through 5.

- **Commits**: 6 per-task commits, each independently compiling, each with a focused scope.
