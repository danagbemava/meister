# Sessions and Resume — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace in-memory pipeline state with persistent SQLite-backed projects and recordings, so closing/crashing the app keeps history and a failed pipeline stage resumes from where it failed.

**Architecture:** A new Rust `storage` module (rusqlite-bundled) owns a single `meister.db` under the app data dir. A single idempotent Rust command `run_recording` walks the four pipeline stages (extract → transcribe → format → save), skipping any whose cached output is still valid. The React UI is restructured around a persistent left sidebar of projects/recordings, with two main-pane views (project, recording) replacing the existing 4-step wizard.

**Tech Stack:** Rust (rusqlite bundled, sha2, uuid) on the backend; existing React 18 + TypeScript + Tailwind on the frontend. No new JS deps.

**Spec:** `docs/superpowers/specs/2026-04-17-sessions-and-resume-design.md`

---

## File structure

### Rust — new files
- `src-tauri/src/storage/mod.rs` — module root; connection init, public re-exports.
- `src-tauri/src/storage/schema.rs` — schema SQL + migration runner.
- `src-tauri/src/storage/types.rs` — `Project`, `Recording`, `StageKind`, `StageStatus`, `StageRecord`, `AppSettings`.
- `src-tauri/src/storage/hash.rs` — `template_hash(content: &str) -> String` (sha256 hex).
- `src-tauri/src/storage/projects.rs` — project CRUD.
- `src-tauri/src/storage/recordings.rs` — recording CRUD + helper to load a full recording with its stages.
- `src-tauri/src/storage/stages.rs` — stage read/update helpers + `reset_downstream` + `mark_running_as_failed`.
- `src-tauri/src/orchestrator.rs` — `inputs_unchanged` + `run_recording` command.
- `src-tauri/tests/storage_integration.rs` — integration tests against `:memory:` DB.

### Rust — modified files
- `src-tauri/Cargo.toml` — add `rusqlite = { version = "0.31", features = ["bundled"] }`, `sha2 = "0.10"`, `uuid = { version = "1", features = ["v4"] }`.
- `src-tauri/src/lib.rs` — init storage connection, register storage + orchestrator modules, add new commands to `invoke_handler`, run crash recovery on startup.
- `src-tauri/src/commands.rs` — expose project/recording CRUD Tauri commands; refactor `extract_audio` / `transcribe_audio` / `format_transcript` into internal `fn` helpers callable from the orchestrator (keep the existing Tauri commands as thin wrappers for now for manual testing).

### React — new files
- `src/types/storage.ts` — TS mirrors of Rust types.
- `src/api/storage.ts` — typed `invoke(...)` wrappers.
- `src/components/Sidebar.tsx` — project/recording tree.
- `src/components/ProjectView.tsx` — project main-pane view.
- `src/components/RecordingView.tsx` — recording main-pane view.
- `src/components/StageCard.tsx` — one stage's status card (used in RecordingView).
- `src/components/TemplateEditor.tsx` — extracted from `InputStep` (template textarea + drop zone + "Load from file").
- `src/components/RecordingDropZone.tsx` — extracted from `InputStep` (drop/open a recording file).
- `src/components/SettingsForm.tsx` — extracted from `SettingsStep` (the form contents, reusable for project defaults and per-recording).
- `src/components/OutputPreview.tsx` — extracted from `ResultStep`.

### React — modified files
- `src/App.tsx` — full rewrite around sidebar + main-pane routing.
- `src/components/InputStep.tsx` — DELETE after its pieces move.
- `src/components/SettingsStep.tsx` — DELETE after its pieces move.
- `src/components/ProcessingStep.tsx` — DELETE (replaced by `StageCard` list).
- `src/components/ResultStep.tsx` — DELETE after `OutputPreview` extraction.

---

# Phase 1 — Storage foundation

## Task 1: Add Rust dependencies

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Add the three new dependencies under `[dependencies]`**

Add these lines to `src-tauri/Cargo.toml` (at the end of `[dependencies]`):

```toml
rusqlite = { version = "0.31", features = ["bundled"] }
sha2 = "0.10"
uuid = { version = "1", features = ["v4"] }
```

- [ ] **Step 2: Verify compile**

Run: `cd src-tauri && cargo check`
Expected: compiles clean (warnings about unused deps are fine; they're used in later tasks).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "deps: add rusqlite, sha2, uuid for storage layer"
```

---

## Task 2: Storage module scaffold + schema

**Files:**
- Create: `src-tauri/src/storage/mod.rs`
- Create: `src-tauri/src/storage/schema.rs`
- Modify: `src-tauri/src/lib.rs` (register module)

- [ ] **Step 1: Create `src-tauri/src/storage/mod.rs`**

```rust
//! Persistent storage for projects, recordings, and stage state.
//!
//! Backed by a single SQLite database at `<app_data>/meister.db`. One shared
//! connection lives in Tauri state; all access is synchronous and must be
//! wrapped in `tokio::task::spawn_blocking` from async commands.

pub mod schema;

use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

pub struct Store {
    pub conn: Mutex<Connection>,
}

impl Store {
    /// Open (or create) the database at `path` and apply migrations.
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create db parent dir: {e}"))?;
        }
        let conn = Connection::open(path).map_err(|e| format!("open db: {e}"))?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|e| format!("enable foreign keys: {e}"))?;
        schema::migrate(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// Open an in-memory database for tests.
    #[cfg(test)]
    pub fn open_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory().map_err(|e| format!("open db: {e}"))?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|e| format!("enable foreign keys: {e}"))?;
        schema::migrate(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }
}
```

- [ ] **Step 2: Create `src-tauri/src/storage/schema.rs`**

```rust
//! Schema + migrations. The database version is tracked via `PRAGMA
//! user_version`. Each migration moves the version forward by one.

use rusqlite::Connection;

const CURRENT_VERSION: i32 = 1;

pub fn migrate(conn: &Connection) -> Result<(), String> {
    let mut version: i32 = conn
        .query_row("PRAGMA user_version;", [], |r| r.get(0))
        .map_err(|e| format!("read user_version: {e}"))?;

    while version < CURRENT_VERSION {
        match version {
            0 => migrate_v0_to_v1(conn)?,
            n => return Err(format!("no migration from version {n}")),
        }
        version += 1;
        conn.execute_batch(&format!("PRAGMA user_version = {version};"))
            .map_err(|e| format!("bump user_version: {e}"))?;
    }
    Ok(())
}

fn migrate_v0_to_v1(conn: &Connection) -> Result<(), String> {
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
        CREATE INDEX idx_recordings_project ON recordings(project_id);

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
        "#,
    )
    .map_err(|e| format!("apply v1 schema: {e}"))
}
```

- [ ] **Step 3: Register module in `src-tauri/src/lib.rs`**

At the top of `src-tauri/src/lib.rs`, below `mod commands;`, add:

```rust
pub mod storage;
```

- [ ] **Step 4: Write a smoke test for migration**

Append to `src-tauri/src/storage/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_memory_applies_schema() {
        let store = Store::open_memory().unwrap();
        let conn = store.conn.lock().unwrap();
        let v: i32 = conn
            .query_row("PRAGMA user_version;", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, 1);
        // Tables exist.
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('projects','recordings','stages');",
                [],
                |r| r.get(0),
            ).unwrap();
        assert_eq!(n, 3);
    }
}
```

- [ ] **Step 5: Run the test**

Run: `cd src-tauri && cargo test --lib storage::tests::open_memory_applies_schema`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/storage/ src-tauri/src/lib.rs
git commit -m "storage: add module scaffold + v1 schema migration"
```

---

## Task 3: Storage types

**Files:**
- Create: `src-tauri/src/storage/types.rs`
- Modify: `src-tauri/src/storage/mod.rs` (expose types)

- [ ] **Step 1: Create `src-tauri/src/storage/types.rs`**

```rust
//! Data types shared by storage, orchestrator, and Tauri commands.
//!
//! `settings` and `default_settings` are stored as JSON strings in SQLite
//! and round-tripped through serde when loaded.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Md,
    Txt,
    Docx,
    Pdf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppSettings {
    #[serde(rename = "ollamaUrl")]
    pub ollama_url: String,
    pub model: String,
    #[serde(rename = "whisperModel")]
    pub whisper_model: String,
    pub language: String,
    #[serde(rename = "outputFormats")]
    pub output_formats: Vec<OutputFormat>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TemplateSource {
    Inline,
    File,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    pub source: TemplateSource,
    pub content: String,
    #[serde(rename = "sourcePath")]
    pub source_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
    pub template: Template,
    #[serde(rename = "defaultSettings")]
    pub default_settings: AppSettings,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum StageKind {
    Extract,
    Transcribe,
    Format,
    Save,
}

impl StageKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            StageKind::Extract => "extract",
            StageKind::Transcribe => "transcribe",
            StageKind::Format => "format",
            StageKind::Save => "save",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "extract" => Some(StageKind::Extract),
            "transcribe" => Some(StageKind::Transcribe),
            "format" => Some(StageKind::Format),
            "save" => Some(StageKind::Save),
            _ => None,
        }
    }

    pub const ALL: [StageKind; 4] = [
        StageKind::Extract,
        StageKind::Transcribe,
        StageKind::Format,
        StageKind::Save,
    ];
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StageStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

impl StageStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            StageStatus::Pending => "pending",
            StageStatus::Running => "running",
            StageStatus::Completed => "completed",
            StageStatus::Failed => "failed",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(StageStatus::Pending),
            "running" => Some(StageStatus::Running),
            "completed" => Some(StageStatus::Completed),
            "failed" => Some(StageStatus::Failed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageRecord {
    pub stage: StageKind,
    pub status: StageStatus,
    pub error: Option<String>,
    #[serde(rename = "completedAt")]
    pub completed_at: Option<i64>,
    #[serde(rename = "modelUsed")]
    pub model_used: Option<String>,
    #[serde(rename = "templateHash")]
    pub template_hash: Option<String>,
    pub output: Option<String>,
    #[serde(rename = "savedPaths")]
    pub saved_paths: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recording {
    pub id: String,
    #[serde(rename = "projectId")]
    pub project_id: String,
    pub name: String,
    #[serde(rename = "sourcePath")]
    pub source_path: String,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
    pub settings: AppSettings,
    pub stages: Vec<StageRecord>,
}
```

- [ ] **Step 2: Re-export from `storage/mod.rs`**

Append to `src-tauri/src/storage/mod.rs` (after the existing `pub mod schema;` line):

```rust
pub mod types;

pub use types::*;
```

- [ ] **Step 3: Verify compile**

Run: `cd src-tauri && cargo check`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/storage/types.rs src-tauri/src/storage/mod.rs
git commit -m "storage: add core types (Project, Recording, Stage*)"
```

---

## Task 4: Projects CRUD

**Files:**
- Create: `src-tauri/src/storage/projects.rs`
- Modify: `src-tauri/src/storage/mod.rs`
- Create: `src-tauri/tests/storage_integration.rs`

- [ ] **Step 1: Write the failing integration test**

Create `src-tauri/tests/storage_integration.rs`:

```rust
//! End-to-end tests for the storage module against an in-memory SQLite DB.

use meister_lib::storage::{
    self, AppSettings, OutputFormat, Project, Store, Template, TemplateSource,
};

fn default_settings() -> AppSettings {
    AppSettings {
        ollama_url: "http://localhost:11434".to_string(),
        model: "gemma3".to_string(),
        whisper_model: "base".to_string(),
        language: "auto".to_string(),
        output_formats: vec![OutputFormat::Md],
    }
}

fn sample_project() -> Project {
    Project {
        id: "p1".to_string(),
        name: "Q2 meetings".to_string(),
        created_at: 1_000,
        updated_at: 1_000,
        template: Template {
            source: TemplateSource::Inline,
            content: "# Minutes\n".to_string(),
            source_path: None,
        },
        default_settings: default_settings(),
    }
}

#[test]
fn projects_crud_roundtrip() {
    let store = Store::open_memory().unwrap();

    // Create
    let mut p = sample_project();
    storage::projects::insert(&store, &p).unwrap();

    // Read by id
    let fetched = storage::projects::get(&store, "p1").unwrap().unwrap();
    assert_eq!(fetched.name, "Q2 meetings");
    assert_eq!(fetched.template.content, "# Minutes\n");

    // List
    let all = storage::projects::list(&store).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].id, "p1");

    // Update
    p.name = "Q2 board meetings".to_string();
    p.updated_at = 2_000;
    storage::projects::update(&store, &p).unwrap();
    let fetched = storage::projects::get(&store, "p1").unwrap().unwrap();
    assert_eq!(fetched.name, "Q2 board meetings");
    assert_eq!(fetched.updated_at, 2_000);

    // Delete
    storage::projects::delete(&store, "p1").unwrap();
    assert!(storage::projects::get(&store, "p1").unwrap().is_none());
    assert_eq!(storage::projects::list(&store).unwrap().len(), 0);
}

#[test]
fn get_missing_project_returns_none() {
    let store = Store::open_memory().unwrap();
    assert!(storage::projects::get(&store, "nope").unwrap().is_none());
}
```

- [ ] **Step 2: Run the test to confirm it fails**

Run: `cd src-tauri && cargo test --test storage_integration`
Expected: FAIL — `projects` module not found.

- [ ] **Step 3: Create `src-tauri/src/storage/projects.rs`**

```rust
//! Project CRUD.

use crate::storage::{Project, Store};
use rusqlite::params;

pub fn insert(store: &Store, p: &Project) -> Result<(), String> {
    let conn = store.conn.lock().unwrap();
    let settings_json = serde_json::to_string(&p.default_settings)
        .map_err(|e| format!("serialize settings: {e}"))?;
    conn.execute(
        "INSERT INTO projects \
         (id, name, created_at, updated_at, template_source, template_content, template_source_path, default_settings) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            p.id, p.name, p.created_at, p.updated_at,
            p.template.source.as_str(),
            p.template.content,
            p.template.source_path,
            settings_json,
        ],
    )
    .map_err(|e| format!("insert project: {e}"))?;
    Ok(())
}

pub fn update(store: &Store, p: &Project) -> Result<(), String> {
    let conn = store.conn.lock().unwrap();
    let settings_json = serde_json::to_string(&p.default_settings)
        .map_err(|e| format!("serialize settings: {e}"))?;
    let n = conn.execute(
        "UPDATE projects SET \
         name = ?2, updated_at = ?3, template_source = ?4, template_content = ?5, \
         template_source_path = ?6, default_settings = ?7 \
         WHERE id = ?1",
        params![
            p.id, p.name, p.updated_at,
            p.template.source.as_str(),
            p.template.content,
            p.template.source_path,
            settings_json,
        ],
    )
    .map_err(|e| format!("update project: {e}"))?;
    if n == 0 {
        return Err(format!("project not found: {}", p.id));
    }
    Ok(())
}

