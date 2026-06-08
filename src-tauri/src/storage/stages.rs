//! Stage-level updates. Each function is a single UPDATE, so every row
//! transition is atomic.

use crate::storage::{StageKind, StageStatus, Store};
use rusqlite::params;

pub struct CompletedOutput {
    pub output: Option<String>,
    pub model_used: Option<String>,
    pub template_hash: Option<String>,
    pub completed_at: i64,
}

/// Replace just the `output` of a completed stage. Used when the user
/// edits the formatted text in the UI before exporting; status / model_used /
/// template_hash / completed_at stay as-is. Requires the stage to be in
/// `completed` status — otherwise the call is a no-op and returns Err.
pub fn set_output(
    store: &Store,
    recording_id: &str,
    stage: StageKind,
    output: &str,
) -> Result<(), String> {
    let conn = store.conn.lock().unwrap();
    let n = conn.execute(
        "UPDATE stages SET output = ?3 \
         WHERE recording_id = ?1 AND stage = ?2 AND status = 'completed'",
        params![recording_id, stage.as_str(), output],
    )
    .map_err(|e| format!("set_output: {e}"))?;
    if n == 0 {
        return Err(format!(
            "stage not editable: recording={recording_id}, stage={}",
            stage.as_str()
        ));
    }
    Ok(())
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
    let conn = store.conn.lock().unwrap();
    conn.execute(
        "UPDATE stages SET \
         status = ?3, error = NULL, completed_at = ?4, model_used = ?5, \
         template_hash = ?6, output = ?7 \
         WHERE recording_id = ?1 AND stage = ?2",
        params![
            recording_id,
            stage.as_str(),
            StageStatus::Completed.as_str(),
            out.completed_at,
            out.model_used,
            out.template_hash,
            out.output,
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
         template_hash = NULL, output = NULL \
         WHERE recording_id = ?1 AND stage IN ({})",
        placeholders.join(",")
    );
    let conn = store.conn.lock().unwrap();
    let mut stmt = conn.prepare(&sql).map_err(|e| format!("prepare: {e}"))?;
    let mut p: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(later.len() + 1);
    p.push(&recording_id);
    for s in &later { p.push(s); }
    stmt.execute(rusqlite::params_from_iter(p.iter()))
        .map_err(|e| format!("reset_downstream: {e}"))?;
    Ok(())
}

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
         template_hash = NULL, output = NULL \
         WHERE recording_id = ?1 AND stage IN ({})",
        placeholders.join(",")
    );
    let conn = store.conn.lock().unwrap();
    let mut stmt = conn.prepare(&sql).map_err(|e| format!("prepare: {e}"))?;
    let mut p: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(kinds.len() + 1);
    p.push(&recording_id);
    for s in &kinds { p.push(s); }
    stmt.execute(rusqlite::params_from_iter(p.iter()))
        .map_err(|e| format!("reset_from: {e}"))?;
    Ok(())
}

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
