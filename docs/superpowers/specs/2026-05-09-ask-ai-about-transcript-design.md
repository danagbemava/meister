# Ask AI about transcript — design

Date: 2026-05-09
Status: design, awaiting review

## Goal

Let users ask natural-language questions about a generated transcript and get answers backed by verbatim citations that link back into the transcript pane. Example query: *"what was said about project financials?"* The model reads the transcript and replies with prose plus quoted snippets the user can click to jump to.

## Scope

In scope:

- A right-side **Ask** panel in the recording view.
- Single-shot queries by default; optional follow-ups within a thread.
- Verbatim quoted citations that scroll/highlight the transcript pane on click.
- Two backend strategies: single-shot (fits in context) and map-shortlist + reduce (long transcripts).
- Streaming answers with cancellation parity to the format pipeline.
- Session-only persistence (in-memory, per-recording, lost on quit).

Out of scope (explicit non-goals):

- Persisting threads across app sessions (no schema/migration).
- Embedding-based retrieval / RAG. Kept in mind as a future option if (A) and (B) prove insufficient at scale, but not built now.
- Editing transcripts after asking (offset-staleness not handled).
- Asking against the formatted output (transcript is the only source of truth).
- Multi-recording asks ("across all my recordings").
- Per-question model override — Ask reuses the global model setting.
- Localized prompts — instruction prompt stays English; the `language` setting only influences model output if the model honors it.

## User experience

### Opening the panel

A new "Ask" button (Icon: `chat.bubble`) sits in the `RecordingView` toolbar. It is enabled only once the recording has a transcript. Clicking toggles a fixed-width (~360px) right-side drawer; ⌘K is the keyboard shortcut. The existing `OutputPreview` reflows to make room.

### Empty state

The first time a recording's panel opens it shows a short hint — *"Ask anything about this transcript — e.g. what was said about project financials?"* — and focuses the input.

### Asking

- Single-line input + send button. Enter sends; Shift+Enter inserts a newline.
- While streaming the send button becomes a Stop button that cancels via the existing `cancellation.rs` token plumbing.
- A small phase indicator reflects backend state: *"Searching transcript…"* during the map pass (path B only), then *"Answering…"* during the reduce or single-shot pass.

### Answer rendering

- The user's question renders as a muted bubble at the top of the entry.
- The assistant's answer streams in token-by-token below.
- Quoted citations render as inline pill-style links: `"...we delayed the Q3 budget review..."`.
- Click → switches `OutputPreview` to the **Transcript** tab, scrolls the cited offset into view, and flashes a 1.2-second highlight on the matched range.
- Citations whose quote cannot be located in the transcript still render as a quote but are non-clickable, with a tooltip: *"couldn't locate in transcript."*

### Follow-up

- Each answer has a "Follow up" affordance. Clicking it expands the entry into a thread; subsequent turns indent under it and prior turns are sent as conversation history to Ollama.
- Submitting a fresh top-level question always starts a new thread (no implicit context bleed between questions).

### Thread management

- Each entry has a "×" to dismiss it.
- A "Clear all" affordance in the panel header empties the list for the current recording.

### Streaming and citations

Answers stream live. To keep the streamed prose clean, the model is instructed to emit citations only after the answer in a `--- citations ---` block. Citations are parsed once the stream completes; until then no citation pills render.

## Architecture

### Frontend (React / TypeScript)

- **`AskPanel.tsx`** — the right-side drawer. Owns the input, the in-flight indicator, and renders the list of `AskThread`s for the active recording.
- **`useAsk(recordingId)` hook** — facade over a small per-recording store. Backed by `Map<recordingId, AskThread[]>` held at `App.tsx` level (or an equivalent module-level store). Switching recordings preserves each thread in memory; quitting the app drops them.
- **`OutputPreview`** gains an optional `highlightRange?: { start: number; end: number }` prop. When the user clicks a citation, the panel forces the preview's tab to **Transcript**, scrolls to the offset, and triggers a transient highlight.

### Backend (Rust / Tauri)

New module `src-tauri/src/ask/` with sub-modules:

- `mod.rs` — exposes the high-level `ask_query` entry point.
- `strategy.rs` — `pick_strategy(transcript, model_ctx_budget)`.
- `single_shot.rs` — path A.
- `map_shortlist.rs` — path B.
- `citations.rs` — verbatim and whitespace-tolerant quote matching.
- `types.rs` — serde types shared with the frontend.

Reuses existing infrastructure:

- Ollama HTTP plumbing in `commands.rs` (`ollama_generate_stream`).
- Chunking helper used by the format pipeline (single source of truth for chunk size and overlap).
- Cancellation token plumbing in `cancellation.rs`.
- Inactivity timeout at `commands.rs:500` (already handles wedged streams).

### Tauri command surface

