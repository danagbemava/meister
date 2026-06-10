# Liquid Glass UI Overhaul — Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring Meister's shell and controls to the Tahoe Liquid Glass look — glass sidebar island, scroll-edge toolbar, concentric radii, AppKit-feel control states — per the approved spec `docs/superpowers/specs/2026-06-10-liquid-glass-overhaul-design.md`.

**Architecture:** Hybrid glass — the existing native `NSGlassEffectView` keeps backing the sidebar; everything else is CSS. New glass tokens + two utility classes (`.glass-overlay`, `.glass-edge`) drive all CSS glass. A concentric radius scale (`window 12 / card 10 / control 7 / field 5`) replaces ad-hoc `rounded-*`. A `useScrollEdge` hook + sticky `Toolbar` implements the scroll-edge effect. Content cards stay calm fills (Direction A) — glass is reserved for chrome and floating overlays.

**Tech Stack:** React 18, Tailwind 3, Radix primitives, vitest + @testing-library/react (jsdom). No Rust changes.

**Verification baseline:** `npm test -- --run` green before starting; must be green after every task.

---

### Task 1: Glass tokens + concentric radius scale

**Files:**
- Modify: `src/styles/tokens.css`
- Modify: `tailwind.config.js`

- [ ] **Step 1: Add glass tokens to `tokens.css`**

Append inside the `:root, [data-appearance="light"]` block (after the Status block):

```css
  /* Liquid Glass materials (Phase 1 overhaul) */
  --glass-overlay-bg:        rgba(255 255 255 / 0.55);
  --glass-overlay-border:    rgba(255 255 255 / 0.35);
  --glass-overlay-highlight: rgba(255 255 255 / 0.50);
  --glass-edge-bg:           rgba(242 242 247 / 0.60);
  --shadow-overlay: 0 10px 38px rgba(0 0 0 / 0.22), 0 1px 3px rgba(0 0 0 / 0.12);
```

Append inside the `[data-appearance="dark"]` block (after the Status block):

```css
  /* Liquid Glass materials (Phase 1 overhaul) */
  --glass-overlay-bg:        rgba(40 40 44 / 0.55);
  --glass-overlay-border:    rgba(255 255 255 / 0.14);
  --glass-overlay-highlight: rgba(255 255 255 / 0.08);
  --glass-edge-bg:           rgba(30 30 30 / 0.55);
  --shadow-overlay: 0 10px 38px rgba(0 0 0 / 0.50), 0 1px 3px rgba(0 0 0 / 0.30);
```

Append inside the `[data-reduce-transparency="true"]` block:

```css
  /* Glass collapses to opaque control background */
  --glass-overlay-bg:        rgb(var(--color-control-background-rgb));
  --glass-overlay-border:    var(--color-separator);
  --glass-overlay-highlight: transparent;
  --glass-edge-bg:           rgb(var(--color-control-background-rgb));
```

Append at the end of `tokens.css` (after the `.sidebar-folder-*` rules):

```css
/* Liquid Glass utility materials. backdrop-filter is dropped (and the bg is
 * already opaque via token overrides) under reduce-transparency. */
.glass-overlay {
  background: var(--glass-overlay-bg);
  border: 0.5px solid var(--glass-overlay-border);
  box-shadow: inset 0 1px 0 0 var(--glass-overlay-highlight), var(--shadow-overlay);
  -webkit-backdrop-filter: blur(28px) saturate(170%);
  backdrop-filter: blur(28px) saturate(170%);
}

.glass-edge {
  background: var(--glass-edge-bg);
  -webkit-backdrop-filter: blur(18px) saturate(160%);
  backdrop-filter: blur(18px) saturate(160%);
}

[data-reduce-transparency="true"] .glass-overlay,
[data-reduce-transparency="true"] .glass-edge {
  -webkit-backdrop-filter: none;
  backdrop-filter: none;
}
```

