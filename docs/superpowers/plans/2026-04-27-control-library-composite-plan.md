# Control Library Composite Primitives Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Spec:** `docs/superpowers/specs/2026-04-27-control-library-composite-design.md`

**Goal:** Ship `SegmentedControl` and `ContextMenu` primitives in `src/components/ui/`, migrate `OutputPreview`'s Formatted/Transcript tabs to `SegmentedControl`, and add right-click context menus (Rename, Delete) to Sidebar project + recording rows.

**Architecture:** Both primitives are compound components that wrap Radix UI (`@radix-ui/react-toggle-group` for SegmentedControl, `@radix-ui/react-context-menu` for ContextMenu). Tests use Vitest + Testing Library + jest-dom. Sidebar gains an inline rename mode driven by a single piece of local state; the existing `api.updateProject`, `api.deleteProject`, `api.updateRecording`, `api.deleteRecording` calls power the actions.

**Tech Stack:** React 18, TypeScript, Tailwind CSS 3, `@radix-ui/react-toggle-group@^1`, `@radix-ui/react-context-menu@^2`, Vitest 1.6 + jsdom + Testing Library.

---

## File Structure

**Create:**
- `src/components/ui/SegmentedControl.tsx`
- `src/components/ui/ContextMenu.tsx`
- `src/components/ui/__tests__/SegmentedControl.test.tsx`
- `src/components/ui/__tests__/ContextMenu.test.tsx`

**Modify:**
- `package.json` — add `@radix-ui/react-toggle-group@^1`, `@radix-ui/react-context-menu@^2`.
- `src/components/ui/index.ts` — export the two new primitives.
- `src/components/OutputPreview.tsx` — replace tab buttons with `<SegmentedControl>`.
- `src/components/Sidebar.tsx` — wrap project rows + recording rows in `<ContextMenu>`, add inline rename state.

---

## Task 1: Install Radix UI primitives

**Files:**
- Modify: `package.json`

- [ ] **Step 1: Install**

```bash
cd /Users/nexus/projects/meister && npm install @radix-ui/react-toggle-group@^1 @radix-ui/react-context-menu@^2
```

- [ ] **Step 2: Verify existing tests still pass**

```bash
npm run test:run
```

Expected: 42 tests pass.

- [ ] **Step 3: Commit**

```bash
git add package.json package-lock.json
git commit -m "chore: install Radix UI toggle-group + context-menu"
```

---

## Task 2: SegmentedControl (TDD)

**Files:**
- Create: `src/components/ui/SegmentedControl.tsx`
- Create: `src/components/ui/__tests__/SegmentedControl.test.tsx`

- [ ] **Step 1: Write the failing test**

Create `src/components/ui/__tests__/SegmentedControl.test.tsx`:

```tsx
import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SegmentedControl } from "../SegmentedControl";

describe("SegmentedControl", () => {
  it("renders all items", () => {
    render(
      <SegmentedControl value="a" onValueChange={() => {}}>
        <SegmentedControl.Item value="a">Apple</SegmentedControl.Item>
        <SegmentedControl.Item value="b">Banana</SegmentedControl.Item>
      </SegmentedControl>,
    );
    expect(screen.getByRole("radio", { name: "Apple" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Banana" })).toBeInTheDocument();
  });

  it("marks the value-matched item as selected via data-state", () => {
    render(
      <SegmentedControl value="b" onValueChange={() => {}}>
        <SegmentedControl.Item value="a">Apple</SegmentedControl.Item>
        <SegmentedControl.Item value="b">Banana</SegmentedControl.Item>
      </SegmentedControl>,
    );
    expect(
      screen.getByRole("radio", { name: "Banana" }).getAttribute("data-state"),
    ).toBe("on");
    expect(
      screen.getByRole("radio", { name: "Apple" }).getAttribute("data-state"),
    ).toBe("off");
  });

  it("fires onValueChange with the new value when an item is clicked", async () => {
    const onValueChange = vi.fn();
    render(
      <SegmentedControl value="a" onValueChange={onValueChange}>
        <SegmentedControl.Item value="a">Apple</SegmentedControl.Item>
        <SegmentedControl.Item value="b">Banana</SegmentedControl.Item>
      </SegmentedControl>,
    );
    await userEvent.click(screen.getByRole("radio", { name: "Banana" }));
    expect(onValueChange).toHaveBeenCalledWith("b");
  });

  it("applies size-based item height", () => {
    render(
      <SegmentedControl value="a" onValueChange={() => {}} size="lg">
        <SegmentedControl.Item value="a">A</SegmentedControl.Item>
      </SegmentedControl>,
    );
    expect(
      screen.getByRole("radio", { name: "A" }).className,
    ).toMatch(/h-7/);
  });
});
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cd /Users/nexus/projects/meister && npm run test:run -- SegmentedControl
```

