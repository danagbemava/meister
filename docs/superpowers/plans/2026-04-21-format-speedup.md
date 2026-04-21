# Format Speedup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reduce end-to-end Format stage wall time on long transcripts by (1) applying easy wins on every call (warm model, explicit ctx sizing, filler stripping, output cache) and (2) switching long transcripts to a parallel map-reduce pipeline over chunked notes extraction.

**Architecture:** The Format stage stays a single atomic stage. `run_format`'s core logic is extracted into `run_format_core(transcript, template, ollama_url, model, cache_dir, on_progress)` — a pure(-ish) function that is unit-testable without `AppHandle`. The public `run_format` wrapper resolves `cache_dir` from `AppHandle` and forwards `format-progress` events via `app.emit`. Internally, `run_format_core` dispatches: cache-hit fast path → `preprocess` → threshold check → either a single streamed Ollama call or a map-reduce (`buffer_unordered(3)` chunk notes + one reduce pass).

**Tech Stack:** Rust (tokio, reqwest, futures-util, sha2), Tauri 2, TypeScript/React frontend. No new Cargo dependencies.

**Spec:** `docs/superpowers/specs/2026-04-21-format-speedup-design.md`

---

## File Structure

**Modified:**
- `src-tauri/src/commands.rs` — rewrite `run_format` as a thin wrapper; add `run_format_core`, `preprocess`, `split_into_chunks`, `cache_key`, `cache_read`, `cache_write`, `map_pass`, `reduce_pass`, `single_pass` helpers and the widened `FormatProgress` / `FormatPhase` / `ChunkInfo` types. All unit + integration tests live in `#[cfg(test)] mod tests` at the bottom of this file.
- `src/components/RecordingView.tsx` — widen `format-progress` payload type; branch the progress render on `phase`.
- `SETUP.md` — add `OLLAMA_NUM_PARALLEL=3` guidance.

**Not modified:** `storage/*`, `orchestrator.rs` (callsite signature stable), `transcription/*`, Cargo.toml.

---

## Task 1: Preprocessing (filler stripping)

**Files:**
- Modify: `src-tauri/src/commands.rs` (add `preprocess` function + tests)

- [ ] **Step 1: Write the failing tests**

