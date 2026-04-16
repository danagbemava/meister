# whisper.cpp Swap — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the `openai-whisper` Python subprocess with native Rust bindings to `whisper.cpp` (whisper-rs crate, Metal + CoreML), bundling a default `base` model and adding a Settings-modal flow for downloading larger models on demand.

**Architecture:** New `src-tauri/src/transcription/` module owns whisper-rs lifecycle, model file management (paths, registry, download with cancellation), and a context cache. `commands.rs` shrinks to thin Tauri handlers. Frontend `SettingsStep.tsx` becomes data-driven from `list_available_models`, with a new `DownloadModelModal.tsx` gating non-bundled sizes. The `base` model and its CoreML encoder ship as Tauri resources, fetched at build time by a script.

**Tech Stack:** Rust (Tauri 2, whisper-rs, hound, reqwest, futures-util, tokio), TypeScript (React, Vite), bash (model fetch script).

**Spec:** `docs/superpowers/specs/2026-04-16-whisper-cpp-swap-design.md`

---

## Conventions

- All commands shown assume CWD = repo root (`/Users/nexus/projects/meister`) unless prefixed otherwise.
- Rust commands run from `src-tauri/`: e.g. `cd src-tauri && cargo test ...`.
- Frontend dev server runs via `npm run tauri dev` from repo root.
- Each task ends with a commit. Use the commit messages shown verbatim — they form a clean history reviewable in `git log --oneline`.
- Use `Result<T, String>` for new Rust code to match the existing `commands.rs` pattern. Do not introduce `anyhow` / `thiserror`.

---

## Task 1: Baseline commit of existing untracked code

The repo was just `git init`'d, so all the existing app code (src/, src-tauri/, configs) is untracked. Without a baseline commit, every later task that touches an existing file will show that file as "new file" with no diff history, making review impossible. Capture the existing state first.

**Files:**
- Modify: none (just `git add` everything not ignored)

- [ ] **Step 1: Inspect what would be added**

```bash
git status
```
Expected: Lists `README.md`, `SETUP.md`, `index.html`, `package.json`, `package-lock.json`, `postcss.config.js`, `src/`, `src-tauri/`, `tailwind.config.js`, `tsconfig.json`, `vite.config.ts` as untracked. Does NOT list `node_modules/`, `src-tauri/target/`, or `src-tauri/gen/` (these are gitignored).

- [ ] **Step 2: Stage all non-ignored existing files**

```bash
git add .
git status
```
Expected: All files above shown as "new file". Verify no `node_modules` paths sneaked in. If any did, fix `.gitignore` and re-stage.

- [ ] **Step 3: Commit**

```bash
git commit -m "Baseline: import existing meister codebase

Snapshot of the app prior to the whisper.cpp swap. Subsequent commits
modify these files; this commit exists so diffs are reviewable."
```

---

## Task 2: Add whisper-rs and supporting crates to Cargo.toml

Add the Rust dependencies needed for native whisper.cpp + WAV reading + streaming HTTP downloads. This task only touches `Cargo.toml`. We don't write code yet, just verify the deps resolve.

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Edit `src-tauri/Cargo.toml`**

In the `[dependencies]` section, add:

```toml
whisper-rs = { version = "0.11", features = ["coreml", "metal"] }
hound = "3"
futures-util = "0.3"
```

And update the existing `reqwest` line to include the `stream` feature:

```toml
reqwest = { version = "0.12", features = ["json", "stream"] }
```

Leave all other entries as-is. (The `regex` crate stays for now; we'll remove it in Task 12 if it's truly unused after the rewrite.)

- [ ] **Step 2: Verify the dependency tree resolves**

```bash
cd src-tauri && cargo fetch
```
Expected: Cargo downloads whisper-rs, hound, futures-util, and reqwest's stream-related crates. No errors. (This does NOT compile whisper.cpp yet — that happens on `cargo build`.)

- [ ] **Step 3: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "Add whisper-rs, hound, futures-util deps for whisper.cpp swap"
```

---

## Task 3: Create the model-fetch script

A bash script that downloads the `base` ggml weights and CoreML encoder into `src-tauri/resources/models/`. Idempotent — skips files that already exist with reasonable size. Used by `build.rs` (next task) and re-runnable by hand.

**Files:**
- Create: `src-tauri/resources/models/.gitkeep`
- Create: `src-tauri/scripts/fetch-base-model.sh`

- [ ] **Step 1: Create the resources dir with a `.gitkeep`**

```bash
mkdir -p src-tauri/resources/models src-tauri/scripts
touch src-tauri/resources/models/.gitkeep
```

- [ ] **Step 2: Write the fetch script**

Create `src-tauri/scripts/fetch-base-model.sh`:

```bash
#!/usr/bin/env bash
# Downloads the bundled base whisper.cpp model + its CoreML encoder.
# Idempotent: skips files that already exist with non-trivial size.

set -euo pipefail

cd "$(dirname "$0")/../resources/models"

BIN_URL="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin"
ENC_URL="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base-encoder.mlmodelc.zip"

# 100 MB minimum sanity check (real file is ~145 MB)
MIN_BIN_BYTES=$((100 * 1024 * 1024))

need_bin=true
if [[ -f ggml-base.bin ]]; then
  size=$(stat -f%z ggml-base.bin 2>/dev/null || stat -c%s ggml-base.bin)
  if (( size > MIN_BIN_BYTES )); then
    need_bin=false
  fi
fi

if $need_bin; then
  echo "Fetching ggml-base.bin (~145 MB)…"
  curl -L --fail --progress-bar -o ggml-base.bin "$BIN_URL"
else
  echo "ggml-base.bin already present; skipping."
fi

if [[ ! -d ggml-base-encoder.mlmodelc ]]; then
  echo "Fetching ggml-base-encoder.mlmodelc.zip (~75 MB)…"
  curl -L --fail --progress-bar -o ggml-base-encoder.mlmodelc.zip "$ENC_URL"
  echo "Unzipping…"
  unzip -q ggml-base-encoder.mlmodelc.zip
  rm ggml-base-encoder.mlmodelc.zip
else
  echo "ggml-base-encoder.mlmodelc already present; skipping."
fi

echo "Bundled base model ready under $(pwd)"
```

```bash
chmod +x src-tauri/scripts/fetch-base-model.sh
```

- [ ] **Step 3: Run the script and verify it works**

```bash
src-tauri/scripts/fetch-base-model.sh
ls -lh src-tauri/resources/models/
```
Expected: `ggml-base.bin` (~145 MB) and `ggml-base-encoder.mlmodelc/` directory both present. Re-running the script should print "skipping" for both.

- [ ] **Step 4: Verify the model files are gitignored**

```bash
git status src-tauri/resources/models/
```
Expected: Only `.gitkeep` shows as a new file. The `.bin` and `.mlmodelc/` paths must NOT appear (they're matched by `src-tauri/resources/models/ggml-*.bin` and `src-tauri/resources/models/ggml-*.mlmodelc/` in `.gitignore`).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/resources/models/.gitkeep src-tauri/scripts/fetch-base-model.sh
git commit -m "Add fetch-base-model.sh: download whisper.cpp base + CoreML encoder"
```

---

## Task 4: Wire the fetch script into build.rs

Make `cargo build` invoke the fetch script so a fresh checkout gets the model files automatically. If the script fails, the build fails loudly — never produce a broken .app.

**Files:**
- Modify: `src-tauri/build.rs`

- [ ] **Step 1: Read the current `build.rs`**

```bash
cat src-tauri/build.rs
```
Expected: A short file containing `fn main() { tauri_build::build() }`. We'll add a fetch step before the tauri_build call.

- [ ] **Step 2: Replace `src-tauri/build.rs` with**

```rust
use std::process::Command;

fn main() {
    // Re-run if the script itself changes
    println!("cargo:rerun-if-changed=scripts/fetch-base-model.sh");

    let script = std::path::Path::new("scripts/fetch-base-model.sh");
    if script.exists() {
        let status = Command::new("bash")
            .arg(script)
            .status()
            .expect("failed to invoke fetch-base-model.sh");
        if !status.success() {
            panic!("fetch-base-model.sh failed (exit {:?}). Cannot build a meister bundle without the bundled whisper model.", status.code());
        }
    }

    tauri_build::build();
}
```

- [ ] **Step 3: Verify a clean build still succeeds**

