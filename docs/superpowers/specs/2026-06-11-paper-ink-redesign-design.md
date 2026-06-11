# Paper & Ink Redesign — Editorial UI with Switchable Colorways

**Date:** 2026-06-11
**Status:** Approved (user selected Paper & Ink direction + all four colorways as settings-toggleable themes; session-wide go-ahead)

## Decision history

- User rejected the native-macOS "Liquid Glass" look ("doesn't look very good") — pivot to *looks good* over *native-correct*.
- From three mockup directions, user chose **Paper & Ink** (editorial: serif display headings, soft rounded cards, pill buttons, generous whitespace) with an **alternate colorscheme**.
- From four colorway mockups, user chose **all four, toggleable in Settings**:

| Theme key | Name | Mode | Accent | Surfaces |
|---|---|---|---|---|
| `ink-indigo` | Ink & Indigo (default) | dark | periwinkle `#8b93f8` | night-blue charcoal `#191a21` |
| `forest-brass` | Forest & Brass | dark | brass `#b8a058` | green charcoal `#181d1a` |
| `plum-rose` | Plum & Rose | dark | dusty rose `#d4899c` | aubergine `#1d181c` |
| `manuscript` | Manuscript | light | forest green `#1f5e4e` | cream `#f5f1e8` |

Mockups preserved at `.superpowers/brainstorm/62423-1781178313/content/` (visual-direction.html, paper-ink-colorways.html) — implementers consult these for exact palette values.

## Architecture: themes as token overrides

Themes piggyback on the existing CSS-custom-property system. A `data-theme="<key>"`
attribute on `<html>` overrides the *existing semantic tokens* (`--color-window-background-rgb`,
`--color-sidebar-background-rgb`, `--color-control-background-rgb`, `--color-fill-*`,
`--color-label-*`, `--color-separator`, `--color-accent-*`, `--color-status-*`, sidebar row vars)
plus a small set of new tokens. Components that already consume semantic Tailwind colors
re-skin automatically; structure work is concentrated in a handful of components.

Token mapping from mockups (per theme: bg→window-background, side→sidebar-background,
card→control-background, well→fill-tertiary, line/cardline→separator, text/text2/muted→
label-primary/secondary/tertiary, accent→accent, on-accent→accent-label, ok→status-success).

New tokens: `--font-display` (serif stack: `Georgia, 'Iowan Old Style', 'Times New Roman', serif`),
per-theme `--shadow-card`.

### Interactions with existing systems

- **macOS accent sync (`src/lib/accent.ts`):** disabled. Themes own their accent; the boot-time
  inline-style override would beat theme attribute selectors. The accent module's apply call is
  removed (file kept; export becomes no-op or call site dropped).
- **Appearance (`data-appearance`):** kept as a *derived* attribute — theme manuscript ⇒ `light`,
  all others ⇒ `dark` — so existing light/dark token blocks and any code reading it stay coherent.
  The macOS appearance observer no longer drives the UI.
- **Native glass / vibrancy:** retired from the visual design. Paper & Ink surfaces are opaque.
  Frontend stops calling `report_sidebar_frame` and disables glass via the existing
  `set_sidebar_glass_enabled_cmd(false)` at boot; Rust plumbing stays (no src-tauri changes).
  `body` gets opaque themed background, so the transparent-window stack is invisible.
- **Reduce transparency:** trivially satisfied (everything opaque); attribute kept harmless.
- **Persistence:** theme key stored in `localStorage` (`meister-theme`) — NOT in `GlobalSettings`
  (that's a Rust struct; src-tauri is off-limits this session). Synchronous read at boot in
  `main.tsx` before first paint (no flash); `storage` event listener syncs a theme change made in
  the Settings window to the main window live. Default `ink-indigo`.

## Visual language (applies to every view)

1. **Shell:** edge-to-edge (drop the `p-2 gap-2` floating-island insets). Sidebar flush left with
   `border-r`, opaque `--side` surface; content area opaque `--bg`. Traffic-light clearance preserved.
2. **Type:** view titles use `font-display` serif (e.g. 22–24px, weight 600); meta lines italic muted;
   section labels stay uppercase tracking-wide but muted.
3. **Cards:** opaque `--card` surface, hairline `--cardline` border, `rounded-card` (12px), soft
   `--shadow-card`. No glass anywhere (ContextMenu + modal become opaque card surfaces with shadow).
4. **Buttons:** pill (`rounded-full`). Primary = solid accent with `--on-accent` text; secondary =
   1.5px outlined, transparent fill; destructive = status-error treatment. Press scale stays.
5. **Sidebar rows:** selected = full accent pill with on-accent text; hover = subtle well fill;
   recording rows show a small waveform glyph (existing icon ok).
6. **Inputs:** `--well` fill, hairline border, `rounded-control`; focus ring in accent.

## Structural changes (beyond theming)

### ProjectView (the screenshotted screen — biggest changes)
- **Header:** serif title + meta line ("N recordings · template configured/none · updated …").
  "Delete project" moves out of the header into a quiet overflow position (kept as a small
  destructive text button at the bottom of the view or behind a toolbar menu — implementer's
  judgment, but not a screaming red link top-right).
- **Template section → collapsible summary card.** Collapsed by default when a template exists
  (one-line summary: first template line + flags); expanded automatically when empty/unconfigured.
  Chevron toggles. Contents when expanded = existing TemplateEditor fields unchanged.
- **Recordings section:** rich rows (existing recordings of the project): waveform glyph, name,
  status line (✓ Transcribed · formatted / ⟳ stage — progress / error), duration-or-date chip.
  Clicking a row opens the recording (same navigation as sidebar). Empty state: one friendly line.
- **Actions:** the three giant dashed wells are replaced by a single row of pill buttons:
  `● Record call` (primary), `Record audio`, `Import files` (secondary outlines). Drag-and-drop
  onto the view still works (drop target = whole recordings section, highlight on drag-over);
  the RecordCallCard/RecordCard expanded recording UIs (while recording) remain, restyled.

### RecordingView
- Serif title, themed stage cards (token-automatic), output preview well on `--card`, status
  colors from tokens. No structural change beyond theming + header treatment.

### Settings
- New **Theme** picker at top: 4 swatch buttons (theme color dot + name) bound to global settings.
- Rest of the form re-skins via tokens.

## Out of scope

- No Rust/src-tauri changes (uncommitted capture diagnostics in the tree must stay untouched).
- No keyboard/menu/drag-drop feature work (former Phases 2–3 — still future).
- Recording duration metadata: if not already stored, the chip shows the recording date instead
  (no new backend work).

## Testing

- Existing suite (151) stays green; class-pinned assertions updated alongside changes.
- New tests: theme module (apply/persist/derive appearance), ProjectView template collapse logic,
  recordings-section rendering states.
- Final: `npm test`, `tsc --noEmit`, `npm run build`, `cargo check` (untouched), plus user's
  visual pass per theme.
