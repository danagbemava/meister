mod commands;
pub mod accent;
pub mod vibrancy;
pub mod transcription;
pub mod storage;
pub mod orchestrator;
pub mod capture;
pub mod cancellation;
pub mod menu;

#[cfg(test)]
mod test_support;

use tauri::{Emitter, Manager};

/// When launched from Finder/Dock, a bundled .app inherits a minimal PATH
/// that excludes the directories Homebrew uses for binaries (`/opt/homebrew/bin`
/// on Apple Silicon, `/usr/local/bin` on Intel). Subprocess lookups for
/// ffmpeg (and any future shell-outs) then fail with "command not found".
/// Prepend the standard Homebrew prefixes so child processes can resolve them.
#[cfg(target_os = "macos")]
fn augment_path_for_homebrew() {
    use std::env;
    const EXTRA: &[&str] = &["/opt/homebrew/bin", "/usr/local/bin", "/usr/local/sbin"];
    let current = env::var("PATH").unwrap_or_default();
    let mut parts: Vec<&str> = current.split(':').filter(|p| !p.is_empty()).collect();
    for p in EXTRA.iter().rev() {
        if !parts.iter().any(|x| x == p) {
            parts.insert(0, p);
        }
    }
    env::set_var("PATH", parts.join(":"));
}

#[cfg(not(target_os = "macos"))]
fn augment_path_for_homebrew() {}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    use std::sync::Arc;

    augment_path_for_homebrew();

    tauri::Builder::default()
        .menu(|handle| menu::build_menu(handle))
        .on_menu_event(|app, event| {
            let id = event.id().0.as_str();
            if id.starts_with("menu:") {
                let _ = app.emit(id, ());
            }
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .manage(Arc::new(transcription::CancelRegistry::default()))
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;

            // In a bundled .app, resources live under app.path().resource_dir().
            // In `cargo run` / `tauri dev`, that returns the binary's directory
            // (target/debug/), which doesn't contain bundle.resources. Fall back
            // to the source tree's src-tauri/resources/ in that case.
            let resource_dir = app.path().resource_dir()?;
            let bundled_dir = if resource_dir.join("models").join("ggml-base.bin").is_file() {
                resource_dir
            } else {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources")
            };

            // whisper.cpp's Metal backend looks for ggml-metal.metal here.
            // Without this, init falls back to CoreML+CPU and the decoder
            // loses GPU acceleration.
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

            let captures_dir = app_data_dir.join("captures");
            match storage::cleanup_orphaned_captures(&store, &captures_dir) {
                Ok(n) if n > 0 => eprintln!("storage: removed {n} orphaned capture file(s)"),
                Ok(_) => {}
                Err(e) => eprintln!("storage: capture cleanup failed (non-fatal): {e}"),
            }

            let call_tmp = capture::all_sessions_temp_dir(&app_data_dir);
            match capture::cleanup_orphaned_call_temps(&call_tmp, &std::collections::HashSet::new()) {
                Ok(n) if n > 0 => eprintln!("capture: removed {n} orphaned call-capture dir(s)"),
                Ok(_) => {}
                Err(e) => eprintln!("capture: call temp cleanup failed (non-fatal): {e}"),
            }

            app.manage(std::sync::Arc::new(store));
            app.manage(std::sync::Arc::new(capture::CallCaptureRegistry::new()));
            app.manage(std::sync::Arc::new(cancellation::RecordingCancelRegistry::default()));

            accent::install_observer(app.handle().clone());

            // Window vibrancy + reduce-transparency observer.
            if let Some(window) = app.get_webview_window("main") {
                if let Err(e) = vibrancy::set_window_transparent(&window) {
                    eprintln!("vibrancy: set_window_transparent failed: {e}");
                    let _ = app.emit(vibrancy::VIBRANCY_UNAVAILABLE_EVENT, ());
                }
                if let Err(e) = vibrancy::apply_vibrancy(&window) {
                    eprintln!("vibrancy: apply_vibrancy failed: {e}");
                    let _ = app.emit(vibrancy::VIBRANCY_UNAVAILABLE_EVENT, ());
                }
            } else {
                eprintln!("vibrancy: 'main' webview window not found at setup");
                let _ = app.emit(vibrancy::VIBRANCY_UNAVAILABLE_EVENT, ());
            }
            vibrancy::install_reduce_transparency_observer(app.handle().clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::check_dependencies,
            commands::list_ollama_models,
            commands::save_text_file,
            commands::save_binary_file,
            commands::save_capture,
            commands::read_file_as_text,
            commands::list_available_models,
            commands::download_model,
            commands::cancel_model_download,
            commands::list_projects,
            commands::create_project,
            commands::update_project,
            commands::delete_project,
            commands::list_recordings,
            commands::get_recording,
            commands::create_recording,
            commands::update_recording,
            commands::delete_recording,
            commands::start_over_recording,
            commands::cancel_recording,
            commands::record_saved_outputs,
            commands::path_exists,
            commands::start_call_capture,
            commands::pause_call_capture,
            commands::resume_call_capture,
            commands::discard_call_capture,
            commands::stop_call_capture,
            accent::get_system_accent,
            vibrancy::get_reduce_transparency,
            vibrancy::is_vibrancy_active,
            orchestrator::run_recording,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Meister");
}
