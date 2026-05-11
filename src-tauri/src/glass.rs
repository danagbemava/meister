//! `NSGlassEffectView` (macOS 26+) bridge for the sidebar surface, with
//! `NSVisualEffectView` (`.sidebar` material) as the pre-Tahoe fallback.
//!
//! The webview reports the sidebar's CSS-pixel rect to Rust via the
//! `report_sidebar_frame` Tauri command. We convert browser coordinates
//! (top-left origin) to AppKit coordinates (bottom-left origin) and frame
//! the native glass view to match.

/// Browser-coords (top-left origin) sidebar rect, in CSS pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BrowserRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// AppKit-coords (bottom-left origin) sidebar rect, in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AppKitRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Convert a browser-coords rect into AppKit-coords given the content view's
/// total height. Y is inverted around the content view's height; x/width/height
/// pass through unchanged.
pub fn to_appkit_rect(browser: BrowserRect, content_height: f64) -> AppKitRect {
    AppKitRect {
        x: browser.x,
        y: content_height - browser.y - browser.height,
        width: browser.width,
        height: browser.height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appkit_origin_is_inverted_from_browser_origin() {
        let r = to_appkit_rect(BrowserRect { x: 0.0, y: 0.0, width: 240.0, height: 600.0 }, 800.0);
        assert_eq!(r, AppKitRect { x: 0.0, y: 200.0, width: 240.0, height: 600.0 });
    }

    #[test]
    fn full_height_sidebar_lands_at_origin_zero() {
        let r = to_appkit_rect(BrowserRect { x: 0.0, y: 0.0, width: 240.0, height: 800.0 }, 800.0);
        assert_eq!(r.y, 0.0);
    }

    #[test]
    fn x_and_width_pass_through() {
        let r = to_appkit_rect(BrowserRect { x: 12.0, y: 36.0, width: 248.0, height: 564.0 }, 800.0);
        assert_eq!(r.x, 12.0);
        assert_eq!(r.width, 248.0);
        assert_eq!(r.height, 564.0);
        assert_eq!(r.y, 200.0);
    }

    #[test]
    fn negative_y_when_browser_rect_extends_below_content_view() {
        let r = to_appkit_rect(BrowserRect { x: 0.0, y: 0.0, width: 240.0, height: 1000.0 }, 800.0);
        assert_eq!(r.y, -200.0);
    }
}

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use tauri::{Runtime, WebviewWindow};

/// Whether the native glass view is currently attached. Used to decide
/// between "create + frame" and "just frame" on each frame-report call.
static GLASS_ATTACHED: AtomicBool = AtomicBool::new(false);

/// Whether reduce-transparency is currently in effect. When true, we skip
/// glass attachment entirely and let the webview-side opaque tokens take over.
static REDUCE_TRANSPARENCY: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "macos")]
mod imp {
    use super::{AppKitRect, Mutex};

    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2_app_kit::{
        NSAutoresizingMaskOptions, NSView, NSVisualEffectBlendingMode, NSVisualEffectMaterial,
        NSVisualEffectState, NSVisualEffectView, NSWindow, NSWindowOrderingMode,
    };
    use objc2_foundation::{MainThreadMarker, NSProcessInfo, NSPoint, NSRect, NSSize};

    /// Wrapper that asserts `Send`/`Sync` for our `Retained<NSView>`. `NSView`
    /// is a MainThreadOnly type, so its `Retained` is neither `Send` nor `Sync`
    /// by default. Every access in this module happens on the main thread
    /// (Tauri command handlers run on the main thread for macOS), so we
    /// statically promise the compiler that the static is safe to share.
    pub struct MainThreadView(pub Retained<NSView>);

    // SAFETY: We only ever access the inner `Retained<NSView>` from the main
    // thread. The `Mutex` adds no additional thread-safety beyond serializing
    // the *takes*; the AppKit calls themselves are not thread-safe.
    unsafe impl Send for MainThreadView {}
    unsafe impl Sync for MainThreadView {}

