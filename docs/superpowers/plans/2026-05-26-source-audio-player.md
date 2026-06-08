# Source Audio Player Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the user play any source file listed on a recording, inline, without leaving the recording view — so transcript-vs-audio debugging stops requiring an external player.

**Architecture:** Three changes — enable Tauri 2's asset protocol in `tauri.conf.json`, add an `expandedSources` `Set<number>` state to `RecordingView`, and render an `<audio controls preload="metadata">` element under expanded source rows using `convertFileSrc()` to map absolute paths to webview URLs.

**Tech Stack:** Tauri 2 (`@tauri-apps/api/core` → `convertFileSrc`), React (existing `useState`), Phosphor icons via the project's `Icon` component, native HTML5 `<audio>`.

**Note on testing:** The spec (`docs/superpowers/specs/2026-05-26-source-audio-player-design.md`) explicitly rules out automated tests for this change — playback is a webview + OS-codec integration that mocking doesn't meaningfully cover. The verification step is manual and lives at the end of the plan as Task 4. This is deliberate, not a gap.

---

## File Structure

| File | Responsibility |
|---|---|
| `src-tauri/tauri.conf.json` | Enables and scopes the asset protocol. Webview-side gate. |
| `src/components/RecordingView.tsx` | UI state (which rows are expanded), the chevron toggle, the `<audio>` element. |
| `docs/superpowers/specs/2026-05-26-source-audio-player-design.md` | Design spec (already committed). No further changes. |

No new files are created. All changes are local to existing files.

---

### Task 1: Enable the Tauri asset protocol

**Files:**
- Modify: `src-tauri/tauri.conf.json` (add a block inside `app.security`)

The current file has `"app": { ..., "security": { "csp": null } }`. We add `assetProtocol` next to `csp`. Object-form scope with `requireLiteralLeadingDot: false` so paths under `~/Library/...` (which is where `app_data_dir` resolves on macOS) aren't filtered as hidden segments.

- [ ] **Step 1: Edit `tauri.conf.json` to add the `assetProtocol` block**

Find the existing `"security"` object inside `"app"` and change it from:

```json
    "security": {
      "csp": null
    }
```

to:

```json
    "security": {
      "csp": null,
      "assetProtocol": {
        "enable": true,
        "scope": {
          "requireLiteralLeadingDot": false,
          "allow": ["**/*"]
        }
      }
    }
```

- [ ] **Step 2: Sanity-build the Tauri config**

Run: `npm run tauri info` (any Tauri CLI command that parses the config — `info` is cheap)
Expected: prints config summary with no JSON parse errors.

If you get a JSON parse error, you broke the file. Re-check commas around the `csp` line.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/tauri.conf.json
git commit -m "feat(tauri): enable asset protocol for source-file playback

Enables app.security.assetProtocol with broad scope (\"**/*\",
requireLiteralLeadingDot=false) so the webview can fetch arbitrary
user-selected source files via convertFileSrc. Matches existing
filesystem posture: source paths already come from the OS file
picker, which the user has explicitly vouched for."
```

---

### Task 2: Add the chevron toggle + `expandedSources` state in RecordingView

**Files:**
- Modify: `src/components/RecordingView.tsx` (state declaration block, source list `<li>`)

We add a `Set<number>` of expanded source indices and a chevron button at the start of each `<li>` that toggles membership. The `<audio>` element itself comes in Task 3 — this task just wires the toggle so we can see it visibly working in Task 4 of itself.

- [ ] **Step 1: Add `convertFileSrc` to the existing core import**

Find this line near the top of `src/components/RecordingView.tsx` (around line 4):

```ts
import { invoke } from "@tauri-apps/api/core";
```

Change it to:

```ts
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
```

(`convertFileSrc` is unused after this step alone — Task 3 will use it. The eslint config in this repo doesn't flag unused imports as errors, but if the build complains during Task 2, just defer the import to Task 3.)

- [ ] **Step 2: Add the `expandedSources` state**

Find the existing state declarations near the top of the `RecordingView` component body. After this block (around line 56-57):

```tsx
  const [formattedDraft, setFormattedDraft] = useState<string | null>(null);
  const persistTimerRef = useRef<number | null>(null);
```

Add:

```tsx
  const [expandedSources, setExpandedSources] = useState<Set<number>>(() => new Set());

  const toggleSourceExpanded = useCallback((idx: number) => {
    setExpandedSources((prev) => {
      const next = new Set(prev);
      if (next.has(idx)) next.delete(idx);
      else next.add(idx);
      return next;
    });
  }, []);
