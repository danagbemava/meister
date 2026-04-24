# Design Tokens & Light/Dark Foundation

Sub-project #1 of the "native macOS feel" overhaul. This spec covers the design-system foundation: semantic color tokens, typography scale, system-appearance tracking, and system-accent-color tracking. No visible redesigns happen here — this lays the plumbing everything else stands on.

## Context

Meister is a Tauri 2 + React transcription app. It currently renders as a polished dark-mode-only web app. Tailwind uses a handful of hardcoded tokens (`surface`, `surface-secondary`, `surface-tertiary`, `accent`, plus Tailwind's default palette). There is no light mode, no system-appearance tracking, no system-accent tracking, no typography scale, and no semantic vocabulary shared with Apple's HIG.

Subsequent sub-projects (#2 window vibrancy, #3 control library, #4 menu bar + shortcuts, #5 SF Symbols, #6 component redesign) each become considerably cheaper once this foundation is in place.

## Goals

- Replace the hardcoded Tailwind palette with a semantic token layer keyed to Apple's NSColor vocabulary (`label-primary`, `separator`, `control-background`, `fill-primary`, etc.).
- Add a complete light-mode palette designed for visual parity with native macOS apps.
- Follow the system's `prefers-color-scheme` automatically; no in-app override.
- Follow the system's accent color (Blue/Purple/Graphite/…), read from AppKit and propagated to the frontend as a CSS variable that updates live when the user changes System Settings.
- Introduce an Apple-HIG-aligned typography scale (`largeTitle`, `title1/2/3`, `headline`, `body`, `callout`, `subheadline`, `footnote`, `caption1/2`) usable via Tailwind utility classes.
- After migration, dark mode should look indistinguishable from today. Light mode is the new visible output.

## Non-goals

- No changes to component shapes, spacing, radii, shadows, or iconography (that's sub-projects #3–#6).
- No new screens or screen redesigns.
- No vibrancy / translucency work (sub-project #2).
- No menu bar or keyboard shortcut work (sub-project #4).
- No user-facing appearance or accent override UI.
- No high-contrast / Increase Contrast accessibility support this pass (future follow-up).
- macOS < 11 not supported (already Tauri's practical floor).

## Architecture

```
AppKit (Rust)  →  CSS variables (:root / [data-appearance])  →  Tailwind theme  →  component classes
```

### CSS variable layer

All tokens live in a new `src/styles/tokens.css`, imported once from `src/main.tsx`. Two blocks keyed to `[data-appearance="light"]` (the `:root` default) and `[data-appearance="dark"]`.

Tokens fall into two storage shapes:

- **Opaque tokens** (accent, status colors, pure backgrounds like `window-background`, `control-background`, `sidebar-background`) are stored as space-separated rgb triplets: `--color-accent-rgb: 10 132 255;`. Tailwind's `<alpha-value>` syntax works against them, so `bg-accent/20` resolves correctly.
- **Inherent-alpha tokens** (labels, separators, fills — any token whose catalog value carries its own alpha channel) are stored as full `rgba(…)` strings: `--color-label-primary: rgba(0 0 0 / 0.85);`. These **do not** support the `/<alpha>` modifier and are exposed to Tailwind as plain CSS-value strings. Components should not need to alpha-modify them — the alpha is part of the semantic meaning.

### Tailwind integration

`tailwind.config.js` extends `theme.colors` and `theme.fontSize` to read from the CSS variables. Opaque tokens use `<alpha-value>`; inherent-alpha tokens reference the variable directly:

```js
colors: {
  // opaque — alpha-modifier supported
  'window-background':  'rgb(var(--color-window-background-rgb) / <alpha-value>)',
  'accent':             'rgb(var(--color-accent-rgb) / <alpha-value>)',
  'status-error':       'rgb(var(--color-status-error-rgb) / <alpha-value>)',

  // inherent-alpha — alpha-modifier not supported
  'label-primary':      'var(--color-label-primary)',
  'label-secondary':    'var(--color-label-secondary)',
  'separator':          'var(--color-separator)',
  'fill-primary':       'var(--color-fill-primary)',
  /* …one entry per token… */
}
```

The existing `surface`, `surface-secondary`, `surface-tertiary` entries are **removed** — no compatibility shim. Every component is migrated in one pass within this sub-project.

### Appearance switching

`src/lib/appearance.ts` runs at boot:

1. Reads `window.matchMedia('(prefers-color-scheme: dark)')`.
2. Sets `document.documentElement.dataset.appearance = 'dark' | 'light'`.
3. Subscribes to the media query's `change` event and updates `data-appearance` live.

No timers, no polling, no React state — the DOM is the source of truth.

### System accent color

Flow:

1. A new Rust command `get_system_accent` returns `{ light: [r,g,b], dark: [r,g,b] }` triplets derived from `NSColor.controlAccentColor` resolved against each appearance.
2. A new Tauri event `system-accent-changed` is emitted when `NSDistributedNotificationCenter` fires `AppleColorPreferencesChangedNotification`, and when `NSApplication.effectiveAppearance` changes via KVO.
3. `src/lib/accent.ts` calls the command once on boot and subscribes to the event. On every update it writes `--color-accent-rgb` (and derived `--color-accent-hover-rgb`, `--color-accent-pressed-rgb`, `--color-accent-label-rgb`) onto `:root`, choosing the `light` or `dark` triplet based on the current `data-appearance`.
4. `appearance.ts` notifies `accent.ts` when `data-appearance` flips so the accent re-resolves without waiting for a system event.

`accent-label` is `#ffffff` for all standard accents except Yellow and light-mode Graphite, where it is `#000000`. The mapping is a small table in `accent.ts` keyed to the rgb triplet.

## Token catalog

Values are the starting palette; exact bytes will be tuned against native-app screenshots during implementation (see Migration plan step 7). Hex values below are shown for readability — `tokens.css` stores opaque tokens as rgb triplets and inherent-alpha tokens as full `rgba(…)` strings, per the architecture section.

### Labels (text)

| Token | NSColor source | Light | Dark |
|---|---|---|---|
| `label-primary` | `labelColor` | `#000000d9` | `#ffffffd9` |
| `label-secondary` | `secondaryLabelColor` | `#0000008c` | `#ffffff8c` |
| `label-tertiary` | `tertiaryLabelColor` | `#00000040` | `#ffffff40` |
| `label-quaternary` | `quaternaryLabelColor` | `#0000001a` | `#ffffff1a` |
| `text-placeholder` | `placeholderTextColor` | `#0000004d` | `#ffffff4d` |

### Backgrounds

| Token | NSColor source | Light | Dark |
|---|---|---|---|
| `window-background` | `windowBackgroundColor` | `#ececec` | `#1e1e1e` |
| `under-page-background` | `underPageBackgroundColor` | `#969696` | `#282828` |
| `control-background` | `controlBackgroundColor` | `#ffffff` | `#1e1e1e` |
| `sidebar-background` | custom (toolbar-like) | `#e8e8e8` | `#252527` |

### Separators & fills

| Token | NSColor source | Light | Dark |
|---|---|---|---|
| `separator` | `separatorColor` | `#0000001a` | `#ffffff1f` |
| `fill-primary` | `systemFillColor` | `#78788033` | `#7878805c` |
| `fill-secondary` | `secondarySystemFillColor` | `#78788029` | `#78788052` |
| `fill-tertiary` | `tertiarySystemFillColor` | `#7676801f` | `#76768045` |
| `fill-quaternary` | `quaternarySystemFillColor` | `#74748014` | `#7676803b` |

### Controls

| Token | NSColor source | Light | Dark |
|---|---|---|---|
| `control-text` | `controlTextColor` | `#000000d9` | `#ffffffd9` |
| `control-text-disabled` | `disabledControlTextColor` | `#00000040` | `#ffffff40` |
| `control-selected-text` | `selectedControlTextColor` | `#ffffff` | `#ffffff` |
| `focus-ring` | `keyboardFocusIndicatorColor` | derived from accent @ 50% α | same |

### Accent (system-driven)

| Token | Source | Notes |
|---|---|---|
| `accent` | `NSColor.controlAccentColor` | Pushed from Rust; changes live |
| `accent-label` | computed | `#ffffff` for most accents; `#000000` for Yellow and light-mode Graphite |
| `accent-hover` | accent + 8% lighten | |
| `accent-pressed` | accent + 12% darken | |

### Status (app-semantic, tied to Apple system colors)

| Token | NSColor source | Light | Dark |
|---|---|---|---|
| `status-success` | `systemGreen` | `#28cd41` | `#32d74b` |
| `status-warning` | `systemOrange` | `#ff9500` | `#ff9f0a` |
| `status-error` | `systemRed` | `#ff3b30` | `#ff453a` |
| `status-info` | `systemBlue` | `#007aff` | `#0a84ff` |

### Typography scale (macOS HIG)

Classes are `text-{name}`. All inherit the existing SF Pro stack from `tailwind.config.js`. Monospace (used in `OutputPreview`) keeps its current stack.

| Token | Size | Line-height | Weight |
|---|---|---|---|
| `text-large-title` | 26px / 1.625rem | 32px / 2rem | 600 |
| `text-title-1` | 22px / 1.375rem | 26px / 1.625rem | 400 |
| `text-title-2` | 17px / 1.0625rem | 22px / 1.375rem | 400 |
| `text-title-3` | 15px / 0.9375rem | 20px / 1.25rem | 400 |
| `text-headline` | 13px / 0.8125rem | 16px / 1rem | 600 |
| `text-body` | 13px / 0.8125rem | 16px / 1rem | 400 |
| `text-callout` | 12px / 0.75rem | 15px / 0.9375rem | 400 |
| `text-subheadline` | 11px / 0.6875rem | 14px / 0.875rem | 400 |
| `text-footnote` | 10px / 0.625rem | 13px / 0.8125rem | 400 |
| `text-caption-1` | 10px / 0.625rem | 13px / 0.8125rem | 400 |
| `text-caption-2` | 10px / 0.625rem | 13px / 0.8125rem | 500 |

## Migration plan

### Old → new color mapping

| Current class | New class |
|---|---|
| `bg-surface` (app background) | `bg-window-background` |
| `bg-surface-secondary` (cards) | `bg-control-background` |
| `bg-surface-secondary` (sidebar) | `bg-sidebar-background` |
| `bg-surface-tertiary` (hover, inset rows) | `bg-fill-tertiary` |
| `text-white` | `text-label-primary` |
| `text-gray-400` / `text-gray-500` | `text-label-secondary` |
| `text-gray-600` | `text-label-tertiary` |
| `border-white/10`, `border-gray-700` | `border-separator` |
| `bg-accent` | `bg-accent` (kept, now system-driven) |
| `bg-accent/10`, `bg-accent/20` | `bg-accent/10` (works via `<alpha-value>`) |
| `text-red-400`, `bg-red-500/10` | `text-status-error`, `bg-status-error/10` |
| `text-green-400`, `bg-green-500/10` | `text-status-success`, `bg-status-success/10` |
| `text-amber-400`, `bg-amber-500/10` | `text-status-warning`, `bg-status-warning/10` |

### Implementation sequence

1. **Plumbing (no component changes)**
   - Create `src/styles/tokens.css`, `src/lib/appearance.ts`, `src/lib/accent.ts`.
   - Extend `tailwind.config.js` with new tokens; keep old `surface*` tokens temporarily.
   - Import `tokens.css` in `src/main.tsx`.
   - Call `initAppearance()` and `initAccent()` from `main.tsx` before React render.
   - Verify: existing dark UI is unchanged; new classes are available but unused.

2. **Rust: accent + appearance notifications**
   - Add `get_system_accent` command returning `{ light: [r,g,b], dark: [r,g,b] }`.
   - Add `NSDistributedNotificationCenter` observer for `AppleColorPreferencesChangedNotification`.
   - Add `effectiveAppearance` KVO on `NSApplication.sharedApplication`.
   - Emit `system-accent-changed` when either fires.
   - Non-macOS build targets compile with a stub command returning hardcoded Apple blue.
   - Verify: change accent in System Settings → Meister's accent updates within ~1s without relaunch.

3. **Frontend appearance + accent wiring**
   - `appearance.ts` sets `data-appearance` on `<html>` from `prefers-color-scheme`, subscribes to media-query changes.
   - `accent.ts` invokes `get_system_accent` on boot, subscribes to `system-accent-changed`, writes `--color-accent-rgb` (+derivatives) onto `:root` per current appearance.
   - Verify: toggle system dark/light → Meister flips palette live; accent re-resolves to the matching light/dark triplet.

4. **Component migration** (one commit per group)

   Group A — chrome:
   - `TitleBar.tsx`
   - `Sidebar.tsx`

   Group B — top-level views:
   - `ProjectView.tsx`
   - `RecordingView.tsx`

   Group C — recording/pipeline cards:
   - `RecordCard.tsx`
   - `RecordCallCard.tsx`
   - `StageCard.tsx`

   Group D — content & staging:
   - `OutputPreview.tsx`
   - `StagingList.tsx`
   - `RecordingDropZone.tsx`

   Group E — forms & modals:
   - `SettingsForm.tsx`
   - `TemplateEditor.tsx`
   - `DownloadModelModal.tsx`

   After each group, visually compare dark mode to a pre-migration screenshot of the same screen; any visible diff is a defect.

5. **Typography migration** — replace ad-hoc `text-sm`, `text-xs`, `text-lg`, `font-semibold` with `text-body`, `text-footnote`, `text-title-3`, `text-headline`, etc., where intent is clear. Conservative; keep existing sizes that already match a scale value.

6. **Remove old tokens** from `tailwind.config.js` (`surface`, `surface-secondary`, `surface-tertiary`, and every replaced color alias). Build must fail on stale references — that's the signal we missed a spot.

7. **Light-mode tuning pass** — walk every screen in light mode against native reference apps (Mail, Messages, Notes) and tweak the light-palette values in `tokens.css` until it looks plausibly native. Tokens only — no component edits.

### Verification gates

- Before step 6: dark-mode screenshots of every screen match pre-migration screenshots.
- After step 7: light-mode screenshots of every screen are legible and visually consistent with native reference apps.
- Accent: change System Settings accent to Graphite and Purple; Meister updates within ~1s; all accent-using surfaces pick up the new color.

## Testing

### Automated

Frontend (Vitest; adding the test runner is in scope if not already present):

- `appearance.test.ts` — with mocked `matchMedia`, verifies `data-appearance` is set correctly on init and updates on change events.
- `accent.test.ts` — with mocked Tauri invoke + event, verifies `--color-accent-rgb` is written on `:root` on boot and re-written when `system-accent-changed` fires; verifies `light`/`dark` triplets route to the matching appearance.

Rust:

- Unit test for `get_system_accent` on macOS confirming it returns non-zero rgb triplets in range. Exact values are user-dependent so not asserted.
- Non-macOS target: command returns hardcoded Apple-blue fallback; tested explicitly.

No visual-regression tooling is added this pass. Screenshot comparisons are manual per §Migration plan.

### Manual QA checklist

For each of `empty state`, `ProjectView`, `RecordingView (active pipeline)`, `RecordingView (completed)`, `SettingsForm`, `DownloadModelModal`:

- Dark mode — pixel-level diff against pre-migration screenshot: zero visible change.
- Light mode — readable, no low-contrast text, no bright-card-in-dim-frame regressions, status colors distinguishable.
- System accent → Purple: blue surfaces become purple; status colors unchanged.
- System accent → Graphite: `accent-label` flips to dark text on light graphite fills (no white-on-light-gray).
- Live appearance toggle while app is open: flips within one frame, no flash of wrong palette.
- Live accent toggle while app is open: updates within ~1s.

## Error handling

| Failure | Behavior |
|---|---|
| `get_system_accent` Tauri call fails or returns malformed data | Fall back to Apple blue (`#007aff` / `#0a84ff`); log once; no retry loop |
| `system-accent-changed` observer fails to attach | Accent stays on boot value; log once; non-fatal |
| `matchMedia` unavailable (shouldn't happen on Tauri/WebKit) | Default to dark; log once |
| `tokens.css` fails to load | Caught at build time by Vite; no runtime path |
| User has Increase Contrast enabled | Out of scope; documented as future work |
| User has Reduce Transparency enabled | Not relevant until sub-project #2 |

## Out-of-scope / future work

- Increase Contrast support (elevated contrast token variants).
- Reduce Transparency integration (relevant when sub-project #2 lands vibrancy).
- A user-facing appearance/accent override in Settings.
- Visual-regression test tooling.
- Migration of radii, spacing, and shadow tokens (deferred; current values are acceptable).