Add to `#[cfg(test)] mod tests { ... }` at the bottom of `commands.rs`. If the module does not exist, create it:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preprocess_strips_standalone_fillers() {
        let input = "Um, so I think, uh, we should go. Ah, okay.";
        let out = preprocess(input);
        assert!(!out.to_lowercase().contains(" um"));
        assert!(!out.to_lowercase().contains(" uh"));
        assert!(!out.to_lowercase().contains(" ah"));
        assert!(out.contains("I think"));
        assert!(out.contains("we should go"));
    }

    #[test]
    fn preprocess_preserves_substrings_that_look_like_fillers() {
        let input = "Uber driver brought an umbrella. Also: Ahmad came.";
        let out = preprocess(input);
        assert!(out.contains("Uber"));
        assert!(out.contains("umbrella"));
        assert!(out.contains("Ahmad"));
    }

    #[test]
    fn preprocess_collapses_3plus_stutters() {
        assert_eq!(preprocess("I I I went"), "I went");
        assert_eq!(preprocess("the the the the end"), "the end");
    }

    #[test]
    fn preprocess_keeps_2x_repeats_as_emphasis() {
        assert_eq!(preprocess("very very good"), "very very good");
    }

    #[test]
    fn preprocess_normalizes_whitespace_preserving_paragraph_breaks() {
        let input = "line one\nstill one.\n\n\nnew para.";
        let out = preprocess(input);
        assert_eq!(out, "line one still one.\n\nnew para.");
    }

    #[test]
    fn preprocess_normalizes_dashes_and_ellipsis() {
        assert_eq!(preprocess("first — second … third"), "first - second ... third");
    }

    #[test]
    fn preprocess_is_idempotent() {
        let input = "Um, so — I I I said, uhm, maybe…\n\n\nThat's it.";
        let once = preprocess(input);
        let twice = preprocess(&once);
        assert_eq!(once, twice);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p meister preprocess_ --manifest-path src-tauri/Cargo.toml`
Expected: compile error (`preprocess` not defined) or all tests FAIL.

- [ ] **Step 3: Implement `preprocess`**

Add near the top of `commands.rs`, after the existing `// ── Types ──` block and before the format-related section:

```rust
// ── Formatting pipeline ────────────────────────────────────────────────────────

/// Conservative text cleanup applied before sending a transcript to the
/// formatter. Strips well-known filler tokens and 3+ stutters; normalizes
/// whitespace while preserving paragraph breaks; normalizes em-dashes and
/// ellipses. Idempotent.
pub(crate) fn preprocess(input: &str) -> String {
    use std::sync::OnceLock;
    static FILLER_RE: OnceLock<regex::Regex> = OnceLock::new();
    static STUTTER_RE: OnceLock<regex::Regex> = OnceLock::new();
    static PARA_RE: OnceLock<regex::Regex> = OnceLock::new();
    static WS_RE: OnceLock<regex::Regex> = OnceLock::new();

    // Step 1: normalize dashes and ellipsis so later patterns are simpler.
    let mut s = input.replace('—', "-").replace('…', "...");

    // Step 2: strip standalone fillers. `(?i)` = case-insensitive.
    // `\b` boundaries mean "Uber", "umbrella", "Ahmad" are safe.
    let filler = FILLER_RE.get_or_init(|| {
        regex::Regex::new(r"(?i)\b(uh|um|uhm|er|erm|hmm|ah)\b[,.!?]?\s*").unwrap()
    });
    s = filler.replace_all(&s, "").to_string();

    // Step 3: collapse 3+ same-word stutters to a single instance.
    // Case-insensitive; captures first occurrence as the survivor.
    let stutter = STUTTER_RE.get_or_init(|| {
        regex::Regex::new(r"(?i)\b(\w+)(?:[,\s]+\1\b){2,}").unwrap()
    });
    s = stutter.replace_all(&s, "$1").to_string();

    // Step 4: paragraph breaks → exactly "\n\n"; other whitespace runs → " ".
    let para = PARA_RE.get_or_init(|| regex::Regex::new(r"\n\s*\n\s*").unwrap());
    s = para.replace_all(&s, "\u{0000}").to_string(); // placeholder for para break
    let ws = WS_RE.get_or_init(|| regex::Regex::new(r"\s+").unwrap());
    s = ws.replace_all(&s, " ").to_string();
    s = s.replace('\u{0000}', "\n\n");

    s.trim().to_string()
}
```

Note on dependencies: `regex` is not currently a top-level dep. Add to `src-tauri/Cargo.toml` under `[dependencies]`:

```toml
regex = "1"
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p meister preprocess_ --manifest-path src-tauri/Cargo.toml`
Expected: all 7 `preprocess_*` tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/commands.rs
git commit -m "format: add preprocess() filler-stripping helper"
```

---

## Task 2: Chunk splitting

**Files:**
- Modify: `src-tauri/src/commands.rs` (add `split_into_chunks` + constants + tests)

- [ ] **Step 1: Write the failing tests**

Append to the `tests` module:

```rust
    #[test]
    fn split_short_input_returns_single_chunk() {
        let input = "Short transcript. Not much here.";
        let chunks = split_into_chunks(input);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], input);
    }

    #[test]
    fn split_long_input_produces_multiple_chunks() {
        // ~24k chars ≈ 6000 tokens → ≥ 2 chunks given CHUNK_TARGET=3000.
        let sentence = "This is a sentence with some length to it. ";
        let input: String = sentence.repeat(550); // ~24k chars
        let chunks = split_into_chunks(&input);
        assert!(chunks.len() >= 2, "expected ≥2 chunks, got {}", chunks.len());
        // Each chunk should be within target+search-window+overlap slack.
        for (i, c) in chunks.iter().enumerate() {
            assert!(
                c.len() <= (CHUNK_TARGET + CHUNK_OVERLAP + 500) * 4 + 50,
                "chunk {i} too large: {} chars", c.len()
            );
        }
    }

    #[test]
    fn split_prefers_sentence_boundaries() {
        // Build input where the target split lands mid-sentence, with a
        // clean '. ' a few hundred chars before the target.
        let head = "A".repeat(3000 * 4 - 300); // well before target (chars, not tokens)
        let tail = ". Then more content here to fill space. ".repeat(200);
        let input = format!("{head}. Clean boundary here. {tail}");
        let chunks = split_into_chunks(&input);
        assert!(chunks.len() >= 2);
        // Chunk 0 should end with sentence-ending punctuation + space (or be trimmed to end with '.').
        let c0 = chunks[0].trim_end();
        assert!(
            c0.ends_with('.') || c0.ends_with('?') || c0.ends_with('!'),
            "chunk 0 ends with: {:?}",
            &c0[c0.len().saturating_sub(40)..],
        );
    }

    #[test]
    fn split_wall_of_text_falls_back_to_whitespace() {
        // No sentence punctuation at all; split should still find a space.
        let word = "word ";
        let input = word.repeat(4000); // ~20k chars, no punctuation
        let chunks = split_into_chunks(&input);
        assert!(chunks.len() >= 2);
        // No chunk should end mid-word.
        for (i, c) in chunks.iter().enumerate() {
            let trimmed = c.trim_end();
            assert!(trimmed.ends_with("word"), "chunk {i} ends mid-word: {:?}", &trimmed[trimmed.len().saturating_sub(20)..]);
        }
    }

    #[test]
    fn split_overlap_prefix_present_on_later_chunks() {
        let sentence = "Sentence number X is here. ";
        let input: String = (0..1200).map(|i| sentence.replace('X', &i.to_string())).collect();
        let chunks = split_into_chunks(&input);
        assert!(chunks.len() >= 2);
        // Chunk 1 should start with some content that appears near the end of chunk 0.
        let tail0_end = &chunks[0][chunks[0].len().saturating_sub(CHUNK_OVERLAP * 4)..];
        // Pick a distinctive word near the end of chunk 0 and check it appears at the start of chunk 1.
        let words: Vec<&str> = tail0_end.split_whitespace().collect();
        if let Some(marker) = words.iter().rev().find(|w| w.len() > 3).copied() {
            let head1 = &chunks[1][..chunks[1].len().min(CHUNK_OVERLAP * 4 + 100)];
            assert!(head1.contains(marker), "chunk 1 head missing overlap marker {marker:?}");
        }
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p meister split_ --manifest-path src-tauri/Cargo.toml`
Expected: compile error (`split_into_chunks`/`CHUNK_TARGET` not defined).

- [ ] **Step 3: Implement `split_into_chunks` + constants**

Add in `commands.rs`, near `preprocess`:

```rust
/// Token-count thresholds (approx: 1 token ≈ 4 chars).
pub(crate) const THRESHOLD: usize = 4000;
pub(crate) const CHUNK_TARGET: usize = 3000;
pub(crate) const CHUNK_OVERLAP: usize = 200;
const CHARS_PER_TOKEN: usize = 4;
const BOUNDARY_SEARCH_WINDOW_CHARS: usize = 500;

fn approx_tokens(s: &str) -> usize {
    s.chars().count() / CHARS_PER_TOKEN
}

/// Split a cleaned transcript into chunks at sentence boundaries, with a
/// small overlap prefix on each chunk after the first. Single-chunk if the
/// input is below THRESHOLD tokens.
pub(crate) fn split_into_chunks(input: &str) -> Vec<String> {
    if approx_tokens(input) < THRESHOLD {
        return vec![input.to_string()];
    }

    let target_chars = CHUNK_TARGET * CHARS_PER_TOKEN;
    let overlap_chars = CHUNK_OVERLAP * CHARS_PER_TOKEN;
    let search_window = BOUNDARY_SEARCH_WINDOW_CHARS;

    let bytes = input.as_bytes();
    let len = bytes.len();

    let mut chunks: Vec<String> = Vec::new();
    let mut start: usize = 0; // byte index

    while start < len {
        let remaining = &input[start..];
        if remaining.len() <= target_chars + search_window {
            chunks.push(prepend_overlap(&chunks, remaining));
            break;
        }

        // Target index within `remaining`.
        let target = target_chars.min(remaining.len());

        // Search for a sentence terminator (.?!) followed by whitespace in
        // [target - search_window, target + search_window].
        let lo = target.saturating_sub(search_window);
        let hi = (target + search_window).min(remaining.len());
        let window = &remaining[lo..hi];

        let split_rel = find_last_sentence_boundary(window)
            .or_else(|| find_last_whitespace(window))
            .unwrap_or(window.len());

        let split_abs = lo + split_rel;
        let body = &remaining[..split_abs];
        chunks.push(prepend_overlap(&chunks, body));

        // Advance. Skip any whitespace at the split point so the next chunk
        // starts cleanly.
        let mut next = start + split_abs;
        while next < len && matches!(bytes[next], b' ' | b'\t' | b'\n' | b'\r') {
            next += 1;
        }
        start = next;
    }

    chunks
        .into_iter()
        .filter(|c| !c.trim().is_empty())
        .collect()
}

/// Returns the byte index (within `s`) of the character just after the last
/// sentence-terminator+whitespace sequence, or None if none found.
fn find_last_sentence_boundary(s: &str) -> Option<usize> {
    let mut last: Option<usize> = None;
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        let c = bytes[i];
        if (c == b'.' || c == b'?' || c == b'!')
            && matches!(bytes[i + 1], b' ' | b'\t' | b'\n')
        {
            last = Some(i + 2);
        }
        i += 1;
    }
    last
}

