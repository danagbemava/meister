//! Native macOS menu bar for Meister. Custom menu items emit Tauri events
//! prefixed with `menu:` so the frontend can dispatch them.

use tauri::menu::{
    AboutMetadataBuilder, Menu, MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder,
};
use tauri::{AppHandle, Runtime};

pub fn build_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    // App menu (Meister)
    let about_meta = AboutMetadataBuilder::new()
        .name(Some("Meister"))
        .version(Some(env!("CARGO_PKG_VERSION")))
        .build();

    let preferences = MenuItemBuilder::new("Settings…")
        .id("menu:settings")
        .accelerator("CmdOrCtrl+,")
        .build(app)?;

    let app_submenu = SubmenuBuilder::new(app, "Meister")
        .item(&PredefinedMenuItem::about(app, Some("About Meister"), Some(about_meta))?)
        .separator()
        .item(&preferences)
        .separator()
        .item(&PredefinedMenuItem::services(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::hide(app, None)?)
        .item(&PredefinedMenuItem::hide_others(app, None)?)
        .item(&PredefinedMenuItem::show_all(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::quit(app, None)?)
        .build()?;

    // File
    let new_project = MenuItemBuilder::new("New Project")
        .id("menu:new-project")
        .accelerator("CmdOrCtrl+N")
        .build(app)?;
    let import_audio = MenuItemBuilder::new("Import Audio…")
        .id("menu:import-audio")
        .accelerator("CmdOrCtrl+Shift+I")
        .build(app)?;
    let export = MenuItemBuilder::new("Export…")
        .id("menu:export")
        .accelerator("CmdOrCtrl+E")
        .build(app)?;

    let file_submenu = SubmenuBuilder::new(app, "File")
        .item(&new_project)
        .item(&import_audio)
        .separator()
        .item(&export)
        .separator()
        .item(&PredefinedMenuItem::close_window(app, None)?)
        .build()?;

    // Edit
    let edit_submenu = SubmenuBuilder::new(app, "Edit")
        .item(&PredefinedMenuItem::undo(app, None)?)
        .item(&PredefinedMenuItem::redo(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::cut(app, None)?)
        .item(&PredefinedMenuItem::copy(app, None)?)
        .item(&PredefinedMenuItem::paste(app, None)?)
        .item(&PredefinedMenuItem::select_all(app, None)?)
        .build()?;

    // View
    let toggle_sidebar = MenuItemBuilder::new("Toggle Sidebar")
        .id("menu:toggle-sidebar")
        .accelerator("Control+CmdOrCtrl+S")
        .build(app)?;

    let view_submenu = SubmenuBuilder::new(app, "View")
        .item(&toggle_sidebar)
        .build()?;

    // Recording (app-specific)
    let run_pipeline = MenuItemBuilder::new("Run")
        .id("menu:run-pipeline")
        .accelerator("CmdOrCtrl+R")
        .build(app)?;

    let recording_submenu = SubmenuBuilder::new(app, "Recording")
        .item(&run_pipeline)
        .build()?;

    // Window
    // Note: Tauri 2.x PredefinedMenuItem has no `bring_all_to_front`; we drop
    // it rather than substitute. Meister is single-window today so it isn't
    // meaningful. Fullscreen would belong in View per macOS HIG, not Window;
    // omitted from this slice.
    let window_submenu = SubmenuBuilder::new(app, "Window")
        .item(&PredefinedMenuItem::minimize(app, None)?)
        .item(&PredefinedMenuItem::maximize(app, None)?)
        .build()?;

    // Help (placeholder)
    let help_submenu = SubmenuBuilder::new(app, "Help").build()?;

    let menu = MenuBuilder::new(app)
        .items(&[
            &app_submenu,
            &file_submenu,
            &edit_submenu,
            &view_submenu,
            &recording_submenu,
            &window_submenu,
            &help_submenu,
        ])
        .build()?;

    Ok(menu)
}