pub fn get(store: &Store, id: &str) -> Result<Option<Project>, String> {
    let conn = store.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, name, created_at, updated_at, template_source, \
             template_content, template_source_path, default_settings \
             FROM projects WHERE id = ?1",
        )
        .map_err(|e| format!("prepare: {e}"))?;
    let mut rows = stmt.query(params![id]).map_err(|e| format!("query: {e}"))?;
    if let Some(row) = rows.next().map_err(|e| format!("next: {e}"))? {
        Ok(Some(row_to_project(row)?))
    } else {
        Ok(None)
    }
}

pub fn list(store: &Store) -> Result<Vec<Project>, String> {
    let conn = store.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, name, created_at, updated_at, template_source, \
             template_content, template_source_path, default_settings \
             FROM projects ORDER BY created_at ASC",
        )
        .map_err(|e| format!("prepare: {e}"))?;
    let rows = stmt
        .query_map([], |row| Ok(row_to_project(row)))
        .map_err(|e| format!("query_map: {e}"))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("row: {e}"))??);
    }
    Ok(out)
}

pub fn delete(store: &Store, id: &str) -> Result<(), String> {
    let conn = store.conn.lock().unwrap();
    conn.execute("DELETE FROM projects WHERE id = ?1", params![id])
        .map_err(|e| format!("delete project: {e}"))?;
    Ok(())
}

fn row_to_project(row: &rusqlite::Row) -> Result<Project, String> {
    use crate::storage::{Template, TemplateSource};
    let source_str: String = row.get(4).map_err(|e| format!("source: {e}"))?;
    let source = match source_str.as_str() {
        "inline" => TemplateSource::Inline,
        "file" => TemplateSource::File,
        other => return Err(format!("unknown template_source: {other}")),
    };
    let settings_json: String = row.get(7).map_err(|e| format!("settings: {e}"))?;
    let default_settings = serde_json::from_str(&settings_json)
        .map_err(|e| format!("parse settings: {e}"))?;
    Ok(Project {
        id: row.get(0).map_err(|e| format!("id: {e}"))?,
        name: row.get(1).map_err(|e| format!("name: {e}"))?,
        created_at: row.get(2).map_err(|e| format!("created_at: {e}"))?,
        updated_at: row.get(3).map_err(|e| format!("updated_at: {e}"))?,
        template: Template {
            source,
            content: row.get(5).map_err(|e| format!("content: {e}"))?,
            source_path: row.get(6).map_err(|e| format!("source_path: {e}"))?,
        },
        default_settings,
    })
}

// TemplateSource string helpers live here so the DB layer owns the enum<->string mapping.
impl crate::storage::TemplateSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            crate::storage::TemplateSource::Inline => "inline",
            crate::storage::TemplateSource::File => "file",
        }
    }
}
```

- [ ] **Step 4: Expose the module**

Edit `src-tauri/src/storage/mod.rs` — add `pub mod projects;` after `pub mod types;`.

- [ ] **Step 5: Run tests**

Run: `cd src-tauri && cargo test --test storage_integration`
Expected: PASS (both `projects_crud_roundtrip` and `get_missing_project_returns_none`).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/storage/projects.rs src-tauri/src/storage/mod.rs src-tauri/tests/storage_integration.rs
git commit -m "storage: add projects CRUD"
```

---

## Task 5: Recordings CRUD (with stage initialization)

**Files:**
- Create: `src-tauri/src/storage/recordings.rs`
- Modify: `src-tauri/src/storage/mod.rs`
- Modify: `src-tauri/tests/storage_integration.rs`

- [ ] **Step 1: Append failing test to `src-tauri/tests/storage_integration.rs`**

```rust
use meister_lib::storage::{Recording, StageKind, StageStatus};

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

#[test]
fn recordings_insert_initializes_pending_stages() {
    let store = Store::open_memory().unwrap();
    storage::projects::insert(&store, &sample_project()).unwrap();

    let r = sample_recording("p1");
    storage::recordings::insert(&store, &r).unwrap();

    let loaded = storage::recordings::get(&store, "r1").unwrap().unwrap();
    assert_eq!(loaded.stages.len(), 4);
    for (i, kind) in StageKind::ALL.iter().enumerate() {
        assert_eq!(loaded.stages[i].stage, *kind);
        assert_eq!(loaded.stages[i].status, StageStatus::Pending);
    }
}

#[test]
fn recordings_list_by_project_scoped() {
    let store = Store::open_memory().unwrap();
    storage::projects::insert(&store, &sample_project()).unwrap();
    storage::recordings::insert(&store, &sample_recording("p1")).unwrap();

    let mut other = sample_project();
    other.id = "p2".to_string();
    other.name = "Other".to_string();
    storage::projects::insert(&store, &other).unwrap();

    let mut r2 = sample_recording("p2");
    r2.id = "r2".to_string();
    storage::recordings::insert(&store, &r2).unwrap();

    let in_p1 = storage::recordings::list_by_project(&store, "p1").unwrap();
    assert_eq!(in_p1.len(), 1);
    assert_eq!(in_p1[0].id, "r1");
}

#[test]
fn delete_project_cascades_to_recordings_and_stages() {
    let store = Store::open_memory().unwrap();
    storage::projects::insert(&store, &sample_project()).unwrap();
    storage::recordings::insert(&store, &sample_recording("p1")).unwrap();

    storage::projects::delete(&store, "p1").unwrap();

    // Recording gone.
    assert!(storage::recordings::get(&store, "r1").unwrap().is_none());
    // Stage rows gone too.
    let conn = store.conn.lock().unwrap();
    let n: i64 = conn
        .query_row("SELECT count(*) FROM stages", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);
}
```

- [ ] **Step 2: Run to confirm failure**

Run: `cd src-tauri && cargo test --test storage_integration`
Expected: FAIL — `recordings` module not found.

- [ ] **Step 3: Create `src-tauri/src/storage/recordings.rs`**

```rust
//! Recording CRUD. On insert, four `pending` stage rows are created so every
//! recording always has a complete 4-row stage set.

use crate::storage::{Recording, StageKind, StageRecord, StageStatus, Store};
use rusqlite::params;

pub fn insert(store: &Store, r: &Recording) -> Result<(), String> {
    let conn = store.conn.lock().unwrap();
    let settings_json = serde_json::to_string(&r.settings)
        .map_err(|e| format!("serialize settings: {e}"))?;
    conn.execute(
        "INSERT INTO recordings \
         (id, project_id, name, source_path, created_at, updated_at, settings) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            r.id, r.project_id, r.name, r.source_path,
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

pub fn update(store: &Store, r: &Recording) -> Result<(), String> {
    let conn = store.conn.lock().unwrap();
    let settings_json = serde_json::to_string(&r.settings)
        .map_err(|e| format!("serialize settings: {e}"))?;
    let n = conn.execute(
        "UPDATE recordings SET \
         name = ?2, source_path = ?3, updated_at = ?4, settings = ?5 \
         WHERE id = ?1",
        params![r.id, r.name, r.source_path, r.updated_at, settings_json],
    )
    .map_err(|e| format!("update recording: {e}"))?;
    if n == 0 {
        return Err(format!("recording not found: {}", r.id));
    }
    Ok(())
}

pub fn get(store: &Store, id: &str) -> Result<Option<Recording>, String> {
    let conn = store.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, name, source_path, created_at, updated_at, settings \
             FROM recordings WHERE id = ?1",
        )
        .map_err(|e| format!("prepare: {e}"))?;
    let mut rows = stmt.query(params![id]).map_err(|e| format!("query: {e}"))?;
    let Some(row) = rows.next().map_err(|e| format!("next: {e}"))? else {
        return Ok(None);
    };
    let mut rec = row_to_recording_without_stages(row)?;
    drop(rows);
    drop(stmt);
    rec.stages = load_stages(&conn, id)?;
    Ok(Some(rec))
}

pub fn list_by_project(store: &Store, project_id: &str) -> Result<Vec<Recording>, String> {
    let conn = store.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, name, source_path, created_at, updated_at, settings \
             FROM recordings WHERE project_id = ?1 ORDER BY created_at ASC",
        )
        .map_err(|e| format!("prepare: {e}"))?;
    let rows = stmt
        .query_map(params![project_id], |row| Ok(row_to_recording_without_stages(row)))
        .map_err(|e| format!("query_map: {e}"))?;
    let mut out = Vec::new();
    for r in rows {
        let mut rec = r.map_err(|e| format!("row: {e}"))??;
        rec.stages = load_stages(&conn, &rec.id)?;
        out.push(rec);
    }
    Ok(out)
}

pub fn delete(store: &Store, id: &str) -> Result<(), String> {
    let conn = store.conn.lock().unwrap();
    conn.execute("DELETE FROM recordings WHERE id = ?1", params![id])
        .map_err(|e| format!("delete recording: {e}"))?;
    Ok(())
}

fn row_to_recording_without_stages(row: &rusqlite::Row) -> Result<Recording, String> {
    let settings_json: String = row.get(6).map_err(|e| format!("settings: {e}"))?;
    let settings = serde_json::from_str(&settings_json)
        .map_err(|e| format!("parse settings: {e}"))?;
    Ok(Recording {
        id: row.get(0).map_err(|e| format!("id: {e}"))?,
        project_id: row.get(1).map_err(|e| format!("project_id: {e}"))?,
        name: row.get(2).map_err(|e| format!("name: {e}"))?,
        source_path: row.get(3).map_err(|e| format!("source_path: {e}"))?,
        created_at: row.get(4).map_err(|e| format!("created_at: {e}"))?,
        updated_at: row.get(5).map_err(|e| format!("updated_at: {e}"))?,
        settings,
        stages: vec![],
    })
}

fn load_stages(
    conn: &rusqlite::Connection,
    recording_id: &str,
) -> Result<Vec<StageRecord>, String> {
    // Always return stages in canonical order.
    let mut stmt = conn
        .prepare(
            "SELECT stage, status, error, completed_at, model_used, template_hash, output, saved_paths \
             FROM stages WHERE recording_id = ?1",
        )
        .map_err(|e| format!("prepare stages: {e}"))?;
    let rows = stmt
        .query_map(params![recording_id], |row| {
            Ok(row_to_stage(row))
        })
        .map_err(|e| format!("query_map stages: {e}"))?;
    let mut by_kind: std::collections::HashMap<StageKind, StageRecord> =
        std::collections::HashMap::new();
    for r in rows {
        let rec = r.map_err(|e| format!("row stage: {e}"))??;
        by_kind.insert(rec.stage, rec);
    }
    let mut out = Vec::with_capacity(4);
    for kind in StageKind::ALL {
        if let Some(rec) = by_kind.remove(&kind) {
            out.push(rec);
        } else {
            out.push(StageRecord {
                stage: kind,
                status: StageStatus::Pending,
                error: None,
                completed_at: None,
                model_used: None,
                template_hash: None,
                output: None,
                saved_paths: None,
            });
        }
    }
    Ok(out)
}

fn row_to_stage(row: &rusqlite::Row) -> Result<StageRecord, String> {
    let stage_str: String = row.get(0).map_err(|e| format!("stage: {e}"))?;
    let status_str: String = row.get(1).map_err(|e| format!("status: {e}"))?;
    let saved_paths_json: Option<String> =
        row.get(7).map_err(|e| format!("saved_paths: {e}"))?;
    let saved_paths = match saved_paths_json {
        None => None,
        Some(s) => Some(
            serde_json::from_str::<Vec<String>>(&s)
                .map_err(|e| format!("parse saved_paths: {e}"))?,
        ),
    };
    Ok(StageRecord {
        stage: StageKind::from_str(&stage_str)
            .ok_or_else(|| format!("unknown stage: {stage_str}"))?,
        status: StageStatus::from_str(&status_str)
            .ok_or_else(|| format!("unknown status: {status_str}"))?,
        error: row.get(2).map_err(|e| format!("error: {e}"))?,
        completed_at: row.get(3).map_err(|e| format!("completed_at: {e}"))?,
        model_used: row.get(4).map_err(|e| format!("model_used: {e}"))?,
        template_hash: row.get(5).map_err(|e| format!("template_hash: {e}"))?,
        output: row.get(6).map_err(|e| format!("output: {e}"))?,
        saved_paths,
    })
}
```

- [ ] **Step 4: Expose the module**

Edit `src-tauri/src/storage/mod.rs` — add `pub mod recordings;`.

- [ ] **Step 5: Run tests**

Run: `cd src-tauri && cargo test --test storage_integration`
Expected: all PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/storage/recordings.rs src-tauri/src/storage/mod.rs src-tauri/tests/storage_integration.rs
git commit -m "storage: add recordings CRUD with auto-initialized stages"
```

---

## Task 6: Stages helpers

**Files:**
- Create: `src-tauri/src/storage/stages.rs`
- Modify: `src-tauri/src/storage/mod.rs`
- Modify: `src-tauri/tests/storage_integration.rs`

- [ ] **Step 1: Append failing tests**

Append to `src-tauri/tests/storage_integration.rs`:

```rust
#[test]
fn stage_update_roundtrip() {
    let store = Store::open_memory().unwrap();
    storage::projects::insert(&store, &sample_project()).unwrap();
    storage::recordings::insert(&store, &sample_recording("p1")).unwrap();

    storage::stages::set_running(&store, "r1", StageKind::Extract).unwrap();
    let rec = storage::recordings::get(&store, "r1").unwrap().unwrap();
    assert_eq!(rec.stages[0].status, StageStatus::Running);

    storage::stages::set_completed(
        &store,
        "r1",
        StageKind::Transcribe,
        storage::stages::CompletedOutput {
            output: Some("hello".to_string()),
            model_used: Some("base".to_string()),
            template_hash: None,
            saved_paths: None,
            completed_at: 9_999,
        },
    )
    .unwrap();
    let rec = storage::recordings::get(&store, "r1").unwrap().unwrap();
    assert_eq!(rec.stages[1].status, StageStatus::Completed);
    assert_eq!(rec.stages[1].output.as_deref(), Some("hello"));
    assert_eq!(rec.stages[1].model_used.as_deref(), Some("base"));
    assert_eq!(rec.stages[1].completed_at, Some(9_999));

    storage::stages::set_failed(&store, "r1", StageKind::Format, "ollama down").unwrap();
    let rec = storage::recordings::get(&store, "r1").unwrap().unwrap();
    assert_eq!(rec.stages[2].status, StageStatus::Failed);
    assert_eq!(rec.stages[2].error.as_deref(), Some("ollama down"));
}

