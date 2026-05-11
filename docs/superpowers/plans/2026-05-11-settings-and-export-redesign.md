# Settings, Export & Ask Panel Redesign — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move all model/runtime settings to a global ⌘, settings window, replace the auto-saving pipeline `save` stage with on-demand ⌘E export, and reposition the Ask panel as a trailing-edge slide-in overlay.

**Architecture:** SQLite migration `v3 → v4` adds an `app_settings` singleton table, drops per-project/per-recording settings copies, and removes save-stage rows. The Rust pipeline stops at the `format` stage. The frontend reads settings via a new `useGlobalSettings` hook backed by Tauri commands. A secondary Tauri window (label `settings`) hosts the Preferences UI. Export is a client-side write triggered from the recording toolbar.

**Tech Stack:** Tauri 2, Rust (rusqlite), React + TypeScript, Vitest, existing `docx` and `jspdf` libraries.

**Spec:** `docs/superpowers/specs/2026-05-11-settings-and-export-redesign-design.md`.

**Working dir convention:** All paths below are relative to the repo root (`/Users/nexus/projects/meister`). Run all `cargo` commands from `src-tauri/`. Run all `npm`/`vitest` commands from the repo root.

---

## Task 1: SQLite migration v3 → v4 (additive, fails the old types)

**Files:**
- Modify: `src-tauri/src/storage/schema.rs`
- Modify: `src-tauri/tests/storage_integration.rs`

The migration creates the new `app_settings` table, adds `projects.language`, drops `projects.default_settings` and `recordings.settings`, and deletes save-stage rows. We seed `app_settings` from the most-recently-updated project before dropping its `default_settings` column. After this task, the Rust code will not compile — that is fixed in the next task.

- [ ] **Step 1: Write migration integration test**

Open `src-tauri/tests/storage_integration.rs`, find a representative existing test for shape, then add at the bottom:

```rust
#[test]
fn migration_v3_to_v4_seeds_app_settings_and_drops_legacy_columns() {
    use rusqlite::Connection;

    let conn = Connection::open_in_memory().expect("open db");
    conn.execute_batch("PRAGMA user_version = 3;").unwrap();
    // Materialize the v3 schema by running the migration framework once,
    // setting user_version back to 0 first so v0→v3 runs in order.
    conn.execute_batch("PRAGMA user_version = 0;").unwrap();
    meister::storage::schema::migrate(&conn).unwrap();

    // Force back to v3 so we can insert v3-shaped rows below, then re-run
    // up to v4. (v3 was the previous CURRENT_VERSION.)
    conn.execute_batch("PRAGMA user_version = 3;").unwrap();

    // Insert a project with default_settings JSON containing an outputFormats array.
    let default_settings = serde_json::json!({
        "ollamaUrl": "http://localhost:11434",
        "model": "gemma3",
        "whisperModel": "small",
        "language": "de",
        "outputFormats": ["docx", "pdf"]
    });
    conn.execute(
        "INSERT INTO projects \
         (id, name, created_at, updated_at, template_source, template_content, \
          template_source_path, default_settings, template_match_style, custom_instructions) \
         VALUES ('p1','Proj',1,100,'inline','','',?1,0,'')",
        rusqlite::params![default_settings.to_string()],
    ).unwrap();

    // Insert a recording with a (now-irrelevant) settings JSON.
    conn.execute(
        "INSERT INTO recordings \
         (id, project_id, name, source_paths, created_at, updated_at, settings) \
         VALUES ('r1','p1','Rec','[\"/x.mp3\"]',1,1,'{}')",
        [],
    ).unwrap();

    // Insert a save-stage row that the migration should delete.
    conn.execute(
        "INSERT INTO stages (recording_id, stage, status) VALUES ('r1','save','completed')",
        [],
    ).unwrap();

    // Run the migration framework forward to v4.
    meister::storage::schema::migrate(&conn).unwrap();

    let v: i32 = conn.query_row("PRAGMA user_version;", [], |r| r.get(0)).unwrap();
    assert_eq!(v, 4);

    // app_settings exists and has exactly one row seeded from the project.
    let (ollama, model, whisper, lang, fmt): (String, String, String, String, String) = conn
        .query_row(
            "SELECT ollama_url, model, whisper_model, language, default_export_format \
             FROM app_settings",
            [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        ).unwrap();
    assert_eq!(ollama, "http://localhost:11434");
    assert_eq!(model, "gemma3");
    assert_eq!(whisper, "small");
    assert_eq!(lang, "de");
    assert_eq!(fmt, "docx");  // first entry of outputFormats

    // projects has new `language` column.
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(projects);").unwrap()
        .query_map([], |r| r.get::<_, String>(1)).unwrap()
        .filter_map(Result::ok).collect();
    assert!(cols.iter().any(|c| c == "language"));
    assert!(!cols.iter().any(|c| c == "default_settings"));

    // recordings dropped settings.
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(recordings);").unwrap()
        .query_map([], |r| r.get::<_, String>(1)).unwrap()
        .filter_map(Result::ok).collect();
    assert!(!cols.iter().any(|c| c == "settings"));

    // save-stage rows are gone.
    let n: i64 = conn
        .query_row("SELECT count(*) FROM stages WHERE stage = 'save'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);
}

#[test]
fn migration_v3_to_v4_seeds_defaults_when_no_projects() {
    use rusqlite::Connection;
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA user_version = 0;").unwrap();
    meister::storage::schema::migrate(&conn).unwrap();

    let (ollama, whisper, lang, fmt): (String, String, String, String) = conn
        .query_row(
            "SELECT ollama_url, whisper_model, language, default_export_format \
             FROM app_settings",
            [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        ).unwrap();
    assert_eq!(ollama, "http://localhost:11434");
    assert_eq!(whisper, "base");
    assert_eq!(lang, "auto");
    assert_eq!(fmt, "md");
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd src-tauri && cargo test --test storage_integration migration_v3_to_v4 -- --nocapture
```

Expected: FAIL with "no migration from version 3" (the framework caps at 3).

- [ ] **Step 3: Implement the migration**

Edit `src-tauri/src/storage/schema.rs`:

```rust
const CURRENT_VERSION: i32 = 4;
```

Inside the `migrate` `while` loop's `match`, add the `3` arm:

```rust
3 => migrate_v3_to_v4(conn)?,
```

Add this function at the bottom of the file:

```rust
fn migrate_v3_to_v4(conn: &Connection) -> Result<(), String> {
    let tx_started = conn
        .execute_batch("BEGIN;")
        .map_err(|e| format!("begin v4: {e}"))?;
    let _ = tx_started;

    // 1) Singleton app_settings table.
    conn.execute_batch(
        r#"
        CREATE TABLE app_settings (
            singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
            ollama_url TEXT NOT NULL,
            model TEXT NOT NULL,
            whisper_model TEXT NOT NULL,
            language TEXT NOT NULL,
            default_export_format TEXT NOT NULL
        );
        "#,
    ).map_err(|e| format!("create app_settings: {e}"))?;

    // 2) Seed from the most-recently-updated project, if any.
    let seed: Option<String> = conn
        .query_row(
            "SELECT default_settings FROM projects ORDER BY updated_at DESC LIMIT 1",
            [],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| format!("read seed project: {e}"))?;

    let (ollama, model, whisper, language, fmt) = match seed {
        Some(json) => {
            let v: serde_json::Value = serde_json::from_str(&json)
                .map_err(|e| format!("parse seed settings: {e}"))?;
            let s = |k: &str, default: &str| {
                v.get(k).and_then(|x| x.as_str()).unwrap_or(default).to_string()
            };
            let formats = v.get("outputFormats").and_then(|x| x.as_array());
            let fmt = formats
                .and_then(|arr| arr.first())
                .and_then(|x| x.as_str())
                .unwrap_or("md")
                .to_string();
            (
                s("ollamaUrl", "http://localhost:11434"),
                s("model", ""),
                s("whisperModel", "base"),
                s("language", "auto"),
                fmt,
            )
        }
        None => (
            "http://localhost:11434".to_string(),
            String::new(),
            "base".to_string(),
            "auto".to_string(),
            "md".to_string(),
        ),
    };

    conn.execute(
        "INSERT INTO app_settings \
         (singleton, ollama_url, model, whisper_model, language, default_export_format) \
         VALUES (1, ?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![ollama, model, whisper, language, fmt],
    ).map_err(|e| format!("seed app_settings: {e}"))?;

    // 3) Add language to projects, drop default_settings.
    conn.execute_batch(
        r#"
        ALTER TABLE projects ADD COLUMN language TEXT;
        ALTER TABLE projects DROP COLUMN default_settings;
        ALTER TABLE recordings DROP COLUMN settings;
        DELETE FROM stages WHERE stage = 'save';
        "#,
    ).map_err(|e| format!("apply v4 column changes: {e}"))?;

    conn.execute_batch("COMMIT;")
        .map_err(|e| format!("commit v4: {e}"))?;
    Ok(())
}
```

