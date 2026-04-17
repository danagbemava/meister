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
        assert_eq!(v, 2);
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
