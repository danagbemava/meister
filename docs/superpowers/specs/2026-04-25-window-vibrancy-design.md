# Window Vibrancy & Translucent Sidebar

Sub-project #2 of the "native macOS feel" overhaul. Builds on sub-project #1 (design tokens + light/dark foundation). This is the change that visually shifts Meister from "polished web app in a window" to "native Mac app" by attaching `NSVisualEffectView` to the main window and threading translucency through the existing token system.

## Context

Sub-project #1 established a semantic token layer keyed to Apple's NSColor vocabulary, full light/dark palettes, system-appearance and system-accent live tracking, and the macOS HIG type scale. The window background, sidebar, and cards currently render as solid colors — there is no vibrancy, no translucency.

The Tauri config already uses `titleBarStyle: "Overlay"` with native traffic lights drawn over a custom React `TitleBar`. The `TitleBar` shows a "Meister" text label which duplicates what the macOS menu bar already shows.

Subsequent sub-projects (#3 control library, #4 menu bar + shortcuts, #5 SF Symbols, #6 component redesign) will layer additional native affordances on top of this vibrant foundation.

## Goals

- Attach an `NSVisualEffectView` with `.windowBackground` material to the main window so the entire window background renders with macOS vibrancy.
- Apply layered translucency: window-background tokens become transparent (vibrancy shows through fully), sidebar gets a subtle tint on top of vibrancy, cards/control surfaces stay mostly opaque (~92%) for content readability.
- Drop the "Meister" label from the custom `TitleBar`. Keep the drag region. Native traffic lights remain at their default position, now floating on vibrancy.
- Add a 1px `border-separator` divider on the right edge of the sidebar — matches how Mail, Music, Notes draw their sidebar boundary against the main content area.
- Detect macOS Reduce Transparency setting and gracefully fall back to fully opaque surfaces when on. Live-update via observer (same pattern sub-project #1 established for accent and appearance).
- Vibrancy follows the native key-window state automatically (this is `NSVisualEffectView`'s default state `.followsWindowActiveState`; no extra work required).

## Non-goals

- No changes to component layouts, spacing, type, or interaction patterns.
- No vibrancy on individual sub-views (cards, modals, popovers) — deferred to sub-project #6 if needed.
- No window-shape changes, no extra rounded corners, no transparent titlebar areas other than what already exists.
- No changes to the sidebar's row styling — sub-project #6's source-list redesign owns that.
- No support for arbitrary user-chosen titlebar styles.
- No `NSToolbar` integration — sub-project #4 owns that.
- No Increase Contrast accessibility support this pass (future follow-up that mirrors the Reduce Transparency plumbing).
- No user-facing "disable vibrancy" toggle. Reduce Transparency is the proper system-level escape hatch.

## Architecture

```
AppKit (Rust)              CSS variables                Tailwind / components
────────────────────────   ────────────────────────     ─────────────────────────────────
NSVisualEffectView         (no new vars at boot)        bg-window-background → fully transparent
attached to main window
                                                        bg-sidebar-background → translucent tint
                                                        bg-control-background → 92% alpha solid

NSWorkspace observer       --color-* token overrides    Same classes — just resolved against
"reduceTransparency"   →   when [data-reduce-           opaque palette under reduced-transparency
                           transparency="true"]
```

### Rust additions

A new `src-tauri/src/vibrancy.rs` module, macOS-only:

1. **`apply_vibrancy(window: &Window)`** — Called once during the Tauri `setup` block. Walks up to the `NSWindow`, creates an `NSVisualEffectView` with material `.windowBackground`, blending mode `.behindWindow`, state `.followsWindowActiveState`. Inserts it as the window's `contentView`'s subview at index 0 (so the React webview sits on top). Sets `autoresizingMask` to `[.width, .height]` so it tracks window resize.

2. **`set_window_transparent(window: &Window)`** — Sets `NSWindow.opaque = NO`, `NSWindow.backgroundColor = NSColor.clear`. Required for vibrancy to actually show through; otherwise AppKit composites a solid window color first.

3. **`get_reduce_transparency() -> bool`** — Tauri command returning `NSWorkspace.shared.accessibilityDisplayShouldReduceTransparency`.

4. **`install_reduce_transparency_observer(app: AppHandle)`** — Subscribes to `NSWorkspace.accessibilityDisplayOptionsDidChangeNotification` (delivered via `NSWorkspace.shared.notificationCenter`, *not* the distributed center). Emits `system-reduce-transparency-changed` with the new boolean. Same forget-the-token pattern as the accent observer in sub-project #1.

Non-macOS builds get stub implementations: `apply_vibrancy` and `set_window_transparent` are no-ops; `get_reduce_transparency` returns `false`; `install_reduce_transparency_observer` is a no-op.

### Tauri config

`src-tauri/tauri.conf.json` window block changes:

- Add `"transparent": true` (was `false`). Required so the window itself doesn't paint a solid background that would occlude vibrancy.
- `titleBarStyle: "Overlay"` stays.
- `decorations: true` stays — we still want the OS-drawn traffic lights.

### Frontend additions

`src/lib/reduceTransparency.ts` — analogous to `appearance.ts`:

- `initReduceTransparency()` — invokes `get_reduce_transparency`, sets `data-reduce-transparency="true"` on `<html>` if on, subscribes to `system-reduce-transparency-changed`, updates the attribute live.
- Idempotent (`initialized` flag); `__resetForTests` helper.
- Tests in `reduceTransparency.test.ts`.

`src/main.tsx` — adds `void initReduceTransparency()` alongside the existing `initAppearance()` and `initAccent()` calls.

### Token mechanism upgrade

Each opaque token whose surface participates in translucency gains a paired `*-alpha` CSS variable. Tailwind composes `rgb(triplet) / alpha-var` instead of `rgb(triplet) / <alpha-value>` for these tokens. Components are unaffected — the class names don't change.

**Trade-off:** the four affected tokens lose Tailwind's `/N` alpha modifier syntax. None of the migrated components currently use `bg-window-background/50` or similar; alpha modifiers remain on accent and status colors, which keep `<alpha-value>`.

### Component changes

- `src/components/TitleBar.tsx` — Remove the "Meister" text label. Keep the drag region and the traffic-light reservation.
- `src/components/Sidebar.tsx` — Add a right-edge `border-r border-separator` for the visual divider.
- No other components touched.

## Token catalog changes

### New `*-alpha` companion variables

For each token below, `tokens.css` gains a paired alpha variable in both light and dark blocks. Initial values are the **vibrancy-on** state:

| Token | Existing rgb triplet (unchanged) | New alpha variable | Vibrancy-on value |
|---|---|---|---|
| `window-background` | `--color-window-background-rgb` | `--color-window-background-alpha` | `0` (fully transparent) |
| `sidebar-background` | `--color-sidebar-background-rgb` | `--color-sidebar-background-alpha` | `0.4` (light tint over vibrancy) |
| `control-background` | `--color-control-background-rgb` | `--color-control-background-alpha` | `0.92` (mostly opaque, slight bleed) |
| `under-page-background` | `--color-under-page-background-rgb` | `--color-under-page-background-alpha` | `0.6` |

Status, accent, scrim, and inherent-alpha tokens (labels, separators, fills) **do not** get alpha companions — they're either already correct or have inherent alpha baked in.

### Tailwind mapping change

```diff
- "window-background":  "rgb(var(--color-window-background-rgb) / <alpha-value>)",
+ "window-background":  "rgb(var(--color-window-background-rgb) / var(--color-window-background-alpha))",

- "sidebar-background": "rgb(var(--color-sidebar-background-rgb) / <alpha-value>)",
+ "sidebar-background": "rgb(var(--color-sidebar-background-rgb) / var(--color-sidebar-background-alpha))",

- "control-background": "rgb(var(--color-control-background-rgb) / <alpha-value>)",
+ "control-background": "rgb(var(--color-control-background-rgb) / var(--color-control-background-alpha))",

- "under-page-background": "rgb(var(--color-under-page-background-rgb) / <alpha-value>)",
+ "under-page-background": "rgb(var(--color-under-page-background-rgb) / var(--color-under-page-background-alpha))",
```

### `[data-reduce-transparency="true"]` block

Added to `tokens.css` after the existing light/dark blocks:

```css
[data-reduce-transparency="true"] {
  --color-window-background-alpha:      1;
  --color-sidebar-background-alpha:     1;
  --color-control-background-alpha:     1;
  --color-under-page-background-alpha:  1;
}
```

When this attribute is set, every translucent surface becomes fully opaque again. AppKit also automatically falls vibrancy back to a solid material under reduce-transparency, so the result is solid surfaces composited on a solid window — visually equivalent to where the app was before sub-project #2 landed.

### Light vs dark `*-alpha`

The same alpha values work in both light and dark mode. Vibrancy in `.windowBackground` material adapts to system appearance automatically (light-mode vibrancy is whitish, dark-mode vibrancy is grayish). Tints layer on top correctly without per-appearance alpha tweaks.

## Testing

### Automated

**Frontend (Vitest):**

`src/lib/reduceTransparency.test.ts` mirrors `appearance.test.ts`:

- Sets `data-reduce-transparency="true"` when `get_reduce_transparency` returns `true`.
- Removes the attribute when it returns `false`.
- Re-applies on `system-reduce-transparency-changed` events.
- Idempotent.
- Falls back to `false` (vibrancy on) if the Tauri command fails.
- `__resetForTests` clears state.

No new tests for `accent.ts` or `appearance.ts` — their behavior is unchanged.

**Rust:**

- `vibrancy.rs` gets a unit test `get_reduce_transparency_returns_bool` confirming the command returns without panicking. We can't assert a specific value — it depends on the test machine's accessibility settings.
- No unit test for `apply_vibrancy` — it requires an actual `NSWindow` which doesn't exist in `cargo test` context. Coverage comes from the manual smoke.

### Manual QA checklist (final task of this sub-project)

For each of light + dark mode:

1. Boot the app over a colorful desktop background. Window background picks up subtle tints from the wallpaper.
2. Drag the window across the screen. Vibrancy re-renders — colors shift as the window moves.
3. Click outside the window. Vibrancy desaturates (inactive state). Click back in — vibrancy resaturates.
4. Sidebar / content boundary visible: 1px separator divides the sidebar from the main content. Sidebar reads as visibly distinct from the main content (slight tint).
5. Cards stay readable. `RecordingView` cards, `StageCard`, `OutputPreview` — text legible; the 8% bleed-through doesn't wash out card content.
6. No "Meister" text in titlebar. Just traffic lights and empty drag space.

**Reduce Transparency interaction:**

7. System Settings → Accessibility → Display → Reduce Transparency: ON. Within ~1s, Meister flips to fully opaque surfaces. Vibrancy gone. App still looks correct (matches the post-sub-project-#1 state).
8. Toggle Reduce Transparency back OFF. Vibrancy returns within ~1s.

**Cross-app comparison:**

9. Compare against Mail.app and Notes.app side by side. Vibrancy intensity should feel comparable. If Meister looks notably more or less translucent, tune the `*-alpha` values.

## Error handling

| Failure | Behavior |
|---|---|
| `apply_vibrancy` fails (window handle invalid, AppKit returns nil) | Log once to stderr; window renders with the rgb triplet at full alpha (since `--color-window-background-alpha: 0` would make it fully transparent — fall back to `1` if vibrancy didn't attach). App still functional. |
| `set_window_transparent` fails | Log + continue. The vibrancy view is just hidden behind the opaque window paint. |
| `get_reduce_transparency` Tauri command fails | Default to `false` (vibrancy on). Log once. |
| `system-reduce-transparency-changed` observer fails to attach | Boot value is used; no live updates. Log once. |
| `transparent: true` Tauri config conflicts with `decorations: true` on macOS | Should not — Tauri 2 supports this combination. If it crashes at startup, fall back to leaving `transparent: false` and accept vibrancy showing through a fully-transparent contentView (same end result, slightly different mechanism). |

### Vibrancy-failure recovery

If `apply_vibrancy` fails at boot, the frontend has no way to know without a signal from Rust. To keep the app legible in that case, `apply_vibrancy`'s failure path emits a `vibrancy-unavailable` Tauri event. The frontend's `initVibrancyFallback()` (a small addition to `reduceTransparency.ts` or a sibling module) listens for it and applies `data-reduce-transparency="true"` as a defensive measure — vibrancy isn't there, so showing translucent surfaces over nothing would look broken. This is a belt-and-suspenders approach since AppKit failures here are extremely rare.

## Out-of-scope / future work

- Increase Contrast support (`[data-increase-contrast="true"]` block; mirrors Reduce Transparency plumbing).
- Per-component vibrancy on popovers, modals, sheets.
- Sidebar source-list row styling (sub-project #6).
- Toolbar / `NSToolbar` integration (sub-project #4).
- Window-shape and corner customization.
- Custom titlebar content (current project name, recording status, etc.) — revisit in sub-project #6.