Add the `OptionalExtension` use at the top of the file (replace the existing `use rusqlite::Connection;`):

```rust
use rusqlite::{Connection, OptionalExtension};
```

- [ ] **Step 4: Run migration tests**

```bash
cd src-tauri && cargo test --test storage_integration migration_v3_to_v4 -- --nocapture
```

Expected: both new tests PASS. The wider test suite and `cargo check` will FAIL because types/CRUD still reference dropped columns — that is fixed in Task 2. Do NOT run the full suite yet.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/storage/schema.rs src-tauri/tests/storage_integration.rs
git commit -m "feat(storage): v3→v4 migration — app_settings table, drop legacy columns"
```

---

## Task 2: Rust types — `GlobalSettings`, drop `AppSettings`, remove `StageKind::Save`

**Files:**
- Modify: `src-tauri/src/storage/types.rs`

After this task the crate still won't compile (CRUD layers still reference the old fields). Tasks 3-7 fix the rest.

- [ ] **Step 1: Rewrite types.rs**

Replace the contents of `src-tauri/src/storage/types.rs` with:

```rust
//! Data types shared by storage, orchestrator, and Tauri commands.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Md,
    Txt,
    Docx,
    Pdf,
}

impl OutputFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            OutputFormat::Md => "md",
            OutputFormat::Txt => "txt",
            OutputFormat::Docx => "docx",
            OutputFormat::Pdf => "pdf",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "md" => Some(OutputFormat::Md),
            "txt" => Some(OutputFormat::Txt),
            "docx" => Some(OutputFormat::Docx),
            "pdf" => Some(OutputFormat::Pdf),
            _ => None,
        }
    }
}

/// App-wide settings. Persisted in the singleton `app_settings` table.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GlobalSettings {
    #[serde(rename = "ollamaUrl")]
    pub ollama_url: String,
    pub model: String,
    #[serde(rename = "whisperModel")]
    pub whisper_model: String,
    pub language: String,
    #[serde(rename = "defaultExportFormat")]
    pub default_export_format: OutputFormat,
}

/// The effective settings the orchestrator runs a recording with, computed
/// from `(GlobalSettings, Project.language)` at run time.
#[derive(Debug, Clone)]
pub struct EffectiveSettings {
    pub ollama_url: String,
    pub model: String,
    pub whisper_model: String,
    pub language: String,
}

impl EffectiveSettings {
    pub fn from(global: &GlobalSettings, project_language: Option<&str>) -> Self {
        let language = project_language
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&global.language)
            .to_string();
        Self {
            ollama_url: global.ollama_url.clone(),
            model: global.model.clone(),
            whisper_model: global.whisper_model.clone(),
            language,
        }
    }
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
    #[serde(rename = "matchStyle", default, skip_serializing_if = "is_false")]
    pub match_style: bool,
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
    #[serde(rename = "customInstructions", default, skip_serializing_if = "String::is_empty")]
    pub custom_instructions: String,
    /// Optional per-project language override. `None`/empty means "use the
    /// global default language".
    #[serde(default)]
    pub language: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum StageKind {
    Extract,
    Transcribe,
    Format,
}

impl StageKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            StageKind::Extract => "extract",
            StageKind::Transcribe => "transcribe",
            StageKind::Format => "format",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "extract" => Some(StageKind::Extract),
            "transcribe" => Some(StageKind::Transcribe),
            "format" => Some(StageKind::Format),
            _ => None,
        }
    }

    pub const ALL: [StageKind; 3] = [
        StageKind::Extract,
        StageKind::Transcribe,
        StageKind::Format,
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
}

fn is_false(b: &bool) -> bool { !*b }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recording {
    pub id: String,
    #[serde(rename = "projectId")]
    pub project_id: String,
    pub name: String,
    #[serde(rename = "sourcePaths")]
    pub source_paths: Vec<String>,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
    pub stages: Vec<StageRecord>,
}
```

- [ ] **Step 2: Commit**

Cargo check will fail across the workspace — that is expected at this checkpoint; do not run it.

```bash
git add src-tauri/src/storage/types.rs
git commit -m "refactor(storage): introduce GlobalSettings, drop AppSettings, remove Save stage"
```

---

## Task 3: `app_settings` storage module + projects CRUD update

**Files:**
- Create: `src-tauri/src/storage/app_settings.rs`
- Modify: `src-tauri/src/storage/mod.rs`
- Modify: `src-tauri/src/storage/projects.rs`

- [ ] **Step 1: Create the app_settings module**

Write `src-tauri/src/storage/app_settings.rs`:

```rust
//! Singleton app-wide settings. Always exactly one row.

use crate::storage::{GlobalSettings, OutputFormat, Store};
use rusqlite::params;

pub fn get(store: &Store) -> Result<GlobalSettings, String> {
    let conn = store.conn.lock().unwrap();
    conn.query_row(
        "SELECT ollama_url, model, whisper_model, language, default_export_format \
         FROM app_settings WHERE singleton = 1",
        [],
        |r| {
            let fmt: String = r.get(4)?;
            Ok(GlobalSettings {
                ollama_url: r.get(0)?,
                model: r.get(1)?,
                whisper_model: r.get(2)?,
                language: r.get(3)?,
                default_export_format: OutputFormat::from_str(&fmt)
                    .unwrap_or(OutputFormat::Md),
            })
        },
    )
    .map_err(|e| format!("read app_settings: {e}"))
}

pub fn set(store: &Store, s: &GlobalSettings) -> Result<(), String> {
    let conn = store.conn.lock().unwrap();
    conn.execute(
        "UPDATE app_settings SET \
         ollama_url = ?1, model = ?2, whisper_model = ?3, language = ?4, \
         default_export_format = ?5 \
         WHERE singleton = 1",
        params![
            s.ollama_url, s.model, s.whisper_model, s.language,
            s.default_export_format.as_str(),
        ],
    )
    .map_err(|e| format!("update app_settings: {e}"))?;
    Ok(())
}
```

- [ ] **Step 2: Register the module**

Edit `src-tauri/src/storage/mod.rs` — add `pub mod app_settings;` next to the other module declarations (after `pub mod hash;`).

- [ ] **Step 3: Rewrite `projects.rs` for the new schema**

Replace `src-tauri/src/storage/projects.rs` with:

```rust
//! Project CRUD.

use crate::storage::{Project, Store, Template, TemplateSource};
use rusqlite::params;

pub fn insert(store: &Store, p: &Project) -> Result<(), String> {
    let conn = store.conn.lock().unwrap();
    conn.execute(
        "INSERT INTO projects \
         (id, name, created_at, updated_at, template_source, template_content, \
          template_source_path, template_match_style, custom_instructions, language) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            p.id, p.name, p.created_at, p.updated_at,
            p.template.source.as_str(),
            p.template.content,
            p.template.source_path,
            p.template.match_style as i32,
            p.custom_instructions,
            p.language,
        ],
    )
    .map_err(|e| format!("insert project: {e}"))?;
    Ok(())
}