```

`useCallback` is already imported at the top of the file (line 1).

- [ ] **Step 3: Add the chevron button at the start of each source `<li>`**

Find the source `<li>` template (around lines 336-360). The current structure is:

```tsx
<li
  key={`${p}-${idx}`}
  className={`flex items-center gap-3 px-3 py-1.5 rounded text-footnote font-mono ...`}
>
  <span className="text-label-tertiary tabular-nums w-5 shrink-0">
    {rec.sourcePaths.length > 1 ? `${idx + 1}.` : ""}
  </span>
  <span className={`flex-1 truncate ${missing ? "text-status-error" : "text-label-secondary"}`} title={p}>
    {p}
  </span>
  {missing && (
    <Button ... > Locate… </Button>
  )}
</li>
```

We need to restructure: the `<li>` itself becomes `flex-col` (so the audio element can sit below the row in Task 3), and the existing row contents are wrapped in a `<div className="flex items-center gap-3">`. The chevron goes first in that inner div, and is hidden when the file is missing.

Replace the entire `<li>...</li>` block (the body of the `.map` callback) with:

```tsx
<li
  key={`${p}-${idx}`}
  className={`flex flex-col gap-1.5 px-3 py-1.5 rounded text-footnote font-mono
    ${missing
      ? "bg-status-error/10 border border-status-error/20"
      : "bg-control-background"}`}
>
  <div className="flex items-center gap-3">
    {!missing ? (
      <button
        type="button"
        aria-label={expandedSources.has(idx) ? "Collapse player" : "Expand player"}
        aria-expanded={expandedSources.has(idx)}
        onClick={() => toggleSourceExpanded(idx)}
        className="shrink-0 inline-flex items-center justify-center w-4 h-4 text-label-tertiary hover:text-label-primary transition-transform"
        style={{ transform: expandedSources.has(idx) ? "rotate(90deg)" : "rotate(0deg)" }}
      >
        <Icon name="chevron-right" size="sm" />
      </button>
    ) : (
      <span className="shrink-0 w-4" aria-hidden />
    )}
    <span className="text-label-tertiary tabular-nums w-5 shrink-0">
      {rec.sourcePaths.length > 1 ? `${idx + 1}.` : ""}
    </span>
    <span className={`flex-1 truncate ${missing ? "text-status-error" : "text-label-secondary"}`} title={p}>
      {p}
    </span>
    {missing && (
      <Button
        variant="ghost"
        size="sm"
        onClick={() => locateSource(idx)}
        className="text-accent shrink-0"
      >
        Locate…
      </Button>
    )}
  </div>
</li>
```

(Task 3 will add the `<audio>` element as a second child of the `<li>`, below the inner `<div>`.)

- [ ] **Step 4: Run the app and verify the chevron toggles visually**

Run: `npm run tauri dev`
Open a recording with at least one non-missing source.
Click the chevron next to a source row.
Expected: the chevron rotates 90° on click and rotates back on the next click. Nothing else changes yet (no audio player — that's Task 3).

If the chevron doesn't appear at all, double-check the `<li>` structure didn't accidentally drop the `aria-expanded` toggle or the `!missing` branch.

- [ ] **Step 5: Commit**

```bash
git add src/components/RecordingView.tsx
git commit -m "feat(recording): chevron toggle for per-source expansion

Adds expandedSources Set state and a chevron button at the start of
each source row that toggles row membership. Visual toggle only;
the audio player itself comes in the next commit."
```

---

### Task 3: Render the `<audio>` element when a row is expanded

**Files:**
- Modify: `src/components/RecordingView.tsx` (inside the source `<li>` from Task 2)

Add the player element as a sibling of the inner `<div>` we created in Task 2, conditional on `expandedSources.has(idx)`. Track a per-row error message in component state so codec failures show up inline.

- [ ] **Step 1: Add per-row playback error state**

Right after the `expandedSources`/`toggleSourceExpanded` block from Task 2 Step 2, add:

```tsx
  const [playerErrors, setPlayerErrors] = useState<Record<number, string>>({});
```

- [ ] **Step 2: Add the `<audio>` element and error label inside the `<li>`**

Inside the source `<li>` from Task 2, after the closing `</div>` of the inner row container and before the `</li>`, add:

```tsx
  {!missing && expandedSources.has(idx) && (
    <div className="pl-7 pr-1 pb-1">
      <audio
        controls
        preload="metadata"
        src={convertFileSrc(p)}
        className="w-full h-8"
        onError={(e) => {
          const el = e.currentTarget;
          const msg = el.error?.message || `Cannot play this file (code ${el.error?.code ?? "?"}).`;
          setPlayerErrors((prev) => ({ ...prev, [idx]: msg }));
        }}
        onLoadedMetadata={() => {
          setPlayerErrors((prev) => {
            if (!(idx in prev)) return prev;
            const { [idx]: _, ...rest } = prev;
            return rest;
          });
        }}
      />
      {playerErrors[idx] && (
        <p className="mt-1 text-status-error text-footnote font-sans">{playerErrors[idx]}</p>
      )}
    </div>
  )}