- [ ] **Step 2: Add radius scale + overlay shadow to `tailwind.config.js`**

In `theme.extend`, add as new keys (sibling of `colors`):

```js
      borderRadius: {
        window:  "12px",
        card:    "10px",
        control: "7px",
        field:   "5px",
      },
      boxShadow: {
        overlay: "var(--shadow-overlay)",
        island:  "0 4px 18px rgba(0 0 0 / 0.10), 0 0.5px 1.5px rgba(0 0 0 / 0.08)",
      },
```

- [ ] **Step 3: Verify build + tests**

Run: `npm test -- --run`
Expected: all pass (token-only change, nothing consumes the new classes yet).

- [ ] **Step 4: Commit**

```bash
git add src/styles/tokens.css tailwind.config.js
git commit -m "feat(tokens): liquid glass materials + concentric radius scale"
```

---

### Task 2: `useScrollEdge` hook (TDD)

**Files:**
- Create: `src/lib/useScrollEdge.ts`
- Create: `src/lib/useScrollEdge.test.ts`

- [ ] **Step 1: Write the failing test**

`src/lib/useScrollEdge.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { act, renderHook } from "@testing-library/react";
import { useRef } from "react";
import { useScrollEdge } from "./useScrollEdge";

function makeScrollable(): HTMLDivElement {
  const el = document.createElement("div");
  Object.defineProperty(el, "scrollTop", { value: 0, writable: true });
  document.body.appendChild(el);
  return el;
}

describe("useScrollEdge", () => {
  it("is false initially when scrolled to top", () => {
    const el = makeScrollable();
    const { result } = renderHook(() => {
      const ref = useRef<HTMLDivElement>(el);
      return useScrollEdge(ref);
    });
    expect(result.current).toBe(false);
  });

  it("becomes true when the container scrolls down, false again at top", () => {
    const el = makeScrollable();
    const { result } = renderHook(() => {
      const ref = useRef<HTMLDivElement>(el);
      return useScrollEdge(ref);
    });

    act(() => {
      (el as any).scrollTop = 120;
      el.dispatchEvent(new Event("scroll"));
    });
    expect(result.current).toBe(true);

    act(() => {
      (el as any).scrollTop = 0;
      el.dispatchEvent(new Event("scroll"));
    });
    expect(result.current).toBe(false);
  });

  it("handles a null ref without crashing", () => {
    const { result } = renderHook(() => {
      const ref = useRef<HTMLDivElement>(null);
      return useScrollEdge(ref);
    });
    expect(result.current).toBe(false);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npm test -- --run src/lib/useScrollEdge.test.ts`
Expected: FAIL — cannot resolve `./useScrollEdge`.

- [ ] **Step 3: Implement the hook**

`src/lib/useScrollEdge.ts`:

```ts
import { useEffect, useState, type RefObject } from "react";

/**
 * True while the given scroll container is scrolled away from its top edge.
 * Drives the Tahoe scroll-edge effect: toolbars are transparent at rest and
 * gain a glass wash + hairline once content moves beneath them.
 */
export function useScrollEdge(ref: RefObject<HTMLElement>): boolean {
  const [scrolled, setScrolled] = useState(false);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const update = () => setScrolled(el.scrollTop > 0);
    update();
    el.addEventListener("scroll", update, { passive: true });
    return () => el.removeEventListener("scroll", update);
  }, [ref]);

  return scrolled;
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npm test -- --run src/lib/useScrollEdge.test.ts`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add src/lib/useScrollEdge.ts src/lib/useScrollEdge.test.ts
git commit -m "feat(lib): useScrollEdge hook for scroll-edge toolbar effect"
```

---

### Task 3: Scroll-edge `Toolbar`

**Files:**
- Modify: `src/components/ui/Toolbar.tsx`
- Modify: `src/components/ui/__tests__/Toolbar.test.tsx`

- [ ] **Step 1: Add failing tests**

Append to the existing describe block in `src/components/ui/__tests__/Toolbar.test.tsx` (match the file's existing render/import style):

```tsx
  it("is transparent at rest (no scrolled attribute)", () => {
    render(<Toolbar data-testid="tb" />);
    const tb = screen.getByTestId("tb");
    expect(tb).not.toHaveAttribute("data-scrolled", "true");
    expect(tb.className).toContain("sticky");
  });

  it("gains the glass edge when `scrolled` is true", () => {
    render(<Toolbar data-testid="tb" scrolled />);
    const tb = screen.getByTestId("tb");
    expect(tb).toHaveAttribute("data-scrolled", "true");
  });
