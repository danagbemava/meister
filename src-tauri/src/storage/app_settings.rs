//! Singleton app-wide settings. Always exactly one row.

use crate::storage::{GlobalSettings, OutputFormat, Store};
use rusqlite::params;

pub fn get(store: &Store) -> Result<GlobalSettings, String> {
    let conn = store.conn.lock().unwrap();
    conn.query_row(
        "SELECT ollama_url, model, whisper_model, language, default_export_format \
         FROM app_settings WHERE singleton = 1",
        [],
        |r| {
            let fmt: String = r.get(4)?;
            Ok(GlobalSettings {
                ollama_url: r.get(0)?,
                model: r.get(1)?,
                whisper_model: r.get(2)?,
                language: r.get(3)?,
                default_export_format: OutputFormat::from_str(&fmt)
                    .unwrap_or(OutputFormat::Md),
            })
        },
    )
    .map_err(|e| format!("read app_settings: {e}"))
}

pub fn set(store: &Store, s: &GlobalSettings) -> Result<(), String> {
    let conn = store.conn.lock().unwrap();
    conn.execute(
        "UPDATE app_settings SET \
         ollama_url = ?1, model = ?2, whisper_model = ?3, language = ?4, \
         default_export_format = ?5 \
         WHERE singleton = 1",
        params![
            s.ollama_url, s.model, s.whisper_model, s.language,
            s.default_export_format.as_str(),
        ],
    )
    .map_err(|e| format!("update app_settings: {e}"))?;
    Ok(())
}
