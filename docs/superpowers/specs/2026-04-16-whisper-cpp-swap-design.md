# Replace openai-whisper with whisper.cpp (native bindings)

**Status:** Approved design, ready for implementation plan
**Date:** 2026-04-16
**Owner:** TBD

## Problem

Transcription in meister is too slow even at the `base` model size. The current pipeline shells out to `openai-whisper` (the Python reference implementation), which runs on CPU via PyTorch and has no Apple Silicon acceleration. On a typical M-series Mac, transcribing a 1-hour recording at `base` takes 10–30% of the audio's duration.

The integration also relies on parsing tqdm percentage strings and `[MM:SS.mmm --> MM:SS.mmm]` timestamp lines from the subprocess's stderr/stdout to drive the progress bar — fragile and dependent on whisper CLI version-specific output formatting.

## Goals

1. **Speed:** transcription runs on Metal + Apple Neural Engine (CoreML), targeting a 5–10× speedup over the current CPU path on Apple Silicon.
2. **Zero runtime setup:** the app ships with a working transcription model — no `pip install`, no `brew install`, no model download required for the default flow.
3. **Robust progress reporting:** progress comes from a real callback, not regex-parsed subprocess output.
4. **Power users still served:** users who want larger models can opt in via Settings, with an explicit consent step before any multi-GB download.

## Non-goals

- A separate "Manage models" UI for deleting downloaded models. Uninstalling the app cleans them up; manual deletion via Finder works for power users.
- Quantized model variants (Q5_0, Q8_0).
- English-only model variants (`base.en`, `small.en`). The language picker requires multilingual models.
- A standalone onboarding wizard. The Settings-modal flow subsumes it.
- Retry/backoff logic on download failure beyond surfacing the error to the user.
- Cross-platform support. macOS only, as today.
- Migration helpers for users who installed `openai-whisper` previously. Their old install becomes orphaned but harmless.

## Architecture

The transcription path changes from `Tauri command → spawn whisper subprocess → parse stderr → read .txt file` to `Tauri command → whisper-rs in-process call → progress callback → return string directly`.

### Module layout

A new `transcription/` module is introduced to contain whisper-rs lifecycle and model-file management. This keeps `commands.rs` focused on being a thin Tauri-handler layer.

```
src-tauri/src/
├── commands.rs          # tauri commands (transcribe_audio rewritten and shrunk)
├── transcription/       # NEW
│   ├── mod.rs           # public API: transcribe_wav, list_available_models, download_model, cancel_model_download
│   ├── models.rs        # model registry, paths, download/probe logic
│   └── engine.rs        # whisper-rs context cache, progress callback wiring
├── lib.rs               # registers commands (only the handler list changes)
└── ...
```

### Dependencies

Added to `src-tauri/Cargo.toml`:

- `whisper-rs` (latest stable at implementation time) with features `["coreml", "metal"]` — Rust bindings over whisper.cpp; vendors and compiles whisper.cpp at build time.
- `hound` — WAV file reader (whisper-rs takes f32 PCM samples, not file paths).
- `futures-util` — for streaming download progress from `reqwest`.
- `reqwest` — already present; gains the `stream` feature.

Removed: the `regex` crate becomes unused after the stderr-parsing code is deleted; remove it from `Cargo.toml` unless used elsewhere (verify during implementation).

## Backend behavior

### Model file storage

Models live at `app_data_dir()/models/`, where `app_data_dir()` is resolved via Tauri's `app.path().app_data_dir()` (typically `~/Library/Application Support/com.meister.app/models/` on macOS). Each model consists of two files:

- `ggml-{size}.bin` — the GGML-format weights
- `ggml-{size}-encoder.mlmodelc/` — the CoreML encoder bundle (a directory)

A model is "available" iff **both** entries exist. The picker reads this directory at runtime to determine availability — there is no separate state file to keep in sync.

### Bundling the default `base` model

The app bundles `ggml-base.bin` (~145 MB) and `ggml-base-encoder.mlmodelc/` (~75 MB) as Tauri resources, totaling ~220 MB added to the .app bundle.

