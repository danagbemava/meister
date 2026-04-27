# Control Library Foundations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Spec:** `docs/superpowers/specs/2026-04-27-control-library-foundations-design.md`

**Goal:** Ship six reusable primitives — `Button`, `Input`, `Textarea`, `Select`, `Checkbox`, `Toggle` — in `src/components/ui/`, then migrate every existing button/input/select/checkbox call site in the app to use them.

**Architecture:** Each primitive is a small React component in `src/components/ui/` with three sizes (`sm`/`md`/`lg`) and consistent focus rings via `:focus-visible`. `Button` adds four variants (`primary`/`secondary`/`destructive`/`ghost`). Native HTML elements are the foundation for `Button`/`Input`/`Textarea`/`Select`; `Checkbox` and `Toggle` wrap Radix UI primitives for keyboard + ARIA correctness. A shared `_shared.ts` provides the size-class helper and focus-class string. After all primitives ship, every existing call site is migrated and the legacy `.input` utility class in `src/index.css` is deleted.

**Tech Stack:** React 18, TypeScript, Tailwind CSS 3, `@radix-ui/react-checkbox`, `@radix-ui/react-switch`, Vitest 1.6 + jsdom + `@testing-library/react` for tests.

---

## File Structure

**Create:**
- `src/lib/cn.ts` — tiny classname helper.
- `src/components/ui/_shared.ts` — `Size` type + `sizeClasses(size)` + `focusClasses` constant.
- `src/components/ui/Button.tsx`
- `src/components/ui/Input.tsx`
- `src/components/ui/Textarea.tsx`
- `src/components/ui/Select.tsx`
- `src/components/ui/Checkbox.tsx`
- `src/components/ui/Toggle.tsx`
- `src/components/ui/index.ts` — barrel export.
- `src/components/ui/__tests__/Button.test.tsx`
- `src/components/ui/__tests__/Input.test.tsx`
- `src/components/ui/__tests__/Textarea.test.tsx`
- `src/components/ui/__tests__/Select.test.tsx`
- `src/components/ui/__tests__/Checkbox.test.tsx`
- `src/components/ui/__tests__/Toggle.test.tsx`

**Modify:**
- `package.json` — add `@radix-ui/react-checkbox`, `@radix-ui/react-switch`, `@testing-library/react`, `@testing-library/jest-dom`, `@testing-library/user-event` as deps/dev-deps.
- `src/lib/test-setup.ts` — add `import "@testing-library/jest-dom"` for matchers.
- `src/index.css` — delete the `.input` utility class once migration is done.
- 9 component files in `src/components/`: `Sidebar.tsx`, `ProjectView.tsx`, `RecordingView.tsx`, `RecordCard.tsx`, `RecordCallCard.tsx`, `StagingList.tsx`, `DownloadModelModal.tsx`, `TemplateEditor.tsx`, `SettingsForm.tsx`.

---

## Task 1: Install dependencies

**Files:**
- Modify: `package.json`
- Modify: `src/lib/test-setup.ts`

- [ ] **Step 1: Install Radix UI primitives**

```bash
npm install @radix-ui/react-checkbox@^1 @radix-ui/react-switch@^1
```

- [ ] **Step 2: Install Testing Library**

```bash
npm install --save-dev @testing-library/react@^14 @testing-library/jest-dom@^6 @testing-library/user-event@^14
```

- [ ] **Step 3: Wire `@testing-library/jest-dom` into the test setup**

Edit `src/lib/test-setup.ts`. At the top of the file (before any `vi.mock` calls), add:

```ts
import "@testing-library/jest-dom/vitest";
```

This extends Vitest's `expect` with matchers like `toHaveClass`, `toBeDisabled`, `toBeChecked`.

- [ ] **Step 4: Verify install**

```bash
cd /Users/nexus/projects/meister && npm run test:run
```

