# Control Library Composite Primitives (Sub-project #3b)

Second slice of sub-project #3 ("control library") in the native-macOS overhaul. This slice adds two composite primitives — `SegmentedControl` and `ContextMenu` — both backed by Radix UI for keyboard / ARIA correctness, and integrates them into the two known call sites: `OutputPreview` tabs (formatted vs transcript) and Sidebar rows (right-click → rename/delete). Sub-project #3a delivered the foundation primitives (`Button`, `Input`, `Textarea`, `Select`, `Checkbox`, `Toggle`); this slice builds on the same `_shared.ts` patterns. Sub-project #3c (`Toolbar`, possibly merged with #4 menu bar) follows.

## Context

After #3a, every text input, button, and standalone control in Meister uses our `src/components/ui/` primitives. Two visible gaps remain:

- `OutputPreview.tsx` has a Formatted / Transcript tab pair built from raw `<button>` elements. Functionally a single-select toggle group; visually should be a macOS-style segmented control. The #3a code review explicitly flagged this as the SegmentedControl call site.
- Sidebar project rows and recording rows have no right-click affordance. macOS users expect right-click context menus on list items (Rename, Delete, etc.). Today the only way to delete a project or recording is via the explicit Delete button at the top of `ProjectView` / `RecordingView`. Right-clicking does nothing.

Other potential composites (`RadioGroup`, `Tooltip`, `Slider`) have no current call sites and are explicitly deferred to avoid bit-rot.

## Goals

- Ship two primitives in `src/components/ui/`: `SegmentedControl` and `ContextMenu`.
- `SegmentedControl` follows the compound API used by Radix and our existing primitives: `<SegmentedControl value="x" onValueChange={...}><SegmentedControl.Item value="x">Label</SegmentedControl.Item></SegmentedControl>`. Three sizes (`sm`/`md`/`lg`) reusing `_shared.ts`.
- `ContextMenu` follows Radix's compound API: `<ContextMenu><ContextMenu.Trigger>{rowContent}</ContextMenu.Trigger><ContextMenu.Content><ContextMenu.Item onSelect={...}>Rename</ContextMenu.Item><ContextMenu.Separator /><ContextMenu.Item onSelect={...} variant="destructive">Delete</ContextMenu.Item></ContextMenu.Content></ContextMenu>`. Item supports a `variant="destructive"` for the Delete row; otherwise default.
- Both primitives back onto Radix UI: `@radix-ui/react-toggle-group` for SegmentedControl, `@radix-ui/react-context-menu` for ContextMenu.
- Migrate `OutputPreview.tsx` to use `SegmentedControl` for the Formatted / Transcript pair.
- Wire `ContextMenu` onto Sidebar project rows (Rename / Delete) and recording rows (Rename / Delete). Hooks for Rename invoke the existing inline-rename callbacks; Delete invokes the existing delete callbacks.
- Add Vitest behavioral tests for each primitive's keyboard contract.

## Non-goals

- No `RadioGroup`, `Tooltip`, or `Slider` (no current call sites; deferred).
- No `ContextMenu` submenus (`Sub`, `SubTrigger`, `SubContent`) until a real consumer needs them. Keep the export surface tight.
- No "Show in Finder" / "Open Outputs" actions on the recording-row context menu. They'd require either expanded paths in the row props or a new Rust command. Out of scope; revisit when icons / commands land in #4 / #5.
- No keyboard shortcut hints (e.g. "⌘⌫" next to Delete). Sub-project #4 (menu bar + shortcuts) owns the shortcut story; only at that point do hints become real.
- No design changes to anything other than the two integration sites above.
- No "checked" or "highlighted" item state (Radix supports this via `CheckboxItem`/`RadioItem`/`Checked`); we don't have a use case.
- No new visual-regression tooling.

## Architecture

### File layout

```
src/components/ui/
  SegmentedControl.tsx
  ContextMenu.tsx
  index.ts                  ← updated to add the two new exports
  __tests__/
    SegmentedControl.test.tsx
    ContextMenu.test.tsx
```

`_shared.ts` (`Size`, `sizeClasses`, `focusClasses`) is reused as-is. No new shared helpers needed.

### `SegmentedControl`

Compound component built on `@radix-ui/react-toggle-group` with `type="single"`. Single component file exporting both the root and the `.Item` member.