pub fn update(store: &Store, p: &Project) -> Result<(), String> {
    let conn = store.conn.lock().unwrap();
    let n = conn.execute(
        "UPDATE projects SET \
         name = ?2, updated_at = ?3, template_source = ?4, template_content = ?5, \
         template_source_path = ?6, template_match_style = ?7, \
         custom_instructions = ?8, language = ?9 \
         WHERE id = ?1",
        params![
            p.id, p.name, p.updated_at,
            p.template.source.as_str(),
            p.template.content,
            p.template.source_path,
            p.template.match_style as i32,
            p.custom_instructions,
            p.language,
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
             template_content, template_source_path, template_match_style, \
             custom_instructions, language \
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
             template_content, template_source_path, template_match_style, \
             custom_instructions, language \
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
    let source_str: String = row.get(4).map_err(|e| format!("source: {e}"))?;
    let source = match source_str.as_str() {
        "inline" => TemplateSource::Inline,
        "file" => TemplateSource::File,
        other => return Err(format!("unknown template_source: {other}")),
    };
    let match_style_int: i64 = row.get(7).map_err(|e| format!("match_style: {e}"))?;
    let custom_instructions: String = row.get(8).map_err(|e| format!("custom_instructions: {e}"))?;
    let language: Option<String> = row.get(9).map_err(|e| format!("language: {e}"))?;
    Ok(Project {
        id: row.get(0).map_err(|e| format!("id: {e}"))?,
        name: row.get(1).map_err(|e| format!("name: {e}"))?,
        created_at: row.get(2).map_err(|e| format!("created_at: {e}"))?,
        updated_at: row.get(3).map_err(|e| format!("updated_at: {e}"))?,
        template: Template {
            source,
            content: row.get(5).map_err(|e| format!("content: {e}"))?,
            source_path: row.get(6).map_err(|e| format!("source_path: {e}"))?,
            match_style: match_style_int != 0,
        },
        custom_instructions,
        language,
    })
}

impl TemplateSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            TemplateSource::Inline => "inline",
            TemplateSource::File => "file",
        }
    }
}
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/storage/app_settings.rs src-tauri/src/storage/mod.rs src-tauri/src/storage/projects.rs
git commit -m "feat(storage): app_settings module + projects CRUD with language"
```

---

## Task 4: Recordings + stages CRUD (drop settings, drop Save)

**Files:**
- Modify: `src-tauri/src/storage/recordings.rs`
- Modify: `src-tauri/src/storage/stages.rs`

- [ ] **Step 1: Rewrite `recordings.rs`**

Replace contents with:

```rust
//! Recording CRUD. On insert, three `pending` stage rows are created so every
//! recording always has a complete 3-row stage set.

use crate::storage::{Recording, StageKind, StageRecord, StageStatus, Store};
use rusqlite::params;

pub fn insert(store: &Store, r: &Recording) -> Result<(), String> {
    let conn = store.conn.lock().unwrap();
    let source_paths_json = serde_json::to_string(&r.source_paths)
        .map_err(|e| format!("serialize source_paths: {e}"))?;
    conn.execute(
        "INSERT INTO recordings \
         (id, project_id, name, source_paths, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            r.id, r.project_id, r.name, source_paths_json,
            r.created_at, r.updated_at,
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
    let source_paths_json = serde_json::to_string(&r.source_paths)
        .map_err(|e| format!("serialize source_paths: {e}"))?;
    let n = conn.execute(
        "UPDATE recordings SET name = ?2, source_paths = ?3, updated_at = ?4 \
         WHERE id = ?1",
        params![r.id, r.name, source_paths_json, r.updated_at],
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
            "SELECT id, project_id, name, source_paths, created_at, updated_at \
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
            "SELECT id, project_id, name, source_paths, created_at, updated_at \
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
    let source_paths_json: String = row.get(3).map_err(|e| format!("source_paths: {e}"))?;
    let source_paths: Vec<String> = serde_json::from_str(&source_paths_json)
        .map_err(|e| format!("parse source_paths: {e}"))?;
    Ok(Recording {
        id: row.get(0).map_err(|e| format!("id: {e}"))?,
        project_id: row.get(1).map_err(|e| format!("project_id: {e}"))?,
        name: row.get(2).map_err(|e| format!("name: {e}"))?,
        source_paths,
        created_at: row.get(4).map_err(|e| format!("created_at: {e}"))?,
        updated_at: row.get(5).map_err(|e| format!("updated_at: {e}"))?,
        stages: vec![],
    })
}

fn load_stages(
    conn: &rusqlite::Connection,
    recording_id: &str,
) -> Result<Vec<StageRecord>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT stage, status, error, completed_at, model_used, template_hash, output \
             FROM stages WHERE recording_id = ?1",
        )
        .map_err(|e| format!("prepare stages: {e}"))?;
    let rows = stmt
        .query_map(params![recording_id], |row| Ok(row_to_stage(row)))
        .map_err(|e| format!("query_map stages: {e}"))?;
    let mut by_kind: std::collections::HashMap<StageKind, StageRecord> =
        std::collections::HashMap::new();
    for r in rows {
        let Ok(Ok(rec)) = r else { continue };
        by_kind.insert(rec.stage, rec);
    }
    let mut out = Vec::with_capacity(3);
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
            });
        }
    }
    Ok(out)
}

fn row_to_stage(row: &rusqlite::Row) -> Result<StageRecord, String> {
    let stage_str: String = row.get(0).map_err(|e| format!("stage: {e}"))?;
    let status_str: String = row.get(1).map_err(|e| format!("status: {e}"))?;
    let Some(stage) = StageKind::from_str(&stage_str) else {
        // Defensive: an unknown stage (e.g. legacy "save" if migration was skipped
        // somehow) is treated as a missing row by `load_stages`. We bubble it as
        // an error so callers can see the data is corrupt.
        return Err(format!("unknown stage: {stage_str}"));
    };
    Ok(StageRecord {
        stage,
        status: StageStatus::from_str(&status_str)
            .ok_or_else(|| format!("unknown status: {status_str}"))?,
        error: row.get(2).map_err(|e| format!("error: {e}"))?,
        completed_at: row.get(3).map_err(|e| format!("completed_at: {e}"))?,
        model_used: row.get(4).map_err(|e| format!("model_used: {e}"))?,
        template_hash: row.get(5).map_err(|e| format!("template_hash: {e}"))?,
        output: row.get(6).map_err(|e| format!("output: {e}"))?,
    })
}
```

Note: the `saved_paths` column on `stages` still exists in the DB but is no longer read or written.

- [ ] **Step 2: Rewrite `stages.rs`**

Replace the file with this trimmed version (no `Save` references, no `saved_paths` in `CompletedOutput`):

```rust
//! Stage-level updates. Each function is a single UPDATE, so every row
//! transition is atomic.

use crate::storage::{StageKind, StageStatus, Store};
use rusqlite::params;

