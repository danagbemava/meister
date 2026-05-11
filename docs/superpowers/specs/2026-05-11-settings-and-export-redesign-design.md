# Settings, Export, and Ask Panel Redesign

Date: 2026-05-11
Status: design, ready for plan

## Motivation

Two patterns in the current app don't match how the user actually works:

1. **Settings are duplicated per project and per recording.** In practice the user does not change models, transcription parameters, or output formats per project — yet every `Project` carries a `defaultSettings` and every `Recording` carries its own `settings` snapshot. The `SettingsForm` is embedded in two places and shown inline in dense UI surfaces, when it would more naturally be a single app-wide configuration.
2. **The pipeline auto-generates files at the end.** The `save` stage writes one file per chosen `outputFormat` immediately after formatting, prompting the user with a sequence of save dialogs. The user wants the formatted content to live in the app as the canonical artifact and to export to a chosen document format on demand.

This spec also folds in a long-pending UI cleanup for the **Ask** feature: today it shares horizontal space with the output preview in a `flex` row; it should instead be a slide-in side panel.

## Goals

- Move all model / runtime knobs to a single global settings page reachable via `⌘,`.
- Replace the auto-saving `save` stage with explicit, on-demand export from the recording toolbar.
- Reposition the Ask panel as a trailing-edge slide-in overlay (RTL-aware).
- Migrate existing data losslessly without a formal migration framework.

## Non-goals

