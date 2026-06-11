# Paper & Ink Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Liquid Glass look with the Paper & Ink editorial design — serif display headings, soft opaque cards, pill buttons — in four user-switchable colorways (Ink & Indigo default, Forest & Brass, Plum & Rose, Manuscript light), per `docs/superpowers/specs/2026-06-11-paper-ink-redesign-design.md`.

**Architecture:** Themes are `[data-theme="<key>"]` CSS-variable override blocks on top of the existing semantic token system — components consuming semantic Tailwind colors re-skin automatically. A small `theme.ts` module owns apply/persist (localStorage) and derives `data-appearance`. Native glass/vibrancy and macOS accent sync are retired. Structure work concentrates in ProjectView.

**Tech Stack:** React 18, Tailwind 3, vitest. **No src-tauri changes** (uncommitted diagnostics there must stay untouched).

**Reference mockups:** `.superpowers/brainstorm/62423-1781178313/content/paper-ink-colorways.html`

---

### Task 1: Theme foundation — theme.ts + token blocks + boot wiring

**Files:**
- Create: `src/lib/theme.ts`, `src/lib/theme.test.ts`
- Modify: `src/styles/tokens.css` (append theme blocks), `tailwind.config.js`, `src/main.tsx`

- [ ] **Step 1: Failing tests** — `src/lib/theme.test.ts`:

```ts
import { beforeEach, describe, expect, it } from "vitest";
import { THEMES, applyTheme, getStoredTheme, initTheme } from "./theme";

describe("theme", () => {
  beforeEach(() => {
    localStorage.clear();
    delete document.documentElement.dataset.theme;
    delete document.documentElement.dataset.appearance;
  });

  it("exposes the four themes with ink-indigo first (default)", () => {
    expect(THEMES.map((t) => t.key)).toEqual([
      "ink-indigo", "forest-brass", "plum-rose", "manuscript",
    ]);
  });

  it("applyTheme sets data-theme, derives appearance, persists", () => {
    applyTheme("manuscript");
    expect(document.documentElement.dataset.theme).toBe("manuscript");
    expect(document.documentElement.dataset.appearance).toBe("light");
    expect(localStorage.getItem("meister-theme")).toBe("manuscript");
    applyTheme("forest-brass");
    expect(document.documentElement.dataset.appearance).toBe("dark");
  });

  it("getStoredTheme falls back to ink-indigo on missing/garbage", () => {
    expect(getStoredTheme()).toBe("ink-indigo");
    localStorage.setItem("meister-theme", "not-a-theme");
    expect(getStoredTheme()).toBe("ink-indigo");
  });

  it("initTheme applies the stored theme", () => {
    localStorage.setItem("meister-theme", "plum-rose");
    initTheme();
    expect(document.documentElement.dataset.theme).toBe("plum-rose");
    expect(document.documentElement.dataset.appearance).toBe("dark");
  });
});
```

- [ ] **Step 2: Run, see failure** — `npm test -- --run src/lib/theme.test.ts`

- [ ] **Step 3: Implement `src/lib/theme.ts`:**

```ts
export type ThemeKey = "ink-indigo" | "forest-brass" | "plum-rose" | "manuscript";

export interface ThemeInfo {
  key: ThemeKey;
  name: string;
  mode: "light" | "dark";
  /** Swatch colors for the settings picker. */
  swatch: { bg: string; accent: string };
}

export const THEMES: ThemeInfo[] = [
  { key: "ink-indigo",   name: "Ink & Indigo",   mode: "dark",  swatch: { bg: "#191a21", accent: "#8b93f8" } },
  { key: "forest-brass", name: "Forest & Brass", mode: "dark",  swatch: { bg: "#181d1a", accent: "#b8a058" } },
  { key: "plum-rose",    name: "Plum & Rose",    mode: "dark",  swatch: { bg: "#1d181c", accent: "#d4899c" } },
  { key: "manuscript",   name: "Manuscript",     mode: "light", swatch: { bg: "#f5f1e8", accent: "#1f5e4e" } },
];

const STORAGE_KEY = "meister-theme";
const DEFAULT_THEME: ThemeKey = "ink-indigo";

export function getStoredTheme(): ThemeKey {
  const v = localStorage.getItem(STORAGE_KEY);
  return THEMES.some((t) => t.key === v) ? (v as ThemeKey) : DEFAULT_THEME;
}

export function applyTheme(key: ThemeKey): void {
  const theme = THEMES.find((t) => t.key === key) ?? THEMES[0];
  document.documentElement.dataset.theme = theme.key;
  document.documentElement.dataset.appearance = theme.mode;
  localStorage.setItem(STORAGE_KEY, theme.key);
}

/** Apply the persisted theme and follow changes made in other windows. */
export function initTheme(): void {
  applyTheme(getStoredTheme());
  window.addEventListener("storage", (e) => {
    if (e.key === STORAGE_KEY) applyTheme(getStoredTheme());
  });
}
```

