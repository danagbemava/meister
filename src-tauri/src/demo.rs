//! Demo mode: an isolated parallel data store activated by a flag file.
//!
//! When `<app_data>/.demo-mode` exists, the app reads/writes
//! `<app_data>/meister-demo.db` and uses `<app_data>/captures-demo/` for
//! captures. Toggling on/off requires an app restart so we never mix the
//! two stores at runtime. On first activation, the demo DB is seeded with
//! a fictional project + recordings backed by a bundled sample audio file
//! so the full pipeline can be exercised for screenshots / recordings.

use crate::storage::{
    self, GlobalSettings, OutputFormat, Project, Recording, StageKind, Store, Template,
    TemplateSource,
};
use std::path::{Path, PathBuf};

const FLAG_FILE: &str = ".demo-mode";

pub fn is_active(app_data_dir: &Path) -> bool {
    app_data_dir.join(FLAG_FILE).exists()
}

/// Resolve the app data directory without relying on Tauri's `PathResolver`,
/// which isn't ready inside the `.menu(...)` builder closure. Mirrors the
/// macOS layout Tauri uses for our configured bundle identifier.
pub fn app_data_dir_static() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(|h| {
        PathBuf::from(h).join("Library/Application Support/com.meister.app")
    })
}

pub fn db_path(app_data_dir: &Path) -> PathBuf {
    if is_active(app_data_dir) {
        app_data_dir.join("meister-demo.db")
    } else {
        app_data_dir.join("meister.db")
    }
}

pub fn captures_dir(app_data_dir: &Path) -> PathBuf {
    if is_active(app_data_dir) {
        app_data_dir.join("captures-demo")
    } else {
        app_data_dir.join("captures")
    }
}

pub fn enable(app_data_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(app_data_dir)
        .map_err(|e| format!("create app_data_dir: {e}"))?;
    std::fs::write(app_data_dir.join(FLAG_FILE), b"")
        .map_err(|e| format!("write demo flag: {e}"))
}

pub fn disable(app_data_dir: &Path) -> Result<(), String> {
    let path = app_data_dir.join(FLAG_FILE);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("remove demo flag: {e}"))?;
    }
    Ok(())
}