- Per-recording settings overrides (removed entirely; not coming back as a hidden panel).
- Cloud LLM or local `llama-cpp` integration (covered separately in `2026-05-05-ollama-removal-research.md`).
- Cross-version compatibility for downgrades (one-way migration is acceptable; old builds can't open data written by new builds).
- Multi-format batch export (export one format at a time; `⌘E` twice for two formats).

## Architecture & Data Model

### Type changes (`src/types/storage.ts`)

```ts
export type OutputFormat = "md" | "txt" | "docx" | "pdf";

export interface GlobalSettings {
  ollamaUrl: string;
  model: string;             // LLM
  whisperModel: string;
  language: string;          // default transcription language (e.g. "auto", "en")
  defaultExportFormat: OutputFormat;  // pre-selected in Save dialog
}

export interface Project {
  id: string;
  name: string;
  createdAt: number;
  updatedAt: number;
  template: Template;
  customInstructions?: string;
  language?: string | null;  // NEW — optional override; null/empty means use global default
  // REMOVED: defaultSettings
}

export interface Recording {
  id: string;
  projectId: string;
  name: string;
  sourcePaths: string[];
  createdAt: number;
  updatedAt: number;
  stages: StageRecord[];
  // REMOVED: settings
}

export type StageKind = "extract" | "transcribe" | "format";  // "save" removed
```

The `AppSettings` type is deleted. `savedPaths` on `StageRecord`, the `save` stage logic, and `recordSavedOutputs` are all removed. `modelUsed` per stage is retained — it remains the audit trail used for stage invalidation.

### Storage / backend

Persistence is SQLite (`meister.db`). The codebase already has a versioned migration framework in `src-tauri/src/storage/schema.rs` (currently at `user_version = 3`). This work introduces a single forward migration `v3 → v4`:

- Create table `app_settings` (single-row config): `ollama_url`, `model`, `whisper_model`, `language`, `default_export_format`, plus a `singleton` CHECK constraint to enforce one row.
- Seed `app_settings` from the most-recently-updated row in `projects.default_settings` before dropping that column. If no projects exist, insert sensible defaults (`ollama_url = "http://localhost:11434"`, `model = ""`, `whisper_model = "base"`, `language = "auto"`, `default_export_format = "md"`).
- `ALTER TABLE projects ADD COLUMN language TEXT;` (nullable).
- `ALTER TABLE projects DROP COLUMN default_settings;`
- `ALTER TABLE recordings DROP COLUMN settings;`
- `DELETE FROM stages WHERE stage = 'save';` (existing save rows are obsolete under the new pipeline).

New Tauri commands: `get_global_settings`, `set_global_settings`, `open_settings_window`. The `record_saved_outputs` command and the `save` stage handling in `orchestrator.rs` are removed. `run_recording` composes the effective settings each run by reading global settings and applying the recording's project's optional `language` override.

### Effective-settings resolution

```
effective.ollamaUrl     = global.ollamaUrl
effective.model         = global.model
effective.whisperModel  = global.whisperModel
effective.language      = (project.language ?? "").trim() || global.language
```

Project-level override applies only to `language`. All other fields are global with no override path. If the user wants to test a different model, they change global settings; this is the same friction they would have with the old project-level snapshot since they were not changing it anyway.

`completedStageInvalidation` in `RecordingView` (which compares each stage's `modelUsed` against current settings to decide if a re-run is needed) is updated to read the effective settings instead of `recording.settings`. The comparison logic itself does not change — only the source of the values.

### Frontend integration

- `useGlobalSettings()` hook backed by a small in-window store. Components read settings from this hook; writes go through the store and are persisted via `set_global_settings`.
- `SettingsForm.tsx` is reshaped to take a `GlobalSettings` and an `onChange`. The two prior embedding sites (`ProjectView`, `RecordingView`) are removed.
- `ProjectView` loses its "Default settings" section. It gains a slim "Language override" row (placeholder text "Use global default · {global.language}") that writes `project.language`.
- `RecordingView` loses its "Recording settings" disclosure entirely.

## UI Surfaces

### Settings window (`⌘,`)

A separate Tauri window, ~520×640, native title bar, vibrancy background matching the main window. Single scrollable form using the reshaped `SettingsForm`:

- **Ollama** — server URL, LLM model
- **Transcription** — whisper model, default language
- **Export** — default format (radio group: Markdown / Plain text / Word / PDF)

No Save button. Changes write on blur/change, matching macOS System Settings. The window can stay open while the user works; closing hides it (it is not unmounted), so reopen is instant.

Entry points:
- Menu item `Meister → Settings…` (`⌘,`)
- Gear icon in the sidebar footer

Both trigger the `open_settings_window` Tauri command.

### Export from recording (`⌘E` and toolbar button)

The recording toolbar gains a new **Export…** button placed next to **Ask**, between status and Delete. It is disabled until the `format` stage is complete (tooltip: "Format first to enable export").

Click / `⌘E` flow:

1. Native `saveDialog` opens with `filters` listing all four supported formats. The user's `defaultExportFormat` is listed first so it is the dialog's default filter.
2. Default filename is `${recording.name}.${defaultExportFormat}`.
3. The chosen filter determines the output format — not the typed extension. After the dialog returns, the export code normalizes the destination path so its extension matches the chosen filter (appending the correct extension if missing, replacing it if it differs). If the user types `report.txt` but the filter is `PDF`, the file is written to `report.pdf`.
4. Writing happens client-side using the existing helpers (`generateDocx`, `generatePdf`, `save_text_file`, `save_binary_file`). No new backend stage.
5. On success, a transient toast in the recording pane footer reads "Exported to {path}" with a "Reveal in Finder" affordance using `revealItemInDir` from `@tauri-apps/plugin-opener`.

Menu wiring: a new `menu:export` event, handled by `RecordingView` through the same imperative-handle pattern already used for `runPipeline`.

No persistence of the last export path. The only remembered preference is `defaultExportFormat` in global settings.

### Ask side panel

The current inline split (the `flex gap-4` row beside `OutputPreview`) is removed. The panel becomes a right-anchored slide-in overlay:

- Positioned with `inset-y-0 end-0`, width ~420px, full content height.
- RTL-aware via logical CSS properties (`end-0`, not `right-0`). In RTL locales the panel slides in from the visual left.
- Background uses the existing glass tokens; a subtle leading-edge shadow separates it from the content underneath.
- Toggled by the existing Ask button and `⌘K`. Disable / enable logic against `hasTranscript` is unchanged.
- The transcript stays in `OutputPreview` underneath the panel; citation click still scrolls and highlights the transcript via the existing `highlightRange` mechanism.
- Close affordances: `⌘K` toggles, `Esc` closes, plus a small close button in the panel header.
- The panel mounts on open and unmounts on close. `useAsk` already handles per-recording thread state, so re-opening restores the conversation.

Desktop-only app; narrow-window behavior is not specified.

## Migration

A single forward SQLite migration `v3 → v4` (see "Storage / backend" above for the full operation list) handles everything in one atomic transaction at app startup. Key points:

- `default_export_format` is seeded from the first entry of the source project's `default_settings.outputFormats` (or `"md"` if the array is empty).
- `Recording.stages` rows where `stage = 'save'` are deleted — the `format` stage is now terminal.
- The `Recording.saved_paths` JSON column on the `stages` table is no longer written. The column stays in place (SQLite makes column drops on older versions painful) and is simply ignored by readers; new writes pass `NULL`.
- The migration is one-way. There is no downgrade path; this is acceptable for a single-user local app.

## Testing

### Rust (`tests/storage_integration.rs`)

- Migrating from a v3 database that contains a project + recording + save-stage row leaves the database at `user_version = 4`, drops the `default_settings` and `settings` columns, populates `app_settings` with the seeded values, and removes the save-stage row.
- `get_global_settings` / `set_global_settings` round-trip against the new table.
- A project with `language = "de"` overrides the global `language = "auto"` in the effective settings; a project with `language = NULL` (or empty string) does not.

### Frontend

- `useGlobalSettings.test.ts`: multiple consumers in one window observe the same value; a write propagates to all subscribers.
- `SettingsForm.test.tsx`: each field updates the passed-in settings; format radios cover all four options; whisper-model picker still triggers the existing download modal flow.
- `RecordingView` tests:
  - Export button disabled when format output is empty; enabled when present.
  - Clicking Export opens `saveDialog` with `defaultExportFormat` first in the filters.
  - Filter choice drives which writer is called — mock `saveDialog`, assert `save_text_file` for md/txt and `save_binary_file` for docx/pdf with expected byte payloads.
  - `⌘E` triggers the same flow as the button.
- `AskPanel` tests: adapt the existing tests to the new overlay layout; assert `⌘K` toggles, `Esc` closes, citation click still calls the highlight callback.

### Manual

- Open settings window, change Ollama URL, switch back to main window — project pane reflects the change without reload.
- Toggle sidebar; `⌘,` still opens settings even with no project / recording selected.
- Re-run a recording that completed under the old build — invalidation reads global settings against `modelUsed` and surfaces the expected re-run warning.

## File Change Map

### Backend (`src-tauri/`)

- `src/storage/types.rs` — add `GlobalSettings`; remove `AppSettings` from `Project`; remove `settings` from `Recording`; add `language: Option<String>` to `Project`; remove `StageKind::Save`.
- `src/storage/schema.rs` — `CURRENT_VERSION = 4`; add `migrate_v3_to_v4`.
- `src/storage/mod.rs` — new `app_settings` module with `get` / `set`.
- `src/storage/projects.rs` — update CRUD to drop `default_settings`, add `language`.
- `src/storage/recordings.rs` — update CRUD to drop `settings`.
- `src/storage/stages.rs` — drop the `Save` branch references; `saved_paths` writes go away.
- `src/commands.rs` — add `get_global_settings`, `set_global_settings`, `open_settings_window`; remove `record_saved_outputs`; update any signatures that took `AppSettings`.
- `src/orchestrator.rs` — drop the `StageKind::Save` arm; iterate only `Extract/Transcribe/Format`; compose effective settings from `(global, project.language)` per run.
- `src/menu.rs` — re-enable the existing `menu:preferences` item (rename id to `menu:settings`, `⌘,`); add `Export…` (`⌘E`) under File.
- `src/lib.rs` — update `invoke_handler` list; in `setup`, create the hidden secondary `settings` window.
- `tauri.conf.json` — register the secondary `settings` window (label `settings`, `visible: false`, sized for a Preferences pane).
- `tests/storage_integration.rs` — migration test and effective-settings test.

### Frontend (`src/`)

- `types/storage.ts` — type changes as in Architecture above.
- `api/storage.ts` — add `getGlobalSettings`, `setGlobalSettings`, `openSettingsWindow`; remove `recordSavedOutputs`.
- `lib/useGlobalSettings.ts` — new hook + simple store.
- `lib/menuEvents.ts` — handlers for `menu:settings` and `menu:export`.
- `components/SettingsForm.tsx` — accept `GlobalSettings`, drop the project-specific embedding, add the default-export-format radios.
- `components/SettingsWindow.tsx` — new shell that renders in the `settings` Tauri window.
- `components/ProjectView.tsx` — remove "Default settings" section; add the small "Language override" row.
- `components/RecordingView.tsx` — remove inline Ask split; remove "Recording settings" disclosure; add Export button + `⌘E` wiring; mount `AskPanel` as the new slide-in overlay.
- `components/AskPanel.tsx` — wrap content in a positioned overlay using logical inset properties; add close button + Esc handler; keep `useAsk` internals.
- `components/OutputPreview.tsx` — remove the `savedPaths` display block.
- `main.tsx` — branch on `getCurrentWindow().label` to render `<App />` for the main window vs `<SettingsWindow />` for the settings window.