Expected: FAIL — module missing.

- [ ] **Step 3: Implement `src/components/ui/SegmentedControl.tsx`**

```tsx
import { createContext, forwardRef, useContext } from "react";
import * as ToggleGroup from "@radix-ui/react-toggle-group";
import { cn } from "../../lib/cn";
import { type Size, focusClasses } from "./_shared";

type RootProps = {
  value: string;
  onValueChange: (value: string) => void;
  size?: Size;
  className?: string;
  children: React.ReactNode;
  "aria-label"?: string;
};

const SizeContext = createContext<Size>("md");

const containerHeight: Record<Size, string> = {
  sm: "h-6",
  md: "h-7",
  lg: "h-8",
};

const itemHeight: Record<Size, string> = {
  sm: "h-5",
  md: "h-6",
  lg: "h-7",
};

const itemPad: Record<Size, string> = {
  sm: "px-2",
  md: "px-3",
  lg: "px-4",
};

const itemFont: Record<Size, string> = {
  sm: "text-callout",
  md: "text-body",
  lg: "text-headline",
};

const Root = forwardRef<HTMLDivElement, RootProps>(function SegmentedControlRoot(
  { value, onValueChange, size = "md", className, children, "aria-label": ariaLabel },
  ref,
) {
  return (
    <SizeContext.Provider value={size}>
      <ToggleGroup.Root
        ref={ref}
        type="single"
        value={value}
        onValueChange={(v) => {
          // ToggleGroup with type="single" allows empty string when user
          // toggles the active item off. We don't want that — keep the
          // current selection if Radix tries to clear it.
          if (v) onValueChange(v);
        }}
        aria-label={ariaLabel}
        className={cn(
          "inline-flex items-center bg-fill-tertiary rounded-md p-0.5 gap-0.5",
          containerHeight[size],
          className,
        )}
      >
        {children}
      </ToggleGroup.Root>
    </SizeContext.Provider>
  );
});

type ItemProps = {
  value: string;
  children: React.ReactNode;
  className?: string;
  disabled?: boolean;
};

const Item = forwardRef<HTMLButtonElement, ItemProps>(function SegmentedControlItem(
  { value, children, className, disabled },
  ref,
) {
  const size = useContext(SizeContext);
  return (
    <ToggleGroup.Item
      ref={ref}
      value={value}
      disabled={disabled}
      className={cn(
        "inline-flex items-center justify-center rounded-sm font-medium transition-colors",
        "text-label-secondary hover:text-label-primary",
        "data-[state=on]:bg-control-background data-[state=on]:text-label-primary data-[state=on]:shadow-[0_0_0_0.5px_rgb(0_0_0/0.04)]",
        "disabled:opacity-50 disabled:cursor-not-allowed",
        itemHeight[size],
        itemPad[size],
        itemFont[size],
        focusClasses,
        className,
      )}
    >
      {children}
    </ToggleGroup.Item>
  );
});

export const SegmentedControl = Object.assign(Root, { Item });
```

- [ ] **Step 4: Run tests to confirm they pass**

```bash
cd /Users/nexus/projects/meister && npm run test:run -- SegmentedControl
```

Expected: 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/components/ui/SegmentedControl.tsx src/components/ui/__tests__/SegmentedControl.test.tsx
git commit -m "feat(ui): add SegmentedControl primitive (Radix toggle-group based)"
```

---

## Task 3: ContextMenu (TDD)

**Files:**
- Create: `src/components/ui/ContextMenu.tsx`
- Create: `src/components/ui/__tests__/ContextMenu.test.tsx`

- [ ] **Step 1: Write the failing test**

Create `src/components/ui/__tests__/ContextMenu.test.tsx`:

```tsx
import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ContextMenu } from "../ContextMenu";

function Harness({
  onRename,
  onDelete,
}: {
  onRename?: () => void;
  onDelete?: () => void;
}) {
  return (
    <ContextMenu>
      <ContextMenu.Trigger>
        <div data-testid="trigger" style={{ width: 100, height: 100 }}>
          row
        </div>
      </ContextMenu.Trigger>
      <ContextMenu.Content>
        <ContextMenu.Item onSelect={onRename}>Rename</ContextMenu.Item>
        <ContextMenu.Separator />
        <ContextMenu.Item variant="destructive" onSelect={onDelete}>
          Delete
        </ContextMenu.Item>
      </ContextMenu.Content>
    </ContextMenu>
  );
}

