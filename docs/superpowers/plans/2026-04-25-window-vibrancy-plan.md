# Window Vibrancy & Translucent Sidebar — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Spec:** `docs/superpowers/specs/2026-04-25-window-vibrancy-design.md`

**Goal:** Attach `NSVisualEffectView` to the main window so Meister's chrome renders with macOS vibrancy, layer translucency through the existing token system, drop the duplicate "Meister" titlebar label, and detect Reduce Transparency for accessibility.

**Architecture:** A new `src-tauri/src/vibrancy.rs` module installs `NSVisualEffectView` with `.windowBackground` material on the Tauri window during setup, sets the window itself to transparent so vibrancy shows through, and observes `NSWorkspace.accessibilityDisplayShouldReduceTransparency` to emit a Tauri event when it changes. The frontend's existing token system gains paired `*-alpha` CSS variables on four opaque tokens (window, sidebar, control, under-page); a new `[data-reduce-transparency="true"]` block in `tokens.css` restores all alphas to `1` when the user has the accessibility setting on. A new `src/lib/reduceTransparency.ts` mirrors the appearance/accent pattern from sub-project #1.

**Tech Stack:** React 18, TypeScript, Tailwind CSS 3, Tauri 2, Rust with `objc2` 0.5 / `objc2-app-kit` 0.2 / `objc2-foundation` 0.2 / `block2` 0.5 (all from sub-project #1). Vitest 1.6 for frontend tests.

---

## File Structure

**Create:**
- `src-tauri/src/vibrancy.rs` — vibrancy attachment, transparency setting, reduce-transparency command, reduce-transparency observer.
- `src/lib/reduceTransparency.ts` — frontend tracker for `data-reduce-transparency` attribute and `vibrancy-unavailable` fallback.
- `src/lib/reduceTransparency.test.ts` — Vitest tests.

**Modify:**
- `src-tauri/Cargo.toml` — enable additional `objc2-app-kit` features (`NSVisualEffectView`, `NSWindow`, `NSView`, `NSWorkspace`).
- `src-tauri/src/lib.rs` — register `vibrancy` module, register `get_reduce_transparency` command, call `apply_vibrancy` + `set_window_transparent` + `install_reduce_transparency_observer` in setup.
- `src-tauri/tauri.conf.json` — set `"transparent": true`.
- `src/styles/tokens.css` — add `*-alpha` companion variables; add `[data-reduce-transparency="true"]` block; flip default alpha values to vibrancy-on after Rust side is wired.
- `tailwind.config.js` — change four token mappings from `<alpha-value>` to `var(--color-*-alpha)` composition.
- `src/main.tsx` — call `initReduceTransparency()` alongside existing inits.
- `src/components/TitleBar.tsx` — drop the "Meister" text label.
- `src/components/Sidebar.tsx` — add right-edge `border-r border-separator`.

---

## Task 1: Add `*-alpha` companion variables to `tokens.css` (defaults `1` — no visible change)

**Files:**
- Modify: `src/styles/tokens.css`

The default value for every alpha companion is `1` (fully opaque). No surfaces change appearance. This task purely introduces the new variables so subsequent tasks have something to override.

- [ ] **Step 1: Edit `src/styles/tokens.css`**

In the `:root, [data-appearance="light"]` block, add these four lines after the opaque `*-rgb` variables (typically right under the `Backgrounds (opaque)` group):

```css
  /* Backgrounds (opaque) — alpha companions, defaults opaque */
  --color-window-background-alpha:      1;
  --color-under-page-background-alpha:  1;
  --color-control-background-alpha:     1;
  --color-sidebar-background-alpha:     1;
```

In the `[data-appearance="dark"]` block, add the identical four lines in the same position.

- [ ] **Step 2: Verify the file is valid CSS**

```bash
cd /Users/nexus/projects/meister && npm run build
```

Expected: builds cleanly. No surfaces visibly change because Tailwind still composes via `<alpha-value>` (Task 2 changes the mapping).

- [ ] **Step 3: Commit**

```bash
git add src/styles/tokens.css
git commit -m "feat(tokens): add *-alpha companion variables for translucency"
```

---

## Task 2: Switch Tailwind mapping to compose against `*-alpha`

**Files:**
- Modify: `tailwind.config.js`

- [ ] **Step 1: Edit the four affected color entries**

Find these four entries in the `colors` block of `tailwind.config.js` and replace their values:

```diff
- "window-background":      "rgb(var(--color-window-background-rgb) / <alpha-value>)",
+ "window-background":      "rgb(var(--color-window-background-rgb) / var(--color-window-background-alpha))",
- "under-page-background":  "rgb(var(--color-under-page-background-rgb) / <alpha-value>)",
+ "under-page-background":  "rgb(var(--color-under-page-background-rgb) / var(--color-under-page-background-alpha))",
- "control-background":     "rgb(var(--color-control-background-rgb) / <alpha-value>)",
+ "control-background":     "rgb(var(--color-control-background-rgb) / var(--color-control-background-alpha))",
- "sidebar-background":     "rgb(var(--color-sidebar-background-rgb) / <alpha-value>)",
+ "sidebar-background":     "rgb(var(--color-sidebar-background-rgb) / var(--color-sidebar-background-alpha))",
```

Leave `accent`, `accent-hover`, `accent-pressed`, `accent-label`, `status-*`, `scrim`, and `control-selected-text` entries unchanged — they keep `<alpha-value>` because components use them with `/N` modifiers.

- [ ] **Step 2: Build**

```bash
cd /Users/nexus/projects/meister && npm run build
```

Expected: succeeds. Surfaces still appear identical (alpha is `1` from Task 1).

- [ ] **Step 3: Confirm no component uses `bg-window-background/N` or similar**

```bash
grep -rE "bg-(window-background|under-page-background|control-background|sidebar-background)/[0-9]" /Users/nexus/projects/meister/src
```

Expected: empty. If any match exists, it's a defect — those tokens lost alpha-modifier support. Migrate to a different token (e.g. `bg-fill-tertiary`).

- [ ] **Step 4: Commit**

```bash
git add tailwind.config.js
git commit -m "feat(tokens): compose translucent surfaces via alpha vars"
```

---

## Task 3: Add `[data-reduce-transparency="true"]` block to `tokens.css`

**Files:**
- Modify: `src/styles/tokens.css`

Adds the override block. Doesn't take effect until something sets the attribute on `<html>` (Task 5).

- [ ] **Step 1: Append block to `src/styles/tokens.css`**

After the existing `[data-appearance="dark"]` block, before the typography comment at the bottom:

```css
[data-reduce-transparency="true"] {
  --color-window-background-alpha:      1;
  --color-sidebar-background-alpha:     1;
  --color-control-background-alpha:     1;
  --color-under-page-background-alpha:  1;
}
```

- [ ] **Step 2: Build**

```bash
cd /Users/nexus/projects/meister && npm run build
```

Expected: succeeds.

- [ ] **Step 3: Commit**

```bash
git add src/styles/tokens.css
git commit -m "feat(tokens): add reduce-transparency override block"
```

---

## Task 4: Frontend `reduceTransparency.ts` (TDD)

**Files:**
- Create: `src/lib/reduceTransparency.ts`
- Create: `src/lib/reduceTransparency.test.ts`

- [ ] **Step 1: Write the failing test**

Create `src/lib/reduceTransparency.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  initReduceTransparency,
  isReduceTransparency,
  __resetForTests,
} from "./reduceTransparency";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const listenMock = listen as unknown as ReturnType<typeof vi.fn>;

describe("reduceTransparency", () => {
  beforeEach(() => {
    document.documentElement.removeAttribute("data-reduce-transparency");
    invokeMock.mockReset();
    listenMock.mockReset().mockResolvedValue(() => {});
    __resetForTests();
  });

  it("sets data-reduce-transparency='true' when the system setting is on", async () => {
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === "get_reduce_transparency" ? true : undefined,
    );
    await initReduceTransparency();
    expect(
      document.documentElement.dataset.reduceTransparency,
    ).toBe("true");
    expect(isReduceTransparency()).toBe(true);
  });

  it("removes the attribute when the system setting is off", async () => {
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === "get_reduce_transparency" ? false : undefined,
    );
    await initReduceTransparency();
    expect(
      document.documentElement.dataset.reduceTransparency,
    ).toBeUndefined();
    expect(isReduceTransparency()).toBe(false);
  });

  it("re-applies on system-reduce-transparency-changed events", async () => {
    invokeMock.mockResolvedValue(false);
    let handler: ((e: { payload: unknown }) => void) | undefined;
    listenMock.mockImplementation(async (event: string, cb: typeof handler) => {
      if (event === "system-reduce-transparency-changed") handler = cb;
      return () => {};
    });
    await initReduceTransparency();
    handler?.({ payload: true });
    expect(
      document.documentElement.dataset.reduceTransparency,
    ).toBe("true");
    handler?.({ payload: false });
    expect(
      document.documentElement.dataset.reduceTransparency,
    ).toBeUndefined();
  });

  it("falls back to false (vibrancy on) if invoke fails", async () => {
    invokeMock.mockRejectedValue(new Error("no tauri"));
    await initReduceTransparency();
    expect(isReduceTransparency()).toBe(false);
  });

  it("forces reduce-transparency mode when vibrancy-unavailable fires", async () => {
    invokeMock.mockResolvedValue(false);
    const handlers: Record<string, ((e: { payload: unknown }) => void) | undefined> = {};
    listenMock.mockImplementation(async (event: string, cb: typeof handlers[string]) => {
      handlers[event] = cb;
      return () => {};
    });
    await initReduceTransparency();
    expect(isReduceTransparency()).toBe(false);
    handlers["vibrancy-unavailable"]?.({ payload: null });
    expect(
      document.documentElement.dataset.reduceTransparency,
    ).toBe("true");
    expect(isReduceTransparency()).toBe(true);
  });

  it("is idempotent — calling init twice does not double-register", async () => {
    invokeMock.mockResolvedValue(false);
    await initReduceTransparency();
    await initReduceTransparency();
    expect(invokeMock).toHaveBeenCalledTimes(1);
  });
});
```

- [ ] **Step 2: Run the tests to confirm they fail**

```bash
cd /Users/nexus/projects/meister && npm run test:run -- reduceTransparency
```

Expected: FAIL — module missing.

- [ ] **Step 3: Implement `src/lib/reduceTransparency.ts`**

```ts
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

let current = false;
let initialized = false;

function apply(value: boolean) {
  current = value;
  if (value) {
    document.documentElement.dataset.reduceTransparency = "true";
  } else {
    delete document.documentElement.dataset.reduceTransparency;
  }
}

export function isReduceTransparency(): boolean {
  return current;
}

export async function initReduceTransparency(): Promise<void> {
  if (initialized) return;
  initialized = true;

  try {
    const value = await invoke<boolean>("get_reduce_transparency");
    apply(value);
  } catch (e) {
    console.warn(
      "reduceTransparency: get_reduce_transparency failed, defaulting to false",
      e,
    );
    apply(false);
  }

  try {
    await listen<boolean>("system-reduce-transparency-changed", (e) => {
      apply(Boolean(e.payload));
    });
  } catch (e) {
    console.warn("reduceTransparency: listen failed, no live updates", e);
  }

  try {
    await listen("vibrancy-unavailable", () => {
      apply(true);
    });
  } catch (e) {
    console.warn("reduceTransparency: vibrancy-unavailable listen failed", e);
  }
}

/** For tests only. */
export function __resetForTests(): void {
  current = false;
  initialized = false;
  document.documentElement.removeAttribute("data-reduce-transparency");
}
```

- [ ] **Step 4: Run tests to confirm they pass**

```bash
cd /Users/nexus/projects/meister && npm run test:run -- reduceTransparency
```

Expected: all 6 tests pass.

- [ ] **Step 5: Run full suite**

```bash
cd /Users/nexus/projects/meister && npm run test:run
```

Expected: 17 tests pass (6 appearance + 5 accent + 6 reduceTransparency).

- [ ] **Step 6: Commit**

```bash
git add src/lib/reduceTransparency.ts src/lib/reduceTransparency.test.ts
git commit -m "feat(reduce-transparency): track macOS Reduce Transparency setting"
```

---

## Task 5: Wire `initReduceTransparency()` into `main.tsx`

**Files:**
- Modify: `src/main.tsx`

- [ ] **Step 1: Edit `src/main.tsx`**

Replace the current contents with:

```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { initAppearance } from "./lib/appearance";
import { initAccent } from "./lib/accent";
import { initReduceTransparency } from "./lib/reduceTransparency";
import "./styles/tokens.css";
import "./index.css";

initAppearance();
void initAccent();
void initReduceTransparency();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
```

- [ ] **Step 2: Build**

```bash
cd /Users/nexus/projects/meister && npm run build
```

Expected: succeeds.

- [ ] **Step 3: Commit**

```bash
git add src/main.tsx
git commit -m "feat: initialize reduce-transparency tracking at boot"
```

---

## Task 6: Enable additional `objc2-app-kit` features in Cargo.toml

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Add features to the existing `objc2-app-kit` entry**

Find this block in `src-tauri/Cargo.toml`:

```toml
objc2-app-kit = { version = "0.2", features = [
    "NSAppearance",
    "NSColor",
    "NSColorSpace",
    "block2",
] }
```

Replace with:

```toml
objc2-app-kit = { version = "0.2", features = [
    "NSAppearance",
    "NSColor",
    "NSColorSpace",
    "NSResponder",
    "NSView",
    "NSVisualEffectView",
    "NSWindow",
    "NSWorkspace",
    "block2",
] }
```

- [ ] **Step 2: Build**

```bash
cd /Users/nexus/projects/meister/src-tauri && cargo build
```

Expected: succeeds. May take a minute on first compile of new features.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "chore(rust): enable AppKit features for vibrancy + workspace"
```

---

## Task 7: Rust `vibrancy.rs` — `apply_vibrancy` and `set_window_transparent`

**Files:**
- Create: `src-tauri/src/vibrancy.rs`
- Modify: `src-tauri/src/lib.rs`

The objc2-app-kit 0.2 API surface for `NSVisualEffectView` may need adjustment relative to the reference code below — implementer has discretion to match the actual API. The required behavior is: a vibrancy view of material `windowBackground`, blending mode `behindWindow`, state `followsWindowActiveState`, inserted as the bottom-most subview of the NSWindow's contentView, with auto-resizing on width and height.

- [ ] **Step 1: Create `src-tauri/src/vibrancy.rs` with the macOS impl + non-macOS stubs**

```rust
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
    use objc2::runtime::ProtocolObject;
    use objc2::ClassType;
    use objc2_app_kit::{
        NSAutoresizingMaskOptions, NSColor, NSView, NSVisualEffectBlendingMode,
        NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView, NSWindow,
    };
    use objc2_foundation::NSRect;

    /// SAFETY: Caller must invoke from the main thread.
    /// Tauri delivers setup on the main thread.
    pub unsafe fn attach_vibrancy(window: &NSWindow) -> Option<Retained<NSVisualEffectView>> {
        let content_view = window.contentView()?;
        let bounds: NSRect = content_view.bounds();

        let alloc = NSVisualEffectView::alloc();
        let view = NSVisualEffectView::initWithFrame(alloc, bounds);
        view.setMaterial(NSVisualEffectMaterial::WindowBackground);
        view.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        view.setState(NSVisualEffectState::FollowsWindowActiveState);
        view.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );

        let v_super: &NSView = ProtocolObject::cast(&view).as_super();
        content_view.addSubview_positioned_relativeTo(
            v_super,
            objc2_app_kit::NSWindowOrderingMode::Below,
            None,
        );

        Some(view)
    }

    /// SAFETY: Caller must invoke from the main thread.
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

    // SAFETY: Tauri exposes `ns_window` on the main thread; the pointer is
    // a valid NSWindow for the current process. We retain a copy to bridge
    // raw -> objc2 ownership.
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
```

If the `objc2-app-kit` 0.2 API names differ (e.g. `NSAutoresizingMaskOptions` variants, `addSubview_positioned_relativeTo` signature, `Retained::retain` ownership rules), adjust call sites to match. Keep:

- The public function names: `apply_vibrancy<R: Runtime>(window: &WebviewWindow<R>)` and `set_window_transparent<R: Runtime>(window: &WebviewWindow<R>)`.
- The `Result<(), String>` return shape.
- The `pub const VIBRANCY_UNAVAILABLE_EVENT: &str = "vibrancy-unavailable";` constant.

- [ ] **Step 2: Register `vibrancy` module in `src-tauri/src/lib.rs`**

Add this line near the other `mod`/`pub mod` declarations at the top of `src-tauri/src/lib.rs` (after `pub mod accent;`):

```rust
pub mod vibrancy;
```

- [ ] **Step 3: Build**

```bash
cd /Users/nexus/projects/meister/src-tauri && cargo build
```

Expected: builds cleanly. The two functions exist and compile but are not yet called — that happens in Task 9.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/vibrancy.rs src-tauri/src/lib.rs
git commit -m "feat(rust): add vibrancy module with apply_vibrancy + set_window_transparent"
```