fn find_last_whitespace(s: &str) -> Option<usize> {
    s.rfind(|c: char| c.is_whitespace()).map(|i| i + 1)
}

/// Prepend `CHUNK_OVERLAP` tokens' worth of tail from the last existing chunk
/// (if any) to `body`, yielding the next chunk's content.
fn prepend_overlap(existing: &[String], body: &str) -> String {
    let Some(prev) = existing.last() else {
        return body.trim_start().to_string();
    };
    let overlap_chars = CHUNK_OVERLAP * CHARS_PER_TOKEN;
    let tail_start = prev.len().saturating_sub(overlap_chars);
    // Snap to a char boundary.
    let tail_start = (tail_start..=prev.len())
        .find(|&i| prev.is_char_boundary(i))
        .unwrap_or(prev.len());
    let tail = &prev[tail_start..];
    format!("{tail}\n\n{}", body.trim_start())
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p meister split_ --manifest-path src-tauri/Cargo.toml`
Expected: all 5 `split_*` tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "format: add split_into_chunks() with sentence-boundary preference"
```

---

## Task 3: Cache key + disk read/write

**Files:**
- Modify: `src-tauri/src/commands.rs` (add cache helpers + tests)

- [ ] **Step 1: Write the failing tests**

Append to `tests` module:

```rust
    #[test]
    fn cache_key_is_stable_for_same_inputs() {
        let a = cache_key("transcript", "template", "gemma3");
        let b = cache_key("transcript", "template", "gemma3");
        assert_eq!(a, b);
        assert_eq!(a.len(), 64, "sha256 hex should be 64 chars");
    }

    #[test]
    fn cache_key_differs_on_any_input_change() {
        let base = cache_key("t", "tpl", "m");
        assert_ne!(base, cache_key("t2", "tpl", "m"));
        assert_ne!(base, cache_key("t", "tpl2", "m"));
        assert_ne!(base, cache_key("t", "tpl", "m2"));
    }

    #[test]
    fn cache_read_missing_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let out = cache_read(tmp.path(), "nope");
        assert!(out.is_none());
    }

    #[test]
    fn cache_write_then_read_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        cache_write(tmp.path(), "abc123", "hello world").unwrap();
        let got = cache_read(tmp.path(), "abc123").unwrap();
        assert_eq!(got, "hello world");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p meister cache_ --manifest-path src-tauri/Cargo.toml`
Expected: compile error (`cache_key`/`cache_read`/`cache_write` not defined).

- [ ] **Step 3: Implement cache helpers**

Add in `commands.rs`:

```rust
use sha2::{Digest, Sha256};
use std::path::Path;

/// SHA-256 over `transcript || 0x00 || template || 0x00 || model` → hex.
pub(crate) fn cache_key(transcript: &str, template: &str, model: &str) -> String {
    let mut h = Sha256::new();
    h.update(transcript.as_bytes());
    h.update([0u8]);
    h.update(template.as_bytes());
    h.update([0u8]);
    h.update(model.as_bytes());
    let digest = h.finalize();
    let mut out = String::with_capacity(64);
    for b in digest.iter() {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

pub(crate) fn cache_read(cache_dir: &Path, key: &str) -> Option<String> {
    let path = cache_dir.join(format!("{key}.md"));
    std::fs::read_to_string(&path).ok()
}

pub(crate) fn cache_write(cache_dir: &Path, key: &str, content: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(cache_dir)?;
    std::fs::write(cache_dir.join(format!("{key}.md")), content)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p meister cache_ --manifest-path src-tauri/Cargo.toml`
Expected: all 4 `cache_*` tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "format: add cache_key + cache_read/cache_write helpers"
```

---

## Task 4: FormatProgress types + phase enum

**Files:**
- Modify: `src-tauri/src/commands.rs` (replace the existing `FormatProgress` struct)

- [ ] **Step 1: Replace the `FormatProgress` struct**

Find the existing block (around `commands.rs:230`):

```rust
#[derive(Clone, serde::Serialize)]
struct FormatProgress {
    chars: usize,
    done: bool,
}
```

Replace with:

```rust
#[derive(Clone, Copy, serde::Serialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub(crate) enum FormatPhase {
    Single,
    Map,
    Reduce,
}

#[derive(Clone, serde::Serialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChunkInfo {
    pub index: usize,
    pub total: usize,
    pub completed: usize,
}