- [ ] **Step 4: Append the four theme blocks to `src/styles/tokens.css`:**

```css
/* ============================================================
 * Paper & Ink colorways. Each block overrides the semantic
 * tokens; components consuming them re-skin automatically.
 * Backgrounds opaque (alpha companions = 1): vibrancy retired.
 * ============================================================ */

[data-theme="ink-indigo"] {
  --color-window-background-rgb:      25 26 33;
  --color-under-page-background-rgb:  20 21 27;
  --color-control-background-rgb:     32 34 44;
  --color-sidebar-background-rgb:     20 21 27;
  --color-window-background-alpha: 1; --color-under-page-background-alpha: 1;
  --color-control-background-alpha: 1; --color-sidebar-background-alpha: 1;
  --color-label-primary:    rgba(232 233 242 / 0.95);
  --color-label-secondary:  rgba(191 194 212 / 0.8);
  --color-label-tertiary:   rgba(125 129 152 / 0.7);
  --color-label-quaternary: rgba(125 129 152 / 0.35);
  --color-text-placeholder: rgba(125 129 152 / 0.6);
  --color-separator:        rgba(43 45 58 / 1);
  --color-fill-primary:     rgba(48 51 68 / 1);
  --color-fill-secondary:   rgba(40 43 57 / 1);
  --color-fill-tertiary:    rgba(34 36 48 / 1);
  --color-fill-quaternary:  rgba(32 34 44 / 0.6);
  --color-control-text:           rgba(232 233 242 / 0.95);
  --color-control-text-disabled:  rgba(125 129 152 / 0.45);
  --color-accent-rgb:         139 147 248;
  --color-accent-hover-rgb:   165 172 255;
  --color-accent-pressed-rgb: 120 128 230;
  --color-accent-label-rgb:   20 21 27;
  --color-status-success-rgb: 125 216 125;
  --color-status-warning-rgb: 229 178 110;
  --color-status-error-rgb:   224 108 117;
  --color-status-info-rgb:    139 147 248;
  --sidebar-row-hover:         rgb(255 255 255 / 0.05);
  --sidebar-row-selected:      rgb(var(--color-accent-rgb) / 1);
  --sidebar-row-selected-text: #14151b;
  --sidebar-divider:           rgb(255 255 255 / 0.06);
  --sidebar-section-label:     rgb(125 129 152 / 0.9);
  --sidebar-search-bg:         rgb(34 36 48 / 1);
  --sidebar-search-border:     rgb(43 45 58 / 1);
  --shadow-card: 0 1px 3px rgba(0 0 0 / 0.3), 0 4px 16px rgba(0 0 0 / 0.18);
}

[data-theme="forest-brass"] {
  --color-window-background-rgb:      24 29 26;
  --color-under-page-background-rgb:  19 24 21;
  --color-control-background-rgb:     31 38 33;
  --color-sidebar-background-rgb:     19 24 21;
  --color-window-background-alpha: 1; --color-under-page-background-alpha: 1;
  --color-control-background-alpha: 1; --color-sidebar-background-alpha: 1;
  --color-label-primary:    rgba(233 239 233 / 0.95);
  --color-label-secondary:  rgba(194 205 194 / 0.8);
  --color-label-tertiary:   rgba(126 139 128 / 0.7);
  --color-label-quaternary: rgba(126 139 128 / 0.35);
  --color-text-placeholder: rgba(126 139 128 / 0.6);
  --color-separator:        rgba(42 51 44 / 1);
  --color-fill-primary:     rgba(48 60 51 / 1);
  --color-fill-secondary:   rgba(40 50 43 / 1);
  --color-fill-tertiary:    rgba(32 40 35 / 1);
  --color-fill-quaternary:  rgba(31 38 33 / 0.6);
  --color-control-text:           rgba(233 239 233 / 0.95);
  --color-control-text-disabled:  rgba(126 139 128 / 0.45);
  --color-accent-rgb:         184 160 88;
  --color-accent-hover-rgb:   212 185 106;
  --color-accent-pressed-rgb: 160 138 72;
  --color-accent-label-rgb:   19 24 21;
  --color-status-success-rgb: 134 201 139;
  --color-status-warning-rgb: 222 178 108;
  --color-status-error-rgb:   217 110 110;
  --color-status-info-rgb:    138 173 196;
  --sidebar-row-hover:         rgb(255 255 255 / 0.05);
  --sidebar-row-selected:      rgb(var(--color-accent-rgb) / 1);
  --sidebar-row-selected-text: #131815;
  --sidebar-divider:           rgb(255 255 255 / 0.06);
  --sidebar-section-label:     rgb(126 139 128 / 0.9);
  --sidebar-search-bg:         rgb(32 40 35 / 1);
  --sidebar-search-border:     rgb(42 51 44 / 1);
  --shadow-card: 0 1px 3px rgba(0 0 0 / 0.3), 0 4px 16px rgba(0 0 0 / 0.18);
}

[data-theme="plum-rose"] {
  --color-window-background-rgb:      29 24 28;
  --color-under-page-background-rgb:  23 19 22;
  --color-control-background-rgb:     37 31 36;
  --color-sidebar-background-rgb:     23 19 22;
  --color-window-background-alpha: 1; --color-under-page-background-alpha: 1;
  --color-control-background-alpha: 1; --color-sidebar-background-alpha: 1;
  --color-label-primary:    rgba(240 232 238 / 0.95);
  --color-label-secondary:  rgba(210 196 207 / 0.8);
  --color-label-tertiary:   rgba(144 128 140 / 0.7);
  --color-label-quaternary: rgba(144 128 140 / 0.35);
  --color-text-placeholder: rgba(144 128 140 / 0.6);
  --color-separator:        rgba(51 42 49 / 1);
  --color-fill-primary:     rgba(60 50 58 / 1);
  --color-fill-secondary:   rgba(50 41 48 / 1);
  --color-fill-tertiary:    rgba(39 32 38 / 1);
  --color-fill-quaternary:  rgba(37 31 36 / 0.6);
  --color-control-text:           rgba(240 232 238 / 0.95);
  --color-control-text-disabled:  rgba(144 128 140 / 0.45);
  --color-accent-rgb:         212 137 156;
  --color-accent-hover-rgb:   230 163 180;
  --color-accent-pressed-rgb: 188 117 136;
  --color-accent-label-rgb:   23 19 22;
  --color-status-success-rgb: 139 201 139;
  --color-status-warning-rgb: 226 181 116;
  --color-status-error-rgb:   224 108 117;
  --color-status-info-rgb:    164 146 220;
  --sidebar-row-hover:         rgb(255 255 255 / 0.05);
  --sidebar-row-selected:      rgb(var(--color-accent-rgb) / 1);
  --sidebar-row-selected-text: #171316;
  --sidebar-divider:           rgb(255 255 255 / 0.06);
  --sidebar-section-label:     rgb(144 128 140 / 0.9);
  --sidebar-search-bg:         rgb(39 32 38 / 1);
  --sidebar-search-border:     rgb(51 42 49 / 1);
  --shadow-card: 0 1px 3px rgba(0 0 0 / 0.3), 0 4px 16px rgba(0 0 0 / 0.18);
}

[data-theme="manuscript"] {
  --color-window-background-rgb:      245 241 232;
  --color-under-page-background-rgb:  236 231 218;
  --color-control-background-rgb:     251 248 241;
  --color-sidebar-background-rgb:     236 231 218;
  --color-window-background-alpha: 1; --color-under-page-background-alpha: 1;
  --color-control-background-alpha: 1; --color-sidebar-background-alpha: 1;
  --color-label-primary:    rgba(43 38 32 / 0.95);
  --color-label-secondary:  rgba(77 70 60 / 0.85);
  --color-label-tertiary:   rgba(141 132 116 / 0.8);
  --color-label-quaternary: rgba(141 132 116 / 0.4);
  --color-text-placeholder: rgba(141 132 116 / 0.65);
  --color-separator:        rgba(140 130 105 / 0.35);
  --color-fill-primary:     rgba(214 207 190 / 1);
  --color-fill-secondary:   rgba(226 219 201 / 1);
  --color-fill-tertiary:    rgba(239 233 220 / 1);
  --color-fill-quaternary:  rgba(243 238 228 / 1);
  --color-control-text:           rgba(43 38 32 / 0.95);
  --color-control-text-disabled:  rgba(141 132 116 / 0.5);
  --color-accent-rgb:         31 94 78;
  --color-accent-hover-rgb:   42 122 102;
  --color-accent-pressed-rgb: 24 76 63;
  --color-accent-label-rgb:   245 241 232;
  --color-status-success-rgb: 63 125 63;
  --color-status-warning-rgb: 178 121 24;
  --color-status-error-rgb:   178 60 60;
  --color-status-info-rgb:    31 94 78;
  --sidebar-row-hover:         rgb(0 0 0 / 0.05);
  --sidebar-row-selected:      rgb(var(--color-accent-rgb) / 1);
  --sidebar-row-selected-text: #f5f1e8;
  --sidebar-divider:           rgb(0 0 0 / 0.07);
  --sidebar-section-label:     rgb(141 132 116 / 1);
  --sidebar-search-bg:         rgb(239 233 220 / 1);
  --sidebar-search-border:     rgb(216 209 191 / 1);
  --shadow-card: 0 1px 2px rgba(60 50 30 / 0.10), 0 4px 14px rgba(60 50 30 / 0.08);
}
```