```

The `pl-7` left-pads the player so it lines up under the path text, past the chevron and index columns.

- [ ] **Step 3: Run the app and verify playback works**

Run: `npm run tauri dev`
Open the recording you've been using for the Twi+English bug. Expand a source row.
Expected: native `<audio>` controls appear; pressing play streams the source. Scrubbing to arbitrary positions works without a noticeable pre-buffer pause (because the asset protocol supports HTTP range requests).

If the player shows up but immediately errors with a "Permission denied" or "not in scope" message, the asset protocol config from Task 1 isn't picking up. Restart the dev server (config changes don't hot-reload).

If the player shows up but `<audio>` fails for a `.mov`/`.mp4` source with a codec-related error, that's expected behavior for the small set of containers/codecs WebKit can't decode — the error label will appear and the user can fall back to QuickTime. Not a bug.

- [ ] **Step 4: Commit**

```bash
git add src/components/RecordingView.tsx
git commit -m "feat(recording): inline source audio player on expanded rows

Renders a native <audio controls preload=\"metadata\"> element below
each expanded source row, with convertFileSrc mapping the absolute
path to an asset:// URL. Codec/IO errors surface inline beneath the
player; successful metadata load clears any previous error.

Resolves the debugging workflow gap that forced switching to
QuickTime to verify whether a transcript mismatch is the source
audio's fault or whisper's."
```

---

### Task 4: Manual verification end-to-end

This is the verification contract the spec specifies — no automated test, but a structured manual run.

- [ ] **Step 1: Verify single-source recording**

Open a single-source recording. Confirm:
- A right-pointing chevron appears at the start of the source row.
- Clicking the chevron rotates it 90° and reveals the audio player below.
- The player loads metadata (duration shows up next to the scrubber).
- Pressing play produces audio.
- Pressing the chevron again collapses the row and pauses playback (the `<audio>` element is unmounted, so playback stops).

- [ ] **Step 2: Verify multi-source recording**

Open a multi-source recording (or use a project with one). Confirm:
- Each row has its own chevron.
- Expanding two rows shows two independent players.
- The index numbers (1., 2., …) are still visible to the left of each path.

- [ ] **Step 3: Verify missing-source row**

Open a recording with at least one source that no longer exists at its path (or temporarily rename a source file). Confirm:
- The chevron is **not** rendered on the missing row (a hidden spacer keeps column alignment).
- The "Locate…" button still appears.
- Other (non-missing) rows still get their chevrons.

- [ ] **Step 4: Verify the original debugging workflow**

This is the whole point of the feature. Open the Twi+English recording from the active debugging session, expand its source, and listen to a section where the transcript reads "I'm gonna give them a two-shot." Confirm whether the actual audio at that timestamp matches the transcript or is something else entirely. Report the finding back to the debugging thread.

(No commit for this task — it's purely verification.)

---

## Self-Review

**Spec coverage check (against `docs/superpowers/specs/2026-05-26-source-audio-player-design.md`):**
- Asset protocol config → Task 1. ✓
- `convertFileSrc` usage → Task 2 Step 1, used in Task 3 Step 2. ✓
- Inline expander per source row → Task 2 Step 3, Task 3 Step 2. ✓
- `expandedSources: Set<number>` state → Task 2 Step 2. ✓
- `<audio controls preload="metadata">` → Task 3 Step 2. ✓
- `onError` surfaces `MediaError.message` → Task 3 Step 2. ✓
- Chevron hidden on missing-file rows → Task 2 Step 3 (`!missing` branch with `<span>` spacer). ✓
- No automated test → noted up front in the plan header; Task 4 is manual verification. ✓
- Multi-source independence → Task 4 Step 2. ✓

**Placeholder scan:** none — every step has the exact code or command.

**Type consistency:** `expandedSources: Set<number>`, `playerErrors: Record<number, string>`, `toggleSourceExpanded(idx: number)` are consistent across Tasks 2 and 3.

**Ambiguity check:** the chevron's hidden-vs-spacer behavior on missing rows is explicit (Task 2 Step 3: `<span className="shrink-0 w-4" aria-hidden />`).
