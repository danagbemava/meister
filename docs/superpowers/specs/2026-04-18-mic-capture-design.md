# Mic Capture — Design

**Status:** Draft
**Date:** 2026-04-18

## Problem

Today a Meister recording's source files come from the user's filesystem — dragged, dropped, or picked. There is no way to capture audio directly inside the app. Users who want to transcribe a live meeting must use a separate recording tool (QuickTime, Audio Hijack, etc.), save a file, then drop it into Meister.

## Goals

- Record audio from the system's default microphone directly inside Meister.
- Captured files land in a project's staging area — the same surface that accepts dropped files — so users can stitch a mic capture with pre-existing files and reuse all the rename/reorder/Create UX.
- Full media controls during capture: Start, Pause, Resume, Stop, Discard.
- Orphan capture files are garbage-collected on next app launch.

## Non-goals

- **System audio capture** (recording Zoom/Meet/WhatsApp call audio). Ship as a separate spec using macOS ScreenCaptureKit or a BlackHole-style virtual-device workflow.
- **Screen recording.**
- **Input device picker.** Mic-only captures the system default. (BlackHole users who want call recording today would need a picker; deferred.)
- **Live input level meter, waveform preview, in-app trimming, multi-capture queueing.** All on the roadmap, none in this ship.

## Architecture

Browser `getUserMedia({ audio: true })` + `MediaRecorder` inside the Tauri WKWebView produces a WebM/Opus Blob. A new Tauri command `save_capture(bytes)` writes it to `<app_data>/captures/<filename>.webm` and returns the path. That path is fed to `ProjectView`'s existing `onFilesPicked([path])` callback — same code path as a dropped file — so staging, naming, reorder, and Create all work unchanged.

The Extract stage already runs `ffmpeg` concat over the source paths and produces a 16 kHz mono WAV for Whisper. WebM/Opus is a standard input ffmpeg accepts natively; no transcoding step needed.

Three additions to the codebase:

1. A new `RecordCard` React component (sibling of `RecordingDropZone`) that owns the capture lifecycle.
2. A new `save_capture` Tauri command.
3. A new `cleanup_orphaned_captures` function run at startup in `lib.rs::setup`, alongside the existing `mark_running_as_failed` crash-sweep.

## UI

### Layout

`ProjectView`'s Recordings section gains a "Sources row" above the staging list — two flex cards, equal width on wide screens, stacked on narrow:

```
Recordings
├─ ┌─ Sources row ──────────────────────────────────────────────┐
│  │ ┌────────────────────┐  ┌────────────────────────────────┐ │
│  │ │ 🎬                 │  │ 🎙                             │ │
│  │ │ Drop files or      │  │ Record audio                   │ │
│  │ │ click to browse    │  │                                │ │
│  │ └────────────────────┘  └────────────────────────────────┘ │
│  └────────────────────────────────────────────────────────────┘
│
├─ Staging list (when non-empty)
└─ Existing recordings list
```

The existing `RecordingDropZone` stays unchanged. The new `RecordCard` lives next to it.

### `RecordCard` states

| State | Visual | Controls |
|---|---|---|
| **Idle** | Gray mic icon, "Record audio" | click → Start |
| **Recording** | Pulsing red dot + timer `00:12` | ⏸ Pause · ⏹ Stop · ✕ Discard |
| **Paused** | Amber dot + frozen timer | ▶ Resume · ⏹ Stop · ✕ Discard |
| **Saving** | Spinner + "Saving…" | (none — brief) |
| **Error** | Red card + banner | Retry (re-requests permission / re-initiates capture) |

### After Stop

`save_capture` writes the file; the returned path is fed to `onFilesPicked([path])`. The Record card returns to Idle. The captured file appears in the staging list as another row — the existing rename, reorder, remove, and Create flow handles it the same as any dropped file.

### Discard

Aborts the `MediaRecorder`, throws the buffered chunks away, stops media tracks, skips the `save_capture` call. Nothing touches disk.

### Mid-capture navigation guard