    /// Strong reference to the attached glass/vibrancy view, kept alive for the
    /// lifetime of the window. Mutex protects against races between frame-report
    /// calls and reduce-transparency toggles. The stored value is `Retained<NSView>`
    /// — both `NSGlassEffectView` and `NSVisualEffectView` derive from `NSView`,
    /// so we use the typed `NSView` API (setFrame, removeFromSuperview,
    /// addSubview:positioned:relativeTo:) regardless of which one is attached.
    pub static ATTACHED_VIEW: Mutex<Option<MainThreadView>> = Mutex::new(None);

    /// Returns true on macOS 26.0 or later.
    pub fn is_tahoe_or_newer() -> bool {
        // SAFETY: NSProcessInfo.processInfo is process-wide and read-only;
        // operatingSystemVersion is thread-safe.
        let pi = NSProcessInfo::processInfo();
        let v = pi.operatingSystemVersion();
        v.majorVersion >= 26
    }

    /// Attach a glass view (Tahoe) or vibrancy view (fallback) at the given
    /// frame. Returns the retained view (as an `NSView`).
    ///
    /// SAFETY: caller must invoke from the main thread.
    pub unsafe fn attach_sidebar_glass(
        window: &NSWindow,
        rect: NSRect,
    ) -> Option<Retained<NSView>> {
        let content_view = window.contentView()?;

        let view: Retained<NSView> = if is_tahoe_or_newer() {
            // NSGlassEffectView (macOS 26+). Allocated via the Obj-C runtime
            // since objc2_app_kit doesn't yet expose typed bindings for it.
            // `AnyClass::get` is a runtime lookup, so the call site is safe even
            // when the class is unavailable; the version guard above ensures we
            // only reach here on systems where the class should exist.
            let cls: &AnyClass = AnyClass::get("NSGlassEffectView")
                .expect("NSGlassEffectView class missing on Tahoe+");
            let alloc: *mut AnyObject = msg_send![cls, alloc];
            let view_ptr: *mut AnyObject = msg_send![alloc, initWithFrame: rect];
            assert!(!view_ptr.is_null(), "NSGlassEffectView init returned nil");
            let _: () = msg_send![view_ptr, setCornerRadius: 12.0_f64];
            // NSGlassEffectView is an NSView subclass; reinterpret the pointer.
            Retained::from_raw(view_ptr as *mut NSView)
                .expect("NSGlassEffectView retain failed")
        } else {
            // Fallback: NSVisualEffectView with .sidebar material.
            let mtm = MainThreadMarker::new_unchecked();
            let alloc = mtm.alloc::<NSVisualEffectView>();
            let veview = NSVisualEffectView::initWithFrame(alloc, rect);
            veview.setMaterial(NSVisualEffectMaterial::Sidebar);
            veview.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
            veview.setState(NSVisualEffectState::FollowsWindowActiveState);
            veview.setAutoresizingMask(NSAutoresizingMaskOptions::empty());
            // NSVisualEffectView → NSView upcast via raw-pointer round-trip.
            let raw: *mut NSView = Retained::into_raw(veview) as *mut NSView;
            Retained::from_raw(raw).expect("NSVisualEffectView upcast failed")
        };

        // Insert at the bottom of the subview stack (behind the webview).
        content_view.addSubview_positioned_relativeTo(
            &view,
            NSWindowOrderingMode::NSWindowBelow,
            None,
        );

        Some(view)
    }

    /// Reframe the attached view.
    ///
    /// SAFETY: caller must invoke from the main thread.
    pub unsafe fn reframe(view: &NSView, rect: NSRect) {
        view.setFrame(rect);
    }

    /// Remove the attached view from the window.
    ///
    /// SAFETY: caller must invoke from the main thread.
    pub unsafe fn detach(view: &NSView) {
        view.removeFromSuperview();
    }

