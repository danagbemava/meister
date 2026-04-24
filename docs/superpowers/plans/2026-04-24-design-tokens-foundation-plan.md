# Design Tokens & Light/Dark Foundation — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Spec:** `docs/superpowers/specs/2026-04-24-design-tokens-foundation-design.md`

**Goal:** Replace Meister's hardcoded Tailwind palette with a semantic token layer keyed to Apple's NSColor vocabulary, add a complete light-mode palette, follow the system's appearance and accent color live, and introduce an HIG-aligned typography scale.

**Architecture:** CSS variables in `src/styles/tokens.css` expose light+dark palettes toggled by `[data-appearance]`; Tailwind reads those variables so class names like `bg-window-background`, `text-label-primary`, `border-separator` work directly. A small Rust module observes `NSColor.controlAccentColor` and emits an event whenever the user changes accent or appearance. Frontend hooks write the accent triplet into a CSS variable at runtime.

**Tech Stack:** React 18, TypeScript, Tailwind CSS 3, Tauri 2 (Rust), `objc2` / `objc2-app-kit` / `objc2-foundation` for AppKit FFI, Vitest (new) for frontend tests.

---

## File Structure

**Create:**
- `src/styles/tokens.css` — CSS variables for light + dark palettes, typography tokens.
- `src/lib/appearance.ts` — `initAppearance()`; sets and tracks `data-appearance` from `prefers-color-scheme`.
- `src/lib/appearance.test.ts` — Vitest tests for appearance module.
- `src/lib/accent.ts` — `initAccent()`; fetches accent from Rust, subscribes to updates, writes CSS variables.
- `src/lib/accent.test.ts` — Vitest tests for accent module.
- `src/lib/test-setup.ts` — jsdom setup + Tauri mocks for Vitest.
- `vitest.config.ts` — Vitest configuration.
- `src-tauri/src/accent.rs` — AppKit-backed accent command + observer (macOS-only; stub on other platforms).

**Modify:**
- `src/main.tsx` — import `tokens.css`, call `initAppearance()` and `initAccent()` before render.
- `src/index.css` — remove hardcoded `#0a84ff` / `#48484a` / `#636366` / `@apply bg-surface text-white`; use new tokens.
- `tailwind.config.js` — add full token catalog; remove old `surface.*`/`accent.*` entries at end of migration.
- `package.json` — add vitest + jsdom dev dependencies; add `test` script.
- `src-tauri/Cargo.toml` — add `objc2`, `objc2-foundation`, `objc2-app-kit` as macOS-targeted deps.
- `src-tauri/src/lib.rs` — register `accent` module, `get_system_accent` command, and install the observer in setup.
- All 13 components (one group per task): `TitleBar.tsx`, `Sidebar.tsx`, `ProjectView.tsx`, `RecordingView.tsx`, `RecordCard.tsx`, `RecordCallCard.tsx`, `StageCard.tsx`, `OutputPreview.tsx`, `StagingList.tsx`, `RecordingDropZone.tsx`, `SettingsForm.tsx`, `TemplateEditor.tsx`, `DownloadModelModal.tsx`.

---

## Task 0: Capture pre-migration screenshots

**Files:**
- Create: `docs/superpowers/artifacts/2026-04-24-pre-migration/` (git-ignored folder for reference screenshots)

- [ ] **Step 1: Run the app in dev mode**

```bash
npm run tauri dev
```

Wait for the window to open.

- [ ] **Step 2: Take reference screenshots of every relevant state**

Using macOS ⌘⇧4 (or Screenshot.app), capture each of these into `docs/superpowers/artifacts/2026-04-24-pre-migration/`:

- `empty-state.png` — app with no projects.
- `project-view.png` — a project with one existing recording and the staging area visible.
- `recording-view-active.png` — a recording mid-pipeline (at least one stage running, one pending).
- `recording-view-done.png` — a recording that finished, with `OutputPreview` visible.
- `settings.png` — `SettingsForm` expanded.
- `download-modal.png` — `DownloadModelModal` open.
- `record-card.png` — live mic `RecordCard` in the recording state.

These are the dark-mode baseline the migration tasks must not visibly alter.

- [ ] **Step 3: Add artifacts folder to .gitignore**

In `.gitignore`, append:

```
docs/superpowers/artifacts/
```

- [ ] **Step 4: Commit the .gitignore change**

```bash
git add .gitignore
git commit -m "chore: ignore design-review screenshot artifacts"
```

---

## Task 1: Set up Vitest

**Files:**
- Modify: `package.json`
- Create: `vitest.config.ts`
- Create: `src/lib/test-setup.ts`

- [ ] **Step 1: Install Vitest + jsdom**

```bash
npm install --save-dev vitest@^1 jsdom@^24 @vitest/ui@^1
```

- [ ] **Step 2: Add `test` and `test:run` scripts**

In `package.json` `scripts`, add:

```json
"test": "vitest",
"test:run": "vitest run"
```

- [ ] **Step 3: Create `vitest.config.ts`**

```ts
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    setupFiles: ["./src/lib/test-setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
  },
});
```

- [ ] **Step 4: Create `src/lib/test-setup.ts`**

```ts
import { vi } from "vitest";

// Tauri invoke/event mocks — tests override per-case.
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

// jsdom ships matchMedia as undefined; stub a minimal impl.
if (!window.matchMedia) {
  window.matchMedia = (query: string) =>
    ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      dispatchEvent: () => false,
    }) as MediaQueryList;
}
```

- [ ] **Step 5: Confirm Vitest runs (no tests yet)**

```bash
npm run test:run
```

Expected: `No test files found` exit code 0 (or the same message with exit 0 depending on Vitest version). If non-zero exit on "no tests," accept that — we'll have tests in Task 3.

- [ ] **Step 6: Commit**

```bash
git add package.json package-lock.json vitest.config.ts src/lib/test-setup.ts
git commit -m "chore: add vitest with jsdom for frontend tests"
```

---

## Task 2: Create `tokens.css` with the full palette

**Files:**
- Create: `src/styles/tokens.css`

- [ ] **Step 1: Write `src/styles/tokens.css`**