---

## Task 8: Rust `vibrancy.rs` — `get_reduce_transparency` command + observer

**Files:**
- Modify: `src-tauri/src/vibrancy.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Append command + observer to `vibrancy.rs`**

Add this block to the bottom of `src-tauri/src/vibrancy.rs`:

```rust
use tauri::{AppHandle, Emitter};

pub const REDUCE_TRANSPARENCY_CHANGED_EVENT: &str = "system-reduce-transparency-changed";

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
    use objc2_foundation::{ns_string, NSNotificationCenter, NSOperationQueue};

    let app_for_cb = app.clone();
    let block = RcBlock::new(move |_note: *mut objc2_foundation::NSNotification| {
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
            &*block,
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
```

If the exact AppKit notification name string differs (e.g. `NSAccessibilityDisplayOptionsDidChangeNotification`, dropping the `Workspace` prefix), adjust accordingly. The Apple docs are the source of truth: `NSWorkspace.accessibilityDisplayOptionsDidChangeNotification`.

- [ ] **Step 2: Register `get_reduce_transparency` in `src-tauri/src/lib.rs`**

In the `tauri::generate_handler![ … ]` list, add `vibrancy::get_reduce_transparency,` (placement near the other module-prefixed commands).

- [ ] **Step 3: Build + run the new test**

```bash
cd /Users/nexus/projects/meister/src-tauri && cargo build && cargo test vibrancy
```

Expected: builds cleanly; test passes.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/vibrancy.rs src-tauri/src/lib.rs
git commit -m "feat(rust): observe NSWorkspace reduce-transparency changes"
```

---

## Task 9: Wire vibrancy + observer into `lib.rs` setup; emit fallback event on failure

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add the calls inside the setup block**

Inside the existing `.setup(|app| { … })` closure in `src-tauri/src/lib.rs`, after the existing `accent::install_observer(app.handle().clone());` line and before `Ok(())`:

```rust
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
```

You may need to add `use tauri::{Manager, Emitter};` at the top of `lib.rs` if those traits aren't already in scope. Check the existing imports first — `Manager` is likely already there.

- [ ] **Step 2: Build**

```bash
cd /Users/nexus/projects/meister/src-tauri && cargo build
```

Expected: builds cleanly.

- [ ] **Step 3: Smoke test — run dev and confirm app boots**

```bash
cd /Users/nexus/projects/meister && npm run tauri dev
```

In another terminal, wait ~15 seconds for compile + boot. The app window should appear. **At this point vibrancy is attached but not visible** — Task 1 set all alphas to `1`, so surfaces are still fully opaque, hiding the vibrancy view. You're confirming the app doesn't crash with `transparent: true` not yet set (Task 11). It may show a flash of an unstyled / system-default window background; that's fine.

Kill the dev process.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: attach vibrancy + observer at app setup; emit fallback on failure"
```

---

## Task 10: Set `transparent: true` in `tauri.conf.json`

**Files:**
- Modify: `src-tauri/tauri.conf.json`

- [ ] **Step 1: Edit the window block**

In `src-tauri/tauri.conf.json`, change the window's `transparent` field from `false` to `true`:

```diff
-        "transparent": false,
+        "transparent": true,
```

Other fields (`titleBarStyle: "Overlay"`, `decorations: true`) stay as-is.

- [ ] **Step 2: Build**

```bash
cd /Users/nexus/projects/meister && npm run tauri build 2>&1 | tail -20
```

(Or just `cd src-tauri && cargo build` plus `npm run build` if the full bundle build is slow — the goal is to confirm the config parses.)

- [ ] **Step 3: Smoke test**

```bash
cd /Users/nexus/projects/meister && npm run tauri dev
```

Wait for the app to launch. Now the window itself is transparent; surfaces are still fully opaque (alpha=1). Vibrancy attached in Task 9 but invisible behind opaque surfaces. The app should look essentially unchanged. Kill the dev process.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/tauri.conf.json
git commit -m "feat(window): make window transparent so vibrancy can show"
```

---

## Task 11: Flip `*-alpha` defaults to vibrancy-on values

**Files:**
- Modify: `src/styles/tokens.css`

This is the visible cutover. Until now, all alphas are `1` and the app looks identical. After this task, vibrancy actually shows.

- [ ] **Step 1: Update both light and dark blocks**

In `src/styles/tokens.css`, change the four `*-alpha` values inside `:root, [data-appearance="light"]`:

```diff
-  --color-window-background-alpha:      1;
-  --color-under-page-background-alpha:  1;
-  --color-control-background-alpha:     1;
-  --color-sidebar-background-alpha:     1;
+  --color-window-background-alpha:      0;
+  --color-under-page-background-alpha:  0.6;
+  --color-control-background-alpha:     0.92;
+  --color-sidebar-background-alpha:     0.4;
```

Make the identical change inside `[data-appearance="dark"]`.

The `[data-reduce-transparency="true"]` block (added in Task 3) keeps all alphas at `1` — that's the override path.

- [ ] **Step 2: Build**

```bash
cd /Users/nexus/projects/meister && npm run build
```

Expected: succeeds.

- [ ] **Step 3: Smoke test**

```bash
cd /Users/nexus/projects/meister && npm run tauri dev
```

Wait for the window to appear over a colorful desktop background. The window background should now show vibrancy — subtle wallpaper colors bleeding through. The sidebar should look slightly tinted relative to the main content. Cards (`bg-control-background`) should be ~92% opaque so vibrancy bleeds through their edges very subtly.

If vibrancy is invisible (window appears solid):
1. Confirm Reduce Transparency is OFF (System Settings → Accessibility → Display).
2. Confirm `data-reduce-transparency` attribute is NOT set on `<html>` (open dev tools, inspect `<html>`).
3. Confirm `transparent: true` is in `tauri.conf.json` and the build picked it up (kill + rebuild dev).

Kill the dev process.

- [ ] **Step 4: Commit**

```bash
git add src/styles/tokens.css
git commit -m "feat(tokens): enable vibrancy by lowering surface alphas"
```

---

## Task 12: TitleBar — drop the "Meister" label

**Files:**
- Modify: `src/components/TitleBar.tsx`

- [ ] **Step 1: Remove the label, keep the drag region**

Read `src/components/TitleBar.tsx`. Find the element rendering the word "Meister" (likely a `<span>` or `<div>` with that text). Delete it. Keep:

- The outer drag region container (`data-tauri-drag-region` or equivalent).
- Whitespace / sizing reservation for traffic lights (the left padding area).
- All className styling.

The result: a 28-32px-tall bar with native traffic lights at the left and empty drag space to the right.

- [ ] **Step 2: Build**

```bash
cd /Users/nexus/projects/meister && npm run build
```

Expected: succeeds.

- [ ] **Step 3: Smoke test**

```bash
cd /Users/nexus/projects/meister && npm run tauri dev
```

Confirm:
- No "Meister" text in the titlebar.
- Native traffic lights still present and functional (close, minimize, maximize).
- Window is still draggable by the titlebar area.

Kill the dev process.

- [ ] **Step 4: Commit**

```bash
git add src/components/TitleBar.tsx
git commit -m "refactor(ui): drop duplicate Meister label from TitleBar"
```

---

## Task 13: Sidebar — add right-edge separator

**Files:**
- Modify: `src/components/Sidebar.tsx`

- [ ] **Step 1: Add `border-r border-separator` to the sidebar root**

Read `src/components/Sidebar.tsx`. Find the root element (likely an `<aside>` or `<div>` with `bg-sidebar-background`). Add `border-r border-separator` to its className.

If the root already has a border on another side, append rather than replace.

- [ ] **Step 2: Build**

```bash
cd /Users/nexus/projects/meister && npm run build
```

Expected: succeeds.

- [ ] **Step 3: Smoke test**

```bash
cd /Users/nexus/projects/meister && npm run tauri dev
```

Confirm a 1px separator between the sidebar and main content. Should be visible in both light and dark mode (the `border-separator` token has different alpha values per appearance).

Kill the dev process.

- [ ] **Step 4: Run full suite**

```bash
cd /Users/nexus/projects/meister && npm run test:run
```

Expected: 17 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/components/Sidebar.tsx
git commit -m "refactor(ui): add separator divider between sidebar and main content"
```

---

## Task 14: Manual QA + tag

**Files:** none (verification only).

- [ ] **Step 1: Visual QA in dark mode**

```bash
cd /Users/nexus/projects/meister && npm run tauri dev
```

System Appearance: Dark.

Walk through:
1. Boot over a colorful desktop background. Window background picks up subtle tints.
2. Drag the window across the screen. Vibrancy re-renders.
3. Click outside the window. Vibrancy desaturates (inactive state).
4. Click back in. Vibrancy resaturates.
5. Sidebar/content boundary visible. Sidebar reads as visibly distinct (slight tint).
6. Cards (`RecordingView`, `StageCard`, `OutputPreview`) stay readable. Text legible; the 8% bleed-through doesn't wash out content.
7. No "Meister" text in titlebar.

- [ ] **Step 2: Visual QA in light mode**

Switch System Settings → Appearance to Light. Repeat steps 1–7.

If light-mode vibrancy looks too washed-out or sidebar too low-contrast, tune the `*-alpha` values in `src/styles/tokens.css` and recommit. Common tweaks:
- Sidebar tint stronger: `0.4` → `0.6`.
- Cards more solid: `0.92` → `0.96`.

- [ ] **Step 3: Reduce Transparency interaction**

System Settings → Accessibility → Display → Reduce Transparency: ON.

Within ~1s, Meister flips to fully opaque surfaces. Vibrancy gone. App should look correct (essentially the post-sub-project-#1 state).

Toggle Reduce Transparency OFF. Vibrancy returns within ~1s.

- [ ] **Step 4: Cross-app comparison**

Open Mail.app or Notes.app side by side with Meister in light mode. Compare vibrancy intensity. They should feel comparable. Tune `*-alpha` values if Meister looks notably more or less translucent.

- [ ] **Step 5: Vibrancy-fallback path**

This step verifies the `vibrancy-unavailable` recovery. There's no easy way to make `apply_vibrancy` actually fail on a working machine, so this is a confidence check by reading code:

- Confirm `src-tauri/src/lib.rs` setup emits `vibrancy::VIBRANCY_UNAVAILABLE_EVENT` on each of the two failure paths (apply + transparent).
- Confirm `src/lib/reduceTransparency.ts` listens for `vibrancy-unavailable` and calls `apply(true)`.

- [ ] **Step 6: Tag**

```bash
cd /Users/nexus/projects/meister && git tag native-feel/02-vibrancy-complete
```

Sub-project #2 is done. Next up is #3 (control library: buttons, segmented controls, inputs, focus rings, context menus) — separate spec and plan.
