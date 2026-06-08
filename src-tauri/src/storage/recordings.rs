//! Recording CRUD. On insert, three `pending` stage rows are created so every
//! recording always has a complete 3-row stage set.

use crate::storage::{Recording, StageKind, StageRecord, StageStatus, Store};
use rusqlite::params;

pub fn insert(store: &Store, r: &Recording) -> Result<(), String> {
    let conn = store.conn.lock().unwrap();
    let source_paths_json = serde_json::to_string(&r.source_paths)
        .map_err(|e| format!("serialize source_paths: {e}"))?;
    conn.execute(
        "INSERT INTO recordings \
         (id, project_id, name, source_paths, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            r.id, r.project_id, r.name, source_paths_json,
            r.created_at, r.updated_at,
        ],
    )
    .map_err(|e| format!("insert recording: {e}"))?;

    for kind in StageKind::ALL {
        conn.execute(
            "INSERT INTO stages (recording_id, stage, status) VALUES (?1, ?2, ?3)",
            params![r.id, kind.as_str(), StageStatus::Pending.as_str()],
        )
        .map_err(|e| format!("insert stage: {e}"))?;
    }
    Ok(())
}

pub fn update(store: &Store, r: &Recording) -> Result<(), String> {
    let conn = store.conn.lock().unwrap();
    let source_paths_json = serde_json::to_string(&r.source_paths)
        .map_err(|e| format!("serialize source_paths: {e}"))?;
    let n = conn.execute(
        "UPDATE recordings SET name = ?2, source_paths = ?3, updated_at = ?4 \
         WHERE id = ?1",
        params![r.id, r.name, source_paths_json, r.updated_at],
    )
    .map_err(|e| format!("update recording: {e}"))?;
    if n == 0 {
        return Err(format!("recording not found: {}", r.id));
    }
    Ok(())
}

pub fn get(store: &Store, id: &str) -> Result<Option<Recording>, String> {
    let conn = store.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, name, source_paths, created_at, updated_at \
             FROM recordings WHERE id = ?1",
        )
        .map_err(|e| format!("prepare: {e}"))?;
    let mut rows = stmt.query(params![id]).map_err(|e| format!("query: {e}"))?;
    let Some(row) = rows.next().map_err(|e| format!("next: {e}"))? else {
        return Ok(None);
    };
    let mut rec = row_to_recording_without_stages(row)?;
    drop(rows);
    drop(stmt);
    rec.stages = load_stages(&conn, id)?;
    Ok(Some(rec))
}

pub fn list_by_project(store: &Store, project_id: &str) -> Result<Vec<Recording>, String> {
    let conn = store.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, name, source_paths, created_at, updated_at \
             FROM recordings WHERE project_id = ?1 ORDER BY created_at ASC",
        )
        .map_err(|e| format!("prepare: {e}"))?;
    let rows = stmt
        .query_map(params![project_id], |row| Ok(row_to_recording_without_stages(row)))
        .map_err(|e| format!("query_map: {e}"))?;
    let mut out = Vec::new();
    for r in rows {
        let mut rec = r.map_err(|e| format!("row: {e}"))??;
        rec.stages = load_stages(&conn, &rec.id)?;
        out.push(rec);
    }
    Ok(out)
}

pub fn delete(store: &Store, id: &str) -> Result<(), String> {
    let conn = store.conn.lock().unwrap();
    conn.execute("DELETE FROM recordings WHERE id = ?1", params![id])
        .map_err(|e| format!("delete recording: {e}"))?;
    Ok(())
}

fn row_to_recording_without_stages(row: &rusqlite::Row) -> Result<Recording, String> {
    let source_paths_json: String = row.get(3).map_err(|e| format!("source_paths: {e}"))?;
    let source_paths: Vec<String> = serde_json::from_str(&source_paths_json)
        .map_err(|e| format!("parse source_paths: {e}"))?;
    Ok(Recording {
        id: row.get(0).map_err(|e| format!("id: {e}"))?,
        project_id: row.get(1).map_err(|e| format!("project_id: {e}"))?,
        name: row.get(2).map_err(|e| format!("name: {e}"))?,
        source_paths,
        created_at: row.get(4).map_err(|e| format!("created_at: {e}"))?,
        updated_at: row.get(5).map_err(|e| format!("updated_at: {e}"))?,
        stages: vec![],
    })
}

fn load_stages(
    conn: &rusqlite::Connection,
    recording_id: &str,
) -> Result<Vec<StageRecord>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT stage, status, error, completed_at, model_used, template_hash, output \
             FROM stages WHERE recording_id = ?1",
        )
        .map_err(|e| format!("prepare stages: {e}"))?;
    let rows = stmt
        .query_map(params![recording_id], |row| Ok(row_to_stage(row)))
        .map_err(|e| format!("query_map stages: {e}"))?;
    let mut by_kind: std::collections::HashMap<StageKind, StageRecord> =
        std::collections::HashMap::new();
    for r in rows {
        let Ok(Ok(rec)) = r else { continue };
        by_kind.insert(rec.stage, rec);
    }
    let mut out = Vec::with_capacity(3);
    for kind in StageKind::ALL {
        if let Some(rec) = by_kind.remove(&kind) {
            out.push(rec);
        } else {
            out.push(StageRecord {
                stage: kind,
                status: StageStatus::Pending,
                error: None,
                completed_at: None,
                model_used: None,
                template_hash: None,
                output: None,
            });
        }
    }
    Ok(out)
}

fn row_to_stage(row: &rusqlite::Row) -> Result<StageRecord, String> {
    let stage_str: String = row.get(0).map_err(|e| format!("stage: {e}"))?;
    let status_str: String = row.get(1).map_err(|e| format!("status: {e}"))?;
    let Some(stage) = StageKind::from_str(&stage_str) else {
        return Err(format!("unknown stage: {stage_str}"));
    };
    Ok(StageRecord {
        stage,
        status: StageStatus::from_str(&status_str)
            .ok_or_else(|| format!("unknown status: {status_str}"))?,
        error: row.get(2).map_err(|e| format!("error: {e}"))?,
        completed_at: row.get(3).map_err(|e| format!("completed_at: {e}"))?,
        model_used: row.get(4).map_err(|e| format!("model_used: {e}"))?,
        template_hash: row.get(5).map_err(|e| format!("template_hash: {e}"))?,
        output: row.get(6).map_err(|e| format!("output: {e}"))?,
    })
}