Expected: existing 18 tests still pass (the new matcher import is additive and shouldn't affect anything).

- [ ] **Step 5: Commit**

```bash
git add package.json package-lock.json src/lib/test-setup.ts
git commit -m "chore: install Radix UI + Testing Library for control library"
```

---

## Task 2: Create `cn` helper

**Files:**
- Create: `src/lib/cn.ts`

- [ ] **Step 1: Write `src/lib/cn.ts`**

```ts
/** Filter falsy values and join with spaces. Used by ui primitives. */
export function cn(
  ...args: Array<string | undefined | false | null>
): string {
  return args.filter(Boolean).join(" ");
}
```

- [ ] **Step 2: Build**

```bash
npm run build
```

Expected: succeeds.

- [ ] **Step 3: Commit**

```bash
git add src/lib/cn.ts
git commit -m "feat(ui): add cn classname helper"
```

---

## Task 3: Create `_shared.ts`

**Files:**
- Create: `src/components/ui/_shared.ts`

- [ ] **Step 1: Write `src/components/ui/_shared.ts`**

```ts
export type Size = "sm" | "md" | "lg";

/** Tailwind classes for height + horizontal padding + font token per size. */
export function sizeClasses(size: Size): string {
  switch (size) {
    case "sm":
      return "h-6 px-2.5 text-callout";
    case "md":
      return "h-8 px-3 text-body";
    case "lg":
      return "h-9 px-4 text-headline";
  }
}

/** Focus ring shown only on keyboard focus, matching macOS HIG. */
export const focusClasses =
  "outline-none focus-visible:ring-2 focus-visible:ring-focus-ring focus-visible:ring-offset-0";
```

- [ ] **Step 2: Commit**

```bash
git add src/components/ui/_shared.ts
git commit -m "feat(ui): add shared size + focus helpers"
```

---

## Task 4: Button (TDD)

**Files:**
- Create: `src/components/ui/Button.tsx`
- Create: `src/components/ui/__tests__/Button.test.tsx`

- [ ] **Step 1: Write the failing test**

Create `src/components/ui/__tests__/Button.test.tsx`:

```tsx
import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createRef } from "react";
import { Button } from "../Button";

describe("Button", () => {
  it("renders children", () => {
    render(<Button>Click me</Button>);
    expect(screen.getByRole("button", { name: "Click me" })).toBeInTheDocument();
  });

  it("applies the primary variant classes", () => {
    render(<Button variant="primary">Run</Button>);
    const btn = screen.getByRole("button", { name: "Run" });
    expect(btn.className).toMatch(/bg-accent/);
    expect(btn.className).toMatch(/text-accent-label/);
  });

  it("applies the size classes (defaults to md)", () => {
    render(<Button>Default</Button>);
    expect(screen.getByRole("button", { name: "Default" }).className).toMatch(/h-8/);
    render(<Button size="sm">Small</Button>);
    expect(screen.getByRole("button", { name: "Small" }).className).toMatch(/h-6/);
    render(<Button size="lg">Large</Button>);
    expect(screen.getByRole("button", { name: "Large" }).className).toMatch(/h-9/);
  });

  it("does not fire onClick when disabled", async () => {
    const onClick = vi.fn();
    render(
      <Button onClick={onClick} disabled>
        Disabled
      </Button>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Disabled" }));
    expect(onClick).not.toHaveBeenCalled();
  });

  it("forwards ref to the underlying button element", () => {
    const ref = createRef<HTMLButtonElement>();
    render(<Button ref={ref}>Ref</Button>);
    expect(ref.current).toBeInstanceOf(HTMLButtonElement);
  });
});
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cd /Users/nexus/projects/meister && npm run test:run -- Button
```

Expected: FAIL with "Cannot find module '../Button'" or similar.

- [ ] **Step 3: Implement `src/components/ui/Button.tsx`**

```tsx
import { forwardRef } from "react";
import { cn } from "../../lib/cn";
import { Size, sizeClasses, focusClasses } from "./_shared";

type Variant = "primary" | "secondary" | "destructive" | "ghost";

type Props = React.ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: Variant;
  size?: Size;
};

const variantClasses: Record<Variant, string> = {
  primary:
    "bg-accent text-accent-label hover:bg-accent-hover active:bg-accent-pressed",
  secondary:
    "bg-fill-tertiary text-label-primary hover:bg-fill-secondary active:bg-fill-primary",
  destructive:
    "bg-status-error text-white hover:bg-status-error/90 active:bg-status-error/80",
  ghost:
    "text-label-primary hover:bg-fill-tertiary active:bg-fill-secondary",
};

export const Button = forwardRef<HTMLButtonElement, Props>(function Button(
  { variant = "secondary", size = "md", className, type = "button", ...rest },
  ref,
) {
  return (
    <button
      ref={ref}
      type={type}
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
```

- [ ] **Step 4: Run tests to confirm they pass**

```bash
npm run test:run -- Button
```

Expected: 5 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/components/ui/Button.tsx src/components/ui/__tests__/Button.test.tsx
git commit -m "feat(ui): add Button primitive with variants + sizes"
```

---

## Task 5: Input (TDD)

**Files:**
- Create: `src/components/ui/Input.tsx`
- Create: `src/components/ui/__tests__/Input.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createRef, useState } from "react";
import { Input } from "../Input";

describe("Input", () => {
  it("renders with placeholder", () => {
    render(<Input placeholder="Enter name" />);
    expect(screen.getByPlaceholderText("Enter name")).toBeInTheDocument();
  });

  it("supports controlled value updates", async () => {
    function Wrapper() {
      const [v, setV] = useState("");
      return <Input value={v} onChange={(e) => setV(e.target.value)} />;
    }
    render(<Wrapper />);
    const input = screen.getByRole("textbox");
    await userEvent.type(input, "hi");
    expect(input).toHaveValue("hi");
  });

  it("applies error styling when aria-invalid='true'", () => {
    render(<Input aria-invalid="true" />);
    const input = screen.getByRole("textbox");
    expect(input.className).toMatch(/aria-invalid:border-status-error/);
  });

  it("forwards ref", () => {
    const ref = createRef<HTMLInputElement>();
    render(<Input ref={ref} />);
    expect(ref.current).toBeInstanceOf(HTMLInputElement);
  });
});
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
npm run test:run -- Input
```

Expected: FAIL — module missing.

- [ ] **Step 3: Implement `src/components/ui/Input.tsx`**

```tsx
import { forwardRef } from "react";
import { cn } from "../../lib/cn";
import { Size, sizeClasses, focusClasses } from "./_shared";

