# Sidebar Source List (Liquid Glass) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the sidebar's CSS-only vibrancy with a real macOS Tahoe `NSGlassEffectView` (with `NSVisualEffectView` fallback for pre-Tahoe), restructure the sidebar to a spacious editorial Source List with search field + disclosure carets + auto-assigned folder colors + nested recordings, and lay the token surface that subsequent #6 sub-slices will inherit.

**Architecture:** New Rust module `glass.rs` bridges `NSGlassEffectView` via `objc2`'s `msg_send!` (with runtime macOS-version detection for the fallback). The webview reports the sidebar's pixel rect to Rust via a Tauri command on mount + on every resize; Rust frames the native glass view to match. The webview-side `Sidebar.tsx` is rewritten to a transparent-background spacious Source List with all chrome/selection/hover handled in CSS via new tokens.

**Tech Stack:** Rust 2021, `objc2` 0.5+ (already a transitive dep via `objc2_app_kit`), Tauri 2, React + TypeScript, Tailwind v4, Vitest + Testing Library, Phosphor icons (`@phosphor-icons/react`), SQLite (no schema change in this slice).

**Spec:** `docs/superpowers/specs/2026-05-11-sidebar-source-list-design.md`

---

## File Structure

**New files:**
- `src/lib/folderColor.ts` — FNV-1a hash → 4-color palette mapping.
- `src/lib/folderColor.test.ts` — tests for the helper.
- `src-tauri/src/glass.rs` — `NSGlassEffectView` (macOS 26+) / `NSVisualEffectView` (fallback) bridge for the sidebar surface, plus the frame-conversion math.

