# Mic Capture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Record audio from the system mic inside Meister; captured files drop into the existing ProjectView staging flow.

**Architecture:** Browser `getUserMedia` + `MediaRecorder` inside the Tauri WKWebView produces a WebM/Opus Blob. A new Tauri command `save_capture(bytes)` writes it to `<app_data>/captures/<name>.webm` and returns the path. That path is fed to ProjectView's existing `onFilesPicked([path])` — same code path as a dropped file. Orphan capture files are swept at startup.

**Tech Stack:** Tauri 2, React 18, TypeScript, Rust (no new deps — reuses existing `uuid`). macOS permissions via `NSMicrophoneUsageDescription` in `tauri.conf.json`.

**Spec:** `docs/superpowers/specs/2026-04-18-mic-capture-design.md`

---

## File structure

### Rust — modified
- `src-tauri/src/commands.rs` — add `save_capture` Tauri command and `write_capture_to` helper.
- `src-tauri/src/storage/mod.rs` — add `cleanup_orphaned_captures` function.
- `src-tauri/src/lib.rs` — register `save_capture` in `invoke_handler!`; call `cleanup_orphaned_captures` in `setup`.
- `src-tauri/tauri.conf.json` — add `NSMicrophoneUsageDescription` to the macOS `bundle.macOS.infoPlist`.
- `src-tauri/tests/storage_integration.rs` — tests for `cleanup_orphaned_captures`.
- `src-tauri/src/commands.rs` tests module — tests for `write_capture_to`.

### Frontend — modified
- `src/App.tsx` — lift a capture-active ref + guarded navigation; pass a `registerActiveCapture` callback down to ProjectView.
- `src/components/ProjectView.tsx` — render new `RecordCard` beside `RecordingDropZone` in a side-by-side "Sources row".
- `src/components/Sidebar.tsx` — call the guarded-navigate callback instead of `onSelect` directly.

### Frontend — new
- `src/components/RecordCard.tsx` — all capture states (idle, recording, paused, saving, error), MediaRecorder wiring, calls `save_capture`, emits path via `onPicked`.

---

# Phase 1 — Rust backend

## Task 1: `save_capture` Tauri command + `write_capture_to` helper

**Files:**
- Modify: `src-tauri/src/commands.rs`

- [ ] **Step 1: Write the failing test**

