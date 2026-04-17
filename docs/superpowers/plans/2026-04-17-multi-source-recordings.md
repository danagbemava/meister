# Multi-Source Recordings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a recording wrap N ordered source files (1..many); concatenate them via a single ffmpeg invocation in the existing Extract stage.

**Architecture:** `Recording.source_path: String` becomes `source_paths: Vec<String>` in the Rust model and SQLite (migration v2), and `sourcePaths: string[]` on the frontend. The Extract stage uses `ffmpeg -filter_complex concat` for N≥2 inputs; for N=1 the existing simple command is kept. `ProjectView` gains an inline staging list with drag-and-drop reorder before commit. `RecordingView` displays a numbered source list with per-row "Locate…" for missing files.

**Tech Stack:** Rust (rusqlite bundled, serde), React 18 + TypeScript + Tailwind, Tauri 2, ffmpeg CLI. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-04-17-multi-source-recordings-design.md`

---

## File structure

### Rust — modified files
- `src-tauri/src/storage/types.rs` — replace `source_path: String` with `source_paths: Vec<String>` on `Recording`.
- `src-tauri/src/storage/schema.rs` — add `migrate_v1_to_v2`, bump `CURRENT_VERSION` to 2.
- `src-tauri/src/storage/recordings.rs` — update INSERT/UPDATE/SELECT SQL and row mapping.
- `src-tauri/src/commands.rs` — extract pure `build_ffmpeg_args`; rewrite `run_extract` to accept `&[String]`; update `create_recording` signature; delete legacy `extract_audio`/`transcribe_audio`/`format_transcript` Tauri command wrappers.
- `src-tauri/src/orchestrator.rs` — pass `&rec.source_paths` to `run_extract`.
- `src-tauri/src/lib.rs` — remove deleted commands from `invoke_handler![…]`.
- `src-tauri/tests/storage_integration.rs` — update `sample_recording` helper; add tests for v1→v2 migration and multi-source list persistence.

### Frontend — modified files
- `src/types/storage.ts` — `sourcePath: string` → `sourcePaths: string[]`.
- `src/api/storage.ts` — `createRecording` signature updated to pass `sourcePaths: string[]`.
- `src/components/RecordingDropZone.tsx` — prop renamed to `onPicked(paths: string[])`; accepts multi-file drop and multi-select file picker.
- `src/components/ProjectView.tsx` — track `staging: string[]`, render `StagingList` when non-empty, commit on Create.
- `src/components/RecordingView.tsx` — numbered source list with per-row "Locate…"; replaces the top-level "file missing" banner.

### Frontend — new files
- `src/components/StagingList.tsx` — ordered, drag-reorderable list for pending sources, with per-row Remove X and Cancel/Create footer.

---

# Phase 1 — Rust data model

## Task 1: Swap `source_path` → `source_paths: Vec<String>` across Rust + migration v2

**Why one task:** a single atomic change. The type, the SQL, the migration, and the existing test fixtures must change together for the crate to compile.

**Files:**
- Modify: `src-tauri/src/storage/types.rs`
- Modify: `src-tauri/src/storage/schema.rs`
- Modify: `src-tauri/src/storage/recordings.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/tests/storage_integration.rs`

- [ ] **Step 1: Update the `Recording` struct**

In `src-tauri/src/storage/types.rs`, change the field:

Before:
```rust
#[serde(rename = "sourcePath")]
pub source_path: String,
```

After:
```rust
#[serde(rename = "sourcePaths")]
pub source_paths: Vec<String>,
```

Leave every other field on `Recording` unchanged.

- [ ] **Step 2: Bump schema version + add v1→v2 migration**

In `src-tauri/src/storage/schema.rs`, change `const CURRENT_VERSION: i32 = 1;` to `const CURRENT_VERSION: i32 = 2;`.

Add a new arm to the `match version` block inside `migrate`:

```rust
    while version < CURRENT_VERSION {
        match version {
            0 => migrate_v0_to_v1(conn)?,
            1 => migrate_v1_to_v2(conn)?,
            n => return Err(format!("no migration from version {n}")),
        }
        version += 1;
        conn.execute_batch(&format!("PRAGMA user_version = {version};"))
            .map_err(|e| format!("bump user_version: {e}"))?;
    }