describe("ContextMenu", () => {
  it("does not render menu items until the trigger is right-clicked", () => {
    render(<Harness />);
    expect(screen.queryByText("Rename")).not.toBeInTheDocument();
  });

  it("opens the menu on right-click of the trigger", async () => {
    render(<Harness />);
    await userEvent.pointer({
      keys: "[MouseRight]",
      target: screen.getByTestId("trigger"),
    });
    expect(await screen.findByText("Rename")).toBeInTheDocument();
    expect(screen.getByText("Delete")).toBeInTheDocument();
  });

  it("fires onSelect when an item is clicked", async () => {
    const onRename = vi.fn();
    render(<Harness onRename={onRename} />);
    await userEvent.pointer({
      keys: "[MouseRight]",
      target: screen.getByTestId("trigger"),
    });
    await userEvent.click(await screen.findByText("Rename"));
    expect(onRename).toHaveBeenCalled();
  });

  it("applies destructive styling when variant='destructive'", async () => {
    render(<Harness />);
    await userEvent.pointer({
      keys: "[MouseRight]",
      target: screen.getByTestId("trigger"),
    });
    const del = await screen.findByText("Delete");
    expect(del.className).toMatch(/text-status-error/);
  });
});
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cd /Users/nexus/projects/meister && npm run test:run -- ContextMenu
```

Expected: FAIL — module missing.

- [ ] **Step 3: Implement `src/components/ui/ContextMenu.tsx`**

```tsx
import { forwardRef } from "react";
import * as Rx from "@radix-ui/react-context-menu";
import { cn } from "../../lib/cn";

type ContentProps = React.ComponentPropsWithoutRef<typeof Rx.Content>;

const Content = forwardRef<HTMLDivElement, ContentProps>(function ContextMenuContent(
  { className, children, ...rest },
  ref,
) {
  return (
    <Rx.Portal>
      <Rx.Content
        ref={ref}
        collisionPadding={8}
        className={cn(
          "z-50 min-w-[10rem] overflow-hidden rounded-md border border-separator bg-control-background p-1 shadow-lg backdrop-blur-md",
          "data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:zoom-in-95",
          "data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-95",
          className,
        )}
        {...rest}
      >
        {children}
      </Rx.Content>
    </Rx.Portal>
  );
});

type ItemProps = React.ComponentPropsWithoutRef<typeof Rx.Item> & {
  variant?: "default" | "destructive";
};

const Item = forwardRef<HTMLDivElement, ItemProps>(function ContextMenuItem(
  { className, variant = "default", ...rest },
  ref,
) {
  return (
    <Rx.Item
      ref={ref}
      className={cn(
        "flex h-7 select-none items-center rounded-sm px-3 text-body outline-none cursor-default",
        "data-[disabled]:opacity-50 data-[disabled]:pointer-events-none",
        variant === "default" &&
          "text-label-primary data-[highlighted]:bg-accent data-[highlighted]:text-accent-label",
        variant === "destructive" &&
          "text-status-error data-[highlighted]:bg-status-error data-[highlighted]:text-white",
        className,
      )}
      {...rest}
    />
  );
});

type SeparatorProps = React.ComponentPropsWithoutRef<typeof Rx.Separator>;

const Separator = forwardRef<HTMLDivElement, SeparatorProps>(function ContextMenuSeparator(
  { className, ...rest },
  ref,
) {
  return (
    <Rx.Separator
      ref={ref}
      className={cn("my-1 h-px bg-separator", className)}
      {...rest}
    />
  );
});

