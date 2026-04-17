//! End-to-end tests for the storage module against an in-memory SQLite DB.

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
