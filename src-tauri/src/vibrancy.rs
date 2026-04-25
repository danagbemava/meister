//! Window vibrancy for macOS:
//! - `apply_vibrancy(window)` attaches an `NSVisualEffectView` of material
//!   `windowBackground` underneath the React webview.
//! - `set_window_transparent(window)` clears the window's background so the
//!   vibrancy view is what gets rendered.
//! Both are no-ops on non-macOS targets.

use tauri::{Runtime, WebviewWindow};

pub const VIBRANCY_UNAVAILABLE_EVENT: &str = "vibrancy-unavailable";

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

    let ns_window: Retained<NSWindow> = unsafe {
        Retained::retain(raw as *mut NSWindow)
            .ok_or_else(|| "apply_vibrancy: retain failed".to_string())?
    };

    unsafe {
        match imp::attach_vibrancy(&ns_window) {
            Some(_) => Ok(()),
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