```css
/*
 * Design tokens — macOS semantic color + typography foundation.
 *
 * Opaque tokens are stored as space-separated rgb triplets so Tailwind's
 * `<alpha-value>` modifier works (e.g. `bg-accent/20`). Inherent-alpha
 * tokens are stored as full rgba() strings; alpha-modifier is not supported.
 *
 * Light is the default on :root; dark is applied by [data-appearance="dark"].
 * --color-accent-rgb is a placeholder; src/lib/accent.ts overrides it at boot.
 */

:root,
[data-appearance="light"] {
  /* Backgrounds (opaque) */
  --color-window-background-rgb:      236 236 236;
  --color-under-page-background-rgb:  150 150 150;
  --color-control-background-rgb:     255 255 255;
  --color-sidebar-background-rgb:     232 232 232;

  /* Labels (inherent alpha) */
  --color-label-primary:     rgba(0 0 0 / 0.85);
  --color-label-secondary:   rgba(0 0 0 / 0.55);
  --color-label-tertiary:    rgba(0 0 0 / 0.25);
  --color-label-quaternary:  rgba(0 0 0 / 0.10);
  --color-text-placeholder:  rgba(0 0 0 / 0.30);

  /* Separators & fills (inherent alpha) */
  --color-separator:         rgba(0 0 0 / 0.10);
  --color-fill-primary:      rgba(120 120 128 / 0.20);
  --color-fill-secondary:    rgba(120 120 128 / 0.16);
  --color-fill-tertiary:     rgba(118 118 128 / 0.12);
  --color-fill-quaternary:   rgba(116 116 128 / 0.08);

  /* Controls */
  --color-control-text:              rgba(0 0 0 / 0.85);
  --color-control-text-disabled:     rgba(0 0 0 / 0.25);
  --color-control-selected-text-rgb: 255 255 255;

  /* Accent — overridden by src/lib/accent.ts */
  --color-accent-rgb:         0 122 255;
  --color-accent-hover-rgb:   42 142 255;
  --color-accent-pressed-rgb: 0 107 229;
  --color-accent-label-rgb:   255 255 255;

  /* Status (opaque) */
  --color-status-success-rgb: 40 205 65;
  --color-status-warning-rgb: 255 149 0;
  --color-status-error-rgb:   255 59 48;
  --color-status-info-rgb:    0 122 255;
}

[data-appearance="dark"] {
  /* Backgrounds */
  --color-window-background-rgb:      30 30 30;
  --color-under-page-background-rgb:  40 40 40;
  --color-control-background-rgb:     30 30 30;
  --color-sidebar-background-rgb:     37 37 39;

  /* Labels */
  --color-label-primary:     rgba(255 255 255 / 0.85);
  --color-label-secondary:   rgba(255 255 255 / 0.55);
  --color-label-tertiary:    rgba(255 255 255 / 0.25);
  --color-label-quaternary:  rgba(255 255 255 / 0.10);
  --color-text-placeholder:  rgba(255 255 255 / 0.30);

  /* Separators & fills */
  --color-separator:        rgba(255 255 255 / 0.12);
  --color-fill-primary:     rgba(120 120 128 / 0.36);
  --color-fill-secondary:   rgba(120 120 128 / 0.32);
  --color-fill-tertiary:    rgba(118 118 128 / 0.27);
  --color-fill-quaternary:  rgba(118 118 128 / 0.23);

  /* Controls */
  --color-control-text:              rgba(255 255 255 / 0.85);
  --color-control-text-disabled:     rgba(255 255 255 / 0.25);
  --color-control-selected-text-rgb: 255 255 255;

  /* Accent — overridden at runtime */
  --color-accent-rgb:         10 132 255;
  --color-accent-hover-rgb:   58 156 255;
  --color-accent-pressed-rgb: 0 113 229;
  --color-accent-label-rgb:   255 255 255;

  /* Status */
  --color-status-success-rgb: 50 215 75;
  --color-status-warning-rgb: 255 159 10;
  --color-status-error-rgb:   255 69 58;
  --color-status-info-rgb:    10 132 255;
}

/* Typography scale — Apple macOS HIG. Classes: text-{name}. */
/* Tailwind reads these via fontSize entries; line-heights in the same var. */
```

- [ ] **Step 2: Commit**

```bash
git add src/styles/tokens.css
git commit -m "feat(tokens): add semantic color variables for light and dark"
```

---

## Task 3: Extend `tailwind.config.js` with new tokens (old tokens kept temporarily)

**Files:**
- Modify: `tailwind.config.js`

- [ ] **Step 1: Replace `tailwind.config.js` with extended theme**

```js
/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      fontFamily: {
        sans: ["-apple-system", "BlinkMacSystemFont", "SF Pro Display", "Segoe UI", "sans-serif"],
        mono: ["SF Mono", "JetBrains Mono", "Fira Code", "monospace"],
      },
      fontSize: {
        // macOS HIG type scale. Each entry: [size, { lineHeight, fontWeight }]
        "large-title": ["1.625rem", { lineHeight: "2rem",      fontWeight: "600" }],
        "title-1":     ["1.375rem", { lineHeight: "1.625rem", fontWeight: "400" }],
        "title-2":     ["1.0625rem",{ lineHeight: "1.375rem", fontWeight: "400" }],
        "title-3":     ["0.9375rem",{ lineHeight: "1.25rem",  fontWeight: "400" }],
        "headline":    ["0.8125rem",{ lineHeight: "1rem",     fontWeight: "600" }],
        "body":        ["0.8125rem",{ lineHeight: "1rem",     fontWeight: "400" }],
        "callout":     ["0.75rem",  { lineHeight: "0.9375rem",fontWeight: "400" }],
        "subheadline": ["0.6875rem",{ lineHeight: "0.875rem", fontWeight: "400" }],
        "footnote":    ["0.625rem", { lineHeight: "0.8125rem",fontWeight: "400" }],
        "caption-1":   ["0.625rem", { lineHeight: "0.8125rem",fontWeight: "400" }],
        "caption-2":   ["0.625rem", { lineHeight: "0.8125rem",fontWeight: "500" }],
      },
      colors: {
        // Old tokens — kept during migration, removed in Task 15.
        surface: {
          DEFAULT: "#1c1c1e",
          secondary: "#2c2c2e",
          tertiary: "#3a3a3c",
        },

        // Opaque — `<alpha-value>` supported.
        "window-background":      "rgb(var(--color-window-background-rgb) / <alpha-value>)",
        "under-page-background":  "rgb(var(--color-under-page-background-rgb) / <alpha-value>)",
        "control-background":     "rgb(var(--color-control-background-rgb) / <alpha-value>)",
        "sidebar-background":     "rgb(var(--color-sidebar-background-rgb) / <alpha-value>)",
        "control-selected-text":  "rgb(var(--color-control-selected-text-rgb) / <alpha-value>)",
        "accent":                 "rgb(var(--color-accent-rgb) / <alpha-value>)",
        "accent-hover":           "rgb(var(--color-accent-hover-rgb) / <alpha-value>)",
        "accent-pressed":         "rgb(var(--color-accent-pressed-rgb) / <alpha-value>)",
        "accent-label":           "rgb(var(--color-accent-label-rgb) / <alpha-value>)",
        "status-success":         "rgb(var(--color-status-success-rgb) / <alpha-value>)",
        "status-warning":         "rgb(var(--color-status-warning-rgb) / <alpha-value>)",
        "status-error":           "rgb(var(--color-status-error-rgb) / <alpha-value>)",
        "status-info":            "rgb(var(--color-status-info-rgb) / <alpha-value>)",

        // Inherent alpha — plain var() reference, no alpha-modifier support.
        "label-primary":          "var(--color-label-primary)",
        "label-secondary":        "var(--color-label-secondary)",
        "label-tertiary":         "var(--color-label-tertiary)",
        "label-quaternary":       "var(--color-label-quaternary)",
        "text-placeholder":       "var(--color-text-placeholder)",
        "separator":              "var(--color-separator)",
        "fill-primary":           "var(--color-fill-primary)",
        "fill-secondary":         "var(--color-fill-secondary)",
        "fill-tertiary":          "var(--color-fill-tertiary)",
        "fill-quaternary":        "var(--color-fill-quaternary)",
        "control-text":           "var(--color-control-text)",
        "control-text-disabled":  "var(--color-control-text-disabled)",

        // Focus ring — accent at 50% alpha, reused in component focus states.
        "focus-ring":             "rgb(var(--color-accent-rgb) / 0.5)",
      },
    },
  },
  plugins: [],
};
```

