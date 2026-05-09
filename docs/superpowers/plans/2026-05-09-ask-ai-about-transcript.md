# Ask AI About Transcript — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a per-recording side panel that lets users ask natural-language questions about a generated transcript and get streaming answers with clickable verbatim citations.

**Architecture:** New `src-tauri/src/ask/` module exposes a single Tauri command `ask_query` that picks between (A) one streaming Ollama call when the transcript fits the context window and (B) a map-shortlist + reduce pass when it doesn't. Citations are emitted by the model as a trailing block, parsed in Rust, matched verbatim against the transcript. State on the frontend is session-only — a `Map<recordingId, AskThread[]>` held in a module-level store; nothing is persisted.

**Tech Stack:** Rust + Tauri 2 backend (reqwest streaming, axum mock harness for tests), React + TypeScript frontend (Vitest + Testing Library, Tauri `invoke`), reuses existing Ollama plumbing, `cancellation::CancelToken`, and the format pipeline's `preprocess` / `split_into_chunks` helpers from `commands.rs`.

**Spec:** `docs/superpowers/specs/2026-05-09-ask-ai-about-transcript-design.md`

---

## File Map

### New files

- `src-tauri/src/ask/mod.rs` — public surface (`ask_query`, `cancel_ask`, `AskCancelRegistry`).
- `src-tauri/src/ask/types.rs` — `AskCitation`, `AskAnswer`, `AskTurnIn`, `AskProgressPayload`.
- `src-tauri/src/ask/strategy.rs` — `pick_strategy`, threshold constants.
- `src-tauri/src/ask/citations.rs` — quote-matching against the transcript.
- `src-tauri/src/ask/single_shot.rs` — path A (one streaming call).
- `src-tauri/src/ask/map_shortlist.rs` — path B (relevance pass + reduce).
- `src-tauri/src/ask/prompts.rs` — instruction/relevance prompt builders.
- `src-tauri/src/test_support.rs` — `#[cfg(test)]` MockOllama, lifted out of `commands.rs` so `ask/*` tests can reuse it.
- `src/types/ask.ts` — `AskCitation`, `AskTurn`, `AskThread`, progress payload.
- `src/api/ask.ts` — `invoke` wrappers for `ask_query` / `cancel_ask`.
- `src/lib/useAsk.ts` — per-recording session-only thread store + hook.
- `src/lib/useAsk.test.ts` — hook unit tests.
- `src/components/AskPanel.tsx` — the right-side drawer.
- `src/components/AskPanel.test.tsx` — panel UI tests.

### Modified files

- `src-tauri/src/commands.rs` — extract `ollama_stream` (generic streaming helper), make `preprocess`, `split_into_chunks`, `THRESHOLD`, `approx_tokens` reachable from `ask/*` (already `pub(crate)`), move `MockOllama` to `test_support.rs`.
- `src-tauri/src/lib.rs` — `pub mod ask;`, manage `Arc<AskCancelRegistry>`, register `ask::ask_query` and `ask::cancel_ask` in `invoke_handler!`.
- `src-tauri/src/main.rs` — no change expected (just re-exports `lib::run`).
- `src/components/RecordingView.tsx` — Ask toolbar button, mount `AskPanel`, ⌘K binding, pass `highlightRange` to `OutputPreview`.
- `src/components/OutputPreview.tsx` — add optional `highlightRange?: { start: number; end: number }` prop; force tab to "transcript" + scroll + flash highlight when it changes.
- `src/components/ui/Icon.tsx` — add `"chat"` → `ChatCircle` (Phosphor) entry.

---

## Task 1: Lift `MockOllama` to a shared test helper

**Files:**
- Create: `src-tauri/src/test_support.rs`
- Modify: `src-tauri/src/lib.rs` (add `#[cfg(test)] mod test_support;`)
- Modify: `src-tauri/src/commands.rs:1645-1755` (delete the local copy, import from `crate::test_support`)

This is a prerequisite refactor so the `ask/*` integration tests can share the existing axum-based fake Ollama. No behaviour changes.

- [ ] **Step 1: Add `axum` to non-dev test reach**

`axum` is already in `[dev-dependencies]` (`Cargo.toml:43`). `test_support.rs` will be `#[cfg(test)]`-gated, so no Cargo changes are needed.

- [ ] **Step 2: Create `src-tauri/src/test_support.rs`**

Copy the entire `MockOllama` struct, `impl MockOllama`, and `impl Drop for MockOllama` block from `commands.rs:1645-1755` into the new file, replacing the implicit `super::*` imports with explicit ones. Make the struct and method `pub(crate)`:

```rust
//! Test-only helpers shared across modules.
//!
//! Lifted out of `commands.rs` so sibling modules (e.g. `ask`) can reuse the
//! same in-process fake Ollama for their integration tests.

#![cfg(test)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub(crate) struct MockOllama {
    pub base_url: String,
    pub calls: Arc<Mutex<Vec<serde_json::Value>>>,
    pub overlaps: Arc<AtomicUsize>,
    pub in_flight: Arc<AtomicUsize>,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl MockOllama {
    pub async fn spawn<F>(handler: F) -> Self
    where
        F: Fn(serde_json::Value) -> String + Send + Sync + 'static,
    {
        use axum::{
            body::Body, extract::State, http::StatusCode, response::Response,
            routing::post, Router,
        };

        let calls: Arc<Mutex<Vec<serde_json::Value>>> = Arc::new(Mutex::new(Vec::new()));
        let overlaps = Arc::new(AtomicUsize::new(0));
        let in_flight = Arc::new(AtomicUsize::new(0));
        let handler = Arc::new(handler);

        #[derive(Clone)]
        struct AppState {
            calls: Arc<Mutex<Vec<serde_json::Value>>>,
            overlaps: Arc<AtomicUsize>,
            in_flight: Arc<AtomicUsize>,
            handler: Arc<dyn Fn(serde_json::Value) -> String + Send + Sync>,
        }

        async fn generate(
            State(s): State<AppState>,
            axum::Json(body): axum::Json<serde_json::Value>,
        ) -> Response {
            s.calls.lock().unwrap().push(body.clone());
            let cur = s.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            if cur > 1 {
                s.overlaps.fetch_add(1, Ordering::SeqCst);
            }

            let text = (s.handler)(body);
            tokio::time::sleep(Duration::from_millis(60)).await;

            let mut lines = String::new();
            for w in text.split_inclusive(' ') {
                let line = serde_json::json!({ "response": w }).to_string();
                lines.push_str(&line);
                lines.push('\n');
            }
            lines.push_str(&serde_json::json!({ "done": true }).to_string());
            lines.push('\n');

            s.in_flight.fetch_sub(1, Ordering::SeqCst);

            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "application/x-ndjson")
                .body(Body::from(lines))
                .unwrap()
        }

        let state = AppState {
            calls: calls.clone(),
            overlaps: overlaps.clone(),
            in_flight: in_flight.clone(),
            handler,
        };

        let app = Router::new()
            .route("/api/generate", post(generate))
            .with_state(state);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let base_url = format!("http://{addr}");

        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let handle = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async { let _ = rx.await; })
                .await
                .ok();
        });

        Self { base_url, calls, overlaps, in_flight, shutdown: Some(tx), handle: Some(handle) }
    }
}

impl Drop for MockOllama {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}
```

- [ ] **Step 3: Replace the inline copy in `commands.rs`**

Delete the `struct MockOllama { … }` block plus its `impl` and `impl Drop` (lines `1645-1755`) from inside `mod tests`. At the top of `mod tests` add:

```rust
use crate::test_support::MockOllama;
```

- [ ] **Step 4: Wire the module in `lib.rs`**

Add directly under the existing `pub mod` declarations:

```rust
#[cfg(test)]
mod test_support;
```

- [ ] **Step 5: Run all backend tests**

```bash
cd src-tauri && cargo test
```

