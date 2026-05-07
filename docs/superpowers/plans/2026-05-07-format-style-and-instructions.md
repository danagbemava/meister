# Format Style + Custom Instructions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a project-level "Match writing style" checkbox on the format template plus a free-form "Additional Instructions" textarea, both threaded through the LLM format prompt and the format cache key.

**Architecture:** SQLite migration adds two columns to `projects` (`template_match_style INTEGER`, `custom_instructions TEXT`). Rust `Template` and `Project` structs grow matching fields with serde defaults. `build_prompt` takes two new args and renders extra clauses when set. `cache_key` includes both new args. `run_format`/`run_format_core` and the orchestrator pass the values through from the project. Frontend types and two UI surfaces (`TemplateEditor`, `ProjectView`) round-trip the new fields through the existing `update_project` Tauri command.

**Tech Stack:** Rust 2021 (Tauri 2, rusqlite), TypeScript (React, Vite), Vitest + Testing Library, SQLite (`PRAGMA user_version` migrations).

**Spec:** `docs/superpowers/specs/2026-05-07-format-style-and-instructions-design.md`

---

## File Structure

**New files:** none.

**Modified files (Rust):**
- `src-tauri/src/storage/schema.rs` — bump `CURRENT_VERSION`, add `migrate_v2_to_v3`.
- `src-tauri/src/storage/types.rs` — add `match_style` to `Template`, `custom_instructions` to `Project`.
- `src-tauri/src/storage/projects.rs` — extend insert/update/select SQL + `row_to_project` mapper.
- `src-tauri/src/commands.rs` — `build_prompt`, `cache_key`, `run_format`, `run_format_core`, `create_project` initializer + the new fields.
- `src-tauri/src/orchestrator.rs` — pass `project.match_style` and `project.custom_instructions` to `run_format`.

**Modified files (frontend):**
- `src/types/storage.ts` — add the two optional fields.
- `src/components/TemplateEditor.tsx` — checkbox + helper text under the textarea.
- `src/components/ProjectView.tsx` — "Additional Instructions" labeled textarea below the `TemplateEditor`.

**Sequencing:** schema migration → Rust types → SQL → prompt/cache (TDD) → wire through orchestrator → TS types → UI (TemplateEditor then ProjectView) → verification.

---

## Task 1: SQLite migration v2 → v3

**Files:**
- Modify: `src-tauri/src/storage/schema.rs`

- [ ] **Step 1: Bump `CURRENT_VERSION` and register the migration**

In `src-tauri/src/storage/schema.rs`, change:

```rust
const CURRENT_VERSION: i32 = 2;
```

to:

```rust
const CURRENT_VERSION: i32 = 3;
```

Then in the `migrate` function, find:

```rust
        match version {
            0 => migrate_v0_to_v1(conn)?,
            1 => migrate_v1_to_v2(conn)?,
            n => return Err(format!("no migration from version {n}")),
        }
```

Replace with:

```rust
        match version {
            0 => migrate_v0_to_v1(conn)?,
            1 => migrate_v1_to_v2(conn)?,
            2 => migrate_v2_to_v3(conn)?,
            n => return Err(format!("no migration from version {n}")),
        }
```

- [ ] **Step 2: Add the migration function**

Append to the bottom of `src-tauri/src/storage/schema.rs`:

```rust
fn migrate_v2_to_v3(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        r#"
        ALTER TABLE projects ADD COLUMN template_match_style INTEGER NOT NULL DEFAULT 0;
        ALTER TABLE projects ADD COLUMN custom_instructions TEXT NOT NULL DEFAULT '';
        "#,
    )
    .map_err(|e| format!("apply v3 schema: {e}"))
}
```

- [ ] **Step 3: Build to verify the migration compiles**

Run: `cd src-tauri && cargo build`
Expected: builds clean. (Existing call sites of the `Project`/`Template` struct will still compile because the struct shape hasn't changed yet — that's Task 2.)

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/storage/schema.rs
git commit -m "feat(storage): migrate v2→v3 — match_style + custom_instructions columns"
```

---

## Task 2: Extend Rust storage types

**Files:**
- Modify: `src-tauri/src/storage/types.rs`

- [ ] **Step 1: Add `match_style` to `Template` and `custom_instructions` to `Project`**

In `src-tauri/src/storage/types.rs`, find:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    pub source: TemplateSource,
    pub content: String,
    #[serde(rename = "sourcePath")]
    pub source_path: Option<String>,
}
```

Replace with:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    pub source: TemplateSource,
    pub content: String,
    #[serde(rename = "sourcePath")]
    pub source_path: Option<String>,
    #[serde(rename = "matchStyle", default, skip_serializing_if = "is_false")]
    pub match_style: bool,
}
```

Find:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
    pub template: Template,
    #[serde(rename = "defaultSettings")]
    pub default_settings: AppSettings,
}
```

