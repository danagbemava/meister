# Meister — Setup Guide

Meister is a macOS desktop app that takes a recording (video or audio), extracts its audio, transcribes it on-device with whisper.cpp (Metal + Apple Neural Engine), then uses a local Ollama model (e.g. Gemma) to format the transcript into structured minutes — in whatever format you supply as a template.

---

## Prerequisites

### 1. Rust & Tauri CLI

```bash
# Install Rust (if not already)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Tauri CLI
cargo install tauri-cli --version "^2"
```

### 2. Node.js

```bash
brew install node
```

### 3. ffmpeg

```bash
brew install ffmpeg
```

### 4. cmake

```bash
brew install cmake
```

Required by whisper-rs to compile whisper.cpp from source.

### 5. Ollama + Gemma

```bash
# Install Ollama
brew install ollama

# Pull Gemma (or any other model you prefer)
ollama pull gemma3

# Start Ollama (keep it running in the background)
ollama serve
```

---

## Running from Source

> First-time `cargo build` downloads ~220 MB of whisper.cpp model weights into `src-tauri/resources/models/`.

---

## Running in Development

```bash
# In the project directory
npm install
npm run tauri dev
```

The app window will open. The frontend hot-reloads on file changes.

---

## Building a Release App

```bash
npm run tauri build
```

The `.app` bundle will be in `src-tauri/target/release/bundle/macos/`.

---

## How It Works

1. **Drop a recording** — drag a `.mp4`, `.mov`, `.mp3`, `.m4a`, `.wav` (etc.) onto the first panel.
2. **Supply a template** — paste your format template directly, or drop/load a `.md`, `.txt`, or `.docx` file. The template tells the model how to structure the output (headings, action items, attendee lists, etc.).
3. **Configure** — choose your Whisper model size, Ollama model, and which output formats to produce (md, txt, docx, pdf).
4. **Run** — Meister will:
   - Extract audio via `ffmpeg`
   - Transcribe via whisper.cpp (on-device, Metal + Apple Neural Engine)
   - Format via Ollama / Gemma
   - Prompt you to save each requested output file
5. **Results** — preview the formatted output and raw transcript in-app.

---

## Example Format Template

```markdown
# Meeting Minutes

**Date:** [DATE]
**Attendees:** [LIST NAMES]
**Facilitator:** [NAME]

---

## Summary
[2-3 sentence overview]

## Topics Discussed
- 

## Decisions Made
- 

## Action Items
| Owner | Task | Due |
|-------|------|-----|
|       |      |     |

## Next Meeting
[Date / TBD]
```

---

## Tips

- **Transcription model**: the bundled `base` works well for most recordings. Larger models can be downloaded in Settings (one-time download per size).
- **Long recordings**: Transcription takes roughly 10–30% of the recording's duration on CPU. Larger models need more time.
- **Template quality**: the more explicit your template — section headers, placeholder text, examples — the more closely Gemma will follow it.
