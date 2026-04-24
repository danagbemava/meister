# Transcription Speed & Hallucinations — Design

**Status:** Draft
**Date:** 2026-04-24

## Problem

The transcription stage is now the slowest step after formatting was parallelized, and whisper frequently produces two classes of hallucinations on real recordings:

- **Repetition loops (type A)** — the same plausible-looking sentence is emitted many times in a row. Observed in production: one recording produced 23 consecutive copies of "And the iron rod and cement are growing up." followed by 34 copies of "I think it's a little different." Neither line is boilerplate; whisper locked into the decode and couldn't escape.
- **Fabricated boilerplate (type B)** — "Thank you for watching", "Please subscribe", "[Music]", "ご視聴ありがとうございました" inserted in silent or low-audio stretches. Whisper's training data is dominated by YouTube, and silence is its favorite excuse to emit a YouTube outro.

Both classes share a single root cause: whisper is being fed 30-second windows that are silent or near-silent, and has to emit *something*. If we don't feed it silence, both classes largely disappear. Silence removal is also the largest single speed lever, because typical meeting audio is 20–40% silence that whisper is currently processing for nothing.

## Goals

- Eliminate or near-eliminate repetition-loop hallucinations on real recordings.
- Eliminate YouTube-boilerplate hallucinations.
- Reduce transcription wall time by 20–40% on typical meeting audio.
- No new heavy dependencies (no onnxruntime, no external VAD service).
- No changes to public API surface (`run_transcribe`, orchestrator contract, UI events) except what follows naturally from reduced hallucinations.

## Non-goals

