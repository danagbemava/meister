//! Stage-level updates. Each function is a single UPDATE, so every row
//! transition is atomic. `reset_downstream` walks the canonical stage order
//! and wipes every stage strictly after the pivot.

use crate::storage::{StageKind, StageStatus, Store};
use rusqlite::params;

/// Everything you can set when a stage completes. Per-stage only a subset
/// is meaningful (see `inputs_unchanged` and `run_recording`), but storing
/// them all here keeps callers simple and the schema narrow.
pub struct CompletedOutput {
    pub output: Option<String>,
    pub model_used: Option<String>,
    pub template_hash: Option<String>,
    pub saved_paths: Option<Vec<String>>,
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
    let saved_paths_json = match &out.saved_paths {
        Some(v) => Some(
            serde_json::to_string(v).map_err(|e| format!("serialize saved_paths: {e}"))?,
        ),
        None => None,
    };
    let conn = store.conn.lock().unwrap();
    conn.execute(
        "UPDATE stages SET \
         status = ?3, error = NULL, completed_at = ?4, model_used = ?5, \
         template_hash = ?6, output = ?7, saved_paths = ?8 \
         WHERE recording_id = ?1 AND stage = ?2",
        params![
            recording_id,
            stage.as_str(),
            StageStatus::Completed.as_str(),
            out.completed_at,
            out.model_used,
            out.template_hash,
            out.output,
            saved_paths_json,
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

/// Reset every stage strictly after `pivot` back to `pending`, clearing its
/// output/error fields. `pivot` itself is untouched.
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
         template_hash = NULL, output = NULL, saved_paths = NULL \
         WHERE recording_id = ?1 AND stage IN ({})",
        placeholders.join(",")
    );
    let conn = store.conn.lock().unwrap();
    let mut stmt = conn.prepare(&sql).map_err(|e| format!("prepare: {e}"))?;
    let mut p: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(later.len() + 1);
    p.push(&recording_id);
    for s in &later {
        p.push(s);
    }
    stmt.execute(rusqlite::params_from_iter(p.iter()))
        .map_err(|e| format!("reset_downstream: {e}"))?;
    Ok(())
}

/// Reset every stage including and after `pivot` back to pending. Used by
/// "Start over" (call with `StageKind::Extract`).
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
         template_hash = NULL, output = NULL, saved_paths = NULL \
         WHERE recording_id = ?1 AND stage IN ({})",
        placeholders.join(",")
    );
    let conn = store.conn.lock().unwrap();
    let mut stmt = conn.prepare(&sql).map_err(|e| format!("prepare: {e}"))?;
    let mut p: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(kinds.len() + 1);
    p.push(&recording_id);
    for s in &kinds {
        p.push(s);
    }
    stmt.execute(rusqlite::params_from_iter(p.iter()))
        .map_err(|e| format!("reset_from: {e}"))?;
    Ok(())
}

/// Flip every `running` stage anywhere in the DB to `failed` with the given
/// message. Called at app startup to clean up state left behind by a crash.
/// Returns the number of rows swept.
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