#[derive(Clone, serde::Serialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FormatProgress {
    pub phase: FormatPhase,
    pub chunk: Option<ChunkInfo>,
    pub chars: usize,
    pub done: bool,
}
```

- [ ] **Step 2: Verify the crate still compiles**

Run: `cargo check -p meister --manifest-path src-tauri/Cargo.toml`
Expected: compile errors pointing at the old `FormatProgress { chars, done }` construction inside `run_format`. These are fixed in Task 5/8.

- [ ] **Step 3: Commit (deferred)**

Leave uncommitted; Task 5 will commit together with the `single_pass` refactor that adjusts the emission sites.

---

## Task 5: Extract `single_pass` helper

**Files:**
- Modify: `src-tauri/src/commands.rs` (extract the existing streaming generate into a reusable helper)

- [ ] **Step 1: Add `single_pass` helper**

Add below `cache_write`:

```rust
/// Call Ollama /api/generate with streaming, forward tokens to `on_progress`
/// with the given phase (and optional chunk info). Returns the accumulated
/// output. Honors `keep_alive: "30m"` and explicit `num_ctx` / `num_predict`.
async fn ollama_generate_stream(
    ollama_url: &str,
    model: &str,
    prompt: String,
    num_ctx: u32,
    phase: FormatPhase,
    chunk: Option<ChunkInfo>,
    on_progress: &(dyn Fn(FormatProgress) + Send + Sync),
) -> Result<String, String> {
    use futures_util::StreamExt;

    #[derive(serde::Serialize)]
    struct Options {
        num_ctx: u32,
        num_predict: i32,
    }
    #[derive(serde::Serialize)]
    struct Req<'a> {
        model: &'a str,
        prompt: String,
        stream: bool,
        keep_alive: &'a str,
        options: Options,
    }

    let body = Req {
        model,
        prompt,
        stream: true,
        keep_alive: "30m",
        options: Options { num_ctx, num_predict: -1 },
    };

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/generate", ollama_url))
        .json(&body)
        .timeout(std::time::Duration::from_secs(600))
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
    while let Some(chunk_bytes) = stream.next().await {
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
                on_progress(FormatProgress {
                    phase,
                    chunk: chunk.clone(),
                    chars: accumulated.chars().count(),
                    done: false,
                });
            }
            if obj.get("done").and_then(|v| v.as_bool()).unwrap_or(false) {
                // Final per-call event; callers may choose to suppress or forward.
                // We do NOT emit `done: true` here; the dispatcher owns that.
            }
        }
    }
    Ok(accumulated)
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p meister --manifest-path src-tauri/Cargo.toml`
Expected: still errors in the existing `run_format` body (to be fixed in Task 8).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "format: widen FormatProgress; add ollama_generate_stream helper"
```

---

## Task 6: `map_pass` helper

**Files:**
- Modify: `src-tauri/src/commands.rs`

- [ ] **Step 1: Add `map_pass`**

Below `ollama_generate_stream`, add:

```rust
fn build_map_prompt(chunk: &str, index: usize, total: usize) -> String {
    format!(
        r#"You are an expert note-taker. You will be given ONE SEGMENT of a longer transcript. This is part {part_num} of {total} parts.

Your job:
1. Read the segment carefully.
2. Produce concise, structured markdown notes that capture everything a downstream document formatter will need to reconstruct what was said: participants mentioned, topics discussed, decisions, action items, dates, specific numbers, and short verbatim quotes worth preserving.
3. Do NOT invent information not present in the segment. If the segment is mostly silence/filler, output "(no substantive content in this segment)".
4. Do NOT write a finished document. These notes are intermediate.
5. Where relevant, flag references to earlier or later segments (e.g. "as discussed earlier", "we'll come back to X").

---
SEGMENT (part {part_num} of {total}):
{chunk}
---

Produce the notes now:"#,
        part_num = index + 1,
        total = total,
        chunk = chunk,
    )
}

/// Run the map-pass prompt against Ollama for one chunk. Emits phase=Map
/// tokens with `chars` set to a running total via `chars_running` (shared
/// across all concurrent chunks).
async fn map_pass(
    ollama_url: &str,
    model: &str,
    chunk: &str,
    index: usize,
    total: usize,
    completed: Arc<std::sync::atomic::AtomicUsize>,
    chars_running: Arc<std::sync::atomic::AtomicUsize>,
    on_progress: Arc<dyn Fn(FormatProgress) + Send + Sync>,
) -> Result<(usize, String), String> {
    use std::sync::atomic::Ordering;

    let prompt = build_map_prompt(chunk, index, total);
    // Emit tokens via a wrapper that accumulates into the shared counter.
    let on_progress_inner = on_progress.clone();
    let chars_running_inner = chars_running.clone();
    let completed_inner = completed.clone();
    let emit = move |p: FormatProgress| {
        // Convert per-call chars delta into a running total across chunks.
        // We just use the per-call chars as a delta proxy (last seen), so
        // we update the shared running counter to be monotonic.
        let this_chars = p.chars;
        let prev = chars_running_inner.swap(
            chars_running_inner.load(Ordering::Relaxed).max(this_chars),
            Ordering::Relaxed,
        );
        let total_chars = prev.max(this_chars);
        on_progress_inner(FormatProgress {
            phase: FormatPhase::Map,
            chunk: Some(ChunkInfo {
                index,
                total,
                completed: completed_inner.load(Ordering::Relaxed),
            }),
            chars: total_chars,
            done: false,
        });
    };

    // The emit closure needs to satisfy `&(dyn Fn + Send + Sync)`. Box it.
    let emit_boxed: Box<dyn Fn(FormatProgress) + Send + Sync> = Box::new(emit);
    let out = ollama_generate_stream(
        ollama_url,
        model,
        prompt,
        6144,
        FormatPhase::Map,
        Some(ChunkInfo { index, total, completed: completed.load(Ordering::Relaxed) }),
        &*emit_boxed,
    )
    .await?;

    // Mark this chunk complete; emit one "completed++" event so UI advances.
    let now_completed = completed.fetch_add(1, Ordering::Relaxed) + 1;
    on_progress(FormatProgress {
        phase: FormatPhase::Map,
        chunk: Some(ChunkInfo { index, total, completed: now_completed }),
        chars: chars_running.load(Ordering::Relaxed),
        done: false,
    });

    Ok((index, out))
}
```