- **Neural VAD (Silero, Whisper's built-in VAD).** Silero needs onnxruntime; whisper's VAD isn't exposed in `whisper-rs` 0.11. A simple energy gate is enough for the quality bar we need.
- **Per-segment confidence-based dropping in Rust.** `whisper-rs` 0.11 doesn't expose per-segment `no_speech_prob` or `avg_logprob` accessors. Whisper.cpp's internal fallback loop uses those thresholds internally and drops bad segments itself — we just need to set the thresholds.
- **Whisper-turbo or quantized model variants.** Orthogonal speed lever, deferred as its own spec.
- **User-editable hallucination blocklist.** Overkill for v1. If a second user hits the same hallucination, promote it to code.
- **Preserving timestamps across the silence gate.** The current transcription path strips timestamps (`set_print_timestamps(false)`), so compressed-audio time is never exposed anyway.

## Approach

Three layers, each independently useful, combining to address both goals:

1. **Pre-whisper: RMS-based silence gate.** Strip silent regions from the input WAV before handing it to whisper. Same apparent speech, 20–40% fewer samples to decode.
2. **Inside whisper: fallback-loop parameters.** Set `no_speech_thold`, `logprob_thold`, `temperature_inc`, `entropy_thold`, `suppress_blank` so whisper.cpp retries bad segments at higher temperature and drops what still fails.
3. **Post-whisper: consecutive-duplicate collapse + boilerplate blocklist.** Whisper's own fallback doesn't always catch loops of plausible-looking text. A simple post-process pass catches residual repetitions and the known YouTube-boilerplate set.

Layers 1 and 2 do most of the work; layer 3 is a small, bounded belt-and-suspenders catch.

## Architecture

### New files

- `src-tauri/src/transcription/silence.rs` — `gate_silence(samples: &[f32], sample_rate: u32) -> Vec<f32>`. Pure function, no whisper dependency, unit-testable.
- `src-tauri/src/transcription/postprocess.rs` — `collapse_repetitions(segments: &[String]) -> Vec<String>` and `filter_hallucinations(segments: &[String]) -> Vec<String>`. Both pure, unit-testable.

Both modules get declared in `src-tauri/src/transcription/mod.rs`.

### Changed files

- `src-tauri/src/transcription/engine.rs` — `transcribe_wav_with_callback` gains three small touch points (silence gate pre-call, four new `FullParams` setters, two-layer post-process before returning). Public signature unchanged.

### Untouched

Orchestrator, storage, commands, frontend, `models.rs`, progress events, `run_transcribe` signature.

## Silence gate

### Algorithm

1. Divide 16 kHz mono samples into 20 ms frames (320 samples each). Compute per-frame RMS: `sqrt(mean(sample²))`.
2. Estimate noise floor as the 20th percentile of all frame RMS values. Compute threshold as `max(noise_floor × 4, MIN_THRESHOLD)`.
3. Walk frames with a two-state machine (`Silence` / `Speech`):
   - In `Silence`: transition to `Speech` after ≥ `ATTACK_MS / FRAME_MS` consecutive frames above threshold. Region start = first-above frame's start, minus `PAD_MS` (clamped to 0).
   - In `Speech`: transition to `Silence` after ≥ `RELEASE_MS / FRAME_MS` consecutive frames below threshold. Region end = last-above frame's end, plus `PAD_MS` (clamped to sample length).
4. Merge overlapping or touching regions (after padding, adjacent regions may collide).
5. Drop regions shorter than 200 ms (likely transient clicks, not speech).
6. Concatenate surviving regions into a new `Vec<f32>`.

### Constants

```rust
const FRAME_MS: u32 = 20;                // 320 samples @ 16 kHz
const ATTACK_MS: u32 = 100;              // frames above threshold to open
const RELEASE_MS: u32 = 300;             // frames below threshold to close
const PAD_MS: u32 = 200;                 // pre/post padding around regions
const MIN_REGION_MS: u32 = 200;          // drop regions shorter than this
const NOISE_FLOOR_PERCENTILE: f32 = 0.20;
const THRESHOLD_MULTIPLIER: f32 = 4.0;
const MIN_THRESHOLD: f32 = 0.003;        // absolute floor for digital-silence clips
const MIN_INPUT_MS: u32 = 100;           // below this, bypass gate entirely
```

### Edge cases

- **Entirely silent input** → empty `Vec<f32>`. `transcribe_wav_with_callback` short-circuits and returns an empty transcript.
- **Entirely speech** → returns input approximately unchanged (length within a few hundred ms due to pad/clamp).
- **Shorter than `MIN_INPUT_MS`** → bypass: return input unchanged.
- **No sentence-terminator / all-noise input** → the threshold math still produces a reasonable value because the percentile adapts; degenerate cases (literal constant signal) fall back to `MIN_THRESHOLD` and treat everything as speech.

### Why not Silero / whisper VAD

Silero needs onnxruntime (heavy dep). Whisper.cpp's `set_vad(true)` isn't in `whisper-rs` 0.11's public surface. An energy gate is 50 lines of Rust, testable in isolation, and the quality bar we need is "don't feed 2 minutes of HVAC hum to a decoder" — easily met.

## Whisper parameter tuning

All set in the existing `FullParams` block in `engine.rs`. Added:

```rust
params.set_temperature(0.0);          // first attempt is greedy
params.set_temperature_inc(0.2);      // retry at +0.2 on failure, capped at 1.0
params.set_no_speech_thold(0.6);      // drop segment if P(no_speech) > 0.6
params.set_logprob_thold(-1.0);       // drop segment if avg token logprob < -1.0
params.set_entropy_thold(2.4);        // whisper.cpp default; set explicitly for clarity
params.set_suppress_blank(true);      // don't emit leading/empty segments
```

When a 30-second window produces a segment with high no-speech probability OR low logprob (the signatures of a loop or garbage decode), whisper.cpp retries at temperature += 0.2. If the segment still fails thresholds at temperature = 1.0, it is dropped entirely.

### Preserved

- `set_no_context(true)` — already set, prevents cross-window loop propagation.
- `set_suppress_non_speech_tokens(true)` — already set, suppresses `[Music]`/`[Applause]` tokens at the sampling level.
- `SamplingStrategy::Greedy { best_of: 1 }` — beam search is slower; the fallback loop is a cheaper quality lever.

### Initial prompt (deferred)

An `initial_prompt` like "The following is a meeting transcript in natural English." nudges whisper away from YouTube-isms, but interacts awkwardly with `no_context=true` and is orthogonal to hallucinations-in-silence. Revisit after v1.

## Post-processing

Two layers, applied in order, in `postprocess.rs`. Runs inside `transcribe_wav_with_callback` between segment collection and concatenation.

### Layer A — `collapse_repetitions`

Walks segments in order. For each, normalizes via `normalize(s)` (lowercase, trim, collapse internal whitespace, strip trailing punctuation) and compares against the last-kept segment's normalized form. Counts consecutive equal segments. On the third and subsequent match in a run, drops the segment; first two are kept.

```rust
const REPETITION_THRESHOLD: usize = 3;
```

Two copies of the same segment can be intentional emphasis ("I mean it. I mean it."). Three or more is the unmistakable shape of a whisper lock-in loop.

Handles:

- Interleaved legit speech: `A A A B A A A A → A B A`.
- Near-identical with punctuation drift: `"I think it's different."` and `"I think it's different"` normalize equal.
- Runs across language/speaker boundaries: still collapse on text equality. Acceptable — losing one legit repetition is worth the huge win on loops.

### Layer B — `filter_hallucinations`

Drops segments that match the boilerplate blocklist. Runs after Layer A so a loop of boilerplate phrases is cheap to filter (first-instance match once, not N times).

Match rule: case-insensitive, substring match anywhere in the trimmed segment, **or** full-segment regex match for short-form cases.

**Literal substrings:**

```
"thank you for watching"
"thanks for watching"
"please subscribe"
"like and subscribe"
"don't forget to subscribe"
"see you in the next video"
"see you next time"
"[music]"
"[applause]"
"[laughter]"
"♪"
"ご視聴ありがとうございました"
"字幕 by"
"translated by"
```

**Full-segment regex** (anchored `^…$` against the trimmed segment, case-insensitive):

```
^(you|thank you|thanks)[.!\s]*$     // whole-segment "you" / "Thank you."
^\s*\.*\s*$                         // empty or punctuation-only
^\[?\s*inaudible\s*\]?$             // "[inaudible]" or bare "inaudible"
```

### Ordering

`collapse_repetitions` first, then `filter_hallucinations`, then `join(" ").trim()`. A loop of a blocklist phrase gets reduced to one occurrence, then filtered — rather than running the blocklist match N times.

### Not in v1

- User-editable blocklist (ship when demand emerges).
- Language-aware lists (one global list suffices for observed patterns).
- Mid-segment removal (drop whole segments only, to stay conservative).

## Integration diff for `engine.rs`

The only changes to `transcribe_wav_with_callback`:

```rust
let samples = read_wav_as_f32_mono(wav_path)?;
let samples = silence::gate_silence(&samples, 16_000);
if samples.is_empty() {
    return Ok(String::new());
}

// ... existing context load, state create, params block with 6 new setters ...

state.full(params, &samples) ...;

let n = state.full_n_segments()...;
let mut segments: Vec<String> = (0..n)
    .map(|i| state.full_get_segment_text(i))
    .collect::<Result<_, _>>()
    .map_err(|e| format!("segment text: {e}"))?;
segments = postprocess::collapse_repetitions(&segments);
segments = postprocess::filter_hallucinations(&segments);
Ok(segments.join(" ").trim().to_string())
```

Existing callback boxing, progress emission, cache loading, and params unchanged. Public signature unchanged.

## Testing

### `silence.rs` unit tests

- Empty input → empty output.
- All-zeros input → empty output.
- Synthetic white-noise input (above threshold) → length within `± 2 × PAD_MS` of input.
- Speech + 2 s silence + speech (synthetic) → length ≈ sum of speech regions + padding.
- Below-`MIN_INPUT_MS` clip → bypass, input returned unchanged.
- Deterministic: same input bytes → same output bytes.

### `postprocess.rs` unit tests

`collapse_repetitions`:
- `A A A B → A B`.
- `A A B → A A B` (under threshold).
- `A A A A A A → A`.
- `A B A B A B → A B A B A B` (alternation preserved).
- `"Yes."` and `"yes"` normalize equal.
- Real-world fixture: 23 copies of the iron-rod sentence + 34 copies of "I think it's a little different." → 2 surviving segments (one of each, from first run).

`is_hallucination`:
- Positive: every blocklist literal and regex matches.
- Negative: `"I think we should ship."`, `"Thanks, John."`, `"This is a video about Kubernetes."`, `"[inaudible speech]"`.

`filter_hallucinations`:
- Preserves order, preserves count minus dropped.
- Compound test: the iron-rod + boilerplate sample → 2 segments surviving both layers.

### Whisper param tests

None. Setting thresholds is configuration; meaningless to unit-test without running whisper on real audio.

### Real-audio regression fixture (optional)

One `#[ignore]`d integration test against a short real-audio fixture that reproduces the iron-rod loop, asserting:

- Output contains the looped phrase ≤ 2 times (not 23).
- Output does not contain `"thank you for watching"`.
- Total output length below a sanity bound.

Skipped by default; `cargo test --ignored` runs it locally. If maintaining a fixture proves annoying, the unit tests on `collapse_repetitions` with the pasted sample as a `&str` fixture already catch the specific bug.

### Manual verification

1. Transcribe a recording known to previously produce loop hallucinations.
2. Confirm loops are gone, legit content preserved.
3. Compare before/after Transcribe stage wall time.

## Risks & mitigations

- **Silence gate clips quiet speech.** Mitigated by conservative defaults (threshold ~1% of full scale, 100 ms attack, 200 ms padding). Also mitigated by the percentile-based noise-floor adaptation — quiet-but-above-HVAC speech estimates a low floor and gates lightly. Residual risk remains; manual verification on a test corpus before shipping.
- **Repetition collapser eats legit triple-repeats** ("No. No. No."). Real but rare in meeting content. A ≥3 threshold (not 2) limits this.
- **Blocklist false positives** (e.g., a literal meeting transcript of a YouTuber's script). Vanishingly rare in the target use case. If it happens, the pattern list is trivially editable in code.
- **Whisper thresholds eat good segments.** `no_speech_thold=0.6` and `logprob_thold=-1.0` match whisper.cpp's own widely-used defaults for long-form transcription; false positives are unlikely. The fallback loop retries before dropping, further reducing risk.
- **Silence gate + whisper fallback together drop too much.** If a recording's quiet sections are important content rather than noise, we'd lose them. Mitigated by the real-audio regression check; if needed, expose a per-recording "aggressive filtering" toggle as a follow-up.
