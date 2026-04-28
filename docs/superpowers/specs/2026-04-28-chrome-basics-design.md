# Chrome Basics (Sub-project #4a)

First slice of sub-project #4 ("macOS chrome") in the native-macOS overhaul. Pulls together the work that gives Meister a real macOS chrome story: a `Toolbar` primitive, a native macOS menu bar with the standard menus and macOS-correct keyboard shortcuts, and `Tauri`-native dialogs replacing browser `confirm()` calls. Sub-projects #4b (notifications + Help / About) and #4c (Preferences pane + Undo) follow.

## Context

After sub-projects #1–#3b, Meister has a credible visual chrome — vibrancy, islands, polished controls, right-click context menus, inline rename. But the macOS *menu bar* still says "Meister" with no sub-menus the user can open. There's no ⌘N to make a new project. ⌘C inside an `<input>` works because WebKit defaults handle it, but Cut/Copy/Paste don't appear in any menu, so a user looking for them has no native discovery path. Delete confirmations are browser `confirm()` dialogs styled by WebKit — they look web-app-y. Each header in `RecordingView` / `ProjectView` is its own ad-hoc title-plus-actions row with no shared primitive.

This slice closes those gaps. Sub-projects #4b and #4c layer additional polish (notifications, Help menu, About dialog, Preferences pane, undo/redo) on top of the foundation built here.

## Goals

- Ship a `Toolbar` primitive in `src/components/ui/`. Thin horizontal-row component with consistent height, padding, and bottom separator. Has a `Toolbar.Spacer` member that pushes adjacent siblings apart.
- Migrate `RecordingView` and `ProjectView` headers to use `<Toolbar>`.
- Build a native macOS menu bar in Rust at Tauri `setup()` using `tauri::menu`:
  - **App menu (Meister):** About Meister (placeholder; full dialog comes in #4b), Preferences… (⌘,) — disabled in #4a, enabled in #4c, plus Hide, Hide Others, Show All, Quit (predefined).
  - **File:** New Project (⌘N), Import Audio… (⌘⇧I), Close Window (⌘W — predefined).
  - **Edit:** Cut / Copy / Paste / Select All (predefined macOS items so the OS routes them to focused inputs / textareas).
  - **View:** Toggle Sidebar (⌃⌘S).
  - **Window:** Minimize, Zoom, Bring All to Front (predefined).
  - **Help:** empty (placeholder; populated in #4b).
- Custom menu items emit Tauri events with the `"menu:"` prefix:
  - `menu:new-project`
  - `menu:import-audio`
  - `menu:run-pipeline` (⌘R)
  - `menu:toggle-sidebar`
- Frontend subscribes via a small `src/lib/menuEvents.ts` module that wires each event into the corresponding handler. Active-recording-aware actions (⌘R) read the current `selection` state.
- Replace browser `confirm()` calls in `Sidebar.tsx` and `App.tsx` with `await dialog.confirm()` from `@tauri-apps/plugin-dialog`. Refactor call sites to be `async`.

## Non-goals

- No notifications. Sub-project #4b owns those.
- No About dialog or Help menu population. Sub-project #4b.
- No Preferences pane. Sub-project #4c. The menu item is rendered but disabled in this slice.
- No Undo / Redo. Sub-project #4c.
- No global hotkeys via `tauri-plugin-global-shortcut`. The menu's accelerators are window-scoped, which is what HIG expects.
- No menu-item dynamic enable/disable based on selection state (e.g. "Run" greying out when no recording is selected). The handler is a no-op when no recording is active; menu state stays static for now. Dynamic state is added in #4c when there are more state-coupled items to justify the plumbing.
- No File-menu Export, Show in Finder, or recent-projects submenu.
- No platform parity for non-macOS. Tauri allows menus on Linux / Windows but the menu bar is macOS-specific in spirit; on other platforms Tauri renders a window-internal menu strip. Out of scope; we test only on macOS.

## Architecture

### File layout

```
src/components/ui/
  Toolbar.tsx                ← new
  index.ts                   ← updated to export Toolbar

src/lib/
  menuEvents.ts              ← new: subscribes to Tauri menu events, exposes
                                hooks/helpers for App.tsx

src-tauri/src/
  menu.rs                    ← new: builds the macOS menu in Rust
  lib.rs                     ← wire menu::build() in setup() and attach to window

src/components/
  RecordingView.tsx          ← migrate header to <Toolbar>
  ProjectView.tsx            ← migrate header to <Toolbar>
  Sidebar.tsx                ← replace confirm() with dialog.confirm() (async)

src/App.tsx                  ← subscribe to menuEvents at boot, dispatch handlers,
                               replace confirm() in active-capture guard with
                               dialog.confirm()
```

### Toolbar primitive

Compound component with a Spacer member. Renders a horizontal row.

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

Sizing: 44px (`h-11`) — matches macOS toolbar standard. The bottom separator gives the toolbar visual weight against the content below. Consumers add `data-tauri-drag-region` if the toolbar should also drag the window (it should, in our island layout).

### Menu builder (Rust)

`src-tauri/src/menu.rs` exposes `build_menu(app: &AppHandle)` that returns a `Menu`. The function:

1. Constructs each submenu with a mix of predefined items (`PredefinedMenuItem::cut(app, None)`, etc.) and custom items (`MenuItemBuilder::new("New Project").id("menu:new-project").accelerator("CmdOrCtrl+N").build(app)?`).
2. Returns the assembled top-level `Menu`.

Wired in `lib.rs` `setup()` after the existing `accent::install_observer(...)` line:

```rust
let menu = menu::build_menu(app.handle())?;
app.set_menu(menu)?;
```

Tauri 2 dispatches menu-item activations to `Builder::on_menu_event(|app, event| { … })` set up alongside the menu. The handler matches on `event.id().0` (the string ID) and emits a Tauri event:

```rust
.on_menu_event(|app, event| {
    if let Some(id) = event.id().0.strip_prefix("menu:") {
        let _ = app.emit(&format!("menu:{id}"), ());
    }
})
```

Predefined items don't have a `"menu:"` prefix and the OS handles them directly; the strip filters those out.

### Frontend menu-event subscription

`src/lib/menuEvents.ts`:

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
    await listen(ev, () => {
      const h = handlers.get(ev);
      if (h) h();
    });
  }
}