```bash
cd src-tauri && cargo build 2>&1 | tail -20
```
Expected: Build completes without errors. The first build will take several minutes because whisper-rs compiles whisper.cpp from source. Subsequent builds are incremental.

If you see "fetch-base-model.sh failed" — the script itself is broken; re-test it with `bash src-tauri/scripts/fetch-base-model.sh` and fix.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/build.rs
git commit -m "build.rs: invoke fetch-base-model.sh before tauri_build"
```

---

## Task 5: Bundle the model files as Tauri resources

Tell the Tauri bundler to copy the model files into the `.app` bundle so the installed app can find them at runtime via `app.path().resource_dir()`.

**Files:**
- Modify: `src-tauri/tauri.conf.json`

- [ ] **Step 1: Open `src-tauri/tauri.conf.json` and locate the `bundle` section**

If the `bundle` block already exists, add a `resources` array. If not, add the whole block. The minimal change:

```json
"bundle": {
  "active": true,
  "resources": [
    "resources/models/ggml-base.bin",
    "resources/models/ggml-base-encoder.mlmodelc/**/*"
  ]
}
```

If the file already has a `bundle` block, merge the `"resources"` key into it without removing other keys. Read first, edit minimally.

- [ ] **Step 2: Verify the JSON parses**

```bash
cat src-tauri/tauri.conf.json | python3 -m json.tool > /dev/null && echo OK
```
Expected: `OK`. If you see a JSON error, re-edit.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/tauri.conf.json
git commit -m "tauri.conf.json: bundle base whisper model + CoreML encoder as resources"
```

---

## Task 6: transcription/models.rs — model registry constants and paths

First slice of the new module: the `ModelSize` enum, the `ModelInfo` struct, the registry of known sizes with their hardcoded byte counts, and path helpers. TDD: tests first.

**Files:**
- Create: `src-tauri/src/transcription/models.rs`
- Create: `src-tauri/src/transcription/mod.rs` (stub)

- [ ] **Step 1: Create `src-tauri/src/transcription/mod.rs` with a stub**

`engine.rs` doesn't exist yet (Task 9), so for now declare only `models`:

```rust
pub mod models;
```

Task 9 will expand this to include `engine`, and Task 11 will add the public re-exports.

- [ ] **Step 2: Write the failing test for model registry lookup**

Create `src-tauri/src/transcription/models.rs` with:

```rust
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelSize {
    Tiny,
    Base,
    Small,
    Medium,
    Large,
}

impl ModelSize {
    pub fn from_str(s: &str) -> Option<ModelSize> {
        match s {
            "tiny" => Some(Self::Tiny),
            "base" => Some(Self::Base),
            "small" => Some(Self::Small),
            "medium" => Some(Self::Medium),
            "large" => Some(Self::Large),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Tiny => "tiny",
            Self::Base => "base",
            Self::Small => "small",
            Self::Medium => "medium",
            Self::Large => "large",
        }
    }

    /// Approximate total download size (.bin + CoreML encoder), in bytes.
    /// Used as the user-facing "this download is N GB" estimate. Real on-disk
    /// sizes are read from the filesystem after download completes.
    pub fn approx_bundle_bytes(&self) -> u64 {
        match self {
            Self::Tiny => 78 * 1024 * 1024 + 17 * 1024 * 1024,
            Self::Base => 148 * 1024 * 1024 + 75 * 1024 * 1024,
            Self::Small => 488 * 1024 * 1024 + 230 * 1024 * 1024,
            Self::Medium => 1_500 * 1024 * 1024 + 700 * 1024 * 1024,
            Self::Large => 3_100 * 1024 * 1024 + 1_500 * 1024 * 1024,
        }
    }
}

pub fn bin_filename(size: ModelSize) -> String {
    format!("ggml-{}.bin", size.as_str())
}

pub fn encoder_dirname(size: ModelSize) -> String {
    format!("ggml-{}-encoder.mlmodelc", size.as_str())
}

pub fn model_paths(models_dir: &std::path::Path, size: ModelSize) -> (PathBuf, PathBuf) {
    (
        models_dir.join(bin_filename(size)),
        models_dir.join(encoder_dirname(size)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_sizes() {
        assert_eq!(ModelSize::from_str("base"), Some(ModelSize::Base));
        assert_eq!(ModelSize::from_str("large"), Some(ModelSize::Large));
        assert_eq!(ModelSize::from_str("xl"), None);
    }

    #[test]
    fn round_trips_str_form() {
        for s in ["tiny", "base", "small", "medium", "large"] {
            let m = ModelSize::from_str(s).unwrap();
            assert_eq!(m.as_str(), s);
        }
    }

    #[test]
    fn paths_are_under_models_dir() {
        let dir = std::path::Path::new("/tmp/x");
        let (bin, enc) = model_paths(dir, ModelSize::Base);
        assert_eq!(bin, std::path::PathBuf::from("/tmp/x/ggml-base.bin"));
        assert_eq!(enc, std::path::PathBuf::from("/tmp/x/ggml-base-encoder.mlmodelc"));
    }

    #[test]
    fn bundle_bytes_are_increasing() {
        let sizes = [ModelSize::Tiny, ModelSize::Base, ModelSize::Small, ModelSize::Medium, ModelSize::Large];
        for w in sizes.windows(2) {
            assert!(w[0].approx_bundle_bytes() < w[1].approx_bundle_bytes());
        }
    }
}
```

- [ ] **Step 3: Add the `transcription` module to `src-tauri/src/lib.rs`**

In `src-tauri/src/lib.rs`, add `mod transcription;` immediately below the existing `mod commands;` line. Don't change anything else yet.

- [ ] **Step 4: Run the tests, verify they pass**

```bash
cd src-tauri && cargo test --lib transcription::models -- --nocapture
```
Expected: All four tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/transcription/ src-tauri/src/lib.rs
git commit -m "transcription/models: ModelSize enum, registry constants, path helpers"
```

---

## Task 7: transcription/models.rs — list_available_models and bundled-copy

Add the runtime model-availability probe (`list_available_models`) and the first-launch logic that copies the bundled `base` model from the app's resource dir into `app_data_dir()` so all models live in one place.

**Files:**
- Modify: `src-tauri/src/transcription/models.rs`

- [ ] **Step 1: Write the failing tests**

Append to `src-tauri/src/transcription/models.rs` (inside `mod tests`):

```rust
    #[test]
    fn lists_no_models_in_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let infos = list_available_models(tmp.path());
        assert_eq!(infos.len(), 5);
        assert!(infos.iter().all(|i| !i.downloaded));
    }

    #[test]
    fn detects_base_when_both_files_present() {
        let tmp = tempfile::tempdir().unwrap();
        let (bin, enc) = model_paths(tmp.path(), ModelSize::Base);
        std::fs::write(&bin, b"fake-bin-bytes").unwrap();
        std::fs::create_dir_all(&enc).unwrap();
        std::fs::write(enc.join("model.mlmodel"), b"fake").unwrap();

        let infos = list_available_models(tmp.path());
        let base = infos.iter().find(|i| i.size == "base").unwrap();
        assert!(base.downloaded);
        assert!(base.size_bytes > 0);
    }

    #[test]
    fn missing_encoder_means_not_downloaded() {
        let tmp = tempfile::tempdir().unwrap();
        let (bin, _enc) = model_paths(tmp.path(), ModelSize::Base);
        std::fs::write(&bin, b"only-bin").unwrap();
        let infos = list_available_models(tmp.path());
        let base = infos.iter().find(|i| i.size == "base").unwrap();
        assert!(!base.downloaded, "model should be incomplete without encoder");
    }

    #[test]
    fn ensure_bundled_copies_when_missing() {
        let resource = tempfile::tempdir().unwrap();
        let app_data = tempfile::tempdir().unwrap();

        // Fake "bundled" base model under resource_dir/models/
        let bundled_models = resource.path().join("models");
        std::fs::create_dir_all(&bundled_models).unwrap();
        let (bin, enc) = model_paths(&bundled_models, ModelSize::Base);
        std::fs::write(&bin, b"bundled-bin").unwrap();
        std::fs::create_dir_all(&enc).unwrap();
        std::fs::write(enc.join("file"), b"x").unwrap();

        ensure_bundled_base_copied(resource.path(), app_data.path()).unwrap();

        let (target_bin, target_enc) = model_paths(&app_data.path().join("models"), ModelSize::Base);
        assert!(target_bin.exists());
        assert!(target_enc.is_dir());
        assert_eq!(std::fs::read(&target_bin).unwrap(), b"bundled-bin");
    }

    #[test]
    fn ensure_bundled_skips_when_already_present() {
        let resource = tempfile::tempdir().unwrap();
        let app_data = tempfile::tempdir().unwrap();
        let bundled = resource.path().join("models");
        std::fs::create_dir_all(&bundled).unwrap();

        // Pre-existing model in app_data
        let target_dir = app_data.path().join("models");
        std::fs::create_dir_all(&target_dir).unwrap();
        let (target_bin, target_enc) = model_paths(&target_dir, ModelSize::Base);
        std::fs::write(&target_bin, b"existing").unwrap();
        std::fs::create_dir_all(&target_enc).unwrap();
        std::fs::write(target_enc.join("file"), b"x").unwrap();

        // Bundled model has different content; ensure we don't overwrite
        let (bin, enc) = model_paths(&bundled, ModelSize::Base);
        std::fs::write(&bin, b"different").unwrap();
        std::fs::create_dir_all(&enc).unwrap();
        std::fs::write(enc.join("file"), b"y").unwrap();

        ensure_bundled_base_copied(resource.path(), app_data.path()).unwrap();
        assert_eq!(std::fs::read(&target_bin).unwrap(), b"existing");
    }