Resources are declared in `tauri.conf.json`:

```json
"bundle": {
  "resources": [
    "resources/models/ggml-base.bin",
    "resources/models/ggml-base-encoder.mlmodelc/**/*"
  ]
}
```

The model files are **not** committed to git. A `src-tauri/scripts/fetch-base-model.sh` script downloads them from `huggingface.co/ggerganov/whisper.cpp` into `src-tauri/resources/models/`. The script is idempotent (skip-if-present) and is invoked by `build.rs` so `cargo build` fails with a clear error if the bundle can't be assembled.

A `src-tauri/resources/models/.gitkeep` is committed; a `.gitignore` excludes the actual model files.

On first app launch, `transcription/models.rs` checks whether `app_data_dir()/models/ggml-base.bin` exists. If not, it copies the bundled files from `app.path().resource_dir()/models/` into `app_data_dir()/models/`. This makes all subsequent code paths uniform — every model lives in `app_data_dir()`, regardless of whether it was bundled or downloaded.

### Downloading non-bundled models

Triggered by the frontend via a new `download_model(size: String)` Tauri command.

Source URLs:
- `https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{size}.bin`
- `https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{size}-encoder.mlmodelc.zip`

Steps:
1. Stream both files into `app_data_dir()/models/.tmp/`, emitting `model-download-progress` events with `{ size, bytes_downloaded, bytes_total }`.
2. Unzip the encoder archive in place.
3. Atomic `rename` the .bin and the unzipped .mlmodelc directory into `app_data_dir()/models/`. This guarantees a partial download is never observed as "available."
4. On any error or user cancel: clean up `.tmp/` and propagate the error.

A `cancel_model_download(size: String)` command sets a flag that the download loop checks each chunk; when set, the in-flight `reqwest` stream is dropped and `.tmp/` is cleaned up.

### Transcription engine

`transcription/engine.rs::transcribe_wav(wav_path, model_size, language) -> Result<String>`:

```
1. load_or_init context for {model_size}:
   - cached in OnceLock<Mutex<HashMap<String, Arc<WhisperContext>>>>
   - first call for a given size constructs WhisperContext::new_with_params (this triggers
     the CoreML one-time compile, ~30–60s, if the model has a CoreML encoder)
   - emit `model-warming` event before construction; clear it after by emitting first
     `transcribe-progress` from the callback
2. Read the WAV via hound, convert i16 → f32 mono samples (the wav already comes in
   as 16 kHz mono from the ffmpeg step; we just need format conversion).
3. Build FullParams:
   - set_language(Some(language)) when language != "auto" / ""
   - set_progress_callback_safe(|pct: i32| emit("transcribe-progress", { percent: pct as u32 }))
   - other params: default (greedy decoding is fine; matches openai-whisper default)
4. ctx.full(params, &samples) — runs synchronously; called inside tokio::task::spawn_blocking
5. Iterate ctx segments, concat segment text into a single String, return.
```

The context cache is keyed by model size and grows monotonically per session. There is no eviction policy — realistically a single session uses one model, and contexts are freed when the app quits. Adding LRU eviction is premature.

### Tauri command surface

Updated `commands.rs`:

- `check_dependencies(ollama_url) -> DepsStatus` — `DepsStatus` becomes `{ ffmpeg: bool, ollama: bool }`. The `whisper` field is removed.
- `transcribe_audio(wav_path, whisper_model, language) -> String` — signature unchanged for the frontend; implementation delegates to `transcription::transcribe_wav`. Shrinks from ~175 lines to ~30.
- `list_available_models() -> Vec<ModelInfo>` — NEW. `ModelInfo = { size: String, downloaded: bool, size_bytes: u64 }`. Reads `app_data_dir()/models/`.
- `download_model(size: String) -> Result<()>` — NEW. Streams download with progress events.
- `cancel_model_download(size: String) -> Result<()>` — NEW. Aborts an in-flight download.
- `extract_audio`, `format_transcript`, `save_text_file`, `save_binary_file`, `read_file_as_text`, `cleanup_temp`, `list_ollama_models` — unchanged.

