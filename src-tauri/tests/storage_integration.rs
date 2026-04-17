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
