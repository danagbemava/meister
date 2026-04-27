# Control Library Foundations (Sub-project #3a)

First of three slices of sub-project #3 ("control library") in the native-macOS overhaul. This slice adds the foundation primitives — `Button`, `Input`, `Textarea`, `Select`, `Checkbox`, `Toggle` — and migrates every existing call site in the app to use them. Sub-projects #3b (composite primitives: `SegmentedControl`, `ContextMenu`, `RadioGroup`, `Tooltip`, `Slider`) and #3c (`Toolbar`, possibly merged with #4) follow.

## Context

After sub-projects #1 (design tokens) and #2 (window vibrancy + Music-style islands), Meister's color, typography, and chrome story are HIG-aligned. But every button, input, and select inline-styles its own classes. Disabled states, hover/active states, focus rings, and sizing are inconsistent across components. This slice replaces the ad-hoc styles with a small library of reusable primitives that bake the right tokens, sizes, and ARIA into one place.

## Goals

- Ship six primitives in `src/components/ui/`: `Button`, `Input`, `Textarea`, `Select`, `Checkbox`, `Toggle`.
- Each primitive supports three sizes (`sm`/`md`/`lg`) mapping to macOS HIG Small / Regular / Large. Default `md`.
- `Button` supports four variants: `primary`, `secondary`, `destructive`, `ghost`.
- All primitives forward refs and pass through unknown HTML attributes via `...rest`.
- Focus rings render on `:focus-visible` only (keyboard, not mouse) — matches macOS convention.
- `Checkbox` and `Toggle` wrap Radix UI primitives (`@radix-ui/react-checkbox`, `@radix-ui/react-switch`) for keyboard nav and ARIA correctness. Other primitives are styled native HTML elements.
- Replace every existing button/input/select/checkbox call site (~9 component files) with the new primitives. Delete the `.input` utility class from `src/index.css` once migration is complete.
- Add Vitest tests for each primitive's behavioral contract.

## Non-goals