    /// Build an NSRect from our `AppKitRect` newtype.
    pub fn nsrect_for(r: AppKitRect) -> NSRect {
        NSRect {
            origin: NSPoint { x: r.x, y: r.y },
            size: NSSize { width: r.width, height: r.height },
        }
    }
}

#[cfg(target_os = "macos")]
fn with_ns_window<R: Runtime, T>(
    window: &WebviewWindow<R>,
    f: impl FnOnce(&objc2_app_kit::NSWindow) -> T,
) -> Result<T, String> {
    use objc2::rc::Retained;
    use objc2_app_kit::NSWindow;
    let raw = window
        .ns_window()
        .map_err(|e| format!("ns_window unavailable: {e}"))?;
    if raw.is_null() {
        return Err("ns_window returned null".into());
    }
    // SAFETY: Tauri's `ns_window()` returns a non-null pointer to a valid
    // NSWindow for the lifetime of the window. We're invoked from the main
    // thread (Tauri command handler).
    let ns_window: Retained<NSWindow> = unsafe {
        Retained::retain(raw as *mut NSWindow).ok_or_else(|| "retain failed".to_string())?
    };
    Ok(f(&ns_window))
}

/// Apply or update the sidebar glass to match the given browser-coords rect.
/// First call attaches; subsequent calls reframe. No-op when reduce-transparency
/// is in effect.
#[cfg(target_os = "macos")]
pub fn apply_or_update_sidebar_glass<R: Runtime>(
    window: &WebviewWindow<R>,
    rect: BrowserRect,
) -> Result<(), String> {
    if REDUCE_TRANSPARENCY.load(Ordering::Acquire) {
        return Ok(());
    }
    with_ns_window(window, |ns_window| unsafe {
        let content_height = ns_window
            .contentView()
            .map(|v| v.bounds().size.height)
            .unwrap_or(0.0);
        let appkit = to_appkit_rect(rect, content_height);
        let nsrect = imp::nsrect_for(appkit);

        let mut guard = imp::ATTACHED_VIEW.lock().unwrap();
        match guard.as_ref() {
            Some(view) => imp::reframe(&view.0, nsrect),
            None => {
                if let Some(view) = imp::attach_sidebar_glass(ns_window, nsrect) {
                    *guard = Some(imp::MainThreadView(view));
                    GLASS_ATTACHED.store(true, Ordering::Release);
                }
            }
        }
    })?;
    Ok(())
}

/// Toggle the sidebar glass on/off. When `enabled = false`, removes the
/// attached view; subsequent `apply_or_update_sidebar_glass` calls become
/// no-ops until re-enabled.
#[cfg(target_os = "macos")]
pub fn set_sidebar_glass_enabled<R: Runtime>(
    window: &WebviewWindow<R>,
    enabled: bool,
) -> Result<(), String> {
    REDUCE_TRANSPARENCY.store(!enabled, Ordering::Release);
    if !enabled {
        with_ns_window(window, |_| unsafe {
            let mut guard = imp::ATTACHED_VIEW.lock().unwrap();
            if let Some(view) = guard.take() {
                imp::detach(&view.0);
                GLASS_ATTACHED.store(false, Ordering::Release);
            }
        })?;
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn apply_or_update_sidebar_glass<R: Runtime>(
    _window: &WebviewWindow<R>,
    _rect: BrowserRect,
) -> Result<(), String> {
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn set_sidebar_glass_enabled<R: Runtime>(
    _window: &WebviewWindow<R>,
    _enabled: bool,
) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub fn report_sidebar_frame<R: Runtime>(
    window: WebviewWindow<R>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    apply_or_update_sidebar_glass(&window, BrowserRect { x, y, width, height })
}

#[tauri::command]
pub fn set_sidebar_glass_enabled_cmd<R: Runtime>(
    window: WebviewWindow<R>,
    enabled: bool,
) -> Result<(), String> {
    set_sidebar_glass_enabled(&window, enabled)
}
