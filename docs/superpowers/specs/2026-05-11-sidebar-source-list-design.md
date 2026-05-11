# Sidebar — Source List (Tahoe Liquid Glass) — Design

Slice **#6.2** of the native-macOS-feel overhaul (`docs/superpowers/native-feel-status.md`). Replace the sidebar's hand-faked vibrancy with a real macOS Tahoe **Liquid Glass** material via `NSGlassEffectView`, restructure the layout to a spacious editorial Source List with project disclosure + nested recordings + a search field at top + auto-assigned folder colors, and lay the token surface that subsequent #6 sub-slices will inherit.

## Why

The current sidebar (slice #2) uses `NSVisualEffectView` with `.windowBackground` material and an `<aside>` whose CSS background is a translucent fill on top. Per WWDC25 session 310 ("Build an AppKit app with the new design"), Apple's explicit guidance for macOS 26 is that the legacy sidebar material does **not** auto-upgrade to Liquid Glass, and `NSVisualEffectView` *blocks* the new material when present. To get the real Tahoe look an app must either render its sidebar via `NSSplitViewController` (AppKit applies Liquid Glass automatically) or attach an `NSGlassEffectView` (public AppKit class, macOS 26+) explicitly. Today we ship neither — the sidebar reads as classic vibrancy on Tahoe, visibly behind system apps. This slice closes that gap and refreshes the layout/density to match Tahoe Source List patterns at the same time.

## Scope

**In:**
- A new Rust module `glass.rs` that bridges `NSGlassEffectView` (macOS 26+) via `objc2` `msg_send!`, with `NSVisualEffectView` (`.sidebar` material) as the pre-Tahoe fallback.
- A frame-reporting protocol between the webview and Rust so the native glass view tracks the sidebar's pixel rectangle on mount and on window resize.
- A substantial Sidebar.tsx rewrite: search field at top; spacious row density; project rows with disclosure carets and auto-assigned folder colors; recordings nested 30px under their project.
- New token surface (`tokens.css`) for sidebar rows / selection / hover / dividers / search field / folder colors. Light + dark + reduce-transparency variants.
- A folder-color helper (`folderColor.ts`) that deterministically maps a project ID to one of four hues via FNV-1a.
- Keyboard navigation across visible rows (↑/↓/Enter), focus rings, screen-reader labels.

**Out (for follow-up slices):**
- Drag-to-reorder projects/recordings.
- Pinned / favorites section.
- Recording status dots on rows (running/done/failed).
- A "Recents" cross-project view.
- User-customizable per-project colors.
- Toolbar / chrome density update — that's slice **#6.3**.
- Card/island elevation — that's slice **#6.4**.
- Broader light-mode tuning beyond the sidebar — slice **#6.8**.
- ⌘[ / ⌘] back/forward navigation — depends on slice **#4c** (Undo/history).

## Architecture

**Native side.** New module `src-tauri/src/glass.rs` exposes:

```rust
pub fn apply_sidebar_glass(window: &tauri::Window, frame_rect: NSRect) -> Result<(), String>;
pub fn update_sidebar_glass_frame(window: &tauri::Window, frame_rect: NSRect) -> Result<(), String>;
pub fn set_sidebar_glass_enabled(window: &tauri::Window, enabled: bool) -> Result<(), String>;
```

Internally:
- On macOS 26+, instantiate `NSGlassEffectView` via `objc2`'s `msg_send!`. Set `cornerRadius = 12.0`. No `tintColor` (we want the system glass, no accent overlay). Insert as a child of the window's content view at index 0 (behind everything else).
- On macOS < 26, fall back to `NSVisualEffectView` with `material = .sidebar` and `state = .active`. Same insertion point and frame behavior.
- The `set_sidebar_glass_enabled(false)` path removes the glass view; the webview falls back to a solid color via the `[data-reduce-transparency]` token block. This is wired to the existing `NSWorkspace.accessibilityDisplayShouldReduceTransparency` observer (slice #2) — when reduce-transparency flips on, we both flip the data attribute AND remove the glass view. (`NSGlassEffectView` does honor reduce-transparency on its own, but explicitly removing it makes the contract crisp and matches what `NSVisualEffectView` slice #2 already does.)
- Macros are dispatched on the main thread via the existing slice #2 dispatch helper.

**Frame-reporting protocol.** The webview's React `<Sidebar>` component reports its bounding rect to Rust on mount and on every window resize. Mechanism:

1. New Tauri command `report_sidebar_frame(x: f64, y: f64, width: f64, height: f64)`.
2. `Sidebar.tsx` measures itself with a `ResizeObserver` keyed on the sidebar element; on each callback (and once on mount), it `invoke("report_sidebar_frame", { x, y, width, height })`.
3. Coordinates are in CSS pixels, top-left origin (browser convention); Rust converts to AppKit coordinates (bottom-left origin) by inverting Y against the window's content view height before applying.
4. The first `report_sidebar_frame` call after window startup triggers `apply_sidebar_glass`. Subsequent calls trigger `update_sidebar_glass_frame`.

**Webview side.** The sidebar `<aside>` keeps its position in the React tree but its background is fully transparent. The native glass shows through. CSS owns: row chrome, selection, hover, dividers, search field, group header, folder glyphs.

**Why not `NSSplitViewController`.** Apple's other Liquid Glass path is to render the sidebar in AppKit using a split view controller. That would mean rewriting the sidebar in Swift and hosting only the main content area in the webview. Major architectural shift; abandons the all-webview approach we built #1–#5 around. Out of scope.

**Why in-house `glass.rs` rather than `tauri-plugin-liquid-glass`.** The plugin works but is a one-author dep for ~80 lines of `objc2`. Its README also incorrectly claims `NSGlassEffectView` is private (it's public per Apple's docs and `xcode-26` SDK headers). We control the fallback behavior, the frame-update flow, and the reduce-transparency wiring more precisely with our own bridge. Tradeoff: we own the maintenance.

## Tokens

Add to `src/styles/tokens.css` alongside slice #2 vars:

| Token | Dark | Light | Reduce-transparency |
|---|---|---|---|
| `--sidebar-row-hover` | `rgb(255 255 255 / 0.06)` | `rgb(0 0 0 / 0.05)` | dark `rgb(255 255 255 / 0.08)`, light `rgb(0 0 0 / 0.06)` |
| `--sidebar-row-selected` | `rgb(var(--color-accent-rgb) / 1.0)` | same | same |
| `--sidebar-row-selected-text` | `#ffffff` | `#ffffff` | same |
| `--sidebar-divider` | `rgb(255 255 255 / 0.07)` | `rgb(0 0 0 / 0.07)` | dark `rgb(255 255 255 / 0.18)`, light `rgb(0 0 0 / 0.18)` |
| `--sidebar-section-label` | `rgb(255 255 255 / 0.55)` | `rgb(60 60 67 / 0.6)` | unchanged |
| `--sidebar-search-bg` | `rgb(255 255 255 / 0.06)` | `rgb(0 0 0 / 0.04)` | dark `rgb(255 255 255 / 0.10)`, light `rgb(0 0 0 / 0.06)` |
| `--sidebar-search-border` | `rgb(255 255 255 / 0.08)` | `rgb(0 0 0 / 0.08)` | unchanged |
| `--folder-blue-from` | `#6cb3ff` | `#7fbcff` | unchanged |
| `--folder-blue-to` | `#2e89ee` | `#4f99ee` | unchanged |
| `--folder-green-from` | `#6fdb84` | `#83df93` | unchanged |
| `--folder-green-to` | `#2fb14b` | `#46b85e` | unchanged |
| `--folder-purple-from` | `#c89bf5` | `#d3aff7` | unchanged |
| `--folder-purple-to` | `#9659e3` | `#a472e6` | unchanged |
| `--folder-orange-from` | `#ffc46f` | `#ffce85` | unchanged |
| `--folder-orange-to` | `#ff8c2a` | `#ff9d4a` | unchanged |

The existing `--color-sidebar-background` token stays for the reduce-transparency fallback; in the normal path the sidebar background is `transparent`.

**Selection material.** Per HIG, the selection bar should follow the system accent. We already track `NSColor.controlAccentColor` via `accent.rs` (slice #1) into `--color-accent-rgb`. The selection token references it directly — no hand-rolled gradient. Tahoe's Liquid Glass behind the selection naturally tints the visible white text.

## Layout & structure

Sidebar regions, top to bottom:

1. **Drag-region strip — 36px.** Existing. Background fully transparent so native glass shows through. Reserves space for OS-rendered traffic lights.
2. **Search field.** Sticky under the drag region. ~32px tall. 8px corner radius. Padding 7px x 10px. Magnifying-glass icon (Phosphor `MagnifyingGlass` — needs adding to `IconName`) at left, "Search" placeholder, clear-x icon (existing `x` IconName) at right when text is non-empty. ⌘F focuses the field. Filters the list as you type: a row matches if its name contains the query (case-insensitive); an unmatched project still renders if any of its recordings match (and is auto-expanded for the duration of the filter).
3. **"Projects" section header.** UPPERCASE label "PROJECTS" + `+` icon-only Button (`iconLeft="plus"`, `aria-label="New project"`) right-aligned. Padding 8px x 16px (top), 2px (bottom). Color `var(--sidebar-section-label)`.
4. **Project rows + nested recording rows.**

Search and "Projects" header coexist — search applies to the whole list; the header stays visible. There is no "Recent" section in this slice (Q6=B in brainstorm).

**Project row:**
- Height 36px. Padding 8px x 10px x. Margin 1px x 8px. Border radius 7px.
- Layout: disclosure caret (10×10, 2px right margin) → folder glyph (14×11) → name (`text-body`, weight 500). 8px gaps.
- Disclosure caret: animated rotate-90 over 180ms ease-out on toggle. Color `text-label-tertiary`.
- Hover: row fills `var(--sidebar-row-hover)`.
- Selected: row fills `var(--sidebar-row-selected)`. Caret + folder + text become `var(--sidebar-row-selected-text)` (white).
- Click: select the project AND toggle its expansion.
- Right-click: existing ContextMenu (Rename, Delete) from slice #3b — kept as-is.

**Recording row:**
- Height 34px. Padding 8px x 10px y, padding-left 30px (the disclosure caret column is 20px wide; 10px after that to the icon).
- Layout: recording icon (`<Icon name="recording" size="sm">`) → name (`text-body`, weight 400). 8px gap.
- Same hover / selected rules as project row.
- Click: select the recording.
- Right-click: existing ContextMenu (Rename, Delete) from slice #3b — kept as-is.

**Empty state (no projects).** Below the "Projects" header, render a centered call-to-action: "No projects yet." in `text-footnote text-label-tertiary`, with a `<Button variant="primary" size="sm">New project</Button>` below. Padding 24px y. No hero card or illustration.

**Disclosure state persistence.** Open/closed state stored in component state, mirrored to `localStorage` keyed by project ID (`sidebar-expanded-{projectId}` → `"true"`/`"false"`). On mount, hydrate from localStorage. New projects default to expanded. When a project is deleted, remove its key.

**Folder colors — algorithm.**

```ts
// src/lib/folderColor.ts
const PALETTE = ["blue", "green", "purple", "orange"] as const;
export type FolderColor = typeof PALETTE[number];

export function folderColorFor(projectId: string): FolderColor {
  // FNV-1a 32-bit, mod 4. Deterministic, fast, no crypto needed.
  let h = 0x811c9dc5;
  for (let i = 0; i < projectId.length; i++) {
    h ^= projectId.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return PALETTE[Math.abs(h) % PALETTE.length];
}
```

**Folder glyph — render.**

```tsx
<span className={`sidebar-folder sidebar-folder-${folderColorFor(p.id)}`} aria-hidden />
```

CSS:

```css
.sidebar-folder {
  display: inline-block;
  width: 14px;
  height: 11px;
  border-radius: 3px;
  flex-shrink: 0;
}
.sidebar-folder-blue   { background: linear-gradient(180deg, var(--folder-blue-from),   var(--folder-blue-to)); }
.sidebar-folder-green  { background: linear-gradient(180deg, var(--folder-green-from),  var(--folder-green-to)); }
.sidebar-folder-purple { background: linear-gradient(180deg, var(--folder-purple-from), var(--folder-purple-to)); }
.sidebar-folder-orange { background: linear-gradient(180deg, var(--folder-orange-from), var(--folder-orange-to)); }
```

## Typography

| Element | Token | Weight | Notes |
|---|---|---|---|
| Section header ("PROJECTS") | `text-caption-1` (11px) | 600 | Letter-spacing 0.6px. Loosens vs slice #2 default — calibrated for Tahoe. |
| Project row name | `text-body` (13px) | 500 | |
| Recording row name | `text-body` (13px) | 400 | |
| Search placeholder | `text-callout` (12px) | 400 | Color `text-label-tertiary`. |
| Empty-state body | `text-footnote` (11px) | 400 | Color `text-label-tertiary`. |

System font (`-apple-system`, etc.) — already the project default.

## Iconography

- Add `magnifying-glass` to `IconName` (Phosphor `MagnifyingGlass`). Used by the search field.
- The folder glyph is a colored shape, not an Icon — see folder rendering above.
- Recording row icon: existing `recording` (Phosphor `Waveform`) at `size="sm"`.
- Disclosure caret: not an Icon — render directly as a chevron span with CSS rotation, since the rotate animation needs continuous control. Path data inlined as a small SVG component or even a 10×10 background-image. (Decision deferred to implementer; either works.)

## Search behavior

- Field stays sticky under the drag region.
- ⌘F focuses the field. Esc clears + blurs.
- As you type, projects + recordings are filtered:
  - A row matches if its name contains the query, case-insensitive.
  - An unmatched project still renders if at least one of its recordings matches; for the duration of the active filter, it is force-expanded (overrides the persisted disclosure state without writing to localStorage).
  - When the filter is cleared (empty input), disclosure state reverts to the localStorage-persisted values.
- The clear-x icon button appears when the field has any text. Click clears the field.
- No debouncing — list filtering is in-memory and cheap.

## Light mode

Light mode shipped in slice #1 but hasn't been audited since. For this slice:

- All sidebar tokens above have light-mode entries.
- Folder gradients are 5–10% desaturated vs dark variants so they read as accents on light glass, not splotches.
- Selection: same `var(--color-accent-rgb)` fill, white text. Confirmed legible across all four folder colors when the selected row contains a folder glyph (the white text is on the accent fill, not on the folder).
- Search field chrome: 1px inner border at `var(--sidebar-search-border)` (`rgb(0 0 0 / 0.08)` in light), fill at `var(--sidebar-search-bg)`.
- Section header: `rgb(60 60 67 / 0.6)` (system secondary label color in light).

**Implementation sanity check:** the implementer toggles `data-appearance="light"` on `<html>` (via the `appearance.ts` machinery from slice #1) during manual smoke and verifies all four folder colors + selection + hover + search render correctly against the light glass.

## Accessibility

- **Reduce transparency.** When `[data-reduce-transparency="true"]` (driven by `NSWorkspace.accessibilityDisplayShouldReduceTransparency`, slice #2 wired this up):
  - Native side: call `set_sidebar_glass_enabled(false)`. The native glass view is removed; the sidebar background falls back to the existing `--color-sidebar-background` token (opaque equivalent of slice #2's vibrancy).
  - Webview side: hover / selected use the opaque-equivalent token values from the table above.
  - Section divider becomes 1px solid `var(--sidebar-divider)` instead of translucent.
- **Keyboard navigation.** ↑/↓ moves selection across all visible rows (project + open-recording rows are flat for navigation purposes — closed projects are skipped past their hidden recordings). Enter activates the selected row. The disclosure caret is independently focusable via Tab and toggleable via Space. ⌘F focuses the search field. Esc clears the search field and returns focus to the previously selected row.
- **Focus rings.** Sidebar rows show the existing `focus-visible` ring (`focusClasses` from `_shared.ts`). Row receives focus on click; arrow-key nav moves focus + selection together.
- **Screen reader labels:**
  - Project row: announce as `"<Name>, project, <N> recordings"` via `aria-label`.
  - Recording row: announce as `"<Name>, recording"` via `aria-label`.
  - Selection: `aria-current="page"` on the selected row.
  - Disclosure caret: `aria-expanded="true|false"` + `aria-label="Show recordings in <project name>"`.
  - Search field: `<input type="search" aria-label="Search projects and recordings">`.

## Testing

**Frontend:**
- `folderColor.test.ts`: deterministic mapping for a fixture set of UUIDs; palette is exactly the four declared values; same input always returns the same output.
- `Sidebar.test.tsx` (extend existing):
  - Search filters the visible rows (typing "stand" hides projects without a matching recording).
  - Search auto-expands projects with matching recordings; clears reverts to persisted state.
  - Disclosure toggle persists to localStorage and restores on remount.
  - Folder color is applied per `folderColorFor(projectId)`.
  - Empty state renders the "New project" CTA when projects.length === 0.
  - Keyboard nav: ↓ moves selection past open recordings; ↓ on a closed project skips its hidden recordings.
  - Selected row has `aria-current="page"`.
- New mocks for `ResizeObserver` in `test-setup.ts` (jsdom doesn't ship one).

**Rust:**
- `glass.rs` unit tests are limited (it's a UI bridge and depends on a real AppKit window). Add a smoke test that constructs the fallback path's frame conversion math (browser-coords → AppKit-coords inversion) without instantiating any view.
- Manual smoke (Task 11-style checklist) covers the visual side.

**Manual smoke matrix** (run via `npm run tauri dev`):
1. Tahoe (macOS 26+): sidebar shows real Liquid Glass; visible refraction when window content moves behind it.
2. Pre-Tahoe (macOS 15 or earlier): sidebar falls back to `.sidebar` material; no Liquid Glass refraction but proper vibrancy.
3. Reduce transparency on (System Settings → Accessibility → Display): glass disappears, opaque sidebar; rows still legible.
4. Light mode: folder colors readable; selection accent legible.
5. Search: filtering works; auto-expand works; ⌘F + Esc work.
6. Keyboard nav: arrow keys, Enter, disclosure toggle via Space.
7. Resize the window: the native glass view tracks the sidebar rect (no visible offset / lag).
8. Toggle the sidebar visibility (View → Toggle Sidebar from slice #4a): the glass view is removed when sidebar is hidden, re-added on show. (Or kept and reframed to zero-width — implementer's choice; simpler is removal.)

## Risks & open implementation choices

- **`NSGlassEffectView` quirks under live frame updates.** Apple's docs don't specify whether re-framing the view every resize causes flicker. If it does, we cache the last reported rect and skip the call when the diff is < 1px.
- **First-mount race:** the webview reports the sidebar frame async (`useEffect` after first paint). Until that fires, the sidebar has no glass — readers will see a brief flash of the fallback color. Mitigation: render the sidebar with `opacity: 0` until the first `report_sidebar_frame` returns, then fade in over 120ms. Implementation detail; flag for the implementer.
- **Disclosure caret as not-an-Icon.** Decision deferred to implementer (SVG component vs CSS-rendered chevron). Either is fine.
- **Folder color palette stability.** If we ever expand the palette beyond 4 colors, the FNV mod-N output changes — existing projects would silently migrate to new colors. Mitigation: persist `folderColor` on Project the first time it's resolved; the helper consults the stored value if present, else computes + persists. Out of scope for this slice — call out as a follow-up.

## Done criteria

- All existing frontend tests + new Sidebar/folderColor tests pass; `tsc --noEmit` clean.
- All existing Rust tests + new `glass.rs` math test pass; `cargo build` clean.
- Manual smoke matrix above passes on both Tahoe and a pre-Tahoe macOS (or at least Tahoe + the fallback code path inspection if pre-Tahoe hardware is unavailable).
- Search, disclosure, folder colors, selection, hover, focus rings, and reduce-transparency all behave per spec.
- Visual sanity: side-by-side with Apple Music / Notes / Mail on Tahoe, the sidebar reads as a peer (not visibly older).

## References

- Brainstorm transcript: this conversation.
- Apple HIG — Materials & Vibrancy.
- WWDC25 session 310 — Build an AppKit app with the new design.
- WWDC25 session 323 — Build a SwiftUI app with the new design.
- `developer.apple.com/documentation/appkit/nsglasseffectview`.
- `tauri-plugin-liquid-glass` (reference impl, not a dep): `github.com/hkandala/tauri-plugin-liquid-glass`.
- Existing in-repo: `src-tauri/src/vibrancy.rs` (slice #2), `src/styles/tokens.css`, `src/components/Sidebar.tsx`, `src-tauri/src/accent.rs` (slice #1), `src-tauri/src/lib.rs` (Tauri command registration).
- Native-feel status: `docs/superpowers/native-feel-status.md`.