- `ask_query(recording_id, question, history) -> AskAnswer` — returns the final answer and parsed citations. `history` is the prior `Vec<AskTurnIn>` (may be empty for a new thread).
- `cancel_ask(recording_id)` — flips the cancellation token registered for the in-flight query.

### Events

`ask-progress` is emitted to the frontend during the query:

```ts
type AskProgressPayload =
  | { phase: "search"; chunk: { index: number; total: number } }
  | { phase: "answer"; chars: number }
  | { phase: "done" };
```

## Data flow

```
User types Q in AskPanel
  → invoke("ask_query", { recordingId, question, history })
  → Rust loads transcript from storage, runs pick_strategy
  → if single-shot: one streaming Ollama call
    if map-shortlist+reduce: relevance pass per chunk, then one streaming reduce call
  → streams "ask-progress" events to the frontend
  → returns AskAnswer { text, citations: [{ quote, start, end }] }
  → AskPanel appends the assistant turn; "Follow up" reuses the thread as history
```

## Backend strategy details

### `pick_strategy`

Decision is based on `transcript_chars` and a budget derived from the model's context window. Concretely: `budget_chars = ctx_window_tokens * approx_chars_per_token * 0.7`, leaving headroom for prompt overhead, history, and the streamed answer. `ctx_window_tokens` is read from the same place the format pipeline reads it; if the format pipeline currently uses a hardcoded default, we share that constant rather than duplicating it. Below the budget → path A. Above → path B. The threshold is a single character-count comparison with no hysteresis.

### Path A — single-shot

One streaming call to `/api/generate` via `ollama_generate_stream`.

Instruction prompt (sketch): *"Answer the question using only the transcript below. Quote verbatim from the transcript for any factual claim. After the answer, output a `--- citations ---` block listing each quoted snippet on its own line, exactly as it appears in the transcript."*

Tokens stream to the frontend via `ask-progress { phase: "answer", chars }`.

### Path B — map-shortlist + reduce

1. Split the transcript using the format pipeline's chunking helper (same chunk size and overlap).
2. **Keyword prefilter.** Score each chunk by lowercase-token overlap with the question (ignoring stopwords). Take the top `K = 5` chunks with score > 0. If no chunk overlaps the question at all, fall back to passing every chunk through the map pass. This step is a pure-Rust word-set intersection — no LLM call — and is essentially free at meeting-length scales.
3. **Map pass.** For each chunk in the prefilter shortlist, a small JSON-mode call: `{"relevant": bool, "gist": "..."}`. Run with bounded concurrency `k = 3` (via `futures::stream::buffer_unordered`). With Ollama's default `OLLAMA_NUM_PARALLEL=1` the requests still serialize at the server, but the futures stay primed; with `OLLAMA_NUM_PARALLEL ≥ 3` the wall time drops near-linearly. Emit `ask-progress { phase: "search", chunk: { index, total } }` per chunk where `total` reflects the shortlist size (not the raw chunk count) so the UI counter matches actual calls.
4. **Reduce pass.** Concatenate the relevant chunks with `[chunk N]` separators (N is the original chunk index, not the shortlist index, so the model sees consistent labels), then call `/api/generate` with the same instruction prompt as path A. Stream output.
5. **Zero relevant chunks** → return an explicit *"couldn't find anything about that in the transcript"* answer without a second call. Cheap, and avoids hallucinated answers on out-of-scope questions.