#[test]
fn reset_downstream_clears_later_stages() {
    let store = Store::open_memory().unwrap();
    storage::projects::insert(&store, &sample_project()).unwrap();
    storage::recordings::insert(&store, &sample_recording("p1")).unwrap();

    // Pretend all four succeeded.
    for kind in StageKind::ALL {
        storage::stages::set_completed(
            &store, "r1", kind,
            storage::stages::CompletedOutput {
                output: Some("x".to_string()),
                model_used: Some("m".to_string()),
                template_hash: None,
                saved_paths: None,
                completed_at: 1,
            },
        ).unwrap();
    }

    // Reset everything downstream of transcribe (so format + save go back to pending).
    storage::stages::reset_downstream(&store, "r1", StageKind::Transcribe).unwrap();
    let rec = storage::recordings::get(&store, "r1").unwrap().unwrap();
    assert_eq!(rec.stages[0].status, StageStatus::Completed);       // extract
    assert_eq!(rec.stages[1].status, StageStatus::Completed);       // transcribe kept
    assert_eq!(rec.stages[2].status, StageStatus::Pending);         // format reset
    assert_eq!(rec.stages[2].output, None);
    assert_eq!(rec.stages[3].status, StageStatus::Pending);         // save reset
}

#[test]
fn mark_running_as_failed_sweeps_interrupted_rows() {
    let store = Store::open_memory().unwrap();
    storage::projects::insert(&store, &sample_project()).unwrap();
    storage::recordings::insert(&store, &sample_recording("p1")).unwrap();

    storage::stages::set_running(&store, "r1", StageKind::Transcribe).unwrap();
    let n = storage::stages::mark_running_as_failed(&store, "crashed").unwrap();
    assert_eq!(n, 1);

    let rec = storage::recordings::get(&store, "r1").unwrap().unwrap();
    assert_eq!(rec.stages[1].status, StageStatus::Failed);
    assert_eq!(rec.stages[1].error.as_deref(), Some("crashed"));
}
```

- [ ] **Step 2: Run to confirm failure**

Run: `cd src-tauri && cargo test --test storage_integration`
Expected: FAIL — `stages` module not found.

- [ ] **Step 3: Create `src-tauri/src/storage/stages.rs`**

```rust
//! Stage-level updates. Each function is a single UPDATE, so every row
//! transition is atomic. `reset_downstream` walks the canonical stage order
//! and wipes every stage strictly after the pivot.

use crate::storage::{StageKind, StageStatus, Store};
use rusqlite::params;

/// Everything you can set when a stage completes. Per-stage only a subset
/// is meaningful (see `inputs_unchanged` and `run_recording`), but storing
/// them all here keeps callers simple and the schema narrow.
pub struct CompletedOutput {
    pub output: Option<String>,
    pub model_used: Option<String>,
    pub template_hash: Option<String>,
    pub saved_paths: Option<Vec<String>>,
    pub completed_at: i64,
}

pub fn set_running(store: &Store, recording_id: &str, stage: StageKind) -> Result<(), String> {
    let conn = store.conn.lock().unwrap();
    conn.execute(
        "UPDATE stages SET status = ?3, error = NULL WHERE recording_id = ?1 AND stage = ?2",
        params![recording_id, stage.as_str(), StageStatus::Running.as_str()],
    )
    .map_err(|e| format!("set_running: {e}"))?;
    Ok(())
}

pub fn set_completed(
    store: &Store,
    recording_id: &str,
    stage: StageKind,
    out: CompletedOutput,
) -> Result<(), String> {
    let saved_paths_json = match &out.saved_paths {
        Some(v) => Some(
            serde_json::to_string(v).map_err(|e| format!("serialize saved_paths: {e}"))?,
        ),
        None => None,
    };
    let conn = store.conn.lock().unwrap();
    conn.execute(
        "UPDATE stages SET \
         status = ?3, error = NULL, completed_at = ?4, model_used = ?5, \
         template_hash = ?6, output = ?7, saved_paths = ?8 \
         WHERE recording_id = ?1 AND stage = ?2",
        params![
            recording_id,
            stage.as_str(),
            StageStatus::Completed.as_str(),
            out.completed_at,
            out.model_used,
            out.template_hash,
            out.output,
            saved_paths_json,
        ],
    )
    .map_err(|e| format!("set_completed: {e}"))?;
    Ok(())
}

pub fn set_failed(
    store: &Store,
    recording_id: &str,
    stage: StageKind,
    error: &str,
) -> Result<(), String> {
    let conn = store.conn.lock().unwrap();
    conn.execute(
        "UPDATE stages SET status = ?3, error = ?4 WHERE recording_id = ?1 AND stage = ?2",
        params![
            recording_id,
            stage.as_str(),
            StageStatus::Failed.as_str(),
            error,
        ],
    )
    .map_err(|e| format!("set_failed: {e}"))?;
    Ok(())
}

/// Reset every stage strictly after `pivot` back to `pending`, clearing its
/// output/error fields. `pivot` itself is untouched.
pub fn reset_downstream(
    store: &Store,
    recording_id: &str,
    pivot: StageKind,
) -> Result<(), String> {
    let pivot_idx = StageKind::ALL.iter().position(|k| *k == pivot).unwrap();
    let later: Vec<&'static str> = StageKind::ALL[pivot_idx + 1..]
        .iter()
        .map(|k| k.as_str())
        .collect();
    if later.is_empty() {
        return Ok(());
    }
    let placeholders: Vec<String> = (0..later.len()).map(|i| format!("?{}", i + 2)).collect();
    let sql = format!(
        "UPDATE stages SET \
         status = 'pending', error = NULL, completed_at = NULL, model_used = NULL, \
         template_hash = NULL, output = NULL, saved_paths = NULL \
         WHERE recording_id = ?1 AND stage IN ({})",
        placeholders.join(",")
    );
    let conn = store.conn.lock().unwrap();
    let mut stmt = conn.prepare(&sql).map_err(|e| format!("prepare: {e}"))?;
    let mut p: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(later.len() + 1);
    p.push(&recording_id);
    for s in &later {
        p.push(s);
    }
    stmt.execute(rusqlite::params_from_iter(p.iter()))
        .map_err(|e| format!("reset_downstream: {e}"))?;
    Ok(())
}

/// Reset every stage including and after `pivot` back to pending. Used by
/// "Start over" (call with `StageKind::Extract`).
pub fn reset_from(store: &Store, recording_id: &str, pivot: StageKind) -> Result<(), String> {
    let pivot_idx = StageKind::ALL.iter().position(|k| *k == pivot).unwrap();
    let kinds: Vec<&'static str> = StageKind::ALL[pivot_idx..]
        .iter()
        .map(|k| k.as_str())
        .collect();
    let placeholders: Vec<String> = (0..kinds.len()).map(|i| format!("?{}", i + 2)).collect();
    let sql = format!(
        "UPDATE stages SET \
         status = 'pending', error = NULL, completed_at = NULL, model_used = NULL, \
         template_hash = NULL, output = NULL, saved_paths = NULL \
         WHERE recording_id = ?1 AND stage IN ({})",
        placeholders.join(",")
    );
    let conn = store.conn.lock().unwrap();
    let mut stmt = conn.prepare(&sql).map_err(|e| format!("prepare: {e}"))?;
    let mut p: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(kinds.len() + 1);
    p.push(&recording_id);
    for s in &kinds {
        p.push(s);
    }
    stmt.execute(rusqlite::params_from_iter(p.iter()))
        .map_err(|e| format!("reset_from: {e}"))?;
    Ok(())
}

/// Flip every `running` stage anywhere in the DB to `failed` with the given
/// message. Called at app startup to clean up state left behind by a crash.
/// Returns the number of rows swept.
pub fn mark_running_as_failed(store: &Store, error: &str) -> Result<usize, String> {
    let conn = store.conn.lock().unwrap();
    let n = conn
        .execute(
            "UPDATE stages SET status = 'failed', error = ?1 WHERE status = 'running'",
            params![error],
        )
        .map_err(|e| format!("mark_running_as_failed: {e}"))?;
    Ok(n)
}
```

- [ ] **Step 4: Expose the module**

Edit `src-tauri/src/storage/mod.rs` — add `pub mod stages;`.

- [ ] **Step 5: Run tests**

Run: `cd src-tauri && cargo test --test storage_integration`
Expected: all PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/storage/stages.rs src-tauri/src/storage/mod.rs src-tauri/tests/storage_integration.rs
git commit -m "storage: add stage update helpers + reset + crash recovery sweep"
```

---

## Task 7: Template hash helper

**Files:**
- Create: `src-tauri/src/storage/hash.rs`
- Modify: `src-tauri/src/storage/mod.rs`

- [ ] **Step 1: Create `src-tauri/src/storage/hash.rs`**

```rust
//! Deterministic content hash for templates, used to detect "template edited
//! since last format" on a recording.

use sha2::{Digest, Sha256};

pub fn template_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_input_same_hash() {
        assert_eq!(template_hash("hi"), template_hash("hi"));
    }

    #[test]
    fn different_input_different_hash() {
        assert_ne!(template_hash("hi"), template_hash("bye"));
    }

    #[test]
    fn known_value() {
        assert_eq!(
            template_hash(""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
```

- [ ] **Step 2: Expose module**

Edit `src-tauri/src/storage/mod.rs` — add `pub mod hash;` and `pub use hash::template_hash;`.

- [ ] **Step 3: Run tests**

Run: `cd src-tauri && cargo test --lib storage::hash::`
Expected: all three PASS.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/storage/hash.rs src-tauri/src/storage/mod.rs
git commit -m "storage: add template_hash helper (sha256)"
```

---

# Phase 2 — Orchestration

## Task 8: `inputs_unchanged` logic (pure function, testable)

**Files:**
- Create: `src-tauri/src/orchestrator.rs`
- Modify: `src-tauri/src/lib.rs` (register module)

- [ ] **Step 1: Create `src-tauri/src/orchestrator.rs` with `inputs_unchanged` and unit tests**

```rust
//! Pipeline orchestration: `inputs_unchanged` decides whether a completed
//! stage's cached output is still valid given the current recording settings
//! and project template. `run_recording` is added in a later task.

use crate::storage::{AppSettings, StageKind, StageRecord, StageStatus};

/// Returns `true` if the stage's cached inputs still match the current
/// recording settings + template hash, meaning its cached output can be
/// reused. Assumes `stage.status == Completed`; caller checks that first.
pub fn inputs_unchanged(
    stage_kind: StageKind,
    stage: &StageRecord,
    current_settings: &AppSettings,
    current_template_hash: &str,
    current_source_path: &str,
    extract_artifact_exists: bool,
) -> bool {
    if stage.status != StageStatus::Completed {
        return false;
    }
    match stage_kind {
        StageKind::Extract => {
            // source_path is immutable for a recording, so extract is never
            // invalidated by content changes. But if someone deleted the
            // audio.wav artifact *and* the WAV is needed downstream (i.e.
            // transcribe isn't completed), we can't reuse. For the common
            // case of "everything already completed", we don't need the wav
            // anymore, so accept.
            let _ = extract_artifact_exists; // currently unused; hook for future
            let _ = current_source_path;
            true
        }
        StageKind::Transcribe => {
            stage.model_used.as_deref() == Some(current_settings.whisper_model.as_str())
                && matches_language(stage, &current_settings.language)
        }
        StageKind::Format => {
            stage.model_used.as_deref() == Some(current_settings.model.as_str())
                && stage.template_hash.as_deref() == Some(current_template_hash)
        }
        StageKind::Save => {
            // Rerun save if the requested format set changed or any saved
            // file is missing on disk.
            let Some(saved) = &stage.saved_paths else { return false; };
            let formats_match = saved
                .iter()
                .all(|p| saved_path_matches_any_format(p, &current_settings.output_formats))
                && current_settings.output_formats.iter().all(|fmt| {
                    saved.iter().any(|p| path_has_extension(p, fmt_to_ext(fmt)))
                });
            if !formats_match {
                return false;
            }
            saved.iter().all(|p| std::path::Path::new(p).is_file())
        }
    }
}

fn matches_language(stage: &StageRecord, current: &str) -> bool {
    // The transcribe stage stores the model under model_used; language is
    // mixed into the "inputs that matter". We carry it on `output`'s
    // companion — but the current schema only has model_used. For MVP:
    // treat language mismatch as invalidation by overloading model_used?
    // Simpler: store language-suffixed model_used. Instead, use a small
    // convention: model_used string format "<model>@<language>".
    let want = format!("{}@{}", stage.model_used.clone().unwrap_or_default(), current);
    match stage.model_used.as_deref() {
        Some(s) if s.contains('@') => s == want,
        // Back-compat: if no "@" suffix, treat as matching any language.
        Some(_) => true,
        None => false,
    }
}

fn fmt_to_ext(fmt: &crate::storage::OutputFormat) -> &'static str {
    use crate::storage::OutputFormat::*;
    match fmt {
        Md => "md",
        Txt => "txt",
        Docx => "docx",
        Pdf => "pdf",
    }
}

fn path_has_extension(path: &str, ext: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.eq_ignore_ascii_case(ext))
        .unwrap_or(false)
}

