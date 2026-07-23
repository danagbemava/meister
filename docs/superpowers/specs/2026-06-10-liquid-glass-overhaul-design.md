# Liquid Glass UI Overhaul — Phase 1: Glass Shell & Control Polish

**Date:** 2026-06-10
**Status:** Approved (user delegated design decisions for this session)

## Context & decisions already made

Meister already has: HIG type scale, semantic light/dark tokens, runtime accent
sync, reduce-transparency support, a Radix-based control library, window
vibrancy, and an `NSGlassEffectView` (real Liquid Glass on macOS 26+) framed
behind the sidebar with an `NSVisualEffectView` fallback.

Decisions from brainstorming:

- **Rendering approach — Hybrid (A):** native glass (`NSGlassEffectView`) for
  window-chrome surfaces that should refract the desktop (sidebar); CSS
  `backdrop-filter` glass for surfaces that float over in-app content
  (popovers, menus, scroll-edge toolbars). Native glass views sit behind the
  webview and cannot layer between webview-rendered elements, so this split is
  structural, not stylistic.
- **Shell direction — Floating islands (A):** stock-Tahoe composition. Sidebar
  floats as a glass island on the wallpaper; content lives in a calm, opaque
  rounded panel; the toolbar fuses into the content panel and gains a
  scroll-edge glass effect when content scrolls beneath it.
- **OS floor:** design for Tahoe (macOS 26+). The pre-Tahoe vibrancy fallback
  must keep working but gets no design investment.