export const ContextMenu = Object.assign(Rx.Root, {
  Trigger: Rx.Trigger,
  Content,
  Item,
  Separator,
});
```

Note: the `data-[state=open]:animate-in` etc. classes require `tailwindcss-animate` to be installed and configured. If those animations don't render (no install), the menu still opens — just without a fade/scale effect. We don't install the plugin in this slice; the data-state classes degrade gracefully. (If the user wants the polished animation, that's a one-line plugin install in a follow-up.)

- [ ] **Step 4: Run tests to confirm they pass**

```bash
cd /Users/nexus/projects/meister && npm run test:run -- ContextMenu
```

Expected: 4 tests pass.

If the third test ("fires onSelect when an item is clicked") flakes due to Radix's pointer-event handling in jsdom, fall back to keyboard activation: replace `await userEvent.click(...)` with `await userEvent.keyboard("{ArrowDown}{Enter}")` after opening the menu.

- [ ] **Step 5: Commit**

```bash
git add src/components/ui/ContextMenu.tsx src/components/ui/__tests__/ContextMenu.test.tsx
git commit -m "feat(ui): add ContextMenu primitive (Radix context-menu based)"
```

---

## Task 4: Update barrel export

**Files:**
- Modify: `src/components/ui/index.ts`

- [ ] **Step 1: Append exports**

```ts
export { Button } from "./Button";
export { Input } from "./Input";
export { Textarea } from "./Textarea";
export { Select } from "./Select";
export { Checkbox } from "./Checkbox";
export { Toggle } from "./Toggle";
export { SegmentedControl } from "./SegmentedControl";
export { ContextMenu } from "./ContextMenu";
```

- [ ] **Step 2: Build + run full suite**

```bash
cd /Users/nexus/projects/meister && npm run build && npm run test:run
```

Expected: build clean. 50 tests pass (42 prior + 4 SegmentedControl + 4 ContextMenu).

- [ ] **Step 3: Commit**

```bash
git add src/components/ui/index.ts
git commit -m "feat(ui): export SegmentedControl + ContextMenu from ui barrel"
```

---

## Task 5: Migrate `OutputPreview` tabs to `SegmentedControl`

**Files:**
- Modify: `src/components/OutputPreview.tsx`

- [ ] **Step 1: Replace the tab markup**

Open `src/components/OutputPreview.tsx`. Current state (lines 17-34) has two raw `<button>` elements. Replace the entire `<div className="flex gap-2 text-footnote">…</div>` block with:

```tsx
<SegmentedControl
  size="sm"
  value={tab}
  onValueChange={(v) => setTab(v as "formatted" | "transcript")}
  aria-label="Output view"
>
  <SegmentedControl.Item value="formatted">Formatted</SegmentedControl.Item>
  <SegmentedControl.Item value="transcript">Transcript</SegmentedControl.Item>
</SegmentedControl>
```

Add to the imports at the top of the file:

```tsx
import { SegmentedControl } from "./ui";
```

The local `useState` for `tab` stays unchanged.

- [ ] **Step 2: Build + run full suite**

```bash
cd /Users/nexus/projects/meister && npm run build && npm run test:run
```

Expected: build clean, 50 tests pass.

- [ ] **Step 3: Commit**

```bash
git add src/components/OutputPreview.tsx
git commit -m "refactor(ui): migrate OutputPreview tabs to SegmentedControl"
```

---

## Task 6: Sidebar — project-row context menu + inline rename

**Files:**
- Modify: `src/components/Sidebar.tsx`

This task adds two pieces:
1. A `<ContextMenu>` wrapping each project row's `<button>` (Rename, Delete).
2. Inline rename state — when Rename is invoked, the row's name display becomes an `<Input>` that saves on Enter / blur, cancels on Escape.

- [ ] **Step 1: Add the rename state and helpers near the top of the component**

Find the existing `const [creating, setCreating] = useState(false);` block in `src/components/Sidebar.tsx`. After it, add:

```tsx
type RenameTarget =
  | { kind: "project"; id: string; value: string }
  | { kind: "recording"; id: string; value: string }
  | null;

const [renaming, setRenaming] = useState<RenameTarget>(null);

const beginRenameProject = (p: Project) => {
  setRenaming({ kind: "project", id: p.id, value: p.name });
};

const beginRenameRecording = (r: Recording) => {
  setRenaming({ kind: "recording", id: r.id, value: r.name });
};

const cancelRename = () => setRenaming(null);

const commitRename = async () => {
  if (!renaming) return;
  const trimmed = renaming.value.trim();
  if (!trimmed) {
    setRenaming(null);
    return;
  }
  if (renaming.kind === "project") {
    const project = projects.find((p) => p.id === renaming.id);
    if (project && trimmed !== project.name) {
      await api.updateProject({ ...project, name: trimmed });
      onProjectsChanged();
    }
  } else {
    const all = Object.values(recordingsByProject).flat();
    const rec = all.find((r) => r.id === renaming.id);
    if (rec && trimmed !== rec.name) {
      await api.updateRecording({ ...rec, name: trimmed });
      // Refresh the project that owns this recording.
      onProjectsChanged();
    }
  }
  setRenaming(null);
};