Note on the running-chars heuristic: the design wants "chars visibly moves as chunks finish concurrently." The simple correct implementation is to treat `chars_running` as a monotonic max across emissions. It's approximate — that's fine for a progress indicator.

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p meister --manifest-path src-tauri/Cargo.toml`
Expected: still errors in the old `run_format` body.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "format: add map_pass helper + map prompt builder"
```

---

## Task 7: `reduce_pass` helper

**Files:**
- Modify: `src-tauri/src/commands.rs`

- [ ] **Step 1: Add `reduce_pass` and expose the existing template prompt**

The existing `build_prompt(transcript, template)` at `commands.rs:299` is reusable as-is for the reduce pass — it formats a body against the template. Add a new helper that stitches notes and calls it:

```rust
fn stitch_notes(notes: &[(usize, String)]) -> String {
    let total = notes.len();
    let mut out = String::new();
    for (i, body) in notes {
        out.push_str(&format!("### Notes from part {} of {}\n\n", i + 1, total));
        out.push_str(body.trim());
        out.push_str("\n\n");
    }
    out.trim_end().to_string()
}

async fn reduce_pass(
    ollama_url: &str,
    model: &str,
    stitched_notes: &str,
    template: &str,
    on_progress: Arc<dyn Fn(FormatProgress) + Send + Sync>,
) -> Result<String, String> {
    let prompt = build_prompt(stitched_notes, template);
    let emit = {
        let on_progress = on_progress.clone();
        move |p: FormatProgress| {
            on_progress(FormatProgress {
                phase: FormatPhase::Reduce,
                chunk: None,
                chars: p.chars,
                done: false,
            });
        }
    };
    let emit_boxed: Box<dyn Fn(FormatProgress) + Send + Sync> = Box::new(emit);
    ollama_generate_stream(
        ollama_url,
        model,
        prompt,
        12288,
        FormatPhase::Reduce,
        None,
        &*emit_boxed,
    )
    .await
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p meister --manifest-path src-tauri/Cargo.toml`
Expected: same pre-existing errors in the old `run_format` body; no new errors.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "format: add reduce_pass helper + stitch_notes"
```

---

## Task 8: `run_format_core` dispatcher + Tauri wrapper

**Files:**
- Modify: `src-tauri/src/commands.rs` (replace body of `run_format`)

- [ ] **Step 1: Add `run_format_core` and the new thin `run_format` wrapper**

Delete the existing `run_format` body (the streaming generate loop at `commands.rs:238-296`) and replace it with:

```rust
/// Core formatting logic, testable without an `AppHandle`. Owns the
/// cache lookup, preprocessing, threshold-based branching, map-reduce
/// orchestration, and final cache write.
pub(crate) async fn run_format_core(
    transcript: &str,
    template: &str,
    ollama_url: &str,
    model: &str,
    cache_dir: &Path,
    on_progress: Arc<dyn Fn(FormatProgress) + Send + Sync>,
) -> Result<String, String> {
    use futures_util::{stream, StreamExt};
    use std::sync::atomic::{AtomicUsize, Ordering};

    // 1. Cache lookup on the RAW transcript + template + model.
    let key = cache_key(transcript, template, model);
    if let Some(hit) = cache_read(cache_dir, &key) {
        on_progress(FormatProgress {
            phase: FormatPhase::Single,
            chunk: None,
            chars: hit.chars().count(),
            done: true,
        });
        return Ok(hit);
    }

    // 2. Preprocess.
    let cleaned = preprocess(transcript);

    // 3. Branch.
    let final_output = if approx_tokens(&cleaned) < THRESHOLD {
        // Single-call path.
        let prompt = build_prompt(&cleaned, template);
        let out = ollama_generate_stream(
            ollama_url,
            model,
            prompt,
            8192,
            FormatPhase::Single,
            None,
            &*{
                let on_progress = on_progress.clone();
                let boxed: Box<dyn Fn(FormatProgress) + Send + Sync> = Box::new(move |p| on_progress(p));
                boxed
            },
        )
        .await?;
        on_progress(FormatProgress {
            phase: FormatPhase::Single,
            chunk: None,
            chars: out.chars().count(),
            done: true,
        });
        out
    } else {
        // Map-reduce path.
        let chunks = split_into_chunks(&cleaned);
        let total = chunks.len();
        let completed = Arc::new(AtomicUsize::new(0));
        let chars_running = Arc::new(AtomicUsize::new(0));

        let url = ollama_url.to_string();
        let model_s = model.to_string();

        let results: Vec<Result<(usize, String), String>> = stream::iter(
            chunks.into_iter().enumerate()
        )
        .map(|(i, chunk_text)| {
            let url = url.clone();
            let model_s = model_s.clone();
            let completed = completed.clone();
            let chars_running = chars_running.clone();
            let on_progress = on_progress.clone();
            async move {
                map_pass(&url, &model_s, &chunk_text, i, total, completed, chars_running, on_progress).await
            }
        })
        .buffer_unordered(3)
        .collect()
        .await;

        // Fail fast on any map error.
        let mut notes: Vec<(usize, String)> = Vec::with_capacity(total);
        for r in results {
            notes.push(r?);
        }
        notes.sort_by_key(|(i, _)| *i);

        let stitched = stitch_notes(&notes);
        let reduced = reduce_pass(ollama_url, model, &stitched, template, on_progress.clone()).await?;

        on_progress(FormatProgress {
            phase: FormatPhase::Reduce,
            chunk: None,
            chars: reduced.chars().count(),
            done: true,
        });
        reduced
    };

    // 4. Cache write (best-effort; a write failure must not fail the stage).
    if let Err(e) = cache_write(cache_dir, &key, &final_output) {
        eprintln!("format cache write failed: {e}");
    }

    Ok(final_output)
}

