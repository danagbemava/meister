#!/usr/bin/env bash
# Downloads the bundled base whisper.cpp model + its CoreML encoder.
# Idempotent: skips files that already exist with non-trivial size.
# The encoder zip is downloaded and extracted in a temp directory so the
# Tauri dev watcher never sees an intermediate zip file appear inside the
# watched src-tauri/ tree (which would loop the dev rebuild).

set -euo pipefail

cd "$(dirname "$0")/../resources/models"

BIN_URL="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin"
ENC_URL="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base-encoder.mlmodelc.zip"

# 100 MB minimum sanity check (real file is ~145 MB)
MIN_BIN_BYTES=$((100 * 1024 * 1024))

# Clean up any partial zip from a previously-killed run. The watcher would
# otherwise re-trigger on the stale zip and loop again.
rm -f ggml-base-encoder.mlmodelc.zip

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

if [[ -d ggml-base-encoder.mlmodelc ]]; then
  echo "ggml-base-encoder.mlmodelc already present; skipping."
else
  echo "Fetching ggml-base-encoder.mlmodelc.zip (~75 MB) into temp dir…"
  staging=$(mktemp -d)
  trap 'rm -rf "$staging"' EXIT
  curl -L --fail --progress-bar -o "$staging/encoder.zip" "$ENC_URL"
  echo "Unzipping…"
  unzip -q "$staging/encoder.zip" -d "$staging/extracted"
  mv "$staging/extracted/ggml-base-encoder.mlmodelc" .
fi

echo "Bundled base model ready under $(pwd)"