```

- [ ] **Step 2: Run to verify failure**

Run: `npm test -- --run src/components/ui/__tests__/Toolbar.test.tsx`
Expected: FAIL — `scrolled` prop unknown / `sticky` missing.

- [ ] **Step 3: Implement**

Replace the `Root` component in `src/components/ui/Toolbar.tsx`:

```tsx
import { forwardRef } from "react";
import { cn } from "../../lib/cn";

type RootProps = React.HTMLAttributes<HTMLDivElement> & {
  /** From useScrollEdge(scrollRef) — true once content scrolls beneath. */
  scrolled?: boolean;
};

const Root = forwardRef<HTMLDivElement, RootProps>(function Toolbar(
  { className, children, scrolled = false, ...rest },
  ref,
) {
  return (
    <div
      ref={ref}
      data-scrolled={scrolled || undefined}
      className={cn(
        "sticky top-0 z-10 flex h-11 items-center gap-2 px-4",
        "border-b border-transparent transition-colors duration-150",
        "data-[scrolled]:glass-edge data-[scrolled]:border-separator",
        className,
      )}
      {...rest}
    >
      {children}
    </div>
  );
});
```

(`Spacer` and the export stay unchanged.)

- [ ] **Step 4: Run ui tests**

Run: `npm test -- --run src/components/ui`
Expected: PASS. If a pre-existing Toolbar test asserts `border-separator` unconditionally, update it to assert the new rest-state classes (`border-transparent`).

- [ ] **Step 5: Commit**

```bash
git add src/components/ui/Toolbar.tsx src/components/ui/__tests__/Toolbar.test.tsx
git commit -m "feat(ui): scroll-edge Toolbar — sticky, glass wash when scrolled"
```

---

### Task 4: Control polish — `_shared.ts` + Button

**Files:**
- Modify: `src/components/ui/_shared.ts`
- Modify: `src/components/ui/Button.tsx`
- Modify: `src/components/ui/__tests__/Button.test.tsx`

- [ ] **Step 1: Add failing tests**

Append to `src/components/ui/__tests__/Button.test.tsx`:

```tsx
  it("uses the control radius and pressed scale", () => {
    render(<Button>Go</Button>);
    const btn = screen.getByRole("button", { name: "Go" });
    expect(btn.className).toContain("rounded-control");
    expect(btn.className).toContain("active:scale-[0.97]");
    expect(btn.className).not.toContain("cursor-pointer");
  });

  it("primary variant carries the glass top highlight", () => {
    render(<Button variant="primary">Go</Button>);
    const btn = screen.getByRole("button", { name: "Go" });
    expect(btn.className).toContain("shadow-[inset_0_1px_0_0_rgba(255,255,255,0.18)]");
  });
```

- [ ] **Step 2: Run to verify failure**

Run: `npm test -- --run src/components/ui/__tests__/Button.test.tsx`
Expected: FAIL on both new tests.

- [ ] **Step 3: Implement**

In `src/components/ui/_shared.ts`, add:

```ts
/** AppKit-feel press: tiny scale-down, transform+color transitions only. */
export const pressClasses =
  "active:scale-[0.97] transition-[transform,background-color,color,border-color,box-shadow] duration-100 select-none";