- **Decomposition:** the full request ("UI overhaul + native macOS
  interactions") is three phases. This spec covers Phase 1 only.
  - **Phase 1 (this spec):** Liquid Glass visuals + pointer/hover/pressed
    polish — everything you *see and feel* with the mouse.
  - **Phase 2:** keyboard & menus — complete menu bar, shortcuts, arrow-key
    list navigation, Esc/Return dialog semantics.
  - **Phase 3:** drag & drop, inline rename, sheet-style dialogs, window/system
    behaviors.

## Goals

1. The app reads instantly as a native Tahoe app: glass sidebar, scroll-edge
   toolbar, concentric corner radii, calm opaque content.
2. Controls feel like AppKit: arrow cursor, tight hover/pressed states,
   transform-only animations, glass overlay materials for floating UI.
3. Zero regressions for reduce-transparency users and pre-Tahoe fallback.

## Non-goals (Phase 1)

- No keyboard/menu work, no drag & drop, no rename, no sheets (Phases 2–3).
- No information-architecture changes — views keep their layout and feature
  set; this is a reskin + interaction-feel pass.
- No new native (Rust/Obj-C) surfaces beyond what exists. The sidebar glass
  plumbing (`glass.rs`, `report_sidebar_frame`) is reused as-is.

## Design

### 1. Glass material tokens (`src/styles/tokens.css` + `tailwind.config.js`)

New token group, light + dark variants, with `[data-reduce-transparency]`
opaque overrides:

```css
/* light */
--glass-overlay-bg:        rgba(255 255 255 / 0.55);  /* popovers/menus: heavy blur material */
--glass-overlay-border:    rgba(255 255 255 / 0.35);  /* hairline */
--glass-overlay-highlight: rgba(255 255 255 / 0.50);  /* inset top highlight */
--glass-edge-bg:           rgba(242 242 247 / 0.60);  /* scroll-edge toolbar wash */
--shadow-overlay: 0 10px 38px rgba(0 0 0 / 0.22), 0 1px 3px rgba(0 0 0 / 0.12);

/* dark */
--glass-overlay-bg:        rgba(40 40 44 / 0.55);
--glass-overlay-border:    rgba(255 255 255 / 0.14);
--glass-overlay-highlight: rgba(255 255 255 / 0.08);
--glass-edge-bg:           rgba(30 30 30 / 0.55);
--shadow-overlay: 0 10px 38px rgba(0 0 0 / 0.50), 0 1px 3px rgba(0 0 0 / 0.30);
```

(Starting values — tuned visually during implementation, kept as tokens so
tuning is one-file.)

Two utility classes in `tokens.css`:

- `.glass-overlay` — `backdrop-filter: blur(28px) saturate(170%)` + overlay
  bg/border/highlight/shadow. For ContextMenu, Select dropdown, popovers.
- `.glass-edge` — `backdrop-filter: blur(18px) saturate(160%)` + edge wash.
  For scroll-edge toolbars. Both collapse to opaque `control-background` under
  reduce-transparency (backdrop-filter removed).

Concentric radius scale as Tailwind theme tokens:

| Token | px | Use |
|---|---|---|
| `rounded-window` | 12 | content panel, sidebar island (matches native `setCornerRadius: 12`) |
| `rounded-card` | 10 | cards, list containers, modals |
| `rounded-control` | 7 | buttons, inputs, selects, segmented controls |
| `rounded-field` | 5 | checkboxes, small inline controls |

### 2. Shell (`App.tsx`, `Sidebar.tsx`)

- Keep the existing inset layout (`p-2 gap-2`); the native sidebar glass
  island stays exactly as plumbed.
- Sidebar container (webview side): add a hairline `border-separator` border
  and a soft drop shadow so the glass island reads as elevated; ensure the
  container is fully transparent so the native glass shows through (verify no
  leftover background fills).
- Content panel: stays `bg-control-background rounded-xl` → migrate to
  `rounded-window`; add hairline border + soft shadow to match the sidebar's
  elevation language.

### 3. Scroll-edge toolbar (`ui/Toolbar.tsx` + view integration)

`Toolbar` becomes scroll-aware:

- Rendered sticky (`sticky top-0 z-10`) inside each view's scroll container.
- Transparent (no border, no bg) when the container is scrolled to top.
- When content scrolls beneath it: gains `.glass-edge` material + hairline
  bottom border, via a `data-scrolled` attribute.
- Implementation: a small `useScrollEdge(ref)` hook (scroll listener, passive,
  toggles state at `scrollTop > 0`). Views pass their scroll container ref —
  or simpler, `Toolbar` finds its nearest scrollable parent on mount.
  Decision: explicit prop `scrollRef` to avoid magic; views already own their
  scroll containers.
- Consumers: ProjectView, RecordingView, SettingsWindow, TemplateEditor (all
  current `Toolbar` users + any view with its own header row).

### 4. Control polish (all of `src/components/ui/`)

Cross-cutting, in `_shared.ts` where possible:

- **Cursor:** AppKit uses the arrow cursor on buttons and controls — ensure no
  `cursor-pointer` anywhere in the control library (links/text-affordances
  excepted). Add `select-none` on control labels.
- **Pressed micro-interaction:** buttons, segmented items, toggles get
  `active:scale-[0.97] transition-[transform,background-color,color]
  duration-100` — transform+color only, GPU-friendly.
- **Primary button glass cue:** inset top highlight
  (`shadow-[inset_0_1px_0_0_rgba(255,255,255,0.18)]`) over the accent fill,
  HIG-style; pressed state drops the highlight.
- **Radius migration:** `rounded-md` → `rounded-control` across Button, Input,
  Select, Textarea, SegmentedControl; Checkbox → `rounded-field`.
- **Overlay materials:** ContextMenu content (and any future popover) gets
  `.glass-overlay` + `rounded-card` + `--shadow-overlay`. Item hover uses the
  accent-fill capsule look (already partially present in ContextMenu). The
  `Select` control is a native `<select>` — its dropdown is OS-rendered and
  already native; only the closed control gets the radius/cursor pass.
- **Focus rings:** unchanged (already HIG-correct, keyboard-only).

### 5. Cards & content surfaces

Per Direction A, in-content cards stay **calm fills, not heavy glass** (long
transcripts must stay legible):

- StageCard, RecordCard, RecordCallCard, StagingList rows, AskPanel, audio
  player container: normalize to one card recipe — `bg-fill-quaternary` +
  hairline `border-separator` + `rounded-card`; hover elevates to
  `bg-fill-tertiary` where the card is interactive.
- DownloadModelModal (and any fixed-position overlay): `.glass-overlay` +
  `rounded-card` + `--shadow-overlay`, replacing flat scrim-only styling.

### 6. Scrollbars (`index.css`)

macOS overlay-style: `::-webkit-scrollbar` width 8px on hover-capable areas,
transparent track, `fill-primary` rounded thumb visible only while
hovering/scrolling the container (`:hover` opacity transition). Applied
globally; WKWebView supports the webkit pseudo-elements.

### 7. Reduce-transparency & fallback behavior

- `[data-reduce-transparency="true"]` removes every `backdrop-filter` and
  swaps glass tokens to opaque equivalents (existing pattern, extended to the
  new tokens).
- Pre-Tahoe: native side already falls back to `NSVisualEffectView`; CSS glass
  works identically (backdrop-filter is OS-version-independent). No extra work.

## Component inventory (touched files)

- `src/styles/tokens.css`, `tailwind.config.js`, `src/index.css`
- `src/components/ui/`: `_shared.ts`, Button, Input, Select, Textarea,
  Checkbox, Toggle, SegmentedControl, ContextMenu, Toolbar (+ their tests)
- `src/App.tsx`, `Sidebar.tsx`
- `src/components/`: ProjectView, RecordingView, StageCard, RecordCard,
  RecordCallCard, StagingList, RecordingDropZone, AskPanel, OutputPreview,
  DownloadModelModal, SettingsWindow, SettingsForm, TemplateEditor
- New: `src/lib/useScrollEdge.ts` (+ test)

## Testing

- Existing ui control vitest suites updated where class assertions change
  (radius/cursor/scale classes); new assertions for `data-scrolled` toolbar
  behavior and `useScrollEdge` hook (jsdom scroll events).
- Manual verification checklist per view: glass sidebar intact, scroll-edge
  toolbar engages, reduce-transparency renders opaque, dark + light, accent
  colors other than blue.
- `cargo test` + full `npm test` green; no Rust changes expected in Phase 1.

## Risks

- **backdrop-filter cost in WKWebView:** blur on large sticky toolbars is
  GPU-composited and cheap; overlay menus are small. If scrolling jank
  appears, reduce blur radius before reaching for `will-change`.
- **Class-assertion test churn:** radius/cursor changes touch many test files;
  mechanical but must be kept in lockstep.
- **Sticky toolbar inside existing layouts:** some views wrap content in
  nested scroll areas; each consumer needs its real scroll container
  identified during implementation.