```

Also add `tempfile = "3"` to `[dev-dependencies]` in `src-tauri/Cargo.toml` (the existing `tempfile = "3"` is in `[dependencies]`, which works too; verify and only add if missing).

- [ ] **Step 2: Run the tests — they should fail to compile**

```bash
cd src-tauri && cargo test --lib transcription::models 2>&1 | tail -10
```
Expected: Compile error — `list_available_models`, `ensure_bundled_base_copied`, and `ModelInfo` don't exist yet.

- [ ] **Step 3: Implement `ModelInfo`, `list_available_models`, `ensure_bundled_base_copied`**

In `src-tauri/src/transcription/models.rs`, add (above the `#[cfg(test)]` block):

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub size: String,
    pub downloaded: bool,
    pub size_bytes: u64,
}

const ALL_SIZES: [ModelSize; 5] = [
    ModelSize::Tiny,
    ModelSize::Base,
    ModelSize::Small,
    ModelSize::Medium,
    ModelSize::Large,
];

pub fn list_available_models(models_dir: &std::path::Path) -> Vec<ModelInfo> {
    ALL_SIZES.iter().map(|&size| {
        let (bin, enc) = model_paths(models_dir, size);
        let downloaded = bin.is_file() && enc.is_dir();
        let size_bytes = if downloaded {
            std::fs::metadata(&bin).map(|m| m.len()).unwrap_or(0)
                + dir_size(&enc).unwrap_or(0)
        } else {
            size.approx_bundle_bytes()
        };
        ModelInfo { size: size.as_str().to_string(), downloaded, size_bytes }
    }).collect()
}

fn dir_size(path: &std::path::Path) -> std::io::Result<u64> {
    let mut total = 0u64;
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        total += if meta.is_dir() { dir_size(&entry.path()).unwrap_or(0) } else { meta.len() };
    }
    Ok(total)
}

/// Copy the bundled base model from `<resource_dir>/models/` into
/// `<app_data_dir>/models/` if not already present. Idempotent.
pub fn ensure_bundled_base_copied(
    resource_dir: &std::path::Path,
    app_data_dir: &std::path::Path,
) -> Result<(), String> {
    let target_dir = app_data_dir.join("models");
    std::fs::create_dir_all(&target_dir).map_err(|e| format!("create models dir: {e}"))?;

    let (target_bin, target_enc) = model_paths(&target_dir, ModelSize::Base);
    if target_bin.is_file() && target_enc.is_dir() {
        return Ok(());
    }

    let bundled_dir = resource_dir.join("models");
    let (src_bin, src_enc) = model_paths(&bundled_dir, ModelSize::Base);

    if !src_bin.is_file() {
        return Err(format!("bundled base model not found at {}", src_bin.display()));
    }

    if !target_bin.exists() {
        std::fs::copy(&src_bin, &target_bin)
            .map_err(|e| format!("copy bin: {e}"))?;
    }
    if !target_enc.exists() {
        copy_dir_recursive(&src_enc, &target_enc)
            .map_err(|e| format!("copy encoder: {e}"))?;
    }
    Ok(())
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let target = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(&path, &target)?;
        } else {
            std::fs::copy(&path, &target)?;
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Run tests, verify they pass**

```bash
cd src-tauri && cargo test --lib transcription::models -- --nocapture
```
Expected: All 9 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/transcription/models.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "transcription/models: list_available_models + bundled-copy logic"
```

---

## Task 8: transcription/models.rs — download_model + cancellation

Streaming HTTP downloads of `.bin` and `.mlmodelc.zip`, with a per-size cancellation flag and a progress callback. The download itself isn't unit-testable cheaply (would need real HTTP fixtures), so we test the cancellation primitive and the path-staging logic; manual QA covers the network path.

**Files:**
- Modify: `src-tauri/src/transcription/models.rs`

- [ ] **Step 1: Write tests for the cancellation registry**

Append inside `mod tests`:

```rust
    #[test]
    fn cancel_token_starts_unset() {
        let tok = new_cancel_token();
        assert!(!tok.is_cancelled());
    }

    #[test]
    fn cancel_token_flips_when_cancelled() {
        let tok = new_cancel_token();
        let tok2 = tok.clone();
        tok2.cancel();
        assert!(tok.is_cancelled());
    }

    #[test]
    fn registry_returns_same_token_per_size() {
        let r = CancelRegistry::default();
        let a = r.token_for("base");
        let b = r.token_for("base");
        b.cancel();
        assert!(a.is_cancelled(), "tokens for the same size should share state");
    }

    #[test]
    fn registry_separates_sizes() {
        let r = CancelRegistry::default();
        let a = r.token_for("base");
        let b = r.token_for("large");
        a.cancel();
        assert!(!b.is_cancelled());
    }
```

- [ ] **Step 2: Run tests — should fail to compile**

```bash
cd src-tauri && cargo test --lib transcription::models 2>&1 | tail -10
```
Expected: Errors about `new_cancel_token`, `CancelRegistry`, etc.

- [ ] **Step 3: Implement the cancellation primitives + download function**

Append to `src-tauri/src/transcription/models.rs` (above `#[cfg(test)]`):

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

#[derive(Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn cancel(&self) { self.0.store(true, Ordering::SeqCst); }
    pub fn is_cancelled(&self) -> bool { self.0.load(Ordering::SeqCst) }
}

pub fn new_cancel_token() -> CancelToken { CancelToken::default() }

#[derive(Default)]
pub struct CancelRegistry {
    inner: Mutex<HashMap<String, CancelToken>>,
}

impl CancelRegistry {
    pub fn token_for(&self, size: &str) -> CancelToken {
        let mut g = self.inner.lock().unwrap();
        g.entry(size.to_string()).or_default().clone()
    }

    pub fn reset(&self, size: &str) {
        let mut g = self.inner.lock().unwrap();
        g.insert(size.to_string(), CancelToken::default());
    }
}

const HF_BASE: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