Expected: same set of tests pass as before (no new tests yet). Specifically the format pipeline tests in `commands.rs` still pass via the relocated `MockOllama`.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/test_support.rs src-tauri/src/lib.rs src-tauri/src/commands.rs
git commit -m "refactor(test): lift MockOllama into shared test_support module"
```

---

## Task 2: Extract a generic `ollama_stream` helper

**Files:**
- Modify: `src-tauri/src/commands.rs:472-630` (replace `ollama_generate_stream`'s body with a thin wrapper that calls a new `pub(crate) ollama_stream`).

The current `ollama_generate_stream` is coupled to `FormatPhase` / `ChunkInfo`. Pull out a generic streaming helper that takes a `FnMut(&str)` token callback and a `FnMut() -> bool` keepalive check, returning the accumulated body string. The format pipeline keeps its existing public surface; `ask/*` reaches the new helper directly.

- [ ] **Step 1: Write the failing test**

Append to `mod tests` in `commands.rs`:

```rust
#[tokio::test]
async fn ollama_stream_collects_tokens_and_calls_back() {
    let mock = MockOllama::spawn(|_| "hello world.".to_string()).await;
    let collected: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let cb = {
        let collected = collected.clone();
        move |tok: &str| collected.lock().unwrap().push(tok.to_string())
    };
    let out = ollama_stream(
        &mock.base_url, "gemma3", "PROMPT".into(), 8192,
        &mut { cb }, None,
    )
    .await
    .unwrap();
    assert_eq!(out.trim(), "hello world.");
    let calls = mock.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0]["prompt"], "PROMPT");
    let toks = collected.lock().unwrap();
    assert!(toks.len() >= 2, "expected multiple token callbacks, got {}", toks.len());
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd src-tauri && cargo test commands::tests::ollama_stream_collects_tokens_and_calls_back -- --exact
```

Expected: FAIL — `ollama_stream` is not defined.

- [ ] **Step 3: Add `pub(crate) ollama_stream`**

Insert above the existing `async fn ollama_generate_stream(` at `commands.rs:472`:

```rust
/// Generic Ollama `/api/generate` streaming helper. Accumulates the response
/// body, invoking `on_token(piece)` for every streamed `response` field.
///
/// Honors `keep_alive: "30m"`, fixed REQUEST_TIMEOUT_SECS / INACTIVITY_TIMEOUT_SECS,
/// and races the read against `cancel.cancelled()` (returns `Err("cancelled")`
/// promptly if tripped during an await).
pub(crate) async fn ollama_stream(
    ollama_url: &str,
    model: &str,
    prompt: String,
    num_ctx: u32,
    on_token: &mut dyn FnMut(&str),
    cancel: Option<&crate::cancellation::CancelToken>,
) -> Result<String, String> {
    use futures_util::StreamExt;

    #[derive(serde::Serialize)]
    struct Options { num_ctx: u32, num_predict: i32 }
    #[derive(serde::Serialize)]
    struct Req<'a> {
        model: &'a str,
        prompt: String,
        stream: bool,
        keep_alive: &'a str,
        options: Options,
    }

    const REQUEST_TIMEOUT_SECS: u64 = 480;
    const INACTIVITY_TIMEOUT_SECS: u64 = 120;

    let body = Req { model, prompt, stream: true, keep_alive: "30m", options: Options { num_ctx, num_predict: -1 } };

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/generate", ollama_url))
        .json(&body)
        .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .send()
        .await
        .map_err(|e| format!("Cannot reach Ollama at {ollama_url}: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Ollama returned {status}: {text}"));
    }

    let mut stream = resp.bytes_stream();
    let mut buf = String::new();
    let mut accumulated = String::new();
    loop {
        let cancel_fut = async {
            match cancel {
                Some(c) => c.cancelled().await,
                None => std::future::pending::<()>().await,
            }
        };
        let next_fut = tokio::time::timeout(
            std::time::Duration::from_secs(INACTIVITY_TIMEOUT_SECS),
            stream.next(),
        );
        let next = tokio::select! {
            biased;
            _ = cancel_fut => return Err("cancelled".into()),
            r = next_fut => r,
        };
        let chunk_bytes = match next {
            Ok(Some(c)) => c,
            Ok(None) => break,
            Err(_) => {
                return Err(format!(
                    "Ollama at {ollama_url} stopped responding (no data for {INACTIVITY_TIMEOUT_SECS}s). The model may have been unloaded or the server crashed."
                ));
            }
        };
        let bytes = chunk_bytes.map_err(|e| format!("Ollama stream error: {e}"))?;
        buf.push_str(&String::from_utf8_lossy(&bytes));
        while let Some(nl) = buf.find('\n') {
            let line = buf[..nl].to_string();
            buf.drain(..=nl);
            if line.trim().is_empty() { continue; }
            let obj: serde_json::Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if let Some(piece) = obj.get("response").and_then(|v| v.as_str()) {
                accumulated.push_str(piece);
                on_token(piece);
            }
        }
    }
    Ok(accumulated)
}
```

- [ ] **Step 4: Re-implement `ollama_generate_stream` on top of `ollama_stream`**

Replace the body of `async fn ollama_generate_stream` (everything between its `{` and final `}`) with:

```rust
    let mut accumulated_chars: usize = 0;
    let mut on_token = |piece: &str| {
        accumulated_chars += piece.chars().count();
        on_progress(FormatProgress {
            phase,
            chunk: chunk.clone(),
            chars: accumulated_chars,
            done: false,
        });
    };
    ollama_stream(ollama_url, model, prompt, num_ctx, &mut on_token, cancel).await
```

- [ ] **Step 5: Run the full backend test suite**

```bash
cd src-tauri && cargo test
```

Expected: all existing format-pipeline tests still pass, the new `ollama_stream_collects_tokens_and_calls_back` test passes.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "refactor(format): extract generic ollama_stream helper"
```

---

## Task 3: Backend module skeleton + types

**Files:**
- Create: `src-tauri/src/ask/mod.rs`
- Create: `src-tauri/src/ask/types.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod ask;`)

- [ ] **Step 1: Write the failing test**

Create `src-tauri/src/ask/types.rs`:

```rust
//! Wire types shared between Rust and TypeScript for the Ask feature.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AskCitation {
    pub quote: String,
    pub start: Option<usize>,
    pub end: Option<usize>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AskAnswer {
    pub text: String,
    pub citations: Vec<AskCitation>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct AskTurnIn {
    pub role: String,
    pub content: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ask_citation_serializes_camel_case() {
        let c = AskCitation { quote: "hi".into(), start: Some(3), end: Some(5) };
        let json = serde_json::to_value(&c).unwrap();
        assert_eq!(json, serde_json::json!({"quote": "hi", "start": 3, "end": 5}));
    }

    #[test]
    fn ask_citation_serializes_null_offsets() {
        let c = AskCitation { quote: "x".into(), start: None, end: None };
        let json = serde_json::to_value(&c).unwrap();
        assert_eq!(json, serde_json::json!({"quote": "x", "start": null, "end": null}));
    }

    #[test]
    fn ask_answer_round_trips_in_camel_case() {
        let a = AskAnswer {
            text: "the answer".into(),
            citations: vec![AskCitation { quote: "q".into(), start: Some(0), end: Some(1) }],
        };
        let json = serde_json::to_value(&a).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"text": "the answer", "citations": [{"quote":"q","start":0,"end":1}]}),
        );
    }

    #[test]
    fn ask_turn_in_deserializes_role_and_content() {
        let v: AskTurnIn = serde_json::from_value(serde_json::json!({"role": "user", "content": "hi"})).unwrap();
        assert_eq!(v, AskTurnIn { role: "user".into(), content: "hi".into() });
    }
}
```

Create `src-tauri/src/ask/mod.rs` with just the module declarations:

```rust
//! Ask-AI-about-transcript feature. See
//! `docs/superpowers/specs/2026-05-09-ask-ai-about-transcript-design.md`.

pub mod types;
```

Add to `src-tauri/src/lib.rs` directly under the other `pub mod` lines (e.g. after `pub mod orchestrator;`):

```rust
pub mod ask;
```

- [ ] **Step 2: Run the type tests**

```bash
cd src-tauri && cargo test ask::types::tests
```

Expected: 4 PASS.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/ask/ src-tauri/src/lib.rs
git commit -m "feat(ask): scaffold ask module + wire types"
```

---

## Task 4: Citation matcher

**Files:**
- Create: `src-tauri/src/ask/citations.rs`
- Modify: `src-tauri/src/ask/mod.rs` (add `pub mod citations;`)

The matcher takes a transcript and a list of model-emitted quotes, returns a `Vec<AskCitation>` where each entry has byte offsets into the transcript or `None` if no plausible match was found. Algorithm: try exact substring first; on miss, try a normalized comparison (collapse runs of whitespace, fold curly→straight quotes, strip trailing punctuation) and translate the matched normalized span back to original byte offsets.

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/src/ask/citations.rs`:

```rust
//! Verbatim and whitespace-tolerant matching of model-emitted citation quotes
//! against the transcript text.

use crate::ask::types::AskCitation;

/// Match each `quote` against `transcript`. Returns one `AskCitation` per
/// input quote, with byte offsets `[start, end)` into the transcript, or
/// `start: None, end: None` if no plausible location was found. The first
/// occurrence wins.
pub fn match_quotes(transcript: &str, quotes: &[String]) -> Vec<AskCitation> {
    quotes.iter().map(|q| match_one(transcript, q)).collect()
}

fn match_one(transcript: &str, quote: &str) -> AskCitation {
    let trimmed = quote.trim();
    if trimmed.is_empty() {
        return AskCitation { quote: quote.clone(), start: None, end: None };
    }
    if let Some(start) = transcript.find(trimmed) {
        return AskCitation {
            quote: quote.clone(),
            start: Some(start),
            end: Some(start + trimmed.len()),
        };
    }
    if let Some((start, end)) = fuzzy_match(transcript, trimmed) {
        return AskCitation { quote: quote.clone(), start: Some(start), end: Some(end) };
    }
    AskCitation { quote: quote.clone(), start: None, end: None }
}

/// Whitespace- and quote-style-tolerant search.
///
/// Normalizes both the transcript and the needle (collapse all whitespace
/// runs to a single space, fold curly quotes to straight, lowercase ASCII)
/// while remembering the byte position in the original transcript that each
/// normalized character came from. A successful normalized substring search
/// then maps back to original `[start, end)`.
fn fuzzy_match(transcript: &str, needle: &str) -> Option<(usize, usize)> {
    let (norm_t, map) = normalize_with_map(transcript);
    let norm_n = normalize_string(needle);
    if norm_n.is_empty() { return None; }
    let pos = norm_t.find(&norm_n)?;
    let end = pos + norm_n.len();
    // map[i] is the byte offset in `transcript` of normalized char `i`.
    // For end, look up the byte AFTER the last matched normalized byte.
    let start_byte = map[pos];
    let end_byte = if end < map.len() { map[end] } else { transcript.len() };
    Some((start_byte, end_byte))
}

fn normalize_string(s: &str) -> String {
    let (n, _) = normalize_with_map(s);
    n
}

fn normalize_with_map(s: &str) -> (String, Vec<usize>) {
    let mut out = String::with_capacity(s.len());
    let mut map: Vec<usize> = Vec::with_capacity(s.len());
    let mut prev_was_space = true; // suppress leading whitespace
    for (idx, ch) in s.char_indices() {
        let folded = match ch {
            '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{2032}' => '\'',
            '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{2033}' => '"',
            c if c.is_whitespace() => ' ',
            c => c,
        };
        if folded == ' ' {
            if prev_was_space { continue; }
            prev_was_space = true;
            for b in 0..folded.len_utf8() {
                let _ = b;
                map.push(idx);
                out.push(' ');
            }
            continue;
        }
        prev_was_space = false;
        let lower = folded.to_ascii_lowercase();
        let mut tmp = [0u8; 4];
        let bytes = lower.encode_utf8(&mut tmp).as_bytes().to_vec();
        for _ in &bytes {
            map.push(idx);
        }
        out.push_str(std::str::from_utf8(&bytes).unwrap());
    }
    // Trailing space cleanup: drop a single trailing space if present.
    while out.ends_with(' ') {
        out.pop();
        map.pop();
    }
    (out, map)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quote(s: &str) -> Vec<String> { vec![s.to_string()] }

    #[test]
    fn exact_substring_matches() {
        let t = "hello world this is fine";
        let cites = match_quotes(t, &quote("world this"));
        assert_eq!(cites[0].start, Some(6));
        assert_eq!(cites[0].end, Some(16));
    }

    #[test]
    fn returns_first_occurrence() {
        let t = "yes yes yes";
        let cites = match_quotes(t, &quote("yes"));
        assert_eq!(cites[0].start, Some(0));
        assert_eq!(cites[0].end, Some(3));
    }

    #[test]
    fn whitespace_tolerant_match() {
        // Transcript has a hard newline; model echoes a single-space version.
        let t = "we delayed the\nQ3 budget review";
        let cites = match_quotes(t, &quote("we delayed the Q3 budget review"));
        assert!(cites[0].start.is_some());
        assert_eq!(cites[0].start, Some(0));
        let end = cites[0].end.unwrap();
        assert_eq!(&t[..end], t);
    }

    #[test]
    fn curly_quote_tolerant_match() {
        let t = "she said \u{201C}ship it\u{201D} firmly";
        let cites = match_quotes(t, &quote(r#"she said "ship it" firmly"#));
        assert!(cites[0].start.is_some(), "fuzzy match should fold curly quotes");
    }

    #[test]
    fn no_match_returns_none_offsets() {
        let cites = match_quotes("hello world", &quote("nothing relevant"));
        assert_eq!(cites[0].start, None);
        assert_eq!(cites[0].end, None);
    }

    #[test]
    fn empty_quote_returns_none() {
        let cites = match_quotes("hello", &quote(""));
        assert_eq!(cites[0].start, None);
        assert_eq!(cites[0].end, None);
    }

    #[test]
    fn quote_longer_than_transcript_returns_none() {
        let cites = match_quotes("hi", &quote("hi there fine thanks"));
        assert_eq!(cites[0].start, None);
    }

    #[test]
    fn multiple_quotes_each_resolved_independently() {
        let t = "alpha beta gamma";
        let q = vec!["alpha".into(), "missing".into(), "gamma".into()];
        let cites = match_quotes(t, &q);
        assert_eq!(cites[0].start, Some(0));
        assert_eq!(cites[1].start, None);
        assert_eq!(cites[2].start, Some(11));
    }

    #[test]
    fn trims_surrounding_whitespace_before_searching() {
        let t = "alpha beta gamma";
        let cites = match_quotes(t, &quote("  beta  "));
        assert_eq!(cites[0].start, Some(6));
        assert_eq!(cites[0].end, Some(10));
    }
}
```

Add to `src-tauri/src/ask/mod.rs`:

```rust
pub mod citations;
```

- [ ] **Step 2: Run the citation tests**

```bash
cd src-tauri && cargo test ask::citations
```

Expected: PASS for all 9 tests.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/ask/citations.rs src-tauri/src/ask/mod.rs
git commit -m "feat(ask): citation matcher (verbatim + whitespace tolerant)"
```

---

## Task 5: Strategy selector

**Files:**
- Create: `src-tauri/src/ask/strategy.rs`
- Modify: `src-tauri/src/ask/mod.rs` (add `pub mod strategy;`)

`pick_strategy` returns `Strategy::SingleShot` or `Strategy::MapShortlist` based on transcript length vs the model context budget. Uses `commands::THRESHOLD` and `commands::approx_tokens` so the boundary stays a single source of truth — the format pipeline already reuses these for its single-vs-map-reduce choice.

- [ ] **Step 1: Write the failing test**

Create `src-tauri/src/ask/strategy.rs`:

```rust
//! Path-A/B selection for the Ask pipeline.
//!
//! Single source of truth lives in `commands::{THRESHOLD, approx_tokens}` —
//! the format pipeline already uses it. We share it so the boundary doesn't
//! drift between features.

use crate::commands::{approx_tokens_pub as approx_tokens, THRESHOLD_PUB as THRESHOLD};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    SingleShot,
    MapShortlist,
}

pub fn pick_strategy(transcript: &str) -> Strategy {
    if approx_tokens(transcript) < THRESHOLD {
        Strategy::SingleShot
    } else {
        Strategy::MapShortlist
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_transcript_picks_single_shot() {
        let t: String = "word ".repeat(100);
        assert_eq!(pick_strategy(&t), Strategy::SingleShot);
    }

    #[test]
    fn long_transcript_picks_map_shortlist() {
        // THRESHOLD = 4000 tokens × ~4 chars/token = ~16k chars. 30k chars
        // is comfortably above that.
        let t: String = "x".repeat(30_000);
        assert_eq!(pick_strategy(&t), Strategy::MapShortlist);
    }

    #[test]
    fn exactly_at_threshold_picks_map_shortlist() {
        // approx_tokens uses chars()/4, so 4000 tokens == 16_000 chars exactly.
        let t: String = "a".repeat(16_000);
        assert_eq!(pick_strategy(&t), Strategy::MapShortlist);
    }

    #[test]
    fn one_below_threshold_picks_single_shot() {
        let t: String = "a".repeat(15_999);
        assert_eq!(pick_strategy(&t), Strategy::SingleShot);
    }
}
```

Add to `src-tauri/src/ask/mod.rs`:

```rust
pub mod strategy;
```

- [ ] **Step 2: Re-export the helpers from `commands`**

`commands.rs` currently has:

```rust
pub(crate) const THRESHOLD: usize = 4000;
fn approx_tokens(s: &str) -> usize { s.chars().count() / CHARS_PER_TOKEN }
```

Make both reachable from `ask::strategy` by adding (next to the existing definitions in `commands.rs:322-330`):

```rust
pub(crate) const THRESHOLD_PUB: usize = THRESHOLD;
pub(crate) fn approx_tokens_pub(s: &str) -> usize { approx_tokens(s) }
```

(We keep the original symbols private to `commands`'s own use, and expose stable re-export names for sibling modules. This avoids accidentally widening the rest of `commands`'s internals.)

- [ ] **Step 3: Run the strategy tests**

```bash
cd src-tauri && cargo test ask::strategy
```

Expected: 4 PASS.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/ask/strategy.rs src-tauri/src/ask/mod.rs src-tauri/src/commands.rs
git commit -m "feat(ask): strategy selector reusing format threshold"
```

---

## Task 6: Prompts + answer parsing

**Files:**
- Create: `src-tauri/src/ask/prompts.rs`
- Modify: `src-tauri/src/ask/mod.rs` (add `pub mod prompts;`)

Two prompt builders and one parser. The answer parser splits a model response on the literal marker `--- citations ---`, returning `(answer_text, Vec<String>)`.

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/src/ask/prompts.rs`:

```rust
//! Prompt construction and answer parsing for the Ask feature.

use crate::ask::types::AskTurnIn;

const CITATION_MARKER: &str = "--- citations ---";

/// Builds the answer prompt for path A and the reduce step of path B.
///
/// Includes any prior conversation `history` inline so the model has context
/// for follow-up questions.
pub fn build_answer_prompt(transcript_or_chunks: &str, question: &str, history: &[AskTurnIn]) -> String {
    let mut history_block = String::new();
    if !history.is_empty() {
        history_block.push_str("Previous conversation:\n");
        for turn in history {
            let label = match turn.role.as_str() {
                "user" => "User",
                "assistant" => "Assistant",
                _ => "User",
            };
            history_block.push_str(&format!("{label}: {}\n", turn.content.trim()));
        }
        history_block.push('\n');
    }

    format!(
        r#"You are answering a question about a transcript. Use ONLY the transcript below — never invent facts. If the transcript doesn't contain the answer, say so plainly.

For every factual claim in your answer, quote verbatim from the transcript.

After the prose answer, output the literal marker on its own line:

{marker}

Then list each verbatim quote you used, one per line, copied EXACTLY as it appears in the transcript (including any punctuation and casing). Do not paraphrase the citations.

{history}TRANSCRIPT:
"""
{transcript}
"""

QUESTION: {question}

Answer now:"#,
        marker = CITATION_MARKER,
        history = history_block,
        transcript = transcript_or_chunks,
        question = question,
    )
}

/// Builds the per-chunk relevance prompt for path B's map pass.
pub fn build_relevance_prompt(chunk: &str, index: usize, total: usize, question: &str) -> String {
    format!(
        r#"You are scanning ONE SEGMENT of a longer transcript (segment {n} of {total}) to decide whether it is relevant to a user's question.

Reply with a single JSON object on one line, with two fields:
- "relevant": true or false
- "gist": one short sentence summarizing what the segment says about the question (or "" if not relevant).

Do not output anything else — no explanation, no markdown.

QUESTION: {question}

SEGMENT:
"""
{chunk}
"""

JSON:"#,
        n = index + 1,
        total = total,
        question = question,
        chunk = chunk,
    )
}

/// Splits a model response on the citation marker. Returns the prose answer
/// (left of marker) and a list of citation lines (right of marker, each line
/// non-empty and trimmed). If the marker is absent, returns `(response, [])`.
pub fn parse_answer(response: &str) -> (String, Vec<String>) {
    match response.split_once(CITATION_MARKER) {
        Some((answer, cites_block)) => {
            let cites = cites_block
                .lines()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty())
                .map(|l| strip_quotes(l).to_string())
                .collect();
            (answer.trim_end().to_string(), cites)
        }
        None => (response.trim().to_string(), Vec::new()),
    }
}

fn strip_quotes(s: &str) -> &str {
    let s = s.trim();
    let s = s.strip_prefix('"').unwrap_or(s);
    let s = s.strip_suffix('"').unwrap_or(s);
    s
}

#[derive(Debug, PartialEq, Eq)]
pub struct RelevanceVerdict {
    pub relevant: bool,
    pub gist: String,
}

/// Best-effort parse of a one-line JSON verdict from the relevance pass.
/// On any parse failure, falls back to `relevant: false` so the chunk is
/// excluded from the reduce pass — safer than including suspect content.
pub fn parse_relevance(s: &str) -> RelevanceVerdict {
    // Find the first `{` and last `}` to tolerate stray prefix/suffix text.
    let start = s.find('{');
    let end = s.rfind('}');
    let json = match (start, end) {
        (Some(a), Some(b)) if b >= a => &s[a..=b],
        _ => return RelevanceVerdict { relevant: false, gist: String::new() },
    };
    let v: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return RelevanceVerdict { relevant: false, gist: String::new() },
    };
    RelevanceVerdict {
        relevant: v.get("relevant").and_then(|x| x.as_bool()).unwrap_or(false),
        gist: v.get("gist").and_then(|x| x.as_str()).unwrap_or("").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_answer_prompt_includes_transcript_and_question() {
        let p = build_answer_prompt("the only line", "what is it?", &[]);
        assert!(p.contains("the only line"));
        assert!(p.contains("what is it?"));
        assert!(p.contains(CITATION_MARKER));
        assert!(!p.contains("Previous conversation"));
    }

    #[test]
    fn build_answer_prompt_appends_history() {
        let history = vec![
            AskTurnIn { role: "user".into(), content: "first q".into() },
            AskTurnIn { role: "assistant".into(), content: "first a".into() },
        ];
        let p = build_answer_prompt("t", "follow up?", &history);
        assert!(p.contains("Previous conversation:"));
        assert!(p.contains("User: first q"));
        assert!(p.contains("Assistant: first a"));
    }

    #[test]
    fn build_relevance_prompt_includes_chunk_and_question() {
        let p = build_relevance_prompt("seg body", 0, 3, "Q?");
        assert!(p.contains("segment 1 of 3"));
        assert!(p.contains("seg body"));
        assert!(p.contains("Q?"));
        assert!(p.contains("\"relevant\""));
    }

    #[test]
    fn parse_answer_splits_on_marker() {
        let resp = "the answer text here.\n\n--- citations ---\n\"first quote\"\n\"second quote\"\n";
        let (text, cites) = parse_answer(resp);
        assert_eq!(text, "the answer text here.");
        assert_eq!(cites, vec!["first quote", "second quote"]);
    }

    #[test]
    fn parse_answer_returns_empty_citations_when_marker_absent() {
        let (text, cites) = parse_answer("plain answer with no citations");
        assert_eq!(text, "plain answer with no citations");
        assert!(cites.is_empty());
    }

    #[test]
    fn parse_answer_strips_blank_lines_in_citation_block() {
        let resp = "answer\n--- citations ---\n\n\"q1\"\n\n\n\"q2\"\n\n";
        let (_, cites) = parse_answer(resp);
        assert_eq!(cites, vec!["q1", "q2"]);
    }

    #[test]
    fn parse_relevance_handles_clean_json() {
        let v = parse_relevance(r#"{"relevant": true, "gist": "discusses Q3 budget"}"#);
        assert_eq!(v, RelevanceVerdict { relevant: true, gist: "discusses Q3 budget".into() });
    }

    #[test]
    fn parse_relevance_falls_back_to_false_on_garbage() {
        let v = parse_relevance("yes definitely relevant!!");
        assert_eq!(v.relevant, false);
        assert_eq!(v.gist, "");
    }

    #[test]
    fn parse_relevance_tolerates_surrounding_whitespace_or_text() {
        let v = parse_relevance("Sure thing: {\"relevant\": false, \"gist\": \"\"} done.");
        assert_eq!(v.relevant, false);
    }
}
```

Add to `src-tauri/src/ask/mod.rs`:

```rust
pub mod prompts;
```

- [ ] **Step 2: Run the prompt tests**

```bash
cd src-tauri && cargo test ask::prompts
```

Expected: 9 PASS.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/ask/prompts.rs src-tauri/src/ask/mod.rs
git commit -m "feat(ask): prompt builders + answer/relevance parsers"
```

---

## Task 7: Single-shot path

**Files:**
- Create: `src-tauri/src/ask/single_shot.rs`
- Modify: `src-tauri/src/ask/mod.rs` (add `pub mod single_shot;`)

`run_single_shot` builds the answer prompt, calls `commands::ollama_stream`, then parses citations and matches them against the transcript. Streams character counts to a progress callback.

- [ ] **Step 1: Write the failing test**

Create `src-tauri/src/ask/single_shot.rs`:

```rust
//! Path A: single-shot Ollama call over the full transcript.

use crate::ask::citations::match_quotes;
use crate::ask::prompts::{build_answer_prompt, parse_answer};
use crate::ask::types::{AskAnswer, AskTurnIn};
use crate::cancellation::CancelToken;
use crate::commands::ollama_stream;

/// Run path A. Returns the final `AskAnswer` with citations resolved against
/// the original transcript.
///
/// `on_chars` is called as tokens stream in, with the running character count
/// of the answer (including any tail that will later be split off as the
/// citation block — splitting only happens at the end).
pub async fn run_single_shot(
    ollama_url: &str,
    model: &str,
    transcript: &str,
    question: &str,
    history: &[AskTurnIn],
    on_chars: &mut dyn FnMut(usize),
    cancel: Option<&CancelToken>,
) -> Result<AskAnswer, String> {
    let prompt = build_answer_prompt(transcript, question, history);
    let mut total_chars: usize = 0;
    let mut on_token = |piece: &str| {
        total_chars += piece.chars().count();
        on_chars(total_chars);
    };
    let raw = ollama_stream(ollama_url, model, prompt, 8192, &mut on_token, cancel).await?;
    let (text, quotes) = parse_answer(&raw);
    let citations = match_quotes(transcript, &quotes);
    Ok(AskAnswer { text, citations })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::MockOllama;

    #[tokio::test]
    async fn single_shot_returns_answer_and_resolves_citations() {
        let transcript = "we delayed the Q3 budget review until November.";
        let mock = MockOllama::spawn(|_body| {
            "We delayed Q3 to November.\n\n--- citations ---\n\"we delayed the Q3 budget review until November\"\n".to_string()
        }).await;

        let mut chars: usize = 0;
        let ans = run_single_shot(
            &mock.base_url, "gemma3", transcript, "when was Q3 reviewed?",
            &[], &mut |c| chars = c, None,
        ).await.unwrap();

        assert!(ans.text.contains("November"));
        assert_eq!(ans.citations.len(), 1);
        assert!(ans.citations[0].start.is_some(), "citation should resolve to transcript offset");
        assert!(chars > 0, "on_chars should have been called as tokens streamed");

        let calls = mock.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        let prompt = calls[0]["prompt"].as_str().unwrap();
        assert!(prompt.contains("when was Q3 reviewed?"));
        assert!(prompt.contains("we delayed the Q3 budget review"));
    }

    #[tokio::test]
    async fn single_shot_marks_unmatched_citation_with_none_offsets() {
        let transcript = "agenda topic: marketing.";
        let mock = MockOllama::spawn(|_| {
            "Marketing was discussed.\n--- citations ---\n\"some text not in transcript\"\n".to_string()
        }).await;

        let ans = run_single_shot(
            &mock.base_url, "gemma3", transcript, "anything?", &[],
            &mut |_| {}, None,
        ).await.unwrap();

        assert_eq!(ans.citations.len(), 1);
        assert!(ans.citations[0].start.is_none());
        assert!(ans.citations[0].end.is_none());
    }

    #[tokio::test]
    async fn single_shot_handles_response_with_no_citation_block() {
        let mock = MockOllama::spawn(|_| "just an answer.".to_string()).await;
        let ans = run_single_shot(
            &mock.base_url, "gemma3", "irrelevant transcript", "q?", &[],
            &mut |_| {}, None,
        ).await.unwrap();
        assert_eq!(ans.text, "just an answer.");
        assert!(ans.citations.is_empty());
    }

    #[tokio::test]
    async fn single_shot_passes_history_into_prompt() {
        let mock = MockOllama::spawn(|_| "follow up answer.".to_string()).await;
        let history = vec![
            AskTurnIn { role: "user".into(), content: "prior question".into() },
            AskTurnIn { role: "assistant".into(), content: "prior answer".into() },
        ];
        let _ = run_single_shot(
            &mock.base_url, "gemma3", "t", "next?", &history,
            &mut |_| {}, None,
        ).await.unwrap();
        let calls = mock.calls.lock().unwrap();
        let prompt = calls[0]["prompt"].as_str().unwrap();
        assert!(prompt.contains("Previous conversation"));
        assert!(prompt.contains("prior question"));
        assert!(prompt.contains("prior answer"));
    }
}
```

Add to `src-tauri/src/ask/mod.rs`:

```rust
pub mod single_shot;
```

- [ ] **Step 2: Run the single-shot tests**

```bash
cd src-tauri && cargo test ask::single_shot
```

Expected: 4 PASS.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/ask/single_shot.rs src-tauri/src/ask/mod.rs
git commit -m "feat(ask): single-shot path A with cited answer"
```

---

## Task 8: Map-shortlist + reduce path

**Files:**
- Create: `src-tauri/src/ask/map_shortlist.rs`
- Modify: `src-tauri/src/ask/mod.rs` (add `pub mod map_shortlist;`)
- Modify: `src-tauri/src/commands.rs` — expose `split_into_chunks_pub` re-export.

Path B: chunk the transcript, run a sequential relevance pass per chunk, then reduce over only the relevant chunks using the same answer prompt as path A. Reuses `commands::split_into_chunks` and `commands::preprocess`.

- [ ] **Step 1: Re-export the chunking helpers**

Add next to the strategy re-exports in `commands.rs:322-330`:

```rust
pub(crate) fn split_into_chunks_pub(s: &str) -> Vec<String> { split_into_chunks(s) }
pub(crate) fn preprocess_pub(s: &str) -> String { preprocess(s) }
```

- [ ] **Step 2: Write the failing test**

Create `src-tauri/src/ask/map_shortlist.rs`:

```rust
//! Path B: map (relevance shortlist) + reduce (single answer over the
//! relevant chunks). Map runs sequentially — Ollama is single-tenant in
//! practice and parallel relevance probes risk model swapping.

use crate::ask::citations::match_quotes;
use crate::ask::prompts::{build_answer_prompt, build_relevance_prompt, parse_answer, parse_relevance};
use crate::ask::types::{AskAnswer, AskTurnIn};
use crate::cancellation::CancelToken;
use crate::commands::{ollama_stream, preprocess_pub as preprocess, split_into_chunks_pub as split_into_chunks};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    Search { index: usize, total: usize },
    Answer { chars: usize },
}

pub async fn run_map_shortlist(
    ollama_url: &str,
    model: &str,
    transcript: &str,
    question: &str,
    history: &[AskTurnIn],
    on_phase: &mut dyn FnMut(Phase),
    cancel: Option<&CancelToken>,
) -> Result<AskAnswer, String> {
    let cleaned = preprocess(transcript);
    let chunks = split_into_chunks(&cleaned);
    let total = chunks.len();

    let mut relevant: Vec<(usize, String)> = Vec::new();
    for (idx, chunk) in chunks.iter().enumerate() {
        on_phase(Phase::Search { index: idx, total });
        let prompt = build_relevance_prompt(chunk, idx, total, question);
        let mut sink = |_: &str| {};
        let raw = ollama_stream(ollama_url, model, prompt, 4096, &mut sink, cancel).await?;
        let verdict = parse_relevance(&raw);
        if verdict.relevant {
            relevant.push((idx, chunk.clone()));
        }
    }

    if relevant.is_empty() {
        return Ok(AskAnswer {
            text: "I couldn't find anything about that in the transcript.".to_string(),
            citations: Vec::new(),
        });
    }

    let mut joined = String::new();
    for (idx, chunk) in &relevant {
        joined.push_str(&format!("[chunk {}]\n", idx + 1));
        joined.push_str(chunk);
        joined.push_str("\n\n");
    }

    let prompt = build_answer_prompt(joined.trim_end(), question, history);
    let mut total_chars: usize = 0;
    let mut on_token = |piece: &str| {
        total_chars += piece.chars().count();
        on_phase(Phase::Answer { chars: total_chars });
    };
    let raw = ollama_stream(ollama_url, model, prompt, 8192, &mut on_token, cancel).await?;
    let (text, quotes) = parse_answer(&raw);
    // Match against the ORIGINAL (unprocessed) transcript so click-to-scroll
    // lands on real text. Some quotes may not match because preprocess
    // removed fillers — that's acceptable; the citation just renders as
    // unclickable.
    let citations = match_quotes(transcript, &quotes);
    Ok(AskAnswer { text, citations })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::MockOllama;

    fn long_transcript() -> String {
        // Designed to split into multiple chunks. THRESHOLD = 4000 tokens × 4
        // chars/token = 16k chars; we go well over.
        let sentence = "This is a long sentence with words. ";
        sentence.repeat(800)
    }

    #[tokio::test]
    async fn relevant_chunks_only_reach_reduce() {
        let transcript = long_transcript();
        let mock = MockOllama::spawn(|body| {
            let prompt = body["prompt"].as_str().unwrap_or("");
            if prompt.contains("Reply with a single JSON object") {
                // Mark first chunk relevant, rest not.
                if prompt.contains("segment 1 of") {
                    r#"{"relevant": true, "gist": "the answer"}"#.to_string()
                } else {
                    r#"{"relevant": false, "gist": ""}"#.to_string()
                }
            } else {
                "Reduced answer.\n--- citations ---\n\"This is a long sentence with words.\"\n".to_string()
            }
        }).await;

        let ans = run_map_shortlist(
            &mock.base_url, "gemma3", &transcript, "what?", &[],
            &mut |_| {}, None,
        ).await.unwrap();

        assert!(ans.text.contains("Reduced answer"));
        let calls = mock.calls.lock().unwrap();
        // N relevance + 1 reduce. We can't pin N exactly without re-running
        // split_into_chunks, but it's >= 2 (multiple chunks) and the LAST
        // call must be the reduce.
        assert!(calls.len() >= 3);
        let last = calls.last().unwrap();
        let last_prompt = last["prompt"].as_str().unwrap();
        assert!(last_prompt.contains("--- citations ---") || last_prompt.contains("QUESTION:"));
        assert!(last_prompt.contains("[chunk"));
        // Reduce must NOT include the irrelevant chunks: only one [chunk N]
        // header should appear in the joined transcript section.
        let header_count = last_prompt.matches("[chunk ").count();
        assert_eq!(header_count, 1, "reduce should see only relevant chunks");
    }

    #[tokio::test]
    async fn zero_relevant_chunks_short_circuits_without_reduce() {
        let transcript = long_transcript();
        let mock = MockOllama::spawn(|body| {
            let prompt = body["prompt"].as_str().unwrap_or("");
            if prompt.contains("Reply with a single JSON object") {
                r#"{"relevant": false, "gist": ""}"#.to_string()
            } else {
                panic!("reduce pass should never run when nothing is relevant")
            }
        }).await;

        let ans = run_map_shortlist(
            &mock.base_url, "gemma3", &transcript, "obscure?", &[],
            &mut |_| {}, None,
        ).await.unwrap();

        assert!(ans.text.to_lowercase().contains("couldn't find"));
        assert!(ans.citations.is_empty());
    }

    #[tokio::test]
    async fn emits_search_phase_per_chunk_then_answer_phase() {
        let transcript = long_transcript();
        let mock = MockOllama::spawn(|body| {
            let prompt = body["prompt"].as_str().unwrap_or("");
            if prompt.contains("Reply with a single JSON object") {
                r#"{"relevant": true, "gist": ""}"#.to_string()
            } else {
                "ok.\n--- citations ---\n".to_string()
            }
        }).await;

        let phases: std::sync::Arc<std::sync::Mutex<Vec<Phase>>> =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let phases_cb = phases.clone();
        let mut on_phase = move |p: Phase| phases_cb.lock().unwrap().push(p);

        let _ = run_map_shortlist(
            &mock.base_url, "gemma3", &transcript, "any?", &[],
            &mut on_phase, None,
        ).await.unwrap();

        let phases = phases.lock().unwrap();
        let search_count = phases.iter().filter(|p| matches!(p, Phase::Search { .. })).count();
        let answer_count = phases.iter().filter(|p| matches!(p, Phase::Answer { .. })).count();
        assert!(search_count >= 2, "expected one Search phase per chunk, got {search_count}");
        assert!(answer_count >= 1, "expected at least one Answer phase, got {answer_count}");

        // Last Search phase index < total, and Answer comes after the searches.
        let last_search = phases.iter().rposition(|p| matches!(p, Phase::Search { .. })).unwrap();
        let first_answer = phases.iter().position(|p| matches!(p, Phase::Answer { .. })).unwrap();
        assert!(first_answer > last_search);
    }
}
```

Add to `src-tauri/src/ask/mod.rs`:

```rust
pub mod map_shortlist;
```

- [ ] **Step 3: Run the map-shortlist tests**

```bash
cd src-tauri && cargo test ask::map_shortlist
```

Expected: 3 PASS.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/ask/map_shortlist.rs src-tauri/src/ask/mod.rs src-tauri/src/commands.rs
git commit -m "feat(ask): map-shortlist + reduce path B"
```

---

## Task 9: Per-recording cancellation registry

**Files:**
- Create: `src-tauri/src/ask/cancel.rs`
- Modify: `src-tauri/src/ask/mod.rs` (add `pub mod cancel;`)

Distinct from `RecordingCancelRegistry` — that registry's token controls the in-flight pipeline run for a recording. Mixing them would mean cancelling an Ask aborts a concurrent format pass (and vice versa). We add a parallel `AskCancelRegistry` keyed by recording_id.

- [ ] **Step 1: Write the failing test**

Create `src-tauri/src/ask/cancel.rs`:

```rust
//! Per-recording cancellation tokens specifically for in-flight Ask queries.
//!
//! Kept separate from `cancellation::RecordingCancelRegistry` so an Ask cancel
//! doesn't trip a concurrent recording-pipeline run, and vice versa.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::cancellation::CancelToken;

#[derive(Default)]
pub struct AskCancelRegistry {
    inner: Mutex<HashMap<String, CancelToken>>,
}

impl AskCancelRegistry {
    pub fn token_for_new_query(&self, recording_id: &str) -> CancelToken {
        let mut g = self.inner.lock().unwrap();
        let tok = CancelToken::default();
        g.insert(recording_id.to_string(), tok.clone());
        tok
    }

    pub fn get(&self, recording_id: &str) -> Option<CancelToken> {
        self.inner.lock().unwrap().get(recording_id).cloned()
    }

    pub fn finish(&self, recording_id: &str) {
        self.inner.lock().unwrap().remove(recording_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_for_new_query_replaces_existing_token() {
        let reg = AskCancelRegistry::default();
        let t1 = reg.token_for_new_query("rec1");
        t1.cancel();
        let t2 = reg.token_for_new_query("rec1");
        assert!(!t2.is_cancelled(), "fresh token must start un-cancelled");
        assert!(t1.is_cancelled());
    }

    #[test]
    fn get_returns_none_after_finish() {
        let reg = AskCancelRegistry::default();
        let _ = reg.token_for_new_query("rec1");
        assert!(reg.get("rec1").is_some());
        reg.finish("rec1");
        assert!(reg.get("rec1").is_none());
    }

    #[test]
    fn cancel_via_registry_propagates_to_held_token() {
        let reg = AskCancelRegistry::default();
        let held = reg.token_for_new_query("rec1");
        reg.get("rec1").unwrap().cancel();
        assert!(held.is_cancelled());
    }
}
```

Add to `src-tauri/src/ask/mod.rs`:

```rust
pub mod cancel;
```

- [ ] **Step 2: Run the registry tests**

```bash
cd src-tauri && cargo test ask::cancel
```

Expected: 3 PASS.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/ask/cancel.rs src-tauri/src/ask/mod.rs
git commit -m "feat(ask): per-recording cancel registry"
```

---

## Task 10: `ask_query` and `cancel_ask` Tauri commands

**Files:**
- Modify: `src-tauri/src/ask/mod.rs` (add the two `#[tauri::command]` functions and wire `ask-progress` events).
- Modify: `src-tauri/src/lib.rs` (manage `Arc<AskCancelRegistry>`, register both commands in `invoke_handler!`).

`ask_query` reads the transcript from the recording's transcribe stage, picks a strategy, dispatches, emits `ask-progress`, and returns the final `AskAnswer`. `cancel_ask` flips the registered token.

- [ ] **Step 1: Replace `src-tauri/src/ask/mod.rs` body**

```rust
//! Ask-AI-about-transcript feature. See
//! `docs/superpowers/specs/2026-05-09-ask-ai-about-transcript-design.md`.

pub mod cancel;
pub mod citations;
pub mod map_shortlist;
pub mod prompts;
pub mod single_shot;
pub mod strategy;
pub mod types;

use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::ask::cancel::AskCancelRegistry;
use crate::ask::strategy::{pick_strategy, Strategy};
use crate::ask::types::{AskAnswer, AskTurnIn};
use crate::storage::{self, StageKind, StageStatus, Store};

#[derive(Clone, Serialize)]
#[serde(tag = "phase", rename_all = "lowercase")]
enum AskProgressPayload {
    #[serde(rename_all = "camelCase")]
    Search { chunk: ChunkInfo },
    Answer { chars: usize },
    Done,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChunkInfo {
    index: usize,
    total: usize,
}

#[tauri::command]
pub async fn ask_query(
    app: AppHandle,
    store: State<'_, Arc<Store>>,
    registry: State<'_, Arc<AskCancelRegistry>>,
    recording_id: String,
    question: String,
    history: Vec<AskTurnIn>,
) -> Result<AskAnswer, String> {
    if question.trim().is_empty() {
        return Err("question is empty".into());
    }

    // `recordings::get` already returns the recording with stages populated,
    // so one read covers both transcript text and Ollama settings.
    let store = (*store).clone();
    let rec_id = recording_id.clone();
    let (transcript, settings) =
        tokio::task::spawn_blocking(move || -> Result<(String, crate::storage::AppSettings), String> {
            let r = storage::recordings::get(&store, &rec_id)?
                .ok_or_else(|| format!("recording not found: {rec_id}"))?;
            let t = r
                .stages
                .iter()
                .find(|s| s.stage == StageKind::Transcribe)
                .ok_or_else(|| "transcribe stage not found".to_string())?;
            if t.status != StageStatus::Completed {
                return Err("transcribe stage has not completed".into());
            }
            let transcript = t
                .output
                .clone()
                .ok_or_else(|| "transcribe stage has no output".to_string())?;
            Ok((transcript, r.settings))
        })
        .await
        .map_err(|e| format!("task join: {e}"))??;

    let token = registry.token_for_new_query(&recording_id);

    let app_for_search = app.clone();
    let app_for_answer = app.clone();

    let result = match pick_strategy(&transcript) {
        Strategy::SingleShot => {
            let mut on_chars = move |chars: usize| {
                let _ = app_for_answer.emit("ask-progress", AskProgressPayload::Answer { chars });
            };
            crate::ask::single_shot::run_single_shot(
                &settings.ollama_url,
                &settings.model,
                &transcript,
                &question,
                &history,
                &mut on_chars,
                Some(&token),
            )
            .await
        }
        Strategy::MapShortlist => {
            let mut on_phase = move |p: crate::ask::map_shortlist::Phase| {
                let payload = match p {
                    crate::ask::map_shortlist::Phase::Search { index, total } => {
                        AskProgressPayload::Search { chunk: ChunkInfo { index, total } }
                    }
                    crate::ask::map_shortlist::Phase::Answer { chars } => {
                        AskProgressPayload::Answer { chars }
                    }
                };
                let _ = app_for_search.emit("ask-progress", payload);
            };
            crate::ask::map_shortlist::run_map_shortlist(
                &settings.ollama_url,
                &settings.model,
                &transcript,
                &question,
                &history,
                &mut on_phase,
                Some(&token),
            )
            .await
        }
    };

    registry.finish(&recording_id);
    let _ = app.emit("ask-progress", AskProgressPayload::Done);
    result
}

#[tauri::command]
pub async fn cancel_ask(
    registry: State<'_, Arc<AskCancelRegistry>>,
    recording_id: String,
) -> Result<(), String> {
    if let Some(t) = registry.get(&recording_id) {
        t.cancel();
        Ok(())
    } else {
        Err(format!("no in-flight ask for recording {recording_id}"))
    }
}
```

- [ ] **Step 2: Register the command and registry in `lib.rs`**

In `src-tauri/src/lib.rs`:

1. After the existing `app.manage(...)` lines in `setup`, add:

```rust
app.manage(Arc::new(crate::ask::cancel::AskCancelRegistry::default()));
```

2. Inside `tauri::generate_handler![...]` add:

```rust
ask::ask_query,
ask::cancel_ask,
```

- [ ] **Step 3: Build it**

```bash
cd src-tauri && cargo build
```

Expected: clean build.

- [ ] **Step 4: Smoke test the registered command**

There's no integration-test pattern for Tauri commands in this repo (the format pipeline tests bypass the command layer and test `run_format_core` directly). The Tauri layer is glue and is exercised manually in Task 18. Run the full suite to confirm nothing broke:

```bash
cd src-tauri && cargo test
```

Expected: all previously-passing tests still pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/ask/mod.rs src-tauri/src/lib.rs
git commit -m "feat(ask): wire ask_query / cancel_ask Tauri commands"
```

---

## Task 11: Frontend types

**Files:**
- Create: `src/types/ask.ts`

- [ ] **Step 1: Create the file**

```ts
export interface AskCitation {
  quote: string;
  start: number | null;
  end: number | null;
}

export interface AskTurn {
  role: "user" | "assistant";
  content: string;
  citations?: AskCitation[];
}

export interface AskThread {
  id: string;
  turns: AskTurn[];
  pending: boolean;
  streamingText?: string;
}

export type AskProgressPayload =
  | { phase: "search"; chunk: { index: number; total: number } }
  | { phase: "answer"; chars: number }
  | { phase: "done" };
```

- [ ] **Step 2: Compile-check**

```bash
npx tsc --noEmit
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/types/ask.ts
git commit -m "feat(ask): frontend wire types"
```

---

## Task 12: Frontend API wrapper

**Files:**
- Create: `src/api/ask.ts`

- [ ] **Step 1: Create the file**

```ts
import { invoke } from "@tauri-apps/api/core";
import type { AskCitation, AskTurn } from "../types/ask";

export interface AskAnswer {
  text: string;
  citations: AskCitation[];
}

export const askApi = {
  query: (recordingId: string, question: string, history: AskTurn[]) =>
    invoke<AskAnswer>("ask_query", {
      recordingId,
      question,
      history: history.map((t) => ({ role: t.role, content: t.content })),
    }),

  cancel: (recordingId: string) =>
    invoke<void>("cancel_ask", { recordingId }),
};
```

- [ ] **Step 2: Compile-check**

```bash
npx tsc --noEmit
```

- [ ] **Step 3: Commit**

```bash
git add src/api/ask.ts
git commit -m "feat(ask): frontend invoke wrapper"
```

---

## Task 13: `useAsk` hook

**Files:**
- Create: `src/lib/useAsk.ts`
- Create: `src/lib/useAsk.test.ts`

A module-level `Map<recordingId, AskThread[]>` keeps threads alive while the user navigates between recordings. `useAsk(recordingId)` returns the current list of threads, plus `ask(question, threadId?)` and `cancel()` actions. Subscribers are notified via a tiny built-in event emitter.

- [ ] **Step 1: Write the failing tests**

Create `src/lib/useAsk.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook, act, waitFor } from "@testing-library/react";

vi.mock("../api/ask", () => ({
  askApi: {
    query: vi.fn(),
    cancel: vi.fn(),
  },
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import { askApi } from "../api/ask";
import { useAsk, __resetAskStoreForTests } from "./useAsk";

beforeEach(() => {
  __resetAskStoreForTests();
  vi.mocked(askApi.query).mockReset();
  vi.mocked(askApi.cancel).mockReset();
});

describe("useAsk", () => {
  it("starts with no threads", () => {
    const { result } = renderHook(() => useAsk("rec-1"));
    expect(result.current.threads).toEqual([]);
  });

  it("appends a new thread for a top-level question", async () => {
    vi.mocked(askApi.query).mockResolvedValue({
      text: "answer",
      citations: [{ quote: "q", start: 0, end: 1 }],
    });
    const { result } = renderHook(() => useAsk("rec-1"));
    await act(async () => { await result.current.ask("what?"); });
    expect(result.current.threads).toHaveLength(1);
    expect(result.current.threads[0].turns).toEqual([
      { role: "user", content: "what?" },
      expect.objectContaining({ role: "assistant", content: "answer" }),
    ]);
  });

  it("appends a follow-up to an existing thread when threadId is provided", async () => {
    vi.mocked(askApi.query).mockResolvedValueOnce({ text: "first", citations: [] });
    vi.mocked(askApi.query).mockResolvedValueOnce({ text: "second", citations: [] });
    const { result } = renderHook(() => useAsk("rec-1"));
    await act(async () => { await result.current.ask("Q1"); });
    const tid = result.current.threads[0].id;
    await act(async () => { await result.current.ask("Q2", tid); });
    expect(result.current.threads).toHaveLength(1);
    expect(result.current.threads[0].turns.map((t) => t.content)).toEqual(["Q1", "first", "Q2", "second"]);
  });

  it("preserves threads when switching recordings and back", async () => {
    vi.mocked(askApi.query).mockResolvedValue({ text: "a", citations: [] });
    const { result, rerender } = renderHook(({ id }) => useAsk(id), {
      initialProps: { id: "rec-1" },
    });
    await act(async () => { await result.current.ask("Q"); });
    rerender({ id: "rec-2" });
    expect(result.current.threads).toEqual([]);
    rerender({ id: "rec-1" });
    expect(result.current.threads).toHaveLength(1);
  });

  it("clears threads for the current recording", async () => {
    vi.mocked(askApi.query).mockResolvedValue({ text: "a", citations: [] });
    const { result } = renderHook(() => useAsk("rec-1"));
    await act(async () => { await result.current.ask("Q"); });
    act(() => { result.current.clearAll(); });
    expect(result.current.threads).toEqual([]);
  });

  it("forwards askApi errors as a failed assistant turn", async () => {
    vi.mocked(askApi.query).mockRejectedValue("Cannot reach Ollama");
    const { result } = renderHook(() => useAsk("rec-1"));
    await act(async () => { await result.current.ask("Q"); });
    const last = result.current.threads[0].turns.at(-1)!;
    expect(last.role).toBe("assistant");
    expect(last.content).toMatch(/Cannot reach Ollama/);
  });

  it("cancel() invokes askApi.cancel for the current recording", async () => {
    vi.mocked(askApi.cancel).mockResolvedValue(undefined);
    const { result } = renderHook(() => useAsk("rec-1"));
    await act(async () => { await result.current.cancel(); });
    expect(askApi.cancel).toHaveBeenCalledWith("rec-1");
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
npx vitest run src/lib/useAsk.test.ts
```

Expected: FAIL — `useAsk` is not defined.

- [ ] **Step 3: Implement the hook**

Create `src/lib/useAsk.ts`:

```ts
import { useEffect, useSyncExternalStore } from "react";
import { askApi } from "../api/ask";
import type { AskThread, AskTurn } from "../types/ask";

type Store = Map<string, AskThread[]>;

const store: Store = new Map();
const listeners = new Set<() => void>();

function emit() {
  for (const l of listeners) l();
}

function getThreads(recordingId: string): AskThread[] {
  return store.get(recordingId) ?? [];
}

function setThreads(recordingId: string, threads: AskThread[]) {
  store.set(recordingId, threads);
  emit();
}

function uid(): string {
  return Math.random().toString(36).slice(2) + Date.now().toString(36);
}

async function runQuery(
  recordingId: string,
  question: string,
  threadId: string | undefined,
) {
  const before = getThreads(recordingId);
  let target: AskThread;
  if (threadId) {
    const found = before.find((t) => t.id === threadId);
    if (!found) throw new Error(`thread not found: ${threadId}`);
    target = found;
  } else {
    target = { id: uid(), turns: [], pending: false };
  }

  // Optimistically add the user turn + mark pending.
  target = {
    ...target,
    pending: true,
    turns: [...target.turns, { role: "user", content: question }],
  };
  const next = threadId
    ? before.map((t) => (t.id === target.id ? target : t))
    : [...before, target];
  setThreads(recordingId, next);

  // History = all turns BEFORE this user turn.
  const history: AskTurn[] = target.turns.slice(0, -1);

  let assistant: AskTurn;
  try {
    const ans = await askApi.query(recordingId, question, history);
    assistant = { role: "assistant", content: ans.text, citations: ans.citations };
  } catch (e) {
    const msg = typeof e === "string" ? e : (e as Error)?.message ?? String(e);
    assistant = { role: "assistant", content: `Error: ${msg}` };
  }

  const after = getThreads(recordingId).map((t) =>
    t.id === target.id
      ? { ...t, pending: false, turns: [...t.turns, assistant], streamingText: undefined }
      : t,
  );
  setThreads(recordingId, after);
}

export function useAsk(recordingId: string) {
  const threads = useSyncExternalStore(
    (cb) => {
      listeners.add(cb);
      return () => { listeners.delete(cb); };
    },
    () => getThreads(recordingId),
    () => getThreads(recordingId),
  );

  // Streaming progress: update the active thread's `streamingText` as
  // ask-progress events arrive. Cleanup on unmount.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      unlisten = await listen<{ phase: string; chars?: number }>("ask-progress", (e) => {
        if (cancelled) return;
        if (e.payload.phase !== "answer" || typeof e.payload.chars !== "number") return;
        const cur = getThreads(recordingId);
        const idx = cur.findIndex((t) => t.pending);
        if (idx === -1) return;
        const updated = { ...cur[idx], streamingText: `(${e.payload.chars} chars…)` };
        setThreads(recordingId, [...cur.slice(0, idx), updated, ...cur.slice(idx + 1)]);
      });
    })();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [recordingId]);

  return {
    threads,
    ask: (question: string, threadId?: string) => runQuery(recordingId, question, threadId),
    cancel: () => askApi.cancel(recordingId),
    clearAll: () => setThreads(recordingId, []),
  };
}

/** @internal — for tests */
export function __resetAskStoreForTests() {
  store.clear();
  listeners.clear();
}
```

- [ ] **Step 4: Run the tests**

```bash
npx vitest run src/lib/useAsk.test.ts
```

Expected: 7 PASS.

- [ ] **Step 5: Commit**

```bash
git add src/lib/useAsk.ts src/lib/useAsk.test.ts
git commit -m "feat(ask): useAsk hook with session-only per-recording threads"
```

---

## Task 14: `OutputPreview` highlight prop

**Files:**
- Modify: `src/components/OutputPreview.tsx`

Adds an optional `highlightRange?: { start: number; end: number; nonce?: number }`. When `highlightRange` is set (or the nonce changes), the component forces the tab to "transcript", scrolls the offset into view, and applies a `bg-status-info/30` class to the matched span for ~1.2 seconds. `nonce` is included so re-clicking the same citation re-fires the highlight.

- [ ] **Step 1: Update the component**

Replace `src/components/OutputPreview.tsx`:

```tsx
import { useEffect, useRef, useState } from "react";
import { SegmentedControl } from "./ui";