`lib.rs::run()` updates the `invoke_handler!` macro to register the three new commands and drop none.

### Events emitted to the frontend

| Event                       | Payload                                                  | Origin                              |
|-----------------------------|----------------------------------------------------------|-------------------------------------|
| `transcribe-progress`       | `{ percent: u32, message: String }`                      | whisper-rs progress callback         |
| `model-warming`             | `{ size: String }`                                       | engine, before CoreML first-compile  |
| `model-download-progress`   | `{ size: String, bytes_downloaded: u64, bytes_total: u64 }` | download stream loop              |

The existing `transcribe-progress` shape is preserved for backwards compatibility with `App.tsx`'s listener.

## Frontend behavior

### `AppSettings` (src/App.tsx)

Unchanged. `whisperModel: string` stays, default stays `"base"`.

### `SettingsStep.tsx`

1. **Dependency badges:** drop the `whisper` badge. The row becomes `ffmpeg | ollama`. Remove the `Whisper not found` install hint block.

2. **Model picker becomes data-driven:** on mount, call `list_available_models()` and render the dropdown from its result. Each option's label includes download state, e.g.:
   - `base — bundled ✓`
   - `medium — downloaded ✓`
   - `large — 4.6 GB (not downloaded)`

   The `size_bytes` shown for not-downloaded models comes from a hardcoded constant table in `transcription/models.rs` (whisper.cpp model sizes are stable and well-known — `tiny: 78 MB`, `base: 148 MB`, `small: 488 MB`, `medium: 1.5 GB`, `large: 3.1 GB`, plus matching CoreML encoder sizes). For downloaded models, `size_bytes` reflects actual file size on disk.

3. **Selection flow:**
   - If user selects an already-downloaded size: commit immediately via `set("whisperModel", newSize)`.
   - If user selects a not-downloaded size: open `<DownloadModelModal />`. The dropdown's visible value reverts to the previous selection until the modal resolves.
   - On modal **Confirm**: start download, show progress in modal. On completion, close modal and commit `set("whisperModel", newSize)`.
   - On modal **Cancel**: invoke `cancel_model_download` if a download was in flight, close modal, dropdown stays at the previous value.

### New: `src/components/DownloadModelModal.tsx`

Standard centered modal, ~80 lines. Props: `{ size: string, sizeBytes: number, onConfirm: () => void, onCancel: () => void, onComplete: () => void }`.

Internal state: `idle | downloading | error`. While `downloading`, listens for `model-download-progress` events filtered by `size` and renders a progress bar plus byte count. Cancel button always visible. Modal copy:

> "Download {size} model? This is a one-time download of {N.N GB}. The bundled base model will continue to work in the meantime."

### `ProcessingStep.tsx`

Listens for the new `model-warming` event. While set, shows "Optimizing model for Neural Engine, ~30s, one-time…" above the existing progress bar. Clears it when the first `transcribe-progress` event arrives. The existing `transcribe-progress` listener in `App.tsx` is unchanged.

### `App.tsx`

No changes beyond what flows from the above. The `runPipeline` call to `transcribe_audio` keeps the same arguments.

## Build and packaging

### `build.rs`

Add a step that invokes `scripts/fetch-base-model.sh`. The script:
- Skips if both `ggml-base.bin` and `ggml-base-encoder.mlmodelc/` already exist under `src-tauri/resources/models/`
- Otherwise downloads them from HuggingFace, unzips the encoder, and verifies file sizes match expected ranges

Build fails loudly if the script fails (network error, etc.) so we never produce a broken .app.

### `tauri.conf.json`

Add the `bundle.resources` entries listed above. No other changes.

### `.gitignore`

If the project is git-initialized at implementation time, add:
```
src-tauri/resources/models/ggml-*.bin
src-tauri/resources/models/ggml-*.mlmodelc/
```

If the project is not yet a git repo, the implementation plan must either (a) skip this step, or (b) explicitly run `git init` as a prerequisite. This is the user's call; the plan should pause to ask.