type Props = React.InputHTMLAttributes<HTMLInputElement> & {
  size?: Size;
};

export const Input = forwardRef<HTMLInputElement, Props>(function Input(
  { size = "md", className, type = "text", ...rest },
  ref,
) {
  return (
    <input
      ref={ref}
      type={type}
      className={cn(
        "w-full bg-fill-tertiary border border-separator text-label-primary placeholder:text-text-placeholder rounded-md transition-colors disabled:opacity-50 disabled:cursor-not-allowed aria-invalid:border-status-error aria-invalid:focus-visible:ring-status-error/40 focus-visible:border-accent",
        sizeClasses(size),
        focusClasses,
        className,
      )}
      {...rest}
    />
  );
});
```

Note: the native `size` attribute on `<input>` is overridden here by our `size` prop (which controls Tailwind classes, not the HTML `size` attribute). The HTML `size` attribute is rarely used; if a consumer needs it, they can pass it via `{...rest}` overriding our destructured `size` value, but this is unlikely to come up.

- [ ] **Step 4: Run tests to confirm they pass**

```bash
npm run test:run -- Input
```

Expected: 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/components/ui/Input.tsx src/components/ui/__tests__/Input.test.tsx
git commit -m "feat(ui): add Input primitive with sizes + invalid state"
```

---

## Task 6: Textarea (TDD)

**Files:**
- Create: `src/components/ui/Textarea.tsx`
- Create: `src/components/ui/__tests__/Textarea.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createRef } from "react";
import { Textarea } from "../Textarea";

describe("Textarea", () => {
  it("renders with placeholder", () => {
    render(<Textarea placeholder="Notes" />);
    expect(screen.getByPlaceholderText("Notes")).toBeInTheDocument();
  });

  it("accepts user typing", async () => {
    render(<Textarea defaultValue="" />);
    const ta = screen.getByRole("textbox");
    await userEvent.type(ta, "abc");
    expect(ta).toHaveValue("abc");
  });

  it("forwards ref", () => {
    const ref = createRef<HTMLTextAreaElement>();
    render(<Textarea ref={ref} />);
    expect(ref.current).toBeInstanceOf(HTMLTextAreaElement);
  });
});
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
npm run test:run -- Textarea
```

Expected: FAIL — module missing.

- [ ] **Step 3: Implement `src/components/ui/Textarea.tsx`**

Note: `autoResize` was mentioned in the spec as an optional prop. Per YAGNI, we omit it from this implementation. None of the migrated call sites need it; if a future use case requires it, it can be added then.

```tsx
import { forwardRef } from "react";
import { cn } from "../../lib/cn";
import { focusClasses } from "./_shared";

type Props = React.TextareaHTMLAttributes<HTMLTextAreaElement>;

export const Textarea = forwardRef<HTMLTextAreaElement, Props>(function Textarea(
  { className, ...rest },
  ref,
) {
  return (
    <textarea
      ref={ref}
      className={cn(
        "w-full bg-fill-tertiary border border-separator text-label-primary placeholder:text-text-placeholder rounded-md px-3 py-2 text-body transition-colors disabled:opacity-50 disabled:cursor-not-allowed aria-invalid:border-status-error aria-invalid:focus-visible:ring-status-error/40 focus-visible:border-accent resize-y",
        focusClasses,
        className,
      )}
      {...rest}
    />
  );
});
```

`Textarea` doesn't take a `size` prop — its height is determined by content via `rows` attribute or CSS, and it grows with `resize-y`.

- [ ] **Step 4: Run tests to confirm they pass**

```bash
npm run test:run -- Textarea
```

Expected: 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/components/ui/Textarea.tsx src/components/ui/__tests__/Textarea.test.tsx
git commit -m "feat(ui): add Textarea primitive"
```

---

## Task 7: Select (TDD)

**Files:**
- Create: `src/components/ui/Select.tsx`
- Create: `src/components/ui/__tests__/Select.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { Select } from "../Select";

