mod commands;
pub mod transcription;
pub mod storage;
pub mod orchestrator;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    use std::sync::Arc;

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .manage(Arc::new(transcription::CancelRegistry::default()))
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;

            let resource_dir = app.path().resource_dir()?;
            let bundled_dir = if resource_dir.join("models").join("ggml-base.bin").is_file() {
                resource_dir
            } else {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources")
            };

            std::env::set_var("GGML_METAL_PATH_RESOURCES", &bundled_dir);

            transcription::ensure_bundled_base_copied(&bundled_dir, &app_data_dir)
                .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

            // Storage: open DB, sweep interrupted `running` stage rows.
            let db_path = app_data_dir.join("meister.db");
            let store = storage::Store::open(&db_path)
                .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
            let swept = storage::stages::mark_running_as_failed(
                &store, "Interrupted (app closed during processing)",
            ).map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
            if swept > 0 {
                eprintln!("storage: marked {swept} interrupted stage(s) as failed");
            }
            app.manage(std::sync::Arc::new(store));

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::check_dependencies,
            commands::list_ollama_models,
            commands::extract_audio,
            commands::transcribe_audio,
            commands::format_transcript,
            commands::save_text_file,
            commands::save_binary_file,
            commands::read_file_as_text,
            commands::cleanup_temp,
            commands::list_available_models,
            commands::download_model,
            commands::cancel_model_download,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Meister");
}