- [ ] **Step 5: `tailwind.config.js`** — in `theme.extend.fontFamily` add `display: ["Georgia", "Iowan Old Style", "Times New Roman", "serif"]`; in `theme.extend.boxShadow` add `card: "var(--shadow-card)"`.

- [ ] **Step 6: `src/main.tsx`** — replace `initAppearance()` and `void initAccent()` (and their imports) with `initTheme()` (`import { initTheme } from "./lib/theme";`). Keep `initReduceTransparency()`. Do NOT delete accent.ts/appearance.ts files (appearance.ts exports are used by accent tests; leave modules in place, only the boot calls go).

- [ ] **Step 7:** `npm test -- --run` all green; commit `feat(theme): Paper & Ink colorway system — four themes, localStorage persistence`.

---

### Task 2: Shell — edge-to-edge, opaque sidebar, retire glass

**Files:**
- Modify: `src/App.tsx`, `src/components/Sidebar.tsx`

- [ ] **Step 1: App.tsx** — outer div `"h-screen flex gap-2 p-2 overflow-hidden"` → `"h-screen flex overflow-hidden"`. Content panel `"flex-1 rounded-window bg-control-background overflow-hidden border border-separator shadow-island"` → `"flex-1 bg-window-background overflow-hidden"`.

- [ ] **Step 2: Sidebar.tsx** —
  - Root `<aside>`: remove `rounded-window border border-separator shadow-island`; ensure classes are `w-60 h-full shrink-0 flex flex-col bg-sidebar-background border-r border-separator overflow-hidden`.
  - Remove the `report_sidebar_frame` effect entirely (the useEffect/ResizeObserver that invokes it) and instead, once on mount, call `invoke("set_sidebar_glass_enabled_cmd", { enabled: false }).catch(() => {})` — retiring the native glass view (plumbing stays in Rust, untouched).