Append a new `#[cfg(test)] mod` at the end of `src-tauri/src/commands.rs` (or extend an existing one — there's already a `build_ffmpeg_args_tests` module):

```rust
#[cfg(test)]
mod write_capture_tests {
    use super::write_capture_to;
    use tempfile::tempdir;

    #[test]
    fn writes_bytes_and_returns_webm_path_under_dir() {
        let dir = tempdir().unwrap();
        let bytes = b"opus-bytes-here".to_vec();
        let path = write_capture_to(dir.path(), &bytes).unwrap();

        assert!(path.starts_with(dir.path()));
        assert_eq!(path.extension().and_then(|s| s.to_str()), Some("webm"));

        let on_disk = std::fs::read(&path).unwrap();
        assert_eq!(on_disk, bytes);

        let name = path.file_name().and_then(|s| s.to_str()).unwrap();
        assert!(name.starts_with("capture-"), "name should start with 'capture-': {name}");
    }

    #[test]
    fn returns_unique_paths_on_repeated_calls() {
        let dir = tempdir().unwrap();
        let p1 = write_capture_to(dir.path(), b"a").unwrap();
        let p2 = write_capture_to(dir.path(), b"b").unwrap();
        assert_ne!(p1, p2, "same dir + separate calls should produce different paths");
    }
}
```

- [ ] **Step 2: Run tests — confirm failure**

Run: `cd src-tauri && cargo test --lib write_capture_tests`
Expected: FAIL — `write_capture_to` not defined.

- [ ] **Step 3: Implement `write_capture_to` and `save_capture`**

In `src-tauri/src/commands.rs`, add these functions near the other file I/O helpers (e.g. above `save_text_file`, or at the bottom of the file — consistent with existing organization):

```rust
/// Write a capture blob to the given directory. Filename format:
/// `capture-YYYYMMDD-HHMMSS-<8-hex>.webm`. Returns the full path.
pub fn write_capture_to(
    dir: &std::path::Path,
    bytes: &[u8],
) -> Result<std::path::PathBuf, String> {
    std::fs::create_dir_all(dir)
        .map_err(|e| format!("create captures dir: {e}"))?;

    let ts = {
        let secs_since_epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        // YYYYMMDD-HHMMSS from epoch seconds (UTC). Simple, sortable.
        format_utc_timestamp(secs_since_epoch)
    };
    let short = uuid::Uuid::new_v4().simple().to_string();
    let short = &short[..8];
    let filename = format!("capture-{ts}-{short}.webm");
    let path = dir.join(filename);
    std::fs::write(&path, bytes)
        .map_err(|e| format!("write capture: {e}"))?;
    Ok(path)
}

/// Return `YYYYMMDD-HHMMSS` for a given unix timestamp (UTC).
fn format_utc_timestamp(secs: u64) -> String {
    // Days since 1970-01-01, time-of-day. Standard civil-calendar conversion
    // (Howard Hinnant's algorithm) — avoids pulling chrono for one function.
    let days = (secs / 86_400) as i64;
    let sod = secs % 86_400;
    let hh = sod / 3_600;
    let mm = (sod % 3_600) / 60;
    let ss = sod % 60;

    // Civil from days (Hinnant):
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}{m:02}{d:02}-{hh:02}{mm:02}{ss:02}")
}

/// Save a captured audio Blob (raw bytes) into the app data dir.
/// Returns the absolute path of the written file.
#[tauri::command]
pub async fn save_capture(
    app: AppHandle,
    bytes: Vec<u8>,
) -> Result<String, String> {
    use tauri::Manager;
    let dir = app.path()
        .app_data_dir()
        .map_err(|e| format!("app_data_dir: {e}"))?
        .join("captures");
    let path = write_capture_to(&dir, &bytes)?;
    Ok(path.to_string_lossy().to_string())
}
```

Ensure `tempfile` is available for tests. Check `src-tauri/Cargo.toml` — if `tempfile = "3"` is already in `[dependencies]` (it is — used by the existing codebase), it's also usable from `#[cfg(test)]`. No action needed. If missing, add to `[dev-dependencies]`:

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 4: Run tests — verify pass**

Run: `cd src-tauri && cargo test --lib write_capture_tests`
Expected: both tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "commands: add save_capture + write_capture_to helper"
```

---

## Task 2: `cleanup_orphaned_captures` + integration tests

**Files:**
- Modify: `src-tauri/src/storage/mod.rs`
- Modify: `src-tauri/tests/storage_integration.rs`

- [ ] **Step 1: Append failing integration tests**

Append to `src-tauri/tests/storage_integration.rs`:

```rust
#[test]
fn cleanup_orphaned_captures_deletes_unreferenced_files() {
    use std::fs;
    let store = Store::open_memory().unwrap();
    storage::projects::insert(&store, &sample_project()).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let keep = dir.path().join("keep.webm");
    let drop1 = dir.path().join("drop1.webm");
    let drop2 = dir.path().join("drop2.webm");
    fs::write(&keep, b"k").unwrap();
    fs::write(&drop1, b"d1").unwrap();
    fs::write(&drop2, b"d2").unwrap();

    let mut r = sample_recording("p1");
    r.source_paths = vec![keep.to_string_lossy().to_string()];
    storage::recordings::insert(&store, &r).unwrap();

    let removed = storage::cleanup_orphaned_captures(&store, dir.path()).unwrap();
    assert_eq!(removed, 2);
    assert!(keep.exists());
    assert!(!drop1.exists());
    assert!(!drop2.exists());
}

#[test]
fn cleanup_orphaned_captures_handles_missing_dir() {
    let store = Store::open_memory().unwrap();
    let bogus = std::path::PathBuf::from("/definitely/not/a/real/path/xxxyyy");
    let removed = storage::cleanup_orphaned_captures(&store, &bogus).unwrap();
    assert_eq!(removed, 0);
}

#[test]
fn cleanup_orphaned_captures_ignores_referenced_files_across_many_recordings() {
    use std::fs;
    let store = Store::open_memory().unwrap();
    storage::projects::insert(&store, &sample_project()).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.webm");
    let b = dir.path().join("b.webm");
    let c = dir.path().join("c.webm");
    for p in [&a, &b, &c] {
        fs::write(p, b"x").unwrap();
    }

    let mut r1 = sample_recording("p1");
    r1.id = "r1".to_string();
    r1.source_paths = vec![a.to_string_lossy().to_string(), b.to_string_lossy().to_string()];
    storage::recordings::insert(&store, &r1).unwrap();

    // A second recording referencing c.webm.
    let mut r2 = sample_recording("p1");
    r2.id = "r2".to_string();
    r2.source_paths = vec![c.to_string_lossy().to_string()];
    storage::recordings::insert(&store, &r2).unwrap();

    let removed = storage::cleanup_orphaned_captures(&store, dir.path()).unwrap();
    assert_eq!(removed, 0);
    for p in [a, b, c] {
        assert!(p.exists());
    }
}
```

- [ ] **Step 2: Run — confirm failure**

Run: `cd src-tauri && cargo test --test storage_integration`
Expected: FAIL — `storage::cleanup_orphaned_captures` not defined.

- [ ] **Step 3: Implement `cleanup_orphaned_captures`**

Append to `src-tauri/src/storage/mod.rs` (below the existing `Store` impl and its tests module):

```rust
use std::path::Path;

/// Scan `captures_dir` and delete any files whose paths are NOT present in any
/// recording's `source_paths`. Safe to call at startup. Returns the number of
/// files removed. Missing directories are treated as "nothing to clean".
pub fn cleanup_orphaned_captures(store: &Store, captures_dir: &Path) -> Result<usize, String> {
    let entries = match std::fs::read_dir(captures_dir) {
        Ok(e) => e,
        Err(_) => return Ok(0),
    };

    let referenced: std::collections::HashSet<String> = {
        let conn = store.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT source_paths FROM recordings")
            .map_err(|e| format!("prepare: {e}"))?;
        stmt.query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| format!("query: {e}"))?
            .filter_map(Result::ok)
            .flat_map(|json| {
                serde_json::from_str::<Vec<String>>(&json).unwrap_or_default()
            })
            .collect()
    };

    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let s = path.to_string_lossy().to_string();
        if !referenced.contains(&s) {
            if std::fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        }
    }
    Ok(removed)
}
```

- [ ] **Step 4: Run tests — verify pass**

Run: `cd src-tauri && cargo test --test storage_integration`
Expected: all pass, including the 3 new tests.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/storage/mod.rs src-tauri/tests/storage_integration.rs
git commit -m "storage: add cleanup_orphaned_captures with integration tests"
```