Replace with:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
    pub template: Template,
    #[serde(rename = "defaultSettings")]
    pub default_settings: AppSettings,
    #[serde(rename = "customInstructions", default, skip_serializing_if = "String::is_empty")]
    pub custom_instructions: String,
}
```

At the bottom of the file, add the helper:

```rust
fn is_false(b: &bool) -> bool {
    !*b
}
```

- [ ] **Step 2: Build to expose all stale call sites**

Run: `cd src-tauri && cargo build`
Expected: errors about missing fields in `Project { ... }` and `Template { ... }` initializers — these are fixed in Tasks 3 and Task 6 (commands.rs `create_project`). The orchestrator and other consumers will compile because they only read existing fields.

Note any error locations; the plan covers `commands.rs::create_project` (Task 6) and `storage/projects.rs::row_to_project` (Task 3). If new error locations appear, add the missing field initializer with the type's default (`false` / `String::new()`).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/storage/types.rs
git commit -m "feat(storage): add Template.match_style + Project.custom_instructions"
```

---

## Task 3: Extend `projects.rs` SQL

**Files:**
- Modify: `src-tauri/src/storage/projects.rs`

- [ ] **Step 1: Update INSERT to include the two new columns**

In `src-tauri/src/storage/projects.rs`, find:

```rust
    conn.execute(
        "INSERT INTO projects \
         (id, name, created_at, updated_at, template_source, template_content, template_source_path, default_settings) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            p.id, p.name, p.created_at, p.updated_at,
            p.template.source.as_str(),
            p.template.content,
            p.template.source_path,
            settings_json,
        ],
    )
    .map_err(|e| format!("insert project: {e}"))?;
```

Replace with:

```rust
    conn.execute(
        "INSERT INTO projects \
         (id, name, created_at, updated_at, template_source, template_content, template_source_path, default_settings, template_match_style, custom_instructions) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            p.id, p.name, p.created_at, p.updated_at,
            p.template.source.as_str(),
            p.template.content,
            p.template.source_path,
            settings_json,
            p.template.match_style as i32,
            p.custom_instructions,
        ],
    )
    .map_err(|e| format!("insert project: {e}"))?;
```

- [ ] **Step 2: Update UPDATE to include the two new columns**

Find:

```rust
    let n = conn.execute(
        "UPDATE projects SET \
         name = ?2, updated_at = ?3, template_source = ?4, template_content = ?5, \
         template_source_path = ?6, default_settings = ?7 \
         WHERE id = ?1",
        params![
            p.id, p.name, p.updated_at,
            p.template.source.as_str(),
            p.template.content,
            p.template.source_path,
            settings_json,
        ],
    )
    .map_err(|e| format!("update project: {e}"))?;
```

Replace with:

```rust
    let n = conn.execute(
        "UPDATE projects SET \
         name = ?2, updated_at = ?3, template_source = ?4, template_content = ?5, \
         template_source_path = ?6, default_settings = ?7, \
         template_match_style = ?8, custom_instructions = ?9 \
         WHERE id = ?1",
        params![
            p.id, p.name, p.updated_at,
            p.template.source.as_str(),
            p.template.content,
            p.template.source_path,
            settings_json,
            p.template.match_style as i32,
            p.custom_instructions,
        ],
    )
    .map_err(|e| format!("update project: {e}"))?;
```

- [ ] **Step 3: Update both SELECT statements to read the new columns**

Find (in `get`):

```rust
            "SELECT id, name, created_at, updated_at, template_source, \
             template_content, template_source_path, default_settings \
             FROM projects WHERE id = ?1",
```

Replace with:

```rust
            "SELECT id, name, created_at, updated_at, template_source, \
             template_content, template_source_path, default_settings, \
             template_match_style, custom_instructions \
             FROM projects WHERE id = ?1",
```

Find (in `list`):

```rust
            "SELECT id, name, created_at, updated_at, template_source, \
             template_content, template_source_path, default_settings \
             FROM projects ORDER BY created_at ASC",
```

Replace with:

```rust
            "SELECT id, name, created_at, updated_at, template_source, \
             template_content, template_source_path, default_settings, \
             template_match_style, custom_instructions \
             FROM projects ORDER BY created_at ASC",
```

- [ ] **Step 4: Update `row_to_project` to read the new columns**

Find:

