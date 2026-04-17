# Sessions and Resume — Design

**Status:** Draft
**Date:** 2026-04-17

## Problem

Meister's pipeline (extract → transcribe → format → save) runs entirely in React memory. Two consequences:

1. **No history.** Every launch is a blank slate. You can't look back at a prior recording's transcript, re-export it in a new format, or see which files you've already processed.
2. **No resume.** If any stage fails (Ollama crashes mid-format, save dialog cancelled), the app drops you back to the input screen. All prior work — extracted audio, completed transcript — is gone. You re-pick the file, re-pick the template, re-run the 20-minute transcription.

We want persistent, named **projects** that group recordings, and a **resume** behavior so a failed stage doesn't nuke the stages before it.

## Goals

- Projects are first-class, navigable places. Each project owns a template + default settings + a list of recordings.
- Recordings persist: state, stage progress, transcripts, and formatted outputs survive app restarts and crashes.
- On failure, one "Resume" button continues from the failed stage using cached outputs of earlier stages.
- Changing a setting that invalidates a completed stage is detected; the user is warned before re-running.

## Non-goals

- Running multiple recordings in parallel (see Future Work).
- Per-stage manual rerun buttons (Run / Resume / Start over is sufficient).
- Cloud sync or multi-device history.
- Importing existing user files — pre-release app, no migration needed.

## Architecture

Three new moving parts:

1. **Persistence layer (Rust)** — a new `storage` module backed by SQLite (`rusqlite`, bundled). Owns schema, migrations, and all reads/writes of project and recording state.
2. **Pipeline orchestrator (Rust)** — one Tauri command `run_recording(recording_id)` that walks the four stages, skipping any whose cached output is still valid and running the rest. Idempotent — this is how both "Run" and "Resume" work.
3. **Sidebar-based UI (React)** — a persistent left sidebar replaces the current top-level wizard. Two main views: **Project view** (template + default settings + recordings list) and **Recording view** (stage cards + run controls + output preview).

The existing 4-step wizard components (`InputStep`, `SettingsStep`, `ProcessingStep`, `ResultStep`) are reorganized into these views rather than thrown away (see §UI).

## Data model

### Project

```ts
{
  id: string,              // uuid
  name: string,            // user-editable
  createdAt: number,       // unix ms
  updatedAt: number,
  template: {
    source: "inline" | "file",
    content: string,
    sourcePath?: string,   // remembered for reference; not a live link
  },
  defaultSettings: {
    ollamaUrl, model, whisperModel, language, outputFormats
  }
}
```

### Recording

```ts
{
  id: string,
  projectId: string,
  name: string,            // defaults to source filename (sans extension); editable
  sourcePath: string,      // path to the original dropped file — NOT copied
  createdAt: number,
  updatedAt: number,
  settings: AppSettings,   // snapshot of project.defaultSettings at creation; editable per-recording
  stages: {
    extract:    { status, error?, completedAt? },
    transcribe: { status, modelUsed?, error?, completedAt? },
    format:     { status, modelUsed?, templateHash?, error?, completedAt? },
    save:       { status, savedPaths?: string[], error?, completedAt? }
  }
}
```

**Stage status:** `pending | running | completed | failed`.

**Derived overall status:** `new | in_progress | completed | failed` (computed from stage statuses; used for the sidebar status dot).

### Notable decisions

- **Source recording files are not copied.** We store the path. Moving/deleting the original file breaks only the extract stage; all downstream artifacts (transcript, formatted output) remain usable. Trade-off accepted: 500 MB × 50 recordings would blow up disk.
- **`templateHash` on the format stage** detects when the project template has changed since the last successful format, so the UI can surface a "template edited — re-format?" hint.
- **Settings are snapshotted per recording** at creation time from the project defaults. Editing project defaults does not retroactively change existing recordings' settings.

## Storage

### File layout

Under the Tauri app data dir (`~/Library/Application Support/com.meister.dev/` on macOS):

```
<app_data>/
├── models/                                 # unchanged: whisper models
├── meister.db                              # SQLite: projects, recordings, stages, text artifacts
└── recordings/
    └── <recording-id>/
        └── audio.wav                       # extract artifact; deleted after transcribe succeeds
```

WAV files live on disk (too big for SQLite). Transcripts and formatted outputs live in SQLite as TEXT columns on the stage row — they're small, belong with their stage state, and this avoids the orphan-artifact failure mode.

### Schema