---

## Task 3: Register `save_capture` + wire cleanup into startup

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Register `save_capture` in `invoke_handler!`**

In `src-tauri/src/lib.rs`, find the `invoke_handler![…]` macro and add this line (order near other `commands::` entries; by convention put it with other file I/O commands like `save_text_file`):

```rust
commands::save_capture,
```

- [ ] **Step 2: Call `cleanup_orphaned_captures` in setup**

Still in `src-tauri/src/lib.rs`, find the `.setup(|app| { ... })` block. After the existing `let swept = storage::stages::mark_running_as_failed(...)` call, add:

```rust
let captures_dir = app_data_dir.join("captures");
match storage::cleanup_orphaned_captures(&store, &captures_dir) {
    Ok(n) if n > 0 => eprintln!("storage: removed {n} orphaned capture file(s)"),
    Ok(_) => {}
    Err(e) => eprintln!("storage: capture cleanup failed (non-fatal): {e}"),
}
```

- [ ] **Step 3: Compile + run suite**

Run: `cd src-tauri && cargo check`
Expected: clean (pre-existing `OllamaModel`/`OllamaTagsResponse`/`run_save_outputs` warnings stay).

Run: `cd src-tauri && cargo test`
Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "lib: register save_capture + sweep orphaned captures at startup"
```

---

## Task 4: Add `NSMicrophoneUsageDescription` to the macOS bundle

**Files:**
- Modify: `src-tauri/tauri.conf.json`

- [ ] **Step 1: Edit `tauri.conf.json`**

Open `src-tauri/tauri.conf.json`. Find the `bundle` object. If it has `macOS`, extend it; if not, add it. The resulting shape should include:

```json
{
  "bundle": {
    ...existing keys...,
    "macOS": {
      ...any existing macOS keys...,
      "infoPlist": {
        "NSMicrophoneUsageDescription": "Meister needs microphone access to record audio for transcription."
      }
    }
  }
}
```

If `bundle.macOS.infoPlist` already exists as a path (string pointing to a file), or contains other entries, merge the `NSMicrophoneUsageDescription` key in. Tauri 2 accepts `infoPlist` as either an object or a relative file path; we use the object form.

- [ ] **Step 2: Verify build runs**

Run: `cd src-tauri && cargo check`
Expected: clean. Tauri validates the config at build time; a malformed JSON will be caught here.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/tauri.conf.json
git commit -m "bundle: add NSMicrophoneUsageDescription for mic capture"
```