- [ ] **Step 3:** `npm test -- --run` green; `npx tsc --noEmit` clean; commit `feat(shell): edge-to-edge Paper & Ink shell, retire native sidebar glass`.

---

### Task 3: Controls — pill buttons, opaque overlays

**Files:**
- Modify: `src/components/ui/Button.tsx`, `src/components/ui/ContextMenu.tsx`, `src/components/DownloadModelModal.tsx`, plus their tests.

- [ ] **Step 1: Button** — base string `rounded-control` → `rounded-full`; secondary variant becomes outlined: `"bg-transparent border-[1.5px] border-separator text-label-primary hover:bg-fill-tertiary active:bg-fill-secondary"`; primary keeps solid accent but DROP the inset glass highlight (`shadow-[inset_…]` and `active:shadow-none` go — flat editorial pill). Update Button tests (radius assertion `rounded-control` → `rounded-full`; remove highlight assertion; add outlined-secondary assertion).
- [ ] **Step 2: ContextMenu** — Content `"… rounded-card glass-overlay p-1"` → `"… rounded-card bg-control-background border border-separator shadow-card p-1"`. Update its glass test to assert the new classes.
- [ ] **Step 3: DownloadModelModal** — panel `glass-overlay rounded-card` → `bg-control-background border border-separator shadow-card rounded-card`.
- [ ] **Step 4:** Confirm `.glass-overlay`/`.glass-edge` now have NO remaining consumers except Toolbar's `data-[scrolled]:glass-edge` — leave Toolbar (scroll-edge wash still works; tokens make it opaque-ish; acceptable) or swap `glass-edge` for `bg-window-background/95 backdrop-blur-sm`… **Decision: replace Toolbar's `data-[scrolled]:glass-edge` with `data-[scrolled]:bg-window-background` (opaque)** and update Toolbar/its test if pinned.
- [ ] **Step 5:** Full suite green; commit `feat(ui): editorial pill buttons + opaque overlay surfaces`.

