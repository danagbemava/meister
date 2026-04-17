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