---

# Phase 2 — Frontend

## Task 5: `RecordCard` component

**Files:**
- Create: `src/components/RecordCard.tsx`

- [ ] **Step 1: Create the component**

Write `src/components/RecordCard.tsx`:

```tsx
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type CaptureState = "idle" | "recording" | "paused" | "saving" | "error";

interface Props {
  onPicked: (filePaths: string[]) => void;
  /** Fired when a capture becomes active; pass `null` when it ends. The parent
   *  uses this to install a navigation guard (confirm before leaving). */
  onActiveChange?: (discard: (() => void) | null) => void;
}

function formatElapsed(ms: number): string {
  const total = Math.floor(ms / 1000);
  const hh = Math.floor(total / 3600);
  const mm = Math.floor((total % 3600) / 60);
  const ss = total % 60;
  const pad = (n: number) => n.toString().padStart(2, "0");
  return hh > 0 ? `${pad(hh)}:${pad(mm)}:${pad(ss)}` : `${pad(mm)}:${pad(ss)}`;
}

function pickMime(): string | undefined {
  if (typeof MediaRecorder === "undefined") return undefined;
  if (MediaRecorder.isTypeSupported("audio/webm;codecs=opus")) {
    return "audio/webm;codecs=opus";
  }
  if (MediaRecorder.isTypeSupported("audio/webm")) {
    return "audio/webm";
  }
  return undefined;
}

export default function RecordCard({ onPicked, onActiveChange }: Props) {
  const [state, setState] = useState<CaptureState>("idle");
  const [errorMsg, setErrorMsg] = useState<string>("");
  const [elapsedMs, setElapsedMs] = useState(0);

  const recorderRef = useRef<MediaRecorder | null>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const chunksRef = useRef<Blob[]>([]);
  const startedAtRef = useRef<number>(0);
  const accumulatedRef = useRef<number>(0); // ms accumulated before current run (for pause/resume)
  const tickerRef = useRef<number | null>(null);

  const stopTicker = () => {
    if (tickerRef.current !== null) {
      window.clearInterval(tickerRef.current);
      tickerRef.current = null;
    }
  };

  const startTicker = () => {
    stopTicker();
    tickerRef.current = window.setInterval(() => {
      setElapsedMs(accumulatedRef.current + (Date.now() - startedAtRef.current));
    }, 200);
  };

  const stopStreamAndRecorder = () => {
    stopTicker();
    try { recorderRef.current?.stop(); } catch { /* ignore */ }
    recorderRef.current = null;
    streamRef.current?.getTracks().forEach((t) => t.stop());
    streamRef.current = null;
  };

  // On unmount, ensure we release the mic.
  useEffect(() => {
    return () => { stopStreamAndRecorder(); };
  }, []);

  const start = async () => {
    setErrorMsg("");
    setElapsedMs(0);
    accumulatedRef.current = 0;
    chunksRef.current = [];
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      streamRef.current = stream;
      const mime = pickMime();
      const recorder = mime
        ? new MediaRecorder(stream, { mimeType: mime })
        : new MediaRecorder(stream);
      recorderRef.current = recorder;

      recorder.ondataavailable = (e) => {
        if (e.data.size > 0) chunksRef.current.push(e.data);
      };

      recorder.onstop = async () => {
        stopTicker();
        const wasDiscarded = (recorder as MediaRecorder & { _discarded?: boolean })._discarded === true;
        streamRef.current?.getTracks().forEach((t) => t.stop());
        streamRef.current = null;
        recorderRef.current = null;
        onActiveChange?.(null);

        if (wasDiscarded) {
          chunksRef.current = [];
          setState("idle");
          return;
        }

        setState("saving");
        try {
          const blob = new Blob(chunksRef.current, {
            type: recorder.mimeType || "audio/webm",
          });
          const buf = await blob.arrayBuffer();
          const path = await invoke<string>("save_capture", {
            bytes: Array.from(new Uint8Array(buf)),
          });
          chunksRef.current = [];
          setState("idle");
          setElapsedMs(0);
          onPicked([path]);
        } catch (e) {
          setState("error");
          setErrorMsg(`Could not save: ${String(e)}`);
        }
      };

      recorder.start();
      startedAtRef.current = Date.now();
      startTicker();
      setState("recording");
      onActiveChange?.(discard);
    } catch (e) {
      const err = e as DOMException | Error;
      setState("error");
      if ("name" in err && err.name === "NotAllowedError") {
        setErrorMsg("Microphone access denied. Open System Settings → Privacy & Security → Microphone to allow.");
      } else if ("name" in err && err.name === "NotFoundError") {
        setErrorMsg("No microphone detected. Plug in a mic and click Retry.");
      } else {
        setErrorMsg(String(err));
      }
    }
  };

  const pause = () => {
    const r = recorderRef.current;
    if (!r || r.state !== "recording") return;
    r.pause();
    stopTicker();
    accumulatedRef.current += Date.now() - startedAtRef.current;
    setState("paused");
  };

  const resume = () => {
    const r = recorderRef.current;
    if (!r || r.state !== "paused") return;
    r.resume();
    startedAtRef.current = Date.now();
    startTicker();
    setState("recording");
  };

  const stop = () => {
    const r = recorderRef.current;
    if (!r) return;
    r.stop(); // onstop handles saving + state transition
  };

  const discard = () => {
    const r = recorderRef.current;
    if (!r) {
      // No active recorder — just reset.
      chunksRef.current = [];
      setState("idle");
      setElapsedMs(0);
      onActiveChange?.(null);
      return;
    }
    (r as MediaRecorder & { _discarded?: boolean })._discarded = true;
    r.stop(); // onstop sees _discarded and skips save
  };

  const retry = () => {
    setState("idle");
    setErrorMsg("");
  };

  // ── Render ────────────────────────────────────────────────────────────────

  const cardBase = "rounded-xl border-2 flex flex-col items-center justify-center gap-2 py-10 px-4 transition-colors";

  if (state === "idle") {
    return (
      <div
        className={`${cardBase} border-dashed border-white/10 hover:border-white/20 bg-surface-secondary cursor-pointer`}
        onClick={start}
      >
        <span className="text-3xl opacity-30">🎙</span>
        <p className="text-sm text-white/40">Record audio</p>
        <p className="text-xs text-white/25">Click to start</p>
      </div>
    );
  }

  if (state === "recording" || state === "paused") {
    const dotColor = state === "recording" ? "bg-red-500 animate-pulse" : "bg-amber-400";
    return (
      <div className={`${cardBase} border-solid border-accent/40 bg-accent/10`}>
        <div className="flex items-center gap-2">
          <span className={`w-2.5 h-2.5 rounded-full ${dotColor}`} />
          <span className="text-2xl font-mono tabular-nums">{formatElapsed(elapsedMs)}</span>
        </div>
        <div className="flex gap-2 mt-2">
          {state === "recording" ? (
            <button onClick={pause} className="px-3 py-1 rounded bg-surface hover:bg-white/10 text-sm">⏸ Pause</button>
          ) : (
            <button onClick={resume} className="px-3 py-1 rounded bg-surface hover:bg-white/10 text-sm">▶ Resume</button>
          )}
          <button onClick={stop} className="px-3 py-1 rounded bg-accent text-surface text-sm font-medium">⏹ Stop</button>
          <button onClick={discard} className="px-3 py-1 rounded bg-surface hover:bg-red-500/20 text-sm text-white/70">✕ Discard</button>
        </div>
      </div>
    );
  }

  if (state === "saving") {
    return (
      <div className={`${cardBase} border-solid border-white/10 bg-surface-secondary`}>
        <span className="spinner" />
        <p className="text-sm text-white/50">Saving…</p>
      </div>
    );
  }

  // error
  return (
    <div className={`${cardBase} border-solid border-red-500/30 bg-red-500/10`}>
      <span className="text-3xl opacity-50">⚠</span>
      <p className="text-sm text-red-300 text-center px-2">{errorMsg}</p>
      <button onClick={retry} className="mt-2 px-3 py-1 rounded bg-surface hover:bg-white/10 text-sm">Retry</button>
    </div>
  );
}
```