interface Props {
  transcript?: string | null;
  formatted?: string | null;
  savedPaths?: string[] | null;
  highlightRange?: { start: number; end: number; nonce?: number };
}

export default function OutputPreview({
  transcript,
  formatted,
  savedPaths,
  highlightRange,
}: Props) {
  const has = (v?: string | null) => (v ?? "").length > 0;
  const hasFormatted = has(formatted);
  const hasTranscript = has(transcript);
  const [tab, setTab] = useState<"formatted" | "transcript">(
    hasFormatted ? "formatted" : "transcript",
  );

  useEffect(() => {
    if (tab === "formatted" && !hasFormatted && hasTranscript) setTab("transcript");
    else if (tab === "transcript" && !hasTranscript && hasFormatted) setTab("formatted");
  }, [tab, hasFormatted, hasTranscript]);

  // When a citation comes in, force the transcript tab and scroll to it.
  const highlightRef = useRef<HTMLSpanElement | null>(null);
  const [flash, setFlash] = useState(false);
  useEffect(() => {
    if (!highlightRange) return;
    setTab("transcript");
    setFlash(true);
    const t = window.setTimeout(() => setFlash(false), 1200);
    // Defer scroll until after layout in case the tab just changed.
    const r = window.requestAnimationFrame(() => {
      highlightRef.current?.scrollIntoView({ block: "center", behavior: "smooth" });
    });
    return () => {
      window.clearTimeout(t);
      window.cancelAnimationFrame(r);
    };
  }, [highlightRange?.start, highlightRange?.end, highlightRange?.nonce]);

  if (!hasTranscript && !hasFormatted) return null;

  const renderTranscript = () => {
    const text = transcript ?? "";
    if (!highlightRange || tab !== "transcript") return text;
    const { start, end } = highlightRange;
    if (start < 0 || end > text.length || start >= end) return text;
    return (
      <>
        {text.slice(0, start)}
        <span
          ref={highlightRef}
          className={
            flash
              ? "bg-status-info/30 transition-colors duration-300"
              : "bg-status-info/10 transition-colors duration-300"
          }
          data-testid="ask-citation-highlight"
        >
          {text.slice(start, end)}
        </span>
        {text.slice(end)}
      </>
    );
  };

  return (
    <div className="flex flex-col gap-3">
      <SegmentedControl
        size="sm"
        value={tab}
        onValueChange={(v) => setTab(v as "formatted" | "transcript")}
        aria-label="Output view"
      >
        <SegmentedControl.Item value="formatted" disabled={!hasFormatted}>
          Formatted
        </SegmentedControl.Item>
        <SegmentedControl.Item value="transcript" disabled={!hasTranscript}>
          Transcript
        </SegmentedControl.Item>
      </SegmentedControl>
      <pre className="bg-control-background border border-separator rounded-xl p-4 text-callout font-mono text-label-primary whitespace-pre-wrap max-h-96 overflow-y-auto scrollable">
        {tab === "formatted" ? (formatted ?? "") : renderTranscript()}
      </pre>
      {savedPaths && savedPaths.length > 0 && (
        <div className="text-footnote text-label-secondary">
          Saved to:
          <ul className="mt-1">
            {savedPaths.map((p) => <li key={p} className="truncate">· {p}</li>)}
          </ul>
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Compile-check**

```bash
npx tsc --noEmit
```

Expected: no errors. The component is consumed only by `RecordingView.tsx`; the new prop is optional so existing call sites still type-check.

- [ ] **Step 3: Commit**

```bash
git add src/components/OutputPreview.tsx
git commit -m "feat(output-preview): support citation highlight + scroll"
```

---

## Task 15: `chat` icon

**Files:**
- Modify: `src/components/ui/Icon.tsx`

- [ ] **Step 1: Add the icon entry**

In `src/components/ui/Icon.tsx`:

1. Add `ChatCircle` to the existing `import { … } from "@phosphor-icons/react"` block.
2. Extend the `IconName` union with `| "chat"`.
3. Extend the `REGISTRY` object with `chat: ChatCircle,`.

- [ ] **Step 2: Verify the icon component still passes its tests**

```bash
npx vitest run src/components/ui/__tests__/Icon.test.tsx
```

Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add src/components/ui/Icon.tsx
git commit -m "feat(icon): register chat icon"
```

---

## Task 16: `AskPanel` component

**Files:**
- Create: `src/components/AskPanel.tsx`
- Create: `src/components/AskPanel.test.tsx`

The panel renders the per-recording threads, an input, a send/stop button, and (per assistant turn) clickable citation pills that call back to the parent.

- [ ] **Step 1: Write the failing tests**

Create `src/components/AskPanel.test.tsx`:

```tsx
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../api/ask", () => ({
  askApi: { query: vi.fn(), cancel: vi.fn() },
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import { askApi } from "../api/ask";
import { __resetAskStoreForTests } from "../lib/useAsk";
import AskPanel from "./AskPanel";

beforeEach(() => {
  __resetAskStoreForTests();
  vi.mocked(askApi.query).mockReset();
  vi.mocked(askApi.cancel).mockReset();
});

describe("AskPanel", () => {
  it("shows the empty-state hint when there are no threads", () => {
    render(<AskPanel recordingId="rec-1" disabled={false} onCitationClick={() => {}} />);
    expect(screen.getByText(/Ask anything about this transcript/i)).toBeInTheDocument();
  });

  it("disables the send button when input is whitespace", async () => {
    render(<AskPanel recordingId="rec-1" disabled={false} onCitationClick={() => {}} />);
    const send = screen.getByRole("button", { name: /send/i });
    expect(send).toBeDisabled();
    await userEvent.type(screen.getByRole("textbox"), "   ");
    expect(send).toBeDisabled();
    await userEvent.type(screen.getByRole("textbox"), "hi");
    expect(send).toBeEnabled();
  });

  it("renders an answer with clickable citation pills", async () => {
    vi.mocked(askApi.query).mockResolvedValue({
      text: "It happened in Q3.",
      citations: [{ quote: "Q3 budget review", start: 5, end: 21 }],
    });
    const onCite = vi.fn();
    render(<AskPanel recordingId="rec-1" disabled={false} onCitationClick={onCite} />);
    await userEvent.type(screen.getByRole("textbox"), "when?");
    await userEvent.click(screen.getByRole("button", { name: /send/i }));
    expect(await screen.findByText(/It happened in Q3/i)).toBeInTheDocument();
    // The pill renders as a `<button>` whose accessible name is the quote
    // wrapped in curly quotes — match by the inner text via regex.
    const pill = screen.getByRole("button", { name: /Q3 budget review/i });
    await userEvent.click(pill);
    expect(onCite).toHaveBeenCalledWith({ quote: "Q3 budget review", start: 5, end: 21 });
  });

  it("renders unmatched citations as non-clickable", async () => {
    vi.mocked(askApi.query).mockResolvedValue({
      text: "answer.",
      citations: [{ quote: "untraceable", start: null, end: null }],
    });
    render(<AskPanel recordingId="rec-1" disabled={false} onCitationClick={() => {}} />);
    await userEvent.type(screen.getByRole("textbox"), "any?");
    await userEvent.click(screen.getByRole("button", { name: /send/i }));
    await screen.findByText(/answer\./);
    // The unmatched pill is rendered as a plain element, not a button. The
    // visible text is wrapped in curly quotes, so use a regex matcher.
    expect(screen.queryByRole("button", { name: /untraceable/i })).toBeNull();
    expect(screen.getByText(/untraceable/)).toBeInTheDocument();
  });

  it("disables input when the recording has no transcript yet", () => {
    render(<AskPanel recordingId="rec-1" disabled={true} onCitationClick={() => {}} />);
    expect(screen.getByRole("textbox")).toBeDisabled();
    expect(screen.getByRole("button", { name: /send/i })).toBeDisabled();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
npx vitest run src/components/AskPanel.test.tsx
```

Expected: FAIL — `AskPanel` is not defined.

- [ ] **Step 3: Implement `AskPanel`**

Create `src/components/AskPanel.tsx`:

```tsx
import { useState } from "react";
import { Button, Input } from "./ui";
import { useAsk } from "../lib/useAsk";
import type { AskCitation } from "../types/ask";

interface Props {
  recordingId: string;
  disabled: boolean;
  onCitationClick: (c: AskCitation) => void;
}

export default function AskPanel({ recordingId, disabled, onCitationClick }: Props) {
  const { threads, ask, cancel, clearAll } = useAsk(recordingId);
  const [input, setInput] = useState("");

  const trimmed = input.trim();
  const anyPending = threads.some((t) => t.pending);
  const canSend = !disabled && trimmed.length > 0 && !anyPending;

  const submit = async (threadId?: string) => {
    if (!canSend) return;
    const q = trimmed;
    setInput("");
    await ask(q, threadId);
  };

  return (
    <aside className="w-[360px] flex flex-col gap-3 border-l border-separator pl-4" aria-label="Ask AI">
      <header className="flex items-center justify-between">
        <h3 className="text-headline font-semibold">Ask</h3>
        {threads.length > 0 && (
          <Button variant="secondary" size="sm" onClick={clearAll}>
            Clear all
          </Button>
        )}
      </header>

      {threads.length === 0 && !disabled && (
        <p className="text-footnote text-label-secondary">
          Ask anything about this transcript — e.g. <em>what was said about project financials?</em>
        </p>
      )}

      <ol className="flex flex-col gap-4 max-h-[60vh] overflow-y-auto scrollable">
        {threads.map((thread) => (
          <li key={thread.id} className="flex flex-col gap-2">
            {thread.turns.map((turn, idx) => (
              <div key={idx} className="flex flex-col gap-1">
                {turn.role === "user" ? (
                  <div className="text-callout text-label-secondary">
                    <span className="font-semibold">You:</span> {turn.content}
                  </div>
                ) : (
                  <div className="text-callout text-label-primary whitespace-pre-wrap">
                    {turn.content}
                    {turn.citations && turn.citations.length > 0 && (
                      <div className="mt-2 flex flex-col gap-1">
                        {turn.citations.map((c, i) => {
                          const matched = c.start !== null && c.end !== null;
                          if (matched) {
                            return (
                              <button
                                key={i}
                                type="button"
                                onClick={() => onCitationClick(c)}
                                className="text-left text-footnote bg-control-background border border-separator rounded-md px-2 py-1 hover:border-tint transition-colors"
                              >
                                “{c.quote}”
                              </button>
                            );
                          }
                          return (
                            <span
                              key={i}
                              title="couldn't locate in transcript"
                              className="text-left text-footnote bg-control-background border border-separator rounded-md px-2 py-1 text-label-tertiary cursor-help"
                            >
                              “{c.quote}”
                            </span>
                          );
                        })}
                      </div>
                    )}
                  </div>
                )}
              </div>
            ))}
            {thread.pending && (
              <div className="text-footnote text-label-secondary">
                {thread.streamingText ?? "Answering…"}
              </div>
            )}
            {!thread.pending && (
              <div>
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => submit(thread.id)}
                  disabled={!canSend}
                >
                  Follow up
                </Button>
              </div>
            )}
          </li>
        ))}
      </ol>

      <form
        className="mt-auto flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <Input
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder="ask the transcript…"
          disabled={disabled || anyPending}
          aria-label="Ask the transcript"
        />
        {anyPending ? (
          <Button type="button" variant="destructive" size="md" onClick={() => void cancel()}>
            Stop
          </Button>
        ) : (
          <Button type="submit" variant="primary" size="md" disabled={!canSend}>
            Send
          </Button>
        )}
      </form>
    </aside>
  );
}
```

- [ ] **Step 4: Run the panel tests**

```bash
npx vitest run src/components/AskPanel.test.tsx
```

Expected: 5 PASS.

- [ ] **Step 5: Commit**

```bash
git add src/components/AskPanel.tsx src/components/AskPanel.test.tsx
git commit -m "feat(ask): AskPanel component with cited answers"
```

---

## Task 17: Wire `AskPanel` into `RecordingView`

**Files:**
- Modify: `src/components/RecordingView.tsx`

The Ask button toggles a piece of local state. When open, the panel renders to the right of `OutputPreview` and a citation click sets a `highlightRange` (with a bumped nonce) that flows to `OutputPreview`. ⌘K toggles the panel.

- [ ] **Step 1: Add state and the Ask button**

In `src/components/RecordingView.tsx`:

1. Near the existing `useState` declarations, add:

```tsx
const [askOpen, setAskOpen] = useState(false);
const [highlightRange, setHighlightRange] = useState<
  { start: number; end: number; nonce: number } | undefined
>(undefined);
```

2. Compute whether ask is available:

```tsx
const transcriptStage = rec?.stages.find((s) => s.stage === "transcribe");
const hasTranscript = transcriptStage?.status === "completed" && (transcriptStage.output?.length ?? 0) > 0;
```

3. Add a Toolbar button (next to Start over / Run / Stop) — when the recording exists:

```tsx
<Button
  variant="secondary"
  size="md"
  disabled={!hasTranscript}
  title={hasTranscript ? "Ask about this transcript (⌘K)" : "Transcribe first"}
  onClick={() => setAskOpen((v) => !v)}
>
  <Icon name="chat" size="md" /> Ask
</Button>
```

(Adjust to whatever Toolbar/Button arrangement currently exists; the goal is a visible toggle.)

4. Add the keyboard shortcut:

```tsx
useEffect(() => {
  const onKey = (e: KeyboardEvent) => {
    if (e.metaKey && e.key.toLowerCase() === "k") {
      if (!hasTranscript) return;
      e.preventDefault();
      setAskOpen((v) => !v);
    }
  };
  window.addEventListener("keydown", onKey);
  return () => window.removeEventListener("keydown", onKey);
}, [hasTranscript]);
```

5. In the render tree, wrap the existing `OutputPreview` and the new `AskPanel` in a flex row when `askOpen`:

```tsx
<div className={askOpen ? "flex gap-4" : ""}>
  <div className="flex-1">
    <OutputPreview
      transcript={transcriptStage?.output ?? ""}
      formatted={rec?.stages.find((s) => s.stage === "format")?.output ?? ""}
      savedPaths={rec?.stages.find((s) => s.stage === "save")?.savedPaths}
      highlightRange={highlightRange}
    />
  </div>
  {askOpen && rec && (
    <AskPanel
      recordingId={rec.id}
      disabled={!hasTranscript}
      onCitationClick={(c) => {
        if (c.start === null || c.end === null) return;
        setHighlightRange({ start: c.start, end: c.end, nonce: Date.now() });
      }}
    />
  )}
</div>
```

(Adjust the JSX to match the actual existing structure around `OutputPreview`. The key change is that `OutputPreview` now receives `highlightRange`, and `AskPanel` lives next to it when open.)

6. At the top of the file:

```tsx
import AskPanel from "./AskPanel";
import { Icon } from "./ui";
```

(`Icon` may already be imported.)

- [ ] **Step 2: Compile-check**

```bash
npx tsc --noEmit
```

Expected: no errors.

- [ ] **Step 3: Run the full frontend test suite**

```bash
npx vitest run
```

Expected: all previously-passing tests still pass; the new `useAsk` and `AskPanel` tests still pass.

- [ ] **Step 4: Commit**

```bash
git add src/components/RecordingView.tsx
git commit -m "feat(ask): mount AskPanel + citation highlight in RecordingView"
```

---

## Task 18: Manual end-to-end verification

This task does not produce a commit — it verifies the integrated feature against the spec's testing checklist.

- [ ] **Step 1: Build the dev app**

```bash
npm run tauri dev
```

Wait for the window to open. Ensure Ollama is running locally (`ollama serve`) and the configured model is pulled (e.g. `ollama pull gemma3`).

- [ ] **Step 2: Verify the empty-state path**

1. Open or create a recording with no transcript yet.
2. Confirm the Ask button is **disabled** with a "Transcribe first" tooltip.

- [ ] **Step 3: Verify the happy path (short transcript → path A)**

1. Open a recording with a completed transcript shorter than ~16k chars.
2. Click Ask (or press ⌘K). The panel opens with the empty-state hint.
3. Type *"what was said about [some topic from your transcript]?"* and submit.
4. Watch the answer stream in.
5. After streaming completes, click any citation pill. Confirm the preview switches to the **Transcript** tab, scrolls to the cited region, and flashes a highlight.

- [ ] **Step 4: Verify the long-transcript path (path B)**

1. Open a recording with a transcript longer than ~16k chars.
2. Submit a question.
3. Confirm the panel shows *"Searching transcript…"* (or equivalent) progress per chunk, then *"Answering…"*.
4. Confirm a final answer renders with citations.

- [ ] **Step 5: Verify cancellation**

1. Submit a long question.
2. Click **Stop** while the answer is streaming.
3. Confirm the panel returns to idle, the user's question stays in the thread, and the partial assistant turn is dropped.

- [ ] **Step 6: Verify follow-up**

1. After an answer arrives, click **Follow up**, type a related question, submit.
2. Confirm a second Q/A pair appears under the same thread, and that the model answers in context.

- [ ] **Step 7: Verify session-only persistence**

1. Submit a question, get an answer.
2. Quit the app fully and relaunch.
3. Open the same recording. Confirm the thread is **gone** (this confirms session-only persistence per the spec).

- [ ] **Step 8: Verify error path**

1. Stop Ollama (`pkill ollama` or quit the app).
2. Submit a question.
3. Confirm an inline error appears in the panel (e.g. *"Error: Cannot reach Ollama at …"*) and the question stays so the user can retry.

- [ ] **Step 9: Verify recording switch mid-stream**

1. Submit a question on recording A.
2. While streaming, switch to recording B in the sidebar.
3. Switch back to recording A. Confirm the thread is preserved (either still streaming if Tauri kept the run alive, or showing the partial state cleanly).

If any step fails, file the discrepancy as a follow-up task and fix before merging.

---

## Self-Review Notes

Spec coverage cross-check:

- Side-panel UX → Task 16 (panel) + Task 17 (mount).
- Empty state, ⌘K, Ask button enabled-only-with-transcript → Task 17.
- Single-shot path A → Task 7.
- Map-shortlist + reduce path B → Task 8.
- `pick_strategy` boundary deterministic → Task 5.
- Verbatim citations + whitespace tolerance + non-clickable on miss → Task 4 (matcher) + Task 16 (UI).
- Cancellation parity (during-await) → Task 2 (`ollama_stream`'s `tokio::select!`) + Task 9 (registry) + Task 10 (`cancel_ask`) + Task 18 step 5.
- `--- citations ---` block parsing → Task 6.
- Streaming UI + "Searching…/Answering…" phases → Task 10 (events) + Task 13 (hook) + Task 16 (panel).
- Session-only `Map<recordingId, AskThread[]>` → Task 13 + Task 18 step 7.
- No DB schema changes → confirmed; storage is touched only to read `transcribe.output` (Task 10).
- Out-of-scope items are not implemented (no DB tables, no embeddings, no formatted-output asks).