```sql
CREATE TABLE projects (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  template_source TEXT NOT NULL,       -- 'inline' | 'file'
  template_content TEXT NOT NULL,
  template_source_path TEXT,
  default_settings TEXT NOT NULL       -- JSON blob (AppSettings)
);

CREATE TABLE recordings (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  source_path TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  settings TEXT NOT NULL               -- JSON snapshot
);
CREATE INDEX idx_recordings_project ON recordings(project_id);

CREATE TABLE stages (
  recording_id TEXT NOT NULL REFERENCES recordings(id) ON DELETE CASCADE,
  stage TEXT NOT NULL,                 -- 'extract' | 'transcribe' | 'format' | 'save'
  status TEXT NOT NULL,                -- 'pending' | 'running' | 'completed' | 'failed'
  error TEXT,
  completed_at INTEGER,
  model_used TEXT,                     -- transcribe & format
  template_hash TEXT,                  -- format
  output TEXT,                         -- transcribe: transcript; format: formatted doc
  saved_paths TEXT,                    -- save: JSON array
  PRIMARY KEY (recording_id, stage)
);

PRAGMA user_version = 1;
```

### Stack

- **`rusqlite`** with the `bundled` feature (~1 MB binary growth, no system libsqlite dependency).
- One shared `Arc<Mutex<Connection>>` in Tauri state. Access wrapped in `tokio::task::spawn_blocking` from async commands.
- **Migrations** via `PRAGMA user_version` + a match on version → apply next migration. No migration library yet; revisit if/when schema changes more than a couple of times.
- **Atomicity**: a single stage update (status + output + completed_at + model_used) happens in one UPDATE statement — no partial state possible.

## Pipeline orchestrator

One Tauri command: `run_recording(recording_id)`. Idempotent.

```
for stage in [extract, transcribe, format, save]:
    state = db.stages(recording_id, stage)
    if state.status == 'completed' and inputs_unchanged(stage):
        continue                           # skip, reuse cached output from DB
    mark stage 'running'
    try:
        result = run_stage(...)
        write result to DB, mark 'completed'
    except e:
        mark 'failed', error = str(e)
        return                             # stop pipeline; UI shows failed stage
```

### `inputs_unchanged(stage)`

| Stage | Invalidates if… |
|---|---|
| extract | `sourcePath` changed (immutable per recording, so effectively never) |
| transcribe | `settings.whisperModel` or `settings.language` differs from `stages.transcribe.model_used` |
| format | `settings.model` differs from `stages.format.model_used`, OR current project `templateHash` differs from `stages.format.template_hash` |
| save | `settings.outputFormats` differs, OR any path in `stages.save.saved_paths` no longer exists on disk |

When a stage is re-run because of invalidation, **all downstream stages are reset to `pending`** before starting, so their outputs can't be reused against stale inputs.

### Recording actions

- **Run / Resume** — one button, label depends on state:
  - All stages `pending` → "Run"
  - Any stage `failed`, or a previously-completed stage invalidated by a settings/template change → "Resume"
  - All stages `completed` and nothing invalidated → button is disabled. A full redo goes through "Start over" first.
- **Start over** — escape hatch. Wipes all stage rows back to `pending`, deletes `audio.wav` if present, clears DB output columns. One click + confirm.

### WAV lifecycle

- Written by extract stage to `recordings/<id>/audio.wav`.
- Kept while transcribe is running or failed (so retry can skip extract).
- Deleted at the end of a **successful** transcribe stage. Nothing downstream needs it.
- Deleted when a recording is deleted, or when "Start over" is pressed.

### Concurrency

One recording runs at a time. A second `run_recording` call while one is in flight returns a "busy" error. Multi-recording queueing is Future Work.

### Crash recovery

On app start, any stage row in `status = 'running'` is from a crash. Mark it `failed` with error `"Interrupted (app closed during processing)"` before the UI loads. Idempotent — the recording shows up as failed; Resume works as usual.

## UI

### Layout

```
┌───────────────────────────────────────────────────────────────┐
│ TitleBar                                                      │
├──────────────────┬────────────────────────────────────────────┤
│ Sidebar          │  Main pane                                 │
│ (240px)          │                                            │
│                  │  Project view  OR  Recording view          │
│ + New project    │                                            │
│                  │                                            │
│ ▼ Q2 meetings    │                                            │
│   ▸ kickoff ●    │                                            │
│   ▸ wk2 ●        │                                            │
│ ▶ Standup log    │                                            │
└──────────────────┴────────────────────────────────────────────┘
```