/// Download a model's .bin and .mlmodelc.zip (streamed) into `models_dir`.
/// `progress` is invoked with (bytes_downloaded, bytes_total_estimate).
/// `cancel` is checked between chunks; if set, the function returns Err("cancelled").
pub async fn download_model(
    models_dir: &std::path::Path,
    size: ModelSize,
    cancel: CancelToken,
    progress: impl Fn(u64, u64) + Send + 'static,
) -> Result<(), String> {
    use futures_util::StreamExt;
    use std::io::Write;

    let tmp_dir = models_dir.join(".tmp").join(size.as_str());
    let _ = std::fs::remove_dir_all(&tmp_dir); // Clean up any leftovers
    std::fs::create_dir_all(&tmp_dir).map_err(|e| format!("create tmp dir: {e}"))?;

    let bin_url = format!("{HF_BASE}/{}", bin_filename(size));
    let enc_url = format!("{HF_BASE}/ggml-{}-encoder.mlmodelc.zip", size.as_str());

    let staged_bin = tmp_dir.join(bin_filename(size));
    let staged_zip = tmp_dir.join(format!("{}-encoder.mlmodelc.zip", size.as_str()));

    let total_estimate = size.approx_bundle_bytes();
    let mut downloaded: u64 = 0;

    // Stream both files, sharing one running counter, into the staging dir.
    for (url, target) in [(&bin_url, &staged_bin), (&enc_url, &staged_zip)] {
        let resp = reqwest::Client::new().get(url).send().await
            .map_err(|e| format!("GET {url}: {e}"))?
            .error_for_status()
            .map_err(|e| format!("HTTP error from {url}: {e}"))?;

        let mut file = std::fs::File::create(target)
            .map_err(|e| format!("create {}: {e}", target.display()))?;
        let mut stream = resp.bytes_stream();

        while let Some(chunk) = stream.next().await {
            if cancel.is_cancelled() {
                let _ = std::fs::remove_dir_all(&tmp_dir);
                return Err("cancelled".to_string());
            }
            let bytes = chunk.map_err(|e| format!("download chunk: {e}"))?;
            file.write_all(&bytes).map_err(|e| format!("write: {e}"))?;
            downloaded += bytes.len() as u64;
            progress(downloaded, total_estimate);
        }
    }

    // Unzip encoder in place
    let zip_file = std::fs::File::open(&staged_zip).map_err(|e| format!("open zip: {e}"))?;
    let mut archive = zip::ZipArchive::new(zip_file).map_err(|e| format!("read zip: {e}"))?;
    archive.extract(&tmp_dir).map_err(|e| format!("unzip: {e}"))?;
    std::fs::remove_file(&staged_zip).ok();

    // Atomic rename into final location
    let (final_bin, final_enc) = model_paths(models_dir, size);
    let staged_enc = tmp_dir.join(encoder_dirname(size));
    if !staged_enc.is_dir() {
        return Err(format!("expected encoder dir after unzip: {}", staged_enc.display()));
    }
    if final_bin.exists() { std::fs::remove_file(&final_bin).ok(); }
    if final_enc.exists() { std::fs::remove_dir_all(&final_enc).ok(); }
    std::fs::rename(&staged_bin, &final_bin).map_err(|e| format!("install bin: {e}"))?;
    std::fs::rename(&staged_enc, &final_enc).map_err(|e| format!("install encoder: {e}"))?;

    // Best-effort cleanup of the tmp staging dir
    let _ = std::fs::remove_dir_all(&tmp_dir);
    Ok(())
}
```

Add to `src-tauri/Cargo.toml` `[dependencies]`:

```toml
zip = "0.6"
```

(The `unzip` shell command works for the build script, but in-process unzipping for downloads needs a Rust crate.)

- [ ] **Step 4: Run cancellation tests, verify they pass**

```bash
cd src-tauri && cargo test --lib transcription::models -- --nocapture
```
Expected: All tests (9 from before + 4 new = 13) pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/transcription/models.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "transcription/models: streaming download + cancellation registry"
```

---

## Task 9: transcription/engine.rs — WAV reading

Stand up `engine.rs` with the WAV-to-f32 conversion. Tested against a small fixture WAV generated locally (or pre-existing).

**Files:**
- Create: `src-tauri/src/transcription/engine.rs`
- Modify: `src-tauri/src/transcription/mod.rs`
- Create: `src-tauri/tests/fixtures/hello.wav` (generated)

- [ ] **Step 1: Generate a small fixture WAV**

```bash
mkdir -p src-tauri/tests/fixtures
say -o /tmp/hello.aiff "hello world this is a test of meister"
ffmpeg -y -i /tmp/hello.aiff -ar 16000 -ac 1 -acodec pcm_s16le src-tauri/tests/fixtures/hello.wav
ls -lh src-tauri/tests/fixtures/hello.wav
```
Expected: ~30–80 KB WAV file.

- [ ] **Step 2: Write the failing test**

Create `src-tauri/src/transcription/engine.rs`:

```rust
use std::path::Path;

pub fn read_wav_as_f32_mono(path: &Path) -> Result<Vec<f32>, String> {
    let mut reader = hound::WavReader::open(path)
        .map_err(|e| format!("open wav {}: {e}", path.display()))?;
    let spec = reader.spec();
    if spec.channels != 1 {
        return Err(format!("expected mono WAV, got {} channels", spec.channels));
    }
    if spec.sample_rate != 16_000 {
        return Err(format!("expected 16 kHz WAV, got {} Hz", spec.sample_rate));
    }
    if spec.bits_per_sample != 16 {
        return Err(format!("expected 16-bit WAV, got {} bits", spec.bits_per_sample));
    }
    let samples: Result<Vec<i16>, _> = reader.samples::<i16>().collect();
    let samples = samples.map_err(|e| format!("read samples: {e}"))?;
    Ok(samples.into_iter().map(|s| s as f32 / 32768.0).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_fixture_wav() {
        let path = std::path::Path::new("tests/fixtures/hello.wav");
        let samples = read_wav_as_f32_mono(path).expect("fixture should be readable");
        assert!(!samples.is_empty(), "should produce samples");
        // 16 kHz mono, ~1–3 seconds of speech → 16k–48k samples
        assert!(samples.len() > 8_000, "too few samples: {}", samples.len());
        assert!(samples.iter().all(|s| s.abs() <= 1.0), "samples should be in [-1, 1]");
    }

    #[test]
    fn rejects_missing_file() {
        let err = read_wav_as_f32_mono(std::path::Path::new("/nonexistent.wav")).unwrap_err();
        assert!(err.contains("open wav"));
    }
}
```

- [ ] **Step 3: Update `src-tauri/src/transcription/mod.rs`**

Replace contents with:
```rust
pub mod engine;
pub mod models;
```

- [ ] **Step 4: Run tests, verify they pass**

```bash
cd src-tauri && cargo test --lib transcription::engine -- --nocapture
```
Expected: Both tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/transcription/ src-tauri/tests/fixtures/hello.wav
git commit -m "transcription/engine: WAV → f32 mono reader with hound"
```

---

## Task 10: transcription/engine.rs — transcribe_wav with whisper-rs

The actual whisper.cpp call. Public API takes a progress callback (closure) so the engine code stays Tauri-agnostic and the integration test can capture progress without an `AppHandle`. A thin Tauri wrapper lives in `commands.rs` (Task 12).

**Files:**
- Modify: `src-tauri/src/transcription/engine.rs`
- Create: `src-tauri/tests/engine_integration.rs`

- [ ] **Step 1: Write the failing integration test**

Create `src-tauri/tests/engine_integration.rs`:

```rust
//! Integration test for the whisper.cpp engine. Requires the bundled `base`
//! model to exist under `resources/models/` (created by `cargo build`'s
//! fetch-base-model.sh hook).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use meister_lib::transcription::engine;
use meister_lib::transcription::models::ModelSize;

fn base_model_path() -> Option<PathBuf> {
    let p = PathBuf::from("resources/models");
    let bin = p.join("ggml-base.bin");
    let enc = p.join("ggml-base-encoder.mlmodelc");
    if bin.is_file() && enc.is_dir() { Some(p) } else { None }
}

#[test]
fn transcribes_fixture_with_base_model() {
    let Some(models_dir) = base_model_path() else {
        eprintln!("skipping: bundled base model not present (run `cargo build` first)");
        return;
    };

    let progress = Arc::new(Mutex::new(Vec::<i32>::new()));
    let p = progress.clone();

    let wav = std::path::Path::new("tests/fixtures/hello.wav");
    let result = engine::transcribe_wav_with_callback(
        &models_dir,
        ModelSize::Base,
        wav,
        Some("en"),
        move |pct| { p.lock().unwrap().push(pct); },
    );

    let text = result.expect("transcription should succeed");
    let lower = text.to_lowercase();
    assert!(lower.contains("hello") || lower.contains("test"),
        "expected 'hello' or 'test' in transcript, got: {text:?}");

    let progress = progress.lock().unwrap();
    assert!(!progress.is_empty(), "progress callback should have fired at least once");
    assert!(progress.iter().any(|&p| p > 0), "progress should reach >0%");
}
```

- [ ] **Step 2: Run the test — should fail to compile**

```bash
cd src-tauri && cargo test --test engine_integration 2>&1 | tail -10
```
Expected: Error about `transcribe_wav_with_callback` not existing.

- [ ] **Step 3: Implement `transcribe_wav_with_callback`**

Append to `src-tauri/src/transcription/engine.rs`:

```rust
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::transcription::models::{model_paths, ModelSize};