/// Call Ollama to format a transcript against a template. Emits
/// `format-progress` events while streaming. Thin wrapper over
/// `run_format_core` that resolves the cache directory from the AppHandle.
pub async fn run_format(
    app: &AppHandle,
    transcript: &str,
    template_content: &str,
    ollama_url: &str,
    model: &str,
) -> Result<String, String> {
    let cache_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app_data_dir: {e}"))?
        .join("format-cache");

    let app_handle = app.clone();
    let on_progress: Arc<dyn Fn(FormatProgress) + Send + Sync> = Arc::new(move |p| {
        let _ = app_handle.emit("format-progress", p);
    });

    run_format_core(
        transcript,
        template_content,
        ollama_url,
        model,
        &cache_dir,
        on_progress,
    )
    .await
}
```

- [ ] **Step 2: Build the whole crate**

Run: `cargo build -p meister --manifest-path src-tauri/Cargo.toml`
Expected: clean build. If any errors about `build_prompt`, `OllamaGenerateRequest`, etc., delete the now-dead `OllamaGenerateRequest` struct (it's replaced by the inline `Req` inside `ollama_generate_stream`).

- [ ] **Step 3: Run existing unit tests**

Run: `cargo test -p meister --manifest-path src-tauri/Cargo.toml --lib`
Expected: all existing tests (orchestrator, storage, etc.) + the new `preprocess_*` / `split_*` / `cache_*` tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "format: wire run_format_core dispatcher (cache + preprocess + map-reduce)"
```

---

## Task 9: Mock Ollama harness + integration tests

**Files:**
- Modify: `src-tauri/src/commands.rs` (append to `#[cfg(test)] mod tests`)
- Modify: `src-tauri/Cargo.toml` (add `axum` + `tempfile` dev-deps)

- [ ] **Step 1: Add dev-dependencies**

In `src-tauri/Cargo.toml`, add (or extend) a `[dev-dependencies]` section:

```toml
[dev-dependencies]
axum = "0.7"
tempfile = "3"
```