```rust
fn row_to_project(row: &rusqlite::Row) -> Result<Project, String> {
    use crate::storage::{Template, TemplateSource};
    let source_str: String = row.get(4).map_err(|e| format!("source: {e}"))?;
    let source = match source_str.as_str() {
        "inline" => TemplateSource::Inline,
        "file" => TemplateSource::File,
        other => return Err(format!("unknown template_source: {other}")),
    };
    let settings_json: String = row.get(7).map_err(|e| format!("settings: {e}"))?;
    let default_settings = serde_json::from_str(&settings_json)
        .map_err(|e| format!("parse settings: {e}"))?;
    Ok(Project {
        id: row.get(0).map_err(|e| format!("id: {e}"))?,
        name: row.get(1).map_err(|e| format!("name: {e}"))?,
        created_at: row.get(2).map_err(|e| format!("created_at: {e}"))?,
        updated_at: row.get(3).map_err(|e| format!("updated_at: {e}"))?,
        template: Template {
            source,
            content: row.get(5).map_err(|e| format!("content: {e}"))?,
            source_path: row.get(6).map_err(|e| format!("source_path: {e}"))?,
        },
        default_settings,
    })
}
```

Replace with:

```rust
fn row_to_project(row: &rusqlite::Row) -> Result<Project, String> {
    use crate::storage::{Template, TemplateSource};
    let source_str: String = row.get(4).map_err(|e| format!("source: {e}"))?;
    let source = match source_str.as_str() {
        "inline" => TemplateSource::Inline,
        "file" => TemplateSource::File,
        other => return Err(format!("unknown template_source: {other}")),
    };
    let settings_json: String = row.get(7).map_err(|e| format!("settings: {e}"))?;
    let default_settings = serde_json::from_str(&settings_json)
        .map_err(|e| format!("parse settings: {e}"))?;
    let match_style_int: i64 = row.get(8).map_err(|e| format!("match_style: {e}"))?;
    let custom_instructions: String = row.get(9).map_err(|e| format!("custom_instructions: {e}"))?;
    Ok(Project {
        id: row.get(0).map_err(|e| format!("id: {e}"))?,
        name: row.get(1).map_err(|e| format!("name: {e}"))?,
        created_at: row.get(2).map_err(|e| format!("created_at: {e}"))?,
        updated_at: row.get(3).map_err(|e| format!("updated_at: {e}"))?,
        template: Template {
            source,
            content: row.get(5).map_err(|e| format!("content: {e}"))?,
            source_path: row.get(6).map_err(|e| format!("source_path: {e}"))?,
            match_style: match_style_int != 0,
        },
        default_settings,
        custom_instructions,
    })
}
```

- [ ] **Step 5: Build to verify compile**