### Sidebar

- Collapsible project nodes with their recordings beneath.
- Recording item: name + status dot (grey=new, blue=running, green=completed, red=failed).
- Click project header → project view. Click recording → recording view.
- Hover "…" menu or right-click → rename / delete, both for projects and recordings.
- "+ New project" button at the top.

### Project view (main pane when a project is selected)

- Editable project name (click-to-edit).
- **Template** section — editor + drop-zone for `.md`/`.txt`/`.docx` (reuses current `InputStep` template code).
- **Default settings** section — whisper model, Ollama model, language, output formats (reuses current `SettingsStep`).
- **Recordings** list with a drop-zone / "Add recording" button. Dropping a file creates a `Recording` (settings snapshotted from project defaults) and navigates to its view.

### Recording view (main pane when a recording is selected)

Single scrolling page, top to bottom:

1. **Header** — editable recording name, source file path (with a "file missing — Locate…" banner if the source path is gone).
2. **Stages** — four vertical stage cards (extract, transcribe, format, save). Each shows icon, label, status, timing, inline error if failed. Live progress bars for transcribe (% from whisper) and format (char count from Ollama stream) remain.
3. **Settings** — collapsible; snapshot of recording settings, editable inline. Editing a setting that would invalidate a completed stage shows a yellow banner explaining what will be re-run.
4. **Output preview** — transcript and formatted output, in tabs (reuses current `ResultStep`). Visible as soon as those stages complete.
5. **Actions bar** (sticky at bottom) — Run / Resume / Re-run + Start over.

### Edge cases

- **Source file missing**: red banner in Recording header with "Locate…" button that updates `source_path`. Only blocks the extract stage; downstream output remains viewable.
- **Template edited since last format**: subtle hint on the format stage card ("template changed — re-format?"). Doesn't auto-invalidate; kicks in only when user hits Run/Resume.
- **Project delete**: confirm dialog ("Delete 'X' and its N recordings? Cannot be undone."). Hard delete. DB cascade removes recordings and stages; recordings' on-disk folders are also removed. User-saved output files (`.md`, `.docx` etc. at paths the user picked) are left alone.
- **Recording delete**: confirm dialog. Deletes DB row and on-disk folder. User-saved output files left alone.

### Empty states

- No projects → main pane shows a "Create your first project" card.
- Project with no recordings → main pane shows project settings + a large drop zone.

### Reuse from existing components

- `InputStep` → TemplateEditor + RecordingDropZone (both in Project view).
- `SettingsStep` → ProjectSettings (Project view) + RecordingSettings (Recording view, collapsible).
- `ProcessingStep` → its stage pipeline becomes the vertical StagesList (Recording view).
- `ResultStep` → OutputPreview section (Recording view).
- `TitleBar`, `DownloadModelModal` — unchanged.

## Error handling

Covered in §Pipeline orchestrator. Summary:

- Stage failure → stage card in red with the error message, pipeline halts, Resume button offered.
- Crash during a stage → next launch marks the `running` stage as `failed` with an "Interrupted" error; Resume works as usual.
- Missing source file → banner with Locate button; only blocks extract.
- Settings invalidating a completed stage → yellow banner warning; stage reset happens at Run/Resume time, not at edit time.

## Testing

- **Unit (Rust)**: `storage` module CRUD; migration from empty schema; cascade deletes; `inputs_unchanged` truth table for each stage.
- **Integration (Rust)**: `run_recording` end-to-end against a tiny sample WAV + a mock Ollama HTTP server; verify skip-completed-stages behavior; verify failure halts pipeline and persists error; verify crash recovery (seed a `running` row, start app, assert it's marked failed).
- **Manual (UI)**: drop a recording, kill Ollama mid-format, confirm Resume completes without re-transcribing; change whisper model on a completed recording, confirm warning banner and downstream reset.

## Future work

- **Multi-recording queue** — run several recordings back-to-back without user interaction.
- **Per-stage manual rerun** — a "redo this stage" affordance on stage cards, for power users.
- **Session export / import** — pack a project into a zip for backup or sharing.
- **Cloud sync** — sync projects across devices.
- **Search** — full-text search across transcripts and formatted outputs.

## Open questions

None at design time. Implementation plan will flesh out remaining details (e.g., exact button placement, sidebar width responsiveness).
