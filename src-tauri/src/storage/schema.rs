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