```

In `src/components/ui/Button.tsx`:
- import `pressClasses` from `./_shared`;
- in `variantClasses.primary`, append the highlight (dropped while pressed):

```ts
  primary:
    "bg-accent text-accent-label hover:bg-accent-hover active:bg-accent-pressed shadow-[inset_0_1px_0_0_rgba(255,255,255,0.18)] active:shadow-none",
```

- in the `cn(...)` call, replace
  `"inline-flex items-center justify-center rounded-md font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed"`
  with
  `"inline-flex items-center justify-center rounded-control font-medium disabled:opacity-50 disabled:cursor-not-allowed"`
  and add `pressClasses` as the next argument.

- [ ] **Step 4: Run tests**

Run: `npm test -- --run src/components/ui/__tests__/Button.test.tsx`
Expected: PASS. Fix any existing assertions that referenced `rounded-md`/`transition-colors` on Button.

- [ ] **Step 5: Commit**

```bash
git add src/components/ui/_shared.ts src/components/ui/Button.tsx src/components/ui/__tests__/Button.test.tsx
git commit -m "feat(ui): Button — control radius, pressed scale, primary glass highlight"
```

---

### Task 5: Radius migration — Input, Select, Textarea, Checkbox

**Files:**
- Modify: `src/components/ui/Input.tsx`, `Select.tsx`, `Textarea.tsx`, `Checkbox.tsx`
- Modify: matching tests under `src/components/ui/__tests__/`

- [ ] **Step 1: Add failing tests**

In `Input.test.tsx`, `Select.test.tsx`, `Textarea.test.tsx` add (adapting the file's existing query helper):

```tsx
  it("uses the control radius", () => {
    // query the rendered element the way the file's other tests do
    expect(element.className).toContain("rounded-control");
  });
```

In `Checkbox.test.tsx`:

```tsx
  it("uses the field radius", () => {
    expect(element.className).toContain("rounded-field");
  });
```

- [ ] **Step 2: Run to verify failures**

Run: `npm test -- --run src/components/ui`
Expected: 4 new failures.

- [ ] **Step 3: Implement**

- `Input.tsx`: in the class string, `rounded-md` → `rounded-control`.
- `Select.tsx`: `rounded-md` → `rounded-control`.
- `Textarea.tsx`: `rounded-md` → `rounded-control`.
- `Checkbox.tsx`: leading `"inline-flex items-center justify-center rounded border ..."` → `"inline-flex items-center justify-center rounded-field border ..."`.

- [ ] **Step 4: Run tests**

Run: `npm test -- --run src/components/ui`
Expected: PASS (update any old assertions pinned to `rounded-md`/`rounded`).

- [ ] **Step 5: Commit**

```bash
git add src/components/ui/Input.tsx src/components/ui/Select.tsx src/components/ui/Textarea.tsx src/components/ui/Checkbox.tsx src/components/ui/__tests__
git commit -m "feat(ui): migrate field controls to concentric radius scale"
```

---

### Task 6: SegmentedControl + Toggle press feel + radius

**Files:**
- Modify: `src/components/ui/SegmentedControl.tsx`
- Modify: `src/components/ui/Toggle.tsx`
- Modify: `src/components/ui/__tests__/SegmentedControl.test.tsx`

- [ ] **Step 1: Add failing test**

```tsx
  it("container uses control radius; items press-scale", () => {
    render(
      <SegmentedControl value="a" onValueChange={() => {}} aria-label="view">
        <SegmentedControl.Item value="a">A</SegmentedControl.Item>
      </SegmentedControl>,
    );
    const item = screen.getByRole("radio", { name: "A" });
    expect(item.className).toContain("active:scale-[0.97]");
  });