export function __resetForTests(): void {
  handlers.clear();
  initialized = false;
}
```

Pattern: register handlers via `setMenuHandler(...)` from `App.tsx`'s `useEffect` (or wherever the handler is defined). Handlers are mutable so that closures over `selection` etc. always read the latest state — `setMenuHandler` overwrites on each App re-render.

`App.tsx` calls `initMenuEvents()` once at boot (alongside `initAppearance()`, etc.) and registers handlers in a `useEffect`:

```ts
useEffect(() => {
  setMenuHandler("menu:new-project", () => {
    /* trigger Sidebar's create-project flow — see plan for plumbing */
  });
  setMenuHandler("menu:import-audio", () => {
    if (selection.kind === "project") {
      /* open file picker for that project */
    } else if (selection.kind === "recording") {
      /* open file picker for the recording's project */
    }
  });
  setMenuHandler("menu:run-pipeline", () => {
    if (selection.kind === "recording") {
      /* trigger run on the selected recording — needs a ref to the run handler */
    }
  });
  setMenuHandler("menu:toggle-sidebar", () => {
    setSidebarVisible((v) => !v);
  });
}, [selection]);
```

`menu:new-project` and `menu:import-audio` handlers need state-lifting from Sidebar to App. The plan covers the lifting concretely.

`menu:toggle-sidebar` needs a new `sidebarVisible` state in App + conditional render of `<Sidebar>`. Toggling collapses to width-0 with a smooth transition.

### Native dialog replacement

The `@tauri-apps/plugin-dialog` package is already installed. Its `confirm()` is async:

```ts
import { confirm as dialogConfirm } from "@tauri-apps/plugin-dialog";