---

### Task 4: ProjectView restructure

**Files:**
- Modify: `src/components/ProjectView.tsx`, `src/components/TemplateEditor.tsx` (wrap-only), `src/components/RecordCard.tsx` + `src/components/RecordCallCard.tsx` (idle-state removal), `src/components/RecordingDropZone.tsx` (if it's the import affordance)
- Possibly add: small helpers within ProjectView (no new files needed)

Read ProjectView fully first; it owns layout. Implement per spec:

- [ ] **Step 1: Header** — title `text-[22px] font-display font-semibold`; below it a meta line `text-callout italic text-label-tertiary`: `"{n} recording(s) · template {configured|not set} · updated {relative}"` (derive n from recordings list — see Step 3 — template state from existing template data; updated from project.updatedAt if available, else omit). Move "Delete project" out of the toolbar: render it as a quiet `variant="ghost"` small destructive-text button at the very bottom of the scroll content (`text-status-error`).
- [ ] **Step 2: Template summary card** — new collapsible card wrapping the existing TemplateEditor:
  - Card: `bg-control-background border border-separator rounded-card shadow-card`.
  - Header row (always visible, clickable, chevron rotates): label "Format template", status chip `· configured` (accent/success) or `· not set` (tertiary), summary line when collapsed (first non-empty template line truncated + flags e.g. "match style ON").
  - Collapsed by default when template content is non-empty; expanded by default when empty.
  - Expanded body renders the existing `<TemplateEditor …>` unchanged.
  - State: `const [templateOpen, setTemplateOpen] = useState<boolean | null>(null)` resolved after template loads (null → auto).
- [ ] **Step 3: Recordings section** — uppercase muted section label "Recordings"; rows from `api.listRecordings(projectId)` (already loaded in App for the sidebar — but ProjectView should fetch its own via the same API on mount + on onRecordingCreated): each row a clickable card-row `bg-control-background border border-separator rounded-card shadow-card hover:bg-fill-tertiary` with: waveform glyph (reuse the sidebar's recording icon), name (`font-semibold`), status line via `overallStatus(r)` (`✓ Transcribed · formatted` success / `⟳ {stage} — {pct}%` accent / `✗ failed` error / `new` tertiary), right-aligned chip with `new Date(r.createdAt).toLocaleDateString()` (`bg-fill-tertiary rounded-full px-2 py-0.5 text-caption-1`). Row onClick → `onSelectRecording(r.id)`. Empty state: one italic muted line "No recordings yet — record a call or drop a file below."
- [ ] **Step 4: Actions row** — replace the three giant dashed wells (RecordCard idle, RecordCallCard idle, RecordingDropZone affordance) with one row of pills: `Record call` (primary, ● prefix), `Record audio` (secondary), `Import files…` (secondary). Clicking swaps in the corresponding ACTIVE component UIs (the existing recording/in-progress states of RecordCard/RecordCallCard stay, restyled by tokens automatically). Import files opens the existing file-pick flow (reuse RecordingDropZone's openFile or the dialog logic already in App). Whole recordings section stays a drop target: keep RecordingDropZone's drag-over visuals wrapped around the section (dashed accent border on drag-over only).
- [ ] **Step 5: Tests** — ProjectView/StagingList-related tests updated; add a test for template auto-collapse (configured → collapsed; empty → expanded) and one for recordings rows rendering + click→onSelectRecording. Full suite green; tsc clean.
- [ ] **Step 6:** Commit `feat(project): editorial project view — collapsible template, rich recording rows, pill actions`.

---

### Task 5: RecordingView + remaining views theming pass

**Files:**
- Modify: `src/components/RecordingView.tsx`, `src/components/StageCard.tsx`, `src/components/OutputPreview.tsx`, `src/components/AskPanel.tsx`, `src/components/StagingList.tsx`, `src/components/SettingsWindow.tsx`

- [ ] **Step 1:** RecordingView title → `font-display text-[22px] font-semibold`; SettingsWindow `<h1>` → `font-display`. Any view h1/h2 page titles likewise (grep `text-large-title|text-title-1` in these files and swap to display treatment where it's a page title).
- [ ] **Step 2:** Cards in these views: `bg-fill-quaternary` surfaces from the old recipe → `bg-control-background … shadow-card` where they are primary content cards (StageCard outer, OutputPreview wells, sources list container); keep subtle fills for secondary/inline wells. Add `shadow-card` only on full cards, not inline chips.
- [ ] **Step 3:** Hardcoded color audit in these files: `grep -n "#[0-9a-fA-F]\{3,8\}\|rgb(" src/components/*.tsx | grep -v test` — replace stragglers with tokens (report any intentional keepers).
- [ ] **Step 4:** Full suite green; tsc clean; commit `feat(views): Paper & Ink theming across recording, settings, ask views`.

---

### Task 6: Settings theme picker

**Files:**
- Modify: `src/components/SettingsForm.tsx` (or SettingsWindow if structure fits better)
- Test: extend an existing settings test file or add `src/components/SettingsForm.test.tsx` case

- [ ] **Step 1:** New "Appearance" section at the top of settings with four swatch buttons from `THEMES`:

```tsx
import { THEMES, applyTheme, getStoredTheme, type ThemeKey } from "../lib/theme";
// in component:
const [theme, setTheme] = useState<ThemeKey>(() => getStoredTheme());
// section JSX:
<div className="flex gap-2">
  {THEMES.map((t) => (
    <button
      key={t.key}
      type="button"
      onClick={() => { applyTheme(t.key); setTheme(t.key); }}
      aria-pressed={theme === t.key}
      className={cn(
        "flex items-center gap-2 rounded-full border px-3 py-1.5 text-callout",
        theme === t.key
          ? "border-accent text-label-primary bg-fill-tertiary"
          : "border-separator text-label-secondary hover:bg-fill-quaternary",
      )}
    >
      <span
        className="inline-block h-4 w-4 rounded-full border border-separator"
        style={{ background: `linear-gradient(135deg, ${t.swatch.bg} 50%, ${t.swatch.accent} 50%)` }}
      />
      {t.name}
    </button>
  ))}
</div>
```

- [ ] **Step 2:** Test: rendering shows 4 theme buttons; clicking one calls through to applyTheme (assert `document.documentElement.dataset.theme`). Full suite green.
- [ ] **Step 3:** Commit `feat(settings): theme picker for the four Paper & Ink colorways`.

---

### Task 7: Final sweep + gates

- [ ] **Step 1:** Residual audit: `grep -rn "glass-overlay\|glass-edge\|shadow-island\|rounded-window" src --include="*.tsx"` — replace/remove stragglers (rounded-window token may stay defined but unused; remove usages). Serif check: every page title uses font-display.
- [ ] **Step 2:** Gates: `npm test -- --run`, `npx tsc --noEmit`, `npm run build`, and `cargo check --quiet` in src-tauri (must pass UNTOUCHED — verify `git status` shows only the two known capture files modified).
- [ ] **Step 3:** Commit `polish(ui): paper & ink final sweep`.

---

## Out of scope
Former Phases 2–3 (keyboard/menus, drag-drop/rename/sheets); src-tauri; recording duration metadata.