- [ ] **Step 2: Verify build still succeeds**

```bash
npm run build
```

Expected: TypeScript + Vite build succeeds with no errors. The dark UI is unchanged because no component uses the new tokens yet.

- [ ] **Step 3: Commit**

```bash
git add tailwind.config.js
git commit -m "feat(tokens): wire semantic tokens into Tailwind theme"
```

---

## Task 4: Create `appearance.ts` with tests (TDD)

**Files:**
- Create: `src/lib/appearance.ts`
- Create: `src/lib/appearance.test.ts`

- [ ] **Step 1: Write the failing test**

Create `src/lib/appearance.test.ts`:

```ts
import { describe, it, expect, beforeEach, vi } from "vitest";
import { initAppearance, getAppearance, onAppearanceChange } from "./appearance";

type MediaQueryListener = (e: MediaQueryListEvent) => void;

function mockMatchMedia(initialMatches: boolean) {
  const listeners: MediaQueryListener[] = [];
  const mql = {
    matches: initialMatches,
    media: "(prefers-color-scheme: dark)",
    addEventListener: (_: string, l: MediaQueryListener) => listeners.push(l),
    removeEventListener: () => {},
    onchange: null,
    dispatchEvent: () => false,
    addListener: () => {},
    removeListener: () => {},
  } as unknown as MediaQueryList;

  window.matchMedia = () => mql;

  return {
    flipTo: (matches: boolean) => {
      (mql as { matches: boolean }).matches = matches;
      for (const l of listeners) {
        l({ matches } as MediaQueryListEvent);
      }
    },
  };
}

describe("appearance", () => {
  beforeEach(() => {
    document.documentElement.removeAttribute("data-appearance");
  });

  it("sets data-appearance to 'dark' when system is dark", () => {
    mockMatchMedia(true);
    initAppearance();
    expect(document.documentElement.dataset.appearance).toBe("dark");
    expect(getAppearance()).toBe("dark");
  });

  it("sets data-appearance to 'light' when system is light", () => {
    mockMatchMedia(false);
    initAppearance();
    expect(document.documentElement.dataset.appearance).toBe("light");
    expect(getAppearance()).toBe("light");
  });

  it("flips data-appearance when the media query changes", () => {
    const m = mockMatchMedia(false);
    initAppearance();
    expect(getAppearance()).toBe("light");
    m.flipTo(true);
    expect(document.documentElement.dataset.appearance).toBe("dark");
    expect(getAppearance()).toBe("dark");
  });

  it("notifies subscribers on change", () => {
    const m = mockMatchMedia(false);
    initAppearance();
    const cb = vi.fn();
    onAppearanceChange(cb);
    m.flipTo(true);
    expect(cb).toHaveBeenCalledWith("dark");
    m.flipTo(false);
    expect(cb).toHaveBeenCalledWith("light");
  });
});
```

- [ ] **Step 2: Run the tests to confirm they fail**

```bash
npm run test:run -- appearance
```

Expected: FAIL — "Cannot find module './appearance'" or similar.

- [ ] **Step 3: Implement `src/lib/appearance.ts`**

```ts
export type Appearance = "light" | "dark";

type Listener = (a: Appearance) => void;

let current: Appearance = "light";
const listeners = new Set<Listener>();

function apply(a: Appearance) {
  current = a;
  document.documentElement.dataset.appearance = a;
  for (const l of listeners) l(a);
}

export function initAppearance(): void {
  const mql = window.matchMedia?.("(prefers-color-scheme: dark)");
  if (!mql) {
    console.warn("appearance: matchMedia unavailable, defaulting to dark");
    apply("dark");
    return;
  }
  apply(mql.matches ? "dark" : "light");
  mql.addEventListener("change", (e) => apply(e.matches ? "dark" : "light"));
}

export function getAppearance(): Appearance {
  return current;
}

export function onAppearanceChange(cb: Listener): () => void {
  listeners.add(cb);
  return () => listeners.delete(cb);
}
```

- [ ] **Step 4: Run the tests to confirm they pass**

```bash
npm run test:run -- appearance
```

Expected: all four tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/lib/appearance.ts src/lib/appearance.test.ts
git commit -m "feat(appearance): track system light/dark via data-appearance"
```

---

## Task 5: Rust — add AppKit deps

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Add macOS-only deps**

Append to `[dependencies]` in `src-tauri/Cargo.toml`:

```toml
[target.'cfg(target_os = "macos")'.dependencies]
objc2 = "0.5"
objc2-foundation = "0.2"
objc2-app-kit = "0.2"
```

- [ ] **Step 2: Verify it compiles**

```bash
(cd src-tauri && cargo build)
```

Expected: successful build. No new code uses the crates yet, but they resolve.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "chore(rust): add objc2 crates for macOS AppKit FFI"
```