pub struct CompletedOutput {
    pub output: Option<String>,
    pub model_used: Option<String>,
    pub template_hash: Option<String>,
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
    let conn = store.conn.lock().unwrap();
    conn.execute(
        "UPDATE stages SET \
         status = ?3, error = NULL, completed_at = ?4, model_used = ?5, \
         template_hash = ?6, output = ?7 \
         WHERE recording_id = ?1 AND stage = ?2",
        params![
            recording_id,
            stage.as_str(),
            StageStatus::Completed.as_str(),
            out.completed_at,
            out.model_used,
            out.template_hash,
            out.output,
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
         template_hash = NULL, output = NULL \
         WHERE recording_id = ?1 AND stage IN ({})",
        placeholders.join(",")
    );
    let conn = store.conn.lock().unwrap();
    let mut stmt = conn.prepare(&sql).map_err(|e| format!("prepare: {e}"))?;
    let mut p: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(later.len() + 1);
    p.push(&recording_id);
    for s in &later { p.push(s); }
    stmt.execute(rusqlite::params_from_iter(p.iter()))
        .map_err(|e| format!("reset_downstream: {e}"))?;
    Ok(())
}

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
         template_hash = NULL, output = NULL \
         WHERE recording_id = ?1 AND stage IN ({})",
        placeholders.join(",")
    );
    let conn = store.conn.lock().unwrap();
    let mut stmt = conn.prepare(&sql).map_err(|e| format!("prepare: {e}"))?;
    let mut p: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(kinds.len() + 1);
    p.push(&recording_id);
    for s in &kinds { p.push(s); }
    stmt.execute(rusqlite::params_from_iter(p.iter()))
        .map_err(|e| format!("reset_from: {e}"))?;
    Ok(())
}

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

- [ ] **Step 2: Commit**

```bash
git add src-tauri/src/storage/recordings.rs src-tauri/src/storage/stages.rs
git commit -m "refactor(storage): drop Recording.settings and saved_paths plumbing"
```

---

## Task 5: Orchestrator update (drop Save arm, use EffectiveSettings)

**Files:**
- Modify: `src-tauri/src/orchestrator.rs`

The orchestrator currently reads `rec.settings` everywhere. It needs to resolve `EffectiveSettings` once per run from `(app_settings::get(...), project.language)` and substitute. Save-stage handling is deleted.

- [ ] **Step 1: Read the file fully and patch**

Open `src-tauri/src/orchestrator.rs` and apply these edits:

1. **At the top of `run_recording` (or wherever the recording is loaded)**, after fetching the `Recording` and its `Project`, fetch global settings and compute effective settings:

```rust
let global = storage::app_settings::get(&store)?;
let effective = storage::EffectiveSettings::from(&global, project.language.as_deref());
```

2. **Replace every `rec.settings.X` reference inside the run loop with `effective.X`**:
   - `rec.settings.ollama_url` → `effective.ollama_url`
   - `rec.settings.model` → `effective.model`
   - `rec.settings.whisper_model` → `effective.whisper_model`
   - `rec.settings.language` → `effective.language`

3. **Delete the `StageKind::Save` arm** in the run loop (around line 294, the comment block ending with `Ok(storage::stages::CompletedOutput { ... saved_paths: Some(Vec::new()), ... })`). The whole `StageKind::Save => { ... }` block is removed.

4. **Update `CompletedOutput` constructors** — remove the `saved_paths: ...` field from every literal (it no longer exists on the struct).

5. **Update `inputs_unchanged`** if it takes/returns anything about save — search for `Save`, `saved_paths`, and remove those branches. In the unit tests at the bottom of the file (the `Save` test cases around lines 405-421), delete those test cases entirely.

6. **Update any tests referencing `rec.settings`** in this file — substitute a fixture `EffectiveSettings` value where needed. Look for helper functions `settings()` and `completed_stage(StageKind::Save)` and delete the latter; adapt the former if it returns `AppSettings` (change it to return `GlobalSettings` or `EffectiveSettings` as appropriate to the call site).

- [ ] **Step 2: Run cargo check**

```bash
cd src-tauri && cargo check
```

Expected: PASS. If compilation fails, look at the error: missing imports (`use crate::storage::EffectiveSettings;`), stragglers referencing dropped fields, or signature mismatches. Fix each, re-run.

- [ ] **Step 3: Run the orchestrator unit tests**

```bash
cd src-tauri && cargo test --lib orchestrator
```