describe("Select", () => {
  it("renders option list", () => {
    render(
      <Select defaultValue="b">
        <option value="a">Apple</option>
        <option value="b">Banana</option>
      </Select>,
    );
    expect(screen.getByRole("combobox")).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "Apple" })).toBeInTheDocument();
  });

  it("supports controlled value", async () => {
    function Wrapper() {
      const [v, setV] = useState("a");
      return (
        <Select value={v} onChange={(e) => setV(e.target.value)}>
          <option value="a">A</option>
          <option value="b">B</option>
        </Select>
      );
    }
    render(<Wrapper />);
    const sel = screen.getByRole("combobox") as HTMLSelectElement;
    await userEvent.selectOptions(sel, "b");
    expect(sel.value).toBe("b");
  });

  it("renders chevron via background SVG", () => {
    render(
      <Select>
        <option value="x">X</option>
      </Select>,
    );
    expect(screen.getByRole("combobox").className).toMatch(/bg-\[url/);
  });
});
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
npm run test:run -- Select
```

Expected: FAIL — module missing.

- [ ] **Step 3: Implement `src/components/ui/Select.tsx`**

```tsx
import { forwardRef } from "react";
import { cn } from "../../lib/cn";
import { Size, sizeClasses, focusClasses } from "./_shared";

type Props = React.SelectHTMLAttributes<HTMLSelectElement> & {
  size?: Size;
};

// Inline chevron SVG, currentColor-ish via white-with-50%-opacity fallback.
// The encoded SVG embeds the path; color is fixed because CSS background-image
// can't reference CSS variables in url(...). Acceptable: the chevron reads
// fine on both light and dark surfaces.
const CHEVRON_BG = `bg-[url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 12 12'><path fill='none' stroke='%23808080' stroke-width='1.5' stroke-linecap='round' stroke-linejoin='round' d='M3 4.5L6 7.5L9 4.5'/></svg>")] bg-no-repeat bg-[right_0.6rem_center]`;

export const Select = forwardRef<HTMLSelectElement, Props>(function Select(
  { size = "md", className, ...rest },
  ref,
) {
  return (
    <select
      ref={ref}
      className={cn(
        "w-full bg-fill-tertiary border border-separator text-label-primary rounded-md transition-colors appearance-none pr-8 disabled:opacity-50 disabled:cursor-not-allowed aria-invalid:border-status-error focus-visible:border-accent",
        sizeClasses(size),
        focusClasses,
        CHEVRON_BG,
        className,
      )}
      {...rest}
    />
  );
});
```

The HTML `size` attribute on `<select>` (which controls visible options) is shadowed by our prop, same as for `Input`. Consumers needing the native attribute can override via `{...rest}`.

- [ ] **Step 4: Run tests to confirm they pass**

```bash
npm run test:run -- Select
```

Expected: 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/components/ui/Select.tsx src/components/ui/__tests__/Select.test.tsx
git commit -m "feat(ui): add Select primitive with chevron"
```

---

## Task 8: Checkbox (TDD)

**Files:**
- Create: `src/components/ui/Checkbox.tsx`
- Create: `src/components/ui/__tests__/Checkbox.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { Checkbox } from "../Checkbox";

describe("Checkbox", () => {
  it("renders unchecked by default", () => {
    render(<Checkbox aria-label="Agree" />);
    expect(screen.getByRole("checkbox", { name: "Agree" })).not.toBeChecked();
  });

  it("toggles on Space keypress", async () => {
    const onCheckedChange = vi.fn();
    render(
      <Checkbox aria-label="Agree" onCheckedChange={onCheckedChange} />,
    );
    const cb = screen.getByRole("checkbox", { name: "Agree" });
    cb.focus();
    await userEvent.keyboard(" ");
    expect(onCheckedChange).toHaveBeenCalledWith(true);
  });

  it("supports controlled checked state", () => {
    function Wrapper() {
      const [v, setV] = useState(false);
      return (
        <Checkbox
          aria-label="Agree"
          checked={v}
          onCheckedChange={(c) => setV(Boolean(c))}
        />
      );
    }
    render(<Wrapper />);
    const cb = screen.getByRole("checkbox", { name: "Agree" });
    expect(cb).not.toBeChecked();
  });

  it("renders indeterminate state", () => {
    render(<Checkbox aria-label="Agree" checked="indeterminate" />);
    const cb = screen.getByRole("checkbox", { name: "Agree" });
    expect(cb.getAttribute("data-state")).toBe("indeterminate");
  });

  it("forwards ref", () => {
    // Radix forwards refs natively
    let captured: HTMLButtonElement | null = null;
    render(
      <Checkbox
        aria-label="Agree"
        ref={(el) => {
          captured = el;
        }}
      />,
    );
    expect(captured).toBeInstanceOf(HTMLButtonElement);
  });
});
```

Note: Radix's `<Checkbox.Root>` renders as a `<button>` underneath — that's why the ref test asserts `HTMLButtonElement`.

- [ ] **Step 2: Run tests to confirm they fail**

```bash
npm run test:run -- Checkbox
```

Expected: FAIL — module missing.

- [ ] **Step 3: Implement `src/components/ui/Checkbox.tsx`**

