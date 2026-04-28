# Chrome Basics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Spec:** `docs/superpowers/specs/2026-04-28-chrome-basics-design.md`

**Goal:** Ship a `Toolbar` primitive, a native macOS menu bar with HIG-correct keyboard shortcuts, and replace browser `confirm()` dialogs with `Tauri`-native ones.

**Architecture:** A new `Toolbar.tsx` UI primitive that `RecordingView` / `ProjectView` headers adopt. A Rust `menu.rs` module builds the macOS menu bar at Tauri `setup()` using `tauri::menu`; custom items emit Tauri events with a `menu:` prefix. Frontend `menuEvents.ts` subscribes via `listen()` and dispatches into handlers App.tsx registers per render. New-project / import-audio / run-pipeline handlers reach into Sidebar / RecordingView via `useImperativeHandle` refs. Toggle-sidebar flips a new App-level boolean. `confirm()` calls in `Sidebar` and `App.tsx` migrate to async `await dialogConfirm()`.

**Tech Stack:** React 18, TypeScript, Tauri 2 (`tauri::menu`), `@tauri-apps/plugin-dialog`, Vitest 1.6 + Testing Library.

---

## File Structure

**Create:**
- `src/components/ui/Toolbar.tsx`
- `src/components/ui/__tests__/Toolbar.test.tsx`
- `src/lib/menuEvents.ts`
- `src/lib/menuEvents.test.ts`
- `src-tauri/src/menu.rs`

**Modify:**
- `src/components/ui/index.ts` — export Toolbar.
- `src/components/RecordingView.tsx` — wrap header with `<Toolbar>`, expose imperative `runPipeline()` via `useImperativeHandle`.
- `src/components/ProjectView.tsx` — wrap header with `<Toolbar>`.
- `src/components/Sidebar.tsx` — replace `confirm()` with `dialogConfirm()`, expose imperative `beginNewProject()` via `useImperativeHandle`.
- `src/App.tsx` — sidebar visibility state + conditional render, refs to Sidebar/RecordingView, register menu-event handlers, replace `confirm()` in `guardedSelect` with `dialogConfirm()`, call `initMenuEvents()` at boot.
- `src-tauri/src/lib.rs` — register `pub mod menu;`, set up the menu in `setup()`, route menu events.

---

## Task 1: Toolbar primitive (TDD)

**Files:**
- Create: `src/components/ui/Toolbar.tsx`
- Create: `src/components/ui/__tests__/Toolbar.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { Toolbar } from "../Toolbar";

describe("Toolbar", () => {
  it("renders children inside a horizontal row", () => {
    render(
      <Toolbar>
        <span data-testid="left">Left</span>
        <span data-testid="right">Right</span>
      </Toolbar>,
    );
    expect(screen.getByTestId("left")).toBeInTheDocument();
    expect(screen.getByTestId("right")).toBeInTheDocument();
  });

  it("applies a bottom separator border", () => {
    const { container } = render(
      <Toolbar>
        <span>x</span>
      </Toolbar>,
    );
    const root = container.firstChild as HTMLElement;
    expect(root.className).toMatch(/border-b/);
    expect(root.className).toMatch(/border-separator/);
  });

  it("Toolbar.Spacer renders a flex-1 div", () => {
    const { container } = render(
      <Toolbar>
        <Toolbar.Spacer />
      </Toolbar>,
    );
    const spacer = container.querySelector("div > div");
    expect(spacer?.className).toMatch(/flex-1/);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd /Users/nexus/projects/meister && npm run test:run -- Toolbar
```

Expected: FAIL — module missing.

- [ ] **Step 3: Implement `src/components/ui/Toolbar.tsx`**