```

(If the existing tests query items via `getByRole("button")` or text, match that style.)

- [ ] **Step 2: Run to verify failure**

Run: `npm test -- --run src/components/ui/__tests__/SegmentedControl.test.tsx`
Expected: FAIL.

- [ ] **Step 3: Implement**

In `SegmentedControl.tsx`:
- import `pressClasses` from `./_shared`;
- Root: `rounded-md` → `rounded-control`;
- Item: `rounded-sm` → `rounded-field`, replace `transition-colors` by adding `pressClasses` to the `cn(...)` list.

In `Toggle.tsx` (press feel on the thumb, AppKit-switch style — the thumb
stretches slightly while pressed instead of the whole control scaling):
- on `Switch.Thumb`, extend the className:

```tsx
        className={cn(
          "block rounded-full bg-white transition-transform shadow-sm active:scale-x-110",
          thumbSize[size],
        )}
```

- [ ] **Step 4: Run tests**

Run: `npm test -- --run src/components/ui/__tests__/SegmentedControl.test.tsx src/components/ui/__tests__/Toggle.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/components/ui/SegmentedControl.tsx src/components/ui/Toggle.tsx src/components/ui/__tests__/SegmentedControl.test.tsx
git commit -m "feat(ui): SegmentedControl + Toggle — radius scale + press feel"
```

---

### Task 7: ContextMenu glass overlay material

**Files:**
- Modify: `src/components/ui/ContextMenu.tsx`
- Modify: `src/components/ui/__tests__/ContextMenu.test.tsx`

- [ ] **Step 1: Add failing test**

```tsx
  it("content uses the glass overlay material", async () => {
    // open the menu the way existing tests in this file do, then:
    const content = await screen.findByRole("menu");
    expect(content.className).toContain("glass-overlay");
    expect(content.className).toContain("rounded-card");
  });
```

- [ ] **Step 2: Run to verify failure**

Run: `npm test -- --run src/components/ui/__tests__/ContextMenu.test.tsx`
Expected: FAIL.

- [ ] **Step 3: Implement**

In `ContextMenu.tsx` `Content`, replace

```
"z-50 min-w-[10rem] overflow-hidden rounded-md border border-separator bg-control-background p-1 shadow-lg backdrop-blur-md"
```

with

```
"z-50 min-w-[10rem] overflow-hidden rounded-card glass-overlay p-1"
```

Item: `rounded-sm` → `rounded-field`.

- [ ] **Step 4: Run tests**

Run: `npm test -- --run src/components/ui/__tests__/ContextMenu.test.tsx`
Expected: PASS (update assertions pinned to the old classes).

- [ ] **Step 5: Commit**

```bash
git add src/components/ui/ContextMenu.tsx src/components/ui/__tests__/ContextMenu.test.tsx
git commit -m "feat(ui): ContextMenu — liquid glass overlay material"
```

---

### Task 8: Shell — content panel + sidebar island elevation

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/components/Sidebar.tsx` (container only)

- [ ] **Step 1: Content panel**

In `src/App.tsx`, change

```tsx
      <div className="flex-1 rounded-xl bg-control-background overflow-hidden">
```

to

```tsx
      <div className="flex-1 rounded-window bg-control-background overflow-hidden border border-separator shadow-island">
```

- [ ] **Step 2: Sidebar island**

In `src/components/Sidebar.tsx`, find the root container element (the outermost div the component returns). Add `rounded-window border border-separator shadow-island overflow-hidden` to its className, and **verify it has no opaque `bg-*` class** (the native glass must show through; `bg-sidebar-background` with its 0.3 alpha companion is fine).

- [ ] **Step 3: Manual check**

Run: `npm run tauri dev` (briefly): sidebar island shows desktop-refracting glass with hairline + shadow; content panel matches. Light + dark.

- [ ] **Step 4: Run tests**

Run: `npm test -- --run src/components/Sidebar.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/App.tsx src/components/Sidebar.tsx
git commit -m "feat(shell): elevated glass sidebar island + content panel (floating islands)"
```

---

### Task 9: Wire scroll-edge toolbars in ProjectView + RecordingView

