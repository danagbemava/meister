# Format Style + Custom Instructions — Design

A small detour on the native-feel roadmap to add two project-level format controls:

1. **Match writing style** — a checkbox on the existing format template that, when enabled, instructs the LLM to also mirror the template's tone, sentence rhythm, and vocabulary in the generated output.
2. **Additional instructions** — a free-form per-project textarea whose content is injected into the LLM prompt as supplementary formatting rules (e.g., locale, length caps, override defaults).

Both ship together because they share the same plumbing: storage shape on `Project`/`Template`, prompt construction in `build_prompt`, and the format cache key.

## Why

Today the format template is interpreted as both layout AND style by the LLM (the prompt at `commands.rs:892-916` says "follow the layout and style of the FORMAT TEMPLATE"). For most projects users want the layout but not the voice — a meeting-notes template usually has placeholder content in a generic voice, but the user's actual transcripts may benefit from a specific tone they want enforced. Today there is no way to opt in to style-mirroring, and no way to add free-form rules ("use British English", "skip empty sections", "max 500 words") without polluting the template itself.

## Scope

**In:**
- `Template.matchStyle?: boolean` — additional flag on the existing template.
- `Project.customInstructions?: string` — new free-form field on the project.
- Prompt construction respects both new fields.
- Format cache key includes both new fields.
- UI: a checkbox under the template textarea + a new textarea below `TemplateEditor`.

**Out:**
- Per-recording overrides (project-level only).
- App-level defaults for either field (`AppSettings.defaultCustomInstructions` etc.).
- A "style-only" mode where the template is interpreted purely as a voice sample (rejected during brainstorming — Question 1, option B).
- Style-profile extraction or caching (the template document is passed inline at each format call as today).
- Any change to the recording-level format flow (`Recording.settings`).

## Data model

```ts
// src/types/storage.ts

export interface Template {
  source: TemplateSource;
  content: string;
  sourcePath?: string | null;
  matchStyle?: boolean;        // NEW; absent → false
}

export interface Project {
  id: string;
  name: string;
  createdAt: number;
  updatedAt: number;
  template: Template;
  defaultSettings: AppSettings;
  customInstructions?: string; // NEW; absent → ""
}
```

**Why `Template.matchStyle` and not `Project.matchStyle`?** The flag governs how the template is used. Storing it with the template keeps the concern local; if a future feature ships per-recording template overrides, the flag travels with the template content.

**Why `Project.customInstructions` and not `Template.customInstructions`?** Custom instructions are decoupled from the template — they're rules about the output, not modifications to the template's structure. Storing them on the project mirrors how `defaultSettings` already lives there.

**Defaults / backwards compatibility.** Both fields are optional. Loading a project saved before this change works unchanged: `matchStyle` resolves to `false`, `customInstructions` to `""`. Saving: write `matchStyle: true` only when true; omit the key when false to keep storage tidy. Same convention for `customInstructions` — omit when empty.

## Prompt changes

**File:** `src-tauri/src/commands.rs`, function `build_prompt` (currently at line 892).

### Signature

```rust
fn build_prompt(
    transcript: &str,
    template: &str,
    match_style: bool,
    custom_instructions: &str,
) -> String
```

Callers at lines 677 and 770 pass through both new arguments from the recording's project state.

### Base prompt change

Today's step 3 reads:

> 3. Produce a clean, well-structured document that strictly follows the layout and style of the FORMAT TEMPLATE below.

Change to:

> 3. Produce a clean, well-structured document that strictly follows the **layout** of the FORMAT TEMPLATE below.

Dropping "and style" makes the new flag's effect crisply additive: today's behavior approximates style-on, but the rephrased base prompt establishes a cleaner layout-only baseline. Style mirroring is opt-in via the flag.

### Style-match clause (when `match_style == true`)

Append after step 3:

> 4. **Match the writing style of the FORMAT TEMPLATE.** Mirror its tone (formal/casual), sentence rhythm, vocabulary, and any voice quirks (first-person/third-person, verbosity, jargon level). Apply this style to all prose in your output, while still drawing factual content only from the transcript.

When `match_style == false`, this step is omitted and subsequent step numbers stay at their current values.

### Custom-instructions block (when `custom_instructions` is non-empty after `trim`)

Insert a new section between FORMAT TEMPLATE and RAW TRANSCRIPT:

```
---
ADDITIONAL INSTRUCTIONS:
{custom_instructions}
---
```

And add a final numbered step (renumbering as needed):

> N. Follow the ADDITIONAL INSTRUCTIONS strictly. They override defaults where they conflict (e.g., length, voice, locale). They do not introduce facts not present in the transcript.

When `custom_instructions` is empty/whitespace-only, omit both the block and the step.

### Numbering

