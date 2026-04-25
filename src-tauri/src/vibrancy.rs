//! Window vibrancy for macOS:
//! - `apply_vibrancy(window)` attaches an `NSVisualEffectView` of material
//!   `windowBackground` underneath the React webview.
//! - `set_window_transparent(window)` clears the window's background so the
//!   vibrancy view is what gets rendered.
//! Both are no-ops on non-macOS targets.

use tauri::{AppHandle, Emitter, Runtime, WebviewWindow};

pub const VIBRANCY_UNAVAILABLE_EVENT: &str = "vibrancy-unavailable";
pub const REDUCE_TRANSPARENCY_CHANGED_EVENT: &str = "system-reduce-transparency-changed";

use std::sync::atomic::{AtomicBool, Ordering};

/// Whether vibrancy successfully attached at boot. Default false; set true
/// only after a successful `apply_vibrancy` call.
static VIBRANCY_ACTIVE: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub fn is_vibrancy_active() -> bool {
    VIBRANCY_ACTIVE.load(Ordering::Acquire)
}

#[cfg(target_os = "macos")]
mod imp {
    use objc2::rc::Retained;
    use objc2_app_kit::{
        NSAutoresizingMaskOptions, NSColor, NSVisualEffectBlendingMode, NSVisualEffectMaterial,
        NSVisualEffectState, NSVisualEffectView, NSWindow, NSWindowOrderingMode,
    };
    use objc2_foundation::{MainThreadMarker, NSRect};

    /// SAFETY: Caller must invoke from the main thread.
    pub unsafe fn attach_vibrancy(window: &NSWindow) -> Option<Retained<NSVisualEffectView>> {
        let content_view = window.contentView()?;
        let bounds: NSRect = content_view.bounds();

        // SAFETY: Caller asserts main-thread invocation; MainThreadOnly types
        // (NSVisualEffectView) require an mtm to allocate.
        let mtm = MainThreadMarker::new_unchecked();
        let alloc = mtm.alloc::<NSVisualEffectView>();
        let view = NSVisualEffectView::initWithFrame(alloc, bounds);
        view.setMaterial(NSVisualEffectMaterial::WindowBackground);
        view.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        view.setState(NSVisualEffectState::FollowsWindowActiveState);
        view.setAutoresizingMask(
            NSAutoresizingMaskOptions::NSViewWidthSizable
                | NSAutoresizingMaskOptions::NSViewHeightSizable,
        );

        // NSVisualEffectView inherits from NSView. The method expects &NSView;
        // Deref via `&*view` performs the upcast.
        content_view.addSubview_positioned_relativeTo(
            &*view,
            NSWindowOrderingMode::NSWindowBelow,
            None,
        );

        Some(view)
    }

    pub unsafe fn make_window_transparent(window: &NSWindow) {
        window.setOpaque(false);
        let clear = NSColor::clearColor();
        window.setBackgroundColor(Some(&clear));
    }
}

#[cfg(target_os = "macos")]
pub fn apply_vibrancy<R: Runtime>(window: &WebviewWindow<R>) -> Result<(), String> {
    use objc2::rc::Retained;
    use objc2_app_kit::NSWindow;

    let raw = window
        .ns_window()
        .map_err(|e| format!("apply_vibrancy: ns_window unavailable: {e}"))?;
    if raw.is_null() {
        return Err("apply_vibrancy: ns_window returned null".into());
    }

    // SAFETY: Tauri's `WebviewWindow::ns_window()` returns a non-null pointer
    // to a valid `NSWindow` for the lifetime of the window. We're called from
    // the main thread during Tauri's setup phase. `Retained::retain` adds a +1
    // to the Cocoa refcount; releasing our `Retained` will balance it. The
    // pointer remains valid for the duration of this function's stack frame.
    let ns_window: Retained<NSWindow> = unsafe {
        Retained::retain(raw as *mut NSWindow)
            .ok_or_else(|| "apply_vibrancy: retain failed".to_string())?
    };

    unsafe {
        match imp::attach_vibrancy(&ns_window) {
            Some(_) => {
                VIBRANCY_ACTIVE.store(true, Ordering::Release);
                Ok(())
            }
            None => Err("apply_vibrancy: contentView missing".into()),
        }
    }
}

#[cfg(target_os = "macos")]
pub fn set_window_transparent<R: Runtime>(window: &WebviewWindow<R>) -> Result<(), String> {
    use objc2::rc::Retained;
    use objc2_app_kit::NSWindow;

    let raw = window
        .ns_window()
        .map_err(|e| format!("set_window_transparent: ns_window unavailable: {e}"))?;
    if raw.is_null() {
        return Err("set_window_transparent: ns_window returned null".into());
    }
    // SAFETY: Tauri's `WebviewWindow::ns_window()` returns a non-null pointer
    // to a valid `NSWindow` for the lifetime of the window. We're called from
    // the main thread during Tauri's setup phase. `Retained::retain` adds a +1
    // to the Cocoa refcount; releasing our `Retained` will balance it. The
    // pointer remains valid for the duration of this function's stack frame.
    let ns_window: Retained<NSWindow> = unsafe {
        Retained::retain(raw as *mut NSWindow)
            .ok_or_else(|| "set_window_transparent: retain failed".to_string())?
    };
    unsafe { imp::make_window_transparent(&ns_window) };
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn apply_vibrancy<R: Runtime>(_window: &WebviewWindow<R>) -> Result<(), String> {
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn set_window_transparent<R: Runtime>(_window: &WebviewWindow<R>) -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "macos")]
fn read_reduce_transparency() -> bool {
    use objc2_app_kit::NSWorkspace;
    // SAFETY: NSWorkspace.shared is process-wide and the property accessor
    // is thread-safe; can be called from any thread.
    unsafe {
        let workspace = NSWorkspace::sharedWorkspace();
        workspace.accessibilityDisplayShouldReduceTransparency()
    }
}

#[cfg(not(target_os = "macos"))]
fn read_reduce_transparency() -> bool {
    false
}

#[tauri::command]
pub fn get_reduce_transparency() -> bool {
    read_reduce_transparency()
}

#[cfg(target_os = "macos")]
pub fn install_reduce_transparency_observer<R: Runtime>(app: AppHandle<R>) {
    use block2::RcBlock;
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{ns_string, NSNotification, NSOperationQueue};
    use std::ptr::NonNull;

    let app_for_cb = app.clone();
    let block = RcBlock::new(move |_note: NonNull<NSNotification>| {
        let value = read_reduce_transparency();
        if let Err(e) = app_for_cb.emit(REDUCE_TRANSPARENCY_CHANGED_EVENT, value) {
            eprintln!("vibrancy: failed to emit reduce-transparency event: {e}");
        }
    });

    // SAFETY: NSWorkspace.notificationCenter is the per-workspace center
    // (NOT the distributed one) for accessibility notifications. The block
    // outlives the observer because we leak the returned token below.
    unsafe {
        let workspace = NSWorkspace::sharedWorkspace();
        let center = workspace.notificationCenter();
        let token = center.addObserverForName_object_queue_usingBlock(
            Some(ns_string!("NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification")),
            None,
            Some(&NSOperationQueue::mainQueue()),
            &block,
        );
        std::mem::forget(token);
    }
}

#[cfg(not(target_os = "macos"))]
pub fn install_reduce_transparency_observer<R: Runtime>(_app: AppHandle<R>) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_reduce_transparency_returns_bool() {
        // Just confirms the call doesn't panic and returns a bool.
        let _: bool = get_reduce_transparency();
    }
}