---

## Task 6: Rust — accent module with `get_system_accent` command

**Files:**
- Create: `src-tauri/src/accent.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write `src-tauri/src/accent.rs`**

```rust
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
    use objc2::rc::autoreleasepool;
    use objc2::runtime::ProtocolObject;
    use objc2_app_kit::{NSAppearance, NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSColor};

    fn color_as_rgb(color: &NSColor) -> [u8; 3] {
        // Convert to sRGB to avoid catalog color spaces returning wrong components.
        let srgb_name = unsafe { objc2_app_kit::NSColorSpaceNameSRGB };
        let converted = unsafe { color.colorUsingColorSpaceName(srgb_name) }
            .unwrap_or_else(|| color.clone());
        let r = unsafe { converted.redComponent() };
        let g = unsafe { converted.greenComponent() };
        let b = unsafe { converted.blueComponent() };
        [
            (r.clamp(0.0, 1.0) * 255.0).round() as u8,
            (g.clamp(0.0, 1.0) * 255.0).round() as u8,
            (b.clamp(0.0, 1.0) * 255.0).round() as u8,
        ]
    }

    fn resolved_accent(appearance_name: &objc2_foundation::NSString) -> [u8; 3] {
        autoreleasepool(|_| {
            let appearance =
                unsafe { NSAppearance::appearanceNamed(appearance_name) }.expect("appearance");
            let mut out = [0u8; 3];
            let block = block2::RcBlock::new(|| {
                let color = unsafe { NSColor::controlAccentColor() };
                out = color_as_rgb(&color);
            });
            unsafe {
                appearance.performAsCurrentDrawingAppearance(&block);
            }
            out
        })
    }

    pub fn read_accent_colors() -> AccentColors {
        AccentColors {
            light: resolved_accent(unsafe { NSAppearanceNameAqua }),
            dark: resolved_accent(unsafe { NSAppearanceNameDarkAqua }),
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
pub fn emit_current<R: Runtime>(app: &AppHandle<R>) {
    if let Err(e) = app.emit(ACCENT_CHANGED_EVENT, imp::read_accent_colors()) {
        eprintln!("accent: failed to emit {ACCENT_CHANGED_EVENT}: {e}");
    }
}
```

- [ ] **Step 2: Add `block2` crate dep for the appearance block call**

Append to the `cfg(target_os = "macos")` deps block in `src-tauri/Cargo.toml`:

```toml
block2 = "0.5"
```

- [ ] **Step 3: Register the module and command in `src-tauri/src/lib.rs`**

Add `pub mod accent;` near the other `mod` declarations at the top.

Add `accent::get_system_accent,` to the `tauri::generate_handler![ … ]` list (the registration block already present in `lib.rs`).

- [ ] **Step 4: Build to verify**

```bash
(cd src-tauri && cargo build)
```

Expected: builds cleanly. If `objc2-app-kit` API names differ from the spec (API has churned across minor versions), pin the exact version and adjust call sites accordingly.

- [ ] **Step 5: Add a Rust test**

At the bottom of `src-tauri/src/accent.rs`, append:

```rust
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
```

Run it:

```bash
(cd src-tauri && cargo test accent)
```

Expected: tests pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/accent.rs src-tauri/src/lib.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat(rust): add get_system_accent command reading NSColor.controlAccentColor"
```

---

## Task 7: Rust — observer for accent and appearance changes

**Files:**
- Modify: `src-tauri/src/accent.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add an observer install function to `accent.rs`**

Append to `src-tauri/src/accent.rs`:

```rust
#[cfg(target_os = "macos")]
pub fn install_observer<R: Runtime>(app: AppHandle<R>) {
    use objc2::rc::{Allocated, Retained};
    use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, Sel};
    use objc2::{declare_class, msg_send_id, mutability, ClassType, DeclaredClass};
    use objc2_foundation::{ns_string, NSNotification, NSNotificationCenter, NSOperationQueue};

    struct Ivars {
        callback: Box<dyn Fn() + Send + Sync>,
    }

    declare_class!(
        struct AccentObserver;

        unsafe impl ClassType for AccentObserver {
            type Super = NSObject;
            type Mutability = mutability::Mutable;
            const NAME: &'static str = "MeisterAccentObserver";
        }

        impl DeclaredClass for AccentObserver {
            type Ivars = Ivars;
        }

        unsafe impl AccentObserver {
            #[method(handleChange:)]
            fn handle_change(&self, _n: &NSNotification) {
                (self.ivars().callback)();
            }
        }

        unsafe impl NSObjectProtocol for AccentObserver {}
    );

    let app_for_cb = app.clone();
    let cb: Box<dyn Fn() + Send + Sync> = Box::new(move || {
        emit_current(&app_for_cb);
    });

    let observer: Retained<AccentObserver> = {
        let alloc: Allocated<AccentObserver> = AccentObserver::alloc();
        let this = alloc.set_ivars(Ivars { callback: cb });
        unsafe { msg_send_id![super(this), init] }
    };

    // Leak the observer for the lifetime of the app; Cocoa holds a weak ref.
    let observer_ptr: *mut AnyObject =
        Retained::into_raw(observer) as *mut AnyObject;

    unsafe {
        let center = NSNotificationCenter::defaultCenter();
        let sel = Sel::register("handleChange:");
        // Accent color change (distributed notification).
        objc2::msg_send![
            &*center,
            addObserver: &*(observer_ptr as *const AnyObject),
            selector: sel,
            name: ns_string!("AppleColorPreferencesChangedNotification"),
            object: std::ptr::null::<AnyObject>()
        ];
        // System appearance change — delivered via
        // AppleInterfaceThemeChangedNotification on the distributed center.
        objc2::msg_send![
            &*center,
            addObserver: &*(observer_ptr as *const AnyObject),
            selector: sel,
            name: ns_string!("AppleInterfaceThemeChangedNotification"),
            object: std::ptr::null::<AnyObject>()
        ];
    }
}

#[cfg(not(target_os = "macos"))]
pub fn install_observer<R: Runtime>(_app: AppHandle<R>) {
    // No-op on non-macOS — accent stays on the boot value.
}
```

- [ ] **Step 2: Call `install_observer` from the Tauri setup block**

In `src-tauri/src/lib.rs`, inside `.setup(|app| { … })`, before `Ok(())`, add:

```rust
accent::install_observer(app.handle().clone());
```

- [ ] **Step 3: Build to verify**

```bash
(cd src-tauri && cargo build)
```

Expected: builds cleanly. Objc class declaration warnings about stability are acceptable.

- [ ] **Step 4: Manual smoke test**

```bash
npm run tauri dev
```

With the dev window open, run in the terminal (while the app is running):

```bash
osascript -e 'tell application "System Events" to tell appearance preferences to set dark mode to not dark mode'
```

Then inspect the app's webview console (right-click → Inspect in the dev build) — you should be able to verify the event fires in Task 9 once the frontend hook is wired. For now, the success criterion is that toggling the appearance **does not crash** the app.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/accent.rs src-tauri/src/lib.rs
git commit -m "feat(rust): observe accent + appearance changes, emit event"
```

---

## Task 8: Create `accent.ts` with tests (TDD)

**Files:**
- Create: `src/lib/accent.ts`
- Create: `src/lib/accent.test.ts`

- [ ] **Step 1: Write the failing test**

Create `src/lib/accent.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { initAccent, getAccentRgb } from "./accent";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const listenMock = listen as unknown as ReturnType<typeof vi.fn>;

function readRootVar(name: string): string {
  return document.documentElement.style.getPropertyValue(name);
}

describe("accent", () => {
  beforeEach(() => {
    document.documentElement.removeAttribute("style");
    document.documentElement.dataset.appearance = "dark";
    invokeMock.mockReset();
    listenMock.mockReset().mockResolvedValue(() => {});
  });

  it("writes --color-accent-rgb from the dark triplet when appearance is dark", async () => {
    invokeMock.mockResolvedValue({ light: [0, 122, 255], dark: [10, 132, 255] });
    await initAccent();
    expect(readRootVar("--color-accent-rgb").trim()).toBe("10 132 255");
    expect(getAccentRgb()).toEqual([10, 132, 255]);
  });

  it("writes the light triplet when appearance is light", async () => {
    document.documentElement.dataset.appearance = "light";
    invokeMock.mockResolvedValue({ light: [0, 122, 255], dark: [10, 132, 255] });
    await initAccent();
    expect(readRootVar("--color-accent-rgb").trim()).toBe("0 122 255");
  });

  it("re-writes on system-accent-changed events", async () => {
    invokeMock.mockResolvedValue({ light: [0, 122, 255], dark: [10, 132, 255] });
    let handler: ((e: { payload: unknown }) => void) | undefined;
    listenMock.mockImplementation(async (_event: string, cb: typeof handler) => {
      handler = cb;
      return () => {};
    });
    await initAccent();
    handler?.({ payload: { light: [191, 90, 242], dark: [191, 90, 242] } });
    expect(readRootVar("--color-accent-rgb").trim()).toBe("191 90 242");
  });

  it("falls back to Apple blue if invoke fails", async () => {
    invokeMock.mockRejectedValue(new Error("no tauri"));
    await initAccent();
    const v = readRootVar("--color-accent-rgb").trim();
    expect(["0 122 255", "10 132 255"]).toContain(v);
  });

  it("chooses black label-rgb for Yellow and light-mode Graphite", async () => {
    document.documentElement.dataset.appearance = "light";
    // Graphite light resolves roughly to (152, 152, 157). We assert the
    // logic produces black text for any near-Graphite/Yellow triplet.
    invokeMock.mockResolvedValue({ light: [152, 152, 157], dark: [152, 152, 157] });
    await initAccent();
    expect(readRootVar("--color-accent-label-rgb").trim()).toBe("0 0 0");
  });
});
```

- [ ] **Step 2: Run the tests to confirm they fail**

```bash
npm run test:run -- accent
```

Expected: FAIL — module missing.

- [ ] **Step 3: Implement `src/lib/accent.ts`**

```ts
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getAppearance, onAppearanceChange } from "./appearance";

type Rgb = [number, number, number];
interface AccentColors {
  light: Rgb;
  dark: Rgb;
}

const FALLBACK: AccentColors = {
  light: [0, 122, 255],
  dark: [10, 132, 255],
};

let current: AccentColors = FALLBACK;

function setVar(name: string, value: string) {
  document.documentElement.style.setProperty(name, value);
}

function lighten(rgb: Rgb, amount: number): Rgb {
  return rgb.map((c) =>
    Math.round(Math.min(255, c + (255 - c) * amount)),
  ) as Rgb;
}

function darken(rgb: Rgb, amount: number): Rgb {
  return rgb.map((c) => Math.round(Math.max(0, c * (1 - amount)))) as Rgb;
}

/** Relative luminance (WCAG) — used to pick black vs white label text. */
function luminance([r, g, b]: Rgb): number {
  const srgb = [r, g, b].map((v) => {
    const n = v / 255;
    return n <= 0.03928 ? n / 12.92 : Math.pow((n + 0.055) / 1.055, 2.4);
  });
  return 0.2126 * srgb[0] + 0.7152 * srgb[1] + 0.0722 * srgb[2];
}

function labelFor(rgb: Rgb): Rgb {
  // White text unless the accent is very light (Yellow, light Graphite).
  return luminance(rgb) > 0.55 ? [0, 0, 0] : [255, 255, 255];
}

function apply() {
  const rgb = getAppearance() === "dark" ? current.dark : current.light;
  setVar("--color-accent-rgb", rgb.join(" "));
  setVar("--color-accent-hover-rgb", lighten(rgb, 0.08).join(" "));
  setVar("--color-accent-pressed-rgb", darken(rgb, 0.12).join(" "));
  setVar("--color-accent-label-rgb", labelFor(rgb).join(" "));
}

export function getAccentRgb(): Rgb {
  return getAppearance() === "dark" ? current.dark : current.light;
}

export async function initAccent(): Promise<void> {
  try {
    current = await invoke<AccentColors>("get_system_accent");
  } catch (e) {
    console.warn("accent: get_system_accent failed, using fallback", e);
    current = FALLBACK;
  }
  apply();

  try {
    await listen<AccentColors>("system-accent-changed", (e) => {
      current = e.payload;
      apply();
    });
  } catch (e) {
    console.warn("accent: listen failed, accent will stay on boot value", e);
  }

  onAppearanceChange(apply);
}
```

- [ ] **Step 4: Run the tests to confirm they pass**

```bash
npm run test:run -- accent
```

Expected: all five tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/lib/accent.ts src/lib/accent.test.ts
git commit -m "feat(accent): follow system accent color with live updates"
```

---

## Task 9: Wire initialization into `main.tsx` and import tokens

**Files:**
- Modify: `src/main.tsx`
- Modify: `src/index.css`

- [ ] **Step 1: Update `src/main.tsx`**

Replace the file contents with:

```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { initAppearance } from "./lib/appearance";
import { initAccent } from "./lib/accent";
import "./styles/tokens.css";
import "./index.css";

initAppearance();
void initAccent();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
```

- [ ] **Step 2: Migrate `src/index.css` to tokens**

Replace the current `src/index.css` with:

```css
@tailwind base;
@tailwind components;
@tailwind utilities;

* {
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}

body {
  @apply bg-window-background text-label-primary;
  font-family: -apple-system, BlinkMacSystemFont, "SF Pro Display", "Helvetica Neue", sans-serif;
  overflow: hidden;
  user-select: none;
}

textarea,
.scrollable {
  user-select: text;
}

/* Custom scrollbar */
::-webkit-scrollbar {
  width: 6px;
}
::-webkit-scrollbar-track {
  background: transparent;
}
::-webkit-scrollbar-thumb {
  background: var(--color-fill-secondary);
  border-radius: 3px;
}
::-webkit-scrollbar-thumb:hover {
  background: var(--color-fill-primary);
}

/* Drop zone animation */
@keyframes pulse-border {
  0%, 100% { border-color: rgb(var(--color-accent-rgb) / 0.6); opacity: 0.6; }
  50%      { border-color: rgb(var(--color-accent-hover-rgb) / 1); opacity: 1; }
}

.drop-active {
  animation: pulse-border 1s ease-in-out infinite;
}

/* Stage indicator */
.stage-dot {
  @apply w-2 h-2 rounded-full;
}
.stage-dot.done    { @apply bg-status-success; }
.stage-dot.active  { @apply bg-accent; }
.stage-dot.pending { @apply bg-fill-tertiary; }

/* Form inputs */
.input {
  @apply w-full bg-control-background border border-separator rounded-lg px-3 py-2 text-body text-label-primary outline-none
         focus:ring-1 focus:ring-focus-ring focus:border-accent transition-colors;
}

select.input option {
  background: rgb(var(--color-control-background-rgb));
}

/* Spinner */
@keyframes spin {
  to { transform: rotate(360deg); }
}
.spinner {
  display: inline-block;
  width: 14px;
  height: 14px;
  border: 2px solid var(--color-fill-secondary);
  border-top-color: rgb(var(--color-accent-rgb));
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}
```

- [ ] **Step 3: Run the app and confirm dark mode is visually unchanged**

```bash
npm run tauri dev
```

Compare against `docs/superpowers/artifacts/2026-04-24-pre-migration/` screenshots. The chrome (`body` background, scrollbar, spinner, dot colors) should look identical to the baseline.

- [ ] **Step 4: Manually test live updates**

While the app is running, open macOS System Settings → Appearance and flip between Light and Dark. The app's body background should flip (components still use old `surface*` tokens so most of the UI stays dark — that's expected until Task 10+).

Change System Settings → Accent color from Blue to Purple. The scrollbar hover color, spinner, and `.stage-dot.active` should pick up the new hue.

- [ ] **Step 5: Commit**

```bash
git add src/main.tsx src/index.css
git commit -m "feat: initialize appearance + accent tracking, migrate index.css to tokens"
```

---

## Task 10: Migrate component group A — chrome (TitleBar, Sidebar)

**Files:**
- Modify: `src/components/TitleBar.tsx`
- Modify: `src/components/Sidebar.tsx`

- [ ] **Step 1: Apply the color mapping to both files**

Walk each file and apply this mapping exactly:

| Old class | → | New class |
|---|---|---|
| `bg-surface` (app-level chrome) | → | `bg-window-background` |
| `bg-surface-secondary` (sidebar surface) | → | `bg-sidebar-background` |
| `bg-surface-secondary` (cards, non-sidebar) | → | `bg-control-background` |
| `bg-surface-tertiary` | → | `bg-fill-tertiary` |
| `hover:bg-surface-tertiary` | → | `hover:bg-fill-tertiary` |
| `text-white` | → | `text-label-primary` |
| `text-gray-300` / `text-gray-400` | → | `text-label-secondary` |
| `text-gray-500` / `text-gray-600` | → | `text-label-tertiary` |
| `border-white/10`, `border-white/5`, `border-gray-700`, `border-gray-800` | → | `border-separator` |
| `bg-accent/10`, `bg-accent/15`, `bg-accent/20` | → | same (`<alpha-value>` now works) |
| `bg-accent` (solid) | → | same |
| `hover:bg-accent-hover` or `hover:bg-[#409cff]` | → | `hover:bg-accent-hover` |
| `text-red-400`, `text-red-500` | → | `text-status-error` |
| `bg-red-500/10`, `bg-red-500/20` | → | `bg-status-error/10` (or `/20`) |
| `text-green-400`, `text-green-500` | → | `text-status-success` |
| `text-amber-400`, `text-yellow-400` | → | `text-status-warning` |
| `bg-amber-500/10`, `bg-yellow-500/10` | → | `bg-status-warning/10` |

For `Sidebar.tsx`, use `bg-sidebar-background` (not `bg-control-background`) for the root sidebar surface; individual rows use `hover:bg-fill-tertiary` for hover and `bg-accent/10 text-accent` for the selected row.

For `TitleBar.tsx`, the root uses `bg-window-background`.

- [ ] **Step 2: Build and confirm the app compiles**

```bash
npm run build
```

Expected: builds successfully.

- [ ] **Step 3: Run dev app and diff against pre-migration screenshots**

```bash
npm run tauri dev
```

Capture new dark-mode screenshots of the sidebar + titlebar regions. Compare against `docs/superpowers/artifacts/2026-04-24-pre-migration/*.png`. Any visible difference in dark mode is a defect. Acceptable differences: slight text-alpha shifts (white → 85% white) — confirm these are acceptable with the reviewer or leave a note in the commit.

- [ ] **Step 4: Verify light mode is plausible**

Flip System Settings → Appearance to Light. Sidebar should have a light-gray tone, titlebar a near-white tone, text dark. Don't worry about perfect polish yet — Task 16 tunes the palette.

- [ ] **Step 5: Commit**

```bash
git add src/components/TitleBar.tsx src/components/Sidebar.tsx
git commit -m "refactor(ui): migrate TitleBar + Sidebar to semantic tokens"
```

---

## Task 11: Migrate component group B — top-level views (ProjectView, RecordingView)

**Files:**
- Modify: `src/components/ProjectView.tsx`
- Modify: `src/components/RecordingView.tsx`

- [ ] **Step 1: Apply the same mapping from Task 10 Step 1**

The mapping table from Task 10 applies identically here. Use `bg-control-background` (not `bg-sidebar-background`) for content cards and `bg-window-background` for outer scrolling regions.

- [ ] **Step 2: Build**

```bash
npm run build
```

Expected: succeeds.

- [ ] **Step 3: Dark-mode visual diff**

```bash
npm run tauri dev
```

Compare against `project-view.png`, `recording-view-active.png`, `recording-view-done.png`. No visible changes in dark mode.

- [ ] **Step 4: Commit**

```bash
git add src/components/ProjectView.tsx src/components/RecordingView.tsx
git commit -m "refactor(ui): migrate ProjectView + RecordingView to semantic tokens"
```

---

## Task 12: Migrate component group C — recording/pipeline cards (RecordCard, RecordCallCard, StageCard)

**Files:**
- Modify: `src/components/RecordCard.tsx`
- Modify: `src/components/RecordCallCard.tsx`
- Modify: `src/components/StageCard.tsx`

- [ ] **Step 1: Apply the Task 10 mapping**

Same table. Additional notes for this group:

- `StageCard.tsx` uses status backgrounds (running/success/error). Map:
  - Running highlight (`bg-accent/10`, `border-accent/40`) → keep; accent is now system.
  - Success tint (`bg-green-500/10`, `text-green-400`, `border-green-500/30`) → `bg-status-success/10`, `text-status-success`, `border-status-success/30`.
  - Error tint (`bg-red-500/10`, `text-red-400`, `border-red-500/30`) → `bg-status-error/10`, `text-status-error`, `border-status-error/30`.
- `RecordCard.tsx` / `RecordCallCard.tsx` recording-state buttons keep `bg-accent` / `bg-status-error` etc.

- [ ] **Step 2: Build**

```bash
npm run build
```

- [ ] **Step 3: Dark-mode visual diff**

Capture `record-card.png` and inspect `StageCard` in a live run. Should be indistinguishable from baseline.

- [ ] **Step 4: Commit**

```bash
git add src/components/RecordCard.tsx src/components/RecordCallCard.tsx src/components/StageCard.tsx
git commit -m "refactor(ui): migrate RecordCard, RecordCallCard, StageCard to semantic tokens"
```

---

## Task 13: Migrate component group D — content & staging (OutputPreview, StagingList, RecordingDropZone)

**Files:**
- Modify: `src/components/OutputPreview.tsx`
- Modify: `src/components/StagingList.tsx`
- Modify: `src/components/RecordingDropZone.tsx`

- [ ] **Step 1: Apply the Task 10 mapping**

Additional notes:
- `OutputPreview.tsx` monospace code box uses `bg-control-background` with `text-label-primary` and `border-separator`. Tab pills: inactive `text-label-secondary hover:bg-fill-tertiary`, active `bg-accent/10 text-accent`.
- `RecordingDropZone.tsx` dashed border uses `border-separator` when idle, `border-accent` during drag; background `bg-fill-quaternary` when idle, `bg-accent/10` during drag.
- `StagingList.tsx` rows: `bg-control-background border-separator`, drag-handle icon `text-label-tertiary`.

- [ ] **Step 2: Build**

```bash
npm run build
```

- [ ] **Step 3: Dark-mode visual diff**

- [ ] **Step 4: Commit**

```bash
git add src/components/OutputPreview.tsx src/components/StagingList.tsx src/components/RecordingDropZone.tsx
git commit -m "refactor(ui): migrate OutputPreview, StagingList, RecordingDropZone to semantic tokens"
```

---

## Task 14: Migrate component group E — forms & modals (SettingsForm, TemplateEditor, DownloadModelModal)

**Files:**
- Modify: `src/components/SettingsForm.tsx`
- Modify: `src/components/TemplateEditor.tsx`
- Modify: `src/components/DownloadModelModal.tsx`

- [ ] **Step 1: Apply the Task 10 mapping**

Additional notes:
- Form inputs already use the `.input` utility class from `index.css` — nothing to change on them.
- `DownloadModelModal.tsx` backdrop: `bg-black/50` stays (true black overlay, not a token).
- Modal surface: `bg-control-background border-separator`.
- Progress bar track: `bg-fill-tertiary`; fill: `bg-accent`.

- [ ] **Step 2: Build**

```bash
npm run build
```

- [ ] **Step 3: Dark-mode visual diff**

- [ ] **Step 4: Commit**

```bash
git add src/components/SettingsForm.tsx src/components/TemplateEditor.tsx src/components/DownloadModelModal.tsx
git commit -m "refactor(ui): migrate SettingsForm, TemplateEditor, DownloadModelModal to semantic tokens"
```

---

## Task 15: Remove old tokens from Tailwind config and catch stragglers

**Files:**
- Modify: `tailwind.config.js`

- [ ] **Step 1: Remove the old `surface` block**

Delete this block from `tailwind.config.js`:

```js
surface: {
  DEFAULT: "#1c1c1e",
  secondary: "#2c2c2e",
  tertiary: "#3a3a3c",
},
```

Also remove the old `accent.hover` shim (we now use `accent-hover` directly):

```js
accent: {
  DEFAULT: "#0a84ff",
  hover: "#409cff",
},
```

(If `accent` is still present elsewhere in the file as a CSS-variable entry, keep that — just remove the object literal form.)

- [ ] **Step 2: Build — expect failures on stale references**

```bash
npm run build
```

If any component still references `bg-surface`, `bg-surface-secondary`, `bg-surface-tertiary`, `bg-accent-hover` via the old literal, the build warning / visual diff will flag it. Fix by applying the Task 10 mapping.

- [ ] **Step 3: Grep for remaining references**

```bash
grep -rE "(surface|bg-\[#[0-9a-fA-F]+\]|text-gray-[0-9]+|border-white/|border-gray-[0-9]+|bg-red-[0-9]+|text-red-[0-9]+|bg-green-[0-9]+|text-green-[0-9]+|bg-amber-|text-amber-|bg-yellow-|text-yellow-)" src/
```

Anything that matches needs to be migrated. Fix inline and re-run the grep until the only remaining matches are intentional (e.g. monospace code in fixtures, comments).

- [ ] **Step 4: Build clean**

```bash
npm run build
```

Expected: succeeds.

- [ ] **Step 5: Full dark-mode regression pass**

Compare every pre-migration screenshot against the app. Any difference is a defect.

- [ ] **Step 6: Commit**

```bash
git add tailwind.config.js src/
git commit -m "refactor(ui): remove legacy surface/accent tokens, migrate stragglers"
```

---

## Task 16: Typography sweep

**Files:**
- Modify: `src/components/*.tsx` (any file that uses ad-hoc text sizes/weights)
- Modify: `src/index.css` (if `.input` or other utility classes use ad-hoc sizes)

- [ ] **Step 1: Find ad-hoc type classes**

```bash
grep -rE "text-(xs|sm|base|lg|xl|2xl|3xl)|font-(normal|medium|semibold|bold)" src/components/ src/index.css
```

- [ ] **Step 2: Apply the type-scale mapping where intent is clear**

| Current | → | Token |
|---|---|---|
| `text-xs` (~12px) | → | `text-callout` (12px) or `text-footnote` (10px) depending on use |
| `text-sm` (~14px) | → | `text-body` (13px) for body copy; `text-headline` (13/600) for row headings |
| `text-base` (~16px) | → | `text-body` (13px) — reduce unless we really need 16px |
| `text-lg` (~18px) | → | `text-title-3` (15px) or `text-title-2` (17px) |
| `text-xl` (~20px) | → | `text-title-2` (17px) |
| `text-2xl` (~24px) | → | `text-title-1` (22px) |
| `text-3xl` (~30px) | → | `text-large-title` (26px) |
| `font-semibold` paired with a size | → | drop `font-semibold` when paired with `text-headline` or `text-large-title` (weight is baked in) |

Don't mass-rewrite; only change cases where the intent matches a scale value. Leave bespoke sizes (e.g. `text-[11px]` for a badge) alone if they aren't on the scale.

- [ ] **Step 3: Build**

```bash
npm run build
```

- [ ] **Step 4: Dark-mode visual diff**

Type changes will produce small but visible shifts (body dropping from 14px to 13px, for example). Capture fresh screenshots and compare against the pre-migration baselines — confirm the shift is in the right direction (tighter, more macOS-native). Flag to the reviewer any case where a change looks worse.

- [ ] **Step 5: Commit**

```bash
git add src/
git commit -m "refactor(ui): adopt macOS HIG type scale"
```

---

## Task 17: Light-mode tuning pass

**Files:**
- Modify: `src/styles/tokens.css` (light block only)

- [ ] **Step 1: Open macOS reference apps side by side**

Open Mail.app, Messages.app, and Notes.app in light mode. These are the reference for what Meister should look like.

- [ ] **Step 2: Walk every screen of Meister in light mode**

```bash
npm run tauri dev
```

For each screen (empty state, ProjectView, RecordingView active/done, SettingsForm, DownloadModelModal, RecordCard/RecordCallCard during recording):

- Compare Meister's window chrome, sidebar, cards, separators, text contrast against Mail/Notes.
- Note issues: too-gray background, too-harsh separators, text-tertiary invisible, cards not visibly raised from the window, etc.

- [ ] **Step 3: Adjust light-block values in `tokens.css`**

Common adjustments you will likely make (don't make them blindly — only as needed):

- `--color-window-background-rgb` from `236 236 236` → `242 242 247` (slight cool tint like macOS Big Sur+).
- `--color-sidebar-background-rgb` from `232 232 232` → `236 236 240` if the sidebar reads as too-flat.
- `--color-separator` from `rgba(0 0 0 / 0.10)` → `rgba(0 0 0 / 0.08)` if separators look heavy.
- `--color-fill-tertiary` if hover states are too dark.

Do a round trip after each change: save, let the app hot-reload, re-compare.

- [ ] **Step 4: Check `accent-label` math in light mode for Graphite and Yellow**

System Settings → Accent → Graphite. Confirm accent label text inside accent buttons is dark (not white). Same with Yellow.

- [ ] **Step 5: Re-run all tests**

```bash
npm run test:run
```

Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add src/styles/tokens.css
git commit -m "refactor(tokens): tune light-mode palette against macOS reference apps"
```

---

## Task 18: Manual QA checklist

**Files:** none (verification only)

- [ ] **Step 1: Dark-mode parity check**

For each pre-migration screenshot in `docs/superpowers/artifacts/2026-04-24-pre-migration/`, capture a matching screenshot from the current build. Diff by eye. Acceptable: text weight/size shifts from Task 16. Unacceptable: color shifts, layout shifts, missing separators.

- [ ] **Step 2: Light-mode usability check**

Walk every screen in light mode. Verify:
- No text at contrast ratio below ~4.5:1 for body copy.
- Every interactive element has a visible border or fill.
- Status colors (success/warning/error) are still distinguishable in light mode.

- [ ] **Step 3: Live appearance toggle**

With the app open, flip System Settings → Appearance between Light and Dark. Expected: single-frame flip, no flash of wrong palette, no stuck elements.

- [ ] **Step 4: Live accent toggle**

With the app open, change System Settings → Accent color: Blue → Purple → Pink → Orange → Yellow → Green → Graphite → Blue. Each change should propagate within ~1s. For Yellow and light-mode Graphite, confirm labels on accent-fill buttons are dark text (not white).

- [ ] **Step 5: Reload & stability**

Kill the app and restart. Accent and appearance should match the system on boot. No console errors about `accent` or `appearance`.

- [ ] **Step 6: Full test suite**

```bash
npm run test:run
(cd src-tauri && cargo test)
```

Expected: everything green.

- [ ] **Step 7: Tag the migration**

```bash
git tag native-feel/01-tokens-complete
```

Sub-project #1 is done. Next up is #2 (window vibrancy + translucent sidebar) — a separate spec and plan.
