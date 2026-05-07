# Icon Pass — Design

Slice **#5** of the native-macOS-feel overhaul (`docs/superpowers/native-feel-status.md`). Replace emoji-as-iconography with a coherent SF-Symbols-adjacent icon system, normalize unicode-symbol transport controls, add a leading-icon affordance to `<Button>`, and audit/migrate every visible icon site in the app.

## Why

Per the native-feel status doc, this slice is "the single biggest 'this is a real Mac app' payoff still pending." Today the app uses emoji glyphs (🎙 🔊 ⚠ 🔒 🎛 ✍️ 🤖 💾) for hero and stage iconography, which is the loudest "AI-app aesthetic" tell remaining after #1–#4a. Unicode-symbol transport buttons (⏸ ▶ ◼) render unevenly and don't match the toolbar/icon vocabulary. There is no central `<Icon>` component, so future slices (#4b notifications, #4c preferences, #6 redesign) have nothing to lean on.

## Scope

**In:**
- A single `<Icon name="..." size={...} />` component with a typed name registry.
- A leading/trailing icon API on `<Button>`.
- Migration of every emoji + literal `✕` in `src/` production code (tests excluded) to `<Icon>`.
- Migration of unicode transport symbols (⏸ ▶ ◼) on `RecordCard` and `RecordCallCard` to icon-rendered buttons.
- An icon registry pre-populated with forward-looking names that have no call sites yet but are well-anticipated by other slices: `gear`/`question`/`info`/`sidebar` (#4b/#4c menus), `folder`/`recording` (#6 source-list treatment), `pencil` (#4c rename + Undo plumbing). Adding them in this slice means each follow-up does not need to touch `Icon.tsx`.

**Out:**
- Filled-variant icons (defer until a consumer needs selected/active state).
- Replacing the `bg-status-error animate-pulse` recording dot with a `record.circle` icon — the CSS dot already matches macOS recording indicators.
- Hand-rolled SVG overrides for any icon (rejected during brainstorming — see "Source" below).
- Icon-pass on screens not currently shipping emoji (TemplateEditor, OutputPreview have nothing to migrate today).
- `SettingsForm.tsx:31-32` has `✓` characters embedded inside the string returned by `formatModelLabel()` — they render as inline body-text glyphs, not icon sites. Replacing them would require restructuring the label helper to return JSX, which is out of scope for this slice.

## Source

**Phosphor icons** (`@phosphor-icons/react`, MIT). Phosphor's `regular` weight at 20px is the closest off-the-shelf match to SF Symbols' "Regular" weight. Phosphor offers six weights — we use three (`bold` / `regular` / `light`), one per size, to reproduce SF Symbols' optical-sizing behavior.

**Hand-rolled overrides considered and rejected.** During brainstorming we considered hand-drawing five "loud" icons (mic, waveform, doc-text, sparkles, equalizer) tuned to SF Symbols' optical traits. Cost was ~4 hours of careful SVG drawing + per-size weight tuning + light/dark testing for marginal gain — Phosphor at the right weight already lands close. Reactive replacement: if a specific icon reads wrong in practice, we override that one in a follow-up.

**License:** Phosphor is MIT, freely redistributable.

## Component & API

**File:** `src/components/ui/Icon.tsx`

```tsx
import * as P from "@phosphor-icons/react";

export type IconName =
  | "mic" | "waveform" | "doc-text" | "media" | "sparkles" | "equalizer" | "save"
  | "lock" | "warning"
  | "play" | "pause" | "stop" | "trash"
  | "plus" | "x" | "check" | "pencil" | "chevron-right"
  | "folder" | "recording"
  | "gear" | "question" | "info"
  | "sidebar";

export type IconSize = "sm" | "md" | "lg";

interface Props {
  name: IconName;
  size?: IconSize;
  className?: string;
  "aria-label"?: string;
}
```

Internal const map resolves `name` to the Phosphor component:

```tsx
const REGISTRY: Record<IconName, React.ComponentType<P.IconProps>> = {
  mic: P.Microphone,
  waveform: P.WaveformCircle,
  "doc-text": P.FileText,
  media: P.FilmStrip,
  sparkles: P.Sparkle,
  equalizer: P.Equalizer,
  save: P.FloppyDisk,
  lock: P.Lock,
  warning: P.WarningCircle,
  play: P.Play,
  pause: P.Pause,
  stop: P.Stop,
  trash: P.Trash,
  plus: P.Plus,
  x: P.X,
  check: P.Check,
  pencil: P.Pencil,
  "chevron-right": P.CaretRight,
  folder: P.Folder,
  recording: P.WaveformBold ?? P.Waveform,
  gear: P.Gear,
  question: P.Question,
  info: P.Info,
  sidebar: P.SidebarSimple,
};
```

(Concrete Phosphor names are best-effort guesses; the implementer verifies actual exports during Task 1 and adjusts.)

### Sizing

| size | px | weight   | use case                              |
|------|----|----------|----------------------------------------|
| sm   | 14 | `bold`     | inline buttons, list-row affordances  |
| md   | 20 | `regular`  | StageCard inline, toolbar             |
| lg   | 32 | `light`    | empty-state heros, error/permission   |

Color is always `currentColor` (Phosphor default). Tinting flows through Tailwind text classes (`text-label-secondary`, `text-status-error`, etc.) on `<Icon>`'s `className` or its parent.

### Accessibility

- When `aria-label` is provided, the rendered SVG carries `role="img"` + the label.
- When omitted, the SVG carries `aria-hidden="true"` and is treated as decorative.
- Icon-only `<Button>` (i.e., no `children`) MUST set `aria-label`. Enforce via TS overload on `<Button>` if low-cost; otherwise runtime `console.warn` in dev.

## Button extension

Add to existing `<Button>` primitive:

```tsx
interface ButtonProps {
  // … existing …
  iconLeft?: IconName;
  iconRight?: IconName;
}
```

Render rules:
- Icon size: `sm` button → `Icon size="sm"`; `md` and `lg` buttons → `Icon size="sm"` for icons-with-text, `Icon size="md"` for icon-only buttons (so single-glyph buttons feel proportionate).
- Layout: `iconLeft` then `children` then `iconRight`, with `gap-1.5` for `sm` buttons and `gap-2` for `md`/`lg`.
- When `children` is empty + only `iconLeft` (icon-only button), apply equal x/y padding so the button is square.

## Migration map

| Site (file:approx-line) | Today | After | Size | Color class | UI tweak |
|---|---|---|---|---|---|
| `RecordCard.tsx` idle hero | `🎙 text-3xl opacity-30` | `mic` | lg | `text-label-tertiary` | drop in |
| `RecordCard.tsx` Pause button | `⏸` | `iconLeft="pause"`, no children | sm | inherits | gap-1.5 |
| `RecordCard.tsx` Resume button | `▶ Resume` | `iconLeft="play"` + "Resume" | sm | inherits | gap-1.5 |
| `RecordCard.tsx` Stop button | `◼ Stop` | `iconLeft="stop"` + "Stop" | sm | inherits | gap-1.5 |
| `RecordCard.tsx` Discard button | "Discard" | `iconLeft="trash"` + "Discard" | sm | inherits | gap-1.5 |
| `RecordCard.tsx` error hero | `⚠ text-3xl opacity-50` | `warning` | lg | `text-status-error/70` | drop in |
| `RecordCallCard.tsx` idle hero | `🔊` | `waveform` | lg | `text-label-tertiary` | drop in |
| `RecordCallCard.tsx` permission-needed hero | `🔒` | `lock` | lg | `text-status-warning/80` | drop in |
| `RecordCallCard.tsx` Pause/Resume/Stop/Discard | same as RecordCard | same | sm | inherits | same |
| `RecordCallCard.tsx` error hero | `⚠` | `warning` | lg | `text-status-error/70` | drop in |
| `StageCard.tsx` extract | `🎛` | `equalizer` | md | inherits | none |
| `StageCard.tsx` transcribe | `✍️` | `doc-text` | md | inherits | none |
| `StageCard.tsx` format | `🤖` | `sparkles` | md | inherits | none |
| `StageCard.tsx` save | `💾` | `save` | md | inherits | none |
| `StagingList.tsx:88` close | `✕` text in `<button>` | `x` (icon-only Button or Icon in existing button) | sm | `text-label-tertiary hover:text-label-primary` | wrap as 24×24 circular hover-scrim hit area |
| `Sidebar.tsx:211` create-confirm | `✓` text | `check` | sm | inherits | none (already in a Button) |
| `Sidebar.tsx:218` create-cancel | `✕` text | `x` | sm | same | none (already in a Button) |
| `Sidebar.tsx` add-project `+` ghost button | text `+` | `plus` | sm | inherits | padding bump → ≥ 24px hit target |
| `RecordingDropZone.tsx` hero | `🎬 text-3xl opacity-30` | `media` (FilmStrip) | lg | `text-label-tertiary` | drop in (added during implementation; not in original sweep) |
| Sidebar / recording-row hover chevron | (none) | `chevron-right` `opacity-0 group-hover:opacity-100` | sm | `text-label-tertiary` | subtle disclosure cue on hover (this is a *new* affordance, deliberate; deferred to #6) |

## Testing

- `Icon.test.tsx`:
  - Renders a known name at each size, asserts `<svg>` has expected `width`/`height` attr (14/20/32).
  - When no `aria-label`: `aria-hidden="true"` present.
  - When `aria-label="Foo"` provided: attr present, no `aria-hidden`.
  - Smoke test: every name in `IconName` resolves without throwing (parameterized).
- `Button.test.tsx` (extension):
  - `iconLeft` renders the right `<Icon>` left of children.
  - `iconRight` renders right of children.
  - Icon-only button (no children, only `iconLeft`) without `aria-label`: dev-mode `console.warn` fires once.
- Migration validation (manual + grep at end):
  - `grep -rE '🎙|🔊|⚠|🔒|🎛|✍️|🤖|💾' src/ --include='*.tsx' --include='*.ts'` → 0 hits outside tests/specs.
  - `grep -nE '>\s*✕\s*<|>\s*✓\s*<' src/ --include='*.tsx' --include='*.ts'` → 0 hits outside tests/specs and outside `SettingsForm.tsx`'s string helper (carved out above).

## File structure & sequencing

**New:**
- `src/components/ui/Icon.tsx`
- `src/components/ui/Icon.test.tsx`

**Modified, in order:**
1. `package.json` — add `@phosphor-icons/react`.
2. `Icon.tsx` + `Icon.test.tsx` (TDD).
3. `src/components/ui/index.ts` — export `Icon`, `IconName`, `IconSize`.
4. `Button.tsx` + `Button.test.tsx` extension — `iconLeft`/`iconRight`.
5. `RecordCard.tsx`.
6. `RecordCallCard.tsx`.
7. `StageCard.tsx`.
8. `StagingList.tsx`.
9. `Sidebar.tsx`.
10. Bundle audit — if `npm run build` shows full-Phosphor in dist, switch `Icon.tsx` to per-icon `@phosphor-icons/react/dist/ssr/<Name>` imports.
11. Final grep-verify; if anything missed, fix in a final cleanup commit.

Each migration (steps 5–9) is its own commit so each step is reviewable and the app stays runnable at every checkpoint.

## Risks

- **Phosphor export name drift.** The exact `P.Microphone` / `P.WaveformCircle` exports may differ from this guess; Task 1 confirms actual names against the installed version and adjusts the registry. Not a design risk — implementation verifies.
- **Bundle size.** Naïve barrel import of `@phosphor-icons/react` may pull all icons. Mitigated by step 10 above.
- **Icon-only Button a11y guard.** TS-level enforcement (overload on `<Button>`) is preferable but might fight the existing prop typing; if so, fall back to dev `console.warn`. Implementation chooses based on cost.
- **Specific icon doesn't read right.** If `equalizer` or `sparkles` (the most likely candidates) feel wrong in practice during the migration commits, override that single icon with a hand-rolled SVG in a follow-up — out of scope for this slice.

## Done criteria

- All 65+ existing frontend tests pass; new `Icon` and extended `Button` tests pass.
- `tsc --noEmit` clean.
- The two grep verifications return 0 production hits.
- `npm run build` succeeds; Phosphor footprint in dist is documented (and tree-shaken if step 10 was needed).
- Manual smoke (per Task 12-style checklist): every screen with a migrated icon renders, light + dark, in the running Tauri app.

## References

- Native-feel status: `docs/superpowers/native-feel-status.md`
- Control library foundations spec: `docs/superpowers/specs/2026-04-27-control-library-foundations-design.md` (the existing `<Button>` we extend)
- Phosphor docs: `https://phosphoricons.com/`