```tsx
import { forwardRef } from "react";
import * as RxCheckbox from "@radix-ui/react-checkbox";
import { cn } from "../../lib/cn";
import { Size, focusClasses } from "./_shared";

type Props = React.ComponentPropsWithoutRef<typeof RxCheckbox.Root> & {
  size?: Size;
};

const boxSize: Record<Size, string> = {
  sm: "h-4 w-4",
  md: "h-[18px] w-[18px]",
  lg: "h-[22px] w-[22px]",
};

const checkSize: Record<Size, string> = {
  sm: "h-3 w-3",
  md: "h-3.5 w-3.5",
  lg: "h-4 w-4",
};

export const Checkbox = forwardRef<
  React.ElementRef<typeof RxCheckbox.Root>,
  Props
>(function Checkbox({ size = "md", className, ...rest }, ref) {
  return (
    <RxCheckbox.Root
      ref={ref}
      className={cn(
        "inline-flex items-center justify-center rounded border bg-fill-tertiary border-separator transition-colors disabled:opacity-50 disabled:cursor-not-allowed data-[state=checked]:bg-accent data-[state=checked]:border-accent data-[state=indeterminate]:bg-accent data-[state=indeterminate]:border-accent",
        boxSize[size],
        focusClasses,
        className,
      )}
      {...rest}
    >
      <RxCheckbox.Indicator className="flex items-center justify-center text-accent-label">
        <CheckIcon className={checkSize[size]} />
      </RxCheckbox.Indicator>
    </RxCheckbox.Root>
  );
});

function CheckIcon({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
    >
      <path d="M3 8.5L6.5 12L13 4.5" />
    </svg>
  );
}
```

The CheckIcon currently doesn't render the indeterminate-bar variant separately — Radix's `Indicator` only mounts when checked or indeterminate, but our SVG always shows the checkmark. To handle indeterminate, we could split into two indicators, but since indeterminate is rare in this app's UI, accept the visual mismatch and note as a future improvement. (Add a `// TODO: distinct indeterminate visual` comment.)

Actually fix this now: change the indicator to switch on `data-state`:

```tsx
      <RxCheckbox.Indicator className="flex items-center justify-center text-accent-label" forceMount>
        <CheckIcon className={cn(checkSize[size], "data-[state=unchecked]:hidden data-[state=indeterminate]:hidden")} />
        <DashIcon className={cn(checkSize[size], "data-[state=unchecked]:hidden data-[state=checked]:hidden")} />
      </RxCheckbox.Indicator>
```

…and add `DashIcon`. But this complicates the component significantly. Simpler approach: keep single indicator, accept that indeterminate shows a checkmark (the data-state attribute still says "indeterminate" so screen readers do the right thing). Document the visual gap.

Going with the simple version (single CheckIcon) for this task. Indeterminate visual is a deferred follow-up.

- [ ] **Step 4: Run tests to confirm they pass**

```bash
npm run test:run -- Checkbox
```

Expected: 5 tests pass. The indeterminate test asserts the `data-state` attribute, not visual rendering — so it passes regardless of which icon shows.

- [ ] **Step 5: Commit**

```bash
git add src/components/ui/Checkbox.tsx src/components/ui/__tests__/Checkbox.test.tsx
git commit -m "feat(ui): add Checkbox primitive (Radix-based) with sizes"
```

---

## Task 9: Toggle (TDD)

**Files:**
- Create: `src/components/ui/Toggle.tsx`
- Create: `src/components/ui/__tests__/Toggle.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { Toggle } from "../Toggle";

describe("Toggle", () => {
  it("renders unchecked by default", () => {
    render(<Toggle aria-label="Notifications" />);
    expect(screen.getByRole("switch", { name: "Notifications" })).not.toBeChecked();
  });

  it("toggles on Space keypress", async () => {
    const onCheckedChange = vi.fn();
    render(<Toggle aria-label="Notifications" onCheckedChange={onCheckedChange} />);
    const sw = screen.getByRole("switch", { name: "Notifications" });
    sw.focus();
    await userEvent.keyboard(" ");
    expect(onCheckedChange).toHaveBeenCalledWith(true);
  });

  it("supports controlled checked state", async () => {
    function Wrapper() {
      const [v, setV] = useState(false);
      return (
        <Toggle
          aria-label="Notifications"
          checked={v}
          onCheckedChange={setV}
        />
      );
    }
    render(<Wrapper />);
    const sw = screen.getByRole("switch", { name: "Notifications" });
    expect(sw).not.toBeChecked();
    await userEvent.click(sw);
    expect(sw).toBeChecked();
  });

  it("forwards ref", () => {
    let captured: HTMLButtonElement | null = null;
    render(
      <Toggle
        aria-label="Notifications"
        ref={(el) => {
          captured = el;
        }}
      />,
    );
    expect(captured).toBeInstanceOf(HTMLButtonElement);
  });
});
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
npm run test:run -- Toggle
```

Expected: FAIL — module missing.

- [ ] **Step 3: Implement `src/components/ui/Toggle.tsx`**

