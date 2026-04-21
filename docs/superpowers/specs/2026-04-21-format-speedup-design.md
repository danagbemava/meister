# Format Speedup — Design

**Status:** Draft
**Date:** 2026-04-21

## Problem

With the whisper.cpp swap, transcription is no longer the slowest stage of a run. The Format stage — a single streamed Ollama call that ingests the entire transcript plus the user template and generates a full formatted document — is now the bottleneck on long recordings. A single call scales linearly with transcript length for both prefill (prompt processing) and decode (token generation), and the call is inherently serial.

Users are willing to trade a small amount of formatting quality for a substantial wall-time reduction.

## Goals

- Reduce end-to-end Format stage wall time, especially on long transcripts (30+ min).
- Preserve the product's core premise that any user-supplied template works.
- Keep the existing stage model (Extract / Transcribe / Format / Save) and resume semantics intact — no schema migration.
- Keep the streaming-progress UX; users should still see the document being built in real time.

## Non-goals

- **Exposing parallelism as a user-facing setting.** A sensible default ships; tuning can be added later if demand emerges.
- **Cloud / remote-API formatter path.** Out of scope — local Ollama stays the only formatter backend.
- **Per-chunk resume.** The Format stage remains atomic; a mid-run failure retries the whole stage.
- **Semantic / speaker-aware chunking.** Whisper output is a flat text blob; chunking is text-based.
- **Template-shape–specific extraction schemas** (meetings vs. lectures). Every template goes through the same free-form notes pipeline.
- **Cache eviction policy.** v1 writes cache entries and never prunes.

## Approach

Two orthogonal changes that compound:

1. **Easy wins on every call** — keep the Ollama model warm (`keep_alive: "30m"`), set explicit `num_ctx` / `num_predict`, strip conservative filler from the transcript, and cache final outputs by `(transcript, template, model)` hash.
2. **Map-reduce on long transcripts** — split into chunks, fire a parallel notes-extraction pass across chunks, then run a single reduce pass that formats the consolidated notes against the user's template.

The intermediate representation between map and reduce is **free-form markdown notes**, chosen for template-agnostic robustness: any user template (meeting, lecture, interview, …) consumes the same notes shape.

## Architecture

### Data flow

```
run_format(transcript, template, model, ollama_url)
├── cache lookup on hash(transcript, template, model) → hit: return cached, emit done
├── preprocess(transcript) → cleaned text
├── if tokens(cleaned) < THRESHOLD:
│     └── single-call path: one streamed generate, current behavior
└── else:
      ├── split_into_chunks(cleaned) → [chunk_1 … chunk_N]
      ├── map phase: parallel (buffer_unordered=3) → notes_i per chunk
      ├── stitch: ordered concat with "### Chunk i of N\n" headers
      └── reduce phase: one streamed generate(stitched_notes, template) → final
└── cache write on success
```

The Format stage remains a single stage from the orchestrator and storage layer's perspective. All new logic lives inside `run_format` (and its helper functions) in `src-tauri/src/commands.rs`. No changes to `storage/types.rs`, no schema migration, no new stages, no new Tauri commands.

### Surface changes

- `commands.rs`: `run_format` rewritten as a dispatcher over three internal helpers — `preprocess`, `split_into_chunks`, `map_pass`, `reduce_pass`.
- `commands.rs`: `FormatProgress` gains `phase` and optional `chunk` fields (see UI section).
- `SETUP.md`: document `OLLAMA_NUM_PARALLEL=3` so users' Ollama server actually runs map calls concurrently.
- Frontend stage-card render: branch on `phase` to show chunk progress vs. single streaming view.

Everything else — orchestrator, schema, Save, resume — is unchanged.

## Chunking strategy

### Tokenization

No real tokenizer. Approximate with `chars / 4 ≈ tokens`. Good enough for picking split points.

### Thresholds (approx-tokens)

- `THRESHOLD = 4000` — below this, single-call path.
- `CHUNK_TARGET = 3000` — target chunk size above threshold.
- `CHUNK_OVERLAP = 200` — trailing-overlap prefix on each chunk after the first.

### Boundary selection

For each chunk after the first:

1. Target split point at `CHUNK_TARGET` tokens (~12k chars) from the previous boundary.
2. Search a window of ±500 chars around the target for the last `.`, `?`, or `!` followed by whitespace — split there.
3. If no sentence boundary in the window, fall back to the last whitespace in the window.
4. Prepend the previous chunk's final `CHUNK_OVERLAP` tokens of text to the new chunk.

