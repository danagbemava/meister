mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running Meister");
}