type ContextCache = Mutex<HashMap<String, Arc<WhisperContext>>>;
fn cache() -> &'static ContextCache {
    static C: OnceLock<ContextCache> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Returns true iff this is the first time we're loading the given size in this
/// process — useful for the UI to show the "warming" message before the call.
pub fn is_warming(size: ModelSize) -> bool {
    !cache().lock().unwrap().contains_key(size.as_str())
}

fn load_context(models_dir: &std::path::Path, size: ModelSize) -> Result<Arc<WhisperContext>, String> {
    let mut g = cache().lock().unwrap();
    if let Some(ctx) = g.get(size.as_str()) {
        return Ok(ctx.clone());
    }
    let (bin, _enc) = model_paths(models_dir, size);
    if !bin.is_file() {
        return Err(format!("model file missing: {}", bin.display()));
    }
    let ctx = WhisperContext::new_with_params(
        bin.to_str().ok_or("non-utf8 model path")?,
        WhisperContextParameters::default(),
    ).map_err(|e| format!("load whisper model: {e}"))?;
    let arc = Arc::new(ctx);
    g.insert(size.as_str().to_string(), arc.clone());
    Ok(arc)
}

/// Run whisper.cpp on a WAV file. Synchronous (CPU/GPU bound). Caller is
/// responsible for invoking inside `tokio::task::spawn_blocking` if used from
/// async context.
pub fn transcribe_wav_with_callback(
    models_dir: &std::path::Path,
    size: ModelSize,
    wav_path: &std::path::Path,
    language: Option<&str>,
    progress_cb: impl Fn(i32) + Send + Sync + 'static,
) -> Result<String, String> {
    let samples = read_wav_as_f32_mono(wav_path)?;
    let ctx = load_context(models_dir, size)?;
    let mut state = ctx.create_state().map_err(|e| format!("create state: {e}"))?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    if let Some(lang) = language.filter(|l| !l.is_empty() && *l != "auto") {
        params.set_language(Some(lang));
    }
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_special(false);
    params.set_print_timestamps(false);

    let cb = Arc::new(progress_cb);
    params.set_progress_callback_safe(move |p| (cb)(p));

    state.full(params, &samples).map_err(|e| format!("whisper full: {e}"))?;

    let n = state.full_n_segments().map_err(|e| format!("n_segments: {e}"))?;
    let mut out = String::new();
    for i in 0..n {
        out.push_str(&state.full_get_segment_text(i).map_err(|e| format!("segment {i}: {e}"))?);
    }
    Ok(out.trim().to_string())
}
```

- [ ] **Step 4: Run the integration test**

```bash
cd src-tauri && cargo test --test engine_integration -- --nocapture
```
Expected: Compiles and runs. The first run will be slow (~30–60s while CoreML compiles for the Neural Engine). Test should pass — transcript contains "hello" or "test".

If you get a compile error about `set_progress_callback_safe` not accepting your closure type, check the whisper-rs version's actual signature with `cargo doc --open -p whisper-rs` and adapt accordingly. The function exists in 0.11+; older versions use `set_progress_callback`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/transcription/engine.rs src-tauri/tests/engine_integration.rs
git commit -m "transcription/engine: transcribe_wav_with_callback via whisper-rs + Metal/CoreML"
```

---

## Task 11: transcription/mod.rs — public API surface

Re-export the things `commands.rs` will need so the import sites are stable.

**Files:**
- Modify: `src-tauri/src/transcription/mod.rs`

- [ ] **Step 1: Replace contents of `src-tauri/src/transcription/mod.rs`**

```rust
pub mod engine;
pub mod models;

pub use engine::{is_warming, transcribe_wav_with_callback};
pub use models::{
    download_model, ensure_bundled_base_copied, list_available_models, new_cancel_token,
    CancelRegistry, CancelToken, ModelInfo, ModelSize,
};
```

- [ ] **Step 2: Verify everything still builds and tests pass**

```bash
cd src-tauri && cargo test --lib 2>&1 | tail -10
```
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/transcription/mod.rs
git commit -m "transcription/mod: re-export public API"
```

---

## Task 12: Rewrite commands.rs::transcribe_audio + drop subprocess code

Swap the subprocess implementation for a thin call into the new module. Strip all the stderr/stdout regex parsing.

**Files:**
- Modify: `src-tauri/src/commands.rs`

- [ ] **Step 1: In `src-tauri/src/commands.rs`, delete the old transcription code.**

Order matters — delete in this order so line numbers don't shift mid-edit:

1. **Delete lines 122–322** — the `// ── Transcription ─────…` banner through the end of `get_wav_duration_ms` (inclusive). This removes `transcribe_audio`, `TranscribeProgress`, and `get_wav_duration_ms` in one block.
2. **Remove the now-unused imports** at the top of the file: delete `use std::io::{BufRead, BufReader};` and `use std::process::{Command, Stdio};`. Keep `use serde::{Deserialize, Serialize};` and `use tauri::{AppHandle, Emitter};`.
3. **Add `Manager` to the tauri import** so `app.path()` and `app.state()` resolve later. Change `use tauri::{AppHandle, Emitter};` to `use tauri::{AppHandle, Emitter, Manager};`.
4. **Add a module import** at the top (right under the tauri import): `use crate::transcription;`.

- [ ] **Step 2: Add the new transcription block**

Where the deleted Transcription section was, insert:

```rust
// ── Transcription ──────────────────────────────────────────────────────────────

#[derive(Clone, serde::Serialize)]
struct TranscribeProgress {
    percent: u32,
    message: String,
}

#[derive(Clone, serde::Serialize)]
struct ModelWarming {
    size: String,
}

#[tauri::command]
pub async fn transcribe_audio(
    app: AppHandle,
    wav_path: String,
    whisper_model: String,
    language: String,
) -> Result<String, String> {
    let size = transcription::ModelSize::from_str(&whisper_model)
        .ok_or_else(|| format!("unknown model size: {whisper_model}"))?;

    let models_dir = app.path()
        .app_data_dir()
        .map_err(|e| format!("resolve app_data_dir: {e}"))?
        .join("models");

    if transcription::is_warming(size) {
        let _ = app.emit("model-warming", ModelWarming { size: whisper_model.clone() });
    }

    let app_for_cb = app.clone();
    let wav_path_buf = std::path::PathBuf::from(&wav_path);
    let lang = if language.is_empty() || language == "auto" { None } else { Some(language) };

    let result = tokio::task::spawn_blocking(move || {
        transcription::transcribe_wav_with_callback(
            &models_dir,
            size,
            &wav_path_buf,
            lang.as_deref(),
            move |pct| {
                let _ = app_for_cb.emit("transcribe-progress", TranscribeProgress {
                    percent: pct.clamp(0, 100) as u32,
                    message: format!("Transcribing… {pct}%"),
                });
            },
        )
    }).await.map_err(|e| format!("task join: {e}"))?;

    let _ = app.emit("transcribe-progress", TranscribeProgress {
        percent: 100,
        message: "Transcription complete.".to_string(),
    });

    result
}
```

- [ ] **Step 3: Update `DepsStatus` and `check_dependencies` to drop whisper**

Replace the `DepsStatus` struct (currently at the top of the file):

```rust
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DepsStatus {
    pub ffmpeg: bool,
    pub ollama: bool,
}
```

In `check_dependencies`, delete the entire `whisper = …` block (the lines that probe `Command::new("whisper")…`) and update the constructor to omit `whisper`:

```rust
Ok(DepsStatus { ffmpeg, ollama })
```

- [ ] **Step 4: Build and run all tests**

```bash
cd src-tauri && cargo build 2>&1 | tail -10 && cargo test 2>&1 | tail -10
```
Expected: Compiles cleanly. If the `regex` crate is now unused, remove it from `[dependencies]` in `Cargo.toml` — the compiler will warn. (Search the codebase: `grep -r 'regex' src-tauri/src/` should return nothing in `commands.rs` after the rewrite.) If still used elsewhere, leave it.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "commands: replace whisper subprocess with native transcription module"
```

---

## Task 13: Add new Tauri commands (list/download/cancel) + bundled-copy on startup

Wire up the model-list, download-start, and download-cancel commands the frontend needs. Also: copy the bundled base model into `app_data_dir` on startup.

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Append new commands to `src-tauri/src/commands.rs`**

(`use tauri::Manager;` was already added in Task 12. `use crate::transcription;` likewise. Add only `use std::sync::Arc;` to the top imports.)

```rust
// ── Model management ───────────────────────────────────────────────────────────

