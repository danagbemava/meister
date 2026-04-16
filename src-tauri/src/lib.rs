mod commands;
pub mod transcription;

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
            let resource_dir = app.path().resource_dir()?;
            let app_data_dir = app.path().app_data_dir()?;
            transcription::ensure_bundled_base_copied(&resource_dir, &app_data_dir)
                .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
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
