# Ollama removal — research notes

Date: 2026-05-05
Status: research, not a decision

## Question

Can Ollama be removed from the meister pipeline?

## Current role of Ollama

Ollama runs the LLM formatting/summarization pass that turns raw Whisper transcripts into formatted output. It is invoked via HTTP (`/api/generate`, streaming) from the Rust backend. Key call sites:

- `src-tauri/src/commands.rs:32` — `check_dependencies` pings `/api/tags`
- `src-tauri/src/commands.rs:52` — `list_ollama_models` populates the model picker
- `src-tauri/src/commands.rs:462` — `ollama_generate_stream` streams tokens
- `src-tauri/src/commands.rs:589` — map pass
- `src-tauri/src/commands.rs:662` / `:726` — reduce / map-reduce orchestration
- `src-tauri/src/commands.rs:823` — single-shot template formatting
- `src-tauri/src/orchestrator.rs:261` — formatter invocation in the recording pipeline

A hard-coded inactivity timeout at `commands.rs:500` exists specifically to detect wedged Ollama servers (model evicted, server crashed, stream stalled).

## Removal options

1. **Drop formatting entirely** — ship raw Whisper transcripts only. Largest UX loss.
2. **Cloud LLM** (Anthropic / OpenAI) — easiest to integrate, but breaks the local-first stance and adds API key management + per-call cost.
3. **Embedded `llama-cpp-rs`** — closest analogue to the whisper.cpp swap already in the codebase. Ollama is a llama.cpp wrapper, so this is a re-host of the same engine in-process.

## Tradeoffs of `llama-cpp-rs`

### Bundle size

- `libllama` linked statically: ~10–20 MB, plus Metal shaders on macOS.
- App binary growth: ~15–30 MB.
- Model weights dominate: 7B Q4 GGUF ~4 GB, 8B Q4 ~5 GB. Bundle vs first-run download — same pattern whisper.cpp already uses.

### Performance

- Essentially identical to Ollama. Same Metal/CUDA backends, same GGUF, same quantizations.
- Wins: no HTTP overhead (negligible vs generation time), no separate-process wedging class — the `commands.rs:500` inactivity timeout becomes unnecessary.
- Costs: app now owns model load/unload, KV cache lifecycle, parallel-request handling. All of this is currently free via Ollama.

### Accuracy

- Identical for the same model + quantization. Same inference code path.

## Non-perf tradeoffs (the actual decision drivers)

- **Build complexity**: CMake + platform backends (similar to whisper-rs, which already requires cmake per SETUP).
- **Model management UX**: lose Ollama's "user already has N models, let them pick" flow at `commands.rs:52`. Either ship one model take-it-or-leave-it, or build a model picker / downloader.
- **Ownership**: download progress, version pinning, eviction, disk-space UX all become app concerns.

## Affected files if we proceed

Beyond the call sites above, references also live in:

- `src-tauri/src/storage/types.rs`, `src-tauri/tests/storage_integration.rs` — `ollama_url` field in settings
- `src/types/storage.ts`, `src/components/SettingsForm.tsx`, `src/components/Sidebar.tsx` — frontend settings + dep status UI
- `SETUP.md` — install instructions
- Multiple plans/specs under `docs/superpowers/` reference Ollama as the formatter

## Open questions

- Which model(s) do we want to support? (affects bundle/download size + perceived accuracy)
- Do we keep a model picker, or commit to a single curated default?
- How do we surface first-run model download progress in the existing onboarding flow?
