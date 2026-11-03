import { useEffect, useRef } from "react";
import { cn } from "../lib/cn";

interface Props {
  /** Normalized [0,1] amplitude samples, oldest → newest. */
  samples: Float32Array;
  state: "recording" | "paused";
  className?: string;
}

const BAR_WIDTH = 2;
const BAR_GAP = 1;
const MIN_BAR_HEIGHT = 2;

export default function RecordingWaveform({ samples, state, className }: Props) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    let ctx: CanvasRenderingContext2D | null = null;
    try {
      ctx = canvas.getContext("2d");
    } catch {
      // jsdom doesn't support canvas; skip rendering.
      return;
    }
    if (!ctx) return;

    const { width, height } = canvas.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    if (canvas.width !== Math.round(width * dpr)) canvas.width = Math.round(width * dpr);
    if (canvas.height !== Math.round(height * dpr)) canvas.height = Math.round(height * dpr);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, height);

    const pitch = BAR_WIDTH + BAR_GAP;
    const barCount = Math.max(1, Math.floor(width / pitch));
    const bars = downsampleMax(samples, barCount);

    const css = getComputedStyle(canvas);
    const color = state === "recording"
      ? `rgb(${css.getPropertyValue("--color-accent-rgb").trim() || "128 128 128"})`
      : css.getPropertyValue("--color-label-tertiary").trim() || "#999";

    ctx.fillStyle = color;
    for (let i = 0; i < bars.length; i++) {
      const v = bars[i];
      const h = Math.max(MIN_BAR_HEIGHT, v * height);
      const x = i * pitch;
      const y = (height - h) / 2;
      ctx.fillRect(x, y, BAR_WIDTH, h);
    }
  }, [samples, state]);

  return (
    <canvas
      ref={canvasRef}
      className={cn("block w-full", className ?? "h-full")}
      aria-hidden="true"
    />
  );
}

/** Downsample `src` to exactly `n` buckets by taking the max of each window. */
export function downsampleMax(src: Float32Array, n: number): Float32Array {
  const out = new Float32Array(n);
  if (src.length === 0 || n === 0) return out;
  if (src.length <= n) {
    // Right-align: newest at the right.
    out.set(src, n - src.length);
    return out;
  }
  const step = src.length / n;
  for (let i = 0; i < n; i++) {
    const start = Math.floor(i * step);
    const end = Math.floor((i + 1) * step);
    let m = 0;
    for (let j = start; j < end; j++) {
      if (src[j] > m) m = src[j];
    }
    out[i] = m;
  }
  return out;
}
