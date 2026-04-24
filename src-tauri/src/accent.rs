//! System accent color: read `NSColor.controlAccentColor` resolved
//! against both light and dark appearances, and emit a Tauri event
//! when the user changes accent or system appearance.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};

pub const ACCENT_CHANGED_EVENT: &str = "system-accent-changed";

#[derive(Debug, Clone, Serialize)]
pub struct AccentColors {
    /// [r, g, b] in 0..=255, resolved against aqua (light) appearance.
    pub light: [u8; 3],
    /// [r, g, b] in 0..=255, resolved against darkAqua appearance.
    pub dark: [u8; 3],
}

#[cfg(target_os = "macos")]
mod imp {
    use super::AccentColors;
    use block2::RcBlock;
    use objc2_app_kit::{
        NSAppearance, NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSColor, NSColorSpace,
    };
    use std::cell::Cell;

    // Apple's default blue fallback — used when AppKit returns something unexpected
    // (e.g. no sRGB conversion). Matches the non-macOS branch for consistency.
    const FALLBACK_LIGHT: [u8; 3] = [0, 122, 255];
    const FALLBACK_DARK: [u8; 3] = [10, 132, 255];

    fn cgfloat_to_u8(v: f64) -> u8 {
        let v = if v.is_nan() { 0.0 } else { v };
        let clamped = v.clamp(0.0, 1.0);
        (clamped * 255.0).round() as u8
    }

    /// Read `NSColor.controlAccentColor` under the given appearance and return
    /// the sRGB [r, g, b] triple.
    ///
    /// SAFETY: All calls into AppKit must happen on the main thread. Callers
    /// must only invoke this from the main thread (e.g. from the Tauri command
    /// handler, which runs on the main thread by default).
    unsafe fn read_for_appearance(appearance: &NSAppearance, fallback: [u8; 3]) -> [u8; 3] {
        let result: Cell<Option<[u8; 3]>> = Cell::new(None);
        // The block captures a pointer to the Cell so it can write the result.
        // Because performAsCurrentDrawingAppearance is synchronous, this is
        // safe — the block only runs during the call below.
        let result_ptr: *const Cell<Option<[u8; 3]>> = &result;

        let block = RcBlock::new(move || {
            let color = NSColor::controlAccentColor();
            let srgb_space = NSColorSpace::sRGBColorSpace();
            let converted = color.colorUsingColorSpace(&srgb_space);
            let rgb = match converted {
                Some(c) => {
                    let r = c.redComponent();
                    let g = c.greenComponent();
                    let b = c.blueComponent();
                    [cgfloat_to_u8(r), cgfloat_to_u8(g), cgfloat_to_u8(b)]
                }
                None => fallback,
            };
            // SAFETY: `result_ptr` is valid for the lifetime of this block
            // (the enclosing function), and the block runs synchronously.
            unsafe {
                (*result_ptr).set(Some(rgb));
            }
        });

        appearance.performAsCurrentDrawingAppearance(&block);

        result.get().unwrap_or(fallback)
    }

    pub fn read_accent_colors() -> AccentColors {
        // SAFETY: AppKit must be used on the main thread. Tauri command
        // handlers run on the main thread by default, and `emit_current` is
        // only called from the setup closure / main-thread observers.
        unsafe {
            let aqua = NSAppearance::appearanceNamed(NSAppearanceNameAqua);
            let dark = NSAppearance::appearanceNamed(NSAppearanceNameDarkAqua);

            let light = match aqua.as_deref() {
                Some(a) => read_for_appearance(a, FALLBACK_LIGHT),
                None => FALLBACK_LIGHT,
            };
            let dark = match dark.as_deref() {
                Some(d) => read_for_appearance(d, FALLBACK_DARK),
                None => FALLBACK_DARK,
            };

            AccentColors { light, dark }
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::AccentColors;
    pub fn read_accent_colors() -> AccentColors {
        AccentColors {
            light: [0, 122, 255],
            dark: [10, 132, 255],
        }
    }
}

#[tauri::command]
pub fn get_system_accent() -> AccentColors {
    imp::read_accent_colors()
}

/// Emit the current accent colors on the `system-accent-changed` channel.
#[allow(dead_code)]
pub fn emit_current<R: Runtime>(app: &AppHandle<R>) {
    if let Err(e) = app.emit(ACCENT_CHANGED_EVENT, imp::read_accent_colors()) {
        eprintln!("accent: failed to emit {ACCENT_CHANGED_EVENT}: {e}");
    }
}

/// Install a macOS-only observer that re-reads accent colors and emits
/// `system-accent-changed` whenever the user changes the system accent
/// or toggles Light/Dark appearance.
///
/// No-op on non-macOS.
#[cfg(target_os = "macos")]
pub fn install_observer<R: Runtime>(app: AppHandle<R>) {
    observer::install(app);
}

#[cfg(not(target_os = "macos"))]
pub fn install_observer<R: Runtime>(_app: AppHandle<R>) {}

#[cfg(target_os = "macos")]
mod observer {
    use super::{emit_current, ACCENT_CHANGED_EVENT};
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::NSObject;
    use objc2_foundation::{
        NSDistributedNotificationCenter, NSNotification, NSNotificationName, NSString,
    };
    use std::ptr::NonNull;
    use tauri::{AppHandle, Runtime};

    // These are Apple's distributed notification names, not exposed as
    // constants in the AppKit headers — we construct NSStrings for them.
    const ACCENT_NOTIFICATION: &str = "AppleColorPreferencesChangedNotification";
    const APPEARANCE_NOTIFICATION: &str = "AppleInterfaceThemeChangedNotification";

    /// Install observers on the shared `NSDistributedNotificationCenter`.
    ///
    /// SAFETY: Uses AppKit via objc2. Must be called from the main thread,
    /// which Tauri's setup closure is.
    pub fn install<R: Runtime>(app: AppHandle<R>) {
        unsafe {
            let center = NSDistributedNotificationCenter::defaultCenter();

            // The observer callback may fire on any thread. We hop to the
            // main thread before reading AppKit colors, since
            // `read_accent_colors` (and NSColor / NSAppearance) require it.
            let add = |name: &str| -> Retained<NSObject> {
                let app = app.clone();
                let ns_name: Retained<NSNotificationName> = NSString::from_str(name);

                let block = RcBlock::new(move |_note: NonNull<NSNotification>| {
                    let app_for_thread = app.clone();
                    // run_on_main_thread is safe to call from any thread.
                    if let Err(e) = app.run_on_main_thread(move || {
                        emit_current(&app_for_thread);
                    }) {
                        eprintln!("accent: run_on_main_thread failed for {ACCENT_CHANGED_EVENT}: {e}");
                    }
                });

                center.addObserverForName_object_queue_usingBlock(
                    Some(&ns_name),
                    None,
                    None,
                    &block,
                )
            };

            let token_accent = add(ACCENT_NOTIFICATION);
            let token_appearance = add(APPEARANCE_NOTIFICATION);

            // Intentionally leak the observer tokens so they live for the
            // duration of the process. The system cleans them up on exit.
            std::mem::forget(token_accent);
            std::mem::forget(token_appearance);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_in_range_triplets() {
        let c = imp::read_accent_colors();
        for rgb in [c.light, c.dark] {
            for v in rgb {
                assert!(v <= 255);
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn non_macos_returns_apple_blue() {
        let c = imp::read_accent_colors();
        assert_eq!(c.light, [0, 122, 255]);
        assert_eq!(c.dark, [10, 132, 255]);
    }
}