```tsx
import { forwardRef } from "react";
import { cn } from "../../lib/cn";

type RootProps = React.HTMLAttributes<HTMLDivElement>;

const Root = forwardRef<HTMLDivElement, RootProps>(function Toolbar(
  { className, children, ...rest },
  ref,
) {
  return (
    <div
      ref={ref}
      className={cn(
        "flex h-11 items-center gap-2 px-4 border-b border-separator",
        className,
      )}
      {...rest}
    >
      {children}
    </div>
  );
});

const Spacer = () => <div className="flex-1" />;

export const Toolbar = Object.assign(Root, { Spacer });
```

- [ ] **Step 4: Run tests to confirm they pass**

```bash
cd /Users/nexus/projects/meister && npm run test:run -- Toolbar
```

Expected: 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/components/ui/Toolbar.tsx src/components/ui/__tests__/Toolbar.test.tsx
git commit -m "feat(ui): add Toolbar primitive with Spacer"
```

---

## Task 2: Export Toolbar from barrel

**Files:**
- Modify: `src/components/ui/index.ts`

- [ ] **Step 1: Append `Toolbar` export**

After the existing `export { ContextMenu } from "./ContextMenu";` line, add:

```ts
export { Toolbar } from "./Toolbar";
```

- [ ] **Step 2: Build + run full suite**

```bash
cd /Users/nexus/projects/meister && npm run build && npm run test:run
```

Expected: build clean, 53 tests pass (50 prior + 3 Toolbar).

- [ ] **Step 3: Commit**

```bash
git add src/components/ui/index.ts
git commit -m "feat(ui): export Toolbar from ui barrel"
```

---

## Task 3: menuEvents.ts (TDD)

**Files:**
- Create: `src/lib/menuEvents.ts`
- Create: `src/lib/menuEvents.test.ts`

- [ ] **Step 1: Write the failing test**

```ts
import { describe, it, expect, vi, beforeEach } from "vitest";
import { listen } from "@tauri-apps/api/event";
import {
  initMenuEvents,
  setMenuHandler,
  __resetForTests,
} from "./menuEvents";

const listenMock = listen as unknown as ReturnType<typeof vi.fn>;

