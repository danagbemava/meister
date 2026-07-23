//! Display-sleep power assertion held for the lifetime of a capture session.
//!
//! ScreenCaptureKit capture is bound to a display (`SCContentFilter::with_display`),
//! and macOS tears the stream down when that display sleeps or locks. With the
//! default 2–10 minute display-sleep timers, an unattended call recording dies
//! silently partway through. Holding a `PreventUserIdleDisplaySleep` assertion
//! while recording — the same thing OBS and meeting apps do — keeps the display
//! (and therefore the stream) alive until the session ends.
//!
//! The assertion is visible to the user via `pmset -g assertions` under the
//! reason string passed to [`DisplaySleepAssertion::create`].

use std::ffi::{c_void, CString};
use std::os::raw::c_char;

type CFStringRef = *const c_void;
type IOPMAssertionID = u32;
type IOReturn = i32;

const CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
const IOPM_ASSERTION_LEVEL_ON: u32 = 255;
const IO_RETURN_SUCCESS: IOReturn = 0;

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFStringCreateWithCString(
        alloc: *const c_void,
        c_str: *const c_char,
        encoding: u32,
    ) -> CFStringRef;
    fn CFRelease(cf: *const c_void);
}

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOPMAssertionCreateWithName(
        assertion_type: CFStringRef,
        level: u32,
        name: CFStringRef,
        id: *mut IOPMAssertionID,
    ) -> IOReturn;
    fn IOPMAssertionRelease(id: IOPMAssertionID) -> IOReturn;
}

/// RAII guard for a `PreventUserIdleDisplaySleep` power assertion. The
/// assertion is released on drop; releasing from a different thread than the
/// creator is fine (the ID is just a token into powerd).
pub struct DisplaySleepAssertion {
    id: IOPMAssertionID,
}

impl DisplaySleepAssertion {
    /// Take the assertion. `reason` is what the user sees in Activity Monitor's
    /// Energy tab / `pmset -g assertions`, so make it self-explanatory.
    pub fn create(reason: &str) -> Result<Self, String> {
        let type_c = CString::new("PreventUserIdleDisplaySleep").expect("static str has no NUL");
        let reason_c = CString::new(reason).map_err(|_| "reason contains NUL byte".to_string())?;
        unsafe {
            let type_cf =
                CFStringCreateWithCString(std::ptr::null(), type_c.as_ptr(), CF_STRING_ENCODING_UTF8);
            let reason_cf = CFStringCreateWithCString(
                std::ptr::null(),
                reason_c.as_ptr(),
                CF_STRING_ENCODING_UTF8,
            );
            if type_cf.is_null() || reason_cf.is_null() {
                if !type_cf.is_null() {
                    CFRelease(type_cf);
                }
                if !reason_cf.is_null() {
                    CFRelease(reason_cf);
                }
                return Err("CFString creation failed".to_string());
            }
            let mut id: IOPMAssertionID = 0;
            let ret =
                IOPMAssertionCreateWithName(type_cf, IOPM_ASSERTION_LEVEL_ON, reason_cf, &mut id);
            CFRelease(type_cf);
            CFRelease(reason_cf);
            if ret == IO_RETURN_SUCCESS {
                Ok(Self { id })
            } else {
                Err(format!("IOPMAssertionCreateWithName failed: {ret:#010x}"))
            }
        }
    }
}

impl Drop for DisplaySleepAssertion {
    fn drop(&mut self) {
        unsafe {
            let _ = IOPMAssertionRelease(self.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_release_assertion() {
        let a = DisplaySleepAssertion::create("meister test assertion")
            .expect("assertion should be created");
        drop(a); // must not panic; release path exercised
    }

    #[test]
    fn reason_with_nul_is_rejected() {
        assert!(DisplaySleepAssertion::create("bad\0reason").is_err());
    }
}