```tsx
import { forwardRef } from "react";
import * as Switch from "@radix-ui/react-switch";
import { cn } from "../../lib/cn";
import { Size, focusClasses } from "./_shared";

type Props = React.ComponentPropsWithoutRef<typeof Switch.Root> & {
  size?: Size;
};

const trackSize: Record<Size, string> = {
  sm: "h-4 w-7",
  md: "h-5 w-9",
  lg: "h-6 w-11",
};

const thumbSize: Record<Size, string> = {
  sm: "h-3 w-3 translate-x-0.5 data-[state=checked]:translate-x-3.5",
  md: "h-4 w-4 translate-x-0.5 data-[state=checked]:translate-x-4",
  lg: "h-5 w-5 translate-x-0.5 data-[state=checked]:translate-x-5",
};

export const Toggle = forwardRef<
  React.ElementRef<typeof Switch.Root>,
  Props
>(function Toggle({ size = "md", className, ...rest }, ref) {
  return (
    <Switch.Root
      ref={ref}
      className={cn(
        "inline-flex shrink-0 items-center rounded-full bg-fill-secondary transition-colors disabled:opacity-50 disabled:cursor-not-allowed data-[state=checked]:bg-accent",
        trackSize[size],
        focusClasses,
        className,
      )}
      {...rest}
    >
      <Switch.Thumb
        className={cn(
          "block rounded-full bg-white transition-transform shadow-sm",
          thumbSize[size],
        )}
      />
    </Switch.Root>
  );
});
```

- [ ] **Step 4: Run tests to confirm they pass**

```bash
npm run test:run -- Toggle
```

Expected: 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/components/ui/Toggle.tsx src/components/ui/__tests__/Toggle.test.tsx
git commit -m "feat(ui): add Toggle primitive (Radix-based) with sizes"
```

---

## Task 10: Barrel export

**Files:**
- Create: `src/components/ui/index.ts`

- [ ] **Step 1: Write `src/components/ui/index.ts`**

```ts
export { Button } from "./Button";
export { Input } from "./Input";
export { Textarea } from "./Textarea";
export { Select } from "./Select";
export { Checkbox } from "./Checkbox";
export { Toggle } from "./Toggle";
```

- [ ] **Step 2: Run full test suite**

```bash
cd /Users/nexus/projects/meister && npm run test:run
```

Expected: 18 (existing) + 24 (new primitives) = 42 tests pass.

- [ ] **Step 3: Commit**

```bash
git add src/components/ui/index.ts
git commit -m "feat(ui): add barrel export for ui primitives"
```

---

## Task 11: Migrate Sidebar

**Files:**
- Modify: `src/components/Sidebar.tsx`

- [ ] **Step 1: Read `src/components/Sidebar.tsx` fully and apply the migration**

For every `<button class="…">` in the file, replace with `<Button variant="…">` per this mapping:

- `+ New project` button (uses `bg-accent text-accent-label`) → `<Button variant="primary" size="md" onClick={...}>+ New project</Button>`
- Confirm button (✓ on accent fill) → `<Button variant="primary" size="sm" onClick={...}>✓</Button>`
- Cancel button (`bg-fill-tertiary` or similar) → `<Button variant="ghost" size="sm" onClick={...}>✕</Button>`
- Project / recording rows (currently styled `<button>` elements with hover states) — these are navigation rows, NOT buttons in the primitive sense. Keep them as `<button>` elements (no migration needed). The mapping table is for actual buttons (CTAs and form actions), not rows that happen to be `<button>`.
- Project-name input (when adding a new project) → `<Input value={...} onChange={...} placeholder="Project name" />`

Add at the top of `Sidebar.tsx`:

```tsx
import { Button, Input } from "./ui";
```

(`./ui` is the barrel from Task 10.)

If any existing className on a button is non-trivial (positioning, spacing), keep that as `className` prop on the new `Button` component — `Button` merges via `cn` so additional classes work.

- [ ] **Step 2: Build**

```bash
npm run build
```

Expected: succeeds.

- [ ] **Step 3: Smoke check**

Run `npm run tauri dev` briefly. Confirm app boots, sidebar still functional (clicking + New project shows input, confirming creates project, etc.).

- [ ] **Step 4: Commit**

```bash
git add src/components/Sidebar.tsx
git commit -m "refactor(ui): migrate Sidebar to ui primitives"
```

---

## Task 12: Migrate ProjectView

**Files:**
- Modify: `src/components/ProjectView.tsx`

- [ ] **Step 1: Read `src/components/ProjectView.tsx` and apply the migration**

Mapping:
- Delete button (`text-status-error` or destructive styling) → `<Button variant="destructive" size="sm">Delete</Button>` (or `ghost` if it's a low-emphasis text link).
- Format toggle pill row → each pill becomes `<Checkbox label="…">` per format. Visual change: pills become checkboxes-with-labels.

Replace the format toggle row markup (currently a row of styled `<button>` elements) with:

```tsx
<div className="flex items-center gap-4">
  {formats.map((fmt) => (
    <label key={fmt} className="flex items-center gap-2 text-body text-label-primary cursor-pointer">
      <Checkbox
        checked={selectedFormats.includes(fmt)}
        onCheckedChange={(c) => toggleFormat(fmt, Boolean(c))}
      />
      {fmt.toUpperCase()}
    </label>
  ))}