const deleteProject = async (p: Project) => {
  if (!confirm(`Delete project "${p.name}" and all its recordings?`)) return;
  await api.deleteProject(p.id);
  if (selection.kind === "project" && selection.projectId === p.id) {
    onSelect({ kind: "none" });
  }
  if (
    selection.kind === "recording" &&
    selection.projectId === p.id
  ) {
    onSelect({ kind: "none" });
  }
  onProjectsChanged();
};

const deleteRecording = async (r: Recording) => {
  if (!confirm(`Delete recording "${r.name}"?`)) return;
  await api.deleteRecording(r.id);
  if (selection.kind === "recording" && selection.recordingId === r.id) {
    onSelect({ kind: "none" });
  }
  onProjectsChanged();
};
```

These helpers depend on the `Project`, `Recording` types already imported, plus `api`, `selection`, `onSelect`, `onProjectsChanged`, `projects`, `recordingsByProject` already in scope.

- [ ] **Step 2: Add `ContextMenu` import**

In the import block at the top of the file, change:

```tsx
import { Button, Input } from "./ui";
```

to:

```tsx
import { Button, Input, ContextMenu } from "./ui";
```

- [ ] **Step 3: Wrap project rows with `<ContextMenu>`**

Find the project-row markup. Currently each project row is rendered as something like:

```tsx
<button
  type="button"
  onClick={() => onSelect({ kind: "project", projectId: p.id })}
  className={...}
>
  {/* row content: name, status dot, expand caret */}
</button>
```

Wrap it as follows:

```tsx
<ContextMenu>
  <ContextMenu.Trigger asChild>
    <button
      type="button"
      onClick={() => onSelect({ kind: "project", projectId: p.id })}
      className={...}
    >
      {/* row content unchanged when not renaming */}
      {renaming?.kind === "project" && renaming.id === p.id ? (
        <Input
          autoFocus
          size="sm"
          value={renaming.value}
          onChange={(e) =>
            setRenaming({ kind: "project", id: p.id, value: e.target.value })
          }
          onBlur={commitRename}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              commitRename();
            } else if (e.key === "Escape") {
              e.preventDefault();
              cancelRename();
            }
          }}
          onClick={(e) => e.stopPropagation()}
          className="flex-1"
        />
      ) : (
        /* existing project-name span / inline children stay here */
        <span className="flex-1 truncate">{p.name}</span>
      )}
    </button>
  </ContextMenu.Trigger>
  <ContextMenu.Content>
    <ContextMenu.Item onSelect={() => beginRenameProject(p)}>
      Rename
    </ContextMenu.Item>
    <ContextMenu.Separator />
    <ContextMenu.Item
      variant="destructive"
      onSelect={() => deleteProject(p)}
    >
      Delete
    </ContextMenu.Item>
  </ContextMenu.Content>
</ContextMenu>
```

Note: the existing project row has more than just the name — it also has a status dot, expand caret, and possibly an "+ add recording" icon. Keep that surrounding structure; replace only the project-name display with the conditional `Input`-vs-`span` block. The `flex-1 truncate` on the name is the typical existing class — preserve whatever it was before.

If the `<button>` element renders any descendants that include their own `<button>` (e.g. an "add recording" button stop-propagated inside), Radix's `Trigger asChild` still works — it just attaches the right-click handler to the outer button. Inner buttons remain functional.

- [ ] **Step 4: Build + smoke**

```bash
cd /Users/nexus/projects/meister && npm run build
```

Expected: builds.

```bash
npm run tauri dev
```

Wait for the app to launch. Right-click a project row — verify the context menu appears with Rename and Delete. Click Rename → row name becomes an `<Input>`. Type a new name, press Enter → it saves. Right-click again, click Delete → confirm prompt, then the project is gone. Kill dev when satisfied.

(Skip the dev test if you've already verified `tauri dev` works in this worktree; the build alone is enough for plan flow.)

- [ ] **Step 5: Run tests**

```bash
npm run test:run
```

Expected: 50 tests pass (no new tests in this task — Sidebar's behavior isn't unit-tested).

- [ ] **Step 6: Commit**

```bash
git add src/components/Sidebar.tsx
git commit -m "feat(sidebar): right-click project rows for Rename + Delete"
```

---

## Task 7: Sidebar — recording-row context menu + inline rename

**Files:**
- Modify: `src/components/Sidebar.tsx`

The state, helpers, and `ContextMenu` import added in Task 6 are reused here. Only the recording-row markup needs updating.

- [ ] **Step 1: Wrap recording rows with `<ContextMenu>`**

Find the recording-row markup. Currently each row looks roughly like:

```tsx
<button
  type="button"
  onClick={() => onSelect({ kind: "recording", projectId: p.id, recordingId: r.id })}
  className={...}