/// Idempotent seed: only runs when the demo DB has zero projects. Copies
/// the bundled `sample-meeting.m4a` from `bundled_dir/demo/` into
/// `captures-demo/` so recordings reference a stable path that survives
/// app reinstalls. Sets the seeded `app_settings` to sensible defaults.
pub fn seed_if_empty(
    store: &Store,
    bundled_dir: &Path,
    app_data_dir: &Path,
) -> Result<(), String> {
    let existing = storage::projects::list(store)?;
    if !existing.is_empty() {
        return Ok(());
    }

    let captures = captures_dir(app_data_dir);
    std::fs::create_dir_all(&captures)
        .map_err(|e| format!("create demo captures dir: {e}"))?;

    let bundled_audio = bundled_dir.join("demo").join("sample-meeting.m4a");
    let demo_audio = captures.join("sample-meeting.m4a");
    if !demo_audio.exists() {
        if !bundled_audio.exists() {
            return Err(format!(
                "bundled demo audio not found at {}",
                bundled_audio.display(),
            ));
        }
        std::fs::copy(&bundled_audio, &demo_audio)
            .map_err(|e| format!("copy demo audio: {e}"))?;
    }
    let audio_path_str = demo_audio.to_string_lossy().to_string();

    // Seed global settings.
    let conn = store.conn.lock().unwrap();
    conn.execute(
        "UPDATE app_settings SET \
         ollama_url = ?1, model = ?2, whisper_model = ?3, language = ?4, \
         default_export_format = ?5 \
         WHERE singleton = 1",
        rusqlite::params![
            "http://localhost:11434",
            "gemma4",
            "base",
            "en",
            "md",
        ],
    )
    .map_err(|e| format!("seed app_settings: {e}"))?;
    drop(conn);

    let now = now_ms();
    let project = Project {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Acme Q3 Operations Review".to_string(),
        created_at: now - 7 * 24 * 60 * 60 * 1000,
        updated_at: now - 24 * 60 * 60 * 1000,
        template: Template {
            source: TemplateSource::Inline,
            content: demo_template().to_string(),
            source_path: None,
            match_style: false,
        },
        custom_instructions:
            "Use crisp executive prose. Lead with numbers when they are present. \
             Convert dollar amounts to plain language (e.g. \"two point four million\" \
             becomes \"$2.4M\")."
                .to_string(),
        language: None,
    };
    storage::projects::insert(store, &project)?;

    // Recording 1: ready to run. Only the source exists; all stages pending.
    let r1 = Recording {
        id: uuid::Uuid::new_v4().to_string(),
        project_id: project.id.clone(),
        name: "Weekly Ops Sync — 2026-05-12".to_string(),
        source_paths: vec![audio_path_str.clone()],
        created_at: now - 60 * 60 * 1000,
        updated_at: now - 60 * 60 * 1000,
        stages: vec![],
    };
    storage::recordings::insert(store, &r1)?;

    // Recording 2: fully completed with pre-baked transcript + formatted text.
    let r2 = Recording {
        id: uuid::Uuid::new_v4().to_string(),
        project_id: project.id.clone(),
        name: "Quarterly Strategy Review — 2026-05-05".to_string(),
        source_paths: vec![audio_path_str.clone()],
        created_at: now - 3 * 24 * 60 * 60 * 1000,
        updated_at: now - 3 * 24 * 60 * 60 * 1000,
        stages: vec![],
    };
    storage::recordings::insert(store, &r2)?;
    {
        let r2_now = now - 3 * 24 * 60 * 60 * 1000;
        storage::stages::set_completed(
            store,
            &r2.id,
            StageKind::Extract,
            storage::stages::CompletedOutput {
                output: Some(audio_path_str.clone()),
                model_used: None,
                template_hash: None,
                completed_at: r2_now,
            },
        )?;
        storage::stages::set_completed(
            store,
            &r2.id,
            StageKind::Transcribe,
            storage::stages::CompletedOutput {
                output: Some(demo_transcript().to_string()),
                model_used: Some("base@en".to_string()),
                template_hash: None,
                completed_at: r2_now,
            },
        )?;
        storage::stages::set_completed(
            store,
            &r2.id,
            StageKind::Format,
            storage::stages::CompletedOutput {
                output: Some(demo_formatted().to_string()),
                model_used: Some("gemma4".to_string()),
                template_hash: Some(storage::template_hash(&project.template.content)),
                completed_at: r2_now,
            },
        )?;
    }

    // Recording 3: extract done, transcribe pending. Captures mid-pipeline UI.
    let r3 = Recording {
        id: uuid::Uuid::new_v4().to_string(),
        project_id: project.id.clone(),
        name: "Board Pre-read — 2026-05-14".to_string(),
        source_paths: vec![audio_path_str.clone()],
        created_at: now - 2 * 60 * 60 * 1000,
        updated_at: now - 2 * 60 * 60 * 1000,
        stages: vec![],
    };
    storage::recordings::insert(store, &r3)?;
    storage::stages::set_completed(
        store,
        &r3.id,
        StageKind::Extract,
        storage::stages::CompletedOutput {
            output: Some(audio_path_str),
            model_used: None,
            template_hash: None,
            completed_at: now - 2 * 60 * 60 * 1000,
        },
    )?;

    Ok(())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn demo_template() -> &'static str {
    "# {{title}}\n\n\
     **Date:** {{date}}\n\
     **Attendees:** {{attendees}}\n\n\
     ## Summary\n{{summary}}\n\n\
     ## Decisions\n{{decisions}}\n\n\
     ## Action Items\n{{action_items}}\n\n\
     ## Open Questions\n{{open_questions}}\n"
}

fn demo_transcript() -> &'static str {
    "Good morning everyone. Thanks for joining the weekly Acme operations sync. \
     Today we have three items on the agenda: Q3 revenue numbers, the customer \
     onboarding pilot, and engineering velocity. Maya, can you kick us off with \
     the Q3 update? Sure. We closed at two point four million in net new ARR, \
     beating target by twelve percent. The biggest contributor was the enterprise \
     segment. We added eight new logos including two Fortune 500 accounts. The \
     pipeline for Q4 looks strong, with twenty active opportunities in late stage \
     review. On the pilot side, retention is tracking at ninety one percent \
     thirty days in. We will share the full deck before Thursday."
}

fn demo_formatted() -> &'static str {
    "# Quarterly Strategy Review\n\n\
     **Date:** 2026-05-05\n\
     **Attendees:** Maya Chen (Revenue), Devon Park (Customer Success), \
     Priya Iyer (Engineering)\n\n\
     ## Summary\n\
     Q3 closed at $2.4M in net new ARR, **12% above target**, driven primarily \
     by enterprise wins including two Fortune 500 accounts. Customer onboarding \
     pilot is showing strong early signals with 91% 30-day retention. Q4 \
     pipeline is robust with 20 late-stage opportunities under active review.\n\n\
     ## Decisions\n\
     - Continue current enterprise GTM motion through Q4\n\
     - Expand the onboarding pilot to the next customer cohort\n\n\
     ## Action Items\n\
     - **Maya:** Share full Q3 deck with the leadership team by Thursday\n\
     - **Devon:** Compile 60-day retention numbers for the pilot cohort\n\
     - **Priya:** Confirm engineering capacity for the Q4 enterprise integrations\n\n\
     ## Open Questions\n\
     - What is the projected Q4 ARR target given the strong Q3 close?\n\
     - Which Fortune 500 expansions should be prioritized first?\n"
}