```tsx
import * as ToggleGroup from "@radix-ui/react-toggle-group";

type RootProps = {
  value: string;
  onValueChange: (value: string) => void;
  size?: Size;
  className?: string;
  children: React.ReactNode;
};

export const SegmentedControl = forwardRef<HTMLDivElement, RootProps>(...);
SegmentedControl.Item = forwardRef<HTMLButtonElement, ItemProps>(...);
```

Internally: `ToggleGroup.Root` rendered with `type="single"` and `value`/`onValueChange` props passed through. The size prop is forwarded to children via `React.Context`.

Visual style:
- Root: `inline-flex items-center bg-fill-tertiary rounded-md p-0.5 gap-0.5`
- Item: rounded-sm, padding/font per size, transparent background by default. Selected: `data-[state=on]:bg-control-background data-[state=on]:text-label-primary data-[state=on]:shadow-[0_0_0_0.5px_rgb(0_0_0/0.04)]` (subtle elevation matching macOS Sequoia segmented controls). Unselected: `text-label-secondary hover:text-label-primary`.

Sizes (matching `_shared.ts` heights, with the segments slightly smaller than freestanding buttons because they sit inside a padded container):

| Size | Item height | Item padding | Item font |
|---|---|---|---|
| `sm` | 20px (`h-5`) | `px-2` | `text-callout` |
| `md` | 24px (`h-6`) | `px-3` | `text-body` |
| `lg` | 28px (`h-7`) | `px-4` | `text-headline` |

Container heights: 24px / 28px / 32px (for sm/md/lg, including the 0.5px padding on each side of the items).

Focus rings via `focusClasses` from `_shared.ts`, applied to each `Item`.

### `ContextMenu`

Compound component built on `@radix-ui/react-context-menu`. Single component file exporting `ContextMenu` (root) plus `Trigger`, `Content`, `Item`, `Separator` as members.

```tsx
import * as Rx from "@radix-ui/react-context-menu";

export const ContextMenu = (props: Rx.ContextMenuProps) => <Rx.Root {...props} />;
ContextMenu.Trigger = Rx.Trigger;  // forwarded as-is, no styling
ContextMenu.Content = ...           // styled wrapper around Rx.Content (within Portal)
ContextMenu.Item = ...              // styled, supports variant="destructive"
ContextMenu.Separator = ...         // styled
```

Visual style:
- Content: `bg-control-background border border-separator rounded-md shadow-lg backdrop-blur-md p-1 min-w-[10rem] z-50` plus a fade/scale animation on open via Tailwind's data-state arbitrary values: `data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:zoom-in-95`. (The `tailwindcss-animate` plugin would simplify this; if not present, use raw CSS via `@keyframes` in `index.css` — flag during plan-write.)
- Item: `flex items-center px-3 h-7 text-body rounded-sm cursor-default outline-none focus:bg-accent focus:text-accent-label data-[disabled]:opacity-50 data-[disabled]:pointer-events-none`. `variant="destructive"`: `text-status-error focus:bg-status-error focus:text-white`.
- Separator: `h-px my-1 bg-separator`.

`Item` `onSelect` is the standard Radix prop — fired when the user clicks or presses Enter on a focused item. We surface it as-is.

`Item` size is fixed (28px / `h-7`, body type). No size prop — context-menu items are HIG-spec'd to a single height.

### Integration: `OutputPreview` → `SegmentedControl`

The current Formatted / Transcript tab pair lives in `src/components/OutputPreview.tsx` around lines 17–37. It's two `<button>` elements managed by local state.

Replace with:

```tsx
<SegmentedControl
  value={tab}
  onValueChange={setTab as (v: string) => void}
  size="sm"
>
  <SegmentedControl.Item value="formatted">Formatted</SegmentedControl.Item>
  <SegmentedControl.Item value="transcript">Transcript</SegmentedControl.Item>
</SegmentedControl>
```

`tab` is a local state with type `"formatted" | "transcript"`. `setTab` accepts that union; `onValueChange` provides `string`, so the cast widens — acceptable since `SegmentedControl.Item` won't dispatch a value outside its declared options.

Size `sm` matches the previous tab-pill scale (the old buttons used `text-footnote`, which was tighter than HIG Regular).

### Integration: Sidebar `ContextMenu`

Sidebar rows are rendered inside `src/components/Sidebar.tsx`. The current row markup is a `<button>` element (project rows around line 130, recording rows around line 158). Wrap each row's content in a `<ContextMenu>`:

```tsx
<ContextMenu>
  <ContextMenu.Trigger asChild>
    <button onClick={...} className="..." >
      {rowContent}
    </button>
  </ContextMenu.Trigger>
  <ContextMenu.Content>
    <ContextMenu.Item onSelect={() => onRename(id)}>Rename</ContextMenu.Item>
    <ContextMenu.Separator />
    <ContextMenu.Item variant="destructive" onSelect={() => onDelete(id)}>
      Delete
    </ContextMenu.Item>
  </ContextMenu.Content>
</ContextMenu>
```

The `asChild` prop on `Trigger` is a Radix convention — the trigger forwards its event handlers onto the existing `<button>` instead of rendering its own wrapper. This preserves the existing button's `onClick` (left-click selects the row) while adding right-click → context menu.

`onRename` and `onDelete` callbacks need to be plumbed in if not already present:
- Project rows: `onRename(projectId)` should trigger the existing rename flow (currently inline rename via clicking the project header in `ProjectView`). For the sidebar context menu to invoke that, `Sidebar` needs a new `onRenameProject(id)` prop, supplied by `App.tsx`. The handler can either dispatch directly to the project header's edit mode (requires lifting state) or simply select the project and set a new "begin rename on selection" flag.
  - Simpler approach: `onRenameProject(id)` selects the project AND sets a query-param-style state that `ProjectView` reads on mount to enter edit mode. Specifics covered in the plan's integration task.
- Recording rows: same pattern with `onRenameRecording(id)`.
- `onDeleteProject` / `onDeleteRecording`: existing delete flows in `ProjectView` / `RecordingView` (via the Delete button). Lift these to props on `Sidebar` so the context menu can invoke them.

If lifting state proves more invasive than the plan anticipates, fallback: the context menu's Rename action selects the row (existing behavior) and shows a small toast / inline cue saying "click the title to rename." Less elegant; documented as a fallback in the plan.

## Testing

### Automated

`SegmentedControl.test.tsx` (4 tests):
1. Renders all items.
2. The `value` prop drives which item has `data-state="on"`.
3. Clicking an unselected item fires `onValueChange` with that item's value.
4. Arrow-key navigation across items focuses each in turn (Radix behavior).

`ContextMenu.test.tsx` (4 tests):
1. Renders trigger; menu content not in DOM until activated.
2. Right-clicking the trigger opens the menu (assert `Content` becomes visible).
3. `onSelect` on an `Item` fires when the item is clicked / Enter-pressed.
4. `variant="destructive"` applies status-error styling to the item.

Total ~8 new tests; running suite goes from 42 → 50.

### Manual QA

For `SegmentedControl`:
- Click each segment in `OutputPreview`. Verify visual state updates.
- Tab to it. Use arrow keys (←/→) to move selection. Verify focus ring appears on `:focus-visible` only.
- Resize the window. Segments should keep their proportions; the control shouldn't grow.

For `ContextMenu`:
- Right-click a project row in the sidebar. Menu opens with Rename and Delete.
- Right-click a recording row. Same options.
- Click outside while open: menu closes.
- Press Escape while open: menu closes.
- Up / Down arrow navigates items. Enter activates the focused item.
- Right-click during an existing context menu: replaces the menu (Radix default).
- Verify the Delete item is rendered with destructive styling (red text), and on hover/focus has a red background with white text.
- Trigger Rename → verify the project's / recording's inline rename flow begins (per the plumbing chosen during plan-write).
- Trigger Delete → verify the existing delete confirmation / removal flow runs.

## Error handling

| Failure | Behavior |
|---|---|
| `onSelect` callback throws | React error boundary handles it. No primitive-level catch. |
| `onValueChange` callback throws | Same. |
| Radix primitive renders in unexpected env (e.g. SSR) | Out of scope — Tauri webviews are client-only. |
| User right-clicks rapidly to spam-open the menu | Radix handles cleanup on each new context. No memory growth. |
| Rename callback unavailable (e.g. row has no rename support) | Hide the Rename item (don't render it) rather than a disabled state. The plan covers this if it comes up. |
| Delete callback unavailable | Same — hide the item. |

## Out-of-scope / future

- `RadioGroup`, `Tooltip`, `Slider` primitives.
- `ContextMenu` submenus, checkbox items, radio items.
- Keyboard shortcut hints in menu items (sub-project #4).
- "Show in Finder", "Open Outputs", or other path-specific recording actions.
- Long-press / haptic affordance hints.
- Visual-regression test tooling.
- Dragging segments to reorder (different control entirely).