The base prompt numbers steps 1–5 today (1 read, 2 extract, 3 produce, 4 don't invent, 5 output only). With both flags off, the prompt is identical to today except for the "and style" → "" rephrase in step 3. With `match_style` on, the new style-match step slots in after step 3 and remaining steps shift. With `custom_instructions` set, the override step slots in after step 5. Step numbers are illustrative — the implementation can renumber dynamically with a small helper rather than maintaining four prompt variants.

## UI changes

**File:** `src/components/TemplateEditor.tsx` (existing).

After the `<Textarea>` block and before the `Loaded:` filename line, add a row:

```tsx
<label className="flex items-center gap-2 text-footnote text-label-secondary cursor-pointer select-none mt-1">
  <Checkbox
    size="sm"
    checked={template.matchStyle ?? false}
    onCheckedChange={(c) =>
      onChange({ ...template, matchStyle: Boolean(c) })
    }
  />
  Match writing style of this template
</label>
<p className="text-footnote text-label-tertiary -mt-0.5 ml-6">
  When enabled, the LLM also mirrors the template's tone and voice in the output.
</p>
```

The `ml-6` aligns the helper text under the checkbox label (not under the checkbox itself).

**New file or new section:** an `<InstructionsEditor>` block below `<TemplateEditor>` in `ProjectView`. Implementation choice: keep it inline in `ProjectView` if simple (a labeled textarea), or extract as a sibling component if it grows. Default: inline, no new file.

Sketch:

```tsx
<div className="flex flex-col gap-2">
  <label className="text-footnote font-medium text-label-secondary uppercase tracking-wider">
    Additional Instructions
  </label>
  <Textarea
    className="h-24 p-3 scrollable"
    placeholder="e.g., Use British English. Keep under 500 words. Skip empty sections."
    value={project.customInstructions ?? ""}
    onChange={(e) => onProjectChange({ ...project, customInstructions: e.target.value })}
    spellCheck
  />
</div>
```

`spellCheck` is on (unlike the template editor, which is `false` because templates often have placeholders). The textarea has body-text styling, not monospace — this is prose instructions, not template content.

**Layout.** The existing `ProjectView` likely renders `TemplateEditor` in a vertical scroll region. The new instructions block sits as the next sibling — same parent, no nesting. Spacing follows the existing rhythm (`gap-4` or whatever the parent uses).

## Cache invalidation

**File:** `src-tauri/src/commands.rs`, function `cache_key` (at line 434).

Today: `cache_key(transcript, template, model)`.

Change to: `cache_key(transcript, template, model, match_style, custom_instructions)`.

The two new args are concatenated into the hashed key after the existing fields. Existing cache entries become unmatched (effectively invalidated) for projects that are loaded with default values — that's correct, because the prompt is changing under them. Users who never touch the new flags will see one cache miss on their next format run, then steady state.

## Testing

**Rust (`commands.rs`):**

- Extend the existing format-pipeline tests. The base-prompt rephrase ("and style" → "") changes the prompt string; update any assertion that grepped for the old phrase.
- New unit tests on `build_prompt` (these may need to be added if they don't exist today):
  - `build_prompt_omits_style_clause_when_flag_off`: asserts the output does NOT contain "Match the writing style".
  - `build_prompt_includes_style_clause_when_flag_on`: asserts the output contains "Match the writing style of the FORMAT TEMPLATE".
  - `build_prompt_omits_instructions_section_when_empty`: asserts no `ADDITIONAL INSTRUCTIONS:` block.
  - `build_prompt_includes_instructions_section_when_set`: asserts `ADDITIONAL INSTRUCTIONS:` block is present and contains the user's text verbatim.
  - `build_prompt_strips_whitespace_only_instructions`: passing `"   \n  "` for instructions is treated as empty.
- `cache_key` test: two calls with the same inputs except `match_style` differ produce different keys; same for `custom_instructions`.

**Frontend:**

- `TemplateEditor` test (extend if exists, else add): toggle the checkbox, assert `onChange` called with `matchStyle: true`.
- `ProjectView` test: typing in the instructions textarea calls the project-update handler with the new string.

**Manual smoke (post-implementation):**

- Load an existing project saved before this change → `matchStyle` shows unchecked, instructions empty. Run a format → output matches prior behavior modulo the "and style" wording fix.
- Toggle `matchStyle` on, re-format the same recording → output is regenerated (cache miss) with style-mirroring applied.
- Add custom instructions ("Use British English") → re-format → output uses British spellings.

## Migration

No explicit migration step. The two new fields are optional with safe defaults; deserializing a pre-change project leaves both unset, and the runtime treats both as off/empty. Saving a project that has not interacted with the new fields produces the same JSON as before (the optional keys are omitted).

## Done criteria

- All existing frontend tests + new TemplateEditor test pass; `tsc --noEmit` clean.
- All existing Rust tests + new `build_prompt` and `cache_key` tests pass.
- An existing project loaded from disk renders without errors and behaves identically to before when the new flags are at their defaults.
- The two manual smoke checks above produce the expected qualitative differences in output.

## References

- Brainstorm transcript: this conversation.
- Existing prompt construction: `src-tauri/src/commands.rs:892-916`.
- Existing format cache key: `src-tauri/src/commands.rs:434`.
- Existing template UI: `src/components/TemplateEditor.tsx`.
- Storage types: `src/types/storage.ts`.