describe("menuEvents", () => {
  beforeEach(() => {
    listenMock.mockReset().mockResolvedValue(() => {});
    __resetForTests();
  });

  it("subscribes to all menu events on init", async () => {
    await initMenuEvents();
    const subscribed = listenMock.mock.calls.map((c) => c[0]);
    expect(subscribed).toEqual(
      expect.arrayContaining([
        "menu:new-project",
        "menu:import-audio",
        "menu:run-pipeline",
        "menu:toggle-sidebar",
      ]),
    );
  });

  it("invokes the registered handler when an event fires", async () => {
    const handlers: Record<string, ((e: { payload: unknown }) => void) | undefined> = {};
    listenMock.mockImplementation(async (event: string, cb: typeof handlers[string]) => {
      handlers[event] = cb;
      return () => {};
    });
    await initMenuEvents();

    const fn = vi.fn();
    setMenuHandler("menu:new-project", fn);
    handlers["menu:new-project"]?.({ payload: null });
    expect(fn).toHaveBeenCalled();
  });

  it("is idempotent — calling init twice does not double-subscribe", async () => {
    await initMenuEvents();
    await initMenuEvents();
    // 4 events × 1 init = 4 listen calls expected.
    expect(listenMock).toHaveBeenCalledTimes(4);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd /Users/nexus/projects/meister && npm run test:run -- menuEvents
```

Expected: FAIL — module missing.

- [ ] **Step 3: Implement `src/lib/menuEvents.ts`**

```ts
import { listen } from "@tauri-apps/api/event";

export type MenuEventName =
  | "menu:new-project"
  | "menu:import-audio"
  | "menu:run-pipeline"
  | "menu:toggle-sidebar";

type Handler = () => void;

const handlers = new Map<MenuEventName, Handler>();
let initialized = false;

export function setMenuHandler(event: MenuEventName, handler: Handler): void {
  handlers.set(event, handler);
}

export async function initMenuEvents(): Promise<void> {
  if (initialized) return;
  initialized = true;
  const events: MenuEventName[] = [
    "menu:new-project",
    "menu:import-audio",
    "menu:run-pipeline",
    "menu:toggle-sidebar",
  ];
  for (const ev of events) {
    try {
      await listen(ev, () => {
        const h = handlers.get(ev);
        if (h) h();
      });
    } catch (e) {
      console.warn(`menuEvents: listen failed for ${ev}`, e);
    }
  }
}

/** For tests only. */
export function __resetForTests(): void {
  handlers.clear();
  initialized = false;
}
```

- [ ] **Step 4: Run tests to confirm they pass**

```bash
cd /Users/nexus/projects/meister && npm run test:run -- menuEvents
```

Expected: 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/lib/menuEvents.ts src/lib/menuEvents.test.ts
git commit -m "feat(menu): add menuEvents bus for Tauri menu → frontend dispatch"
```

---

## Task 4: Rust menu builder

**Files:**
- Create: `src-tauri/src/menu.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Create `src-tauri/src/menu.rs`**

```rust
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

    let preferences = MenuItemBuilder::new("Preferences…")
        .id("menu:preferences")
        .accelerator("CmdOrCtrl+,")
        .enabled(false) // re-enabled in sub-project #4c when Preferences pane lands
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

    let file_submenu = SubmenuBuilder::new(app, "File")
        .item(&new_project)
        .item(&import_audio)
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

    // App-specific actions (under File or its own; we put it under "Recording")
    let run_pipeline = MenuItemBuilder::new("Run")
        .id("menu:run-pipeline")
        .accelerator("CmdOrCtrl+R")
        .build(app)?;

    let recording_submenu = SubmenuBuilder::new(app, "Recording")
        .item(&run_pipeline)
        .build()?;

    // Window
    let window_submenu = SubmenuBuilder::new(app, "Window")
        .item(&PredefinedMenuItem::minimize(app, None)?)
        .item(&PredefinedMenuItem::maximize(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::bring_all_to_front(app, None)?)
        .build()?;

    // Help (placeholder — populated in sub-project #4b)
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
```

If the exact `tauri::menu` API names differ from the reference (e.g. `PredefinedMenuItem::maximize` may be `zoom` instead, `bring_all_to_front` may have a different name in the installed Tauri version), adjust call sites to match. The structure (App/File/Edit/View/Recording/Window/Help) and item IDs (`menu:new-project`, `menu:import-audio`, `menu:run-pipeline`, `menu:toggle-sidebar`, `menu:preferences`) MUST stay identical — those are the contract with the frontend.

- [ ] **Step 2: Register `pub mod menu;` in `src-tauri/src/lib.rs`**

Near the other `pub mod` declarations at the top of `lib.rs` (after `pub mod vibrancy;` if present):

```rust
pub mod menu;
```

- [ ] **Step 3: Build to verify compilation**

```bash
cd /Users/nexus/projects/meister/src-tauri && cargo build
```

Expected: builds cleanly. Some `unused` warnings about menu items not yet wired are fine.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/menu.rs src-tauri/src/lib.rs
git commit -m "feat(rust): add macOS menu bar builder"
```

---

## Task 5: Wire menu in lib.rs setup + on_menu_event handler

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Set the menu and route events in `setup()`**

In `src-tauri/src/lib.rs`, find the existing `tauri::Builder::default()` chain. We need to:
1. Add `.menu(|handle| menu::build_menu(handle))` BEFORE `.setup(...)` so the menu is built and attached to all windows automatically.
2. Add `.on_menu_event(...)` to dispatch custom items as Tauri events.

The exact insertion: replace the start of the builder chain:

```rust
tauri::Builder::default()
    .plugin(tauri_plugin_dialog::init())
    .plugin(tauri_plugin_fs::init())
    .plugin(tauri_plugin_shell::init())
    // ... existing chain
```

with:

```rust
tauri::Builder::default()
    .menu(|handle| menu::build_menu(handle))
    .on_menu_event(|app, event| {
        // Custom items have IDs prefixed with "menu:" — emit them on the
        // same channel name so the frontend can listen via menuEvents.ts.
        let id = event.id().0.as_str();
        if id.starts_with("menu:") {
            let _ = app.emit(id, ());
        }
    })
    .plugin(tauri_plugin_dialog::init())
    .plugin(tauri_plugin_fs::init())
    .plugin(tauri_plugin_shell::init())
    // ... existing chain unchanged after this point
```

You may need to add `use tauri::Emitter;` to the imports at the top of `lib.rs` if not already present (sub-project #2 added it).

- [ ] **Step 2: Build and run the app briefly**

```bash
cd /Users/nexus/projects/meister && npm run tauri dev
```

Wait for the app to launch. The macOS menu bar should now show **Meister | File | Edit | View | Recording | Window | Help** instead of just "Meister".

Try clicking each menu — the items should appear. Custom items (New Project, Import Audio…, Run, Toggle Sidebar) currently do nothing because the frontend isn't subscribed yet. Predefined items (Hide, Quit, Cut/Copy/Paste/Select All, Minimize, Zoom) should work immediately.

Press ⌘N — nothing visible happens (no handler yet). Press ⌘W — window closes (Tauri default). Reopen via dock.

Kill the dev process.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(rust): attach macOS menu and route custom items to Tauri events"
```

---

## Task 6: Sidebar — expose imperative beginNewProject + replace confirm()

**Files:**
- Modify: `src/components/Sidebar.tsx`

- [ ] **Step 1: Convert Sidebar to forwardRef + add imperative handle**

In `src/components/Sidebar.tsx`, find the current default export (around line 30):

```tsx
export default function Sidebar({ ... }: Props) {
```

Convert to a `forwardRef` with `useImperativeHandle`. Specifically, add to the imports:

```tsx
import { forwardRef, useImperativeHandle, useState } from "react";
import { confirm as dialogConfirm } from "@tauri-apps/plugin-dialog";
```

(Drop the existing `import { useState } from "react";`.)

Define the handle type at the top of the file (after imports):

```tsx
export interface SidebarHandle {
  beginNewProject: () => void;
}
```

Convert the default export. The body (everything from `const [expanded, setExpanded]…` to the JSX return) stays the same — only the function signature changes. Replace:

```tsx
export default function Sidebar({
  projects,
  recordingsByProject,
  selection,
  onSelect,
  onProjectsChanged,
  onPickFilesForProject,
}: Props) {
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [creating, setCreating] = useState(false);
  const [newName, setNewName] = useState("");
  // ...rest of body...
}
```

with:

```tsx
const Sidebar = forwardRef<SidebarHandle, Props>(function Sidebar(
  {
    projects,
    recordingsByProject,
    selection,
    onSelect,
    onProjectsChanged,
    onPickFilesForProject,
  },
  ref,
) {
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [creating, setCreating] = useState(false);
  const [newName, setNewName] = useState("");

  useImperativeHandle(ref, () => ({
    beginNewProject: () => {
      setCreating(true);
      setNewName("");
    },
  }));

  // ...rest of body unchanged...
});

export default Sidebar;
```

- [ ] **Step 2: Replace `confirm()` with `dialogConfirm()`**

Find these two existing helpers in `Sidebar.tsx` (added in sub-project #3b):

```tsx
const deleteProject = async (p: Project) => {
  if (!confirm(`Delete project "${p.name}" and all its recordings?`)) return;
  // ...
};

const deleteRecording = async (r: Recording) => {
  if (!confirm(`Delete recording "${r.name}"?`)) return;
  // ...
};
```

Replace each `confirm(...)` call with `await dialogConfirm(...)`:

```tsx
const deleteProject = async (p: Project) => {
  const ok = await dialogConfirm(
    `Delete project "${p.name}" and all its recordings?`,
    { title: "Delete project", kind: "warning" },
  );
  if (!ok) return;
  // ...
};

const deleteRecording = async (r: Recording) => {
  const ok = await dialogConfirm(`Delete recording "${r.name}"?`, {
    title: "Delete recording",
    kind: "warning",
  });
  if (!ok) return;
  // ...
};
```

- [ ] **Step 3: Build + run tests**

```bash
cd /Users/nexus/projects/meister && npm run build && npm run test:run
```

Expected: build clean, 56 tests pass (53 prior + 3 menuEvents).

- [ ] **Step 4: Commit**

```bash
git add src/components/Sidebar.tsx
git commit -m "feat(sidebar): expose imperative beginNewProject + native dialogs"
```

---

## Task 7: RecordingView — expose imperative runPipeline

**Files:**
- Modify: `src/components/RecordingView.tsx`

- [ ] **Step 1: Convert RecordingView to forwardRef + add imperative handle**

In `src/components/RecordingView.tsx`, find the existing run handler (the function called when the user clicks the Run button). It's typically named something like `handleRun`, `onRun`, or inline as the `onClick`. Read the file first to identify the actual name and arguments — the run handler is the function attached to the prominent `<Button variant="primary" size="lg">Run</Button>`'s `onClick`.

Once you've identified the run-button's onClick handler (call it `runPipeline` for the rest of this task; rename the local helper to `runPipeline` if it has a different name), expose it via `useImperativeHandle`:

Add to imports:

```tsx
import { forwardRef, useImperativeHandle, /* existing imports */ } from "react";
```

Define the handle type at the top:

```tsx
export interface RecordingViewHandle {
  runPipeline: () => void;
}
```

Convert the default export to `forwardRef<RecordingViewHandle, Props>(...)` with the same body, plus a `useImperativeHandle` call after the existing handler definitions:

```tsx
useImperativeHandle(ref, () => ({
  runPipeline: () => {
    // Call the actual run handler here. If the handler requires args
    // (e.g. format options), supply them with sensible defaults.
    runPipeline();
  },
}));
```

(If the existing run handler is named differently, e.g. `handleRunClick`, rename the imperative wrapper to call the real handler — `runPipeline: () => handleRunClick()`.)

Update the export to `export default RecordingView;` after closing the `forwardRef`.

- [ ] **Step 2: Build**

```bash
cd /Users/nexus/projects/meister && npm run build
```

Expected: builds cleanly. If TypeScript complains about the run handler's signature mismatch (e.g. the original takes a `MouseEvent`), wrap accordingly (`runPipeline: () => runPipeline(undefined as never)` is **not** acceptable — instead refactor the underlying handler to take no args, or wrap with appropriate defaults).

- [ ] **Step 3: Commit**

```bash
git add src/components/RecordingView.tsx
git commit -m "feat(recording): expose imperative runPipeline for menu integration"
```

---

## Task 8: Toolbar integration in RecordingView header

**Files:**
- Modify: `src/components/RecordingView.tsx`

- [ ] **Step 1: Wrap the existing header in `<Toolbar>`**

`RecordingView.tsx`'s top section currently renders something like:

```tsx
<div className="flex items-center justify-between">
  {/* recording-name input/title + Delete button + status */}
</div>
```

Replace the wrapping `<div>` with `<Toolbar>`. Because the existing flex container has `justify-between`, the new structure uses `<Toolbar.Spacer />` to push trailing items to the right:

```tsx
<Toolbar data-tauri-drag-region>
  {/* leading: recording-name input/title (existing children) */}
  <Toolbar.Spacer />
  {/* trailing: status text + Delete button (existing children) */}
</Toolbar>
```

If the original used `justify-between` with a single child on each side, the layout maps directly — left children stay before the Spacer; right children after. If the original had three or more sibling groups, place a `<Toolbar.Spacer />` between every group that should expand to push the next group to the edge.

Add `import { Toolbar } from "./ui";` to the imports (alongside existing `import { Button } from "./ui";` — combine into `import { Button, Toolbar } from "./ui";`).

The existing `p-4`, `p-8`, or whatever inner padding the parent container has should be preserved — `<Toolbar>` brings its own `px-4` so consumers may want to drop conflicting horizontal padding from the parent.

- [ ] **Step 2: Build + smoke**

```bash
cd /Users/nexus/projects/meister && npm run build && npm run tauri dev
```

Open a recording. The header now sits in a 44px row with a bottom border. Drag the window by the toolbar — works.

Kill dev.

- [ ] **Step 3: Commit**

```bash
git add src/components/RecordingView.tsx
git commit -m "refactor(recording): migrate header to Toolbar primitive"
```

---

## Task 9: Toolbar integration in ProjectView header

**Files:**
- Modify: `src/components/ProjectView.tsx`

- [ ] **Step 1: Wrap header with Toolbar**

Same pattern as RecordingView. `ProjectView.tsx`'s top section renders the project-name rename input, Delete button, etc. Replace the wrapping flex `<div>` with `<Toolbar>`:

```tsx
<Toolbar data-tauri-drag-region>
  {/* leading: project-name input (existing) */}
  <Toolbar.Spacer />
  {/* trailing: Delete button (existing) */}
</Toolbar>
```

Add `import { Toolbar } from "./ui";` to the imports (combine with existing `Button` / `Checkbox` imports).

- [ ] **Step 2: Build + smoke**

```bash
cd /Users/nexus/projects/meister && npm run build && npm run tauri dev
```

Open a project. Header looks like RecordingView's. Drag window by toolbar — works. Kill dev.

- [ ] **Step 3: Commit**

```bash
git add src/components/ProjectView.tsx
git commit -m "refactor(project): migrate header to Toolbar primitive"
```

---

## Task 10: App.tsx — sidebar visibility, menu-event subscription, refs

**Files:**
- Modify: `src/App.tsx`

This is the biggest task in the plan. It wires up everything from the menu bar to the existing handlers.

- [ ] **Step 1: Add imports**

At the top of `src/App.tsx`, add:

```tsx
import { useCallback, useEffect, useRef, useState } from "react";
import Sidebar, { type SidebarHandle } from "./components/Sidebar";
import RecordingView, { type RecordingViewHandle } from "./components/RecordingView";
import ProjectView from "./components/ProjectView";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { confirm as dialogConfirm } from "@tauri-apps/plugin-dialog";
import { initMenuEvents, setMenuHandler } from "./lib/menuEvents";
import { api } from "./api/storage";
import type { Project, Recording } from "./types/storage";
```

(The `import Sidebar from "./components/Sidebar"` and `import RecordingView from "./components/RecordingView"` lines may already exist — replace them with the new shapes that bring in the handle types.)

- [ ] **Step 2: Add sidebar visibility state, refs, and run useEffect**

Inside the `App` component, after the existing state declarations (just after `const [pendingStagedPaths, setPendingStagedPaths] = useState(...)`), add:

```tsx
const [sidebarVisible, setSidebarVisible] = useState(true);
const sidebarRef = useRef<SidebarHandle>(null);
const recordingViewRef = useRef<RecordingViewHandle>(null);
```

- [ ] **Step 3: Add the audio-import handler**

After the existing `guardedSelect` function definition, add:

```tsx
const AUDIO_VIDEO_EXTS = [
  "mp4", "mov", "mkv", "avi", "mp3", "m4a", "wav", "ogg", "webm", "aac", "flac",
];

const importAudioToActiveProject = useCallback(async () => {
  // Determine which project receives the imported audio.
  let projectId: string | null = null;
  if (selection.kind === "project") projectId = selection.projectId;
  else if (selection.kind === "recording") projectId = selection.projectId;
  if (!projectId) return;

  const picked = await openDialog({
    multiple: true,
    filters: [{ name: "Audio / Video", extensions: AUDIO_VIDEO_EXTS }],
  });
  if (!picked) return;
  const list = Array.isArray(picked) ? picked : [picked];
  if (list.length === 0) return;
  guardedSelect({ kind: "project", projectId });
  setPendingStagedPaths({ projectId, paths: list });
}, [selection, guardedSelect]);
```

- [ ] **Step 4: Replace the `guardedSelect` `confirm()` with `dialogConfirm()`**

The existing `guardedSelect` uses a synchronous `confirm()`. Make it async:

```tsx
const guardedSelect = useCallback(async (next: Selection) => {
  if (activeCaptureRef.current) {
    const ok = await dialogConfirm("Recording in progress — stop and discard?", {
      title: "Stop recording",
      kind: "warning",
    });
    if (!ok) return;
    activeCaptureRef.current();
    activeCaptureRef.current = null;
  }
  setSelection(next);
}, []);
```

Note: changing the signature to async means the `Sidebar`'s `onSelect` prop now expects a `(s: Selection) => Promise<void>` instead of `(s: Selection) => void`. TypeScript narrows happily (Promise-returning function is assignable where void-returning is expected). No changes needed at the consumer side.

The `onPickFilesForProject` callback inside the `<Sidebar>` JSX call (currently passes a synchronous `guardedSelect({ kind: "project", projectId })`) becomes:

```tsx
onPickFilesForProject={(projectId, paths) => {
  void guardedSelect({ kind: "project", projectId });
  setPendingStagedPaths({ projectId, paths });
}}
```

The leading `void` discards the returned promise (we don't need to await before staging the paths).

- [ ] **Step 5: Initialize menu events at boot**

Add a `useEffect` that runs once on mount, after the existing `useEffect(() => { refreshProjects()... }, [refreshProjects]);` block:

```tsx
useEffect(() => {
  void initMenuEvents();
}, []);
```

- [ ] **Step 6: Register menu-event handlers per render**

Add another `useEffect` that registers handlers on every render so closures over `selection` etc. always read the latest state:

```tsx
useEffect(() => {
  setMenuHandler("menu:new-project", () => {
    sidebarRef.current?.beginNewProject();
  });
  setMenuHandler("menu:import-audio", () => {
    void importAudioToActiveProject();
  });
  setMenuHandler("menu:run-pipeline", () => {
    recordingViewRef.current?.runPipeline();
  });
  setMenuHandler("menu:toggle-sidebar", () => {
    setSidebarVisible((v) => !v);
  });
}, [importAudioToActiveProject]);
```

- [ ] **Step 7: Wire refs and conditional sidebar render**

In the JSX `return`, find the existing `<Sidebar ... />` element. Add `ref={sidebarRef}`:

```tsx
<Sidebar
  ref={sidebarRef}
  projects={projects}
  // ...rest of props unchanged
/>
```

Wrap the `<Sidebar>` element in a conditional that responds to `sidebarVisible`. Use a CSS-driven collapse rather than removing the element so React state inside Sidebar (e.g. expanded folders) survives the toggle:

```tsx
<div
  className={`transition-all duration-200 ${
    sidebarVisible ? "w-60 opacity-100" : "w-0 opacity-0"
  } overflow-hidden shrink-0`}
>
  <Sidebar
    ref={sidebarRef}
    projects={projects}
    recordingsByProject={recordingsByProject}
    selection={selection}
    onSelect={guardedSelect}
    onProjectsChanged={refreshProjects}
    onPickFilesForProject={(projectId, paths) => {
      void guardedSelect({ kind: "project", projectId });
      setPendingStagedPaths({ projectId, paths });
    }}
  />
</div>
```

The original `<Sidebar>` had `className="w-60 shrink-0 rounded-xl bg-sidebar-background flex flex-col"` (set inside Sidebar.tsx). The wrapping `<div>` collapses width via Tailwind transitions; the inner Sidebar keeps its rounded-xl pill shape.

Add `ref={recordingViewRef}` to the existing `<RecordingView ... />` element:

```tsx
{selection.kind === "recording" && (
  <RecordingView
    ref={recordingViewRef}
    recordingId={selection.recordingId}
    // ...rest unchanged
  />
)}
```

- [ ] **Step 8: Build**

```bash
cd /Users/nexus/projects/meister && npm run build
```

Expected: build clean. TypeScript may complain about `Sidebar` not accepting a `ref` prop if Task 6's `forwardRef` conversion didn't land — verify Task 6 was done.

- [ ] **Step 9: Smoke test**

```bash
cd /Users/nexus/projects/meister && npm run tauri dev
```

In the running app:
- Press ⌘N — sidebar enters new-project input mode.
- Press ⌃⌘S — sidebar collapses with smooth transition. Press again — sidebar reopens.
- With a recording selected, press ⌘R — pipeline runs.
- With a project selected, press ⌘⇧I — file picker opens; choose audio files; staging area appears.
- Right-click a project row → Delete. Native dialog (not browser `confirm`) appears.

Kill dev.

- [ ] **Step 10: Commit**

```bash
git add src/App.tsx
git commit -m "feat(app): wire menu-bar events to Sidebar/RecordingView; add sidebar toggle; native dialogs"
```

---

## Task 11: Manual QA + tag

**Files:** none (verification only).

- [ ] **Step 1: Functional QA — menu bar**

```bash
cd /Users/nexus/projects/meister && npm run tauri dev
```

Walk the menu bar:

- **Meister** menu: About Meister opens the system About panel. Preferences… is greyed out. Hide / Hide Others / Show All / Quit work.
- **File**: New Project (⌘N) → sidebar enters create mode. Import Audio… (⌘⇧I) — opens file picker for the active project. Close Window (⌘W) closes the window.
- **Edit**: Cut / Copy / Paste / Select All. Test inside an `<Input>` (rename a project; verify ⌘C and ⌘V work).
- **View**: Toggle Sidebar (⌃⌘S) — sidebar collapses smoothly.
- **Recording**: Run (⌘R) — runs the pipeline if a recording is selected; no-op otherwise.
- **Window**: Minimize, Zoom, Bring All to Front.
- **Help**: empty.

- [ ] **Step 2: Functional QA — Toolbar**

- Open RecordingView for any recording. Header sits in a 44px bordered row.
- Drag the window by the toolbar area — drags.
- Same in ProjectView.

- [ ] **Step 3: Functional QA — native dialogs**

- Right-click a project → Delete. Native dialog (with macOS appearance, warning icon). OK and Cancel both behave correctly.
- Same with a recording.
- Start a live recording. Try to switch to a different project. Native dialog asks "Recording in progress — stop and discard?" OK proceeds; Cancel keeps the recording.

- [ ] **Step 4: Keyboard / accessibility QA**

- Tab through the app. Inputs accept Cut / Copy / Paste via menu and via keyboard.
- The collapsed sidebar (after ⌃⌘S) is visually hidden but Tab order skips its content (because `overflow-hidden` + width-0 makes it not focusable in practice).

- [ ] **Step 5: Cross-app comparison**

- Open Notes.app or Music.app. Compare menu-bar feel: standard items match (Cut/Copy/Paste/Select All, Hide/Quit, Window items).
- Compare Run-style action: Music has ⌘R for "Refresh"; ours uses ⌘R for "Run". Acceptable — both consistent within their app.

- [ ] **Step 6: Tag**

```bash
cd /Users/nexus/projects/meister && git tag native-feel/04a-chrome-basics-complete
```

Sub-project #4a is done. Next: sub-project #4b (notifications + Help / About) — separate spec and plan.