**Files:**
- Modify: `src/components/ProjectView.tsx`
- Modify: `src/components/RecordingView.tsx`

- [ ] **Step 1: ProjectView**

Locate the scrollable content container (the element with `overflow-y-auto`) and the `<Toolbar>` usage. Restructure so the `Toolbar` is the first child **inside** the scroll container (sticky works within the scrolling element), and wire the hook:

```tsx
import { useScrollEdge } from "../lib/useScrollEdge";
// inside component:
const scrollRef = useRef<HTMLDivElement>(null);
const scrolled = useScrollEdge(scrollRef);
// JSX:
<div ref={scrollRef} className="h-full overflow-y-auto">
  <Toolbar scrolled={scrolled}> ... existing toolbar children ... </Toolbar>
  ... existing scrollable content ...
</div>
```

Preserve all existing toolbar children/props. Remove any `border-b border-separator` the view added around the toolbar (the Toolbar handles its own edge now).

- [ ] **Step 2: RecordingView**

Same restructure with its own `scrollRef`.

- [ ] **Step 3: Manual check**

Run: `npm run tauri dev`: at rest the toolbar is transparent over the content panel; scrolling brings up the glass wash + hairline; returning to top dissolves it.

- [ ] **Step 4: Run full frontend tests**

Run: `npm test -- --run`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/components/ProjectView.tsx src/components/RecordingView.tsx
git commit -m "feat(views): scroll-edge toolbars in project + recording views"
```

---

### Task 10: Card recipe normalization

**Files:**
- Modify: `src/components/StageCard.tsx`, `RecordCard.tsx`, `RecordCallCard.tsx`, `StagingList.tsx`, `AskPanel.tsx`, `OutputPreview.tsx`

- [ ] **Step 1: Apply the card recipe**

In each file, normalize the main card/container surfaces to the single recipe:

- container/card backgrounds: whatever ad-hoc mix of `bg-control-background` / `bg-fill-*` + `rounded-md|lg|xl` they use → `bg-fill-quaternary border border-separator rounded-card`;
- interactive cards (clickable rows/cards with hover styling): hover state → `hover:bg-fill-tertiary`;
- nested inner surfaces (e.g. code/preview wells inside a card): `rounded-control`;
- do NOT touch semantic colors (status dots, accent fills) or layout classes.

`RecordCard.tsx` / `RecordCallCard.tsx`: the dashed "start" affordances keep their dashed border but adopt `rounded-card` and drop `cursor-pointer` (AppKit arrow-cursor rule).

- [ ] **Step 2: Run tests**

Run: `npm test -- --run`
Expected: PASS (AskPanel has tests; fix any class-pinned assertions).

- [ ] **Step 3: Manual check**

Run: `npm run tauri dev`: ProjectView staging rows + record cards, RecordingView stage cards + output preview + Ask panel all share one surface language; radii look concentric against the window edge.

- [ ] **Step 4: Commit**

```bash
git add src/components/StageCard.tsx src/components/RecordCard.tsx src/components/RecordCallCard.tsx src/components/StagingList.tsx src/components/AskPanel.tsx src/components/OutputPreview.tsx
git commit -m "feat(views): unified card recipe — quaternary fill, hairline, card radius"
```

---

### Task 11: Overlays + remaining views

**Files:**
- Modify: `src/components/DownloadModelModal.tsx`, `RecordingDropZone.tsx`, `SettingsWindow.tsx`, `SettingsForm.tsx`, `TemplateEditor.tsx`, `Sidebar.tsx` (rows)

- [ ] **Step 1: DownloadModelModal**

The modal panel (the centered box above the scrim): replace its `bg-* border rounded-* shadow-*` classes with `glass-overlay rounded-card`. The scrim stays.

- [ ] **Step 2: RecordingDropZone**

`rounded-xl` → `rounded-card`; remove `cursor-pointer`.

- [ ] **Step 3: Settings + TemplateEditor pass**

- `SettingsWindow.tsx` / `SettingsForm.tsx`: section containers → card recipe (`bg-fill-quaternary border border-separator rounded-card`); labels with `cursor-pointer` → remove the cursor class (keep `select-none`).
- `TemplateEditor.tsx`: same; its single `rounded-*` container → `rounded-card`; label `cursor-pointer` → removed.

- [ ] **Step 4: Sidebar rows**

In `Sidebar.tsx` row classNames (`Sidebar.tsx:332` and `Sidebar.tsx:403`): `rounded-md` → `rounded-control`; remove `cursor-pointer` (AppKit arrow). Keep focus-ring classes.

- [ ] **Step 5: Run tests + commit**

Run: `npm test -- --run`
Expected: PASS.

```bash
git add src/components/DownloadModelModal.tsx src/components/RecordingDropZone.tsx src/components/SettingsWindow.tsx src/components/SettingsForm.tsx src/components/TemplateEditor.tsx src/components/Sidebar.tsx
git commit -m "feat(views): glass modal, settings card pass, AppKit cursor sweep"
```

---

### Task 12: macOS overlay scrollbars

**Files:**
- Modify: `src/index.css`

- [ ] **Step 1: Replace the scrollbar block**

Replace the existing `/* Custom scrollbar */` rules with:

```css
/* macOS-style overlay scrollbars: invisible at rest, thumb appears while
 * the container is hovered. WKWebView honors the -webkit pseudo-elements. */