</div>
```

(Adjust `formats`, `selectedFormats`, `toggleFormat` to match the actual variable names in ProjectView.)

Add:

```tsx
import { Button, Checkbox } from "./ui";
```

- [ ] **Step 2: Build**

```bash
npm run build
```

- [ ] **Step 3: Smoke check**

Run dev briefly. Confirm format checkboxes toggle correctly; deleting a project still works.

- [ ] **Step 4: Commit**

```bash
git add src/components/ProjectView.tsx
git commit -m "refactor(ui): migrate ProjectView to ui primitives"
```

---

## Task 13: Migrate RecordingView

**Files:**
- Modify: `src/components/RecordingView.tsx`

- [ ] **Step 1: Read `src/components/RecordingView.tsx` and apply migration**

Mapping:
- Run button (`bg-accent`) → `<Button variant="primary" size="lg">Run</Button>`
- Start over button (text link / ghost) → `<Button variant="ghost" size="md">Start over</Button>`
- Source-list row buttons (these are navigation rows; keep as `<button>` if they don't look like CTAs, or use `<Button variant="ghost">` if they do).

Add:

```tsx
import { Button } from "./ui";
```

- [ ] **Step 2: Build + smoke**

```bash
npm run build
```

Run dev briefly; confirm Run / Start over still functional.

- [ ] **Step 3: Commit**

```bash
git add src/components/RecordingView.tsx
git commit -m "refactor(ui): migrate RecordingView to ui primitives"
```

---

## Task 14: Migrate RecordCard

**Files:**
- Modify: `src/components/RecordCard.tsx`

- [ ] **Step 1: Read and migrate**

Mapping:
- Stop button (`bg-status-error` or `bg-accent` while recording) → `<Button variant="destructive" size="md">Stop</Button>` (or `primary` if Stop visually represents "complete the recording" semantically — pick by what the original used).
- Pause button → `<Button variant="secondary" size="md">Pause</Button>`
- Resume button → `<Button variant="primary" size="md">Resume</Button>`
- Discard button → `<Button variant="destructive" size="md">Discard</Button>`
- Save button → `<Button variant="primary" size="md">Save</Button>`

Add:

```tsx
import { Button } from "./ui";
```

- [ ] **Step 2: Build + smoke**

- [ ] **Step 3: Commit**

```bash
git add src/components/RecordCard.tsx
git commit -m "refactor(ui): migrate RecordCard to ui primitives"
```

---

## Task 15: Migrate RecordCallCard

**Files:**
- Modify: `src/components/RecordCallCard.tsx`

- [ ] **Step 1: Read and migrate**

Mapping:
- Same set of recording-state buttons as RecordCard (Stop / Pause / Resume / Discard / Save) — use the same variants.
- Pre-flight checkbox (currently a styled `<input type="checkbox">`) → `<Checkbox checked={...} onCheckedChange={...} />` with a `<label>` wrapper.

Add:

```tsx
import { Button, Checkbox } from "./ui";
```

- [ ] **Step 2: Build + smoke**

- [ ] **Step 3: Commit**

```bash
git add src/components/RecordCallCard.tsx
git commit -m "refactor(ui): migrate RecordCallCard to ui primitives"
```

---

## Task 16: Migrate StagingList

**Files:**
- Modify: `src/components/StagingList.tsx`

- [ ] **Step 1: Read and migrate**

Mapping:
- Cancel button → `<Button variant="ghost" size="sm">Cancel</Button>`
- Create button (`bg-accent`) → `<Button variant="primary" size="sm">Create</Button>`
- Recording-name input → `<Input value={...} onChange={...} placeholder="Recording name" size="sm" />`
- Drag-handle button (icon-only) — keep as `<button>` or migrate to `<Button variant="ghost" size="sm">`.
- Remove button (`text-status-error` on hover) → `<Button variant="ghost" size="sm" className="hover:text-status-error">✕</Button>`.

Add:

```tsx
import { Button, Input } from "./ui";
```

- [ ] **Step 2: Build + smoke**

- [ ] **Step 3: Commit**

```bash
git add src/components/StagingList.tsx
git commit -m "refactor(ui): migrate StagingList to ui primitives"
```

---

## Task 17: Migrate DownloadModelModal

**Files:**
- Modify: `src/components/DownloadModelModal.tsx`

- [ ] **Step 1: Read and migrate**

Mapping:
- Download button (`bg-accent`) → `<Button variant="primary" size="md">Download</Button>`
- Cancel button → `<Button variant="ghost" size="md">Cancel</Button>`
- Retry button → `<Button variant="primary" size="md">Retry</Button>`
- Close button → `<Button variant="secondary" size="md">Close</Button>`

Modal scrim, surface, and progress-bar markup stay as-is.

Add:

```tsx
import { Button } from "./ui";
```

- [ ] **Step 2: Build + smoke**

- [ ] **Step 3: Commit**

```bash
git add src/components/DownloadModelModal.tsx
git commit -m "refactor(ui): migrate DownloadModelModal to ui primitives"
```

---

## Task 18: Migrate TemplateEditor

**Files:**
- Modify: `src/components/TemplateEditor.tsx`

- [ ] **Step 1: Read and migrate**

Mapping:
- Drop-zone trigger button (text link) → `<Button variant="ghost" size="sm">…</Button>`
- Name input → `<Input value={...} onChange={...} placeholder="Template name" size="sm" />`
- Textarea → `<Textarea value={...} onChange={...} placeholder="…" />`

Add:

```tsx
import { Button, Input, Textarea } from "./ui";
```

- [ ] **Step 2: Build + smoke**

- [ ] **Step 3: Commit**

```bash
git add src/components/TemplateEditor.tsx
git commit -m "refactor(ui): migrate TemplateEditor to ui primitives"
```

---

## Task 19: Migrate SettingsForm

**Files:**
- Modify: `src/components/SettingsForm.tsx`

- [ ] **Step 1: Read and migrate**

Mapping:
- Ollama URL input → `<Input value={...} onChange={...} />`
- Ollama model name input → `<Input value={...} onChange={...} />`
- Whisper model select → `<Select value={...} onChange={...}>{options}</Select>`
- Language select (if present) → `<Select>`
- Format toggle row (currently styled `<button>` pills) → row of `<Checkbox label="…">` per format, same pattern as ProjectView Task 12.

Add:

```tsx
import { Button, Input, Select, Checkbox } from "./ui";
```

- [ ] **Step 2: Build + smoke**

- [ ] **Step 3: Commit**

```bash
git add src/components/SettingsForm.tsx
git commit -m "refactor(ui): migrate SettingsForm to ui primitives"
```

---

## Task 20: Delete legacy `.input` utility class

**Files:**
- Modify: `src/index.css`

- [ ] **Step 1: Confirm no remaining usage**

```bash
grep -rE 'className=.*\binput\b' /Users/nexus/projects/meister/src/components 2>/dev/null
```

Expected: empty. If matches exist, those files weren't fully migrated — go back and fix.

- [ ] **Step 2: Delete the `.input` block from `src/index.css`**

Find the block:

```css
/* Form inputs */
.input {
  @apply w-full bg-control-background border border-separator rounded-lg px-3 py-2 text-body text-label-primary outline-none
         focus:ring-1 focus:ring-focus-ring focus:border-accent transition-colors;
}