fn saved_path_matches_any_format(
    path: &str,
    formats: &[crate::storage::OutputFormat],
) -> bool {
    formats.iter().any(|f| path_has_extension(path, fmt_to_ext(f)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{AppSettings, OutputFormat, StageKind, StageRecord, StageStatus};

    fn settings() -> AppSettings {
        AppSettings {
            ollama_url: "u".to_string(),
            model: "gemma3".to_string(),
            whisper_model: "base".to_string(),
            language: "en".to_string(),
            output_formats: vec![OutputFormat::Md],
        }
    }

    fn completed_stage(kind: StageKind) -> StageRecord {
        StageRecord {
            stage: kind,
            status: StageStatus::Completed,
            error: None,
            completed_at: Some(1),
            model_used: None,
            template_hash: None,
            output: None,
            saved_paths: None,
        }
    }

    #[test]
    fn extract_always_valid_if_completed() {
        let s = completed_stage(StageKind::Extract);
        assert!(inputs_unchanged(StageKind::Extract, &s, &settings(), "h", "/x", true));
    }

    #[test]
    fn transcribe_invalid_if_model_changed() {
        let mut s = completed_stage(StageKind::Transcribe);
        s.model_used = Some("base@en".to_string());
        let mut cur = settings();
        cur.whisper_model = "small".to_string();
        assert!(!inputs_unchanged(StageKind::Transcribe, &s, &cur, "h", "/x", true));
    }

    #[test]
    fn transcribe_valid_if_model_matches() {
        let mut s = completed_stage(StageKind::Transcribe);
        s.model_used = Some("base@en".to_string());
        assert!(inputs_unchanged(StageKind::Transcribe, &s, &settings(), "h", "/x", true));
    }

    #[test]
    fn format_invalid_on_template_change() {
        let mut s = completed_stage(StageKind::Format);
        s.model_used = Some("gemma3".to_string());
        s.template_hash = Some("old".to_string());
        assert!(!inputs_unchanged(StageKind::Format, &s, &settings(), "new", "/x", true));
    }

    #[test]
    fn format_invalid_on_model_change() {
        let mut s = completed_stage(StageKind::Format);
        s.model_used = Some("llama3".to_string());
        s.template_hash = Some("h".to_string());
        assert!(!inputs_unchanged(StageKind::Format, &s, &settings(), "h", "/x", true));
    }

    #[test]
    fn save_invalid_if_paths_missing() {
        let mut s = completed_stage(StageKind::Save);
        s.saved_paths = Some(vec!["/definitely/not/here.md".to_string()]);
        assert!(!inputs_unchanged(StageKind::Save, &s, &settings(), "h", "/x", true));
    }

    #[test]
    fn save_invalid_if_output_formats_changed() {
        let mut s = completed_stage(StageKind::Save);
        // Pretend the file exists — build a temp file.
        let dir = std::env::temp_dir();
        let f = dir.join("meister_test.md");
        std::fs::write(&f, "x").unwrap();
        s.saved_paths = Some(vec![f.to_string_lossy().to_string()]);

        let mut cur = settings();
        cur.output_formats = vec![OutputFormat::Md, OutputFormat::Txt];
        assert!(!inputs_unchanged(StageKind::Save, &s, &cur, "h", "/x", true));

        let _ = std::fs::remove_file(&f);
    }
}
```

- [ ] **Step 2: Register the module in `src-tauri/src/lib.rs`**

Add below `pub mod storage;`:

```rust
pub mod orchestrator;
```

- [ ] **Step 3: Run tests**

Run: `cd src-tauri && cargo test --lib orchestrator::tests`
Expected: all PASS.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/orchestrator.rs src-tauri/src/lib.rs
git commit -m "orchestrator: add inputs_unchanged with per-stage staleness rules"
```

---

## Task 9: Initialize storage + register Store in Tauri state + crash recovery

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Wire the Store into `setup()`**

In `src-tauri/src/lib.rs`, replace the body of `.setup(|app| { ... })` with the version below. Keep the existing transcription setup; add storage init + crash recovery:

```rust
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;

            let resource_dir = app.path().resource_dir()?;
            let bundled_dir = if resource_dir.join("models").join("ggml-base.bin").is_file() {
                resource_dir
            } else {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources")
            };

            std::env::set_var("GGML_METAL_PATH_RESOURCES", &bundled_dir);

            transcription::ensure_bundled_base_copied(&bundled_dir, &app_data_dir)
                .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

            // Storage: open DB, sweep interrupted `running` stage rows.
            let db_path = app_data_dir.join("meister.db");
            let store = storage::Store::open(&db_path)
                .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
            let swept = storage::stages::mark_running_as_failed(
                &store, "Interrupted (app closed during processing)",
            ).map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
            if swept > 0 {
                eprintln!("storage: marked {swept} interrupted stage(s) as failed");
            }
            app.manage(std::sync::Arc::new(store));

            Ok(())
        })
```

- [ ] **Step 2: Verify compile**

Run: `cd src-tauri && cargo check`
Expected: clean.

- [ ] **Step 3: Manual smoke test**

Run: `npm run tauri dev`
Expected: app launches without error. Check console for any "storage:" messages (none expected on first run).
Close the app.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "lib: init storage at startup + sweep interrupted stage rows"
```

---

## Task 10: Refactor pipeline stages into internal functions

**Files:**
- Modify: `src-tauri/src/commands.rs`

This task makes the four stage bodies callable from the orchestrator without going through Tauri's `invoke_handler`. The existing Tauri commands stay intact as thin wrappers so the current UI keeps working until the orchestrator replaces it.

- [ ] **Step 1: Extract `extract_audio` body into `fn run_extract`**

In `src-tauri/src/commands.rs`, replace the existing `extract_audio` function with:

```rust
/// Extract audio to a caller-chosen output path. Returns the path on success.
pub async fn run_extract(
    input_path: &str,
    output_wav: &std::path::Path,
) -> Result<(), String> {
    if let Some(parent) = output_wav.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create output dir: {e}"))?;
    }
    let output = Command::new("ffmpeg")
        .args([
            "-i", input_path,
            "-vn",
            "-acodec", "pcm_s16le",
            "-ar", "16000",
            "-ac", "1",
            "-y",
            output_wav.to_str().unwrap(),
        ])
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

/// Legacy Tauri command — kept so the current UI keeps working during the
/// transition. Writes to a predictable temp path.
#[tauri::command]
pub async fn extract_audio(
    _app: AppHandle,
    input_path: String,
) -> Result<String, String> {
    let temp_wav = std::env::temp_dir().join("meister_audio.wav");
    run_extract(&input_path, &temp_wav).await?;
    Ok(temp_wav.to_string_lossy().to_string())
}
```

- [ ] **Step 2: Extract `transcribe_audio` body into `fn run_transcribe`**

In the same file, insert after `run_extract`:

```rust
/// Run whisper on an already-extracted WAV. Returns the transcript text.
pub async fn run_transcribe(
    app: &AppHandle,
    wav_path: &std::path::Path,
    whisper_model: &str,
    language: &str,
) -> Result<String, String> {
    let size = transcription::ModelSize::from_str(whisper_model)
        .ok_or_else(|| format!("unknown model size: {whisper_model}"))?;
    let models_dir = app.path()
        .app_data_dir()
        .map_err(|e| format!("resolve app_data_dir: {e}"))?
        .join("models");

    if transcription::is_warming(size) {
        let _ = app.emit("model-warming", ModelWarming { size: whisper_model.to_string() });
    }

    let app_cb = app.clone();
    let wav = wav_path.to_path_buf();
    let lang = if language.is_empty() || language == "auto" {
        None
    } else {
        Some(language.to_string())
    };

    let result = tokio::task::spawn_blocking(move || {
        transcription::transcribe_wav_with_callback(
            &models_dir,
            size,
            &wav,
            lang.as_deref(),
            move |pct| {
                let _ = app_cb.emit("transcribe-progress", TranscribeProgress {
                    percent: pct.clamp(0, 100) as u32,
                    message: format!("Transcribing… {pct}%"),
                });
            },
        )
    })
    .await
    .map_err(|e| format!("task join: {e}"))?;

    let _ = app.emit("transcribe-progress", TranscribeProgress {
        percent: 100,
        message: "Transcription complete.".to_string(),
    });
    result
}
```

And replace the existing `#[tauri::command] transcribe_audio` with:

```rust
#[tauri::command]
pub async fn transcribe_audio(
    app: AppHandle,
    wav_path: String,
    whisper_model: String,
    language: String,
) -> Result<String, String> {
    run_transcribe(&app, std::path::Path::new(&wav_path), &whisper_model, &language).await
}
```

- [ ] **Step 3: Extract `format_transcript` body into `fn run_format`**

Insert after `run_transcribe`:

```rust
/// Call Ollama to format a transcript against a template. Emits
/// `format-progress` events while streaming.
pub async fn run_format(
    app: &AppHandle,
    transcript: &str,
    template_content: &str,
    ollama_url: &str,
    model: &str,
) -> Result<String, String> {
    use futures_util::StreamExt;

    let prompt = build_prompt(transcript, template_content);
    let client = reqwest::Client::new();
    let body = OllamaGenerateRequest {
        model: model.to_string(),
        prompt,
        stream: true,
    };
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
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| format!("Ollama stream error: {e}"))?;
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
                let _ = app.emit("format-progress", FormatProgress {
                    chars: accumulated.chars().count(),
                    done: false,
                });
            }
            if obj.get("done").and_then(|v| v.as_bool()).unwrap_or(false) {
                let _ = app.emit("format-progress", FormatProgress {
                    chars: accumulated.chars().count(),
                    done: true,
                });
            }
        }
    }
    Ok(accumulated)
}
```

Replace `#[tauri::command] format_transcript`:

```rust
#[tauri::command]
pub async fn format_transcript(
    app: AppHandle,
    transcript: String,
    template_content: String,
    ollama_url: String,
    model: String,
) -> Result<String, String> {
    run_format(&app, &transcript, &template_content, &ollama_url, &model).await
}
```

- [ ] **Step 4: Verify compile**

Run: `cd src-tauri && cargo check`
Expected: clean.

- [ ] **Step 5: Smoke test — run existing pipeline end-to-end**

Run: `npm run tauri dev`, drop a short recording, configure, hit Run, confirm the full pipeline still works (pre-change behavior).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "commands: extract run_extract/run_transcribe/run_format helpers"
```

---

## Task 11: `run_recording` orchestrator command

**Files:**
- Modify: `src-tauri/src/orchestrator.rs`
- Modify: `src-tauri/src/lib.rs` (register command)

- [ ] **Step 1: Append `run_recording` to `src-tauri/src/orchestrator.rs`**

Add to the existing file (keep `inputs_unchanged` and its tests; add imports and the new function at the bottom, before the `#[cfg(test)]` block):

```rust
use std::sync::Arc;
use tauri::AppHandle;

use crate::commands::{run_extract, run_format, run_transcribe};
use crate::storage::{self, Recording, StageKind, Store};

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn recording_dir(app: &AppHandle, recording_id: &str) -> Result<std::path::PathBuf, String> {
    use tauri::Manager;
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app_data_dir: {e}"))?
        .join("recordings")
        .join(recording_id))
}

/// Run the pipeline for a recording, skipping any stage whose cached output is
/// still valid. Idempotent — this is how both "Run" and "Resume" work.
#[tauri::command]
pub async fn run_recording(
    app: AppHandle,
    store: tauri::State<'_, Arc<Store>>,
    recording_id: String,
) -> Result<(), String> {
    let store: Arc<Store> = (*store).clone();

    // Load recording + project.
    let rec: Recording = storage::recordings::get(&store, &recording_id)
        .map_err(|e| format!("load recording: {e}"))?
        .ok_or_else(|| format!("recording not found: {recording_id}"))?;
    let project = storage::projects::get(&store, &rec.project_id)
        .map_err(|e| format!("load project: {e}"))?
        .ok_or_else(|| format!("project not found: {}", rec.project_id))?;

    let template_hash = storage::template_hash(&project.template.content);
    let wav_path = recording_dir(&app, &recording_id)?.join("audio.wav");
    let wav_exists = wav_path.is_file();

    // Walk stages; on invalidation, reset downstream before starting.
    // We reload the recording's stages after each transition to avoid stale in-memory copies.
    for stage_kind in StageKind::ALL {
        let rec = storage::recordings::get(&store, &recording_id)
            .map_err(|e| format!("reload recording: {e}"))?
            .ok_or_else(|| "recording vanished".to_string())?;
        let stage = rec
            .stages
            .iter()
            .find(|s| s.stage == stage_kind)
            .ok_or_else(|| format!("stage row missing: {:?}", stage_kind))?;

        let reusable = matches!(stage.status, storage::StageStatus::Completed)
            && inputs_unchanged(
                stage_kind,
                stage,
                &rec.settings,
                &template_hash,
                &rec.source_path,
                wav_exists,
            );

        if reusable {
            continue;
        }

        // Invalidation: clear this stage + everything downstream before running.
        storage::stages::reset_from(&store, &recording_id, stage_kind)
            .map_err(|e| format!("reset_from: {e}"))?;

        storage::stages::set_running(&store, &recording_id, stage_kind)
            .map_err(|e| format!("set_running: {e}"))?;

        let result = match stage_kind {
            StageKind::Extract => {
                run_extract(&rec.source_path, &wav_path).await
                    .map(|_| storage::stages::CompletedOutput {
                        output: None,
                        model_used: None,
                        template_hash: None,
                        saved_paths: None,
                        completed_at: now_ms(),
                    })
            }
            StageKind::Transcribe => {
                let whisper_model = rec.settings.whisper_model.clone();
                let language = rec.settings.language.clone();
                let transcript = run_transcribe(&app, &wav_path, &whisper_model, &language).await?;
                // Success: delete the wav to reclaim disk.
                let _ = std::fs::remove_file(&wav_path);
                Ok(storage::stages::CompletedOutput {
                    output: Some(transcript),
                    // Encode language into model_used for invalidation checks.
                    model_used: Some(format!("{}@{}", whisper_model, language)),
                    template_hash: None,
                    saved_paths: None,
                    completed_at: now_ms(),
                })
            }
            StageKind::Format => {
                // Load transcript from transcribe stage output.
                let rec2 = storage::recordings::get(&store, &recording_id)
                    .map_err(|e| format!("reload for format: {e}"))?
                    .ok_or_else(|| "recording vanished".to_string())?;
                let transcript = rec2
                    .stages
                    .iter()
                    .find(|s| s.stage == StageKind::Transcribe)
                    .and_then(|s| s.output.clone())
                    .ok_or_else(|| "transcribe output missing".to_string())?;
                let formatted = run_format(
                    &app,
                    &transcript,
                    &project.template.content,
                    &rec.settings.ollama_url,
                    &rec.settings.model,
                )
                .await?;
                Ok(storage::stages::CompletedOutput {
                    output: Some(formatted),
                    model_used: Some(rec.settings.model.clone()),
                    template_hash: Some(template_hash.clone()),
                    saved_paths: None,
                    completed_at: now_ms(),
                })
            }
            StageKind::Save => {
                // Save is user-interactive (file dialogs). We do not run it here —
                // the UI invokes `save_recording_outputs` separately once the user
                // picks destinations. Flip this stage to completed with an empty
                // saved_paths list; the UI will update it.
                Ok(storage::stages::CompletedOutput {
                    output: None,
                    model_used: None,
                    template_hash: None,
                    saved_paths: Some(Vec::new()),
                    completed_at: now_ms(),
                })
            }
        };

        match result {
            Ok(out) => {
                storage::stages::set_completed(&store, &recording_id, stage_kind, out)
                    .map_err(|e| format!("set_completed: {e}"))?;
            }
            Err(e) => {
                storage::stages::set_failed(&store, &recording_id, stage_kind, &e)
                    .map_err(|ee| format!("set_failed: {ee}"))?;
                return Err(e);
            }
        }
    }

    Ok(())
}
```

Note on the Save stage: file-dialog interaction is handled on the React side. The orchestrator marks Save as `completed` with an empty `saved_paths` list; a separate Tauri command (`record_saved_outputs`, added in Task 13) lets the UI append paths once the user confirms save locations. The `inputs_unchanged` rule for Save already handles an empty `saved_paths` list correctly (it returns `false`, so Save re-runs if the user never actually saved — which means hitting Resume again will re-enter the save flow).

- [ ] **Step 2: Register the command in `src-tauri/src/lib.rs`**

In the `invoke_handler![…]` list, add `orchestrator::run_recording,`.

- [ ] **Step 3: Verify compile**

Run: `cd src-tauri && cargo check`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/orchestrator.rs src-tauri/src/lib.rs
git commit -m "orchestrator: add run_recording command with skip-on-valid-cache"
```

---

# Phase 3 — Tauri command surface

## Task 12: Project + Recording CRUD commands

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add command functions at the bottom of `src-tauri/src/commands.rs`**

```rust
// ── Storage commands ──────────────────────────────────────────────────────────

use crate::storage::{self, AppSettings, Project, Recording, Store, Template, TemplateSource};
use std::sync::Arc;

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[tauri::command]
pub async fn list_projects(
    store: tauri::State<'_, Arc<Store>>,
) -> Result<Vec<Project>, String> {
    let store = (*store).clone();
    tokio::task::spawn_blocking(move || storage::projects::list(&store))
        .await
        .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn create_project(
    store: tauri::State<'_, Arc<Store>>,
    name: String,
    default_settings: AppSettings,
    template: Template,
) -> Result<Project, String> {
    let store = (*store).clone();
    tokio::task::spawn_blocking(move || {
        let id = uuid::Uuid::new_v4().to_string();
        let now = now_ms();
        let p = Project {
            id,
            name,
            created_at: now,
            updated_at: now,
            template,
            default_settings,
        };
        storage::projects::insert(&store, &p)?;
        Ok::<Project, String>(p)
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn update_project(
    store: tauri::State<'_, Arc<Store>>,
    project: Project,
) -> Result<(), String> {
    let store = (*store).clone();
    let mut p = project;
    p.updated_at = now_ms();
    tokio::task::spawn_blocking(move || storage::projects::update(&store, &p))
        .await
        .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn delete_project(
    app: AppHandle,
    store: tauri::State<'_, Arc<Store>>,
    id: String,
) -> Result<(), String> {
    use tauri::Manager;
    let store = (*store).clone();
    let recordings_root = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app_data_dir: {e}"))?
        .join("recordings");
    let id2 = id.clone();
    tokio::task::spawn_blocking(move || {
        // Collect recording IDs before the cascade deletes them, then remove
        // their on-disk folders after the DB delete succeeds.
        let recs = storage::recordings::list_by_project(&store, &id2)?;
        storage::projects::delete(&store, &id2)?;
        for r in recs {
            let _ = std::fs::remove_dir_all(recordings_root.join(&r.id));
        }
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn list_recordings(
    store: tauri::State<'_, Arc<Store>>,
    project_id: String,
) -> Result<Vec<Recording>, String> {
    let store = (*store).clone();
    tokio::task::spawn_blocking(move || storage::recordings::list_by_project(&store, &project_id))
        .await
        .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn get_recording(
    store: tauri::State<'_, Arc<Store>>,
    id: String,
) -> Result<Option<Recording>, String> {
    let store = (*store).clone();
    tokio::task::spawn_blocking(move || storage::recordings::get(&store, &id))
        .await
        .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn create_recording(
    store: tauri::State<'_, Arc<Store>>,
    project_id: String,
    name: String,
    source_path: String,
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
            source_path,
            created_at: now,
            updated_at: now,
            settings,
            stages: vec![],
        };
        storage::recordings::insert(&store, &r)?;
        // Re-read to pick up the auto-initialized stage rows.
        storage::recordings::get(&store, &r.id)?
            .ok_or_else(|| "recording vanished after insert".to_string())
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn update_recording(
    store: tauri::State<'_, Arc<Store>>,
    recording: Recording,
) -> Result<(), String> {
    let store = (*store).clone();
    let mut r = recording;
    r.updated_at = now_ms();
    tokio::task::spawn_blocking(move || storage::recordings::update(&store, &r))
        .await
        .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn delete_recording(
    app: AppHandle,
    store: tauri::State<'_, Arc<Store>>,
    id: String,
) -> Result<(), String> {
    use tauri::Manager;
    let store = (*store).clone();
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app_data_dir: {e}"))?
        .join("recordings")
        .join(&id);
    let id2 = id.clone();
    tokio::task::spawn_blocking(move || storage::recordings::delete(&store, &id2))
        .await
        .map_err(|e| format!("join: {e}"))??;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[tauri::command]
pub async fn start_over_recording(
    app: AppHandle,
    store: tauri::State<'_, Arc<Store>>,
    id: String,
) -> Result<(), String> {
    use tauri::Manager;
    let store = (*store).clone();
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app_data_dir: {e}"))?
        .join("recordings")
        .join(&id);
    let id2 = id.clone();
    tokio::task::spawn_blocking(move || {
        storage::stages::reset_from(&store, &id2, storage::StageKind::Extract)
    })
    .await
    .map_err(|e| format!("join: {e}"))??;
    let _ = std::fs::remove_file(dir.join("audio.wav"));
    Ok(())
}
```

- [ ] **Step 2: Register commands in `invoke_handler![…]`**

In `src-tauri/src/lib.rs`, add these to the handler list:

```
commands::list_projects,
commands::create_project,
commands::update_project,
commands::delete_project,
commands::list_recordings,
commands::get_recording,
commands::create_recording,
commands::update_recording,
commands::delete_recording,
commands::start_over_recording,
```

- [ ] **Step 3: Verify compile**

Run: `cd src-tauri && cargo check`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "commands: add project + recording CRUD + start_over"
```

---

## Task 13: `record_saved_outputs` + `run_save_outputs` helper

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`

This task adds two things:
1. `run_save_outputs` — an internal helper referenced by the orchestrator's `StageKind::Save` branch. It's a no-op placeholder today (the actual file dialogs happen on the React side), but having the symbol keeps the import in `orchestrator.rs` valid.
2. `record_saved_outputs` — a Tauri command the UI calls after the user has saved outputs, recording the paths into the save stage.

- [ ] **Step 1: Append to `src-tauri/src/commands.rs`**

```rust
/// Placeholder helper kept so the orchestrator has a symbol to import — the
/// actual save step runs on the React side via file dialogs. No-op.
pub async fn run_save_outputs() -> Result<(), String> {
    Ok(())
}

/// Called by the UI after the user finishes picking save paths and writing
/// the output files. Appends to the save stage's saved_paths and marks it
/// completed.
#[tauri::command]
pub async fn record_saved_outputs(
    store: tauri::State<'_, Arc<Store>>,
    recording_id: String,
    saved_paths: Vec<String>,
) -> Result<(), String> {
    let store = (*store).clone();
    tokio::task::spawn_blocking(move || {
        storage::stages::set_completed(
            &store,
            &recording_id,
            storage::StageKind::Save,
            storage::stages::CompletedOutput {
                output: None,
                model_used: None,
                template_hash: None,
                saved_paths: Some(saved_paths),
                completed_at: now_ms(),
            },
        )
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}
```

- [ ] **Step 2: Register command in `invoke_handler![…]`**

Add `commands::record_saved_outputs,` to the list.

- [ ] **Step 3: Verify compile**

Run: `cd src-tauri && cargo check`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "commands: add record_saved_outputs + run_save_outputs stub"
```

---

## Task 14: End-to-end orchestrator integration test (skip happy path; cover skip/invalidate/fail)

**Files:**
- Modify: `src-tauri/tests/storage_integration.rs`

This test drives the state machine directly (seeding stages) without actually running ffmpeg/whisper/Ollama. It verifies that `inputs_unchanged` + `reset_from` + stage helpers compose correctly end-to-end.

- [ ] **Step 1: Append test**

```rust
use meister_lib::orchestrator;

#[test]
fn invalidating_transcribe_resets_format_and_save() {
    let store = Store::open_memory().unwrap();
    storage::projects::insert(&store, &sample_project()).unwrap();
    storage::recordings::insert(&store, &sample_recording("p1")).unwrap();

    // All four stages "complete" at once (fake data).
    for kind in StageKind::ALL {
        let model_used = match kind {
            StageKind::Transcribe => Some("base@en".to_string()),
            StageKind::Format => Some("gemma3".to_string()),
            _ => None,
        };
        let template_hash = match kind {
            StageKind::Format => Some(storage::template_hash("# Minutes\n")),
            _ => None,
        };
        storage::stages::set_completed(
            &store, "r1", kind,
            storage::stages::CompletedOutput {
                output: Some("x".to_string()),
                model_used,
                template_hash,
                saved_paths: if kind == StageKind::Save { Some(vec![]) } else { None },
                completed_at: 1,
            },
        ).unwrap();
    }

    // Simulate the orchestrator visiting the Transcribe stage with a changed setting:
    // user swapped whisper model from base→small. inputs_unchanged returns false,
    // so reset_from is called.
    let mut rec = storage::recordings::get(&store, "r1").unwrap().unwrap();
    rec.settings.whisper_model = "small".to_string();

    let transcribe = rec.stages.iter().find(|s| s.stage == StageKind::Transcribe).unwrap();
    let template_hash = storage::template_hash("# Minutes\n");
    let valid = orchestrator::inputs_unchanged(
        StageKind::Transcribe, transcribe, &rec.settings,
        &template_hash, &rec.source_path, true,
    );
    assert!(!valid);

    storage::stages::reset_from(&store, "r1", StageKind::Transcribe).unwrap();
    let rec = storage::recordings::get(&store, "r1").unwrap().unwrap();
    assert_eq!(rec.stages[0].status, StageStatus::Completed); // extract kept
    assert_eq!(rec.stages[1].status, StageStatus::Pending);   // transcribe reset
    assert_eq!(rec.stages[2].status, StageStatus::Pending);   // format reset
    assert_eq!(rec.stages[3].status, StageStatus::Pending);   // save reset
}
```

- [ ] **Step 2: Run**

Run: `cd src-tauri && cargo test --test storage_integration`
Expected: all PASS.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/tests/storage_integration.rs
git commit -m "storage: integration test for invalidate+reset composition"
```

---

# Phase 4 — React foundation

## Task 15: TS types + API wrappers

**Files:**
- Create: `src/types/storage.ts`
- Create: `src/api/storage.ts`

- [ ] **Step 1: Create `src/types/storage.ts`**

```ts
export type OutputFormat = "md" | "txt" | "docx" | "pdf";

export interface AppSettings {
  ollamaUrl: string;
  model: string;
  whisperModel: string;
  language: string;
  outputFormats: OutputFormat[];
}

export type TemplateSource = "inline" | "file";

export interface Template {
  source: TemplateSource;
  content: string;
  sourcePath?: string | null;
}

export interface Project {
  id: string;
  name: string;
  createdAt: number;
  updatedAt: number;
  template: Template;
  defaultSettings: AppSettings;
}

export type StageKind = "extract" | "transcribe" | "format" | "save";
export type StageStatus = "pending" | "running" | "completed" | "failed";

export interface StageRecord {
  stage: StageKind;
  status: StageStatus;
  error?: string | null;
  completedAt?: number | null;
  modelUsed?: string | null;
  templateHash?: string | null;
  output?: string | null;
  savedPaths?: string[] | null;
}

export interface Recording {
  id: string;
  projectId: string;
  name: string;
  sourcePath: string;
  createdAt: number;
  updatedAt: number;
  settings: AppSettings;
  stages: StageRecord[];
}

export const STAGES: StageKind[] = ["extract", "transcribe", "format", "save"];

export function overallStatus(r: Recording): "new" | "in_progress" | "completed" | "failed" {
  if (r.stages.some((s) => s.status === "failed")) return "failed";
  if (r.stages.some((s) => s.status === "running")) return "in_progress";
  if (r.stages.every((s) => s.status === "completed")) return "completed";
  if (r.stages.some((s) => s.status !== "pending")) return "in_progress";
  return "new";
}
```

- [ ] **Step 2: Create `src/api/storage.ts`**

```ts
import { invoke } from "@tauri-apps/api/core";
import type { AppSettings, Project, Recording, Template } from "../types/storage";

export const api = {
  listProjects: () => invoke<Project[]>("list_projects"),

  createProject: (name: string, defaultSettings: AppSettings, template: Template) =>
    invoke<Project>("create_project", { name, defaultSettings, template }),

  updateProject: (project: Project) => invoke<void>("update_project", { project }),

  deleteProject: (id: string) => invoke<void>("delete_project", { id }),

  listRecordings: (projectId: string) =>
    invoke<Recording[]>("list_recordings", { projectId }),

  getRecording: (id: string) => invoke<Recording | null>("get_recording", { id }),

  createRecording: (
    projectId: string,
    name: string,
    sourcePath: string,
    settings: AppSettings,
  ) => invoke<Recording>("create_recording", { projectId, name, sourcePath, settings }),

  updateRecording: (recording: Recording) =>
    invoke<void>("update_recording", { recording }),

  deleteRecording: (id: string) => invoke<void>("delete_recording", { id }),

  startOverRecording: (id: string) => invoke<void>("start_over_recording", { id }),

  runRecording: (recordingId: string) =>
    invoke<void>("run_recording", { recordingId }),

  recordSavedOutputs: (recordingId: string, savedPaths: string[]) =>
    invoke<void>("record_saved_outputs", { recordingId, savedPaths }),
};
```

- [ ] **Step 3: Verify compile**

Run: `npm run build`
Expected: tsc passes.

- [ ] **Step 4: Commit**

```bash
git add src/types/storage.ts src/api/storage.ts
git commit -m "frontend: add storage types + typed API wrappers"
```

---

## Task 16: App shell restructure — sidebar + main pane, no functionality yet

**Files:**
- Modify: `src/App.tsx`

- [ ] **Step 1: Rewrite `src/App.tsx`**

This step replaces the whole file. Existing components (`InputStep`, `SettingsStep`, `ProcessingStep`, `ResultStep`) are not yet removed — the new App just doesn't render them. Removal happens after replacements land in later tasks.

```tsx
import { useCallback, useEffect, useState } from "react";
import TitleBar from "./components/TitleBar";
import Sidebar from "./components/Sidebar";
import ProjectView from "./components/ProjectView";
import RecordingView from "./components/RecordingView";
import { api } from "./api/storage";
import type { Project, Recording } from "./types/storage";

export type Selection =
  | { kind: "none" }
  | { kind: "project"; projectId: string }
  | { kind: "recording"; projectId: string; recordingId: string };

export default function App() {
  const [projects, setProjects] = useState<Project[]>([]);
  const [recordingsByProject, setRecordingsByProject] = useState<Record<string, Recording[]>>({});
  const [selection, setSelection] = useState<Selection>({ kind: "none" });
  const [loadingProjects, setLoadingProjects] = useState(true);

  const refreshProjects = useCallback(async () => {
    const list = await api.listProjects();
    setProjects(list);
  }, []);

  const refreshRecordings = useCallback(async (projectId: string) => {
    const list = await api.listRecordings(projectId);
    setRecordingsByProject((prev) => ({ ...prev, [projectId]: list }));
  }, []);

  useEffect(() => {
    refreshProjects().finally(() => setLoadingProjects(false));
  }, [refreshProjects]);

  useEffect(() => {
    for (const p of projects) {
      if (!(p.id in recordingsByProject)) {
        refreshRecordings(p.id);
      }
    }
  }, [projects, recordingsByProject, refreshRecordings]);

  return (
    <div className="flex flex-col h-screen bg-surface overflow-hidden">
      <TitleBar />
      <div className="flex-1 flex overflow-hidden">
        <Sidebar
          projects={projects}
          recordingsByProject={recordingsByProject}
          selection={selection}
          onSelect={setSelection}
          onProjectsChanged={refreshProjects}
        />
        <div className="flex-1 overflow-hidden">
          {selection.kind === "none" && (
            <EmptyState loading={loadingProjects} hasProjects={projects.length > 0} />
          )}
          {selection.kind === "project" && (
            <ProjectView
              projectId={selection.projectId}
              onProjectChanged={refreshProjects}
              onRecordingCreated={(r) => {
                refreshRecordings(r.projectId);
                setSelection({ kind: "recording", projectId: r.projectId, recordingId: r.id });
              }}
            />
          )}
          {selection.kind === "recording" && (
            <RecordingView
              recordingId={selection.recordingId}
              onRecordingChanged={() => refreshRecordings(selection.projectId)}
              onRecordingDeleted={() => {
                refreshRecordings(selection.projectId);
                setSelection({ kind: "project", projectId: selection.projectId });
              }}
            />
          )}
        </div>
      </div>
    </div>
  );
}

function EmptyState({ loading, hasProjects }: { loading: boolean; hasProjects: boolean }) {
  if (loading) return <div className="flex h-full items-center justify-center text-white/40">Loading…</div>;
  return (
    <div className="flex h-full items-center justify-center">
      <div className="text-center max-w-md px-8">
        <h1 className="text-xl font-semibold mb-2">
          {hasProjects ? "Select a project from the sidebar" : "Create your first project"}
        </h1>
        <p className="text-sm text-white/40">
          Projects group recordings that share a template and default settings.
        </p>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Create minimal stubs so `npm run build` passes**

Create `src/components/Sidebar.tsx`:

```tsx
import type { Project, Recording } from "../types/storage";
import type { Selection } from "../App";

interface Props {
  projects: Project[];
  recordingsByProject: Record<string, Recording[]>;
  selection: Selection;
  onSelect: (s: Selection) => void;
  onProjectsChanged: () => void;
}

export default function Sidebar({ projects }: Props) {
  return (
    <aside className="w-60 shrink-0 border-r border-white/5 bg-surface-secondary p-3 text-sm text-white/40">
      <div>Projects ({projects.length})</div>
    </aside>
  );
}
```

Create `src/components/ProjectView.tsx`:

```tsx
import type { Recording } from "../types/storage";

interface Props {
  projectId: string;
  onProjectChanged: () => void;
  onRecordingCreated: (r: Recording) => void;
}

export default function ProjectView(_: Props) {
  return <div className="p-8 text-white/40">Project view (stub)</div>;
}
```

Create `src/components/RecordingView.tsx`:

```tsx
interface Props {
  recordingId: string;
  onRecordingChanged: () => void;
  onRecordingDeleted: () => void;
}

export default function RecordingView(_: Props) {
  return <div className="p-8 text-white/40">Recording view (stub)</div>;
}
```

- [ ] **Step 3: Verify build**

Run: `npm run build`
Expected: tsc + vite build succeed.

- [ ] **Step 4: Smoke test**

Run: `npm run tauri dev`
Expected: app opens with an empty sidebar and "Create your first project" card. No crashes.

- [ ] **Step 5: Commit**

```bash
git add src/App.tsx src/components/Sidebar.tsx src/components/ProjectView.tsx src/components/RecordingView.tsx
git commit -m "frontend: app shell with sidebar + project/recording view stubs"
```

---

# Phase 5 — Sidebar + project management

## Task 17: Sidebar — list, select, create project

**Files:**
- Modify: `src/components/Sidebar.tsx`

- [ ] **Step 1: Replace `src/components/Sidebar.tsx`**

```tsx
import { useState } from "react";
import type { Project, Recording, StageStatus } from "../types/storage";
import { overallStatus } from "../types/storage";
import { api } from "../api/storage";
import type { Selection } from "../App";

interface Props {
  projects: Project[];
  recordingsByProject: Record<string, Recording[]>;
  selection: Selection;
  onSelect: (s: Selection) => void;
  onProjectsChanged: () => void;
}

const DEFAULTS = {
  ollamaUrl: "http://localhost:11434",
  model: "gemma3",
  whisperModel: "base",
  language: "auto",
  outputFormats: ["md"] as const,
};

export default function Sidebar({
  projects,
  recordingsByProject,
  selection,
  onSelect,
  onProjectsChanged,
}: Props) {
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [creating, setCreating] = useState(false);
  const [newName, setNewName] = useState("");

  const toggleExpand = (id: string) =>
    setExpanded((e) => ({ ...e, [id]: !e[id] }));

  const createProject = async () => {
    const name = newName.trim();
    if (!name) return;
    const p = await api.createProject(
      name,
      {
        ollamaUrl: DEFAULTS.ollamaUrl,
        model: DEFAULTS.model,
        whisperModel: DEFAULTS.whisperModel,
        language: DEFAULTS.language,
        outputFormats: [...DEFAULTS.outputFormats],
      },
      { source: "inline", content: "", sourcePath: null },
    );
    setNewName("");
    setCreating(false);
    onProjectsChanged();
    onSelect({ kind: "project", projectId: p.id });
    setExpanded((e) => ({ ...e, [p.id]: true }));
  };

  const projectSelected = (id: string) =>
    selection.kind === "project" && selection.projectId === id;
  const recordingSelected = (id: string) =>
    selection.kind === "recording" && selection.recordingId === id;

  return (
    <aside className="w-60 shrink-0 border-r border-white/5 bg-surface-secondary flex flex-col">
      <div className="p-3 border-b border-white/5">
        {!creating ? (
          <button
            onClick={() => setCreating(true)}
            className="w-full px-3 py-2 rounded-lg text-sm bg-accent/10 hover:bg-accent/20 text-accent"
          >
            + New project
          </button>
        ) : (
          <div className="flex gap-2">
            <input
              autoFocus
              className="input flex-1"
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") createProject();
                if (e.key === "Escape") { setCreating(false); setNewName(""); }
              }}
              placeholder="Project name"
            />
            <button
              onClick={createProject}
              className="px-2 py-1 text-xs rounded bg-accent text-surface"
            >
              ✓
            </button>
          </div>
        )}
      </div>

      <div className="flex-1 overflow-y-auto scrollable p-1">
        {projects.length === 0 && (
          <p className="text-xs text-white/30 p-3">No projects yet.</p>
        )}
        {projects.map((p) => {
          const recs = recordingsByProject[p.id] ?? [];
          const isExpanded = expanded[p.id] ?? true;
          return (
            <div key={p.id}>
              <button
                onClick={() => {
                  onSelect({ kind: "project", projectId: p.id });
                  toggleExpand(p.id);
                }}
                className={`w-full text-left px-2 py-1.5 rounded flex items-center gap-1.5 text-sm
                  ${projectSelected(p.id)
                    ? "bg-accent/15 text-accent"
                    : "hover:bg-white/5 text-white/80"}`}
              >
                <span className="text-white/30 text-xs w-3">{isExpanded ? "▾" : "▸"}</span>
                <span className="truncate flex-1">{p.name}</span>
              </button>
              {isExpanded && recs.map((r) => (
                <button
                  key={r.id}
                  onClick={() =>
                    onSelect({ kind: "recording", projectId: p.id, recordingId: r.id })
                  }
                  className={`w-full text-left pl-7 pr-2 py-1 rounded flex items-center gap-2 text-sm
                    ${recordingSelected(r.id)
                      ? "bg-accent/15 text-accent"
                      : "hover:bg-white/5 text-white/70"}`}
                >
                  <StatusDot status={overallStatus(r)} />
                  <span className="truncate flex-1">{r.name}</span>
                </button>
              ))}
              {isExpanded && recs.length === 0 && (
                <p className="pl-7 py-1 text-xs text-white/25">No recordings</p>
              )}
            </div>
          );
        })}
      </div>
    </aside>
  );
}

function StatusDot({ status }: { status: "new" | "in_progress" | "completed" | "failed" }) {
  const color =
    status === "failed" ? "bg-red-400"
    : status === "completed" ? "bg-green-400"
    : status === "in_progress" ? "bg-blue-400"
    : "bg-white/20";
  return <span className={`w-1.5 h-1.5 rounded-full shrink-0 ${color}`} />;
}
```

- [ ] **Step 2: Build & smoke-test**

Run: `npm run build` → expected pass.
Run: `npm run tauri dev` → click "+ New project", type a name, press Enter. Project appears in sidebar and gets selected. Restart the app — project persists.

- [ ] **Step 3: Commit**

```bash
git add src/components/Sidebar.tsx
git commit -m "frontend: sidebar with project list, create, and status dots"
```

---

## Task 18: Extract reusable components from existing InputStep / SettingsStep / ResultStep

**Files:**
- Create: `src/components/TemplateEditor.tsx`
- Create: `src/components/RecordingDropZone.tsx`
- Create: `src/components/SettingsForm.tsx`
- Create: `src/components/OutputPreview.tsx`

Copy behavior from the existing files without referencing them; the originals will be deleted later in the plan.

- [ ] **Step 1: Create `src/components/TemplateEditor.tsx`**

```tsx
import { useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { Template } from "../types/storage";

interface Props {
  template: Template;
  onChange: (t: Template) => void;
}

const TEMPLATE_EXTS = new Set(["md", "txt", "docx"]);

export default function TemplateEditor({ template, onChange }: Props) {
  const [dragging, setDragging] = useState(false);
  const textRef = useRef<HTMLTextAreaElement>(null);

  const loadFile = async (path: string) => {
    const content = await invoke<string>("read_file_as_text", { path });
    onChange({ source: "file", content, sourcePath: path });
  };

  const openFile = async () => {
    const path = await open({
      filters: [{ name: "Document", extensions: ["md", "txt", "docx"] }],
    });
    if (path && typeof path === "string") await loadFile(path);
  };

  const handleDrop = async (e: React.DragEvent) => {
    e.preventDefault();
    setDragging(false);
    const file = e.dataTransfer.files[0];
    if (!file) return;
    const ext = file.name.split(".").pop()?.toLowerCase() ?? "";
    if (!TEMPLATE_EXTS.has(ext)) return;
    const path = (file as File & { path?: string }).path ?? file.name;
    await loadFile(path);
  };

  const filename = (p?: string | null) => p ? p.split(/[\\/]/).pop() ?? p : "";

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between">
        <label className="text-xs font-medium text-white/50 uppercase tracking-wider">
          Format Template
        </label>
        <button
          onClick={openFile}
          className="text-xs text-accent hover:text-accent-hover transition-colors"
        >
          {template.sourcePath ? "Load different file" : "Load from file"}
        </button>
      </div>
      <div
        className={`relative rounded-xl border transition-colors
          ${dragging ? "border-accent bg-accent/10" : "border-white/10"}`}
        onDragOver={(e) => { e.preventDefault(); setDragging(true); }}
        onDragLeave={() => setDragging(false)}
        onDrop={handleDrop}
      >
        {dragging && (
          <div className="absolute inset-0 flex items-center justify-center rounded-xl bg-accent/10 z-10 pointer-events-none">
            <p className="text-accent font-medium">Drop template file here</p>
          </div>
        )}
        <textarea
          ref={textRef}
          className="w-full h-48 bg-surface-secondary rounded-xl p-4 text-sm font-mono text-white/80 resize-y outline-none focus:ring-1 focus:ring-accent/50 placeholder-white/20 scrollable"
          placeholder={"# Meeting Minutes\nDate: [DATE]\n..."}
          value={template.content}
          onChange={(e) =>
            onChange({ source: "inline", content: e.target.value, sourcePath: null })
          }
          spellCheck={false}
        />
      </div>
      {template.sourcePath && (
        <p className="text-xs text-white/30 truncate">
          Loaded: {filename(template.sourcePath)}
        </p>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Create `src/components/RecordingDropZone.tsx`**

```tsx
import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";

interface Props {
  onPicked: (filePath: string) => void;
}

const AUDIO_VIDEO_EXTS = new Set([
  "mp4", "mov", "mkv", "avi", "mp3", "m4a", "wav", "ogg", "webm", "aac", "flac",
]);

export default function RecordingDropZone({ onPicked }: Props) {
  const [dragging, setDragging] = useState(false);

  const openFile = async () => {
    const path = await open({
      filters: [{ name: "Audio / Video", extensions: [...AUDIO_VIDEO_EXTS] }],
    });
    if (path && typeof path === "string") onPicked(path);
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setDragging(false);
    const file = e.dataTransfer.files[0];
    if (!file) return;
    const ext = file.name.split(".").pop()?.toLowerCase() ?? "";
    if (!AUDIO_VIDEO_EXTS.has(ext)) return;
    const path = (file as File & { path?: string }).path ?? file.name;
    onPicked(path);
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
        Drop a recording here, or click to browse
      </p>
      <p className="text-xs text-white/25">mp4 · mov · mp3 · m4a · wav · …</p>
    </div>
  );
}
```

- [ ] **Step 3: Create `src/components/SettingsForm.tsx`**

```tsx
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AppSettings, OutputFormat } from "../types/storage";
import DownloadModelModal from "./DownloadModelModal";

interface Props {
  settings: AppSettings;
  onChange: (s: AppSettings) => void;
}

interface ModelInfo {
  size: string;
  downloaded: boolean;
  size_bytes: number;
}

const OUTPUT_FORMATS: { id: OutputFormat; label: string; desc: string }[] = [
  { id: "md", label: "Markdown", desc: ".md" },
  { id: "txt", label: "Plain text", desc: ".txt" },
  { id: "docx", label: "Word doc", desc: ".docx" },
  { id: "pdf", label: "PDF", desc: ".pdf" },
];

function formatSize(bytes: number) {
  if (bytes >= 1024 ** 3) return (bytes / 1024 ** 3).toFixed(1) + " GB";
  return Math.round(bytes / 1024 ** 2) + " MB";
}

function modelLabel(m: ModelInfo) {
  if (m.size === "base" && m.downloaded) return `${m.size} — bundled ✓`;
  if (m.downloaded) return `${m.size} — downloaded ✓ (${formatSize(m.size_bytes)})`;
  return `${m.size} — ${formatSize(m.size_bytes)} (not downloaded)`;
}

export default function SettingsForm({ settings, onChange }: Props) {
  const [ollamaModels, setOllamaModels] = useState<string[]>([]);
  const [modelInfos, setModelInfos] = useState<ModelInfo[]>([]);
  const [pendingDownload, setPendingDownload] = useState<ModelInfo | null>(null);

  const set = <K extends keyof AppSettings>(key: K, val: AppSettings[K]) =>
    onChange({ ...settings, [key]: val });

  const toggleFormat = (fmt: OutputFormat) => {
    const next = settings.outputFormats.includes(fmt)
      ? settings.outputFormats.filter((f) => f !== fmt)
      : [...settings.outputFormats, fmt];
    if (next.length > 0) set("outputFormats", next);
  };

  useEffect(() => { refresh(); }, []);

  const refresh = async () => {
    try {
      const m = await invoke<string[]>("list_ollama_models", { ollamaUrl: settings.ollamaUrl });
      setOllamaModels(m);
      if (m.length > 0 && !m.includes(settings.model)) set("model", m[0]);
    } catch { /* ignore */ }
    try {
      const infos = await invoke<ModelInfo[]>("list_available_models");
      setModelInfos(infos);
    } catch { /* ignore */ }
  };

  const pickWhisper = (newSize: string) => {
    const info = modelInfos.find((m) => m.size === newSize);
    if (!info) return;
    if (info.downloaded) set("whisperModel", newSize);
    else setPendingDownload(info);
  };

  return (
    <div className="flex flex-col gap-5">
      <Section title="Ollama">
        <Field label="Server URL">
          <input type="text" className="input"
            value={settings.ollamaUrl}
            onChange={(e) => set("ollamaUrl", e.target.value)}
            onBlur={refresh}
            placeholder="http://localhost:11434" />
        </Field>
        <Field label="Model">
          {ollamaModels.length > 0 ? (
            <select className="input" value={settings.model}
              onChange={(e) => set("model", e.target.value)}>
              {ollamaModels.map((m) => <option key={m} value={m}>{m}</option>)}
            </select>
          ) : (
            <input type="text" className="input" value={settings.model}
              onChange={(e) => set("model", e.target.value)} placeholder="gemma3" />
          )}
        </Field>
      </Section>

      <Section title="Transcription">
        <Field label="Model">
          <select className="input" value={settings.whisperModel}
            onChange={(e) => pickWhisper(e.target.value)}>
            {modelInfos.map((m) => (
              <option key={m.size} value={m.size}>{modelLabel(m)}</option>
            ))}
          </select>
        </Field>
        <Field label="Language">
          <input type="text" className="input"
            value={settings.language}
            onChange={(e) => set("language", e.target.value)}
            placeholder="auto  (or e.g. en, de, fr)" />
        </Field>
      </Section>

      <Section title="Output Formats">
        <div className="flex gap-2 flex-wrap">
          {OUTPUT_FORMATS.map(({ id, label, desc }) => {
            const active = settings.outputFormats.includes(id);
            return (
              <button key={id} onClick={() => toggleFormat(id)}
                className={`px-4 py-2 rounded-lg text-sm transition-colors flex flex-col items-center gap-0.5
                  ${active
                    ? "bg-accent/20 border border-accent text-accent"
                    : "bg-surface-secondary border border-white/10 text-white/50 hover:border-white/20"}`}>
                <span className="font-medium">{label}</span>
                <span className="text-[10px] opacity-60">{desc}</span>
              </button>
            );
          })}
        </div>
      </Section>

      {pendingDownload && (
        <DownloadModelModal
          size={pendingDownload.size}
          totalBytesEstimate={pendingDownload.size_bytes}
          onComplete={() => {
            set("whisperModel", pendingDownload.size);
            setPendingDownload(null);
            refresh();
          }}
          onCancel={() => setPendingDownload(null)}
        />
      )}
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-2">
      <h2 className="text-xs font-semibold text-white/40 uppercase tracking-wider">{title}</h2>
      {children}
    </div>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1">
      <label className="text-sm text-white/60">{label}</label>
      {children}
    </div>
  );
}
```

- [ ] **Step 4: Create `src/components/OutputPreview.tsx`**

```tsx
import { useState } from "react";

interface Props {
  transcript?: string | null;
  formatted?: string | null;
  savedPaths?: string[] | null;
}

export default function OutputPreview({ transcript, formatted, savedPaths }: Props) {
  const [tab, setTab] = useState<"formatted" | "transcript">("formatted");
  const has = (v?: string | null) => (v ?? "").length > 0;

  if (!has(transcript) && !has(formatted)) return null;

  return (
    <div className="flex flex-col gap-3">
      <div className="flex gap-2 text-xs">
        <button
          onClick={() => setTab("formatted")}
          className={`px-3 py-1 rounded ${
            tab === "formatted" ? "bg-accent/15 text-accent" : "text-white/50 hover:text-white/80"
          }`}
        >
          Formatted
        </button>
        <button
          onClick={() => setTab("transcript")}
          className={`px-3 py-1 rounded ${
            tab === "transcript" ? "bg-accent/15 text-accent" : "text-white/50 hover:text-white/80"
          }`}
        >
          Transcript
        </button>
      </div>
      <pre className="bg-surface-secondary rounded-xl p-4 text-xs font-mono text-white/80 whitespace-pre-wrap max-h-96 overflow-y-auto scrollable">
        {tab === "formatted" ? (formatted ?? "") : (transcript ?? "")}
      </pre>
      {savedPaths && savedPaths.length > 0 && (
        <div className="text-xs text-white/40">
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

- [ ] **Step 5: Verify build**

Run: `npm run build`
Expected: clean compile.

- [ ] **Step 6: Commit**

```bash
git add src/components/TemplateEditor.tsx src/components/RecordingDropZone.tsx src/components/SettingsForm.tsx src/components/OutputPreview.tsx
git commit -m "frontend: extract reusable TemplateEditor/DropZone/SettingsForm/OutputPreview"
```

---

## Task 19: ProjectView — wire template, default settings, recordings list, add-recording

**Files:**
- Modify: `src/components/ProjectView.tsx`

- [ ] **Step 1: Replace `src/components/ProjectView.tsx`**

```tsx
import { useCallback, useEffect, useState } from "react";
import TemplateEditor from "./TemplateEditor";
import SettingsForm from "./SettingsForm";
import RecordingDropZone from "./RecordingDropZone";
import { api } from "../api/storage";
import type { Project, Recording } from "../types/storage";

interface Props {
  projectId: string;
  onProjectChanged: () => void;
  onRecordingCreated: (r: Recording) => void;
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

  const load = useCallback(async () => {
    const list = await api.listProjects();
    const p = list.find((x) => x.id === projectId) ?? null;
    setProject(p);
    if (p) setDraftName(p.name);
    const recs = await api.listRecordings(projectId);
    setRecordings(recs);
  }, [projectId]);

  useEffect(() => { load(); }, [load]);

  const save = async (next: Project) => {
    await api.updateProject(next);
    setProject(next);
    onProjectChanged();
  };

  const addRecording = async (sourcePath: string) => {
    if (!project) return;
    const filename = sourcePath.split(/[\\/]/).pop() ?? sourcePath;
    const name = filename.replace(/\.[^.]+$/, "");
    const rec = await api.createRecording(
      projectId,
      name,
      sourcePath,
      project.defaultSettings,
    );
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
          <RecordingDropZone onPicked={addRecording} />
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Build & smoke test**

Run: `npm run build` → expected pass.
Run: `npm run tauri dev` → select a project, rename it, paste a template, drop a recording → a new recording shows in the sidebar and the view switches to RecordingView (still a stub).

- [ ] **Step 3: Commit**

```bash
git add src/components/ProjectView.tsx
git commit -m "frontend: project view — template + defaults + recordings list"
```

---

# Phase 6 — Recording workflow UI

## Task 20: StageCard + RecordingView skeleton

**Files:**
- Create: `src/components/StageCard.tsx`
- Modify: `src/components/RecordingView.tsx`

- [ ] **Step 1: Create `src/components/StageCard.tsx`**

```tsx
import type { StageKind, StageStatus } from "../types/storage";

interface Props {
  kind: StageKind;
  status: StageStatus;
  error?: string | null;
  subtitle?: string;
  progress?: number;
}

const LABELS: Record<StageKind, string> = {
  extract: "Extract audio",
  transcribe: "Transcribe",
  format: "Format with LLM",
  save: "Save outputs",
};

const ICONS: Record<StageKind, string> = {
  extract: "🎛",
  transcribe: "✍️",
  format: "🤖",
  save: "💾",
};

export default function StageCard({ kind, status, error, subtitle, progress }: Props) {
  const tone =
    status === "completed" ? "border-green-500/30 bg-green-500/5"
    : status === "running" ? "border-accent/40 bg-accent/10"
    : status === "failed" ? "border-red-500/30 bg-red-500/10"
    : "border-white/10 bg-surface-secondary";

  const statusText =
    status === "completed" ? "Completed"
    : status === "running" ? "Running…"
    : status === "failed" ? "Failed"
    : "Pending";

  return (
    <div className={`rounded-xl border p-4 flex flex-col gap-2 ${tone}`}>
      <div className="flex items-center gap-3">
        <span className="text-xl">{ICONS[kind]}</span>
        <div className="flex-1">
          <div className="text-sm font-medium">{LABELS[kind]}</div>
          {subtitle && <div className="text-xs text-white/40">{subtitle}</div>}
        </div>
        <div className={`text-xs ${
          status === "failed" ? "text-red-400"
          : status === "completed" ? "text-green-400"
          : status === "running" ? "text-accent"
          : "text-white/30"
        }`}>{statusText}</div>
      </div>
      {status === "running" && typeof progress === "number" && progress > 0 && (
        <div className="h-1.5 w-full bg-white/5 rounded-full overflow-hidden">
          <div className="h-full bg-accent rounded-full transition-all"
               style={{ width: `${Math.min(100, Math.max(0, progress))}%` }} />
        </div>
      )}
      {status === "failed" && error && (
        <pre className="text-xs text-red-400 whitespace-pre-wrap font-mono">{error}</pre>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Replace `src/components/RecordingView.tsx`**

```tsx
import { useCallback, useEffect, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import StageCard from "./StageCard";
import OutputPreview from "./OutputPreview";
import SettingsForm from "./SettingsForm";
import { api } from "../api/storage";
import {
  overallStatus,
  type Recording,
  type StageRecord,
  type StageKind,
} from "../types/storage";

interface Props {
  recordingId: string;
  onRecordingChanged: () => void;
  onRecordingDeleted: () => void;
}

const STAGE_ORDER: StageKind[] = ["extract", "transcribe", "format", "save"];

export default function RecordingView({
  recordingId,
  onRecordingChanged,
  onRecordingDeleted,
}: Props) {
  const [rec, setRec] = useState<Recording | null>(null);
  const [running, setRunning] = useState(false);
  const [transcribePct, setTranscribePct] = useState(0);
  const [formatChars, setFormatChars] = useState(0);

  const reload = useCallback(async () => {
    const r = await api.getRecording(recordingId);
    setRec(r);
  }, [recordingId]);

  useEffect(() => { reload(); }, [reload]);

  useEffect(() => {
    let unlisten1: UnlistenFn | undefined;
    let unlisten2: UnlistenFn | undefined;
    listen<{ percent: number }>("transcribe-progress", (e) =>
      setTranscribePct(e.payload.percent)
    ).then((fn) => { unlisten1 = fn; });
    listen<{ chars: number }>("format-progress", (e) =>
      setFormatChars(e.payload.chars)
    ).then((fn) => { unlisten2 = fn; });
    return () => { unlisten1?.(); unlisten2?.(); };
  }, []);

  if (!rec) return <div className="p-8 text-white/40">Loading…</div>;

  const stageOf = (k: StageKind): StageRecord | undefined =>
    rec.stages.find((s) => s.stage === k);
  const transcript = stageOf("transcribe")?.output ?? "";
  const formatted = stageOf("format")?.output ?? "";
  const savedPaths = stageOf("save")?.savedPaths ?? [];
  const anyFailed = rec.stages.some((s) => s.status === "failed");
  const allCompleted = rec.stages.every((s) => s.status === "completed");
  const canRun = !running && !allCompleted; // re-run happens via Start over + Run

  const run = async () => {
    setRunning(true);
    setTranscribePct(0);
    setFormatChars(0);
    try {
      await api.runRecording(recordingId);
      await reload();
      // If save stage is "completed" with empty savedPaths, ask the user to pick paths.
      const updated = await api.getRecording(recordingId);
      if (updated) {
        const s = updated.stages.find((x) => x.stage === "save");
        if (s && s.status === "completed" && (s.savedPaths ?? []).length === 0) {
          await runSaveFlow(updated);
        }
      }
      await reload();
    } catch (_) {
      await reload();
    } finally {
      setRunning(false);
      onRecordingChanged();
    }
  };

  const runSaveFlow = async (r: Recording) => {
    const formatted = r.stages.find((s) => s.stage === "format")?.output ?? "";
    const paths: string[] = [];
    for (const fmt of r.settings.outputFormats) {
      const defaultName = `${r.name}.${fmt}`;
      const dest = await saveDialog({
        defaultPath: defaultName,
        filters: [{ name: fmt.toUpperCase(), extensions: [fmt] }],
      });
      if (!dest) continue;
      if (fmt === "md" || fmt === "txt") {
        await invoke("save_text_file", { content: formatted, path: dest });
        paths.push(dest);
      } else if (fmt === "docx") {
        const bytes = await generateDocx(formatted);
        await invoke("save_binary_file", { bytes: Array.from(bytes), path: dest });
        paths.push(dest);
      } else if (fmt === "pdf") {
        const bytes = await generatePdf(formatted);
        await invoke("save_binary_file", { bytes: Array.from(bytes), path: dest });
        paths.push(dest);
      }
    }
    await api.recordSavedOutputs(r.id, paths);
  };

  const startOver = async () => {
    if (!confirm("Start over — discard all progress for this recording?")) return;
    await api.startOverRecording(recordingId);
    await reload();
    onRecordingChanged();
  };

  const deleteMe = async () => {
    if (!confirm(`Delete recording '${rec.name}'?`)) return;
    await api.deleteRecording(recordingId);
    onRecordingDeleted();
  };

  return (
    <div className="h-full flex flex-col">
      <div className="flex-1 overflow-y-auto scrollable p-8 flex flex-col gap-6">
        <div className="flex items-center justify-between">
          <h1 className="text-xl font-semibold">{rec.name}</h1>
          <div className="flex items-center gap-2 text-xs">
            <span className="text-white/40">{overallStatus(rec)}</span>
            <button onClick={deleteMe} className="text-white/40 hover:text-red-400">Delete</button>
          </div>
        </div>
        <p className="text-xs text-white/40 truncate">Source: {rec.sourcePath}</p>

        <div className="flex flex-col gap-2">
          {STAGE_ORDER.map((k) => {
            const s = stageOf(k)!;
            const subtitle =
              k === "transcribe" ? `model: ${rec.settings.whisperModel}`
              : k === "format" ? `model: ${rec.settings.model}`
              : k === "save" ? `formats: ${rec.settings.outputFormats.join(", ")}`
              : undefined;
            const progress =
              s.status === "running" && k === "transcribe" ? transcribePct
              : undefined;
            return (
              <StageCard
                key={k}
                kind={k}
                status={s.status}
                error={s.error}
                subtitle={subtitle}
                progress={progress}
              />
            );
          })}
        </div>

        <details className="bg-surface-secondary rounded-xl p-4">
          <summary className="cursor-pointer text-sm text-white/60">Recording settings</summary>
          <div className="mt-4">
            <SettingsForm
              settings={rec.settings}
              onChange={async (settings) => {
                const next = { ...rec, settings };
                setRec(next);
                await api.updateRecording(next);
                onRecordingChanged();
              }}
            />
          </div>
        </details>

        <OutputPreview
          transcript={transcript}
          formatted={formatted}
          savedPaths={savedPaths}
        />

        {rec.stages.find((s) => s.stage === "format")?.status === "running" && formatChars > 0 && (
          <p className="text-xs text-white/40">Formatting… {formatChars.toLocaleString()} chars</p>
        )}
      </div>

      <div className="shrink-0 border-t border-white/5 p-4 flex items-center justify-between bg-surface">
        <button
          onClick={startOver}
          className="text-sm text-white/50 hover:text-white"
        >
          Start over
        </button>
        <button
          disabled={!canRun}
          onClick={run}
          className="px-6 py-2 rounded-lg bg-accent hover:bg-accent-hover disabled:opacity-30 disabled:cursor-not-allowed text-sm font-semibold transition-colors"
        >
          {running ? "Running…" : anyFailed ? "Resume" : "Run"}
        </button>
      </div>
    </div>
  );
}

// ── Output generators (copied verbatim from old App.tsx) ─────────────────────

async function generateDocx(markdown: string): Promise<Uint8Array> {
  const { Document, Packer, Paragraph, TextRun, HeadingLevel } = await import("docx");
  const lines = markdown.split("\n");
  const children = lines.map((line) => {
    if (line.startsWith("### ")) {
      return new Paragraph({ text: line.slice(4), heading: HeadingLevel.HEADING_3 });
    } else if (line.startsWith("## ")) {
      return new Paragraph({ text: line.slice(3), heading: HeadingLevel.HEADING_2 });
    } else if (line.startsWith("# ")) {
      return new Paragraph({ text: line.slice(2), heading: HeadingLevel.HEADING_1 });
    } else if (line.startsWith("**") && line.endsWith("**") && line.length > 4) {
      return new Paragraph({ children: [new TextRun({ text: line.slice(2, -2), bold: true })] });
    } else if (line.startsWith("- ") || line.startsWith("* ")) {
      return new Paragraph({ text: line.slice(2), bullet: { level: 0 } });
    } else if (line.trim() === "") {
      return new Paragraph({ text: "" });
    } else {
      return new Paragraph({ text: line });
    }
  });
  const doc = new Document({ sections: [{ properties: {}, children }] });
  const blob = await Packer.toBlob(doc);
  const ab = await blob.arrayBuffer();
  return new Uint8Array(ab);
}

async function generatePdf(markdown: string): Promise<Uint8Array> {
  const { jsPDF } = await import("jspdf");
  const doc = new jsPDF({ unit: "pt", format: "a4" });
  const pageWidth = doc.internal.pageSize.getWidth();
  const margin = 60;
  const maxWidth = pageWidth - margin * 2;
  let y = 80;
  const lines = markdown.split("\n");
  for (const raw of lines) {
    const line = raw.trimEnd();
    if (y > doc.internal.pageSize.getHeight() - 60) {
      doc.addPage();
      y = 60;
    }
    if (line.startsWith("# ")) {
      doc.setFontSize(20).setFont("helvetica", "bold");
      doc.text(line.slice(2), margin, y); y += 28;
    } else if (line.startsWith("## ")) {
      doc.setFontSize(16).setFont("helvetica", "bold");
      doc.text(line.slice(3), margin, y); y += 22;
    } else if (line.startsWith("### ")) {
      doc.setFontSize(13).setFont("helvetica", "bold");
      doc.text(line.slice(4), margin, y); y += 18;
    } else if (line.startsWith("- ") || line.startsWith("* ")) {
      doc.setFontSize(11).setFont("helvetica", "normal");
      const wrapped = doc.splitTextToSize(`• ${line.slice(2)}`, maxWidth - 12);
      doc.text(wrapped, margin + 8, y); y += wrapped.length * 14 + 2;
    } else if (line.trim() === "") {
      y += 8;
    } else {
      doc.setFontSize(11).setFont("helvetica", "normal");
      const wrapped = doc.splitTextToSize(line, maxWidth);
      doc.text(wrapped, margin, y); y += wrapped.length * 14 + 2;
    }
  }
  return new Uint8Array(doc.output("arraybuffer") as ArrayBuffer);
}
```

- [ ] **Step 3: Build & smoke-test full pipeline**

Run: `npm run build` → expected pass.
Run: `npm run tauri dev`:
- Create a project, set a template, drop a short recording, press Run.
- Confirm the four stage cards progress (extract → transcribe → format → save with dialogs).
- Output preview shows the formatted result.
- Close the app, relaunch, select the same recording — state is still there.

- [ ] **Step 4: Test resume**

Stop Ollama mid-format (`pkill ollama` or `brew services stop ollama` in another terminal) during Run. Wait for format stage to fail. Restart Ollama. Hit Resume. Verify extract + transcribe stages skip (they stay green) and only format + save run.

- [ ] **Step 5: Commit**

```bash
git add src/components/StageCard.tsx src/components/RecordingView.tsx
git commit -m "frontend: recording view with stage cards, run/resume, save flow"
```

---

## Task 21: Invalidation warning banner on settings edits

**Files:**
- Modify: `src/components/RecordingView.tsx`

- [ ] **Step 1: Add a pure helper at the bottom of `src/components/RecordingView.tsx`**

```ts
function completedStageInvalidation(r: Recording): string | null {
  const affected: string[] = [];
  const stage = (k: StageKind) => r.stages.find((s) => s.stage === k);

  const t = stage("transcribe");
  if (t?.status === "completed") {
    const encoded = `${r.settings.whisperModel}@${r.settings.language}`;
    if (t.modelUsed !== encoded) affected.push("transcribe");
  }

  const f = stage("format");
  if (f?.status === "completed" && f.modelUsed !== r.settings.model) {
    affected.push("format");
  }

  const s = stage("save");
  if (s?.status === "completed") {
    const saved = (s.savedPaths ?? []).map((p) => p.split(".").pop() ?? "");
    const want = [...r.settings.outputFormats].sort().join(",");
    const have = [...new Set(saved)].sort().join(",");
    if (want !== have) affected.push("save");
  }

  if (affected.length === 0) return null;
  const downstream = affected.includes("transcribe")
    ? " (+ format, save)"
    : affected.includes("format") ? " (+ save)"
    : "";
  return `Changing settings will re-run: ${affected.join(", ")}${downstream} on next Run.`;
}
```

- [ ] **Step 2: Use the helper to show a yellow banner**

Replace the `<details>` block containing `<SettingsForm>` with this (keeps the same structure):

```tsx
<details className="bg-surface-secondary rounded-xl p-4">
  <summary className="cursor-pointer text-sm text-white/60">Recording settings</summary>
  <div className="mt-4 flex flex-col gap-3">
    {(() => {
      const warning = completedStageInvalidation(rec);
      return warning ? (
        <p className="text-xs bg-amber-500/10 border border-amber-500/20 text-amber-300 rounded px-3 py-2">
          {warning}
        </p>
      ) : null;
    })()}
    <SettingsForm
      settings={rec.settings}
      onChange={async (settings) => {
        const next = { ...rec, settings };
        setRec(next);
        await api.updateRecording(next);
        onRecordingChanged();
      }}
    />
  </div>
</details>
```

- [ ] **Step 3: Build & smoke-test**

Run: `npm run build` → expected pass.
Run: `npm run tauri dev` → complete a recording end-to-end. Open the Settings drawer, change whisper model. Yellow banner appears.

- [ ] **Step 4: Commit**

```bash
git add src/components/RecordingView.tsx
git commit -m "frontend: invalidation warning banner on recording settings edits"
```

---

## Task 22: Missing source file banner + cleanup legacy components

**Files:**
- Modify: `src/components/RecordingView.tsx`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Delete: `src/components/InputStep.tsx`, `src/components/SettingsStep.tsx`, `src/components/ProcessingStep.tsx`, `src/components/ResultStep.tsx`

- [ ] **Step 1: Add a Rust helper that checks whether a file exists**

At the bottom of `src-tauri/src/commands.rs`, add:

```rust
#[tauri::command]
pub fn path_exists(path: String) -> bool {
    std::path::Path::new(&path).exists()
}
```

Register in `src-tauri/src/lib.rs`'s `invoke_handler![…]`: `commands::path_exists,`.

- [ ] **Step 2: Use it in `RecordingView` to show a "file missing" banner with a "Locate…" button**

Add to the top of the RecordingView component, right after `useState` hooks:

```tsx
const [sourceMissing, setSourceMissing] = useState(false);

useEffect(() => {
  if (!rec) return;
  invoke<boolean>("path_exists", { path: rec.sourcePath })
    .then((ok) => setSourceMissing(!ok))
    .catch(() => setSourceMissing(false));
}, [rec?.sourcePath, rec?.id]);

const locateSource = async () => {
  if (!rec) return;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const path = await open({ multiple: false });
  if (typeof path === "string") {
    const next = { ...rec, sourcePath: path };
    setRec(next);
    await api.updateRecording(next);
    setSourceMissing(false);
    onRecordingChanged();
  }
};
```

And in the JSX, insert below the source-path paragraph:

```tsx
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

(Also add `import { invoke } from "@tauri-apps/api/core";` at the top if not present.)

- [ ] **Step 3: Delete legacy components**

```bash
rm src/components/InputStep.tsx src/components/SettingsStep.tsx src/components/ProcessingStep.tsx src/components/ResultStep.tsx
```

- [ ] **Step 4: Build & smoke-test**

Run: `npm run build`
Expected: tsc passes (no references to the deleted components remain — if any turn up, delete them).

Run: `npm run tauri dev`:
- Create a recording, complete it, then rename or move the source file in Finder. Click the recording — banner appears. Click Locate, pick the moved file. Banner goes away.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "frontend+backend: missing-source banner with Locate; drop legacy step components"
```

---

## Task 23: End-to-end verification and cleanup

**Files:**
- Modify: `src-tauri/src/commands.rs` (delete dead `cleanup_temp` body if unused)

- [ ] **Step 1: Test all critical paths manually**

Run: `npm run tauri dev` and walk through:
1. Launch empty → "Create your first project" card.
2. Create project "Test", set template, drop default settings.
3. Drop a short recording, run it end-to-end. Outputs saved.
4. Relaunch app. Project + recording persist. Output preview still shows transcript + formatted.
5. Create a second project. Drop a recording there. Sidebar shows both projects.
6. Delete a recording → confirm → sidebar updates; folder on disk gone (`ls ~/Library/Application\ Support/com.meister.dev/recordings/`).
7. Delete a project → confirm → sidebar updates.
8. Kill Ollama during a new recording's format stage. Stage goes red. Restart Ollama. Hit Resume. Format + save complete without re-transcribing.
9. Complete a recording. Change whisper model in its settings drawer — yellow banner. Hit Run — transcribe + format + save re-run; extract is skipped.
10. Start over button wipes stages; audio.wav is removed from disk (verify with `ls`).
11. Crash recovery: during a run, kill the app process with the transcribe stage mid-flight. Relaunch. The stage is marked `failed` with `"Interrupted (app closed during processing)"`. Hit Resume.

- [ ] **Step 2: Run full Rust test suite**

Run: `cd src-tauri && cargo test`
Expected: all pass.

- [ ] **Step 3: Drop the old `cleanup_temp` command**

The old path `temp_dir()/meister_audio.wav` is no longer written by the new orchestrator, but the `cleanup_temp` command is still registered and was called by the old `App.tsx`. No caller remains, so remove it.

In `src-tauri/src/commands.rs`, delete the `cleanup_temp` function.
In `src-tauri/src/lib.rs` `invoke_handler![…]`, remove the `commands::cleanup_temp,` line.

Run: `cd src-tauri && cargo check`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "commands: drop unused cleanup_temp after orchestrator takeover"
```

---

## Notes for the implementer

- **Connection pooling vs single mutex:** the plan uses one `Mutex<Connection>`. At this app's scale (one user, one app window, no concurrent pipeline runs) that's fine. If future features introduce concurrency (e.g. the "recording queue" listed in the spec's Future Work), swap to a connection pool (`r2d2-sqlite`).
- **`inputs_unchanged` for `language`:** the current encoding is `"<model>@<language>"` in `model_used`. It works for MVP. If we ever want to store more dimensions (e.g. temperature, prompt variant), split this into a dedicated JSON column `stage_inputs` on the `stages` table in a v2 migration.
- **Long-running `run_recording` and UI responsiveness:** the Rust command is `async`, so Tauri invokes it on the Tokio runtime. Emitted progress events keep the UI alive. Don't `spawn_blocking` the outer orchestrator — only the individual stage helpers do blocking work internally.
- **What about the `cleanup_temp` call in old `App.tsx`?** Deleted with the file. The new orchestrator manages `audio.wav` lifecycle under `recordings/<id>/`.