#[derive(Clone, serde::Serialize)]
struct ModelDownloadProgress {
    size: String,
    bytes_downloaded: u64,
    bytes_total: u64,
}

#[tauri::command]
pub async fn list_available_models(
    app: AppHandle,
) -> Result<Vec<transcription::ModelInfo>, String> {
    let models_dir = app.path()
        .app_data_dir()
        .map_err(|e| format!("resolve app_data_dir: {e}"))?
        .join("models");
    Ok(transcription::list_available_models(&models_dir))
}

#[tauri::command]
pub async fn download_model(
    app: AppHandle,
    size: String,
) -> Result<(), String> {
    let model_size = transcription::ModelSize::from_str(&size)
        .ok_or_else(|| format!("unknown model size: {size}"))?;

    let models_dir = app.path()
        .app_data_dir()
        .map_err(|e| format!("resolve app_data_dir: {e}"))?
        .join("models");

    let registry = app.state::<Arc<transcription::CancelRegistry>>();
    registry.reset(&size);
    let token = registry.token_for(&size);

    let app_for_cb = app.clone();
    let size_for_cb = size.clone();
    transcription::download_model(
        &models_dir,
        model_size,
        token,
        move |bytes_downloaded, bytes_total| {
            let _ = app_for_cb.emit("model-download-progress", ModelDownloadProgress {
                size: size_for_cb.clone(),
                bytes_downloaded,
                bytes_total,
            });
        },
    ).await
}