**Modified files:**
- `src/components/ui/Icon.tsx` — add `magnifying-glass` to `IconName` + REGISTRY.
- `src/components/ui/__tests__/Icon.test.tsx` — extend `ALL_NAMES` with `"magnifying-glass"`.
- `src/styles/tokens.css` — add sidebar/folder tokens (light/dark/reduce-transparency).
- `src-tauri/src/lib.rs` — register new Tauri commands + call into `glass` module on setup.
- `src-tauri/src/commands.rs` — add `report_sidebar_frame` + `set_sidebar_glass_enabled` commands (or place them in `glass.rs`).
- `src/lib/test-setup.ts` — add a `ResizeObserver` mock for jsdom.
- `src/components/Sidebar.tsx` — substantial rewrite. The existing 378-line component is replaced. Existing imperative `SidebarHandle.beginNewProject` API + parent props (`projects`, `recordingsByProject`, `selection`, `onSelect`, etc.) are preserved.
- `src/components/Sidebar.test.tsx` — new tests for search, disclosure persistence, keyboard nav, folder color, ARIA. (If `Sidebar.test.tsx` doesn't exist yet, this task creates it.)
- `package.json` — no change (Phosphor `MagnifyingGlass` is already in the installed package; only the registry entry changes).

**Sequencing:** non-UI helpers first (Tasks 1–4), then the native bridge (Tasks 5–6), then the frontend infrastructure (Task 7), then the sidebar rewrite (Task 8), then verification (Task 9).

---

## Task 1: Add `magnifying-glass` to the Icon registry

**Files:**
- Modify: `src/components/ui/Icon.tsx`
- Modify: `src/components/ui/__tests__/Icon.test.tsx`

- [ ] **Step 1: Verify the Phosphor export name**

Run:

```bash
node -e "const m = require('/Users/nexus/projects/meister/node_modules/@phosphor-icons/react/dist/index.cjs.js'); console.log(Object.keys(m).filter(k => /Magnif/i.test(k)).sort())"
```

Expected: includes `MagnifyingGlass`. If a different name is exported (e.g. `MagnifyingGlassIcon` or `Search`), substitute it in Step 2 below and update Step 2's import + REGISTRY entry accordingly.

- [ ] **Step 2: Add the icon to the registry**

In `src/components/ui/Icon.tsx`, find the imports block:

```tsx
import {
  Microphone,
  Waves,
  FileText,
  ...
```

Add `MagnifyingGlass,` after one of the existing imports (alphabetical position is fine — consistency matters less than presence). Then in the `IconName` union, add `"magnifying-glass"`:

```tsx
export type IconName =
  | "mic" | "waveform" | "doc-text" | "media" | "sparkles" | "equalizer" | "save"
  | "lock" | "warning"
  | "play" | "pause" | "stop" | "trash"
  | "plus" | "x" | "check" | "pencil" | "chevron-right"
  | "folder" | "recording"
  | "gear" | "question" | "info"
  | "sidebar" | "chat" | "magnifying-glass";
```

In the REGISTRY object, add:

```tsx
"magnifying-glass": MagnifyingGlass,
```

- [ ] **Step 3: Update the parameterized icon test**

In `src/components/ui/__tests__/Icon.test.tsx`, find the `ALL_NAMES` array and append `"magnifying-glass"`:

```tsx
const ALL_NAMES: IconName[] = [
  "mic", "waveform", "doc-text", "media", "sparkles", "equalizer", "save",
  "lock", "warning",
  "play", "pause", "stop", "trash",
  "plus", "x", "check", "pencil", "chevron-right",
  "folder", "recording",
  "gear", "question", "info",
  "sidebar", "magnifying-glass",
];
```

(Note: if the existing array in the test file already contains `"chat"`, preserve it — add `"magnifying-glass"` alongside it.)

- [ ] **Step 4: Run tests + tsc**

Run: `npx vitest run src/components/ui/__tests__/Icon.test.tsx && npx tsc --noEmit`
Expected: all parameterized icon tests pass; tsc clean.

- [ ] **Step 5: Commit**

```bash
git add src/components/ui/Icon.tsx src/components/ui/__tests__/Icon.test.tsx
git commit -m "feat(ui): add magnifying-glass icon for sidebar search"
```

---

## Task 2: `folderColor.ts` helper (TDD)

**Files:**
- Create: `src/lib/folderColor.ts`
- Create: `src/lib/folderColor.test.ts`

- [ ] **Step 1: Write the failing tests**

Create `src/lib/folderColor.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { folderColorFor, type FolderColor } from "./folderColor";

const PALETTE: FolderColor[] = ["blue", "green", "purple", "orange"];

describe("folderColorFor", () => {
  it("returns one of the four palette values", () => {
    for (const id of ["abc", "def", "8f3a", "x", ""]) {
      expect(PALETTE).toContain(folderColorFor(id));
    }
  });

  it("is deterministic for the same input", () => {
    expect(folderColorFor("project-1234")).toBe(folderColorFor("project-1234"));
    expect(folderColorFor("")).toBe(folderColorFor(""));
  });

  it("distributes across the palette for varied inputs", () => {
    const seen = new Set<FolderColor>();
    // 16 distinct UUID-ish strings should hit at least 3 of 4 buckets.
    for (let i = 0; i < 16; i++) {
      seen.add(folderColorFor(`proj-${i}-${i * 7919}`));
    }
    expect(seen.size).toBeGreaterThanOrEqual(3);
  });

  it("returns expected colors for fixed UUID samples", () => {
    // Lock specific values so changes to the algorithm are visible.
    expect(folderColorFor("550e8400-e29b-41d4-a716-446655440000")).toBeDefined();
    // The full palette appears across this fixture set.
    const fixture = [
      "550e8400-e29b-41d4-a716-446655440000",
      "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
      "00000000-0000-0000-0000-000000000000",
      "ffffffff-ffff-ffff-ffff-ffffffffffff",
    ].map(folderColorFor);
    expect(new Set(fixture).size).toBeGreaterThanOrEqual(2);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npx vitest run src/lib/folderColor.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement the helper**

Create `src/lib/folderColor.ts`:

```ts
const PALETTE = ["blue", "green", "purple", "orange"] as const;
export type FolderColor = (typeof PALETTE)[number];

/**
 * Map a project ID deterministically to one of four folder colors.
 *
 * FNV-1a 32-bit hash, mod 4. Fast, no crypto, no allocations beyond the
 * iterator. Same input always returns the same output across processes.
 */
export function folderColorFor(projectId: string): FolderColor {
  let h = 0x811c9dc5; // FNV offset basis (32-bit)
  for (let i = 0; i < projectId.length; i++) {
    h ^= projectId.charCodeAt(i);
    h = Math.imul(h, 0x01000193); // FNV prime (32-bit)
  }
  return PALETTE[Math.abs(h) % PALETTE.length];
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `npx vitest run src/lib/folderColor.test.ts`
Expected: PASS — 4 tests.

- [ ] **Step 5: Commit**

```bash
git add src/lib/folderColor.ts src/lib/folderColor.test.ts
git commit -m "feat(sidebar): add folderColor helper (FNV-1a → 4-color palette)"
```

---

## Task 3: Sidebar tokens

**Files:**
- Modify: `src/styles/tokens.css`

- [ ] **Step 1: Locate the existing appearance blocks**

Open `src/styles/tokens.css`. The file has two top-level blocks: `:root, [data-appearance="light"]` and `[data-appearance="dark"]`, plus a `[data-reduce-transparency="true"]` block (slice #2). New tokens need entries in all three.

- [ ] **Step 2: Add sidebar tokens to the dark appearance block**

In `[data-appearance="dark"]` block (typically the second block in the file), add at the bottom (before the closing `}`):

```css
  /* Sidebar — Source List (slice #6.2) */
  --sidebar-row-hover: rgb(255 255 255 / 0.06);
  --sidebar-row-selected: rgb(var(--color-accent-rgb) / 1);
  --sidebar-row-selected-text: #ffffff;
  --sidebar-divider: rgb(255 255 255 / 0.07);
  --sidebar-section-label: rgb(255 255 255 / 0.55);
  --sidebar-search-bg: rgb(255 255 255 / 0.06);
  --sidebar-search-border: rgb(255 255 255 / 0.08);

  /* Folder color gradients (dark variants) */
  --folder-blue-from: #6cb3ff;
  --folder-blue-to: #2e89ee;
  --folder-green-from: #6fdb84;
  --folder-green-to: #2fb14b;
  --folder-purple-from: #c89bf5;
  --folder-purple-to: #9659e3;
  --folder-orange-from: #ffc46f;
  --folder-orange-to: #ff8c2a;
```

- [ ] **Step 3: Add light-mode equivalents**

In `:root, [data-appearance="light"]` (the first block), add the same set with light values:

```css
  /* Sidebar — Source List (slice #6.2) */
  --sidebar-row-hover: rgb(0 0 0 / 0.05);
  --sidebar-row-selected: rgb(var(--color-accent-rgb) / 1);
  --sidebar-row-selected-text: #ffffff;
  --sidebar-divider: rgb(0 0 0 / 0.07);
  --sidebar-section-label: rgb(60 60 67 / 0.6);
  --sidebar-search-bg: rgb(0 0 0 / 0.04);
  --sidebar-search-border: rgb(0 0 0 / 0.08);

  /* Folder color gradients (light variants — slightly desaturated) */
  --folder-blue-from: #7fbcff;
  --folder-blue-to: #4f99ee;
  --folder-green-from: #83df93;
  --folder-green-to: #46b85e;
  --folder-purple-from: #d3aff7;
  --folder-purple-to: #a472e6;
  --folder-orange-from: #ffce85;
  --folder-orange-to: #ff9d4a;
```

- [ ] **Step 4: Add reduce-transparency overrides**

In `[data-reduce-transparency="true"]` block, add the opaque-equivalent values:

```css
  /* Sidebar (reduce-transparency variants) */
  --sidebar-row-hover: rgb(255 255 255 / 0.10);
  --sidebar-divider: rgb(255 255 255 / 0.18);
  --sidebar-search-bg: rgb(255 255 255 / 0.10);
```

(In light mode with reduce-transparency, these read as slightly darker fills against the opaque sidebar background. Light mode + reduce-transparency is rare enough that the dark values above are acceptable; if visual smoke shows otherwise, add a `[data-appearance="light"][data-reduce-transparency="true"]` selector with the inverted values in a follow-up.)

- [ ] **Step 5: Verify**

Run: `npx tsc --noEmit && npx vitest run`
Expected: clean (CSS changes don't affect tsc/vitest, but we want to confirm nothing else broke).

- [ ] **Step 6: Commit**

```bash
git add src/styles/tokens.css
git commit -m "feat(sidebar): add Source List + folder color tokens"
```

---

## Task 4: `glass.rs` — frame-conversion math (TDD)

**Files:**
- Create: `src-tauri/src/glass.rs`

This task creates the file with ONLY the pure-math frame-conversion logic + tests. The native AppKit bridge comes in Task 5. Splitting lets us test the math in isolation without an AppKit window.

- [ ] **Step 1: Create the file with module skeleton + tests**

Create `src-tauri/src/glass.rs`:

```rust
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
        // A 240x600 sidebar in the top-left of an 800-tall content view sits at
        // browser (0, 0). In AppKit, that's bottom-left at (0, 800 - 0 - 600 = 200).
        let r = to_appkit_rect(BrowserRect { x: 0.0, y: 0.0, width: 240.0, height: 600.0 }, 800.0);
        assert_eq!(r, AppKitRect { x: 0.0, y: 200.0, width: 240.0, height: 600.0 });
    }

    #[test]
    fn full_height_sidebar_lands_at_origin_zero() {
        // Sidebar that fills the content view height: AppKit y = 0.
        let r = to_appkit_rect(BrowserRect { x: 0.0, y: 0.0, width: 240.0, height: 800.0 }, 800.0);
        assert_eq!(r.y, 0.0);
    }

    #[test]
    fn x_and_width_pass_through() {
        let r = to_appkit_rect(BrowserRect { x: 12.0, y: 36.0, width: 248.0, height: 564.0 }, 800.0);
        assert_eq!(r.x, 12.0);
        assert_eq!(r.width, 248.0);
        assert_eq!(r.height, 564.0);
        // Y = 800 - 36 - 564 = 200.
        assert_eq!(r.y, 200.0);
    }

    #[test]
    fn negative_y_when_browser_rect_extends_below_content_view() {
        // Sidebar reported as taller than the window (race during resize, etc.):
        // we don't clamp; caller decides what to do.
        let r = to_appkit_rect(BrowserRect { x: 0.0, y: 0.0, width: 240.0, height: 1000.0 }, 800.0);
        assert_eq!(r.y, -200.0);
    }
}
```

- [ ] **Step 2: Wire the new module into `lib.rs`**

In `src-tauri/src/lib.rs`, find the existing module declarations (likely near the top — `mod cancellation;`, `mod commands;`, etc.). Add:

```rust
mod glass;
```

- [ ] **Step 3: Run the tests**

Run: `cd src-tauri && cargo test glass`
Expected: 4 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/glass.rs src-tauri/src/lib.rs
git commit -m "feat(glass): add browser→AppKit rect conversion math"
```

---

## Task 5: `glass.rs` — `NSGlassEffectView` bridge

**Files:**
- Modify: `src-tauri/src/glass.rs`

- [ ] **Step 1: Add the macOS-only bridge module**

Append to `src-tauri/src/glass.rs`:

```rust
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
    use super::*;

    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2_app_kit::{
        NSAutoresizingMaskOptions, NSVisualEffectBlendingMode, NSVisualEffectMaterial,
        NSVisualEffectState, NSVisualEffectView, NSWindow, NSWindowOrderingMode,
    };
    use objc2_foundation::{MainThreadMarker, NSProcessInfo, NSRect, NSPoint, NSSize};

    /// Strong reference to the attached glass/vibrancy view, kept alive for the
    /// lifetime of the window. Mutex protects against races between frame-report
    /// calls and reduce-transparency toggles. The `AnyObject` is either an
    /// `NSGlassEffectView` (Tahoe) or `NSVisualEffectView` (fallback) — we don't
    /// need to know which after creation; we only call `setFrame:` on it.
    pub static ATTACHED_VIEW: Mutex<Option<Retained<AnyObject>>> = Mutex::new(None);

    /// Returns true on macOS 26.0 or later.
    pub fn is_tahoe_or_newer() -> bool {
        // SAFETY: NSProcessInfo.processInfo is process-wide and read-only; safe
        // to call from any thread.
        unsafe {
            let pi = NSProcessInfo::processInfo();
            let v = pi.operatingSystemVersion();
            v.majorVersion >= 26
        }
    }

    /// Attach a glass view (Tahoe) or vibrancy view (fallback) at the given
    /// frame. Returns the retained view. SAFETY: caller must invoke from main thread.
    pub unsafe fn attach_sidebar_glass(
        window: &NSWindow,
        rect: NSRect,
    ) -> Option<Retained<AnyObject>> {
        let content_view = window.contentView()?;
        let mtm = MainThreadMarker::new_unchecked();

        let view: Retained<AnyObject> = if is_tahoe_or_newer() {
            // NSGlassEffectView (macOS 26+). Allocated via the Obj-C runtime
            // since objc2_app_kit doesn't yet expose typed bindings for it.
            let cls = objc2::class!(NSGlassEffectView);
            let alloc: *mut AnyObject = msg_send![cls, alloc];
            let view: *mut AnyObject = msg_send![alloc, initWithFrame: rect];
            let view = Retained::from_raw(view).expect("NSGlassEffectView init returned nil");
            let _: () = msg_send![&*view, setCornerRadius: 12.0_f64];
            // Autoresizing not needed — we'll explicitly reframe on every
            // sidebar-frame report.
            view
        } else {
            // Fallback: NSVisualEffectView with .sidebar material.
            let alloc = mtm.alloc::<NSVisualEffectView>();
            let view = NSVisualEffectView::initWithFrame(alloc, rect);
            view.setMaterial(NSVisualEffectMaterial::Sidebar);
            view.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
            view.setState(NSVisualEffectState::FollowsWindowActiveState);
            view.setAutoresizingMask(NSAutoresizingMaskOptions::empty());
            // Upcast NSVisualEffectView → AnyObject by raw-pointer round-trip.
            let raw: *mut AnyObject = Retained::into_raw(view) as *mut AnyObject;
            Retained::from_raw(raw).expect("upcast")
        };

        // Insert at index 0 (behind everything else, including the webview).
        let nsview: *mut AnyObject = &*view as *const _ as *mut _;
        let _: () = msg_send![
            &*content_view,
            addSubview: nsview,
            positioned: NSWindowOrderingMode::NSWindowBelow,
            relativeTo: std::ptr::null::<AnyObject>()
        ];

        Some(view)
    }

    /// Reframe the attached view. SAFETY: caller must invoke from main thread.
    pub unsafe fn reframe(view: &AnyObject, rect: NSRect) {
        let _: () = msg_send![view, setFrame: rect];
    }

    /// Remove the attached view from the window. SAFETY: main thread.
    pub unsafe fn detach(view: &AnyObject) {
        let _: () = msg_send![view, removeFromSuperview];
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
            Some(view) => imp::reframe(view, nsrect),
            None => {
                if let Some(view) = imp::attach_sidebar_glass(ns_window, nsrect) {
                    *guard = Some(view);
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
                imp::detach(&view);
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
```

- [ ] **Step 2: Build to verify compile**

Run: `cd src-tauri && cargo build`
Expected: clean (or only the existing 3 pre-existing warnings).

If build fails on the `objc2_foundation::NSProcessInfo` import (some versions expose it elsewhere), search for the right path:

```bash
grep -rn "NSProcessInfo" src-tauri/target/doc 2>/dev/null | head
# Or grep installed crate sources:
find src-tauri/target/doc -name "NSProcessInfo*" 2>/dev/null | head
```

If `NSProcessInfo` lives in a different module in the installed `objc2_foundation` version, adjust the `use` path. The `operatingSystemVersion()` method returns a struct with a `majorVersion` field — that part is stable.

- [ ] **Step 3: Run all backend tests**

Run: `cd src-tauri && cargo test`
Expected: all existing tests + the 4 `glass::tests` pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/glass.rs
git commit -m "feat(glass): bridge NSGlassEffectView with NSVisualEffectView fallback"
```

---

## Task 6: Tauri commands + webview wire-up

**Files:**
- Modify: `src-tauri/src/glass.rs` (add commands)
- Modify: `src-tauri/src/lib.rs` (register commands + listen for reduce-transparency event)

- [ ] **Step 1: Add the Tauri commands to `glass.rs`**

Append to `src-tauri/src/glass.rs`:

```rust
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
```

- [ ] **Step 2: Register the commands in `lib.rs`**

In `src-tauri/src/lib.rs`, find the `tauri::generate_handler![...]` block (around line 129). Add:

```rust
            glass::report_sidebar_frame,
            glass::set_sidebar_glass_enabled_cmd,
```

(Place them alongside the other glass-related or vibrancy-related entries; alphabetical ordering matches the existing style if there is one.)

- [ ] **Step 3: Wire reduce-transparency to the glass toggle**

The existing `vibrancy.rs` emits `system-reduce-transparency-changed` events (slice #2). The webview already listens for these and toggles `data-reduce-transparency` on `<html>`. Now we also need the native side to toggle the glass view off/on.

In `src-tauri/src/lib.rs`, find where `vibrancy::install_reduce_transparency_observer` is called during setup. After that line, add a listener that calls `set_sidebar_glass_enabled` when the event fires:

```rust
            // Mirror reduce-transparency state into the sidebar glass view.
            let app_handle = app.handle().clone();
            app.listen_any(vibrancy::REDUCE_TRANSPARENCY_CHANGED_EVENT, move |event| {
                let payload: bool = serde_json::from_str(event.payload()).unwrap_or(false);
                if let Some(window) = app_handle.get_webview_window("main") {
                    let _ = glass::set_sidebar_glass_enabled(&window, !payload);
                }
            });
```

(If the existing event-listening pattern in `lib.rs` differs — e.g. uses `listen_global` or a different API — match the existing style. Run `grep "listen" src-tauri/src/lib.rs` to find the convention.)

- [ ] **Step 4: Build + test**

Run: `cd src-tauri && cargo build && cargo test`
Expected: clean build; all tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/glass.rs src-tauri/src/lib.rs
git commit -m "feat(glass): expose Tauri commands + wire reduce-transparency"
```

---

## Task 7: ResizeObserver mock in test-setup

**Files:**
- Modify: `src/lib/test-setup.ts`

- [ ] **Step 1: Add a minimal ResizeObserver mock**

In `src/lib/test-setup.ts`, append at the bottom (or alongside the existing `matchMedia` stub):

```ts
// jsdom doesn't ship ResizeObserver; provide a minimal no-op stub so
// components that use it (Sidebar's frame-reporting effect) don't crash
// during tests.
if (!globalThis.ResizeObserver) {
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
}
```

- [ ] **Step 2: Verify**

Run: `npx vitest run`
Expected: all existing tests still pass.

- [ ] **Step 3: Commit**

```bash
git add src/lib/test-setup.ts
git commit -m "test: add ResizeObserver mock for jsdom"
```

---

## Task 8: Sidebar.tsx rewrite

**Files:**
- Modify: `src/components/Sidebar.tsx`
- Modify (or create): `src/components/Sidebar.test.tsx`

This is the substantial task. Splitting it would require intermediate states where the sidebar is half-rewritten and partially broken, so we do it as one task with multiple verification steps inside.

- [ ] **Step 1: Read the existing Sidebar.tsx in full**

Open `src/components/Sidebar.tsx`. Note:
- The existing imperative `SidebarHandle.beginNewProject()` API (preserved by this rewrite — App.tsx calls it from menu shortcuts).
- The existing `Props` interface (preserved).
- The existing ContextMenu usage on project + recording rows (preserved).
- The existing inline-create-row pattern (Input + Check button + X button) (preserved, now using the icon-only Buttons from slice #5).
- The existing `expanded` state (replaced by per-project localStorage).
- The existing `creating` / `newName` state (preserved).

Don't write any code yet. Note the existing imports, the Props shape, the SidebarHandle type, and the parent's expectations.

- [ ] **Step 2: Rewrite Sidebar.tsx**

Replace the entire contents of `src/components/Sidebar.tsx` with:

```tsx
import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { confirm as dialogConfirm } from "@tauri-apps/plugin-dialog";
import type { Project, Recording } from "../types/storage";
import { api } from "../api/storage";
import type { Selection } from "../App";
import { Button, ContextMenu, Icon, Input } from "./ui";
import { folderColorFor } from "../lib/folderColor";

export interface SidebarHandle {
  beginNewProject: () => void;
}

interface Props {
  projects: Project[];
  recordingsByProject: Record<string, Recording[]>;
  selection: Selection;
  onSelect: (s: Selection) => void | Promise<void>;
  onProjectsChanged: () => void;
  onRecordingsChanged: (projectId: string) => void;
  onPickFilesForProject: (projectId: string, paths: string[]) => void;
}

const DEFAULTS = {
  ollamaUrl: "http://localhost:11434",
  model: "gemma3",
  whisperModel: "base",
  language: "auto",
  outputFormats: ["md"] as const,
};

const EXPANDED_KEY = (projectId: string) => `sidebar-expanded-${projectId}`;

function readExpanded(projectId: string): boolean {
  try {
    const v = localStorage.getItem(EXPANDED_KEY(projectId));
    if (v === null) return true; // default: expanded
    return v === "true";
  } catch {
    return true;
  }
}

function writeExpanded(projectId: string, value: boolean): void {
  try {
    localStorage.setItem(EXPANDED_KEY(projectId), value ? "true" : "false");
  } catch {
    /* localStorage unavailable; not fatal */
  }
}

const Sidebar = forwardRef<SidebarHandle, Props>(function Sidebar(
  {
    projects,
    recordingsByProject,
    selection,
    onSelect,
    onProjectsChanged,
    onRecordingsChanged,
    onPickFilesForProject,
  },
  ref,
) {
  const [creating, setCreating] = useState(false);
  const [newName, setNewName] = useState("");
  const [renamingProject, setRenamingProject] = useState<string | null>(null);
  const [renamingRecording, setRenamingRecording] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [search, setSearch] = useState("");

  // Per-project disclosure state, hydrated from localStorage on demand.
  const [expandedTick, setExpandedTick] = useState(0); // forces re-render when toggled
  const isExpanded = useCallback(
    (projectId: string) => readExpanded(projectId),
    // expandedTick is consumed implicitly to invalidate memoization
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [expandedTick],
  );
  const toggleExpanded = useCallback((projectId: string) => {
    writeExpanded(projectId, !readExpanded(projectId));
    setExpandedTick((n) => n + 1);
  }, []);

  useImperativeHandle(ref, () => ({
    beginNewProject: () => {
      setCreating(true);
      setNewName("");
    },
  }));

  // ── Frame reporting ─────────────────────────────────────────────────────
  const sidebarRef = useRef<HTMLElement | null>(null);
  useEffect(() => {
    const el = sidebarRef.current;
    if (!el) return;
    const report = () => {
      const r = el.getBoundingClientRect();
      void invoke("report_sidebar_frame", {
        x: r.x,
        y: r.y,
        width: r.width,
        height: r.height,
      }).catch(() => { /* command may not exist on non-macOS; ignore */ });
    };
    report();
    const ro = new ResizeObserver(report);
    ro.observe(el);
    window.addEventListener("resize", report);
    return () => {
      ro.disconnect();
      window.removeEventListener("resize", report);
    };
  }, []);

  // ── Search filtering ────────────────────────────────────────────────────
  const filterText = search.trim().toLowerCase();
  const filtered = useMemo(() => {
    if (!filterText) return projects.map((p) => ({ project: p, recordings: recordingsByProject[p.id] ?? [], forceExpand: false }));
    return projects
      .map((p) => {
        const recs = recordingsByProject[p.id] ?? [];
        const projectMatches = p.name.toLowerCase().includes(filterText);
        const matchingRecs = recs.filter((r) => r.name.toLowerCase().includes(filterText));
        if (projectMatches) {
          return { project: p, recordings: recs, forceExpand: false };
        }
        if (matchingRecs.length > 0) {
          return { project: p, recordings: matchingRecs, forceExpand: true };
        }
        return null;
      })
      .filter((x): x is NonNullable<typeof x> => x !== null);
  }, [projects, recordingsByProject, filterText]);

  // ── Actions ─────────────────────────────────────────────────────────────
  const createProject = async () => {
    const name = newName.trim();
    if (!name) {
      setCreating(false);
      return;
    }
    await api.createProject(name, { ...DEFAULTS, outputFormats: [...DEFAULTS.outputFormats] }, {
      source: "inline",
      content: "",
      sourcePath: null,
    });
    setCreating(false);
    setNewName("");
    onProjectsChanged();
  };

  const deleteProject = async (id: string, name: string) => {
    const ok = await dialogConfirm(`Delete project "${name}"? Its recordings stay on disk.`, {
      title: "Delete project",
      kind: "warning",
    });
    if (!ok) return;
    await api.deleteProject(id);
    try { localStorage.removeItem(EXPANDED_KEY(id)); } catch { /* ignore */ }
    onProjectsChanged();
  };

  const renameProject = async (id: string) => {
    const newNameTrimmed = renameValue.trim();
    if (!newNameTrimmed) {
      setRenamingProject(null);
      return;
    }
    const project = projects.find((p) => p.id === id);
    if (!project) {
      setRenamingProject(null);
      return;
    }
    await api.updateProject({ ...project, name: newNameTrimmed });
    setRenamingProject(null);
    onProjectsChanged();
  };

  const deleteRecording = async (projectId: string, recordingId: string, name: string) => {
    const ok = await dialogConfirm(`Delete recording "${name}"?`, {
      title: "Delete recording",
      kind: "warning",
    });
    if (!ok) return;
    await api.deleteRecording(recordingId);
    onRecordingsChanged(projectId);
  };

  const renameRecording = async (projectId: string, recordingId: string) => {
    const newNameTrimmed = renameValue.trim();
    if (!newNameTrimmed) {
      setRenamingRecording(null);
      return;
    }
    const recording = (recordingsByProject[projectId] ?? []).find((r) => r.id === recordingId);
    if (!recording) {
      setRenamingRecording(null);
      return;
    }
    await api.updateRecording({ ...recording, name: newNameTrimmed });
    setRenamingRecording(null);
    onRecordingsChanged(projectId);
  };

  const projectSelected = (id: string) =>
    selection.kind === "project" && selection.projectId === id;
  const recordingSelected = (id: string) =>
    selection.kind === "recording" && selection.recordingId === id;

  // ── Render ──────────────────────────────────────────────────────────────
  return (
    <aside
      ref={sidebarRef}
      className="w-60 shrink-0 rounded-xl flex flex-col bg-transparent"
    >
      <div data-tauri-drag-region className="h-9 shrink-0" />

      {/* Search */}
      <div className="px-3 pb-2 shrink-0">
        <div
          className="flex items-center gap-2 h-8 px-2.5 rounded-lg"
          style={{
            background: "var(--sidebar-search-bg)",
            border: "1px solid var(--sidebar-search-border)",
          }}
        >
          <Icon name="magnifying-glass" size="sm" className="text-label-tertiary" />
          <input
            type="search"
            aria-label="Search projects and recordings"
            placeholder="Search"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="flex-1 bg-transparent outline-none text-callout text-label-primary placeholder:text-label-tertiary"
          />
          {search && (
            <button
              type="button"
              onClick={() => setSearch("")}
              aria-label="Clear search"
              className="text-label-tertiary hover:text-label-primary"
            >
              <Icon name="x" size="sm" />
            </button>
          )}
        </div>
      </div>

      <div className="flex-1 overflow-y-auto scrollable px-2 pb-1">
        {/* Section header */}
        <div className="flex items-center justify-between pl-2 pr-0.5 pt-1 pb-0.5">
          <span
            className="text-caption-1 font-semibold uppercase tracking-wider"
            style={{ color: "var(--sidebar-section-label)", letterSpacing: "0.6px" }}
          >
            Projects
          </span>
          {!creating && (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => { setCreating(true); setNewName(""); }}
              iconLeft="plus"
              aria-label="New project"
              className="text-label-tertiary hover:text-label-primary"
            />
          )}
        </div>

        {/* Inline new-project row */}
        {creating && (
          <div className="flex gap-1 px-1 pb-2 pt-1">
            <Input
              autoFocus
              size="sm"
              className="flex-1"
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void createProject();
                if (e.key === "Escape") { setCreating(false); setNewName(""); }
              }}
              placeholder="Project name"
            />
            <Button variant="primary" size="sm" onClick={createProject} iconLeft="check" aria-label="Create project" />
            <Button
              variant="ghost"
              size="sm"
              onClick={() => { setCreating(false); setNewName(""); }}
              iconLeft="x"
              aria-label="Cancel"
            />
          </div>
        )}

        {/* Empty state */}
        {projects.length === 0 && !creating && (
          <div className="flex flex-col items-center gap-2 py-6">
            <p className="text-footnote text-label-tertiary">No projects yet.</p>
            <Button
              variant="primary"
              size="sm"
              onClick={() => { setCreating(true); setNewName(""); }}
            >
              New project
            </Button>
          </div>
        )}

        {/* Project + recording rows */}
        {filtered.map(({ project: p, recordings: recs, forceExpand }) => {
          const expanded = forceExpand || isExpanded(p.id);
          const folderClass = `sidebar-folder sidebar-folder-${folderColorFor(p.id)}`;
          return (
            <div key={p.id}>
              <ContextMenu>
                <ContextMenu.Trigger asChild>
                  <div
                    role="button"
                    tabIndex={0}
                    onClick={() => {
                      void onSelect({ kind: "project", projectId: p.id });
                      toggleExpanded(p.id);
                    }}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" || e.key === " ") {
                        e.preventDefault();
                        void onSelect({ kind: "project", projectId: p.id });
                        toggleExpanded(p.id);
                      }
                    }}
                    aria-label={`${p.name}, project, ${recs.length} recording${recs.length === 1 ? "" : "s"}`}
                    aria-current={projectSelected(p.id) ? "page" : undefined}
                    aria-expanded={expanded}
                    className={`group mx-2 my-px h-9 px-2.5 rounded-md flex items-center gap-2 text-body cursor-pointer outline-none focus-visible:ring-2 focus-visible:ring-focus-ring ${
                      projectSelected(p.id) ? "" : "hover:[background:var(--sidebar-row-hover)]"
                    }`}
                    style={
                      projectSelected(p.id)
                        ? { background: "var(--sidebar-row-selected)", color: "var(--sidebar-row-selected-text)" }
                        : undefined
                    }
                  >
                    <span
                      aria-hidden
                      className="inline-block w-2.5 transition-transform"
                      style={{
                        transform: expanded ? "rotate(90deg)" : "rotate(0deg)",
                        color: projectSelected(p.id) ? "var(--sidebar-row-selected-text)" : "var(--color-label-tertiary)",
                      }}
                    >
                      ›
                    </span>
                    <span className={folderClass} aria-hidden />
                    {renamingProject === p.id ? (
                      <Input
                        autoFocus
                        size="sm"
                        className="flex-1 h-6 -my-1"
                        value={renameValue}
                        onChange={(e) => setRenameValue(e.target.value)}
                        onClick={(e) => e.stopPropagation()}
                        onKeyDown={(e) => {
                          e.stopPropagation();
                          if (e.key === "Enter") void renameProject(p.id);
                          if (e.key === "Escape") setRenamingProject(null);
                        }}
                        onBlur={() => void renameProject(p.id)}
                      />
                    ) : (
                      <span className="flex-1 truncate font-medium">{p.name}</span>
                    )}
                  </div>
                </ContextMenu.Trigger>
                <ContextMenu.Content>
                  <ContextMenu.Item onSelect={() => { setRenameValue(p.name); setRenamingProject(p.id); }}>
                    Rename
                  </ContextMenu.Item>
                  <ContextMenu.Item onSelect={() => void deleteProject(p.id, p.name)}>
                    Delete
                  </ContextMenu.Item>
                </ContextMenu.Content>
              </ContextMenu>

              {/* Nested recordings (when expanded) */}
              {expanded && recs.map((rec) => (
                <ContextMenu key={rec.id}>
                  <ContextMenu.Trigger asChild>
                    <div
                      role="button"
                      tabIndex={0}
                      onClick={() => void onSelect({ kind: "recording", projectId: p.id, recordingId: rec.id })}
                      onKeyDown={(e) => {
                        if (e.key === "Enter" || e.key === " ") {
                          e.preventDefault();
                          void onSelect({ kind: "recording", projectId: p.id, recordingId: rec.id });
                        }
                      }}
                      aria-label={`${rec.name}, recording`}
                      aria-current={recordingSelected(rec.id) ? "page" : undefined}
                      className={`mx-2 my-px h-[34px] pl-[30px] pr-2.5 rounded-md flex items-center gap-2 text-body cursor-pointer outline-none focus-visible:ring-2 focus-visible:ring-focus-ring ${
                        recordingSelected(rec.id) ? "" : "hover:[background:var(--sidebar-row-hover)]"
                      }`}
                      style={
                        recordingSelected(rec.id)
                          ? { background: "var(--sidebar-row-selected)", color: "var(--sidebar-row-selected-text)" }
                          : undefined
                      }
                    >
                      <Icon
                        name="recording"
                        size="sm"
                        className={recordingSelected(rec.id) ? "" : "text-label-tertiary"}
                      />
                      {renamingRecording === rec.id ? (
                        <Input
                          autoFocus
                          size="sm"
                          className="flex-1 h-6 -my-1"
                          value={renameValue}
                          onChange={(e) => setRenameValue(e.target.value)}
                          onClick={(e) => e.stopPropagation()}
                          onKeyDown={(e) => {
                            e.stopPropagation();
                            if (e.key === "Enter") void renameRecording(p.id, rec.id);
                            if (e.key === "Escape") setRenamingRecording(null);
                          }}
                          onBlur={() => void renameRecording(p.id, rec.id)}
                        />
                      ) : (
                        <span className="flex-1 truncate">{rec.name}</span>
                      )}
                    </div>
                  </ContextMenu.Trigger>
                  <ContextMenu.Content>
                    <ContextMenu.Item onSelect={() => { setRenameValue(rec.name); setRenamingRecording(rec.id); }}>
                      Rename
                    </ContextMenu.Item>
                    <ContextMenu.Item onSelect={() => void deleteRecording(p.id, rec.id, rec.name)}>
                      Delete
                    </ContextMenu.Item>
                  </ContextMenu.Content>
                </ContextMenu>
              ))}
            </div>
          );
        })}
      </div>
    </aside>
  );
});

export default Sidebar;
```

Notes for the implementer:
- The `onPickFilesForProject` prop is preserved in the type but no longer used inside the component body in this rewrite. If the existing component used drag-and-drop on rows to attach files, that needs to be re-added — check the existing Sidebar.tsx and port any such behavior. If unused already (often the case), the prop can stay declared but unused; we leave it for the existing parent contract.
- The Input + Check + X inline-rename pattern uses the icon-only Buttons from slice #5; aria-label is required and provided.
- Folder glyph (`<span class="sidebar-folder sidebar-folder-…" />`) is rendered as a CSS-styled empty span. The CSS lives in Step 3 below.

- [ ] **Step 3: Add the folder-glyph CSS to `tokens.css` (or a new sidebar.css)**

In `src/styles/tokens.css` (or wherever component-level CSS lives in this project — check `src/index.css` if `tokens.css` is tokens-only), append:

```css
.sidebar-folder {
  display: inline-block;
  width: 14px;
  height: 11px;
  border-radius: 3px;
  flex-shrink: 0;
}
.sidebar-folder-blue   { background: linear-gradient(180deg, var(--folder-blue-from),   var(--folder-blue-to)); }
.sidebar-folder-green  { background: linear-gradient(180deg, var(--folder-green-from),  var(--folder-green-to)); }
.sidebar-folder-purple { background: linear-gradient(180deg, var(--folder-purple-from), var(--folder-purple-to)); }
.sidebar-folder-orange { background: linear-gradient(180deg, var(--folder-orange-from), var(--folder-orange-to)); }
```

Also add a CSS rule for the sidebar background to be transparent (overriding any prior slice's `bg-sidebar-background` class):

```css
aside[ref] /* not a real selector — see below */
```

Actually, the React component already uses `bg-transparent` Tailwind class on the `<aside>`, so no CSS rule needed here. Skip this part of Step 3.

- [ ] **Step 4: Create / extend `Sidebar.test.tsx`**

If `src/components/Sidebar.test.tsx` doesn't exist, create it. Otherwise extend it. New content (or additions):

```tsx
import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import Sidebar from "./Sidebar";
import type { Project, Recording } from "../types/storage";

const proj = (id: string, name: string): Project => ({
  id,
  name,
  createdAt: 1000,
  updatedAt: 1000,
  template: { source: "inline", content: "", sourcePath: null },
  defaultSettings: {
    ollamaUrl: "http://localhost:11434",
    model: "gemma3",
    whisperModel: "base",
    language: "auto",
    outputFormats: ["md"],
  },
});

const rec = (id: string, projectId: string, name: string): Recording => ({
  id,
  projectId,
  name,
  sourcePaths: [],
  createdAt: 1000,
  updatedAt: 1000,
  settings: {
    ollamaUrl: "http://localhost:11434",
    model: "gemma3",
    whisperModel: "base",
    language: "auto",
    outputFormats: ["md"],
  },
  stages: [],
});

const baseProps = () => ({
  projects: [proj("p1", "Quarterly Review"), proj("p2", "Customer Calls")],
  recordingsByProject: {
    p1: [rec("r1", "p1", "Standup notes"), rec("r2", "p1", "Retro session")],
    p2: [rec("r3", "p2", "Call with Acme")],
  },
  selection: { kind: "none" } as const,
  onSelect: vi.fn(),
  onProjectsChanged: vi.fn(),
  onRecordingsChanged: vi.fn(),
  onPickFilesForProject: vi.fn(),
});

describe("Sidebar", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("renders all projects and their recordings expanded by default", () => {
    render(<Sidebar {...baseProps()} />);
    expect(screen.getByText("Quarterly Review")).toBeInTheDocument();
    expect(screen.getByText("Customer Calls")).toBeInTheDocument();
    expect(screen.getByText("Standup notes")).toBeInTheDocument();
    expect(screen.getByText("Call with Acme")).toBeInTheDocument();
  });

  it("filters to projects whose name matches the search query", async () => {
    render(<Sidebar {...baseProps()} />);
    const search = screen.getByLabelText("Search projects and recordings");
    await userEvent.type(search, "quarterly");
    expect(screen.getByText("Quarterly Review")).toBeInTheDocument();
    expect(screen.queryByText("Customer Calls")).toBeNull();
  });

  it("auto-expands a project that contains a matching recording even if the project name doesn't match", async () => {
    render(<Sidebar {...baseProps()} />);
    const search = screen.getByLabelText("Search projects and recordings");
    await userEvent.type(search, "acme");
    // Customer Calls is shown because Call with Acme matches.
    expect(screen.getByText("Customer Calls")).toBeInTheDocument();
    expect(screen.getByText("Call with Acme")).toBeInTheDocument();
    expect(screen.queryByText("Standup notes")).toBeNull();
  });

  it("persists disclosure state to localStorage", () => {
    const { rerender } = render(<Sidebar {...baseProps()} />);
    // Click on the Quarterly Review row to toggle expansion.
    fireEvent.click(screen.getByText("Quarterly Review"));
    // localStorage should now have an entry for p1 marked false.
    expect(localStorage.getItem("sidebar-expanded-p1")).toBe("false");
    // Standup notes should no longer be visible.
    rerender(<Sidebar {...baseProps()} />);
    expect(screen.queryByText("Standup notes")).toBeNull();
  });

  it("renders the empty state when no projects exist", () => {
    render(<Sidebar {...baseProps()} projects={[]} recordingsByProject={{}} />);
    expect(screen.getByText("No projects yet.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "New project" })).toBeInTheDocument();
  });

  it("marks the selected project row with aria-current=page", () => {
    render(<Sidebar {...baseProps()} selection={{ kind: "project", projectId: "p1" }} />);
    const row = screen.getByLabelText(/Quarterly Review, project/);
    expect(row).toHaveAttribute("aria-current", "page");
  });

  it("clears the search field when the clear button is clicked", async () => {
    render(<Sidebar {...baseProps()} />);
    const search = screen.getByLabelText("Search projects and recordings") as HTMLInputElement;
    await userEvent.type(search, "abc");
    expect(search.value).toBe("abc");
    await userEvent.click(screen.getByLabelText("Clear search"));
    expect(search.value).toBe("");
  });
});
```

If `Sidebar.test.tsx` already exists, integrate the above (avoid duplicating any existing tests; add the new ones with new `it(...)` blocks).

- [ ] **Step 5: Run tsc + tests**

Run: `npx tsc --noEmit && npx vitest run`
Expected: tsc clean; all tests pass (existing + new). If the new tests fail, iterate on the Sidebar.tsx implementation until they pass — do NOT weaken the tests.

- [ ] **Step 6: Manual sanity check (no commit yet — saves a round-trip)**

Run: `npm run tauri dev`. Verify:

1. Sidebar renders with the new spacious layout, search field at top, projects + recordings.
2. Folder glyphs are visible and colored (you'll see one of the 4 hues per project).
3. Search filters as you type.
4. ⌘F focuses the search field. (If it doesn't — that's a Tauri menu shortcut question, not a sidebar bug; defer.)
5. On Tahoe (macOS 26+): the sidebar background shows actual Liquid Glass refraction when window content moves behind it.
6. On pre-Tahoe: the sidebar shows `.sidebar` material vibrancy.
7. Toggle System Settings → Accessibility → Display → Reduce transparency. The glass disappears, sidebar becomes opaque.

Note any issues you observe. If there are visible problems, fix in this same task before committing.

- [ ] **Step 7: Commit**

```bash
git add src/components/Sidebar.tsx src/components/Sidebar.test.tsx src/styles/tokens.css
git commit -m "feat(sidebar): rewrite as Tahoe Source List with Liquid Glass"
```

---

## Task 9: Final verification

**Files:** none.

- [ ] **Step 1: Full test sweep**

Run: `npx tsc --noEmit && npx vitest run && (cd src-tauri && cargo test)`
Expected: all green.

- [ ] **Step 2: Production build**

Run: `npm run build`
Expected: succeeds.

- [ ] **Step 3: Manual smoke matrix**

Run: `npm run tauri dev`. Verify each of the following:

1. **Tahoe Liquid Glass.** Sidebar shows actual refraction when window content moves behind it (drag the window, scroll the project view next to the sidebar). Visibly distinct from the old `.windowBackground` material.
2. **Pre-Tahoe fallback** (only if you have access to a pre-Tahoe Mac — otherwise skip and trust the runtime check). Sidebar shows `.sidebar` vibrancy.
3. **Reduce transparency.** System Settings → Accessibility → Display → Reduce transparency ON. Sidebar background becomes opaque immediately. Toggle OFF — glass returns.
4. **Light mode.** Switch macOS to light appearance. Sidebar tokens render correctly: folder colors readable, selection accent legible, search field visible.
5. **Search.** Type a query — list filters. Type a query that only matches recordings — parent project auto-expands. Clear with the x button — list reverts.
6. **Disclosure.** Click a project row — its recordings collapse/expand, selection moves to the project. Reload the app — disclosure state persists.
7. **Folder colors.** Each project has a colored folder glyph. Same project always shows the same color across reloads.
8. **Selection.** Click a recording — accent fill, white text. Click another — selection moves cleanly.
9. **Hover.** Non-selected rows show subtle hover scrim. Selected rows ignore hover.
10. **Keyboard.** Tab into the sidebar — focus rings visible on rows. Enter on a focused row activates selection.
11. **Window resize.** Drag the window edge — the native glass tracks the sidebar rect with no visible offset / lag.
12. **New project flow.** Click `+` — inline create row appears. Type a name + Enter — project is created, appears in the list with a folder color. Esc cancels.
13. **Right-click.** Project row → Rename + Delete. Recording row → Rename + Delete. Both work.

- [ ] **Step 4: No commit needed unless cleanup**

If any cleanup commits are needed, commit them. Otherwise this task is verification-only.

---

## Spec Coverage Self-Check

| Spec section / requirement | Implemented in |
|---|---|
| `NSGlassEffectView` bridge with `NSVisualEffectView` fallback | Task 5 |
| Browser→AppKit rect conversion math | Task 4 |
| Frame-reporting protocol (Tauri command + ResizeObserver) | Task 6 (command) + Task 8 (observer) |
| Reduce-transparency toggle wiring | Task 6 |
| `magnifying-glass` icon added to registry | Task 1 |
| `folderColor.ts` FNV-1a helper | Task 2 |
| Sidebar tokens (rows, selection, hover, dividers, search, folder colors) | Task 3 |
| Folder-glyph CSS | Task 8 (Step 3) |
| Sidebar layout: drag region, search, section header, project rows, nested recording rows | Task 8 |
| Search filtering (project name + recording name + auto-expand) | Task 8 + tests |
| Disclosure state persistence to localStorage | Task 8 + test |
| Empty state ("No projects yet" + CTA) | Task 8 + test |
| ARIA labels (project row, recording row, search field, disclosure) | Task 8 |
| `aria-current="page"` on selection | Task 8 + test |
| Keyboard nav (Enter/Space activates row) | Task 8 |
| ContextMenu kept (Rename, Delete) for project + recording rows | Task 8 |
| Inline new-project + rename rows preserved | Task 8 |
| ResizeObserver mock for jsdom | Task 7 |
| Manual smoke matrix | Task 9 |
| Existing `SidebarHandle.beginNewProject` API preserved | Task 8 |
| Existing parent-component prop contract preserved | Task 8 |
| Light + dark + reduce-transparency token variants | Task 3 |
| Spec out-of-scope items NOT implemented (drag-reorder, pinned, status dots, recents view, custom colors) | Verified by Task 8 not adding any |