- [ ] **Step 2: Verify build**

Run: `cd /Users/nexus/projects/meister && npm run build`
Expected: clean — the component doesn't have any callers yet, but it must typecheck.

- [ ] **Step 3: Commit**

```bash
git add src/components/RecordCard.tsx
git commit -m "frontend: add RecordCard with full media controls"
```

---

## Task 6: Integrate `RecordCard` into `ProjectView` sources row

**Files:**
- Modify: `src/components/ProjectView.tsx`

- [ ] **Step 1: Update imports**

At the top of `src/components/ProjectView.tsx`, add:

```tsx
import RecordCard from "./RecordCard";
```

- [ ] **Step 2: Accept an optional active-capture callback from App**

Extend the `Props` interface:

```tsx
interface Props {
  projectId: string;
  onProjectChanged: () => void;
  onRecordingCreated: (r: Recording) => void;
  initialStagedPaths?: string[];
  onInitialStagedConsumed?: () => void;
  onCaptureActiveChange?: (discard: (() => void) | null) => void;
}
```

And destructure `onCaptureActiveChange` in the component signature.

- [ ] **Step 3: Replace the drop zone render with a Sources row**

Find the block in the JSX that currently renders (after the T49 reorder, the drop zone is above the staging list):

```tsx
<div className="flex flex-col gap-3">
  <RecordingDropZone onPicked={onFilesPicked} />
  {staging.length > 0 && (
    <StagingList ... />
  )}
  {recordings.length > 0 && (
    ...
  )}
</div>
```