select.input option {
  background: rgb(var(--color-control-background-rgb));
}
```

Delete the entire block (both `.input` and `select.input option`).

- [ ] **Step 3: Build**

```bash
npm run build
```

Expected: succeeds.

- [ ] **Step 4: Run full test suite**

```bash
npm run test:run
```

Expected: 42 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/index.css
git commit -m "refactor(ui): drop legacy .input utility class"
```

---

## Task 21: Manual QA + tag

**Files:** none (verification only).

- [ ] **Step 1: Functional QA**

Run `npm run tauri dev`. Walk through:
- Click each variant of `Button` in the running app — primary (Run, Save), secondary (Toggle pills if any remain), destructive (Discard, Delete), ghost (Cancel, Start over). Verify hover and active states feel native.
- Try every input: Sidebar's project-name input, StagingList's recording-name input, SettingsForm's Ollama URL / model fields, TemplateEditor's template body textarea.
- Toggle every checkbox: ProjectView format checkboxes, SettingsForm format checkboxes, RecordCallCard pre-flight checkbox.
- Open a select: SettingsForm's Whisper model and language dropdowns.

- [ ] **Step 2: Keyboard QA**

Tab through a screen with multiple controls. Confirm:
- Focus rings appear ONLY when navigating via Tab/arrow keys, NOT when clicking with mouse.
- Space toggles checkboxes and toggles.
- Enter activates focused buttons.
- Tab order is sensible.

- [ ] **Step 3: Accessibility spot-check**

In a focused state, verify:
- Buttons have a clear focus ring against the island background.
- Inputs and selects have a visible focus ring + accent border.
- Disabled controls show 50% opacity and don't accept clicks.

- [ ] **Step 4: Cross-app comparison**

Open Mail.app or System Settings. Compare focus ring appearance and timing. Should feel comparable in width, color, and `:focus-visible`-only behavior.

- [ ] **Step 5: Tag**

```bash
git tag native-feel/03a-controls-foundations-complete
```

Sub-project #3a is done. Next: brainstorm sub-project #3b (composite primitives — `SegmentedControl`, `ContextMenu`, `RadioGroup`, `Tooltip`, `Slider`).