Expected: PASS. Failures here mean the test fixtures still reference old types — clean those up.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/orchestrator.rs
git commit -m "feat(orchestrator): use EffectiveSettings, drop Save stage"
```

---

## Task 6: Commands — add `get_global_settings`, `set_global_settings`, `open_settings_window`; remove `record_saved_outputs`

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Patch `commands.rs`**

Make these specific edits:

1. **Remove `record_saved_outputs` and `run_save_outputs`** — delete the two functions (around lines 1372 and 1380) entirely.

2. **Replace any signatures that took `AppSettings`** — the most likely callers are `create_project` and `create_recording`. Adjust each so they no longer take `AppSettings`:

```rust
// Old shape:
// pub async fn create_project(name: String, default_settings: AppSettings, template: Template, ...)
// New:
#[tauri::command]
pub async fn create_project(
    name: String,
    template: storage::Template,
    state: tauri::State<'_, std::sync::Arc<storage::Store>>,
) -> Result<storage::Project, String> {
    let now = now_ms();
    let id = uuid::Uuid::new_v4().to_string();
    let project = storage::Project {
        id, name, created_at: now, updated_at: now,
        template, custom_instructions: String::new(),
        language: None,
    };
    let store = state.inner().clone();
    let p = project.clone();
    tokio::task::spawn_blocking(move || storage::projects::insert(&store, &p))
        .await.map_err(|e| e.to_string())??;
    Ok(project)
}
```

(Adapt to the exact `now_ms()` and state-shape conventions already in `commands.rs`. Keep the existing import for `uuid::Uuid` if it's already there; otherwise leave UUID generation to whatever helper is already in use.)

For `create_recording`, drop the `settings: AppSettings` parameter:

```rust
#[tauri::command]
pub async fn create_recording(
    project_id: String,
    name: String,
    source_paths: Vec<String>,
    state: tauri::State<'_, std::sync::Arc<storage::Store>>,
) -> Result<storage::Recording, String> {
    // ... unchanged body except: no settings field on Recording.
}
```

3. **Add the new commands** somewhere near the bottom of the file (alongside other settings-related commands):

```rust
#[tauri::command]
pub async fn get_global_settings(
    state: tauri::State<'_, std::sync::Arc<storage::Store>>,
) -> Result<storage::GlobalSettings, String> {
    let store = state.inner().clone();
    tokio::task::spawn_blocking(move || storage::app_settings::get(&store))
        .await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn set_global_settings(
    settings: storage::GlobalSettings,
    state: tauri::State<'_, std::sync::Arc<storage::Store>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let store = state.inner().clone();
    tokio::task::spawn_blocking(move || storage::app_settings::set(&store, &settings))
        .await.map_err(|e| e.to_string())??;
    // Notify all windows so subscribers refresh.
    let _ = app.emit("global-settings-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn open_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window("settings") {
        w.show().map_err(|e| e.to_string())?;
        w.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }
    // Window is registered in tauri.conf.json with visible=false; if absent
    // (e.g. closed permanently), surface a clear error rather than re-creating
    // it here — config-driven creation keeps the runtime simple.
    Err("settings window not found".to_string())
}
```

Add `use tauri::Emitter;` at the top of `commands.rs` if it isn't already imported.

- [ ] **Step 2: Patch `lib.rs`**

In the `invoke_handler!` list (around line 144), make these changes:

- Remove `commands::record_saved_outputs,`
- Add `commands::get_global_settings,`
- Add `commands::set_global_settings,`
- Add `commands::open_settings_window,`

- [ ] **Step 3: cargo check**

```bash
cd src-tauri && cargo check
```

Expected: PASS. Compile errors usually mean a frontend-shaped command signature still expects `AppSettings` somewhere — search for `AppSettings` and clean up.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat(commands): get/set/open global settings; drop record_saved_outputs"
```

---

## Task 7: Tauri config — register settings window; menu wiring (Settings, Export)

**Files:**
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/src/menu.rs`

- [ ] **Step 1: Add the settings window to `tauri.conf.json`**

In `app.windows`, append a second window entry next to the existing one:

```json
{
  "label": "settings",
  "title": "Settings",
  "url": "index.html",
  "width": 520,
  "height": 640,
  "minWidth": 480,
  "minHeight": 500,
  "resizable": true,
  "fullscreen": false,
  "decorations": true,
  "transparent": true,
  "titleBarStyle": "Overlay",
  "visible": false
}
```

The main window has no `label`, so Tauri assigns it `main` automatically. The settings window uses the same `index.html` — `main.tsx` branches on `getCurrentWindow().label` (done in Task 11).

- [ ] **Step 2: Patch `src-tauri/src/menu.rs`**

Find the `let preferences = ... .enabled(false) ...` block and replace it with an enabled item with the new id:

```rust
let preferences = MenuItemBuilder::new("Settings…")
    .id("menu:settings")
    .accelerator("CmdOrCtrl+,")
    .build(app)?;
```

Find the `File` submenu construction and add an `Export…` item:

```rust
let export = MenuItemBuilder::new("Export…")
    .id("menu:export")
    .accelerator("CmdOrCtrl+E")
    .enabled(true)
    .build(app)?;
```

Insert `&export` into the `file_submenu` chain after `&import_audio`:

```rust
let file_submenu = SubmenuBuilder::new(app, "File")
    .item(&new_project)
    .item(&import_audio)
    .separator()
    .item(&export)
    .separator()
    .item(&PredefinedMenuItem::close_window(app, None)?)
    .build()?;
```

- [ ] **Step 3: cargo check + run integration tests**

```bash
cd src-tauri && cargo check && cargo test
```

Expected: PASS across the whole suite. The migration test from Task 1 should still pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/tauri.conf.json src-tauri/src/menu.rs
git commit -m "feat(menu,config): settings window registration; ⌘, and ⌘E items"
```

---

## Task 8: Frontend types — `GlobalSettings`, drop `AppSettings`, update `Recording`/`Project`/`StageKind`

**Files:**
- Modify: `src/types/storage.ts`

- [ ] **Step 1: Rewrite the type module**

Replace contents of `src/types/storage.ts` with:

```ts
export type OutputFormat = "md" | "txt" | "docx" | "pdf";

export interface GlobalSettings {
  ollamaUrl: string;
  model: string;
  whisperModel: string;
  language: string;
  defaultExportFormat: OutputFormat;
}

export type TemplateSource = "inline" | "file";

export interface Template {
  source: TemplateSource;
  content: string;
  sourcePath?: string | null;
  matchStyle?: boolean;
}

export interface Project {
  id: string;
  name: string;
  createdAt: number;
  updatedAt: number;
  template: Template;
  customInstructions?: string;
  language?: string | null;
}

export type StageKind = "extract" | "transcribe" | "format";
export type StageStatus = "pending" | "running" | "completed" | "failed";

export interface StageRecord {
  stage: StageKind;
  status: StageStatus;
  error?: string | null;
  completedAt?: number | null;
  modelUsed?: string | null;
  templateHash?: string | null;
  output?: string | null;
}

export interface Recording {
  id: string;
  projectId: string;
  name: string;
  sourcePaths: string[];
  createdAt: number;
  updatedAt: number;
  stages: StageRecord[];
}

export const STAGES: StageKind[] = ["extract", "transcribe", "format"];

export function overallStatus(r: Recording): "new" | "in_progress" | "completed" | "failed" {
  if (r.stages.some((s) => s.status === "failed")) return "failed";
  if (r.stages.some((s) => s.status === "running")) return "in_progress";
  if (r.stages.every((s) => s.status === "completed")) return "completed";
  if (r.stages.some((s) => s.status !== "pending")) return "in_progress";
  return "new";
}
```

- [ ] **Step 2: Commit**

The frontend will not type-check after this — that is fixed across the next several tasks.

```bash
git add src/types/storage.ts
git commit -m "refactor(types): GlobalSettings, drop AppSettings/savedPaths/save stage"
```

---

## Task 9: Frontend API client — global settings commands, drop `recordSavedOutputs`

**Files:**
- Modify: `src/api/storage.ts`

- [ ] **Step 1: Patch the API file**

Open `src/api/storage.ts` and apply:

1. **Update imports**:

```ts
import type { GlobalSettings, Project, Recording, Template } from "../types/storage";
```

(Drop `AppSettings`.)

2. **Update `createProject` and `createRecording` signatures** — drop the `defaultSettings` and `settings` parameters:

```ts
createProject: (name: string, template: Template) =>
  invoke<Project>("create_project", { name, template }),

createRecording: (
  projectId: string,
  name: string,
  sourcePaths: string[],
) => invoke<Recording>("create_recording", { projectId, name, sourcePaths }),
```

3. **Remove** the `recordSavedOutputs` entry entirely.

4. **Add** these new entries to the `api` object:

```ts
getGlobalSettings: () => invoke<GlobalSettings>("get_global_settings"),

setGlobalSettings: (settings: GlobalSettings) =>
  invoke<void>("set_global_settings", { settings }),

openSettingsWindow: () => invoke<void>("open_settings_window"),
```

- [ ] **Step 2: Commit**

```bash
git add src/api/storage.ts
git commit -m "feat(api): get/set/open global settings; drop recordSavedOutputs"
```

---

## Task 10: `useGlobalSettings` hook + tests

**Files:**
- Create: `src/lib/useGlobalSettings.ts`
- Create: `src/lib/useGlobalSettings.test.ts`

Module-scoped store so every consumer in the same window sees the same snapshot. The store hydrates lazily on first `useGlobalSettings()`, and refreshes on the `global-settings-changed` event emitted by the backend.

- [ ] **Step 1: Write the test first**

Write `src/lib/useGlobalSettings.test.ts`:

```ts
import { describe, expect, it, vi, beforeEach } from "vitest";
import { renderHook, act, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
const listenMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: (...args: unknown[]) => listenMock(...args),
}));

import { useGlobalSettings, __resetGlobalSettingsForTests } from "./useGlobalSettings";

const SAMPLE = {
  ollamaUrl: "http://localhost:11434",
  model: "gemma3",
  whisperModel: "base",
  language: "auto",
  defaultExportFormat: "md" as const,
};

beforeEach(() => {
  invokeMock.mockReset();
  listenMock.mockReset();
  listenMock.mockResolvedValue(() => {});
  __resetGlobalSettingsForTests();
});

describe("useGlobalSettings", () => {
  it("hydrates from get_global_settings on first mount", async () => {
    invokeMock.mockImplementation((name: string) => {
      if (name === "get_global_settings") return Promise.resolve(SAMPLE);
      return Promise.resolve();
    });
    const { result } = renderHook(() => useGlobalSettings());
    await waitFor(() => expect(result.current.settings).toEqual(SAMPLE));
  });

  it("update() calls set_global_settings and refreshes locally", async () => {
    invokeMock.mockImplementation((name: string, args?: unknown) => {
      if (name === "get_global_settings") return Promise.resolve(SAMPLE);
      if (name === "set_global_settings") {
        return Promise.resolve();
      }
      return Promise.resolve();
    });
    const { result } = renderHook(() => useGlobalSettings());
    await waitFor(() => expect(result.current.settings).toEqual(SAMPLE));

    await act(async () => {
      await result.current.update({ ...SAMPLE, model: "qwen3" });
    });

    expect(invokeMock).toHaveBeenCalledWith("set_global_settings", {
      settings: { ...SAMPLE, model: "qwen3" },
    });
    expect(result.current.settings).toEqual({ ...SAMPLE, model: "qwen3" });
  });

  it("two consumers see the same snapshot", async () => {
    invokeMock.mockResolvedValue(SAMPLE);
    const a = renderHook(() => useGlobalSettings());
    const b = renderHook(() => useGlobalSettings());
    await waitFor(() => expect(a.result.current.settings).toEqual(SAMPLE));
    expect(b.result.current.settings).toEqual(SAMPLE);
    expect(invokeMock).toHaveBeenCalledTimes(1);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

```bash
npx vitest run src/lib/useGlobalSettings.test.ts
```

Expected: FAIL — module does not exist.

- [ ] **Step 3: Implement the hook**

Write `src/lib/useGlobalSettings.ts`:

```ts
import { useEffect, useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { GlobalSettings } from "../types/storage";

type Listener = () => void;
const listeners = new Set<Listener>();
let snapshot: GlobalSettings | null = null;
let hydrating: Promise<void> | null = null;
let unlisten: UnlistenFn | null = null;

function emit() {
  for (const l of listeners) l();
}

async function hydrate() {
  if (hydrating) return hydrating;
  hydrating = (async () => {
    snapshot = await invoke<GlobalSettings>("get_global_settings");
    emit();
    unlisten = await listen("global-settings-changed", async () => {
      snapshot = await invoke<GlobalSettings>("get_global_settings");
      emit();
    });
  })();
  return hydrating;
}

function subscribe(l: Listener): () => void {
  listeners.add(l);
  return () => { listeners.delete(l); };
}

function getSnapshot(): GlobalSettings | null {
  return snapshot;
}

export function useGlobalSettings(): {
  settings: GlobalSettings | null;
  update: (next: GlobalSettings) => Promise<void>;
} {
  const settings = useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
  useEffect(() => { void hydrate(); }, []);
  const update = async (next: GlobalSettings) => {
    await invoke("set_global_settings", { settings: next });
    snapshot = next;
    emit();
  };
  return { settings, update };
}

/** Test-only: reset module state between tests. */
export function __resetGlobalSettingsForTests() {
  listeners.clear();
  snapshot = null;
  hydrating = null;
  if (unlisten) { unlisten(); unlisten = null; }
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
npx vitest run src/lib/useGlobalSettings.test.ts
```

Expected: all 3 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/lib/useGlobalSettings.ts src/lib/useGlobalSettings.test.ts
git commit -m "feat(lib): useGlobalSettings hook + module store"
```

---

## Task 11: Reshape `SettingsForm` to consume `GlobalSettings`

**Files:**
- Modify: `src/components/SettingsForm.tsx`

`SettingsForm` keeps its existing visual structure but now binds to `GlobalSettings` (no per-project / per-recording embedding). It also gains a "Default export format" radio group. The whisper download-modal flow stays unchanged.

- [ ] **Step 1: Rewrite the component**

Replace `src/components/SettingsForm.tsx` with:

```tsx
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { GlobalSettings, OutputFormat } from "../types/storage";
import DownloadModelModal from "./DownloadModelModal";
import { Checkbox, Input, Select } from "./ui";

interface Props {
  settings: GlobalSettings;
  onChange: (s: GlobalSettings) => void;
}

interface ModelInfo {
  size: string;
  downloaded: boolean;
  size_bytes: number;
}

const EXPORT_FORMATS: { id: OutputFormat; label: string; desc: string }[] = [
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

  const set = <K extends keyof GlobalSettings>(key: K, val: GlobalSettings[K]) =>
    onChange({ ...settings, [key]: val });

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
          <Input
            value={settings.ollamaUrl}
            onChange={(e) => set("ollamaUrl", e.target.value)}
            onBlur={refresh}
            placeholder="http://localhost:11434" />
        </Field>
        <Field label="Model">
          {ollamaModels.length > 0 ? (
            <Select value={settings.model}
              onChange={(e) => set("model", e.target.value)}>
              {ollamaModels.map((m) => <option key={m} value={m}>{m}</option>)}
            </Select>
          ) : (
            <Input value={settings.model}
              onChange={(e) => set("model", e.target.value)} placeholder="gemma3" />
          )}
        </Field>
      </Section>

      <Section title="Transcription">
        <Field label="Model">
          <Select value={settings.whisperModel}
            onChange={(e) => pickWhisper(e.target.value)}>
            {modelInfos.map((m) => (
              <option key={m.size} value={m.size}>{modelLabel(m)}</option>
            ))}
          </Select>
        </Field>
        <Field label="Default language">
          <Input
            value={settings.language}
            onChange={(e) => set("language", e.target.value)}
            placeholder="auto  (or e.g. en, de, fr)" />
        </Field>
      </Section>

      <Section title="Export">
        <div className="flex flex-col gap-2">
          <label className="text-body text-label-secondary">Default format</label>
          <div className="flex flex-col gap-1">
            {EXPORT_FORMATS.map(({ id, label, desc }) => (
              <label key={id}
                className="flex items-center gap-2 text-body text-label-primary cursor-pointer">
                <input
                  type="radio"
                  name="defaultExportFormat"
                  checked={settings.defaultExportFormat === id}
                  onChange={() => set("defaultExportFormat", id)}
                />
                <span className="font-medium">{label}</span>
                <span className="text-footnote text-label-tertiary">{desc}</span>
              </label>
            ))}
          </div>
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
      <h2 className="text-footnote font-semibold text-label-secondary uppercase tracking-wider">{title}</h2>
      {children}
    </div>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1">
      <label className="text-body text-label-secondary">{label}</label>
      {children}
    </div>
  );
}
```

- [ ] **Step 2: Commit**

```bash
git add src/components/SettingsForm.tsx
git commit -m "refactor(SettingsForm): consume GlobalSettings; add default export format"
```

---

## Task 12: Settings window shell + window-label routing in `main.tsx`

**Files:**
- Create: `src/components/SettingsWindow.tsx`
- Modify: `src/main.tsx`

- [ ] **Step 1: Create the settings window shell**

Write `src/components/SettingsWindow.tsx`:

```tsx
import { useGlobalSettings } from "../lib/useGlobalSettings";
import SettingsForm from "./SettingsForm";

export default function SettingsWindow() {
  const { settings, update } = useGlobalSettings();
  return (
    <div className="h-screen overflow-y-auto p-8 bg-window-background">
      <h1 className="text-title-2 font-semibold mb-6" data-tauri-drag-region>Settings</h1>
      {settings ? (
        <SettingsForm settings={settings} onChange={(s) => void update(s)} />
      ) : (
        <p className="text-label-tertiary">Loading…</p>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Branch on window label in `main.tsx`**

Open `src/main.tsx` (likely currently renders `<App />`) and update to:

```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import SettingsWindow from "./components/SettingsWindow";
import "./index.css";

const root = ReactDOM.createRoot(document.getElementById("root")!);
const label = getCurrentWindow().label;

root.render(
  <React.StrictMode>
    {label === "settings" ? <SettingsWindow /> : <App />}
  </React.StrictMode>,
);
```

(Adapt imports / strict-mode to match what currently exists — the only essential change is the label branch.)

- [ ] **Step 3: Commit**

```bash
git add src/components/SettingsWindow.tsx src/main.tsx
git commit -m "feat(ui): settings window shell + label-based routing"
```

---

## Task 13: Menu event handlers — `menu:settings`, `menu:export`

**Files:**
- Modify: `src/lib/menuEvents.ts`
- Modify: `src/App.tsx`

- [ ] **Step 1: Read the existing menuEvents file**

Open `src/lib/menuEvents.ts` and confirm the shape (it has `initMenuEvents`, `setMenuHandler`, and a registry of `menu:*` keys). The existing wiring already accepts arbitrary `menu:*` ids; no changes are strictly required there.

- [ ] **Step 2: Wire handlers in `App.tsx`**

Open `src/App.tsx`. In the `useEffect` that already calls `setMenuHandler(...)` for the other menu ids (around the `menu:new-project`, `menu:import-audio` block), add:

```tsx
setMenuHandler("menu:settings", () => {
  void api.openSettingsWindow();
});
setMenuHandler("menu:export", () => {
  recordingViewRef.current?.exportFile();
});
```

Note: `exportFile` will be added to `RecordingViewHandle` in Task 14.

- [ ] **Step 3: Commit**

```bash
git add src/App.tsx
git commit -m "feat(menu): wire ⌘, → open settings window; ⌘E → export"
```

---

## Task 14: Recording export — helpers + button + ⌘E

**Files:**
- Create: `src/lib/exportRecording.ts`
- Modify: `src/components/RecordingView.tsx`

`exportFile` is a single function the recording toolbar button and the menu handler both call. It opens the native save dialog with format-aware filters, normalizes the extension, generates bytes (re-using the existing `generateDocx`/`generatePdf` helpers — move them to the new module so the recording view stays thin), and writes the file.

- [ ] **Step 1: Create the export helper**

Write `src/lib/exportRecording.ts`:

```ts
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { invoke } from "@tauri-apps/api/core";
import type { OutputFormat } from "../types/storage";

const FILTER_LABELS: Record<OutputFormat, string> = {
  md: "Markdown",
  txt: "Plain text",
  docx: "Word document",
  pdf: "PDF",
};

const ALL_FORMATS: OutputFormat[] = ["md", "txt", "docx", "pdf"];

function filtersFor(defaultFmt: OutputFormat) {
  const ordered = [defaultFmt, ...ALL_FORMATS.filter((f) => f !== defaultFmt)];
  return ordered.map((f) => ({ name: FILTER_LABELS[f], extensions: [f] }));
}

function inferFormat(path: string, fallback: OutputFormat): OutputFormat {
  const ext = path.split(".").pop()?.toLowerCase();
  if (ext && (ALL_FORMATS as string[]).includes(ext)) return ext as OutputFormat;
  return fallback;
}

function normalizeExt(path: string, fmt: OutputFormat): string {
  const lower = path.toLowerCase();
  if (lower.endsWith(`.${fmt}`)) return path;
  const dot = path.lastIndexOf(".");
  if (dot > 0 && (ALL_FORMATS as string[]).includes(lower.slice(dot + 1))) {
    return path.slice(0, dot) + `.${fmt}`;
  }
  return path + `.${fmt}`;
}

export interface ExportInput {
  formatted: string;
  recordingName: string;
  defaultFormat: OutputFormat;
}

export interface ExportResult {
  path: string | null;
}

export async function exportRecording(input: ExportInput): Promise<ExportResult> {
  const dest = await saveDialog({
    defaultPath: `${input.recordingName}.${input.defaultFormat}`,
    filters: filtersFor(input.defaultFormat),
  });
  if (typeof dest !== "string") return { path: null };

  const fmt = inferFormat(dest, input.defaultFormat);
  const final = normalizeExt(dest, fmt);

  if (fmt === "md" || fmt === "txt") {
    await invoke("save_text_file", { content: input.formatted, path: final });
  } else if (fmt === "docx") {
    const bytes = await generateDocx(input.formatted);
    await invoke("save_binary_file", { bytes: Array.from(bytes), path: final });
  } else if (fmt === "pdf") {
    const bytes = await generatePdf(input.formatted);
    await invoke("save_binary_file", { bytes: Array.from(bytes), path: final });
  }

  try { await revealItemInDir(final); } catch { /* best-effort */ }
  return { path: final };
}

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

(`@tauri-apps/plugin-opener` is already a transitive dependency; if `npm run typecheck` complains it can't find the module, add it: `npm install @tauri-apps/plugin-opener` and rerun.)

- [ ] **Step 2: Modify `RecordingView` to use it**

Apply these changes to `src/components/RecordingView.tsx`:

1. **Add the import** at the top:

```ts
import { exportRecording } from "../lib/exportRecording";
import { useGlobalSettings } from "../lib/useGlobalSettings";
```

2. **Extend the handle**:

```ts
export interface RecordingViewHandle {
  runPipeline: () => void;
  exportFile: () => void;
}
```

3. **Inside the component**, just below the existing state declarations:

```ts
const { settings: globalSettings } = useGlobalSettings();
```

4. **Replace the old `runSaveFlow`** function entirely with an `exportFlow` (note: keep `run` simple — it no longer needs the save sub-flow):

```ts
const exportFlow = async () => {
  if (!rec || !globalSettings) return;
  const formatted = rec.stages.find((s) => s.stage === "format")?.output ?? "";
  if (!formatted) return;
  await exportRecording({
    formatted,
    recordingName: rec.name,
    defaultFormat: globalSettings.defaultExportFormat,
  });
};
```

5. **Strip the save-stage handling out of `run`**:

```ts
const run = async () => {
  setRunning(true);
  setTranscribePct(0);
  setFormatProgress(null);
  try {
    await api.runRecording(recordingId);
    await reload();
  } catch (err) {
    console.error("run_recording failed:", err);
    await reload();
  } finally {
    setRunning(false);
    onRecordingChanged();
  }
};
```

6. **Replace the `useImperativeHandle`**:

```ts
useImperativeHandle(ref, () => ({
  runPipeline: () => { runRef.current(); },
  exportFile: () => { void exportFlow(); },
}));
```

7. **Add a ⌘E keybinding** alongside the existing ⌘K listener:

```ts
useEffect(() => {
  const onKey = (e: KeyboardEvent) => {
    if (e.metaKey && e.key.toLowerCase() === "e") {
      const formatted = rec?.stages.find((s) => s.stage === "format")?.output ?? "";
      if (!formatted) return;
      e.preventDefault();
      void exportFlow();
    }
  };
  window.addEventListener("keydown", onKey);
  return () => window.removeEventListener("keydown", onKey);
}, [rec, globalSettings]);
```

8. **Add the Export button** in the toolbar — find the line with the Ask button and add the Export button just before it:

```tsx
<Button
  variant="secondary"
  size="md"
  disabled={!(rec.stages.find((s) => s.stage === "format")?.output ?? "")}
  title="Export the formatted output to a file (⌘E)"
  onClick={() => void exportFlow()}
>
  Export…
</Button>
```

9. **Remove the "Recording settings" `<details>` disclosure** — delete the whole `<details className="bg-control-background rounded-xl p-4">…</details>` block (it shows `SettingsForm` and `completedStageInvalidation`).

10. **Remove `completedStageInvalidation` and its helper**, plus the import of `SettingsForm` and `AppSettings`. They are no longer used in this file.

11. **Drop the `STAGE_ORDER` `"save"` entry** if it was there. The new constant is `["extract", "transcribe", "format"]`.

12. **Drop any references to `savedPaths`** in the body (passed to `OutputPreview`, etc.).

- [ ] **Step 3: Run the typechecker**

```bash
npx tsc --noEmit
```

Expected: passes. Errors should pinpoint stragglers (any place still referencing `AppSettings`, `recordSavedOutputs`, etc.).

- [ ] **Step 4: Commit**

```bash
git add src/lib/exportRecording.ts src/components/RecordingView.tsx
git commit -m "feat(export): on-demand export with ⌘E + toolbar button"
```

---

## Task 15: `ProjectView` — drop default-settings section, add language override

**Files:**
- Modify: `src/components/ProjectView.tsx`

- [ ] **Step 1: Edit ProjectView**

Apply:

1. **Drop the import** `SettingsForm` and `AppSettings`.

2. **Update `createFromStaging`** — `createRecording` no longer takes settings:

```ts
const createFromStaging = async () => {
  if (!project || staging.length === 0) return;
  const name = stagingName.trim() || deriveName(staging);
  const rec = await api.createRecording(projectId, name, staging);
  setStaging([]); setStagingName("");
  setRecordings((rs) => [...rs, rec]);
  onRecordingCreated(rec);
};
```

3. **Remove the entire "Default settings" `<div>` block** (the one whose `<h2>` is "Default settings").

4. **Add a Language override row** — insert this block immediately after the Additional Instructions block:

```tsx
<div className="flex flex-col gap-2">
  <label className="text-footnote font-medium text-label-secondary uppercase tracking-wider">
    Language override (optional)
  </label>
  <Input
    value={project.language ?? ""}
    onChange={(e) =>
      save({ ...project, language: e.target.value.trim() === "" ? null : e.target.value })
    }
    placeholder="Leave empty to use the global default"
  />
</div>
```

5. **Adjust `createProject` callers** elsewhere — anywhere in the codebase that calls `api.createProject(name, defaultSettings, template)` becomes `api.createProject(name, template)`. Search for `createProject(`:

```bash
grep -rn "createProject(" src/
```

The likely callsite is `src/components/Sidebar.tsx`'s "New project" flow — update it to the two-arg form.

- [ ] **Step 2: Run typecheck + sidebar tests**

```bash
npx tsc --noEmit
npx vitest run src/components/Sidebar.test.tsx
```

Expected: PASS. Sidebar tests may need a one-line fixture update if they construct a `Project` literal with `defaultSettings` — drop that field.

- [ ] **Step 3: Commit**

```bash
git add src/components/ProjectView.tsx src/components/Sidebar.tsx src/components/Sidebar.test.tsx
git commit -m "feat(ProjectView): remove default-settings block; add language override"
```

---

## Task 16: `OutputPreview` — drop `savedPaths` display

**Files:**
- Modify: `src/components/OutputPreview.tsx`

- [ ] **Step 1: Edit OutputPreview**

1. **Remove `savedPaths` from props** — drop it from the interface, drop the prop from the destructure, drop the block of JSX that renders it (look for a `<ul>` / list iterating `savedPaths`).

2. **Confirm `RecordingView` no longer passes `savedPaths`** (already done in Task 14).

- [ ] **Step 2: Run typecheck**

```bash
npx tsc --noEmit
```

Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add src/components/OutputPreview.tsx
git commit -m "refactor(OutputPreview): drop savedPaths display"
```

---

## Task 17: Ask panel — slide-in overlay with logical positioning

**Files:**
- Modify: `src/components/AskPanel.tsx`
- Modify: `src/components/AskPanel.test.tsx`
- Modify: `src/components/RecordingView.tsx`

`AskPanel` becomes self-positioning. It renders fixed/absolute over the recording pane on the trailing edge, includes its own header with a close button, listens for Esc to close, and uses logical inset properties so RTL works.

- [ ] **Step 1: Update existing AskPanel tests**

Open `src/components/AskPanel.test.tsx`. Add a new prop `onClose: () => void` everywhere the test renders `<AskPanel ... />`. Add a new test:

```tsx
it("calls onClose when Escape is pressed", async () => {
  const onClose = vi.fn();
  render(
    <AskPanel
      recordingId="r1"
      disabled={false}
      onCitationClick={() => {}}
      onClose={onClose}
    />,
  );
  fireEvent.keyDown(window, { key: "Escape" });
  expect(onClose).toHaveBeenCalled();
});

it("renders a visible close button that calls onClose", async () => {
  const onClose = vi.fn();
  render(
    <AskPanel
      recordingId="r1"
      disabled={false}
      onCitationClick={() => {}}
      onClose={onClose}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: /close/i }));
  expect(onClose).toHaveBeenCalled();
});
```

(Use `fireEvent` / `screen` / `render` exactly the way the rest of the file already does; copy that import block.)

- [ ] **Step 2: Run tests to verify they fail**

```bash
npx vitest run src/components/AskPanel.test.tsx
```

Expected: the two new tests FAIL with "no onClose prop" / "no Close button" or similar.

- [ ] **Step 3: Update `AskPanel.tsx`**

Apply (without showing the whole file — preserve all existing internals like `useAsk`, message rendering, citation handling):

1. **Add `onClose: () => void` to the props interface.**

2. **Wrap the existing root JSX** with a positioned overlay div:

```tsx
return (
  <div
    role="dialog"
    aria-label="Ask"
    className="fixed inset-y-0 end-0 z-20 w-[420px] bg-window-background/95
               backdrop-blur-xl shadow-[-12px_0_24px_rgba(0,0,0,0.15)]
               flex flex-col"
  >
    <header className="flex items-center justify-between px-4 py-3 border-b border-separator">
      <h2 className="text-body font-semibold">Ask</h2>
      <Button variant="ghost" size="sm" aria-label="Close" onClick={onClose}>
        ✕
      </Button>
    </header>
    <div className="flex-1 overflow-y-auto p-4 scrollable">
      {/* existing message list */}
    </div>
    {/* existing composer footer */}
  </div>
);
```

(Make sure `Button` and `Icon` imports remain. Keep all existing handler wiring.)

3. **Add an Escape listener**:

```tsx
useEffect(() => {
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") onClose();
  };
  window.addEventListener("keydown", onKey);
  return () => window.removeEventListener("keydown", onKey);
}, [onClose]);
```

- [ ] **Step 4: Update `RecordingView`'s Ask mount**

Replace the existing `<div className={askOpen ? "flex gap-4" : ""}> ... </div>` block with:

```tsx
<OutputPreview
  transcript={transcript}
  formatted={formatted}
  highlightRange={highlightRange}
/>
{askOpen && rec && (
  <AskPanel
    recordingId={rec.id}
    disabled={!hasTranscript}
    onCitationClick={(c) => {
      if (c.start === null || c.end === null) return;
      setHighlightRange({ start: c.start, end: c.end, nonce: Date.now() });
    }}
    onClose={() => setAskOpen(false)}
  />
)}
```

The AskPanel positions itself absolutely; it no longer participates in the flex row.

- [ ] **Step 5: Run all the panel tests**

```bash
npx vitest run src/components/AskPanel.test.tsx
```

Expected: all tests PASS.

- [ ] **Step 6: Commit**

```bash
git add src/components/AskPanel.tsx src/components/AskPanel.test.tsx src/components/RecordingView.tsx
git commit -m "feat(ask): trailing-edge slide-in panel with Esc/close button"
```

---

## Task 18: Verification + smoke test

**Files:**
- None to edit. Manual + automated verification.

- [ ] **Step 1: Run the full Rust suite**

```bash
cd src-tauri && cargo test
```

Expected: every test passes, including the v3→v4 migration tests.

- [ ] **Step 2: Run the full frontend suite**

```bash
npm run test -- --run
```

(or `npx vitest run` if the npm script differs.)

Expected: every test passes.

- [ ] **Step 3: Type check**

```bash
npx tsc --noEmit
```

Expected: no errors.

- [ ] **Step 4: Run the app and execute the manual smoke checklist**

```bash
npm run tauri dev
```

Confirm each:

- App launches; no migration error in the dev console / terminal.
- `⌘,` opens the Settings window. Changing the Ollama URL or model persists across closing/reopening the window.
- Open a project — the "Default settings" disclosure is gone, the "Language override" row is present and empty.
- Enter "de" in the language override; create a recording; run it. The transcribe stage logs/uses German.
- Clear the language override (empty string saves as `null`); subsequent runs use the global default.
- Run a recording to completion; the timeline shows only Extract / Transcribe / Format (no Save row).
- Click **Export…** on a completed recording → native save dialog shows the user's default format first; selecting a different filter writes the right type; the file lands at the chosen path and a Finder window is revealed.
- Type `report.txt` in the dialog but pick the PDF filter — the file is written as `report.pdf`.
- Press ⌘E from the recording view — same behavior as the Export button.
- Press ⌘K (or click Ask) — panel slides in from the right; conversation works; Esc closes; ⌘W also closes.
- With macOS in RTL (or simulated via dev tools `dir="rtl"`) the panel slides from the visual left.

- [ ] **Step 5: Final commit (only if any cleanup was needed during smoke test)**

If any small follow-ups appeared (e.g., a stale prop, a missed file), fix them and commit:

```bash
git add -A
git commit -m "chore: post-smoke cleanups"
```

If nothing was needed, skip this step.
