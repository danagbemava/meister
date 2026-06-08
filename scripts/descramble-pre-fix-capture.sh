#!/usr/bin/env bash
# Recover audio from a Meister .webm capture made before the planar-buffer
# fix (commit 9bf145f). Decodes the webm to PCM, walks it in 960-sample
# chunks via descramble.py, and writes a descrambled WAV next to the input.
#
# The pre-fix bug: ScreenCaptureKit delivered audio in two non-interleaved
# (planar) buffers — one per channel — but the audio callback wrote each
# buffer's samples sequentially to a stereo WAV as if interleaved. The
# resulting file had remote speakers shifted up an octave with weird
# stereo artifacts. The fix (committed) lazily reads the actual format
# from each sample buffer and interleaves planar layouts correctly.
#
# Existing recordings are still salvageable in post: the buggy interleaving
# is deterministic and per-callback, so walking the WAV in 960-frame chunks
# (= 20 ms at 48 kHz, macOS's default audio quantum) and reversing the
# packing recovers the original channels.
#
# Caveat: the mic audio in your capture was already correctly formatted
# pre-fix. Descrambling re-scrambles it (your own voice will sound
# chipmunk-pitched in the output). For meetings dominated by remote
# speakers, this is the right trade-off — it unlocks transcription of the
# dominant signal at the cost of garbling your own voice (which you
# remember anyway).
#
# Usage:
#   scripts/descramble-pre-fix-capture.sh <input.webm> [chunk_frames]
#
# Default chunk = 960. If remote speakers still sound wrong, try 480, 512,
# or 1024 — those are the other plausible macOS audio quanta.
#
# Output: <input>-descrambled.wav next to the input. Drop that into Meister
# as a new source and re-transcribe.

set -euo pipefail

if [[ $# -lt 1 || $# -gt 2 ]]; then
  echo "usage: $0 <input.webm> [chunk_frames=960]" >&2
  exit 1
fi

INPUT="$1"
CHUNK="${2:-960}"

if [[ ! -f "$INPUT" ]]; then
  echo "error: input not found: $INPUT" >&2
  exit 1
fi

for cmd in ffmpeg python3; do
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "error: $cmd required on PATH" >&2
    exit 1
  fi
done

if ! python3 -c 'import numpy' 2>/dev/null; then
  cat >&2 <<'EOM'
error: numpy required.

On macOS Sequoia+ the system python3 rejects `pip3 install` (PEP 668).
Quick fix — create a one-off venv and re-run with it:

    python3 -m venv /tmp/descramble-venv
    /tmp/descramble-venv/bin/pip install numpy
    PATH=/tmp/descramble-venv/bin:$PATH "$0" "$@"

Or use pipx / homebrew python / a project venv if you have one.
EOM
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DESCRAMBLER="$SCRIPT_DIR/descramble.py"

if [[ ! -f "$DESCRAMBLER" ]]; then
  echo "error: descrambler script missing at $DESCRAMBLER" >&2
  exit 1
fi

stem="${INPUT%.*}"
temp_decoded="${stem}-pre-descramble.wav"
temp_descrambled="${stem}-descrambled-48k-stereo.wav"
output="${stem}-descrambled-16k-mono.wav"

cleanup() {
  rm -f "$temp_decoded"
  if [[ "${KEEP_STEREO:-0}" != "1" ]]; then
    rm -f "$temp_descrambled"
  fi
}
trap cleanup EXIT

orig_duration=$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$INPUT")
echo "→ input duration: ${orig_duration}s"

echo "→ stage 1/3: decoding $INPUT to 48 kHz stereo s16le WAV..."
ffmpeg -hide_banner -loglevel error \
  -i "$INPUT" \
  -ar 48000 \
  -ac 2 \
  -c:a pcm_s16le \
  -y \
  "$temp_decoded"

echo "→ stage 2/3: descrambling at $CHUNK frames per chunk..."
python3 "$DESCRAMBLER" "$temp_decoded" "$temp_descrambled" --chunk "$CHUNK"

# Stage 3 — clean up + shrink:
#   - lowpass=4000  : strips most of the re-scrambled mic noise (it lives
#                     above ~4 kHz because the original mic content was
#                     correctly formatted pre-fix and gets pitch-doubled
#                     by the descrambling operation). Speech intelligibility
#                     is preserved (speech fundamentals + first formants
#                     are below 4 kHz). Whisper resamples to 16 kHz
#                     internally so it doesn't use anything above 8 kHz anyway.
#   - ac=1, ar=16000: mono 16 kHz — what whisper actually consumes. Drops
#                     the file size from ~1.6 GB to ~265 MB for a 138-min
#                     recording, and Meister's extract step becomes ~no-op.
echo "→ stage 3/3: lowpass + mono + 16 kHz (drops noise floor and file size)..."
ffmpeg -hide_banner -loglevel error \
  -i "$temp_descrambled" \
  -af "lowpass=f=4000" \
  -ar 16000 \
  -ac 1 \
  -c:a pcm_s16le \
  -y \
  "$output"

echo
echo "Done. Output: $output"
ls -lh "$output" | awk '{print "  size:", $5}'
echo
echo "Drop $output into Meister as a new source and re-transcribe."
echo
echo "If you want to listen at full quality (no lowpass, stereo, 48 kHz),"
echo "set KEEP_STEREO=1 and re-run — the descrambled-48k-stereo.wav will"
echo "be preserved next to the input."
echo
echo "If remote speakers still sound wrong even after this, the chunk size"
echo "may not be 960. Try:"
echo "  $0 \"$INPUT\" 480"
echo "  $0 \"$INPUT\" 512"
echo "  $0 \"$INPUT\" 1024"