Run: `cd src-tauri && cargo build`
Expected: One remaining error: `commands.rs::create_project` constructs `Project { ... }` without the new field. That's fixed in Task 6. If only that error remains, proceed.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/storage/projects.rs
git commit -m "feat(storage): persist match_style + custom_instructions on projects"
```

---

## Task 4: Update `build_prompt` (TDD)

**Files:**
- Modify: `src-tauri/src/commands.rs`

- [ ] **Step 1: Write failing tests**

In `src-tauri/src/commands.rs`, find the existing `mod tests` (likely near the bottom of the file). Append these tests inside it (do not create a new module):

```rust
    #[test]
    fn build_prompt_omits_style_clause_when_flag_off() {
        let p = build_prompt("transcript", "tpl", false, "");
        assert!(!p.contains("Match the writing style"), "should not include style clause");
    }

    #[test]
    fn build_prompt_includes_style_clause_when_flag_on() {
        let p = build_prompt("transcript", "tpl", true, "");
        assert!(
            p.contains("Match the writing style of the FORMAT TEMPLATE"),
            "missing style clause:\n{p}"
        );
    }

    #[test]
    fn build_prompt_omits_instructions_section_when_empty() {
        let p = build_prompt("transcript", "tpl", false, "");
        assert!(!p.contains("ADDITIONAL INSTRUCTIONS"), "should not include instructions block");
    }

    #[test]
    fn build_prompt_strips_whitespace_only_instructions() {
        let p = build_prompt("transcript", "tpl", false, "   \n  \t");
        assert!(!p.contains("ADDITIONAL INSTRUCTIONS"), "whitespace-only must be treated as empty");
    }

    #[test]
    fn build_prompt_includes_instructions_section_when_set() {
        let p = build_prompt("transcript", "tpl", false, "Use British English.");
        assert!(p.contains("ADDITIONAL INSTRUCTIONS:"), "missing instructions header");
        assert!(p.contains("Use British English."), "missing instructions body");
        assert!(
            p.contains("Follow the ADDITIONAL INSTRUCTIONS strictly"),
            "missing instructions step"
        );
    }

    #[test]
    fn build_prompt_base_uses_layout_only_phrasing() {
        // The base prompt should say "follow the layout" (singular), not "layout and style".
        let p = build_prompt("t", "tpl", false, "");
        assert!(p.contains("strictly follows the layout of the FORMAT TEMPLATE"), "base phrasing");
        assert!(
            !p.contains("layout and style of the FORMAT TEMPLATE"),
            "old conflated phrasing must be removed"
        );
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test build_prompt 2>&1 | grep -E "test |result"`
Expected: 6 tests fail to compile (`build_prompt` takes 2 args, not 4).

- [ ] **Step 3: Update `build_prompt`**

In `src-tauri/src/commands.rs`, find:

```rust
fn build_prompt(transcript: &str, template: &str) -> String {
    format!(
        r#"You are an expert document formatter. You will be given a raw meeting/recording transcript and a format template.

Your job:
1. Carefully read the entire transcript.
2. Identify and extract all key information: participants, topics discussed, decisions made, action items, dates, and any other relevant details.
3. Produce a clean, well-structured document that strictly follows the layout and style of the FORMAT TEMPLATE below.
4. Do not invent information not present in the transcript. If a section has no content, write "N/A" or omit it gracefully.
5. Output only the final formatted document — no preamble, no explanation.

---
FORMAT TEMPLATE:
{template}
---

RAW TRANSCRIPT:
{transcript}
---

Produce the formatted document now:"#,
        template = template,
        transcript = transcript,
    )
}
```

Replace with:

```rust
fn build_prompt(
    transcript: &str,
    template: &str,
    match_style: bool,
    custom_instructions: &str,
) -> String {
    let instructions_trimmed = custom_instructions.trim();

    // Build the numbered job steps in order. Steps with optional clauses
    // get appended conditionally so step numbers stay contiguous.
    let mut steps: Vec<String> = vec![
        "Carefully read the entire transcript.".into(),
        "Identify and extract all key information: participants, topics discussed, decisions made, action items, dates, and any other relevant details.".into(),
        "Produce a clean, well-structured document that strictly follows the layout of the FORMAT TEMPLATE below.".into(),
    ];
    if match_style {
        steps.push(
            "Match the writing style of the FORMAT TEMPLATE. Mirror its tone (formal/casual), sentence rhythm, vocabulary, and any voice quirks (first-person/third-person, verbosity, jargon level). Apply this style to all prose in your output, while still drawing factual content only from the transcript.".into(),
        );
    }
    steps.push(
        "Do not invent information not present in the transcript. If a section has no content, write \"N/A\" or omit it gracefully.".into(),
    );
    steps.push(
        "Output only the final formatted document — no preamble, no explanation.".into(),
    );
    if !instructions_trimmed.is_empty() {
        steps.push(
            "Follow the ADDITIONAL INSTRUCTIONS strictly. They override defaults where they conflict (e.g., length, voice, locale). Do not introduce facts not present in the transcript.".into(),
        );
    }
    let numbered_steps = steps
        .iter()
        .enumerate()
        .map(|(i, s)| format!("{}. {}", i + 1, s))
        .collect::<Vec<_>>()
        .join("\n");

    let instructions_block = if instructions_trimmed.is_empty() {
        String::new()
    } else {
        format!(
            "\n---\nADDITIONAL INSTRUCTIONS:\n{instructions_trimmed}\n",
        )
    };

    format!(
        r#"You are an expert document formatter. You will be given a raw meeting/recording transcript and a format template.

Your job:
{numbered_steps}

---
FORMAT TEMPLATE:
{template}
---{instructions_block}
RAW TRANSCRIPT:
{transcript}
---

Produce the formatted document now:"#,
        numbered_steps = numbered_steps,
        template = template,
        instructions_block = instructions_block,
        transcript = transcript,
    )
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test build_prompt 2>&1 | grep -E "test |result"`
Expected: 6 tests pass. Compile errors at the call sites of `build_prompt` (lines 677 and 770) remain — those are fixed in Task 6. If the build error is exclusively at those two call sites, proceed.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "feat(format): build_prompt supports match_style + custom_instructions"
```

---

## Task 5: Update `cache_key` (TDD)

**Files:**
- Modify: `src-tauri/src/commands.rs`

- [ ] **Step 1: Write failing test extension**

In `src-tauri/src/commands.rs`, find the existing `cache_key_differs_on_any_input_change` test:

```rust
    fn cache_key_differs_on_any_input_change() {
        let base = cache_key("t", "tpl", "m");
        assert_ne!(base, cache_key("t2", "tpl", "m"));
        assert_ne!(base, cache_key("t", "tpl2", "m"));
        assert_ne!(base, cache_key("t", "tpl", "m2"));
```

Replace with (note the signature change):

```rust
    fn cache_key_differs_on_any_input_change() {
        let base = cache_key("t", "tpl", "m", false, "");
        assert_ne!(base, cache_key("t2", "tpl", "m", false, ""));
        assert_ne!(base, cache_key("t", "tpl2", "m", false, ""));
        assert_ne!(base, cache_key("t", "tpl", "m2", false, ""));
        assert_ne!(base, cache_key("t", "tpl", "m", true, ""));
        assert_ne!(base, cache_key("t", "tpl", "m", false, "extra"));
```

Also update `cache_key_is_stable_for_same_inputs`:

```rust
    fn cache_key_is_stable_for_same_inputs() {
        let a = cache_key("transcript", "template", "gemma3");
        let b = cache_key("transcript", "template", "gemma3");
```

Replace with:

```rust
    fn cache_key_is_stable_for_same_inputs() {
        let a = cache_key("transcript", "template", "gemma3", false, "");
        let b = cache_key("transcript", "template", "gemma3", false, "");
```

- [ ] **Step 2: Run tests to verify they fail to compile**

Run: `cd src-tauri && cargo test cache_key 2>&1 | grep -E "test |result|error"`
Expected: compile errors (`cache_key` takes 3 args, not 5).

- [ ] **Step 3: Update `cache_key`**

In `src-tauri/src/commands.rs`, find:

```rust
/// SHA-256 over `transcript || 0x00 || template || 0x00 || model` → hex.
pub(crate) fn cache_key(transcript: &str, template: &str, model: &str) -> String {
    let mut h = Sha256::new();
    h.update(transcript.as_bytes());
    h.update([0u8]);
    h.update(template.as_bytes());
    h.update([0u8]);
    h.update(model.as_bytes());
    let digest = h.finalize();
    let mut out = String::with_capacity(64);
    for b in digest.iter() {
        out.push_str(&format!("{b:02x}"));
    }
    out
}
```

Replace with:

```rust
/// SHA-256 over `transcript || 0x00 || template || 0x00 || model || 0x00 ||
/// match_style_byte || 0x00 || custom_instructions` → hex.
pub(crate) fn cache_key(
    transcript: &str,
    template: &str,
    model: &str,
    match_style: bool,
    custom_instructions: &str,
) -> String {
    let mut h = Sha256::new();
    h.update(transcript.as_bytes());
    h.update([0u8]);
    h.update(template.as_bytes());
    h.update([0u8]);
    h.update(model.as_bytes());
    h.update([0u8]);
    h.update([if match_style { 1u8 } else { 0u8 }]);
    h.update([0u8]);
    h.update(custom_instructions.as_bytes());
    let digest = h.finalize();
    let mut out = String::with_capacity(64);
    for b in digest.iter() {
        out.push_str(&format!("{b:02x}"));
    }
    out
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test cache_key 2>&1 | grep -E "test |result"`
Expected: both cache_key tests pass. Compile errors at the existing call site (line 756) remain — fixed in Task 6.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "feat(format): cache_key includes match_style + custom_instructions"
```

---

## Task 6: Wire `run_format` and `run_format_core` through

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/orchestrator.rs`

- [ ] **Step 1: Update `run_format_core` signature + call sites within**

In `src-tauri/src/commands.rs`, find:

```rust
pub(crate) async fn run_format_core(
    transcript: &str,
    template: &str,
    ollama_url: &str,
    model: &str,
    cache_dir: &Path,
    on_progress: Arc<dyn Fn(FormatProgress) + Send + Sync>,
    cancel: Option<crate::cancellation::CancelToken>,
) -> Result<String, String> {
    use futures_util::{stream, StreamExt};
    use std::sync::atomic::AtomicUsize;

    let key = cache_key(transcript, template, model);
```

Replace with:

```rust
pub(crate) async fn run_format_core(
    transcript: &str,
    template: &str,
    ollama_url: &str,
    model: &str,
    match_style: bool,
    custom_instructions: &str,
    cache_dir: &Path,
    on_progress: Arc<dyn Fn(FormatProgress) + Send + Sync>,
    cancel: Option<crate::cancellation::CancelToken>,
) -> Result<String, String> {
    use futures_util::{stream, StreamExt};
    use std::sync::atomic::AtomicUsize;

    let key = cache_key(transcript, template, model, match_style, custom_instructions);
```

Within the same function, find:

```rust
        let prompt = build_prompt(&cleaned, template);
```

Replace with:

```rust
        let prompt = build_prompt(&cleaned, template, match_style, custom_instructions);
```

Also find (in the map-reduce branch — the `stitched_notes` call):

```rust
    let prompt = build_prompt(stitched_notes, template);
```

Replace with:

```rust
    let prompt = build_prompt(stitched_notes, template, match_style, custom_instructions);
```

(There are two `build_prompt` call sites inside this file region — one inside `run_format_core` for the single-pass branch around line 770, and one inside the reduce helper around line 677. Update both. The reduce helper takes `template` already; thread `match_style` and `custom_instructions` through it the same way — see Step 2.)

- [ ] **Step 2: Update the `reduce_pass` helper signature**

In `src-tauri/src/commands.rs`, find the function `reduce_pass` (around line 669):

```rust
async fn reduce_pass(
    ollama_url: &str,
    model: &str,
    stitched_notes: &str,
    template: &str,
    on_progress: Arc<dyn Fn(FormatProgress) + Send + Sync>,
    cancel: Option<crate::cancellation::CancelToken>,
) -> Result<String, String> {
    let prompt = build_prompt(stitched_notes, template);
```

Replace with:

```rust
async fn reduce_pass(
    ollama_url: &str,
    model: &str,
    stitched_notes: &str,
    template: &str,
    match_style: bool,
    custom_instructions: &str,
    on_progress: Arc<dyn Fn(FormatProgress) + Send + Sync>,
    cancel: Option<crate::cancellation::CancelToken>,
) -> Result<String, String> {
    let prompt = build_prompt(stitched_notes, template, match_style, custom_instructions);
```

Update the call site (around line 839 inside `run_format_core`):

```rust
        let reduced = reduce_pass(ollama_url, model, &stitched, template, on_progress.clone(), cancel.clone()).await?;
```

Replace with:

```rust
        let reduced = reduce_pass(ollama_url, model, &stitched, template, match_style, custom_instructions, on_progress.clone(), cancel.clone()).await?;
```

- [ ] **Step 3: Update `run_format` (the AppHandle wrapper) signature**

Find:

```rust
pub async fn run_format(
    app: &AppHandle,
    transcript: &str,
    template_content: &str,
    ollama_url: &str,
    model: &str,
    cancel: Option<crate::cancellation::CancelToken>,
) -> Result<String, String> {
    let cache_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app_data_dir: {e}"))?
        .join("format-cache");

    let app_handle = app.clone();
    let on_progress: Arc<dyn Fn(FormatProgress) + Send + Sync> = Arc::new(move |p| {
        let _ = app_handle.emit("format-progress", p);
    });

    run_format_core(
        transcript,
        template_content,
        ollama_url,
        model,
        &cache_dir,
        on_progress,
        cancel,
    )
    .await
}
```

Replace with:

```rust
pub async fn run_format(
    app: &AppHandle,
    transcript: &str,
    template_content: &str,
    ollama_url: &str,
    model: &str,
    match_style: bool,
    custom_instructions: &str,
    cancel: Option<crate::cancellation::CancelToken>,
) -> Result<String, String> {
    let cache_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app_data_dir: {e}"))?
        .join("format-cache");

    let app_handle = app.clone();
    let on_progress: Arc<dyn Fn(FormatProgress) + Send + Sync> = Arc::new(move |p| {
        let _ = app_handle.emit("format-progress", p);
    });

    run_format_core(
        transcript,
        template_content,
        ollama_url,
        model,
        match_style,
        custom_instructions,
        &cache_dir,
        on_progress,
        cancel,
    )
    .await
}
```

- [ ] **Step 4: Fix `create_project` initializer**

Find:

```rust
        let p = Project {
            id,
            name,
            created_at: now,
            updated_at: now,
            template,
            default_settings,
        };
```

Replace with:

```rust
        let p = Project {
            id,
            name,
            created_at: now,
            updated_at: now,
            template,
            default_settings,
            custom_instructions: String::new(),
        };
```

(`template` carries `match_style` from the frontend payload via serde; `custom_instructions` defaults to empty for new projects, settable later via `update_project`.)

- [ ] **Step 5: Update existing `run_format_core` test invocations**

In the same file's `mod tests`, find each call to `run_format_core(...)` (there are several — lines 1700, 1739, 1768, 1769, 1780, 1781). Add `false, ""` for the two new args after `model` and before `cache_dir` / `tmp.path()`. Example:

Find:

```rust
        let out = run_format_core(
            "t", "tpl", &mock.base_url, "gemma3", tmp.path(), noop_progress(), None,
        ).await.unwrap();
```

Replace with:

```rust
        let out = run_format_core(
            "t", "tpl", &mock.base_url, "gemma3", false, "", tmp.path(), noop_progress(), None,
        ).await.unwrap();
```

(Apply the same pattern to every `run_format_core` call site in tests. The `false, ""` placement is between the model name and the cache directory path.)

- [ ] **Step 6: Update orchestrator call site**

In `src-tauri/src/orchestrator.rs`, find:

```rust
                let formatted = run_format(
                    &app,
                    &transcript,
                    &project.template.content,
                    &rec.settings.ollama_url,
                    &rec.settings.model,
                    Some(cancel.clone()),
                )
                .await?;
```

Replace with:

```rust
                let formatted = run_format(
                    &app,
                    &transcript,
                    &project.template.content,
                    &rec.settings.ollama_url,
                    &rec.settings.model,
                    project.template.match_style,
                    &project.custom_instructions,
                    Some(cancel.clone()),
                )
                .await?;
```

- [ ] **Step 7: Build + test**

Run: `cd src-tauri && cargo build`
Expected: clean.

Run: `cd src-tauri && cargo test 2>&1 | grep "test result"`
Expected: all suites pass.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/orchestrator.rs
git commit -m "feat(format): thread match_style + custom_instructions through pipeline"
```

---

## Task 7: Update TS storage types

**Files:**
- Modify: `src/types/storage.ts`

- [ ] **Step 1: Add the optional fields**

In `src/types/storage.ts`, find:

```ts
export interface Template {
  source: TemplateSource;
  content: string;
  sourcePath?: string | null;
}
```

Replace with:

```ts
export interface Template {
  source: TemplateSource;
  content: string;
  sourcePath?: string | null;
  matchStyle?: boolean;
}
```

Find:

```ts
export interface Project {
  id: string;
  name: string;
  createdAt: number;
  updatedAt: number;
  template: Template;
  defaultSettings: AppSettings;
}
```

Replace with:

```ts
export interface Project {
  id: string;
  name: string;
  createdAt: number;
  updatedAt: number;
  template: Template;
  defaultSettings: AppSettings;
  customInstructions?: string;
}
```

- [ ] **Step 2: Type-check**

Run: `npx tsc --noEmit`
Expected: clean — both fields are optional, so existing call sites stay valid.

- [ ] **Step 3: Commit**

```bash
git add src/types/storage.ts
git commit -m "feat(types): add Template.matchStyle + Project.customInstructions"
```

---

## Task 8: TemplateEditor — checkbox + helper text

**Files:**
- Modify: `src/components/TemplateEditor.tsx`

- [ ] **Step 1: Update imports**

At the top of `src/components/TemplateEditor.tsx`, find:

```tsx
import { Button, Textarea } from "./ui";
```

Replace with:

```tsx
import { Button, Checkbox, Textarea } from "./ui";
```

- [ ] **Step 2: Add the checkbox + helper text after the Textarea**

In `src/components/TemplateEditor.tsx`, find:

```tsx
        <Textarea
          ref={textRef}
          className="h-48 p-4 font-mono scrollable"
          placeholder={"# Meeting Minutes\nDate: [DATE]\n..."}
          value={template.content}
          onChange={(e) =>
            onChange({ source: "inline", content: e.target.value, sourcePath: null })
          }
          spellCheck={false}
        />
      </div>
      {template.sourcePath && (
        <p className="text-footnote text-label-tertiary truncate">
          Loaded: {filename(template.sourcePath)}
        </p>
      )}
    </div>
```

Replace with:

```tsx
        <Textarea
          ref={textRef}
          className="h-48 p-4 font-mono scrollable"
          placeholder={"# Meeting Minutes\nDate: [DATE]\n..."}
          value={template.content}
          onChange={(e) =>
            onChange({
              source: "inline",
              content: e.target.value,
              sourcePath: null,
              matchStyle: template.matchStyle,
            })
          }
          spellCheck={false}
        />
      </div>
      <div className="mt-1">
        <label className="flex items-center gap-2 text-footnote text-label-secondary cursor-pointer select-none">
          <Checkbox
            size="sm"
            checked={template.matchStyle ?? false}
            onCheckedChange={(c) =>
              onChange({ ...template, matchStyle: Boolean(c) })
            }
          />
          Match writing style of this template
        </label>
        <p className="text-footnote text-label-tertiary ml-6">
          When enabled, the LLM also mirrors the template's tone and voice in the output.
        </p>
      </div>
      {template.sourcePath && (
        <p className="text-footnote text-label-tertiary truncate">
          Loaded: {filename(template.sourcePath)}
        </p>
      )}
    </div>
```

The Textarea's `onChange` now preserves `matchStyle` (so editing template content doesn't accidentally clear the checkbox state). The new block sits between the textarea row and the optional "Loaded:" filename.

- [ ] **Step 3: Type-check**

Run: `npx tsc --noEmit`
Expected: clean.

- [ ] **Step 4: Run tests**

Run: `npx vitest run`
Expected: all existing tests pass. (No new test required; the behavior is small enough that the manual smoke in Task 11 covers it. If a `TemplateEditor.test.tsx` exists today, it should still pass since the existing onChange contract is preserved.)

- [ ] **Step 5: Commit**

```bash
git add src/components/TemplateEditor.tsx
git commit -m "feat(template): add Match writing style checkbox"
```

---

## Task 9: ProjectView — Additional Instructions textarea

**Files:**
- Modify: `src/components/ProjectView.tsx`

- [ ] **Step 1: Read the existing TemplateEditor block to find the insertion point**

Open `src/components/ProjectView.tsx`. The `TemplateEditor` is rendered around line 172. Note that `ProjectView` already has a `save` function pattern that calls `api.updateProject(...)` with the updated project; the new textarea calls the same `save` with an updated `customInstructions`.

- [ ] **Step 2: Update imports if needed**

Ensure `Textarea` is imported from `./ui`. Find the existing import line in `ProjectView.tsx` from `./ui` (if any) and add `Textarea` if missing. If there is no `./ui` import yet, add:

```tsx
import { Textarea } from "./ui";
```

(Combine with any existing `./ui` import.)

- [ ] **Step 3: Insert the Additional Instructions block**

After the `<TemplateEditor ... />` JSX block (around line 172-175), insert a new sibling block before the next existing section ("Default settings", around line 177):

```tsx
      <div className="flex flex-col gap-2">
        <label className="text-footnote font-medium text-label-secondary uppercase tracking-wider">
          Additional Instructions
        </label>
        <Textarea
          className="h-24 p-3 scrollable"
          placeholder="e.g., Use British English. Keep under 500 words. Skip empty sections."
          value={project.customInstructions ?? ""}
          onChange={(e) =>
            save({ ...project, customInstructions: e.target.value })
          }
          spellCheck
        />
      </div>
```

The `save` function is the existing handler used by `TemplateEditor`'s `onChange={(template) => save({ ...project, template })}`. If the function in this file has a different name (e.g., `updateProject`, `persist`), use that name instead — find it by searching for the existing TemplateEditor's `onChange` line.

- [ ] **Step 4: Type-check + tests**

Run: `npx tsc --noEmit && npx vitest run`
Expected: tsc clean; all tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/components/ProjectView.tsx
git commit -m "feat(project): add Additional Instructions textarea"
```

---

## Task 10: Final verification

**Files:** none (verification only).

- [ ] **Step 1: Full test sweep**

Run: `npx tsc --noEmit && npx vitest run && (cd src-tauri && cargo test)`
Expected: all green.

- [ ] **Step 2: Build production bundle**

Run: `npm run build`
Expected: build succeeds.

- [ ] **Step 3: Manual smoke (Tauri dev)**

Run: `npm run tauri dev`

Verify the following manually:

1. **Existing project loads.** Open an existing project from before this change. The "Match writing style" checkbox is unchecked. The "Additional Instructions" textarea is empty. Trigger Format on a recording — output is generated (cache miss, since the cache key now differs). Output quality is similar to before (modulo the "and style" → "layout only" wording change in the base prompt).

2. **Toggle match-style on.** Check the box. Re-format the same recording. Output is regenerated (different cache key). The output prose should mirror the template's voice more closely than before (qualitative — verify by eye).

3. **Add custom instructions.** Type "Use British English. Keep under 500 words." into the Additional Instructions textarea. Re-format. Output should: (a) use British spellings ("organisation", "behaviour"); (b) be ≤ 500 words.

4. **Persistence.** Close and reopen the app. The checkbox state and instructions text persist (loaded from SQLite v3 columns).

5. **New project.** Create a new project. The checkbox is unchecked, the textarea is empty by default.

6. **DB migration on existing install.** If you have a real `.app` install with prior data, launch it once. The schema migrates v2→v3 transparently; no data is lost; existing projects show the new fields with default values.

- [ ] **Step 4: Note any deviations**

If any manual smoke step revealed a deviation from the spec (UI placement off, persistence not working, prompt change has unexpected output effect), note it. Small UI tweaks can be folded into a single follow-up commit; behavioral surprises require revisiting the spec.

- [ ] **Step 5: No commit needed**

Verification-only task. If any cleanup is needed, commit it as:

```bash
git commit -m "chore(format): cleanup from icon-pass detour"
```

---

## Spec Coverage Self-Check

| Spec section / requirement | Implemented in |
|---|---|
| `Template.matchStyle` field | Task 2 (Rust), Task 7 (TS) |
| `Project.customInstructions` field | Task 2 (Rust), Task 7 (TS) |
| Backwards-compat: defaults on missing fields | Task 2 (`default` serde attr), Task 3 (SQL `DEFAULT`), Task 7 (optional types) |
| Storage migration v2→v3 | Task 1 |
| Template stays as layout, base prompt rephrase | Task 4 |
| Style-match clause when flag on | Task 4 |
| Additional Instructions block + step when set | Task 4 |
| Whitespace-only instructions treated as empty | Task 4 (test + impl) |
| Cache key includes both new fields | Task 5 |
| `run_format` / `run_format_core` plumbing | Task 6 |
| Orchestrator passes flags from project | Task 6 |
| `create_project` initializes empty `custom_instructions` | Task 6 |
| TemplateEditor checkbox + helper text | Task 8 |
| ProjectView Additional Instructions textarea | Task 9 |
| Manual smoke (load existing, toggle, type, persist, new project) | Task 10 |
| Done criteria (tests + tsc + manual checks) | Task 10 |
