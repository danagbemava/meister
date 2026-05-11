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