Change the `<RecordingDropZone …>` line to a Sources row wrapping both controls:

```tsx
<div className="flex flex-col gap-3">
  <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
    <RecordingDropZone onPicked={onFilesPicked} />
    <RecordCard
      onPicked={onFilesPicked}
      onActiveChange={onCaptureActiveChange}
    />
  </div>
  {staging.length > 0 && (
    <StagingList ... />
  )}
  {recordings.length > 0 && (
    ...
  )}
</div>
```

Leave `staging` and `recordings` rendering untouched.

- [ ] **Step 4: Verify build**

Run: `npm run build`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add src/components/ProjectView.tsx
git commit -m "frontend: ProjectView — side-by-side drop zone + record card"
```

---

## Task 7: Mid-capture navigation guard in App + Sidebar

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/components/Sidebar.tsx`

- [ ] **Step 1: Lift active-capture state in `App.tsx`**

In `src/App.tsx`, inside the `App` component, add:

```tsx
const activeCaptureRef = useRef<null | (() => void)>(null);
const registerActiveCapture = useCallback(
  (discard: (() => void) | null) => {
    activeCaptureRef.current = discard;
  },
  [],
);

const guardedSelect = useCallback((next: Selection) => {
  if (activeCaptureRef.current) {
    if (!confirm("Recording in progress — stop and discard?")) return;
    activeCaptureRef.current();
    activeCaptureRef.current = null;
  }
  setSelection(next);
}, []);
```

Add `useRef` and `useCallback` to the imports if not already present.

- [ ] **Step 2: Wire into ProjectView and Sidebar**

Update the `<ProjectView />` render to pass the callback:

```tsx
<ProjectView
  projectId={selection.projectId}
  onProjectChanged={refreshProjects}
  onRecordingCreated={(r) => { ... existing ...; }}
  initialStagedPaths={ ... existing ... }
  onInitialStagedConsumed={() => setPendingStagedPaths(null)}
  onCaptureActiveChange={registerActiveCapture}
/>
```