### `.claude/settings.local.json`

Remove the two allowlisted bash entries that invoke `whisper`. They become dead.

## Documentation updates

### `SETUP.md`

- **Delete** Section 5 (`Whisper — installed as a uv tool`) entirely.
- **Update** the intro paragraph: replace "transcribes it with Whisper, then uses a local Ollama model" with "transcribes it on-device with whisper.cpp (Metal + Apple Neural Engine), then uses a local Ollama model".
- **Update** the pipeline description (currently lines 96–97): "Transcribe via whisper.cpp (on-device, Metal + Apple Neural Engine)".
- **Replace** the "Whisper model sizes" tip: "The `base` model ships with the app and works well for most recordings. Larger models can be downloaded in Settings."
- **Delete** the "Updating Whisper" line.
- **Add** under the build/run section: "First-time `cargo build` downloads ~220 MB of model weights into `src-tauri/resources/models/`."

### `README.md`

Verify against the same wording changes if it mentions Whisper.

## Error handling

| Failure                                                | Surfaced as                                                                 |
|--------------------------------------------------------|----------------------------------------------------------------------------|
| `whisper-rs` panics or returns error during transcribe | Error string back through the Tauri command → `App.tsx` catch → ProcessingStep error UI |
| Bundled model copy fails on first launch                | Error returned from first `list_available_models()` call → Settings shows blocking error message |
| Model download fails mid-stream                         | `download_model` returns Err → modal switches to `error` state with retry/cancel buttons |
| User cancels download                                   | `cancel_model_download` cleans `.tmp/`, modal closes silently               |
| CoreML compile takes longer than expected               | UI keeps showing "Optimizing for Neural Engine…" — no timeout. User can close the app if needed. |
| WAV file is unreadable / wrong format                   | hound returns Err → propagated as transcription error                       |

## Testing strategy

- **Unit tests** in `transcription/models.rs`: model availability detection, path resolution, .tmp/ cleanup logic. Use `tempfile::TempDir` to fake `app_data_dir()`.
- **Integration test** for the engine: load the bundled `base` model, transcribe a short fixture WAV (~5s of speech committed under `src-tauri/tests/fixtures/`), assert the result contains expected words. Skipped in CI by default if the model isn't downloaded; runs locally after `cargo build`.
- **Manual QA checklist** (in the implementation plan, not this spec):
  - Fresh install → first transcription with bundled base
  - Switch to a non-bundled size → modal appears → confirm → download succeeds → transcription works
  - Switch to a non-bundled size → cancel mid-download → no orphaned files in `app_data_dir()/models/`
  - Restart app after download → picker shows downloaded model as available
  - Pull network during download → error surfaces in modal

## Risks and known caveats

1. **Build time:** `whisper-rs` compiles whisper.cpp from source. Adds 30–60s to a clean `cargo build`. Mitigated by Cargo's incremental compilation on subsequent builds.
2. **Bundle size:** ~225 MB total .app size (vs. a few MB today). Affects notarization upload time once. No runtime impact.
3. **CoreML first-run compile:** unpredictable duration (30s–2min depending on hardware). The `model-warming` event makes it visible to the user, but it remains a real wait. Happens once per model file, ever.
4. **Repository becomes git-tracked:** the project is currently not a git repo. Adding `.gitignore` entries assumes `git init` has been run. The implementation plan should explicitly call this out as a prereq step or skip the `.gitignore` entries if the user prefers to stay non-git.
5. **No fallback:** removing `openai-whisper` entirely means a whisper-rs bug becomes a hard failure. Risk is low (whisper-rs wraps the same C++ code as the CLI), but worth noting.

## Future work (deliberately deferred)

- "Manage models" UI for deleting downloaded models
- Quantized model variants (Q5_0, Q8_0) for users with limited disk
- English-only variants (`.en`) for ~30% additional speedup when language is known
- Resumable downloads (HTTP Range requests) for users on flaky connections
- Auto-update of the bundled model when a new whisper.cpp release ships
