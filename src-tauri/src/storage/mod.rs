//! Persistent storage for projects, recordings, and stage state.
//!
//! Backed by a single SQLite database at `<app_data>/meister.db`. One shared
//! connection lives in Tauri state; all access is synchronous and must be
//! wrapped in `tokio::task::spawn_blocking` from async commands.

pub mod schema;
pub mod types;
pub mod projects;
pub mod recordings;
pub mod stages;
pub mod hash;

pub use types::*;
pub use hash::template_hash;

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
    pub fn open_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory().map_err(|e| format!("open db: {e}"))?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|e| format!("enable foreign keys: {e}"))?;
        schema::migrate(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }
}

/// Scan `captures_dir` and delete any files whose paths are NOT present in any
/// recording's `source_paths`. Safe to call at startup. Returns the number of
/// files removed. Missing directories are treated as "nothing to clean".
pub fn cleanup_orphaned_captures(store: &Store, captures_dir: &Path) -> Result<usize, String> {
    let entries = match std::fs::read_dir(captures_dir) {
        Ok(e) => e,
        Err(_) => return Ok(0),
    };

    let referenced: std::collections::HashSet<String> = {
        let conn = store.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT source_paths FROM recordings")
            .map_err(|e| format!("prepare: {e}"))?;
        let set: std::collections::HashSet<String> = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| format!("query: {e}"))?
            .filter_map(Result::ok)
            .flat_map(|json| {
                serde_json::from_str::<Vec<String>>(&json).unwrap_or_default()
            })
            .collect();
        set
    };

    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let s = path.to_string_lossy().to_string();
        if !referenced.contains(&s) {
            if std::fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        }
    }
    Ok(removed)
}

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
        assert_eq!(v, 3);
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