>
  {/* status dot + name */}
</button>
```

Wrap it:

```tsx
<ContextMenu>
  <ContextMenu.Trigger asChild>
    <button
      type="button"
      onClick={() =>
        onSelect({
          kind: "recording",
          projectId: p.id,
          recordingId: r.id,
        })
      }
      className={...}
    >
      {/* status dot stays */}
      {renaming?.kind === "recording" && renaming.id === r.id ? (
        <Input
          autoFocus
          size="sm"
          value={renaming.value}
          onChange={(e) =>
            setRenaming({ kind: "recording", id: r.id, value: e.target.value })
          }
          onBlur={commitRename}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              commitRename();
            } else if (e.key === "Escape") {
              e.preventDefault();
              cancelRename();
            }
          }}
          onClick={(e) => e.stopPropagation()}
          className="flex-1"
        />
      ) : (
        /* existing recording-name span stays here */
        <span className="flex-1 truncate">{r.name}</span>
      )}
    </button>
  </ContextMenu.Trigger>
  <ContextMenu.Content>
    <ContextMenu.Item onSelect={() => beginRenameRecording(r)}>
      Rename
    </ContextMenu.Item>
    <ContextMenu.Separator />
    <ContextMenu.Item
      variant="destructive"
      onSelect={() => deleteRecording(r)}
    >
      Delete
    </ContextMenu.Item>
  </ContextMenu.Content>
</ContextMenu>
```

Same caveats as project rows: preserve the surrounding structure (status dot, indentation, etc.) and only swap the name display.

- [ ] **Step 2: Build**

```bash
cd /Users/nexus/projects/meister && npm run build
```

- [ ] **Step 3: Run tests**

```bash
npm run test:run
```

Expected: 50 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/components/Sidebar.tsx
git commit -m "feat(sidebar): right-click recording rows for Rename + Delete"
```

---

## Task 8: Manual QA + tag

**Files:** none (verification only).

- [ ] **Step 1: Functional QA — SegmentedControl**

```bash
cd /Users/nexus/projects/meister && npm run tauri dev
```

In the running app, open a recording with both formatted output and a transcript. The segmented control replaces the old Formatted/Transcript pills:

- Click each segment. Verify the content area updates.
- Tab to the segmented control. Use Left / Right arrow keys to move selection. Verify focus ring appears on `:focus-visible` only (not on click).
- The selected segment should have a slightly raised, control-background fill; the unselected segment should be a muted secondary-label color.

- [ ] **Step 2: Functional QA — ContextMenu (project rows)**

- Right-click a project row. The menu opens with **Rename** and **Delete** (red text).
- Click outside the menu → it closes.
- Right-click again → click **Rename**. The row name becomes an editable input.
  - Type a new name, press Enter → name persists.
  - Right-click → Rename again. Press Escape → revert.
  - Right-click → Rename. Click outside the input → it saves on blur.
- Right-click → **Delete**. A confirm dialog appears (`confirm()`). Click OK → the project disappears.

- [ ] **Step 3: Functional QA — ContextMenu (recording rows)**

- Expand a project to show its recordings.
- Right-click a recording row. Menu opens with **Rename** / **Delete**.
- Test Rename + Delete the same way as for projects.
- If the deleted recording was selected, the main view should clear back to the empty state.

- [ ] **Step 4: Keyboard QA — ContextMenu**

- Right-click a row to open the menu.
- Press Down arrow → first item ("Rename") becomes highlighted.
- Press Down again → "Delete" becomes highlighted (red bg, white text).
- Press Up → back to "Rename".
- Press Enter → action fires.
- Right-click again, press Esc → menu closes without firing.

- [ ] **Step 5: Cross-app comparison**

- Open Finder. Right-click a file. Compare to Meister's context menu — proportions, animation, hover affordance should feel comparable.
- Open Music. Compare a Music segmented control (e.g. on the Albums screen) to Meister's. Should feel comparable.

- [ ] **Step 6: Tag**

```bash
cd /Users/nexus/projects/meister && git tag native-feel/03b-controls-composite-complete
```

Sub-project #3b done. Next: sub-project #3c (`Toolbar` primitive, possibly merged with #4 menu bar) — separate spec and plan.
