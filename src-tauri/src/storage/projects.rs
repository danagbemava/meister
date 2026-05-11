//! Project CRUD.

use crate::storage::{Project, Store, Template, TemplateSource};
use rusqlite::params;

pub fn insert(store: &Store, p: &Project) -> Result<(), String> {
    let conn = store.conn.lock().unwrap();
    conn.execute(
        "INSERT INTO projects \
         (id, name, created_at, updated_at, template_source, template_content, \
          template_source_path, template_match_style, custom_instructions, language) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            p.id, p.name, p.created_at, p.updated_at,
            p.template.source.as_str(),
            p.template.content,
            p.template.source_path,
            p.template.match_style as i32,
            p.custom_instructions,
            p.language,
        ],
    )
    .map_err(|e| format!("insert project: {e}"))?;
    Ok(())
}

pub fn update(store: &Store, p: &Project) -> Result<(), String> {
    let conn = store.conn.lock().unwrap();
    let n = conn.execute(
        "UPDATE projects SET \
         name = ?2, updated_at = ?3, template_source = ?4, template_content = ?5, \
         template_source_path = ?6, template_match_style = ?7, \
         custom_instructions = ?8, language = ?9 \
         WHERE id = ?1",
        params![
            p.id, p.name, p.updated_at,
            p.template.source.as_str(),
            p.template.content,
            p.template.source_path,
            p.template.match_style as i32,
            p.custom_instructions,
            p.language,
        ],
    )
    .map_err(|e| format!("update project: {e}"))?;
    if n == 0 {
        return Err(format!("project not found: {}", p.id));
    }
    Ok(())
}

pub fn get(store: &Store, id: &str) -> Result<Option<Project>, String> {
    let conn = store.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, name, created_at, updated_at, template_source, \
             template_content, template_source_path, template_match_style, \
             custom_instructions, language \
             FROM projects WHERE id = ?1",
        )
        .map_err(|e| format!("prepare: {e}"))?;
    let mut rows = stmt.query(params![id]).map_err(|e| format!("query: {e}"))?;
    if let Some(row) = rows.next().map_err(|e| format!("next: {e}"))? {
        Ok(Some(row_to_project(row)?))
    } else {
        Ok(None)
    }
}

pub fn list(store: &Store) -> Result<Vec<Project>, String> {
    let conn = store.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, name, created_at, updated_at, template_source, \
             template_content, template_source_path, template_match_style, \
             custom_instructions, language \
             FROM projects ORDER BY created_at ASC",
        )
        .map_err(|e| format!("prepare: {e}"))?;
    let rows = stmt
        .query_map([], |row| Ok(row_to_project(row)))
        .map_err(|e| format!("query_map: {e}"))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("row: {e}"))??);
    }
    Ok(out)
}

pub fn delete(store: &Store, id: &str) -> Result<(), String> {
    let conn = store.conn.lock().unwrap();
    conn.execute("DELETE FROM projects WHERE id = ?1", params![id])
        .map_err(|e| format!("delete project: {e}"))?;
    Ok(())
}

fn row_to_project(row: &rusqlite::Row) -> Result<Project, String> {
    let source_str: String = row.get(4).map_err(|e| format!("source: {e}"))?;
    let source = match source_str.as_str() {
        "inline" => TemplateSource::Inline,
        "file" => TemplateSource::File,
        other => return Err(format!("unknown template_source: {other}")),
    };
    let match_style_int: i64 = row.get(7).map_err(|e| format!("match_style: {e}"))?;
    let custom_instructions: String = row.get(8).map_err(|e| format!("custom_instructions: {e}"))?;
    let language: Option<String> = row.get(9).map_err(|e| format!("language: {e}"))?;
    Ok(Project {
        id: row.get(0).map_err(|e| format!("id: {e}"))?,
        name: row.get(1).map_err(|e| format!("name: {e}"))?,
        created_at: row.get(2).map_err(|e| format!("created_at: {e}"))?,
        updated_at: row.get(3).map_err(|e| format!("updated_at: {e}"))?,
        template: Template {
            source,
            content: row.get(5).map_err(|e| format!("content: {e}"))?,
            source_path: row.get(6).map_err(|e| format!("source_path: {e}"))?,
            match_style: match_style_int != 0,
        },
        custom_instructions,
        language,
    })
}

impl TemplateSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            TemplateSource::Inline => "inline",
            TemplateSource::File => "file",
        }
    }
}