Update the `<Sidebar />` render — change `onSelect={setSelection}` to `onSelect={guardedSelect}`:

```tsx
<Sidebar
  projects={projects}
  recordingsByProject={recordingsByProject}
  selection={selection}
  onSelect={guardedSelect}
  onProjectsChanged={refreshProjects}
  onPickFilesForProject={(projectId, paths) => {
    guardedSelect({ kind: "project", projectId });
    setPendingStagedPaths({ projectId, paths });
  }}
/>
```

Note: the inline "+ files" handler in App's Sidebar call already calls `setSelection` — replace it with `guardedSelect` too (as shown above).

- [ ] **Step 3: Verify build**

Run: `npm run build`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add src/App.tsx
git commit -m "frontend: guard sidebar navigation while a capture is active"
```

---

# Phase 3 — Verification

## Task 8: End-to-end verification

**Files:** none — verification only.

- [ ] **Step 1: Rust tests**

Run: `cd src-tauri && cargo test`
Expected: all pass (current count + the new `write_capture_tests` + the 3 new cleanup tests — roughly 45 tests total).

- [ ] **Step 2: Frontend build**

Run: `cd /Users/nexus/projects/meister && npm run build`
Expected: tsc + vite build clean.

- [ ] **Step 3: Manual runtime tests**

Launch: `npm run tauri dev` (ensure port 1420 is free).

Walk through each:

1. **Permission prompt.** Open a project. Click Record. Native macOS mic prompt appears on first run. Allow. Timer starts (`00:00` → `00:01` → …).

2. **Pause / Resume.** At ~5s, click Pause. Timer freezes. Card turns amber. Click Resume. Timer continues from 5s. Dot turns red again.

3. **Stop + stitch.** Stop at ~10s. Card briefly shows Saving… then returns to Idle. A new row `capture-YYYYMMDD-HHMMSS-xxxxxxxx.webm` appears in the staging list. Drop an `.mp3` file from Finder into the drop zone. Both files in staging. Reorder. Type a name. Click Create. RecordingView opens. Click Run. Pipeline completes with a transcript that covers both sources.

4. **Discard mid-capture.** Start a new capture. At ~3s click Discard. Card returns to Idle. `ls ~/Library/Application\ Support/com.meister.dev/captures/` should show no new file for this capture.

5. **Navigation guard.** Start a capture. In the sidebar, click another project. `confirm` prompt appears: "Recording in progress — stop and discard?". Cancel → capture continues. OK → capture is discarded, project switch happens.

6. **Permission denied.** Remove Meister from System Settings → Privacy & Security → Microphone (or deny the first prompt, then re-launch). Click Record. Card goes to Error state with the guidance copy. Click Retry — it re-initiates `getUserMedia`.

7. **Orphan cleanup on relaunch.** Manually drop a file into `<app_data>/captures/` (`touch …captures/orphan.webm`). Close and reopen Meister. The `orphan.webm` is gone (verify `ls`). Any file that's actually referenced by a recording is untouched.

- [ ] **Step 4: If anything fails, STOP and report.**

Do not mark this plan complete on partial success.

---

## Notes for the implementer

- **No new Rust deps.** `uuid` and `tempfile` are already in the project.
- **Filename timestamp uses civil-calendar arithmetic** from unix seconds (Hinnant's algorithm) to avoid pulling `chrono`. It's a handful of lines and has been battle-tested for 40+ years.
- **The `_discarded` flag on `MediaRecorder`** is a TypeScript-unclean escape hatch (we add an ad-hoc property). It's the simplest way to signal intent between the synchronous `discard` call and the asynchronous `onstop` handler. If the TypeScript compiler complains, keep the cast `(r as MediaRecorder & { _discarded?: boolean })` as shown.
- **The navigation guard uses `confirm`** rather than a custom modal — consistent with the existing project/recording delete flow which also uses `confirm`.
- **macOS mic-permission caching:** once a user has denied permission, macOS stops showing the prompt; the only path to re-enable is System Settings. The Retry button deliberately re-invokes `getUserMedia`, which will succeed if permission was granted in the meantime, or surface the same rejection again. The banner copy directs the user.
