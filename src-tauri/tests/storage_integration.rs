//! End-to-end tests for the storage module against an in-memory SQLite DB.

use meister_lib::orchestrator;
use meister_lib::storage::{
    self, AppSettings, OutputFormat, Project, Store, Template, TemplateSource,
};
use meister_lib::storage::{Recording, StageKind, StageStatus};

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
            match_style: false,
        },
        default_settings: default_settings(),
        custom_instructions: String::new(),
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
        &template_hash, rec.source_paths.first().map(String::as_str).unwrap_or(""), true,
    );
    assert!(!valid);

    storage::stages::reset_from(&store, "r1", StageKind::Transcribe).unwrap();
    let rec = storage::recordings::get(&store, "r1").unwrap().unwrap();
    assert_eq!(rec.stages[0].status, StageStatus::Completed); // extract kept
    assert_eq!(rec.stages[1].status, StageStatus::Pending);   // transcribe reset
    assert_eq!(rec.stages[2].status, StageStatus::Pending);   // format reset
    assert_eq!(rec.stages[3].status, StageStatus::Pending);   // save reset
}

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
    assert_eq!(v, 3);

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
