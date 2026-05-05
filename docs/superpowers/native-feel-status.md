# Native macOS Feel — Status

Running progress log of the multi-part overhaul to make Meister feel native on macOS. Each sub-project has its own spec under `docs/superpowers/specs/` and plan under `docs/superpowers/plans/`. This file tracks where we are and what remains.

## Overview

The overhaul was decomposed into 6 sub-projects, with #3 and #4 each split into smaller slices. Total: 11 tracked slices.

| Slice | Status | Tag (after merge) |
|---|---|---|
| #1 Design tokens + light/dark | shipped | `native-feel/01-tokens-complete` (untagged; merged) |
| #2 Window vibrancy + Music-style islands | shipped | `native-feel/02-vibrancy-complete` (untagged; merged) |
| #3a Control library — foundations | shipped | `native-feel/03a-controls-foundations-complete` (untagged; merged) |
| #3b Control library — composite | shipped | `native-feel/03b-controls-composite-complete` (untagged; merged) |
| #4a Chrome basics | branch `native-feel/04a-chrome` (this worktree) | not yet merged |
| #4b Notifications + Help/About | not started | |
| #4c Preferences + Undo | not started | |
| #5 SF Symbols + icon pass | not started | |
| #6 Component redesign | not started | |

## What shipped (per slice)

### #1 — Design tokens + light/dark foundation
- Semantic color tokens keyed to Apple `NSColor` vocabulary (`label-primary`, `separator`, `control-background`, `fill-primary`, etc.) in `src/styles/tokens.css`.
- Two appearance blocks (`:root, [data-appearance="light"]` and `[data-appearance="dark"]`) wired via `prefers-color-scheme` in `src/lib/appearance.ts`.
- System accent color tracked via `NSColor.controlAccentColor` (Rust `src-tauri/src/accent.rs`) → Tauri event → `src/lib/accent.ts` writes `--color-accent-rgb` (and derived hover/pressed/label) on `:root`.
- macOS HIG type scale in `tailwind.config.js` (`text-body`, `text-headline`, `text-title-1/2/3`, `text-large-title`, `text-callout`, `text-footnote`, `text-caption-1/2`).
- Tailwind colors compose tokens via `rgb(var(--color-*-rgb) / <alpha-value>)` for opaque tokens and `var(--color-*)` for inherent-alpha tokens.

### #2 — Window vibrancy + Music-style islands
- `NSVisualEffectView` with `.windowBackground` material attached to the main window (Rust `src-tauri/src/vibrancy.rs`).
- `transparent: true` on the Tauri window + `setOpaque(false)` + `clearColor`.
- Token mechanism upgraded with `*-alpha` companion variables for opaque surfaces; vibrancy-on values: window=0, sidebar=0.4, control=0.92, under-page=0.6.
- `[data-reduce-transparency="true"]` block restores all alphas to 1; observer in Rust watches `NSWorkspace.accessibilityDisplayShouldReduceTransparency`.
- `vibrancy-unavailable` Tauri event fires if `apply_vibrancy` errors at boot; frontend forces reduced-transparency mode in that case.
- Music.app full-window-island layout: TitleBar component removed; sidebar + main content as separate rounded panels with vibrancy showing in the gaps. Sidebar reserves a 36px drag-region strip at top for OS-rendered traffic lights.
- `tauri.conf.json` window title set to `""` to suppress the OS-drawn title text.

### #3a — Control library foundations
- Six primitives in `src/components/ui/`: `Button` (4 variants × 3 sizes), `Input`, `Textarea`, `Select`, `Checkbox` (Radix), `Toggle` (Radix).
- All forwardRef, all compose via `cn()` from `src/lib/cn.ts`, all share `_shared.ts` for `Size`/`sizeClasses`/`focusClasses`.
- Focus rings render only on `:focus-visible` (HIG keyboard-only behavior).
- Tailwind `aria-invalid:` variant enabled in config so `Input`/`Textarea`/`Select` invalid-state styling actually applies.
- Migration of all 9 component files away from inline-styled controls; legacy `.input` utility class deleted from `src/index.css`.
- Vitest + Testing Library + jest-dom installed; ~24 new behavioral tests.

### #3b — Control library composite
- Two more primitives: `SegmentedControl` (Radix `react-toggle-group` `type="single"`) and `ContextMenu` (Radix `react-context-menu`).
- `SegmentedControl` integrated into `OutputPreview` Formatted/Transcript tabs.
- `ContextMenu` integrated onto Sidebar project rows (Rename, Delete) and recording rows (Rename, Delete) with inline rename via Input swap-in.
- Recording row uses `<div role="button" tabIndex={0}>` to avoid `<input>`-inside-`<button>` HTML invalidity when renaming.
- ~8 new tests; total at 50.

### #4a — Chrome basics (in flight)
- `Toolbar` primitive (`src/components/ui/Toolbar.tsx`) used by `RecordingView` and `ProjectView` headers.
- Native macOS menu bar built in Rust (`src-tauri/src/menu.rs`):
  - **Meister**: About, Preferences (disabled), Hide/Show All/Quit.
  - **File**: New Project (⌘N), Import Audio… (⌘⇧I), Close Window.
  - **Edit**: Cut/Copy/Paste/Select All (predefined; OS routes to focused inputs).
  - **View**: Toggle Sidebar (⌃⌘S).
  - **Recording**: Run (⌘R).
  - **Window**: Minimize, Zoom.
  - **Help**: empty placeholder for #4b.
