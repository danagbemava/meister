#!/usr/bin/env python3
"""
Descramble a stereo WAV that was produced by Meister's capture pipeline
before the planar-buffer fix (commit 9bf145f).

The pre-fix SCStream audio callback iterated `for buf in list.iter()` over
the two planar AudioBuffers ScreenCaptureKit delivers (one per channel) and
wrote each f32 sample sequentially to a stereo WAV via hound. hound packs
successive `write_sample` calls as stereo frames in L,R,L,R,... order, so
a callback delivering 960 samples per channel produced this pattern in the
WAV (per callback chunk):

  frames 0..479   : (L_{2i},   L_{2i+1})  for i in 0..479   -- pairs of L samples
  frames 480..959 : (R_{2i},   R_{2i+1})  for i in 0..479   -- pairs of R samples

The first half of each chunk packs the original L channel into stereo
frames; the second half packs R. This script reverses that arrangement,
reconstructing the original interleaved (L_n, R_n) layout.

Usage:
  ./descramble.py input.wav output.wav [--chunk N]

Default chunk size is 960 frames (20 ms at 48 kHz — macOS's default audio
quantum). If the recovered audio still sounds off, try 480, 512, or 1024.
"""
import argparse
import sys
import wave

try:
    import numpy as np
except ImportError:
    sys.stderr.write(
        "error: numpy required.\n"
        "  python3 -m venv /tmp/descramble-venv\n"
        "  /tmp/descramble-venv/bin/pip install numpy\n"
        "  /tmp/descramble-venv/bin/python "
        + sys.argv[0]
        + " <args>\n"
    )
    sys.exit(1)


def descramble(input_path: str, output_path: str, chunk_frames: int) -> None:
    with wave.open(input_path, "rb") as src:
        n_channels = src.getnchannels()
        if n_channels != 2:
            raise ValueError(f"expected 2 channels, got {n_channels}")
        sample_width = src.getsampwidth()
        if sample_width != 2:
            raise ValueError(f"expected 16-bit samples, got {sample_width * 8}-bit")
        params = src.getparams()
        n_frames = src.getnframes()
        raw = src.readframes(n_frames)

    if chunk_frames % 2:
        raise ValueError(f"chunk_frames must be even, got {chunk_frames}")
    half = chunk_frames // 2

    data = np.frombuffer(raw, dtype=np.int16).reshape(-1, 2)

    n_callbacks = data.shape[0] // chunk_frames
    if n_callbacks == 0:
        raise ValueError(f"input has fewer than {chunk_frames} frames")

    trimmed = data[: n_callbacks * chunk_frames]
    chunks = trimmed.reshape(n_callbacks, chunk_frames, 2)

    # Per chunk: the first half (480 frames) carries the original L channel
    # interleaved as (L_{2i}, L_{2i+1}). Flattening recovers L_0, L_1, …, L_959.
    new_L = chunks[:, :half, :].reshape(n_callbacks, chunk_frames)
    new_R = chunks[:, half:, :].reshape(n_callbacks, chunk_frames)

    output = np.stack([new_L.flatten(), new_R.flatten()], axis=1).astype(np.int16)

    leftover = data.shape[0] - n_callbacks * chunk_frames
    if leftover:
        ms = leftover / params.framerate * 1000
        print(
            f"note: dropping {leftover} unaligned trailing frames ({ms:.1f} ms)",
            file=sys.stderr,
        )

    with wave.open(output_path, "wb") as dst:
        dst.setparams(params)
        dst.writeframes(output.tobytes())

    duration_s = output.shape[0] / params.framerate
    print(
        f"wrote {output.shape[0]} frames ({duration_s:.1f} s) "
        f"at {params.framerate} Hz stereo",
        file=sys.stderr,
    )


def main():
    ap = argparse.ArgumentParser(description="Descramble a pre-fix Meister capture WAV.")
    ap.add_argument("input", help="input stereo 48 kHz s16le WAV")
    ap.add_argument("output", help="output WAV (same format)")
    ap.add_argument(
        "--chunk",
        type=int,
        default=960,
        help="frames per SCStream callback (default 960 = 20 ms at 48 kHz)",
    )
    args = ap.parse_args()
    descramble(args.input, args.output, args.chunk)


if __name__ == "__main__":
    main()