When a capture is in progress, clicking another project or recording in the sidebar triggers a `confirm("Recording in progress — stop and discard?")`. If confirmed, the card runs its Discard path and navigation proceeds. The `isRecording` flag is lifted to `ProjectView` and passed to `Sidebar` via a new prop (or via a small React context for cleanliness — implementer's call).

## Capture stack

### Frontend

```ts
// Start
const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
const mime = MediaRecorder.isTypeSupported("audio/webm;codecs=opus")
  ? "audio/webm;codecs=opus"
  : undefined; // let browser pick
const recorder = mime ? new MediaRecorder(stream, { mimeType: mime })
                      : new MediaRecorder(stream);
const chunks: Blob[] = [];
recorder.ondataavailable = (e) => { if (e.data.size > 0) chunks.push(e.data); };
recorder.start();

// Pause / Resume
recorder.pause();
recorder.resume();

// Stop + save
recorder.onstop = async () => {
  const blob = new Blob(chunks, { type: recorder.mimeType || "audio/webm" });
  const bytes = new Uint8Array(await blob.arrayBuffer());
  const path = await invoke<string>("save_capture", { bytes: Array.from(bytes) });
  stream.getTracks().forEach((t) => t.stop());
  onPicked([path]);
};
recorder.stop();

// Discard — no save
recorder.stop();
stream.getTracks().forEach((t) => t.stop());
chunks.length = 0;
```

### Rust — `save_capture`

```rust
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
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("create captures dir: {e}"))?;
    let filename = format!(
        "capture-{}-{}.webm",
        chrono_timestamp(),
        short_uuid(),
    );
    let path = dir.join(filename);
    std::fs::write(&path, &bytes)
        .map_err(|e| format!("write capture: {e}"))?;
    Ok(path.to_string_lossy().to_string())
}
```

Helpers: `chrono_timestamp()` returns `YYYYMMDD-HHMMSS` as a plain string (can be done inline via `std::time::SystemTime` + formatting; no new deps needed). `short_uuid()` returns the first 8 chars of a v4 UUID for filename uniqueness.

Registered in `invoke_handler![…]` in `lib.rs`.

### Orphan cleanup

```rust
// in src-tauri/src/storage/mod.rs
pub fn cleanup_orphaned_captures(store: &Store, captures_dir: &Path) -> Result<usize, String> {
    let Ok(entries) = std::fs::read_dir(captures_dir) else { return Ok(0); };
    let conn = store.conn.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT source_paths FROM recordings")
        .map_err(|e| format!("prepare: {e}"))?;
    let referenced: std::collections::HashSet<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| format!("query: {e}"))?
        .filter_map(Result::ok)
        .flat_map(|json| serde_json::from_str::<Vec<String>>(&json).unwrap_or_default())
        .collect();
    drop(conn);

    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let s = path.to_string_lossy().to_string();
        if !referenced.contains(&s) {
            if std::fs::remove_file(&path).is_ok() { removed += 1; }
        }
    }
    Ok(removed)
}
```

Called from `lib.rs::setup` right after `mark_running_as_failed`:

```rust
let _ = storage::cleanup_orphaned_captures(&store, &app_data_dir.join("captures"));
```

Failure is non-fatal — a startup error here should not block the app.

## macOS permissions

Add to the bundle `Info.plist` via `src-tauri/tauri.conf.json`'s `bundle.macOS.infoPlist`:

```json
"NSMicrophoneUsageDescription": "Meister needs microphone access to record audio for transcription."
```

WKWebView forwards the first `getUserMedia({ audio: true })` call to the OS, which displays the native permission prompt. On deny, subsequent calls reject with `NotAllowedError`; we catch and show the "denied" card state.

## Error handling

| Error | Surface |
|---|---|
| `NotAllowedError` (permission denied) | Red card state + "Open System Settings → Privacy & Security → Microphone." + Retry |
| `NotFoundError` (no mic) | Red card state + "No microphone detected." + Retry |
| Unsupported mime | Silent fallback to `MediaRecorder` default |
| `save_capture` write failure | Banner on card; chunks kept in memory so Retry can re-attempt save. Discard still available. |
| Mid-capture app close | On next launch, orphan cleanup removes the unflushed file (which is already on disk via `save_capture` only if Stop completed; unsaved chunks are lost — acceptable). |

## File naming

- On disk: `<app_data>/captures/capture-YYYYMMDD-HHMMSS-<xxxxxxxx>.webm`
  (e.g. `capture-20260418-113045-a3c1b9e2.webm`)
- In the staging list: the basename is shown, prefixed with a 🎙 icon to distinguish from dropped files.

The filename is unique (uuid fragment), sorts chronologically by default, and is human-scannable.

## Testing

### Rust unit tests (new)

- `save_capture_writes_expected_file` — inject a temp dir in place of `app_data_dir` (via a helper), call with sample bytes, verify the file exists at the returned path and contains the bytes.
- `cleanup_orphaned_captures_keeps_referenced_files` — seed three files in a temp dir, insert a recording with one of them in `source_paths`, run the cleanup, assert exactly two deleted and the referenced one kept.

### Manual (UI)

1. **First run after install:** click Record → OS mic-permission prompt appears → allow → timer starts.
2. **Pause / Resume:** at 5s click Pause → timer freezes. Click Resume → timer continues from 5s.
3. **Stop + stitch:** Stop → `capture-…webm` appears in staging. Drop a second `.mp3` file. Reorder. Click Create. Run the pipeline. Transcript contains audio from both sources in the chosen order.
4. **Discard:** start → 3s → Discard. `ls <app_data>/captures/` shows no new file.
5. **Permission denied:** deny the prompt on first run → Record card shows denied banner → click Retry. (On macOS, once denied, the OS suppresses the prompt; Retry opens System Settings. Document this in the banner copy.)
6. **Mid-capture navigation:** click Record → click a different project in the sidebar → `confirm` appears. Cancel keeps recording; OK discards and navigates.
7. **Orphan cleanup on crash path:** start the app, click Record, `kill -9` the app. Relaunch. If a partial capture landed on disk (it won't, since `save_capture` only runs on Stop), it would be unreferenced and cleaned up — verify `<app_data>/captures/` is empty (or contains only referenced files) after relaunch.

## Future work

- **System-audio + screen capture** (Zoom/Meet/WhatsApp call recording) — separate spec, either native via ScreenCaptureKit or documented BlackHole workflow with a device picker.
- **Input device picker.**
- **Live level meter** during recording.
- **Post-capture waveform preview / trimming** before the file lands in staging.
- **Keyboard shortcut** for Start/Stop (e.g. `⌘R`).

## Open questions

None.