- Custom items emit `menu:` Tauri events; `src/lib/menuEvents.ts` dispatches to handlers App.tsx registers per render.
- Sidebar/RecordingView expose imperative refs (`SidebarHandle.beginNewProject`, `RecordingViewHandle.runPipeline`) so menu shortcuts reach into them.
- All `confirm()` calls (Sidebar delete × 2, RecordingView delete + start-over, App active-capture guard, ProjectView delete) replaced with `@tauri-apps/plugin-dialog`'s native `confirm()`.
- `~/.zshrc`-aware PATH augment at app startup (`augment_path_for_homebrew`) so production `.app` finds Homebrew binaries (ffmpeg) when launched from Finder/Dock.
- Pipeline cancellation via `RecordingCancelRegistry` keyed by recording ID; `cancel_recording` Tauri command flips a `CancelToken` checked in `ollama_generate_stream`'s loop. RecordingView swaps Run for a destructive Stop button while the pipeline is running.
- Adaptive map-pass concurrency: serial for non-trivial transcripts (avoiding default `OLLAMA_NUM_PARALLEL=1` queue contention); parallel only for ≤3 small chunks.
- Reduce-phase progress event emitted before Ollama call so UI flips from "Analyzing chunk N/N" to "Composing output…" instead of freezing on the last map event.
- ProjectView recordings list now clickable; navigates to recording.
- TemplateEditor double-bordered Textarea fixed.
- Sidebar source-list refresh: "PROJECTS" section header in small caps + ghost `+` button replacing the loud full-width "New Project" CTA. Tighter selection pills (`rounded-md`, smaller padding).
- 56 frontend tests + 107 Rust tests passing.

## Architectural patterns established

These patterns recur across slices and should inform future work:

- **Native APIs over polyfills.** When a macOS API exists (NSColor, NSVisualEffectView, NSWorkspace, native menu, native dialog), bridge to it via Rust + objc2 rather than approximate in CSS/JS.
- **Frontend dispatches via Tauri events.** Native side observes/emits; frontend listens. Modules: `appearance.ts`, `accent.ts`, `reduceTransparency.ts`, `menuEvents.ts`. All idempotent + have `__resetForTests`.
- **Imperative refs for one-off cross-component calls.** `SidebarHandle`, `RecordingViewHandle` — used by menu shortcuts to trigger handlers buried in component-local state. Keeps state lifting minimal.
- **Compound primitive components.** `<X>` + `X.Item`/`X.Trigger`/`X.Content` etc., backed by Radix UI for behavior + ours for styling. Established by `SegmentedControl` and `ContextMenu`.
- **`cn()` is concatenation, not merging.** Consumer `className` comes last; primitives put theirs first. Tailwind sorts utility classes alphabetically internally, so conflict resolution is predictable but not always intuitive — known foot-gun, surfaced in TemplateEditor's double-border bug. `tailwind-merge` deferred until a real consumer needs it.
- **Testing Library + jest-dom.** Behavioral assertions, not className regex. Vitest mocks Tauri's `invoke`/`listen` globally in `src/lib/test-setup.ts`.

## Known gaps + follow-ups

- **Inert sidebar when collapsed.** The `aria-hidden`-when-collapsed is a workaround for missing `inert` typings in our React version; revisit when `@types/react` upgrades.
- **Tailwind conflicts.** Adopt `tailwind-merge` when a real call site needs to override a primitive's class (most likely in #6).
- **Predefined Undo/Redo in Edit menu** route to WebKit's input-undo today. #4c will need to coordinate with app-level undo (rename undo, etc.).
- **`bring_all_to_front`** not in current Tauri version. Single-window app so not material; revisit if multi-window lands.
- **`buffer_unordered` heuristic** is conservative (always serial unless chunks small + ≤3). Could be configurable per project once Preferences (#4c) ships, OR auto-detect `OLLAMA_NUM_PARALLEL` from Ollama's `/api/version` if it exposes it.
- **Login-shell PATH.** Current macOS PATH augment hardcodes Homebrew prefixes. ~5% of users with non-standard binary locations (MacPorts, fink, manual `/usr/local/bin/ffmpeg-7`) won't be covered. VSCode-style `$SHELL -lc 'echo $PATH'` is the proper fix when needed.
- **Indeterminate `Checkbox` visual** still shows the checkmark icon (ARIA correct, visually wrong). Needs a `DashIcon` + `data-state` switching when a real consumer needs indeterminate state.
- **Title sizes in Toolbar** shrunk from `text-title-2` to `text-body` to fit the 44px height. A 48px Toolbar with `text-callout` titles would be more breathing room — revisit if it feels cramped.
- **`OutputPreview` empty pre block** when transcript exists but format failed — already partly fixed (auto-select populated tab + disable empty one). If both missing, component returns null.

## What's next

- **#4b — Notifications + Help/About.** Native notifications when long-running steps (Format, Save) complete. Help menu populated. About dialog with app version + GitHub link.
- **#4c — Preferences + Undo.** Global Preferences pane (⌘,) with default-inheritance for new projects. Undo/Redo plumbing for inline rename (replaces or coexists with the predefined Edit menu items currently routing to WebKit).
- **#5 — SF Symbols + icon pass.** Replace emoji status dots, document icons, etc. with SF Symbols rendered via SVG (or a font approach). Add icons to `Button` variants. This is the single biggest "this is a real Mac app" payoff still pending.
- **#6 — Component redesign pass.** Full visual polish: Source-List-style sidebar (NSOutlineView treatment), card-on-island elevation distinction, modal scrim semantics, light-mode tuning, etc.

## References

- Specs: `docs/superpowers/specs/2026-04-{24,25,27,28}-*.md`
- Plans: `docs/superpowers/plans/2026-04-{24,25,27,28}-*.md`
- Brainstorming notes live in the conversation transcripts; specs are authoritative.