(`tempfile` is already in regular deps; re-declaring it as a dev-dep is harmless. If you'd rather avoid the duplicate, just leave it out and rely on the main dep — the `tempfile` calls in earlier tasks already work.)

- [ ] **Step 2: Add the mock-server module**

Append to the `tests` module in `commands.rs`:

```rust
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    use std::time::Duration;

    /// A minimal Ollama-compatible mock. Each POST to /api/generate returns a
    /// streaming NDJSON response. Request bodies are captured for assertions.
    struct MockOllama {
        base_url: String,
        calls: Arc<Mutex<Vec<serde_json::Value>>>,
        overlaps: Arc<AtomicUsize>,
        in_flight: Arc<AtomicUsize>,
        shutdown: Option<tokio::sync::oneshot::Sender<()>>,
        handle: Option<tokio::task::JoinHandle<()>>,
    }

    impl MockOllama {
        async fn spawn<F>(handler: F) -> Self
        where
            F: Fn(serde_json::Value) -> String + Send + Sync + 'static,
        {
            use axum::{
                body::Body,
                extract::State,
                http::StatusCode,
                response::Response,
                routing::post,
                Router,
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
                // Give tests time to observe overlap.
                tokio::time::sleep(Duration::from_millis(60)).await;

                // Stream the text as NDJSON with one "response" line per word + one "done".
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

            Self {
                base_url,
                calls,
                overlaps,
                in_flight,
                shutdown: Some(tx),
                handle: Some(handle),
            }
        }
    }

    impl Drop for MockOllama {
        fn drop(&mut self) {
            if let Some(tx) = self.shutdown.take() { let _ = tx.send(()); }
        }
    }

    fn noop_progress() -> Arc<dyn Fn(FormatProgress) + Send + Sync> {
        Arc::new(|_p| {})
    }

    fn collect_progress() -> (Arc<Mutex<Vec<FormatProgress>>>, Arc<dyn Fn(FormatProgress) + Send + Sync>) {
        let v: Arc<Mutex<Vec<FormatProgress>>> = Arc::new(Mutex::new(Vec::new()));
        let v2 = v.clone();
        let cb: Arc<dyn Fn(FormatProgress) + Send + Sync> = Arc::new(move |p| {
            v2.lock().unwrap().push(p);
        });
        (v, cb)
    }
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo test -p meister --manifest-path src-tauri/Cargo.toml --lib --no-run`
Expected: builds cleanly.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/commands.rs
git commit -m "format: add mock-ollama test harness"
```

---

## Task 10: Integration — single-call path

**Files:**
- Modify: `src-tauri/src/commands.rs` (`tests` module)

- [ ] **Step 1: Add the test**

Append to the `tests` module:

```rust
    #[tokio::test]
    async fn single_call_path_makes_one_upstream_call() {
        let mock = MockOllama::spawn(|_body| "FORMATTED\n".to_string()).await;
        let tmp = tempfile::tempdir().unwrap();

        let out = run_format_core(
            "short transcript",
            "# Template",
            &mock.base_url,
            "gemma3",
            tmp.path(),
            noop_progress(),
        )
        .await
        .unwrap();

        assert!(out.trim().contains("FORMATTED"));
        let calls = mock.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        // Sanity: request body carried keep_alive + num_ctx.
        assert_eq!(calls[0]["keep_alive"], "30m");
        assert_eq!(calls[0]["options"]["num_ctx"], 8192);
        assert_eq!(calls[0]["options"]["num_predict"], -1);
    }
```

- [ ] **Step 2: Run it**

Run: `cargo test -p meister --manifest-path src-tauri/Cargo.toml --lib single_call_path_ -- --nocapture`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "format: integration test for single-call path"
```

---

## Task 11: Integration — map-reduce path + concurrency

**Files:**
- Modify: `src-tauri/src/commands.rs` (`tests` module)

- [ ] **Step 1: Add the test**

Append to the `tests` module:

```rust
    #[tokio::test]
    async fn map_reduce_path_issues_N_plus_1_calls_with_concurrency() {
        // Long transcript → multiple chunks.
        let sentence = "This is a long sentence that will get split eventually. ";
        let transcript: String = sentence.repeat(500); // ~28k chars ≈ 7k tokens

        let mock = MockOllama::spawn(|body| {
            // Mock echoes the prompt length so tests can tell map vs. reduce apart.
            let prompt = body["prompt"].as_str().unwrap_or("");
            if prompt.contains("You are an expert note-taker") {
                "NOTES for this segment. ".to_string()
            } else {
                "REDUCED OUTPUT. ".to_string()
            }
        })
        .await;

        let tmp = tempfile::tempdir().unwrap();
        let out = run_format_core(
            &transcript,
            "# Template",
            &mock.base_url,
            "gemma3",
            tmp.path(),
            noop_progress(),
        )
        .await
        .unwrap();

        assert!(out.contains("REDUCED OUTPUT"));
        let calls = mock.calls.lock().unwrap();
        assert!(calls.len() >= 3, "expected N+1 (N≥2 map + 1 reduce) calls, got {}", calls.len());
        // Last call must be the reduce (not the map prompt).
        let last_prompt = calls.last().unwrap()["prompt"].as_str().unwrap();
        assert!(last_prompt.contains("expert document formatter"), "last call was not reduce");
        // At least 2 map calls overlapped — concurrency actually happened.
        assert!(mock.overlaps.load(Ordering::SeqCst) >= 1, "no concurrency observed");

        // num_ctx differs between phases.
        let map_call = calls.iter().find(|c| c["prompt"].as_str().unwrap().contains("note-taker")).unwrap();
        assert_eq!(map_call["options"]["num_ctx"], 6144);
        let reduce_call = calls.iter().rev().find(|c| c["prompt"].as_str().unwrap().contains("expert document formatter")).unwrap();
        assert_eq!(reduce_call["options"]["num_ctx"], 12288);
    }
```

- [ ] **Step 2: Run it**

Run: `cargo test -p meister --manifest-path src-tauri/Cargo.toml --lib map_reduce_path_ -- --nocapture`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "format: integration test for map-reduce path + concurrency"
```

---

## Task 12: Integration — cache hit / miss

**Files:**
- Modify: `src-tauri/src/commands.rs` (`tests` module)

- [ ] **Step 1: Add the tests**

```rust
    #[tokio::test]
    async fn cache_hit_short_circuits_second_call() {
        let mock = MockOllama::spawn(|_b| "FORMATTED".to_string()).await;
        let tmp = tempfile::tempdir().unwrap();

        run_format_core("t", "tpl", &mock.base_url, "gemma3", tmp.path(), noop_progress()).await.unwrap();
        run_format_core("t", "tpl", &mock.base_url, "gemma3", tmp.path(), noop_progress()).await.unwrap();

        let calls = mock.calls.lock().unwrap();
        assert_eq!(calls.len(), 1, "second run should have hit the cache");
    }

    #[tokio::test]
    async fn cache_miss_on_model_change() {
        let mock = MockOllama::spawn(|_b| "FORMATTED".to_string()).await;
        let tmp = tempfile::tempdir().unwrap();

        run_format_core("t", "tpl", &mock.base_url, "modelA", tmp.path(), noop_progress()).await.unwrap();
        run_format_core("t", "tpl", &mock.base_url, "modelB", tmp.path(), noop_progress()).await.unwrap();

        let calls = mock.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
    }
```

- [ ] **Step 2: Run them**

Run: `cargo test -p meister --manifest-path src-tauri/Cargo.toml --lib cache_hit_ cache_miss_on -- --nocapture`
Expected: both PASS.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "format: integration tests for cache hit/miss"
```

---

## Task 13: Integration — progress event ordering

**Files:**
- Modify: `src-tauri/src/commands.rs` (`tests` module)

- [ ] **Step 1: Add the test**

```rust
    #[tokio::test]
    async fn progress_events_order_map_then_reduce_then_done() {
        let sentence = "This is a long sentence that will get split eventually. ";
        let transcript: String = sentence.repeat(500);

        let mock = MockOllama::spawn(|b| {
            if b["prompt"].as_str().unwrap().contains("note-taker") {
                "NOTES. ".to_string()
            } else {
                "REDUCED. ".to_string()
            }
        })
        .await;

        let tmp = tempfile::tempdir().unwrap();
        let (events, cb) = collect_progress();

        run_format_core(&transcript, "# Template", &mock.base_url, "gemma3", tmp.path(), cb)
            .await
            .unwrap();

        let events = events.lock().unwrap().clone();
        assert!(!events.is_empty());

        // Phase order: some Map events, then Reduce events, and the last is done=true.
        let phases: Vec<FormatPhase> = events.iter().map(|e| e.phase).collect();
        let first_reduce_idx = phases.iter().position(|p| *p == FormatPhase::Reduce).expect("no reduce phase");
        assert!(phases[..first_reduce_idx].iter().all(|p| *p == FormatPhase::Map));

        // chunk.completed is monotonic non-decreasing during map.
        let mut last_completed = 0;
        for e in events.iter().take(first_reduce_idx) {
            let c = e.chunk.as_ref().expect("map event missing chunk info");
            assert!(c.completed >= last_completed);
            last_completed = c.completed;
        }
        // Map phase ends with completed == total.
        let last_map = &events[first_reduce_idx - 1];
        let c = last_map.chunk.as_ref().unwrap();
        assert_eq!(c.completed, c.total);

        // Final event is done=true with phase=Reduce.
        let last = events.last().unwrap();
        assert!(last.done);
        assert_eq!(last.phase, FormatPhase::Reduce);
    }
```

- [ ] **Step 2: Run it**

Run: `cargo test -p meister --manifest-path src-tauri/Cargo.toml --lib progress_events_order_ -- --nocapture`
Expected: PASS.

- [ ] **Step 3: Run the whole suite**

Run: `cargo test -p meister --manifest-path src-tauri/Cargo.toml`
Expected: all tests PASS.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "format: integration test for progress event ordering"
```

---

## Task 14: Frontend — chunk-aware progress UI

**Files:**
- Modify: `src/components/RecordingView.tsx`

- [ ] **Step 1: Widen the progress state + listener**

Find (near line 32):

```tsx
  const [formatChars, setFormatChars] = useState(0);
```

Replace with:

```tsx
  type FormatPhase = "single" | "map" | "reduce";
  type FormatProgressPayload = {
    phase: FormatPhase;
    chunk: { index: number; total: number; completed: number } | null;
    chars: number;
    done: boolean;
  };
  const [formatProgress, setFormatProgress] = useState<FormatProgressPayload | null>(null);
```

Find (around line 82):

```tsx
    listen<{ chars: number }>("format-progress", (e) =>
      setFormatChars(e.payload.chars)
    ).then((fn) => { unlisten2 = fn; });
```

Replace with:

```tsx
    listen<FormatProgressPayload>("format-progress", (e) =>
      setFormatProgress(e.payload)
    ).then((fn) => { unlisten2 = fn; });
```

Find (around line 113, inside `run`):

```tsx
      setFormatChars(0);
```

Replace with:

```tsx
      setFormatProgress(null);
```

- [ ] **Step 2: Update the render**

Find (around line 301):

```tsx
        {rec.stages.find((s) => s.stage === "format")?.status === "running" && formatChars > 0 && (
          <p className="text-xs text-white/40">Formatting… {formatChars.toLocaleString()} chars</p>
        )}
```

Replace with:

```tsx
        {rec.stages.find((s) => s.stage === "format")?.status === "running" && formatProgress && (
          <div className="text-xs text-white/40 space-y-1">
            {formatProgress.phase === "single" && (
              <p>Formatting… {formatProgress.chars.toLocaleString()} chars</p>
            )}
            {formatProgress.phase === "map" && formatProgress.chunk && (
              <>
                <p>
                  Analyzing chunk {Math.min(formatProgress.chunk.completed + 1, formatProgress.chunk.total)}/
                  {formatProgress.chunk.total}…
                </p>
                <div className="h-1 w-full bg-white/5 rounded">
                  <div
                    className="h-1 bg-white/30 rounded"
                    style={{
                      width: `${(formatProgress.chunk.completed / formatProgress.chunk.total) * 100}%`,
                    }}
                  />
                </div>
              </>
            )}
            {formatProgress.phase === "reduce" && (
              <p>Composing output… {formatProgress.chars.toLocaleString()} chars</p>
            )}
          </div>
        )}
```

- [ ] **Step 3: Type-check the frontend**

Run: `npm run tsc -- --noEmit` (if the project has a tsc script) or `npx tsc --noEmit`.
Expected: no type errors.

If the project doesn't have a type-check npm script, skip this step; the `npm run tauri dev` smoke in Task 16 will catch type errors.

- [ ] **Step 4: Commit**

```bash
git add src/components/RecordingView.tsx
git commit -m "frontend: chunk-aware format progress (map/reduce phases)"
```

---

## Task 15: Document `OLLAMA_NUM_PARALLEL`

**Files:**
- Modify: `SETUP.md`

- [ ] **Step 1: Add the guidance**

Find this block in `SETUP.md` (around line 48–50):

```markdown
# Start Ollama (keep it running in the background)
ollama serve
```

Replace with:

```markdown
# Start Ollama (keep it running in the background).
# OLLAMA_NUM_PARALLEL=3 lets Meister's formatter run chunks concurrently on long
# transcripts — Meister's map-reduce formatting path fires up to 3 parallel
# /api/generate calls. At the default of 1 the output is still correct, just
# slower.
OLLAMA_NUM_PARALLEL=3 ollama serve
```

- [ ] **Step 2: Commit**

```bash
git add SETUP.md
git commit -m "docs: note OLLAMA_NUM_PARALLEL for parallel format chunks"
```

---

## Task 16: Manual verification on a real recording

**Files:** none

- [ ] **Step 1: Start Ollama with the parallel setting**

In a terminal, run:

```bash
OLLAMA_NUM_PARALLEL=3 ollama serve
```

Leave it running.

- [ ] **Step 2: Launch the app**

```bash
cd /Users/nexus/projects/meister
npm run tauri dev
```

- [ ] **Step 3: Run a long recording end-to-end**

Pick or create a project, drop in a recording file that produces a ~30+ minute transcript, run through Extract → Transcribe → Format → Save.

Expected:
- Format stage shows "Analyzing chunk 1/N…", "Analyzing chunk 2/N…", etc. — the sub-bar moves as chunks finish.
- Then "Composing output…" appears and streams characters.
- Final formatted document has reasonable quality (decisions, action items, topics all captured).
- Rerunning the recording after toggling output formats (invalidates Save, not Format) is effectively instant — cache hit.

- [ ] **Step 4: Compare timings**

Compare wall time against the same recording before this change (pre-map-reduce single-call). Expect 2–5× speedup on transcripts > 30 minutes. Note in the final PR description.

- [ ] **Step 5: Commit a CHANGELOG entry (optional)**

If the project tracks a changelog, add a line. Otherwise, skip.

---

## Self-review

- **Spec coverage:** Problem → Tasks 1–14. Easy wins (keep_alive, num_ctx, filler stripping, cache) → Tasks 1, 3, 5, 8. Map-reduce → Tasks 2, 6, 7, 8. Progress/UI → Tasks 4, 8, 14. Testing → Tasks 9–13. SETUP.md → Task 15. Manual verification → Task 16.
- **Types are consistent** across tasks: `FormatPhase`, `ChunkInfo`, `FormatProgress`, `run_format_core`, `cache_key`, `cache_read`, `cache_write`, `preprocess`, `split_into_chunks`, `ollama_generate_stream`, `map_pass`, `reduce_pass`, `stitch_notes`.
- **No placeholders.** Every step has concrete code or a concrete command.
- **TDD** for the pure helpers (Tasks 1–3); integration tests live with the code (Tasks 9–13) since they need all helpers wired through `run_format_core`.
- **Frequent commits** — 15 commits across the plan.
