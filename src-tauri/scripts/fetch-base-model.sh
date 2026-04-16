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