The keyword-prefilter + bounded-concurrency design replaces the original "sequential map over every chunk" approach. Rationale: that approach scaled linearly with transcript length, hitting 2+ minutes on a 10-chunk transcript with default Ollama. The prefilter caps LLM calls at K + 1 regardless of transcript length, and concurrency turns idle wait into parallel inference whenever the user has tuned Ollama. Quality risk (paraphrased questions whose keywords don't overlap the transcript) is mitigated by the fallback-to-all-chunks branch when the shortlist is empty.

### Cancellation

A cancellation token is registered against `recording_id` when `ask_query` starts. The Stop button invokes `cancel_ask`, which trips the token. Mirroring the recent format-pipeline fix (`de1403c`), the streaming HTTP body read checks the token during awaits, not only between bytes.

### Citation parsing

After stream completion the backend splits the response on the `--- citations ---` marker and parses each quoted line. Each is matched against the transcript:

1. Exact substring search (first occurrence wins).
2. Fallback: whitespace- and punctuation-tolerant search (collapse runs of whitespace, ignore curly-vs-straight quotes).
3. No match → returned with `start: None, end: None`.

The frontend uses the returned offsets directly against the same transcript string `OutputPreview` renders — no re-tokenization.

## Data shapes

### Frontend (TypeScript)

```ts
export interface AskCitation {
  quote: string;
  // Byte offsets into the transcript text. null if the model hallucinated
  // a quote that the matcher couldn't locate.
  start: number | null;
  end: number | null;
}

export interface AskTurn {
  role: "user" | "assistant";
  content: string;
  citations?: AskCitation[]; // assistant turns only
}

export interface AskThread {
  id: string;             // uuid, stable while the panel is open
  turns: AskTurn[];
  pending: boolean;       // true while a query is streaming
  streamingText?: string; // partial assistant content during stream
}
```

Per-recording state: `Map<recordingId, AskThread[]>`. Never written to SQLite, never to localStorage.

### Backend (Rust)

```rust
#[derive(Serialize)]
pub struct AskCitation {
    quote: String,
    start: Option<usize>,
    end: Option<usize>,
}

#[derive(Serialize)]
pub struct AskAnswer {
    text: String,
    citations: Vec<AskCitation>,
}

#[derive(Deserialize)]
pub struct AskTurnIn {
    role: String,    // "user" | "assistant"
    content: String,
}
```

`ask_query(recording_id: String, question: String, history: Vec<AskTurnIn>) -> Result<AskAnswer, String>`.

## Error handling

- **Ollama unreachable / connection error.** Inline error in the panel: *"Couldn't reach the model — is Ollama running?"* The user's question stays in place so they can retry. Wording matches the format-pipeline style.
- **Model evicted / inactivity timeout.** Handled by the existing inactivity timeout in `commands.rs:500`; surface as the same inline error.
- **User cancellation.** The user's question stays in the thread; the partial assistant turn is dropped; the panel returns to idle.
- **Path B, zero relevant chunks.** Explicit *"couldn't find anything about that in the transcript"* — not a model call.
- **Missing or malformed citation block.** The answer still renders; citations array is empty; no error shown.
- **Citation matching failure.** Pill renders unclickable with a tooltip; not treated as an error.

## Edge cases

- **No transcript yet.** Ask button disabled (tooltip: *"transcribe first"*).
- **Empty question (whitespace only).** Send disabled.
- **Switching recordings mid-stream.** In-flight query cancelled; the thread is preserved in the per-recording map and resumes visually if the user returns.
- **Very long answers.** Streaming UI auto-scrolls the panel; a user-initiated scroll-up pins it so reading isn't yanked away.
- **Transcript at the path-A/B boundary.** Decision is deterministic (single character-count threshold) — no flapping.

## Testing

### Rust

- `ask/citations.rs`: unit tests for verbatim match, whitespace-tolerant match, no-match → `None`, multiple occurrences (return first), quote longer than transcript, empty quote.
- `ask/strategy.rs`: `pick_strategy` table tests at boundary lengths.
- `ask/map_shortlist.rs`: with mocked Ollama responses, assert only relevant chunks reach the reduce pass, and that zero-relevant short-circuits without a reduce call.
- Cancellation: integration test flipping the token mid-stream returns promptly and does not leak the HTTP connection.

### Frontend

- `useAsk` hook: new-thread, follow-up-appends-to-thread, switch-recording preserves both threads in memory, switch-back restores.
- `AskPanel.test.tsx`: empty state, send disabled on empty input, citation click forces Transcript tab + scroll, unmatched citation renders as non-clickable.
- Manual E2E checklist (in line with how other features are validated):
  1. Record → transcribe → ask → click citation → verify scroll & highlight.
  2. Ask follow-up.
  3. Cancel mid-stream.
  4. Quit and reopen → thread is gone (confirms session-only).

## Files affected

New files:

- `src/components/AskPanel.tsx`
- `src/hooks/useAsk.ts` (or co-located in `AskPanel.tsx` if small enough)
- `src/types/ask.ts`
- `src-tauri/src/ask/mod.rs`
- `src-tauri/src/ask/strategy.rs`
- `src-tauri/src/ask/single_shot.rs`
- `src-tauri/src/ask/map_shortlist.rs`
- `src-tauri/src/ask/citations.rs`
- `src-tauri/src/ask/types.rs`

Modified:

- `src/components/RecordingView.tsx` — Ask toolbar button, panel mount, ⌘K binding.
- `src/components/OutputPreview.tsx` — `highlightRange` prop, scroll-and-flash behavior.
- `src-tauri/src/lib.rs` — module registration.
- `src-tauri/src/commands.rs` — register `ask_query`, `cancel_ask` Tauri commands.

No DB schema changes.

## Future considerations

- **Persisted threads.** Add `ask_threads` table keyed by `recording_id`; migrate `useAsk` to write-through. Out of scope here, but the in-memory shape (`AskThread`) is designed to be persistable as-is.
- **RAG / embedding retrieval.** If transcripts grow well beyond what map-shortlist handles cleanly, swap path B for embedding-based chunk retrieval. Self-contained behind `pick_strategy` — no UI changes.
- **Cross-recording asks.** Layer on top of (RAG) once ingestion is in place.