- No `SegmentedControl` (sub-project #3b).
- No `ContextMenu`, `RadioGroup`, `Tooltip`, `Slider` (#3b).
- No `Toolbar` (#3c, possibly merged with #4).
- No layout or design changes to the existing components beyond replacing their controls. We don't redesign `RecordCard`'s layout — we just swap its `<button>` elements for `<Button>`.
- No new visual-regression tooling.
- No icon library — buttons that need icons keep their current emoji/text content. Icons land in sub-project #5.
- No accessibility audit beyond what Radix gives us by default.
- No internationalization or RTL.
- No `asChild` Polymorphic prop on `Button` until a real call site needs it.
- No icon-only `Button` variant until icons exist.

## Architecture

### File layout

```
src/components/ui/
  Button.tsx           — variants × sizes; styled <button>
  Input.tsx            — styled <input>; sizes; aria-invalid styling
  Textarea.tsx         — styled <textarea>; sizes; optional autoResize
  Select.tsx           — styled native <select>; chevron via CSS background
  Checkbox.tsx         — Radix Checkbox primitive; sizes; indeterminate
  Toggle.tsx           — Radix Switch primitive; sizes
  _shared.ts           — Size type + shared size helpers + focus-class strings
  index.ts             — barrel export

src/components/ui/__tests__/
  Button.test.tsx
  Input.test.tsx
  Textarea.test.tsx
  Select.test.tsx
  Checkbox.test.tsx
  Toggle.test.tsx
```

### Component pattern

Every primitive follows the same shape — Button shown as the canonical example:

```tsx
import { forwardRef } from "react";
import { cn } from "../../lib/cn";  // tiny classnames helper, see Section 6
import { Size, sizeClasses, focusClasses } from "./_shared";

type Variant = "primary" | "secondary" | "destructive" | "ghost";

type Props = React.ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: Variant;
  size?: Size;
};

export const Button = forwardRef<HTMLButtonElement, Props>(function Button(
  { variant = "secondary", size = "md", className, ...rest },
  ref,
) {
  return (
    <button
      ref={ref}
      className={cn(
        "inline-flex items-center justify-center rounded-md font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed",
        sizeClasses(size),
        focusClasses,
        variantClasses[variant],
        className,
      )}
      {...rest}
    />
  );
});

const variantClasses: Record<Variant, string> = {
  primary: "bg-accent text-accent-label hover:bg-accent-hover active:bg-accent-pressed",
  secondary: "bg-fill-tertiary text-label-primary hover:bg-fill-secondary active:bg-fill-primary",
  destructive: "bg-status-error text-white hover:bg-status-error/90 active:bg-status-error/80",
  ghost: "text-label-primary hover:bg-fill-tertiary active:bg-fill-secondary",
};
```

`Checkbox` and `Toggle` follow the same shape but compose Radix primitives (`<Checkbox.Root>`, `<Switch.Root>`) instead of native `<input>` elements. Visual style is fully ours; Radix gives us keyboard, ARIA, and focus management.

### `_shared.ts`

```ts
export type Size = "sm" | "md" | "lg";

export function sizeClasses(size: Size): string {
  switch (size) {
    case "sm": return "h-6 px-2.5 text-callout";
    case "md": return "h-8 px-3 text-body";
    case "lg": return "h-9 px-4 text-headline";
  }
}

export const focusClasses =
  "outline-none focus-visible:ring-2 focus-visible:ring-focus-ring focus-visible:ring-offset-0";
```

### Sizing values (HIG-aligned)

| Size | Height | Horizontal padding | Font token |
|---|---|---|---|
| `sm` | 24px (`h-6`) | 10px (`px-2.5`) | `text-callout` (12px) |
| `md` | 32px (`h-8`) | 12px (`px-3`) | `text-body` (13px) |
| `lg` | 36px (`h-9`) | 16px (`px-4`) | `text-headline` (13px / 600) |

### `Button` variant styling

- `primary`: `bg-accent text-accent-label hover:bg-accent-hover active:bg-accent-pressed`
- `secondary`: `bg-fill-tertiary text-label-primary hover:bg-fill-secondary active:bg-fill-primary`
- `destructive`: `bg-status-error text-white hover:bg-status-error/90 active:bg-status-error/80`
- `ghost`: `text-label-primary hover:bg-fill-tertiary active:bg-fill-secondary`

`destructive` uses literal `text-white` because we don't have a `status-label` token yet and white renders correctly against all four status saturated colors. Adding a `status-label` token is out of scope.

### `Input` / `Textarea` / `Select` styling

- Idle: `bg-fill-tertiary border border-separator text-label-primary placeholder:text-text-placeholder rounded-md`
- Focus: handled by `focusClasses` above (`:focus-visible` ring) plus `focus-visible:border-accent`
- Disabled: `disabled:opacity-50 disabled:cursor-not-allowed`
- Invalid: `aria-invalid="true"` triggers `aria-invalid:border-status-error aria-invalid:focus-visible:ring-status-error/40` (uses Tailwind's `aria-invalid:` variant)

`Select`: chevron is added via inline SVG background using `--color-label-secondary`. Right padding is bumped (`pr-8`) to leave room.

`Textarea` `autoResize` (optional prop, default false): when true, height grows with content via JS effect that sets `style.height` on input.

### `Checkbox` / `Toggle` styling

- `Checkbox`: 16px box at `sm`, 18px at `md`, 22px at `lg`. Idle: `bg-fill-tertiary border border-separator`. Checked: `bg-accent border-accent`. Checkmark: white SVG inside. Indeterminate: white horizontal bar.
- `Toggle`: pill shape. Off: 32×18 (sm), 38×22 (md), 44×26 (lg) with `bg-fill-secondary` track and white thumb. On: `bg-accent` track. Thumb slides 14/16/18 px on toggle. Smooth transition (~150ms).

## Migration mapping

For each existing component file, replace controls per this table:

| Current pattern | → | New |
|---|---|---|
| `<button class="bg-accent text-accent-label …">…</button>` | → | `<Button variant="primary">…</Button>` |
| `<button class="text-label-secondary hover:bg-fill-tertiary …">…</button>` | → | `<Button variant="ghost">…</Button>` |
| `<button class="bg-status-error …">…</button>` | → | `<Button variant="destructive">…</Button>` |
| `<button class="bg-fill-tertiary border-separator …">…</button>` | → | `<Button variant="secondary">…</Button>` |
| `<button class="bg-window-background border-separator …">…</button>` (recessed control) | → | `<Button variant="secondary">…</Button>` |
| `<input class="input" …>` | → | `<Input ... />` |
| `<input class="…tailwind…" type="text">` (any custom-styled text input) | → | `<Input ... />` |
| `<textarea class="input" …>` | → | `<Textarea ... />` |
| `<textarea class="…tailwind…">` | → | `<Textarea ... />` |
| `<select class="input">` | → | `<Select>` |
| `<input type="checkbox" …>` | → | `<Checkbox ... />` |

Files affected (9):

- `src/components/Sidebar.tsx` — `+ New project` button, project rows, recording rows, project-name input, confirm/cancel buttons.
- `src/components/ProjectView.tsx` — Delete button, format toggle buttons (these become checkboxes or toggles? See note below), recording grid item buttons.
- `src/components/RecordingView.tsx` — Run, Start over, source-list buttons.
- `src/components/RecordCard.tsx` — Stop / Pause / Resume / Discard / Save buttons.
- `src/components/RecordCallCard.tsx` — same set as RecordCard, plus Pre-flight checkbox.
- `src/components/StagingList.tsx` — Cancel / Create buttons, recording-name input, drag-handle button.
- `src/components/DownloadModelModal.tsx` — Download / Cancel / Retry / Close buttons.
- `src/components/TemplateEditor.tsx` — Textarea, name input, drop-zone trigger button.
- `src/components/SettingsForm.tsx` — All inputs, the Whisper-model select, format checkboxes (each becomes `<Checkbox>`).

After all 9 migrations, delete the `.input` utility class from `src/index.css`.

**ProjectView format toggle buttons:** these currently render as a row of buttons that toggle output formats. They are semantically checkboxes. Migrate them to `<Checkbox label="…">` instead of `<Button>`. Document the visual change for the user during QA — the row of pill buttons becomes a row of checkboxes.

## Testing

### Automated (Vitest + Testing Library)

Each primitive gets a focused test file. Targets per primitive:

- `Button.test.tsx` (5 tests): renders children; respects `variant` and `size` (assert classes match expected substrings); `disabled` blocks `onClick`; passes through `aria-label`; forwards ref.
- `Input.test.tsx` (4 tests): renders with placeholder; controlled value updates; `aria-invalid="true"` applies error styling; forwards ref.
- `Textarea.test.tsx` (3 tests): renders; controlled; `autoResize` grows scroll height (mocked DOM measurement).
- `Select.test.tsx` (3 tests): renders option list; controlled value; chevron icon present in DOM.
- `Checkbox.test.tsx` (5 tests): renders; Space toggles checked state; controlled checked; indeterminate renders bar; aria-checked correct.
- `Toggle.test.tsx` (4 tests): renders; Space toggles; controlled; aria-checked correct.

Total ~24 new tests, bringing the suite to ~42.

The Testing Library install (`@testing-library/react`, `@testing-library/jest-dom`, `@testing-library/user-event`) becomes part of the install task at the top of the plan.

### Manual QA checklist (last task of the sub-project)

- Click every variant of `Button` in the running app — primary, secondary, destructive, ghost. Verify hover, active, disabled states.
- Tab through a screen with multiple controls — focus rings appear only when navigating with Tab/arrow keys, not when clicking with mouse.
- Type into every `Input` and `Textarea` in the app. Verify focus ring renders correctly inside the main island's translucent background.
- `Checkbox` and `Toggle`: navigate to one, press Space, confirm state flips. Use mouse: click, confirm state flips.
- `Select`: open with mouse and with keyboard (Space/Enter), navigate options, choose one.
- Cross-app comparison: open Mail.app or Notes.app, focus a button or text field. Compare focus ring appearance and timing — should feel comparable.

## Error handling

| Failure | Behavior |
|---|---|
| `Button` `onClick` throws | React's normal error boundary handling. No primitive-level catch. |
| `Input`'s `onChange` throws | Same. |
| Radix primitive throws (e.g. mounted in a non-React context) | Should not happen in this app. Tests cover the happy path. |
| Custom invalid `variant`/`size` values | TypeScript prevents these at compile time. Runtime defaults via destructuring (e.g. `variant = "secondary"`) handle JavaScript callers. |

## Out of scope / future

- `Button` icon variants (sub-project #5).
- `asChild` polymorphic prop (until a real consumer needs it).
- Form-validation integrations (react-hook-form etc.).
- Per-control HIG-mode-specific styling beyond what tokens already give us.
- Component documentation / Storybook (the codebase doesn't use it; no current need).

## Section 6 — `cn` helper

A trivial classname-merging helper is needed by all primitives. Convention is the `clsx` + `tailwind-merge` combo, but for a project this size, a 5-line `cn` is fine:

```ts
// src/lib/cn.ts
export function cn(...args: Array<string | undefined | false | null>): string {
  return args.filter(Boolean).join(" ");
}
```

If `tailwind-merge` is needed later (when a primitive needs to override a class consumer-supplied via `className`), we add it then. For now this is sufficient because primitives put their own classes first and consumer `className` last, so consumer wins for conflicts.