::-webkit-scrollbar {
  width: 8px;
  height: 8px;
}
::-webkit-scrollbar-track {
  background: transparent;
}
::-webkit-scrollbar-thumb {
  background: transparent;
  border-radius: 4px;
  border: 2px solid transparent;
  background-clip: padding-box;
}
*:hover > ::-webkit-scrollbar-thumb,
:hover::-webkit-scrollbar-thumb {
  background: var(--color-fill-primary);
  background-clip: padding-box;
}
::-webkit-scrollbar-thumb:hover {
  background: var(--color-fill-secondary);
  background-clip: padding-box;
}
::-webkit-scrollbar-corner {
  background: transparent;
}
```

- [ ] **Step 2: Manual check**

Run: `npm run tauri dev`: scrollbars invisible at rest; appear on container hover; thumb has the inset overlay look.

- [ ] **Step 3: Commit**

```bash
git add src/index.css
git commit -m "feat(css): macOS overlay-style scrollbars"
```

---

### Task 13: Final verification sweep

**Files:** none new.

- [ ] **Step 1: Cursor + radius audit**

```bash
grep -rn "cursor-pointer" src --include="*.tsx" | grep -v test
grep -rn "rounded-md\|rounded-lg\b\|rounded-xl" src/components --include="*.tsx" | grep -v test
```

Expected: no `cursor-pointer` outside genuine links; remaining legacy radii only where intentionally untouched (report any stragglers and fix).

- [ ] **Step 2: Full test suite**

Run: `npm test -- --run`
Expected: all green.

- [ ] **Step 3: Manual checklist (dev app)**

- Sidebar: real glass refracting desktop, island elevation, selection capsule.
- Toolbars: transparent at rest → glass on scroll, both main views.
- Context menu + Select dropdown + DownloadModelModal: glass overlay material.
- Buttons: arrow cursor, pressed scale, primary highlight.
- Dark mode + light mode + an accent color other than blue.
- System Settings → Accessibility → Reduce transparency ON: everything opaque, no blur, no broken contrast.

- [ ] **Step 4: Commit any fixes + final commit**

```bash
git add -A -- src
git commit -m "polish(ui): liquid glass phase 1 verification fixes"
```

---

## Out of scope (later phases)

- Phase 2: menu bar completeness, shortcuts, arrow-key list nav, dialog key semantics.
- Phase 3: drag & drop, inline rename, sheet dialogs, window behaviors.