#[tauri::command]
pub fn cancel_model_download(
    app: AppHandle,
    size: String,
) -> Result<(), String> {
    let registry = app.state::<Arc<transcription::CancelRegistry>>();
    registry.token_for(&size).cancel();
    Ok(())
}
```

- [ ] **Step 2: Update `src-tauri/src/lib.rs` to register everything**

Replace the body of `pub fn run()` with:

```rust
pub fn run() {
    use std::sync::Arc;

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .manage(Arc::new(transcription::CancelRegistry::default()))
        .setup(|app| {
            let resource_dir = app.path().resource_dir()?;
            let app_data_dir = app.path().app_data_dir()?;
            transcription::ensure_bundled_base_copied(&resource_dir, &app_data_dir)
                .map_err(|e| format!("copy bundled base model: {e}"))?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::check_dependencies,
            commands::list_ollama_models,
            commands::extract_audio,
            commands::transcribe_audio,
            commands::format_transcript,
            commands::save_text_file,
            commands::save_binary_file,
            commands::read_file_as_text,
            commands::cleanup_temp,
            commands::list_available_models,
            commands::download_model,
            commands::cancel_model_download,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Meister");
}
```

Also add `use tauri::Manager;` at the top of `lib.rs` if not already present (needed for `app.path()` and `app.manage()`).

- [ ] **Step 3: Build**

```bash
cd src-tauri && cargo build 2>&1 | tail -15
```
Expected: Clean build. If you see "trait `Manager` not in scope" — re-add the use statement.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "commands+lib: expose list/download/cancel commands; copy bundled base on startup"
```

---

## Task 14: Frontend — DownloadModelModal component

A standard centered modal that handles confirm → progress → success/error states for a single model download.

**Files:**
- Create: `src/components/DownloadModelModal.tsx`

- [ ] **Step 1: Create `src/components/DownloadModelModal.tsx`**

```tsx
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

interface Props {
  size: string;
  totalBytesEstimate: number;
  onComplete: () => void;
  onCancel: () => void;
}

interface ProgressEvent {
  size: string;
  bytes_downloaded: number;
  bytes_total: number;
}

type State = "confirm" | "downloading" | "error";

function formatGB(bytes: number) {
  return (bytes / (1024 ** 3)).toFixed(1) + " GB";
}

function formatMB(bytes: number) {
  return (bytes / (1024 ** 2)).toFixed(0) + " MB";
}

export default function DownloadModelModal({ size, totalBytesEstimate, onComplete, onCancel }: Props) {
  const [state, setState] = useState<State>("confirm");
  const [bytesDone, setBytesDone] = useState(0);
  const [bytesTotal, setBytesTotal] = useState(totalBytesEstimate);
  const [errorMsg, setErrorMsg] = useState("");

  useEffect(() => {
    if (state !== "downloading") return;
    let unlisten: UnlistenFn | undefined;
    listen<ProgressEvent>("model-download-progress", (e) => {
      if (e.payload.size !== size) return;
      setBytesDone(e.payload.bytes_downloaded);
      setBytesTotal(e.payload.bytes_total || totalBytesEstimate);
    }).then((fn) => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, [state, size, totalBytesEstimate]);

  const startDownload = async () => {
    setState("downloading");
    setBytesDone(0);
    try {
      await invoke("download_model", { size });
      onComplete();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      if (msg === "cancelled") {
        onCancel();
      } else {
        setErrorMsg(msg);
        setState("error");
      }
    }
  };

  const cancel = async () => {
    if (state === "downloading") {
      try { await invoke("cancel_model_download", { size }); } catch { /* ignore */ }
    }
    onCancel();
  };

  const pct = bytesTotal > 0 ? Math.min(100, Math.floor((bytesDone / bytesTotal) * 100)) : 0;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
      <div className="bg-surface rounded-xl shadow-2xl w-[420px] p-6 flex flex-col gap-4">
        {state === "confirm" && (
          <>
            <h2 className="text-lg font-semibold">Download {size} model?</h2>
            <p className="text-sm text-white/70">
              This is a one-time download of <strong>{formatGB(totalBytesEstimate)}</strong>.
              The bundled <code className="font-mono">base</code> model will continue to work in the meantime.
            </p>
            <div className="flex justify-end gap-2 mt-2">
              <button onClick={cancel}
                className="px-4 py-2 rounded-lg bg-surface-secondary hover:bg-surface-tertiary text-sm">Cancel</button>
              <button onClick={startDownload}
                className="px-4 py-2 rounded-lg bg-accent hover:bg-accent-hover text-sm font-semibold">Download</button>
            </div>
          </>
        )}

        {state === "downloading" && (
          <>
            <h2 className="text-lg font-semibold">Downloading {size}…</h2>
            <div className="h-2 bg-surface-secondary rounded overflow-hidden">
              <div className="h-full bg-accent transition-all" style={{ width: `${pct}%` }} />
            </div>
            <p className="text-xs text-white/60 font-mono">
              {formatMB(bytesDone)} / {formatMB(bytesTotal)} ({pct}%)
            </p>
            <div className="flex justify-end mt-2">
              <button onClick={cancel}
                className="px-4 py-2 rounded-lg bg-surface-secondary hover:bg-surface-tertiary text-sm">Cancel</button>
            </div>
          </>
        )}

        {state === "error" && (
          <>
            <h2 className="text-lg font-semibold text-red-400">Download failed</h2>
            <p className="text-sm text-white/70 break-words">{errorMsg}</p>
            <div className="flex justify-end gap-2 mt-2">
              <button onClick={onCancel}
                className="px-4 py-2 rounded-lg bg-surface-secondary hover:bg-surface-tertiary text-sm">Close</button>
              <button onClick={startDownload}
                className="px-4 py-2 rounded-lg bg-accent hover:bg-accent-hover text-sm font-semibold">Retry</button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Type-check**

```bash
npx tsc --noEmit
```
Expected: No errors. (If `npx tsc --noEmit` complains about missing imports or types, ensure `@tauri-apps/api` is installed — it is per `package.json`.)

- [ ] **Step 3: Commit**

```bash
git add src/components/DownloadModelModal.tsx
git commit -m "Add DownloadModelModal: confirm → progress → error/retry states"
```

---

## Task 15: Frontend — rewrite SettingsStep model picker; drop whisper badge

Make the picker data-driven. When the user picks a non-downloaded size, open the modal.

**Files:**
- Modify: `src/components/SettingsStep.tsx`

- [ ] **Step 1: Replace contents of `src/components/SettingsStep.tsx`**

```tsx
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AppSettings, OutputFormat } from "../App";
import DownloadModelModal from "./DownloadModelModal";

interface Props {
  settings: AppSettings;
  onChange: (s: AppSettings) => void;
  onBack: () => void;
  onRun: () => void;
}

interface ModelInfo {
  size: string;
  downloaded: boolean;
  size_bytes: number;
}

const OUTPUT_FORMATS: { id: OutputFormat; label: string; desc: string }[] = [
  { id: "md", label: "Markdown", desc: ".md" },
  { id: "txt", label: "Plain text", desc: ".txt" },
  { id: "docx", label: "Word doc", desc: ".docx" },
  { id: "pdf", label: "PDF", desc: ".pdf" },
];

function formatSize(bytes: number) {
  if (bytes >= 1024 ** 3) return (bytes / 1024 ** 3).toFixed(1) + " GB";
  return Math.round(bytes / 1024 ** 2) + " MB";
}

function modelLabel(m: ModelInfo) {
  if (m.size === "base" && m.downloaded) return `${m.size} — bundled ✓`;
  if (m.downloaded) return `${m.size} — downloaded ✓ (${formatSize(m.size_bytes)})`;
  return `${m.size} — ${formatSize(m.size_bytes)} (not downloaded)`;
}

export default function SettingsStep({ settings, onChange, onBack, onRun }: Props) {
  const [models, setModels] = useState<string[]>([]);
  const [modelInfos, setModelInfos] = useState<ModelInfo[]>([]);
  const [checking, setChecking] = useState(false);
  const [deps, setDeps] = useState<{ ffmpeg: boolean; ollama: boolean } | null>(null);
  const [pendingDownload, setPendingDownload] = useState<ModelInfo | null>(null);

  const set = <K extends keyof AppSettings>(key: K, val: AppSettings[K]) =>
    onChange({ ...settings, [key]: val });

  const toggleFormat = (fmt: OutputFormat) => {
    const next = settings.outputFormats.includes(fmt)
      ? settings.outputFormats.filter((f) => f !== fmt)
      : [...settings.outputFormats, fmt];
    if (next.length > 0) set("outputFormats", next);
  };

  useEffect(() => {
    refreshAll();
  }, []);

  const refreshAll = async () => {
    setChecking(true);
    try {
      const dep = await invoke<{ ffmpeg: boolean; ollama: boolean }>(
        "check_dependencies",
        { ollamaUrl: settings.ollamaUrl }
      );
      setDeps(dep);

      if (dep.ollama) {
        const m = await invoke<string[]>("list_ollama_models", { ollamaUrl: settings.ollamaUrl });
        setModels(m);
      }

      const infos = await invoke<ModelInfo[]>("list_available_models");
      setModelInfos(infos);
    } catch {
      // silently fail — user can still proceed
    } finally {
      setChecking(false);
    }
  };

  const onPickModel = (newSize: string) => {
    const info = modelInfos.find((m) => m.size === newSize);
    if (!info) return;
    if (info.downloaded) {
      set("whisperModel", newSize);
    } else {
      setPendingDownload(info);
      // Don't commit setting; modal will commit on success.
    }
  };

  const onDownloadComplete = () => {
    if (pendingDownload) {
      set("whisperModel", pendingDownload.size);
    }
    setPendingDownload(null);
    refreshAll();
  };

  const canRun = settings.outputFormats.length > 0;

  return (
    <div className="flex flex-col h-full px-8 py-6 gap-6">
      <h1 className="text-xl font-semibold">Configure</h1>

      <div className="flex-1 overflow-y-auto scrollable flex flex-col gap-6">

        {/* Dependency status — whisper badge removed */}
        <Section title="System Dependencies">
          <div className="flex gap-3 flex-wrap">
            {(["ffmpeg", "ollama"] as const).map((dep) => (
              <div
                key={dep}
                className={`flex items-center gap-2 px-3 py-1.5 rounded-lg text-sm
                  ${checking
                    ? "bg-surface-tertiary text-white/40"
                    : deps?.[dep]
                      ? "bg-green-500/15 text-green-400"
                      : "bg-red-500/15 text-red-400"
                  }`}
              >
                {checking ? <span className="spinner" /> : deps?.[dep] ? <span>✓</span> : <span>✗</span>}
                <span className="font-mono">{dep}</span>
              </div>
            ))}
            <button onClick={refreshAll}
              className="px-3 py-1.5 rounded-lg text-sm text-white/40 hover:text-white/70 bg-surface-secondary transition-colors">
              Recheck
            </button>
          </div>
          {deps && !deps.ollama && (
            <p className="text-xs text-red-400 mt-2">
              Ollama not reachable at <code className="font-mono">{settings.ollamaUrl}</code>.
              Check the URL below or start Ollama.
            </p>
          )}
          {deps && !deps.ffmpeg && (
            <p className="text-xs text-red-400 mt-1">
              ffmpeg not found. Install with: <code className="font-mono">brew install ffmpeg</code>
            </p>
          )}
        </Section>

        {/* Ollama settings */}
        <Section title="Ollama">
          <div className="flex flex-col gap-3">
            <Field label="Server URL">
              <input type="text" className="input"
                value={settings.ollamaUrl}
                onChange={(e) => set("ollamaUrl", e.target.value)}
                onBlur={refreshAll}
                placeholder="http://localhost:11434" />
            </Field>
            <Field label="Model">
              {models.length > 0 ? (
                <select className="input" value={settings.model} onChange={(e) => set("model", e.target.value)}>
                  {models.map((m) => <option key={m} value={m}>{m}</option>)}
                </select>
              ) : (
                <input type="text" className="input" value={settings.model}
                  onChange={(e) => set("model", e.target.value)} placeholder="gemma3" />
              )}
              <p className="text-xs text-white/30 mt-1">
                Pull a model first: <code className="font-mono">ollama pull gemma3</code>
              </p>
            </Field>
          </div>
        </Section>

        {/* Transcription model */}
        <Section title="Transcription model">
          <div className="flex flex-col gap-3">
            <Field label="Model">
              <select className="input" value={settings.whisperModel}
                onChange={(e) => onPickModel(e.target.value)}>
                {modelInfos.map((m) => (
                  <option key={m.size} value={m.size}>{modelLabel(m)}</option>
                ))}
              </select>
              <p className="text-xs text-white/30 mt-1">
                Larger models are more accurate but require a one-time download.
              </p>
            </Field>
            <Field label="Language (optional)">
              <input type="text" className="input"
                value={settings.language}
                onChange={(e) => set("language", e.target.value)}
                placeholder="auto  (or e.g. en, de, fr)" />
            </Field>
          </div>
        </Section>

        {/* Output formats */}
        <Section title="Output Formats">
          <div className="flex gap-2 flex-wrap">
            {OUTPUT_FORMATS.map(({ id, label, desc }) => {
              const active = settings.outputFormats.includes(id);
              return (
                <button key={id} onClick={() => toggleFormat(id)}
                  className={`px-4 py-2 rounded-lg text-sm transition-colors flex flex-col items-center gap-0.5
                    ${active
                      ? "bg-accent/20 border border-accent text-accent"
                      : "bg-surface-secondary border border-white/10 text-white/50 hover:border-white/20"
                    }`}>
                  <span className="font-medium">{label}</span>
                  <span className="text-[10px] opacity-60">{desc}</span>
                </button>
              );
            })}
          </div>
        </Section>
      </div>

      {/* Footer */}
      <div className="flex items-center justify-between shrink-0">
        <button onClick={onBack}
          className="px-5 py-2 rounded-lg bg-surface-secondary hover:bg-surface-tertiary text-sm font-medium transition-colors">
          ← Back
        </button>
        <button disabled={!canRun} onClick={onRun}
          className="px-6 py-2 rounded-lg bg-accent hover:bg-accent-hover disabled:opacity-30 disabled:cursor-not-allowed text-sm font-semibold transition-colors">
          Run →
        </button>
      </div>

      {pendingDownload && (
        <DownloadModelModal
          size={pendingDownload.size}
          totalBytesEstimate={pendingDownload.size_bytes}
          onComplete={onDownloadComplete}
          onCancel={() => setPendingDownload(null)}
        />
      )}
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-3">
      <h2 className="text-xs font-semibold text-white/40 uppercase tracking-wider">{title}</h2>
      {children}
    </div>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1">
      <label className="text-sm text-white/60">{label}</label>
      {children}
    </div>
  );
}
```

- [ ] **Step 2: Type-check**

```bash
npx tsc --noEmit
```
Expected: No errors.

- [ ] **Step 3: Commit**

```bash
git add src/components/SettingsStep.tsx
git commit -m "SettingsStep: data-driven model picker, drop whisper dep badge, gate downloads via modal"
```

---

## Task 16: Frontend — model-warming event in ProcessingStep

Show a friendly "Optimizing for Neural Engine…" message during the CoreML first-run compile.

**Files:**
- Modify: `src/components/ProcessingStep.tsx`
- Modify: `src/App.tsx` (forward `model-warming` events into a new piece of state)

- [ ] **Step 1: Read the current `ProcessingStep.tsx`**

```bash
cat src/components/ProcessingStep.tsx
```
Note the props it accepts. We need to add an optional `modelWarming?: string | null` prop (the size name being warmed, or null when not warming).

- [ ] **Step 2: Modify `src/components/ProcessingStep.tsx`**

In the `Props` interface, add:

```ts
modelWarming?: string | null;
```

In the component body, near where `transcribeProgress` is rendered, add an early-return-style block:

```tsx
{modelWarming && (
  <div className="text-xs text-amber-400 mb-2">
    Optimizing {modelWarming} model for Apple Neural Engine, ~30s, one-time…
  </div>
)}
```

Place this just above the existing transcribe-progress bar. (Do not delete or restructure existing markup.)

- [ ] **Step 3: Modify `src/App.tsx` to track warming state**

Add a new state:

```tsx
const [modelWarming, setModelWarming] = useState<string | null>(null);
```

Add a new effect (next to the existing `transcribe-progress` listener):

```tsx
useEffect(() => {
  let unlisten: UnlistenFn | undefined;
  listen<{ size: string }>("model-warming", (event) => {
    setModelWarming(event.payload.size);
  }).then((fn) => { unlisten = fn; });
  return () => { unlisten?.(); };
}, []);
```

In the existing `transcribe-progress` listener's handler, **also** clear warming on first progress event:

```tsx
listen<{ percent: number; message: string }>("transcribe-progress", (event) => {
  setTranscribeProgress(event.payload.percent);
  setModelWarming(null);
}).then((fn) => { unlisten = fn; });
```

Pass the prop through to `<ProcessingStep modelWarming={modelWarming} ... />`.

In `runPipeline`, before invoking `transcribe_audio`, reset: `setModelWarming(null);` (not strictly needed but defensive — the warming event will set it again if applicable).

- [ ] **Step 4: Type-check**

```bash
npx tsc --noEmit
```
Expected: No errors.

- [ ] **Step 5: Commit**

```bash
git add src/components/ProcessingStep.tsx src/App.tsx
git commit -m "Show CoreML warming state during first-run model load"
```

---

## Task 17: Update SETUP.md, README.md, and clean up settings.local.json

Doc cleanup. No code changes.

**Files:**
- Modify: `SETUP.md`
- Modify: `README.md`
- Modify: `.claude/settings.local.json`

- [ ] **Step 1: SETUP.md edits**

Open `SETUP.md` and apply:

1. **Intro paragraph** (line 3): Replace `transcribes it with Whisper` with `transcribes it on-device with whisper.cpp (Metal + Apple Neural Engine)`.

2. **Section 5** (`### 5. Whisper — installed as a uv tool`, including the entire block through the "Whisper downloads model weights…" line): Delete entirely.

3. After the deletion, renumber the following section if there's a "### 6. Ollama + Gemma" — make it "### 5. Ollama + Gemma".

4. **Pipeline description** (around line 96): Change `Transcribe with whisper` to `Transcribe via whisper.cpp (on-device, Metal + Apple Neural Engine)`.

5. **Tips section**: Replace the "Whisper model sizes" tip with: `**Transcription model**: the bundled \`base\` works well for most recordings. Larger models can be downloaded in Settings (one-time download per size).`

6. **Tips section**: Delete `**Updating Whisper**: \`uv tool upgrade openai-whisper\``.

7. Add under the "Running from source" / build section: `> First-time \`cargo build\` downloads ~220 MB of whisper.cpp model weights into \`src-tauri/resources/models/\`.`

- [ ] **Step 2: README.md edits**

Skim README.md. If it mentions Whisper or `pip install openai-whisper`, apply the same wording shift (whisper.cpp, on-device, Metal/ANE, no Python install required).

```bash
grep -ni "whisper\|openai-whisper\|pip install" README.md
```
For each hit, update or remove as appropriate. If README has no Whisper mentions, this step is a no-op.

- [ ] **Step 3: .claude/settings.local.json cleanup**

Open `.claude/settings.local.json`. Remove the two allowlisted bash entries that reference `whisper`:

- `"Bash(whisper --help)"`
- `"Bash(whisper /tmp/test_silence.wav --model tiny --output_format txt --output_dir /tmp/whisper_test --verbose True)"`

Leave all other allowlist entries intact.

- [ ] **Step 4: Verify the JSON parses**

```bash
cat .claude/settings.local.json | python3 -m json.tool > /dev/null && echo OK
```
Expected: `OK`.

- [ ] **Step 5: Commit**

```bash
git add SETUP.md README.md .claude/settings.local.json
git commit -m "docs: switch SETUP/README to whisper.cpp; drop dead allowlist entries"
```

---

## Task 18: End-to-end smoke test

Final verification — run the app, complete the full pipeline, verify each new behavior. This task is manual; no commit at the end unless you find and fix a bug.

**Files:** none

- [ ] **Step 1: Clean rebuild**

```bash
cd src-tauri && cargo clean && cd ..
npm run tauri dev 2>&1 | tee /tmp/meister-dev.log
```
Expected: Long initial build (whisper.cpp compile + Tauri bundle). App launches.

- [ ] **Step 2: Verify the Settings step UI**

In the app:
- Go to Settings step
- The dependency badges row shows only `ffmpeg | ollama` (no `whisper` badge)
- The Transcription Model dropdown shows: `base — bundled ✓`, `tiny — 95 MB (not downloaded)`, `small — 718 MB (not downloaded)`, `medium — 2.2 GB (not downloaded)`, `large — 4.6 GB (not downloaded)`

- [ ] **Step 3: Pick a non-bundled size, verify the modal**

- Click the dropdown, select `tiny` (smallest, fastest to test the download flow)
- Modal appears with copy "Download tiny model? This is a one-time download of ~95 MB…"
- Click **Cancel** — modal closes, dropdown reverts to `base`
- Click the dropdown again, select `tiny`, click **Download**
- Progress bar advances; byte count updates
- On completion modal closes; dropdown now shows `tiny — downloaded ✓ (78 MB)`

- [ ] **Step 4: Run a full transcription with the bundled base**

- Switch back to `base`
- Drop a short recording (any audio/video file ~30s)
- Drop a template (any .md)
- Click Run
- Processing step shows: `Extracting audio…`, then `Optimizing base model for Apple Neural Engine, ~30s, one-time…` (only on the very first transcription of the session — happens once per model file ever per machine), then `Transcribing… 12%` etc.
- Pipeline completes; Result step shows formatted output

- [ ] **Step 5: Restart and re-run; confirm warming message does NOT appear**

Quit the app, relaunch, run another transcription with `base`. The warming message should still appear (it's per-process, not per-machine — the cache is in-memory).

If you want it to be per-machine, that's deliberately out of scope per the spec. Note any unexpected behavior in a follow-up issue.

- [ ] **Step 6: Verify file layout under app_data_dir**

```bash
ls -lh "$HOME/Library/Application Support/com.meister.app/models/" 2>/dev/null \
  || ls -lh "$HOME/Library/Application Support/meister/models/" 2>/dev/null
```
Expected: Both `ggml-base.bin` and `ggml-base-encoder.mlmodelc/` present, plus any models you downloaded in step 3.

- [ ] **Step 7 (only if any bugs found): Fix and commit**

For each bug found, write a focused commit. If everything works, no commit — the task is just verification.

---

## Done

After Task 18:

- The full whisper.cpp swap is live
- Bundled `base` model works on first launch with no setup
- Larger models downloadable via Settings with consent + cancel + progress
- All Apple Silicon acceleration (Metal + ANE) engaged
- `openai-whisper` references purged from code, docs, and allowlist
- Tests cover the model registry and engine entry point
- Each commit is a clean, reviewable slice of the work
