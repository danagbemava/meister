# Meister — Setup Guide

Meister is a macOS desktop app that takes a recording (video or audio), extracts its audio, transcribes it with Whisper, then uses a local Ollama model (e.g. Gemma) to format the transcript into structured minutes — in whatever format you supply as a template.

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

### 4. uv (Python package manager)

```bash
brew install uv
```

### 5. Whisper — installed as a uv tool

```bash
uv tool install openai-whisper
```

That's it. `uv tool` manages the isolation internally and puts the `whisper` binary in `~/.local/bin`, which is already on your PATH. No venv to activate, no PATH changes needed.

```bash
# Verify
whisper --help
```

Whisper downloads model weights on first use. The `base` model (~145 MB) is the default and works well for most recordings.

### 6. Ollama + Gemma

```bash
# Install Ollama
brew install ollama

# Pull Gemma (or any other model you prefer)
ollama pull gemma3

# Start Ollama (keep it running in the background)
ollama serve
```

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
   - Transcribe with `whisper`
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

- **Whisper model sizes**: `tiny` is fastest (lower accuracy), `large` is most accurate (slowest). `base` or `small` covers most cases well.
- **Long recordings**: Whisper takes roughly 10–30% of the recording's duration on CPU. `medium`/`large` models need more time.
- **Template quality**: the more explicit your template — section headers, placeholder text, examples — the more closely Gemma will follow it.
- **Updating Whisper**: `uv tool upgrade openai-whisper`