### Expected chunk counts

- 30-min transcript (~5–8k tokens) → 2–3 chunks.
- 90-min transcript (~15–25k tokens) → 5–9 chunks.

With `buffer_unordered(3)`, a 9-chunk map phase runs in ~3 sequential rounds rather than one long single call.

### Prompts

- **Map-pass prompt** — fixed system text: "extract concise notes from this segment of a longer transcript; focus on who said what, decisions, action items, topics, specifics, quotes worth preserving; do not invent; this is part N of M." No user template.
- **Reduce-pass prompt** — the existing template-formatter prompt (`build_prompt` in `commands.rs`), unchanged, but with consolidated notes substituted for the raw transcript.

## Ollama interaction

### Per-request parameters

Added to every generate call:

- `keep_alive: "30m"` — prevents the 5-min default idle unload between stages and runs.
- `num_predict: -1` — no output cap; task length is bounded by content.
- `num_ctx`:
  - Single-call path: `8192` — up to ~4000 tokens of transcript + prompt + template + generated output fit comfortably.
  - Map-pass: `6144` — chunk up to ~3700 tokens (target + boundary search slack) + prompt + ~500-token notes output.
  - Reduce-pass: `12288` — stitched notes at 9 chunks × ~500 tokens = ~4500 tokens, plus template, prompt, and generated output.

Explicit `num_ctx` avoids Ollama over-allocating KV cache to the model's max context.

### Parallelism

Map-pass futures are fired through `futures::stream::iter(chunks).map(map_pass).buffer_unordered(3)`, then collected, sorted by chunk index, and stitched.

Ollama serves parallel requests only when `OLLAMA_NUM_PARALLEL ≥ 3` on the server side. At `1` (current default), map calls serialize without error — still correct, but no speedup. `SETUP.md` documents setting this env var.

The app does not detect, configure, or require the server setting; it simply submits the requests and lets Ollama do its thing.

### Timeout

Keep the current `600s` per-request timeout. Map calls will be much shorter in practice; the reduce call and the single-call path are both within this bound.

### Cancellation

Honor the existing cancel registry. In-flight map futures abort cleanly when the outer future is dropped by the orchestrator (reqwest drops the connection; Ollama cancels the generate).

## Preprocessing (filler stripping)

Pure function, applied unconditionally before both paths. Conservative — prefers leaving filler in place over mangling real content.

1. Normalize whitespace in two steps: runs of two or more newlines → exactly `\n\n` (paragraph break preserved); all other runs of whitespace (including single newlines) → a single space.
2. Remove standalone filler tokens between word boundaries: `uh`, `um`, `uhm`, `er`, `erm`, `hmm`, `ah` (case-insensitive). Only standalone — must not touch `Uber`, `umbrella`, etc.
3. Collapse stutters: `\b(\w+)([,\s]+\1\b){2,}` → `\1` (three or more repeats of the same word collapse to one; two repeats stay as emphasis).
4. Normalize em-dash and ellipsis noise: `—` → `-`, `…` → `...`.

Do **not** strip "you know", "I mean", "like", "basically" — too much meaning risk.

Expected transcript reduction: **5–15%** on conversational content. The meaningful wall-time savings come from map-reduce, not from this.

## Output cache

- **Key**: `blake3(transcript_raw || 0x00 || template_content || 0x00 || model)` → hex.
- **Location**: `<app_data_dir>/format-cache/<hex>.md`.
- **Lookup**: first step of `run_format`, before preprocessing. Hit → read file, emit a single `FormatProgress { done: true }` event, return.
- **Write**: after successful completion of either path, write final output.
- **Invalidation**: implicit via the key; any change to transcript, template, or model misses.
- **Eviction**: none in v1. Each entry is ≤ a few hundred KB.

The cache's primary value is making re-runs cheap during development iteration and after non-invalidating Save stage tweaks. First-run on a fresh recording always misses — which is fine, because that's exactly where the map-reduce speedup lands.

## Progress & UI

### Event shape

`format-progress` remains the single event channel. Payload widens:

```rust
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FormatProgress {
    phase: FormatPhase,              // "single" | "map" | "reduce"
    chunk: Option<ChunkInfo>,        // Some(..) only during "map"
    chars: usize,                    // streaming char count of current phase output
    done: bool,                      // true only on final completion of the stage
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ChunkInfo {
    index: usize,                    // 0-based
    total: usize,
    completed: usize,                // chunks that have finished map-pass
}
```

### Emission

- `phase: "single"` — current behavior; stream `chars`, `done: true` at end.
- `phase: "map"` — on each map-pass token emit `{ phase: "map", chunk, chars: sum-across-chunks, done: false }`. Increment `completed` whenever a chunk finishes. `done` stays false through the whole map phase.
- `phase: "reduce"` — reset `chars` to 0, stream the reduce output. Only this phase's final event sets `done: true`.

### Frontend (`RecordingView` stage card)

Widen the `FormatProgress` TS type; branch the render:

- `phase: "single"` — unchanged.
- `phase: "map"` — show `"Analyzing chunk {completed+1}/{total}…"` with a sub-bar at `completed / total`.
- `phase: "reduce"` — show `"Composing output…"` plus the streaming char counter.

No new IPC surface, no new commands.

## Testing

### Rust unit tests (in `commands.rs` `#[cfg(test)]`)

- `preprocess`:
  - Strips `um`/`uh` between word boundaries; preserves `Uber` / `umbrella`.
  - Collapses 3+ stutters to 1; preserves 2 repeats.
  - Normalizes whitespace; preserves paragraph breaks.
  - Idempotent: `preprocess(preprocess(x)) == preprocess(x)`.
- `split_into_chunks`:
  - Short input → 1 chunk, unchanged content.
  - Long input → N chunks, each within `CHUNK_TARGET` + overlap slack.
  - Split lands after `.` / `?` / `!` within the search window when one exists.
  - Falls back to whitespace on wall-of-text input.
  - Overlap: chunk *i*+1 starts with the last ~`CHUNK_OVERLAP` tokens of chunk *i*.
  - Concatenated chunks (minus overlaps) reconstruct the input.
- `cache_key`:
  - Different transcripts → different keys.
  - Same inputs → stable keys across calls.
  - Different model names → different keys.

### Integration tests (`src-tauri/tests/format.rs`)

A tiny in-process mock Ollama server (axum or wiremock) speaks `/api/generate` streaming NDJSON on a random port. Tests pass its URL via the existing `ollama_url` parameter.

- Single-call path: short transcript; assert exactly one upstream call; output echoes mock.
- Map-reduce path: long transcript; assert `N+1` upstream calls (N map + 1 reduce); assert concurrency ≤ 3 by observing request overlap; assert final output comes from the reduce call.
- Cache hit: run twice with identical inputs; assert zero upstream calls on the second run.
- Cache miss on model change: run; change model; run. Both issue upstream calls.
- Event ordering: for a 3-chunk run, observed `format-progress` events have monotonically non-decreasing `chunk.completed` during map phase, reduce phase events have `phase: "reduce"`, final event has `done: true`.

### Regressions

Run the full existing suite. `orchestrator.rs`'s `format_invalid_on_template_change` covers Format stage invalidation; no changes needed, but confirm it still passes.

### Manual verification

No automated tests hit real Ollama. The implementation plan's final task is a manual run on a long real recording: launch the app, run a 60+ min recording end-to-end, confirm the speedup and that the formatted output quality is acceptable.

## Risks & mitigations

- **Chunk boundaries split an important statement** — mitigated by sentence-aware boundary search + 200-token overlap. Not eliminated; acceptable in exchange for the speedup.
- **Reduce pass can't stitch because notes lack cross-chunk context** — the map-pass prompt explicitly tells the model "this is part N of M," and we concatenate notes with chunk headers. If the reduce output ends up choppy, the next lever is enriching the map-pass prompt (e.g., "preserve cross-references to earlier or later segments") rather than abandoning the architecture.
- **Ollama server at `OLLAMA_NUM_PARALLEL=1`** — map calls serialize, speedup disappears. Correct output, wrong performance. Mitigated by `SETUP.md` documentation. A future task could query the server's config on startup and surface a warning; out of scope for v1.
- **Filler stripping removes meaningful content** — mitigated by the conservative rule set; only high-confidence noise patterns are stripped, and even those only at word boundaries. Unit tests pin the behavior.
- **Cache grows unbounded** — acceptable for v1 given small entry sizes. A size-capped LRU sweep is straightforward to add when it matters.
