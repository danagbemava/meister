# Multi-Source Recordings — Design

**Status:** Draft
**Date:** 2026-04-17

## Problem

Today a `Recording` has a single `sourcePath`. Users need to stitch several recordings (e.g. a meeting split into three files, or a lecture recorded in chunks) into one transcript. File order matters — mis-ordered audio produces a garbled transcript. The current data model and extract stage can't express multiple ordered inputs.

## Goals

- A recording can have 1..N source files, ordered, immutable after creation.
- Creation flow lets the user drop multiple files at once, reorder by drag-and-drop, and commit.
- Extract stage concatenates the ordered sources into a single WAV that the rest of the pipeline (transcribe → format → save) consumes unchanged.
- Single-file recordings remain the common case and work exactly as before.

## Non-goals

- Adding or removing sources after a recording is created (except for "Locate…" when a source file moves).
- Per-source audio normalization (Whisper is robust to volume variation).
- Per-source transcript annotations ("transcript from file 2 begins here").
- Parallel stitching of several recordings — still one recording at a time (see sessions spec's Future Work).

## Architecture

One change to the data model: `Recording.sourcePath: string` becomes `sourcePaths: string[]`. Extract stage gains a new shape: for N > 1 sources it runs one ffmpeg invocation using `filter_complex concat` to produce the same 16 kHz mono WAV the current pipeline consumes. Nothing else in the pipeline changes.

The creation flow in `ProjectView` becomes two-step: files drop into an inline staging list → user reorders / removes → clicks "Create" to commit. Everything after creation is immutable (with one exception: "Locate…" when a specific file path has moved on disk, covered below).

## Data model

### Rust — `storage/types.rs`

```rust
pub struct Recording {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub source_paths: Vec<String>,   // was: source_path: String
    pub created_at: i64,
    pub updated_at: i64,
    pub settings: AppSettings,
    pub stages: Vec<StageRecord>,
}
```

### TypeScript — `types/storage.ts`

```ts
export interface Recording {
  id: string;
  projectId: string;
  name: string;
  sourcePaths: string[];    // was: sourcePath: string
  createdAt: number;
  updatedAt: number;
  settings: AppSettings;
  stages: StageRecord[];
}
```

### SQLite schema — migration v1 → v2

```sql
ALTER TABLE recordings ADD COLUMN source_paths TEXT;
UPDATE recordings SET source_paths = json_array(source_path);
ALTER TABLE recordings DROP COLUMN source_path;
```

- Migration is additive then destructive (add new column, backfill from old, drop old).
- `rusqlite 0.31` with `bundled` feature includes SQLite ≥ 3.44, so `DROP COLUMN` is available.
- Migration runs inside existing `storage::schema::migrate` before `mark_running_as_failed` and before the Store is registered in Tauri state, so no command can observe a half-migrated DB.
- Any failure surfaces as `Store::open` returning `Err`, which aborts startup.

### Immutability

`source_paths` is write-once. The only permitted post-creation mutation is a single-path correction via the "Locate…" flow when a source file is moved/renamed on disk. That's a path fix, not a content change — still the same file the user meant.

Extract's `inputs_unchanged` rule stays as "always true if Completed" — consistent with today.

## Extract stage

### ffmpeg concat strategy

The `concat` *filter* (not demuxer) handles heterogeneous inputs — different codecs, sample rates, channel counts — and re-encodes uniformly. The demuxer would be faster (stream copy) but requires identical codecs, which we can't guarantee across user files.

**For N = 1** — keep the existing simple command. No filter overhead for the common case:

```
ffmpeg -i file.mp4 -vn -acodec pcm_s16le -ar 16000 -ac 1 -y output.wav
```

**For N ≥ 2** — filter_complex concat:

```
ffmpeg \
  -i file1.mp4 -i file2.mov -i file3.mp3 \
  -filter_complex "[0:a][1:a][2:a]concat=n=3:v=0:a=1[out]" \
  -map "[out]" \
  -acodec pcm_s16le -ar 16000 -ac 1 \
  -y output.wav
```

`concat=n=N:v=0:a=1` means "concat N streams, 0 video outputs, 1 audio output". Video tracks on inputs are dropped automatically.

### Rust change — `commands.rs`

Extract a pure `build_ffmpeg_args` function to make the arg-construction testable:

```rust
pub fn build_ffmpeg_args(input_paths: &[String], output_wav: &Path) -> Vec<String> { ... }
```

`run_extract` becomes:

```rust
pub async fn run_extract(
    input_paths: &[String],
    output_wav: &Path,
) -> Result<(), String> {
    if input_paths.is_empty() {
        return Err("no source files provided".to_string());
    }
    let missing: Vec<&str> = input_paths
        .iter()
        .filter(|p| !Path::new(p).exists())
        .map(|s| s.as_str())
        .collect();
    if !missing.is_empty() {
        return Err(format!("source file(s) missing:\n  {}", missing.join("\n  ")));
    }
    if let Some(parent) = output_wav.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create output dir: {e}"))?;
    }
    let args = build_ffmpeg_args(input_paths, output_wav);
    let output = Command::new("ffmpeg").args(&args).output()
        .map_err(|e| format!("ffmpeg not found or failed to start: {e}. Install via: brew install ffmpeg"))?;
    if !output.status.success() {
        return Err(format!("ffmpeg error:\n{}", String::from_utf8_lossy(&output.stderr)));
    }
    Ok(())
}
```

### Orchestrator change

`run_recording`'s Extract branch passes `&rec.source_paths` (slice of String) instead of `&rec.source_path`. No other orchestrator logic changes.

### Legacy Tauri command cleanup

The following commands were kept as thin wrappers during the sessions-and-resume migration; they have no callers in the new UI and should be removed in the same change:

- `extract_audio`
- `transcribe_audio`
- `format_transcript`

Remove the `#[tauri::command]` wrappers and delete the entries from `invoke_handler![…]` in `lib.rs`. The internal `run_extract` / `run_transcribe` / `run_format` helpers stay — they're used by the orchestrator.

## UI

### ProjectView — creation flow with staging

Today: dropping a file into `RecordingDropZone` immediately creates a recording. That becomes a two-step flow with an inline staging list above the drop zone.

```
Recordings
├─ (existing recordings list)
│
├─ ┌─ Staging (only visible when ≥1 file queued) ──────────┐
│  │ ⋮⋮ kickoff.mp4                                    ✕  │
│  │ ⋮⋮ wk2.mov                                        ✕  │
│  │ ⋮⋮ wk3.mp3                                        ✕  │
│  │                                      [Cancel] [Create]│
│  └──────────────────────────────────────────────────────┘
│
└─ ┌─ Drop zone ────────────────────────────────────────────┐
   │ Drop a recording here, or click to browse              │
   └────────────────────────────────────────────────────────┘
```

**New component: `src/components/StagingList.tsx`** — renders the ordered row list, handles drag-and-drop reordering via native HTML5 DnD (no new library; list is short, 2-5 items typical), and emits ordered paths to the parent.

**Changes to `RecordingDropZone`:**
- Prop renames: `onPicked(filePath: string)` → `onPicked(filePaths: string[])`.
- File picker opens with `open({ multiple: true })` so Finder allows Cmd+click multi-select.
- Drop events accept `dataTransfer.files` (all of them) instead of `files[0]`, filtered by extension.

**Parent (ProjectView) changes:**
- Tracks staging state: `const [staging, setStaging] = useState<string[]>([])`.
- On drop/pick: append new paths to staging, dedupe against existing staged paths.
- Renders `StagingList` when `staging.length > 0`.
- Passes current staging and handlers (reorder, remove, cancel, create) to `StagingList`.
- "Create" calls `api.createRecording(projectId, deriveName(staging), staging, defaultSettings)`, clears staging, navigates to the new recording.
- Default recording name = first file's basename without extension (e.g. `kickoff`). Editable after creation via the existing click-to-rename.

### RecordingView — read-only source list

Current: `<p className="text-xs text-white/40 truncate">Source: {rec.sourcePath}</p>`.

New: numbered source list (used even for N=1, for consistency):

```
Sources:
  1. /Users/you/Movies/kickoff.mp4              ✓
  2. /Users/you/Movies/wk2.mov                  ⚠ Locate…
  3. /Users/you/Movies/wk3.mp3                  ✓
```

- Each row calls `path_exists` on mount + when the path changes.
- A missing row shows a red background + inline "Locate…" button.
- Clicking "Locate…" opens a file picker; the chosen path replaces that single entry via `api.updateRecording`. Other entries untouched.
- The previous top-level "Original file missing" banner is removed (per-row UX supersedes it).

### Small UI component impact summary

- **New:** `StagingList.tsx`.
- **Modified:** `ProjectView.tsx` (staging state), `RecordingDropZone.tsx` (multi-file prop), `RecordingView.tsx` (source list + per-row Locate).
- **Unchanged:** `StageCard`, `OutputPreview`, `SettingsForm`, `TemplateEditor`, `Sidebar`.

## Error handling

- **Zero files at Create time:** "Create" button disabled when staging is empty.
- **Source file missing at Run time:** `run_extract` returns `Err` naming every missing file. Extract stage card shows red with the list. User resolves by clicking "Locate…" per row in the source list (not by hitting Resume — Resume is a no-op while files are missing). After fixing, click Run again.
- **File with no audio stream:** ffmpeg's stderr references the input index (e.g. `Input #1 has no audio`). We surface that verbatim on the Extract stage card. User can identify the offending file by position.
- **Crash during multi-file extract:** existing `mark_running_as_failed` sweep handles it. Resume restarts Extract from scratch (concat is idempotent).
- **Migration failure:** `Store::open` returns `Err`, app startup aborts with the error message. User's DB is untouched (SQLite statement-level atomicity).

## Testing

**Rust unit tests (new):**
- `build_ffmpeg_args_single_file` — asserts no `filter_complex` for N=1, expected simple arg shape.
- `build_ffmpeg_args_multi_file` — asserts `filter_complex` with `[0:a][1:a][2:a]concat=n=3:v=0:a=1[out]` and `-map [out]` for N=3.
- `build_ffmpeg_args_always_writes_pcm_s16le_16k_mono` — asserts codec/rate/channels tail regardless of N.

**Rust integration tests (new in `storage_integration.rs`):**
- `migrates_v1_to_v2_wraps_source_path_into_array` — seed a v1-shaped `recordings` table with a `source_path` column, run `Store::open`, verify: `source_paths = '["/x"]'`, `source_path` column gone (`PRAGMA table_info`).
- `recordings_store_source_paths_list` — insert a recording with 3 source_paths, get it, assert order preserved.

**Manual (UI):**
- Drop 3 files → staging shows 3 rows in drop order. Drag row 3 above row 1 → order updates. Remove middle row → remaining rows renumbered. Click Create → recording appears in the list. Open it → source list shows exactly 2 rows in the expected order.
- Delete one of the source files from Finder, reopen the recording → that row is red with "Locate…". Click Locate, pick the moved file → row turns green. Click Run → Extract succeeds.
- Single-file recording still works end-to-end (drop 1 file, Create, Run, finish).

## Future work

- **Per-source audio trimming** (e.g. "skip first 10 seconds of file 2") — would need a richer source representation than a bare path.
- **Per-source transcript markers** — e.g. inject `--- [kickoff.mp4 ends] ---` between segments in the raw transcript.
- **Audio normalization across sources** — apply loudness match before concat.
- **Live reordering of an existing recording** (drop the immutability constraint). Would invalidate Extract + Transcribe + Format + Save via the existing `inputs_unchanged` machinery.

## Open questions

None.
