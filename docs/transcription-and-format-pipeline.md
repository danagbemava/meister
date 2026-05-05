# Transcription + Format Pipeline — Reference

Quick reference for the work shipped April–May 2026 to speed up the
formatter, fix transcription accuracy, and harden against Ollama hangs.
Use this when tuning, debugging, or onboarding to the pipeline.

## Pipeline shape

```
Recording sources → Extract → Transcribe → Format → Save
                  (per-WAV)  (per-source) (map-reduce LLM) (user dialog)
```

- **Extract** (`commands.rs::run_extract`) — one ffmpeg invocation per
  source, producing one `audio-{i}.wav` per source. Sources are kept
  separate so whisper transcribes each independently.
- **Transcribe** (`transcription/engine.rs::transcribe_wav_with_callback`)
  — whisper.cpp via whisper-rs, called per source. Per-source segments
  are joined with `\n\n`. Pipeline is silence-gate → whisper → collapse
  → filter (see "Layers" below).
- **Format** (`commands.rs::run_format_core`) — Ollama LLM dispatcher:
  cache lookup → preprocess → branch on token count → single-call OR
  map-reduce → cache write. Map-reduce fires up to 3 parallel chunk
  calls via `buffer_unordered(3)`.
- **Save** — user-driven file dialog flow.

## Pipeline layers (transcription)

Inside `transcribe_wav_with_callback`, in order:

1. **Read WAV** (16 kHz mono f32).
2. **Silence gate** (`transcription/silence.rs::gate_silence`).
   Conservative: drops only stretches ≥ `MIN_SILENCE_MS` (3 s) of audio
   below `ABSOLUTE_THRESHOLD = 0.003` RMS. Keeps `PAD_MS = 500` of
   silence on each end of a dropped region. Short pauses pass through
   untouched.
   - **Bypass**: `MEISTER_NO_SILENCE_GATE=1` env var.
3. **Whisper** with these `FullParams`:
   - `no_context = true`, `suppress_non_speech_tokens = true`
   - `temperature = 0.0`, `temperature_inc = 0.2`
   - `no_speech_thold = 0.6`, `logprob_thold = -1.0`, `entropy_thold = 2.4`
   - `suppress_blank = true`
4. **collapse_repetitions** (`transcription/postprocess.rs`). Two passes:
   - **Pass 1**: maximal runs of identical normalized segments → keep first
     only when `run_len ≥ REPETITION_THRESHOLD = 3`.
   - **Pass 2**: sliding window of the last `RECENT_WINDOW = 10` segments;
     drop any segment whose normalized form already appears
     `REPETITION_THRESHOLD` times in the window. Catches period-2+ loops
     (A-B-A-B-…) that pass 1 misses.
5. **filter_hallucinations** — drops segments matching a literal blocklist
   (YouTube boilerplate: "thank you for watching", "[music]", etc.) or
   anchored regex (whole-segment "you" / "thank you" / "[inaudible]" /
   empty-or-punctuation-only).

## Pipeline layers (format)

`run_format_core(transcript, template, ollama_url, model, cache_dir, on_progress)`:

1. **Cache lookup**: `<cache_dir>/<sha256(transcript ‖ template ‖ model)>.md`.
   Hit → emit one progress event with `done: true`, return cached text.
2. **preprocess** (`commands.rs::preprocess`) — strip filler ("uh"/"um"/
   "uhm"/"er"/"erm"/"hmm"/"ah" between word boundaries), collapse 3+
   stutters of the same word to 1, normalize em-dash → `-`, ellipsis →
   `...`, collapse whitespace while preserving paragraph breaks.
3. **Branch** on `approx_tokens(cleaned) < THRESHOLD = 4000`:
   - **Single-call path**: one streamed Ollama call.
   - **Map-reduce path**: `split_into_chunks` (target 3000 tokens, 200-
     token overlap, sentence-boundary preference, UTF-8-safe slicing) →
     parallel `map_pass` per chunk via `buffer_unordered(3)` →
     `stitch_notes` → single `reduce_pass`.
4. **Cache write** (best-effort; failure logs to stderr, doesn't fail
   the stage).

### Ollama request parameters (every call)

- `keep_alive = "30m"` — keeps the model warm between calls.
- `num_predict = -1` — no output cap.
- `num_ctx`: 8192 (single-call), 6144 (map-pass), 12288 (reduce-pass).

### Timeouts (every Ollama request)

- `REQUEST_TIMEOUT_SECS = 480` — outer wall on the whole request.
- `INACTIVITY_TIMEOUT_SECS = 120` — abort if no streaming bytes arrive
  for 2 min. Surfaces a clear error in the UI pointing at the recovery
  command.

## Diagnostic logging

Every call to `transcribe_wav_with_callback` emits stderr lines tagged
`[transcribe:audio-N.wav]`:

```
[transcribe:audio-0.wav] silence gate: 12345678 → 11000000 samples (89.1% kept)
[transcribe:audio-0.wav] raw whisper segments: 412 (top-5 lengths: [109, 91, 91, 90, 87])
[transcribe:audio-0.wav]   raw[0] len=58: "I think the whole of this year, it was started a year ago,"
[transcribe:audio-0.wav]   raw[1] len=72: "as a president, I was a program, a director, and so on."
   ... up to 20 raw segments ...
[transcribe:audio-0.wav] collapse_repetitions: 412 → 198 (dropped 214)
[transcribe:audio-0.wav] filter_hallucinations: 198 → 194 (dropped 4); first drops:
[transcribe:audio-0.wav]   filtered: "[Music]"
```

This is intentional and useful — leave it in until we have a real need
to silence it.

## Known issues / follow-ups

- **Within-segment repetition** (one whisper segment containing the same
  phrase several times internally) is invisible to `collapse_repetitions`,
  which operates at segment boundaries. A residual ~9 lines of "For
  people who are interested in that" survived a real run because of this.
  Acceptable for now; if it bites again, fix is segment-internal phrase
  detection (Levenshtein or word-pair n-gram counting).
- **Native macOS look** — Tauri webview UI doesn't pass as native.
  Separate brainstorm queued, not started. (Cargo.toml has
  `macos-private-api` + objc2/AppKit/Foundation deps in place — likely
  the start of that work.)
- **Format cache eviction**: none in v1. `<app_data>/format-cache/*.md`
  grows forever. Each entry is small (≤ a few hundred KB) but worth a
  size-capped LRU sweep eventually.
- **Stuck-format observability**: when Ollama wedges and we hit the 120 s
  inactivity timeout, the stage fails with a clear error. But there's no
  per-chunk progress *within* a chunk's streaming — the UI shows
  `"Analyzing chunk 3/3 · X chars streamed"` which moves but doesn't
  estimate completion. Lower priority.

## Recovery commands

**Ollama wedged / model unloaded mid-request:**

```bash
pkill -9 ollama
OLLAMA_NUM_PARALLEL=3 ollama serve
```

Then in Meister: `Start over` on the affected recording → `Run`. Extract
and Transcribe outputs are preserved (orchestrator re-uses completed
stages); only Format re-runs.

**Silence gate suspected of dropping legit speech (re-test):**

```bash
MEISTER_NO_SILENCE_GATE=1 npm run tauri dev
```

Compare the resulting transcript against a normal-mode run; significant
content differences imply over-gating. Tune constants in
`silence.rs` (`ABSOLUTE_THRESHOLD`, `MIN_SILENCE_MS`, `PAD_MS`).

**Swift runtime can't link (libswift_Concurrency.dylib not found):**

The build script `src-tauri/build.rs` resolves Xcode's
`swift-5.5/macosx` back-deployment lib via `xcode-select -p` and
emits an rpath. If this fails, either reinstall Xcode CLT
(`xcode-select --install`) or update the path in `find_swift_runtime_dir`
in `build.rs`.

## Tunable constants — where they live

| Constant | File | Default |
|---|---|---|
| `THRESHOLD` (single-vs-map cutoff) | `commands.rs` | 4000 tokens |
| `CHUNK_TARGET` | `commands.rs` | 3000 tokens |
| `CHUNK_OVERLAP` | `commands.rs` | 200 tokens |
| `REQUEST_TIMEOUT_SECS` | `commands.rs` | 480 |
| `INACTIVITY_TIMEOUT_SECS` | `commands.rs` | 120 |
| `keep_alive` | `commands.rs` | "30m" |
| `num_ctx` (per phase) | `commands.rs` | 8192/6144/12288 |
| `MIN_SILENCE_MS` | `silence.rs` | 3000 |
| `PAD_MS` (silence gate) | `silence.rs` | 500 |
| `ABSOLUTE_THRESHOLD` (RMS) | `silence.rs` | 0.003 |
| `REPETITION_THRESHOLD` | `postprocess.rs` | 3 |
| `RECENT_WINDOW` (period-N detector) | `postprocess.rs` | 10 |

## Specs and plans

- `docs/superpowers/specs/2026-04-21-format-speedup-design.md` +
  `docs/superpowers/plans/2026-04-21-format-speedup.md`
- `docs/superpowers/specs/2026-04-24-transcription-speed-and-hallucinations-design.md` +
  `docs/superpowers/plans/2026-04-24-transcription-speed-and-hallucinations.md`

## Map of recent commits (high-level)

- Format speedup: `e08056e` (preprocess) → `7c3b9aa` (dispatcher wired)
  → `608c41b`/`4421f97`/`b9fc113`/`e6158fc` (integration tests) →
  `ee9222f` (frontend chunk-aware progress) → `ee097e1` (SETUP doc).
- Per-source extract refactor: `84342d2` (commands + orchestrator +
  engine; previously split across `6a35849`+`2e31b59` before history
  cleanup).
- Mic-capture / system-audio merges: `d43d7cb`, `1883d16`.
- Swift runtime rpath: `94ae899`.
- Frontend "Running" state from backend: `5c978e7`.
- Map-phase streamed-chars indicator: `7d03c88`.
- Conservative silence gate: `c635dfe`.
- Period-2 loop catcher: `ab039bb`.
- Diagnostic stderr logging: `54b1d2e`.
- Ollama inactivity timeout: `d2e06c8`.
