//! Schema + migrations. The database version is tracked via `PRAGMA
//! user_version`. Each migration moves the version forward by one.

use rusqlite::{Connection, OptionalExtension};

const CURRENT_VERSION: i32 = 4;

pub fn migrate(conn: &Connection) -> Result<(), String> {
    let mut version: i32 = conn
        .query_row("PRAGMA user_version;", [], |r| r.get(0))
        .map_err(|e| format!("read user_version: {e}"))?;

    while version < CURRENT_VERSION {
        match version {
            0 => migrate_v0_to_v1(conn)?,
            1 => migrate_v1_to_v2(conn)?,
            2 => migrate_v2_to_v3(conn)?,
            3 => migrate_v3_to_v4(conn)?,
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

fn migrate_v2_to_v3(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        r#"
        ALTER TABLE projects ADD COLUMN template_match_style INTEGER NOT NULL DEFAULT 0;
        ALTER TABLE projects ADD COLUMN custom_instructions TEXT NOT NULL DEFAULT '';
        "#,
    )
    .map_err(|e| format!("apply v3 schema: {e}"))
}

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