const ok = await dialogConfirm("Delete project foo?", {
  title: "Delete project",
  kind: "warning",
});
if (!ok) return;
```

Replace these call sites:

- `Sidebar.tsx:84` (`deleteProject`): `confirm(...)` → `await dialogConfirm(...)`.
- `Sidebar.tsx:96` (`deleteRecording`): same.
- `App.tsx:50` (`guardedSelect` — "Recording in progress — stop and discard?"): same. Note: `guardedSelect` is currently synchronous (returns `void`); making it async means callers `await` it, which is straightforward at the call site (already inside async event handlers).

The `kind: "warning"` argument renders a yellow warning icon in the dialog. For destructive confirmations, that's HIG-correct.

### Toolbar integration

`RecordingView.tsx`'s current top section (around lines 188–210) renders the recording's title (rename input) plus a "completed | Delete" status row. Wrap it:

```tsx
<Toolbar data-tauri-drag-region>
  <Toolbar.Spacer />
  {/* OR: title-as-Input on the left, status + Delete button on the right */}
  {/* preserve existing rename-input + status text + Delete Button */}
</Toolbar>
```

Same for `ProjectView.tsx`'s top section. The integration is a swap of the wrapping container, not a redesign — keep title placement and Delete button.

## Testing

### Automated

`Toolbar.test.tsx` (3 tests):
1. Renders children inside a row.
2. Applies `border-b` on the bottom edge.
3. `Toolbar.Spacer` renders a `flex-1` div.

`menuEvents.test.ts` (3 tests):
1. `initMenuEvents` subscribes via `listen`.
2. Handler registered via `setMenuHandler` is invoked when the corresponding event fires.
3. `__resetForTests` clears handlers and the initialized flag.

(Rust menu construction has no clean unit-test surface — it depends on a real `AppHandle`. Coverage is via the manual smoke test.)

Total ~6 new tests; suite goes from 50 → 56.

### Manual QA

For the menu bar (the centerpiece):
- Open the app. Click the "Meister" app menu in the macOS menu bar — verify the standard items (About, Preferences disabled, Hide, Quit). Preferences is disabled (greyed out).
- Click File. Items: New Project (⌘N), Import Audio… (⌘⇧I), Close Window (⌘W).
- Press ⌘N from anywhere in the app → new-project flow starts (input appears in sidebar).
- Press ⌘⇧I when a project is selected → file picker opens for that project's staging area.
- Press ⌘⇧I when nothing is selected → no-op (ignores the event).
- Press ⌘R when a recording is selected → pipeline runs.
- Press ⌘R when no recording is selected → no-op.
- Press ⌃⌘S → sidebar slides closed. Press again → slides open.
- Click Edit. Standard cluster — Cut/Copy/Paste/Select All. Try ⌘C in any input or text area: it works.
- Window menu: Minimize / Zoom / Bring All to Front.
- Help menu: empty (placeholder fine).

For dialogs:
- Right-click a project row → Delete. Native Tauri dialog appears (not the browser confirm). Confirms the project is gone after OK; cancels on Cancel.
- Same for recording rows.
- Stop a live recording mid-flight, then try to switch to another project. Native dialog appears asking to stop and discard. OK proceeds; Cancel keeps the recording.

For Toolbar:
- Open `RecordingView` for a recording. The header looks visually like before, just inside a 44px bordered row.
- Same for `ProjectView`.
- The toolbar is draggable (drag the window by its toolbar area).

## Error handling

| Failure | Behavior |
|---|---|
| `app.set_menu` fails at boot | `eprintln!` and continue; app boots without a custom menu (default Tauri menu). Non-fatal. |
| `listen()` for a menu event fails | Log once; that menu item is dead until restart. Non-fatal. |
| Menu event fires before frontend has registered a handler (e.g. user presses ⌘N very quickly at boot) | The handler map's `get()` returns undefined; we silently ignore. The user just retries. |
| `dialog.confirm()` throws | Treat as "Cancel" — the destructive action does not proceed. Log to console. |
| `menu:run-pipeline` fires when `selection.kind !== "recording"` | No-op (no error). Future improvement: dynamic disable. |
| Sidebar visibility toggle fires while `creating` is true (new-project input is open) | Allowed — sidebar just closes; the in-flight name input is lost. Acceptable. |

## Out-of-scope / future

- Notifications (#4b).
- About dialog (#4b).
- Preferences pane (#4c).
- Undo / Redo (#4c).
- Global hotkeys outside the menu bar.
- Dynamic menu-item enable/disable.
- Touch Bar support (legacy hardware; out of scope forever).
- File-menu Export, Show in Finder.
- Recent Projects submenu.
- Platform parity beyond macOS.