```

Append the migration function at the end of the file:

```rust
fn migrate_v1_to_v2(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        r#"
        ALTER TABLE recordings ADD COLUMN source_paths TEXT;
        UPDATE recordings SET source_paths = json_array(source_path);
        ALTER TABLE recordings DROP COLUMN source_path;
        "#,
    )
    .map_err(|e| format!("apply v2 schema: {e}"))
}
```

- [ ] **Step 3: Update recordings CRUD SQL and row mapping**

In `src-tauri/src/storage/recordings.rs`:

Replace the body of `insert` with:

```rust
pub fn insert(store: &Store, r: &Recording) -> Result<(), String> {
    let conn = store.conn.lock().unwrap();
    let settings_json = serde_json::to_string(&r.settings)
        .map_err(|e| format!("serialize settings: {e}"))?;
    let source_paths_json = serde_json::to_string(&r.source_paths)
        .map_err(|e| format!("serialize source_paths: {e}"))?;
    conn.execute(
        "INSERT INTO recordings \
         (id, project_id, name, source_paths, created_at, updated_at, settings) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            r.id, r.project_id, r.name, source_paths_json,
            r.created_at, r.updated_at, settings_json,
        ],
    )
    .map_err(|e| format!("insert recording: {e}"))?;

    for kind in StageKind::ALL {
        conn.execute(
            "INSERT INTO stages (recording_id, stage, status) VALUES (?1, ?2, ?3)",
            params![r.id, kind.as_str(), StageStatus::Pending.as_str()],
        )
        .map_err(|e| format!("insert stage: {e}"))?;
    }
    Ok(())
}
```

Replace the body of `update`:

```rust
pub fn update(store: &Store, r: &Recording) -> Result<(), String> {
    let conn = store.conn.lock().unwrap();
    let settings_json = serde_json::to_string(&r.settings)
        .map_err(|e| format!("serialize settings: {e}"))?;
    let source_paths_json = serde_json::to_string(&r.source_paths)
        .map_err(|e| format!("serialize source_paths: {e}"))?;
    let n = conn.execute(
        "UPDATE recordings SET \
         name = ?2, source_paths = ?3, updated_at = ?4, settings = ?5 \
         WHERE id = ?1",
        params![r.id, r.name, source_paths_json, r.updated_at, settings_json],
    )
    .map_err(|e| format!("update recording: {e}"))?;
    if n == 0 {
        return Err(format!("recording not found: {}", r.id));
    }
    Ok(())
}
```

In both `get` and `list_by_project`, update the SELECT column list from `... source_path, ...` to `... source_paths, ...` (column 3 of the row is the serialized list now, not a plain string).

Replace `row_to_recording_without_stages`:

```rust
fn row_to_recording_without_stages(row: &rusqlite::Row) -> Result<Recording, String> {
    let source_paths_json: String = row.get(3).map_err(|e| format!("source_paths: {e}"))?;
    let source_paths: Vec<String> = serde_json::from_str(&source_paths_json)
        .map_err(|e| format!("parse source_paths: {e}"))?;
    let settings_json: String = row.get(6).map_err(|e| format!("settings: {e}"))?;
    let settings = serde_json::from_str(&settings_json)
        .map_err(|e| format!("parse settings: {e}"))?;
    Ok(Recording {
        id: row.get(0).map_err(|e| format!("id: {e}"))?,
        project_id: row.get(1).map_err(|e| format!("project_id: {e}"))?,
        name: row.get(2).map_err(|e| format!("name: {e}"))?,
        source_paths,
        created_at: row.get(4).map_err(|e| format!("created_at: {e}"))?,
        updated_at: row.get(5).map_err(|e| format!("updated_at: {e}"))?,
        settings,
        stages: vec![],
    })
}
```

Also update the v0→v1 migration SQL in `schema.rs` — NO, leave it alone. A fresh v0 DB goes through v1 with the old `source_path` column, then immediately v2 which swaps it to `source_paths`. Correct behavior preserved.

- [ ] **Step 4: Update `create_recording` Tauri command signature**

In `src-tauri/src/commands.rs`, find `pub async fn create_recording(…)`. Replace its body with:

```rust
#[tauri::command]
pub async fn create_recording(
    store: tauri::State<'_, Arc<Store>>,
    project_id: String,
    name: String,
    source_paths: Vec<String>,
    settings: AppSettings,
) -> Result<Recording, String> {
    let store = (*store).clone();
    tokio::task::spawn_blocking(move || {
        let id = uuid::Uuid::new_v4().to_string();
        let now = now_ms();
        let r = Recording {
            id,
            project_id,
            name,
            source_paths,
            created_at: now,
            updated_at: now,
            settings,
            stages: vec![],
        };
        storage::recordings::insert(&store, &r)?;
        storage::recordings::get(&store, &r.id)?
            .ok_or_else(|| "recording vanished after insert".to_string())
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}
```

- [ ] **Step 5: Update the `sample_recording` test fixture**

In `src-tauri/tests/storage_integration.rs`, replace the `sample_recording` helper:

Before:
```rust
fn sample_recording(project_id: &str) -> Recording {
    Recording {
        id: "r1".to_string(),
        project_id: project_id.to_string(),
        name: "kickoff".to_string(),
        source_path: "/tmp/kickoff.mp4".to_string(),
        created_at: 1_500,
        updated_at: 1_500,
        settings: default_settings(),
        stages: vec![],
    }
}
```

After:
```rust
fn sample_recording(project_id: &str) -> Recording {
    Recording {
        id: "r1".to_string(),
        project_id: project_id.to_string(),
        name: "kickoff".to_string(),
        source_paths: vec!["/tmp/kickoff.mp4".to_string()],
        created_at: 1_500,
        updated_at: 1_500,
        settings: default_settings(),
        stages: vec![],
    }
}
```

- [ ] **Step 6: Compile**

Run: `cd src-tauri && cargo check`
Expected: clean. Pre-existing dead-code warnings (`OllamaModel`, `OllamaTagsResponse`, `run_save_outputs`) stay.

- [ ] **Step 7: Run the full test suite**

Run: `cd src-tauri && cargo test`
Expected: all existing tests pass. (No new tests yet — new tests land in Task 5.)

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/storage/types.rs \
        src-tauri/src/storage/schema.rs \
        src-tauri/src/storage/recordings.rs \
        src-tauri/src/commands.rs \
        src-tauri/tests/storage_integration.rs
git commit -m "storage: switch Recording to source_paths: Vec<String>; schema v2 migration"
```

---

## Task 2: Extract `build_ffmpeg_args` as a pure function + unit tests

**Files:**
- Modify: `src-tauri/src/commands.rs`

- [ ] **Step 1: Write failing unit tests**

Append to `src-tauri/src/commands.rs` (near the bottom, or in a `#[cfg(test)] mod tests` block if one exists — if not, add one):

```rust
#[cfg(test)]
mod build_ffmpeg_args_tests {
    use super::build_ffmpeg_args;
    use std::path::PathBuf;

    #[test]
    fn single_file_uses_simple_vn_shape() {
        let paths = vec!["/a.mp4".to_string()];
        let out = PathBuf::from("/tmp/out.wav");
        let args = build_ffmpeg_args(&paths, &out);
        assert_eq!(
            args,
            vec![
                "-i", "/a.mp4",
                "-vn",
                "-acodec", "pcm_s16le",
                "-ar", "16000",
                "-ac", "1",
                "-y",
                "/tmp/out.wav",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
        );
        assert!(!args.iter().any(|a| a == "-filter_complex"));
    }

    #[test]
    fn multi_file_uses_concat_filter() {
        let paths = vec![
            "/a.mp4".to_string(),
            "/b.mov".to_string(),
            "/c.mp3".to_string(),
        ];
        let out = PathBuf::from("/tmp/out.wav");
        let args = build_ffmpeg_args(&paths, &out);

        let input_count = args.iter().filter(|a| *a == "-i").count();
        assert_eq!(input_count, 3);

        let filter_idx = args
            .iter()
            .position(|a| a == "-filter_complex")
            .expect("should have filter_complex");
        assert_eq!(
            args[filter_idx + 1],
            "[0:a][1:a][2:a]concat=n=3:v=0:a=1[out]"
        );

        let map_idx = args.iter().position(|a| a == "-map").expect("should have -map");
        assert_eq!(args[map_idx + 1], "[out]");

        // Common output tail.
        assert!(args.windows(2).any(|w| w[0] == "-acodec" && w[1] == "pcm_s16le"));
        assert!(args.windows(2).any(|w| w[0] == "-ar" && w[1] == "16000"));
        assert!(args.windows(2).any(|w| w[0] == "-ac" && w[1] == "1"));
        assert_eq!(args.last().unwrap(), "/tmp/out.wav");
    }

    #[test]
    fn two_file_concat_string_is_well_formed() {
        let paths = vec!["/a.mp4".to_string(), "/b.mp4".to_string()];
        let out = PathBuf::from("/tmp/out.wav");
        let args = build_ffmpeg_args(&paths, &out);
        let filter_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        assert_eq!(args[filter_idx + 1], "[0:a][1:a]concat=n=2:v=0:a=1[out]");
    }
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cd src-tauri && cargo test --lib build_ffmpeg_args_tests`
Expected: FAIL (function `build_ffmpeg_args` not defined).

- [ ] **Step 3: Implement `build_ffmpeg_args`**

Still in `src-tauri/src/commands.rs`, add the function above `pub async fn run_extract(…)`:

```rust
/// Build the ffmpeg argv (without the `ffmpeg` binary itself) for extracting
/// audio to a 16 kHz mono WAV. For a single input, uses the simple `-vn`
/// shape; for multiple inputs, uses `filter_complex concat` to stitch their
/// audio streams together in order.
pub fn build_ffmpeg_args(input_paths: &[String], output_wav: &std::path::Path) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    for p in input_paths {
        args.push("-i".to_string());
        args.push(p.clone());
    }
    if input_paths.len() > 1 {
        let mut filter = String::new();
        for i in 0..input_paths.len() {
            filter.push_str(&format!("[{i}:a]"));
        }
        filter.push_str(&format!("concat=n={}:v=0:a=1[out]", input_paths.len()));
        args.push("-filter_complex".to_string());
        args.push(filter);
        args.push("-map".to_string());
        args.push("[out]".to_string());
    } else {
        args.push("-vn".to_string());
    }
    args.push("-acodec".to_string());
    args.push("pcm_s16le".to_string());
    args.push("-ar".to_string());
    args.push("16000".to_string());
    args.push("-ac".to_string());
    args.push("1".to_string());
    args.push("-y".to_string());
    args.push(output_wav.to_string_lossy().to_string());
    args
}
```

- [ ] **Step 4: Run tests to verify pass**

Run: `cd src-tauri && cargo test --lib build_ffmpeg_args_tests`
Expected: all 3 PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "commands: add build_ffmpeg_args pure function for single + multi-source"
```

---

## Task 3: `run_extract` takes `&[String]` + pre-flight missing file check

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/orchestrator.rs`

- [ ] **Step 1: Rewrite `run_extract`**

In `src-tauri/src/commands.rs`, replace `pub async fn run_extract(…)` with:

```rust
/// Extract audio from one or more input files into a single 16 kHz mono WAV.
/// Inputs are concatenated in order when there is more than one.
pub async fn run_extract(
    input_paths: &[String],
    output_wav: &std::path::Path,
) -> Result<(), String> {
    if input_paths.is_empty() {
        return Err("no source files provided".to_string());
    }
    let missing: Vec<&str> = input_paths
        .iter()
        .filter(|p| !std::path::Path::new(p).exists())
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
    let output = Command::new("ffmpeg")
        .args(&args)
        .output()
        .map_err(|e| format!("ffmpeg not found or failed to start: {e}. Install via: brew install ffmpeg"))?;
    if !output.status.success() {
        return Err(format!(
            "ffmpeg error:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}
```

- [ ] **Step 2: Update the orchestrator call site**

In `src-tauri/src/orchestrator.rs`, find the `StageKind::Extract` branch in `run_recording`. Replace:

```rust
            StageKind::Extract => {
                run_extract(&rec.source_path, &wav_path).await
                    .map(|_| storage::stages::CompletedOutput { ... })
            }
```

With:

```rust
            StageKind::Extract => {
                run_extract(&rec.source_paths, &wav_path).await
                    .map(|_| storage::stages::CompletedOutput {
                        output: None,
                        model_used: None,
                        template_hash: None,
                        saved_paths: None,
                        completed_at: now_ms(),
                    })
            }
```

(If the existing code already spells out the `CompletedOutput` fields inline, just change `&rec.source_path` → `&rec.source_paths`.)

- [ ] **Step 3: Compile**

Run: `cd src-tauri && cargo check`
Expected: clean.

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test`
Expected: all pass (including the 3 `build_ffmpeg_args` tests from Task 2).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/orchestrator.rs
git commit -m "extract: take slice of source paths; pre-flight missing-file check"
```

---

## Task 4: Remove legacy `extract_audio` / `transcribe_audio` / `format_transcript` Tauri commands

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`

These three Tauri command wrappers were kept for backward compatibility with the old UI, which is gone. The internal `run_extract` / `run_transcribe` / `run_format` helpers stay — they are used by the orchestrator.

- [ ] **Step 1: Delete the three `#[tauri::command]` wrappers from `commands.rs`**

Find and remove the entire bodies of:

```rust
#[tauri::command]
pub async fn extract_audio(...) { ... }

#[tauri::command]
pub async fn transcribe_audio(...) { ... }

#[tauri::command]
pub async fn format_transcript(...) { ... }
```

Leave `run_extract`, `run_transcribe`, and `run_format` (no `#[tauri::command]` attribute) in place.

- [ ] **Step 2: Remove the three commands from `invoke_handler![…]`**

In `src-tauri/src/lib.rs`, delete these lines from the `invoke_handler![…]` macro:

```
commands::extract_audio,
commands::transcribe_audio,
commands::format_transcript,
```

- [ ] **Step 3: Compile**

Run: `cd src-tauri && cargo check`
Expected: clean.

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "commands: drop legacy extract_audio/transcribe_audio/format_transcript wrappers"
```

---

## Task 5: Integration tests — v1→v2 migration + multi-source list persistence

**Files:**
- Modify: `src-tauri/tests/storage_integration.rs`

- [ ] **Step 1: Append migration test**

Append to `src-tauri/tests/storage_integration.rs`:

```rust
#[test]
fn migrates_v1_to_v2_wraps_source_path_into_array() {
    use rusqlite::Connection;

    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();

    // Seed a v1-shaped DB manually (pre-migration schema).
    conn.execute_batch(
        r#"
        CREATE TABLE projects (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            template_source TEXT NOT NULL,
            template_content TEXT NOT NULL,
            template_source_path TEXT,
            default_settings TEXT NOT NULL
        );
        CREATE TABLE recordings (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            source_path TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            settings TEXT NOT NULL
        );
        CREATE TABLE stages (
            recording_id TEXT NOT NULL REFERENCES recordings(id) ON DELETE CASCADE,
            stage TEXT NOT NULL,
            status TEXT NOT NULL,
            error TEXT,
            completed_at INTEGER,
            model_used TEXT,
            template_hash TEXT,
            output TEXT,
            saved_paths TEXT,
            PRIMARY KEY (recording_id, stage)
        );
        PRAGMA user_version = 1;
        "#,
    )
    .unwrap();

    let settings_json = serde_json::to_string(&default_settings()).unwrap();
    conn.execute(
        "INSERT INTO projects (id, name, created_at, updated_at, template_source, template_content, default_settings) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params!["p1", "proj", 1i64, 1i64, "inline", "", &settings_json],
    ).unwrap();
    conn.execute(
        "INSERT INTO recordings (id, project_id, name, source_path, created_at, updated_at, settings) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params!["r1", "p1", "rec", "/x.mp4", 2i64, 2i64, &settings_json],
    ).unwrap();

    meister_lib::storage::schema::migrate(&conn).unwrap();

    // user_version bumped.
    let v: i32 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, 2);

    // source_paths is a JSON array with the old path.
    let sp: String = conn
        .query_row(
            "SELECT source_paths FROM recordings WHERE id = 'r1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(sp, r#"["/x.mp4"]"#);

    // source_path column is gone.
    let mut stmt = conn.prepare("PRAGMA table_info(recordings)").unwrap();
    let cols: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    assert!(cols.contains(&"source_paths".to_string()));
    assert!(!cols.contains(&"source_path".to_string()));
}

#[test]
fn recordings_store_source_paths_list_preserves_order() {
    let store = Store::open_memory().unwrap();
    storage::projects::insert(&store, &sample_project()).unwrap();

    let paths = vec![
        "/a.mp4".to_string(),
        "/b.mov".to_string(),
        "/c.mp3".to_string(),
    ];
    let mut r = sample_recording("p1");
    r.source_paths = paths.clone();
    storage::recordings::insert(&store, &r).unwrap();

    let loaded = storage::recordings::get(&store, "r1").unwrap().unwrap();
    assert_eq!(loaded.source_paths, paths);
}
```

- [ ] **Step 2: Run the integration test suite**

Run: `cd src-tauri && cargo test --test storage_integration`
Expected: all pass, including the 2 new tests.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/tests/storage_integration.rs
git commit -m "tests: v1→v2 migration + multi-source list persistence"
```

---

# Phase 2 — Frontend

## Task 6: TS types + API wrapper

**Files:**
- Modify: `src/types/storage.ts`
- Modify: `src/api/storage.ts`

- [ ] **Step 1: Update `Recording` type in `src/types/storage.ts`**

Change:

```ts
sourcePath: string;
```

to:

```ts
sourcePaths: string[];
```

Leave every other field alone.

- [ ] **Step 2: Update `createRecording` wrapper in `src/api/storage.ts`**

Change the `createRecording` signature from:

```ts
createRecording: (
    projectId: string,
    name: string,
    sourcePath: string,
    settings: AppSettings,
) => invoke<Recording>("create_recording", { projectId, name, sourcePath, settings }),
```

to:

```ts
createRecording: (
    projectId: string,
    name: string,
    sourcePaths: string[],
    settings: AppSettings,
) => invoke<Recording>("create_recording", { projectId, name, sourcePaths, settings }),
```

- [ ] **Step 3: Build — expect type errors to surface callers**

Run: `npm run build`
Expected: FAIL (TypeScript errors) — `ProjectView.tsx` and `RecordingView.tsx` still reference `sourcePath`. That's OK; those callers are updated in Tasks 9 and 10. To keep the commit compileable, do a temporary compatibility shim for now, then remove it in the UI tasks.

Alternative: commit this task together with Task 9 and Task 10 as a single compile-coherent change. That's what we'll do.

**Revise Step 3:** _Do not run `npm run build` yet_. This task's commit is deferred until Tasks 9 and 10 land — they're all committed together.

- [ ] **Step 4: Stage the changes but do not commit yet**

Run:
```bash
git add src/types/storage.ts src/api/storage.ts
```

Do NOT commit. This task's changes will be committed together with Tasks 9 + 10 when the frontend compiles cleanly.

---

## Task 7: `RecordingDropZone` accepts multiple files

**Files:**
- Modify: `src/components/RecordingDropZone.tsx`

- [ ] **Step 1: Replace the file with the multi-file version**

Overwrite `src/components/RecordingDropZone.tsx`:

```tsx
import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";

interface Props {
  onPicked: (filePaths: string[]) => void;
}

const AUDIO_VIDEO_EXTS = new Set([
  "mp4", "mov", "mkv", "avi", "mp3", "m4a", "wav", "ogg", "webm", "aac", "flac",
]);

function isAudioVideo(name: string): boolean {
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  return AUDIO_VIDEO_EXTS.has(ext);
}

export default function RecordingDropZone({ onPicked }: Props) {
  const [dragging, setDragging] = useState(false);

  const openFile = async () => {
    const paths = await open({
      multiple: true,
      filters: [{ name: "Audio / Video", extensions: [...AUDIO_VIDEO_EXTS] }],
    });
    if (!paths) return;
    const list = Array.isArray(paths) ? paths : [paths];
    if (list.length > 0) onPicked(list);
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setDragging(false);
    const files = Array.from(e.dataTransfer.files);
    const picked: string[] = [];
    for (const file of files) {
      if (!isAudioVideo(file.name)) continue;
      const path = (file as File & { path?: string }).path ?? file.name;
      picked.push(path);
    }
    if (picked.length > 0) onPicked(picked);
  };

  return (
    <div
      className={`rounded-xl border-2 border-dashed flex flex-col items-center justify-center gap-2 cursor-pointer py-10 transition-colors
        ${dragging
          ? "border-accent bg-accent/10"
          : "border-white/10 hover:border-white/20 bg-surface-secondary"}`}
      onDragOver={(e) => { e.preventDefault(); setDragging(true); }}
      onDragLeave={() => setDragging(false)}
      onDrop={handleDrop}
      onClick={openFile}
    >
      <span className="text-3xl opacity-30">🎬</span>
      <p className="text-sm text-white/40 text-center px-4">
        Drop one or more files here, or click to browse
      </p>
      <p className="text-xs text-white/25">mp4 · mov · mp3 · m4a · wav · …</p>
    </div>
  );
}
```

- [ ] **Step 2: Stage the change**

```bash
git add src/components/RecordingDropZone.tsx
```

Do NOT commit yet — ProjectView still calls the old `(path: string) => void` shape. This lands with Task 9.

---

## Task 8: New `StagingList` component

**Files:**
- Create: `src/components/StagingList.tsx`

- [ ] **Step 1: Create the file**

Write `src/components/StagingList.tsx`:

```tsx
import { useState } from "react";

interface Props {
  paths: string[];
  onReorder: (next: string[]) => void;
  onRemove: (index: number) => void;
  onCancel: () => void;
  onCreate: () => void;
}

function basename(p: string): string {
  return p.split(/[\\/]/).pop() ?? p;
}

export default function StagingList({
  paths,
  onReorder,
  onRemove,
  onCancel,
  onCreate,
}: Props) {
  const [draggingIdx, setDraggingIdx] = useState<number | null>(null);
  const [overIdx, setOverIdx] = useState<number | null>(null);

  const handleDragStart = (idx: number) => (e: React.DragEvent) => {
    setDraggingIdx(idx);
    e.dataTransfer.effectAllowed = "move";
  };

  const handleDragOver = (idx: number) => (e: React.DragEvent) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
    if (idx !== overIdx) setOverIdx(idx);
  };

  const handleDrop = (dropIdx: number) => (e: React.DragEvent) => {
    e.preventDefault();
    if (draggingIdx === null || draggingIdx === dropIdx) {
      setDraggingIdx(null);
      setOverIdx(null);
      return;
    }
    const next = [...paths];
    const [moved] = next.splice(draggingIdx, 1);
    next.splice(dropIdx, 0, moved);
    onReorder(next);
    setDraggingIdx(null);
    setOverIdx(null);
  };

  const handleDragEnd = () => {
    setDraggingIdx(null);
    setOverIdx(null);
  };

  return (
    <div className="rounded-xl border border-white/10 bg-surface-secondary p-3 flex flex-col gap-2">
      <ul className="flex flex-col">
        {paths.map((p, idx) => (
          <li
            key={`${p}-${idx}`}
            draggable
            onDragStart={handleDragStart(idx)}
            onDragOver={handleDragOver(idx)}
            onDrop={handleDrop(idx)}
            onDragEnd={handleDragEnd}
            className={`flex items-center gap-2 px-2 py-1.5 rounded text-sm
              ${overIdx === idx ? "bg-accent/10" : ""}
              ${draggingIdx === idx ? "opacity-40" : ""}`}
          >
            <span className="text-white/30 cursor-grab select-none" title="Drag to reorder">⋮⋮</span>
            <span className="text-white/30 tabular-nums w-5 text-right">{idx + 1}.</span>
            <span className="flex-1 truncate text-white/80" title={p}>
              {basename(p)}
            </span>
            <button
              onClick={() => onRemove(idx)}
              className="text-white/40 hover:text-red-400 text-xs px-1"
              title="Remove"
            >
              ✕
            </button>
          </li>
        ))}
      </ul>
      <div className="flex items-center justify-end gap-2 pt-2 border-t border-white/5">
        <button
          onClick={onCancel}
          className="px-3 py-1.5 rounded text-xs text-white/60 hover:text-white"
        >
          Cancel
        </button>
        <button
          onClick={onCreate}
          disabled={paths.length === 0}
          className="px-4 py-1.5 rounded text-sm bg-accent text-surface disabled:opacity-30 disabled:cursor-not-allowed"
        >
          Create
        </button>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Stage the file**

```bash
git add src/components/StagingList.tsx
```

Do NOT commit yet — lands with Task 9.

---

## Task 9: Integrate staging into `ProjectView` + compile + commit Phase 2a

**Files:**
- Modify: `src/components/ProjectView.tsx`

- [ ] **Step 1: Rewrite `ProjectView.tsx`**

Replace the entire file with:

```tsx
import { useCallback, useEffect, useState } from "react";
import TemplateEditor from "./TemplateEditor";
import SettingsForm from "./SettingsForm";
import RecordingDropZone from "./RecordingDropZone";
import StagingList from "./StagingList";
import { api } from "../api/storage";
import type { Project, Recording } from "../types/storage";

interface Props {
  projectId: string;
  onProjectChanged: () => void;
  onRecordingCreated: (r: Recording) => void;
}

function basename(p: string): string {
  return p.split(/[\\/]/).pop() ?? p;
}

function deriveName(paths: string[]): string {
  const first = basename(paths[0] ?? "recording");
  return first.replace(/\.[^.]+$/, "");
}

export default function ProjectView({
  projectId,
  onProjectChanged,
  onRecordingCreated,
}: Props) {
  const [project, setProject] = useState<Project | null>(null);
  const [recordings, setRecordings] = useState<Recording[]>([]);
  const [renaming, setRenaming] = useState(false);
  const [draftName, setDraftName] = useState("");
  const [staging, setStaging] = useState<string[]>([]);

  const load = useCallback(async () => {
    const list = await api.listProjects();
    const p = list.find((x) => x.id === projectId) ?? null;
    setProject(p);
    if (p) setDraftName(p.name);
    const recs = await api.listRecordings(projectId);
    setRecordings(recs);
  }, [projectId]);

  useEffect(() => { load(); }, [load]);

  // Clear staging when switching projects.
  useEffect(() => { setStaging([]); }, [projectId]);

  const save = async (next: Project) => {
    await api.updateProject(next);
    setProject(next);
    onProjectChanged();
  };

  const onFilesPicked = (picked: string[]) => {
    setStaging((prev) => {
      const seen = new Set(prev);
      const next = [...prev];
      for (const p of picked) {
        if (!seen.has(p)) {
          next.push(p);
          seen.add(p);
        }
      }
      return next;
    });
  };

  const removeAt = (idx: number) =>
    setStaging((prev) => prev.filter((_, i) => i !== idx));

  const cancelStaging = () => setStaging([]);

  const createFromStaging = async () => {
    if (!project || staging.length === 0) return;
    const rec = await api.createRecording(
      projectId,
      deriveName(staging),
      staging,
      project.defaultSettings,
    );
    setStaging([]);
    setRecordings((rs) => [...rs, rec]);
    onRecordingCreated(rec);
  };

  const deleteProject = async () => {
    if (!confirm(`Delete '${project?.name}' and its ${recordings.length} recordings? This cannot be undone.`)) return;
    await api.deleteProject(projectId);
    onProjectChanged();
  };

  if (!project) return <div className="p-8 text-white/40">Loading…</div>;

  return (
    <div className="h-full overflow-y-auto scrollable p-8 flex flex-col gap-8">
      <div className="flex items-center justify-between">
        {renaming ? (
          <input
            autoFocus
            className="input text-xl font-semibold"
            value={draftName}
            onChange={(e) => setDraftName(e.target.value)}
            onBlur={async () => {
              const name = draftName.trim() || project.name;
              setRenaming(false);
              if (name !== project.name) await save({ ...project, name });
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") (e.target as HTMLInputElement).blur();
              if (e.key === "Escape") { setDraftName(project.name); setRenaming(false); }
            }}
          />
        ) : (
          <h1
            onClick={() => setRenaming(true)}
            className="text-xl font-semibold cursor-text hover:text-accent"
            title="Click to rename"
          >
            {project.name}
          </h1>
        )}
        <button
          onClick={deleteProject}
          className="text-xs text-white/40 hover:text-red-400"
        >
          Delete project
        </button>
      </div>

      <TemplateEditor
        template={project.template}
        onChange={(template) => save({ ...project, template })}
      />

      <div>
        <h2 className="text-xs font-semibold text-white/40 uppercase tracking-wider mb-3">
          Default settings
        </h2>
        <SettingsForm
          settings={project.defaultSettings}
          onChange={(defaultSettings) => save({ ...project, defaultSettings })}
        />
      </div>

      <div>
        <h2 className="text-xs font-semibold text-white/40 uppercase tracking-wider mb-3">
          Recordings
        </h2>
        <div className="flex flex-col gap-3">
          {recordings.length > 0 && (
            <ul className="flex flex-col gap-1">
              {recordings.map((r) => (
                <li key={r.id} className="text-sm text-white/70 px-3 py-1.5 bg-surface-secondary rounded">
                  {r.name}
                </li>
              ))}
            </ul>
          )}
          {staging.length > 0 && (
            <StagingList
              paths={staging}
              onReorder={setStaging}
              onRemove={removeAt}
              onCancel={cancelStaging}
              onCreate={createFromStaging}
            />
          )}
          <RecordingDropZone onPicked={onFilesPicked} />
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Verify the frontend compiles (except for `RecordingView`, which still references `sourcePath`)**

Run: `npm run build`
Expected: FAIL — `RecordingView.tsx` references `rec.sourcePath`. That's fixed in Task 10.

- [ ] **Step 3: Stage the changes**

```bash
git add src/components/ProjectView.tsx
```

Do NOT commit yet — wait for Task 10 to make the frontend compile.

---

## Task 10: Update `RecordingView` — numbered source list + per-row Locate

**Files:**
- Modify: `src/components/RecordingView.tsx`

- [ ] **Step 1: Rewrite the Source section of `RecordingView.tsx`**

In `src/components/RecordingView.tsx`, locate the state declarations near the top of the component (around `const [sourceMissing, setSourceMissing] = useState(false);`) and the JSX that renders `<p className="text-xs text-white/40 truncate">Source: {rec.sourcePath}</p>` plus the banner `{sourceMissing && …}`. Replace them with per-path tracking.

Replace the existing `const [sourceMissing, setSourceMissing] = useState(false);` block AND its effect with:

```tsx
const [missingByPath, setMissingByPath] = useState<Record<string, boolean>>({});

useEffect(() => {
  if (!rec) return;
  let cancelled = false;
  (async () => {
    const entries: [string, boolean][] = [];
    for (const p of rec.sourcePaths) {
      try {
        const ok = await invoke<boolean>("path_exists", { path: p });
        entries.push([p, !ok]);
      } catch {
        entries.push([p, false]);
      }
    }
    if (!cancelled) {
      setMissingByPath(Object.fromEntries(entries));
    }
  })();
  return () => { cancelled = true; };
}, [rec?.id, rec?.sourcePaths?.join("|")]);
```

Replace the existing `const locateSource = async () => { … }` with an index-aware version:

```tsx
const locateSource = async (idx: number) => {
  if (!rec) return;
  const path = await open({ multiple: false });
  if (typeof path !== "string") return;
  const nextPaths = [...rec.sourcePaths];
  nextPaths[idx] = path;
  const next = { ...rec, sourcePaths: nextPaths };
  setRec(next);
  await api.updateRecording(next);
  onRecordingChanged();
};
```

(Note: `open` is already imported at the top of `RecordingView.tsx` from `@tauri-apps/plugin-dialog` — no new import needed.)

Replace the single-source JSX block:

```tsx
<p className="text-xs text-white/40 truncate">Source: {rec.sourcePath}</p>

{sourceMissing && (
  <div className="bg-red-500/10 border border-red-500/20 rounded-lg p-3 text-sm flex items-center justify-between">
    <span className="text-red-400">Original file is missing.</span>
    <button
      onClick={locateSource}
      className="text-accent hover:text-accent-hover"
    >
      Locate…
    </button>
  </div>
)}
```

with:

```tsx
<div className="flex flex-col gap-1">
  <span className="text-xs font-medium text-white/50 uppercase tracking-wider">
    {rec.sourcePaths.length > 1 ? "Sources" : "Source"}
  </span>
  <ul className="flex flex-col gap-1">
    {rec.sourcePaths.map((p, idx) => {
      const missing = !!missingByPath[p];
      return (
        <li
          key={`${p}-${idx}`}
          className={`flex items-center gap-3 px-3 py-1.5 rounded text-xs font-mono
            ${missing
              ? "bg-red-500/10 border border-red-500/20"
              : "bg-surface-secondary"}`}
        >
          <span className="text-white/30 tabular-nums w-5 shrink-0">
            {rec.sourcePaths.length > 1 ? `${idx + 1}.` : ""}
          </span>
          <span className={`flex-1 truncate ${missing ? "text-red-400" : "text-white/60"}`} title={p}>
            {p}
          </span>
          {missing && (
            <button
              onClick={() => locateSource(idx)}
              className="text-accent hover:text-accent-hover shrink-0"
            >
              Locate…
            </button>
          )}
        </li>
      );
    })}
  </ul>
</div>
```

- [ ] **Step 2: Build**

Run: `npm run build`
Expected: clean (tsc + vite both succeed).

- [ ] **Step 3: Commit Phase 2a (Tasks 6, 7, 8, 9, 10 together)**

```bash
git add src/types/storage.ts \
        src/api/storage.ts \
        src/components/RecordingDropZone.tsx \
        src/components/StagingList.tsx \
        src/components/ProjectView.tsx \
        src/components/RecordingView.tsx
git commit -m "frontend: multi-source recordings — staging list + numbered sources + per-row locate"
```

(If `git status` shows any of these already committed, this command is a no-op for those files; adjust the `git add` list.)

---

# Phase 3 — Verification

## Task 11: End-to-end verification

**Files:** none — verification only.

- [ ] **Step 1: Rust tests**

Run: `cd src-tauri && cargo test`
Expected: all pass (36+ tests including the new `build_ffmpeg_args` unit tests and the new migration + multi-source CRUD integration tests).

- [ ] **Step 2: Frontend build**

Run: `cd /Users/nexus/projects/meister/.worktrees/sessions-and-resume && npm run build`
Expected: tsc + vite build clean.

- [ ] **Step 3: Manual runtime smoke test**

Run `npm run tauri dev` (ensure port 1420 is free — kill any stale vite server first).

Walk through:

1. **Upgrade path on existing data.** If the DB from the previous feature still has recordings, the first launch should migrate them in place. Open an existing recording → source list shows a single-row numbered list with the original path. No data loss.
2. **Single-file recording.** Drop one file into the project drop zone → staging list shows 1 row → click Create → RecordingView opens, Source section shows one row, no number shown. Run → pipeline succeeds as before.
3. **Two-file stitch.** Drop two files in one drag event → staging list shows both in drop order → drag row 2 above row 1 → Create → RecordingView Sources section shows 2 numbered rows in the reversed order. Run → Extract stage succeeds; transcript in the output preview reflects both files' audio, in order.
4. **Three-file stitch with mixed types.** Drop an mp4, a mov, and an mp3 in sequence (three drops) → staging accumulates three rows. Reorder so mp3 is last → Create → Run → pipeline succeeds.
5. **Remove + cancel.** Stage 3 files → click ✕ on middle row → rows renumber correctly → click Cancel → staging clears. Drop zone still visible, no recording created.
6. **Missing file + Locate.** Run a multi-source recording to Completed → close app → in Finder, move source file #2 to a new location → reopen recording in Meister. Row 2 is red with "Locate…" button. Click Locate → pick moved file → row turns normal. Hit Run → Extract re-runs and succeeds.
7. **Invalid file in drop.** Try dropping a `.txt` or other non-audio file — should be silently filtered; staging does not add it.

- [ ] **Step 4: If anything fails, STOP and report.**

Do not mark this plan complete on partial success.

- [ ] **Step 5: Commit — if a final docs/changelog note is worth adding, otherwise no-op**

No commit required unless manual verification discovered a fix needed.

---

## Notes for the implementer

- **Task 1 is intentionally large.** It's an atomic refactor — you cannot partially apply it and keep the crate compiling. Do it in one commit, in the order listed.
- **Tasks 6–10 are staged but not committed individually.** Task 10's final step commits them together. This is because each task alone leaves the frontend in a non-compiling state; only after all five changes are in place does `npm run build` succeed.
- **HTML5 native drag-and-drop** is used instead of a library like `@dnd-kit` to keep dep count low. It has quirks (e.g. `dragover.preventDefault` required to accept drops) that the `StagingList` code already handles.
- **`rusqlite`'s `bundled` feature** ships SQLite ≥ 3.44, so `ALTER TABLE ... DROP COLUMN` is safe. Do not second-guess this.
- **If a stored v1 DB from a prior session exists**, the migration runs silently at app startup (via the `Store::open` → `schema::migrate` path already in `src-tauri/src/lib.rs`). Test 6 in the manual pass explicitly exercises this path.
