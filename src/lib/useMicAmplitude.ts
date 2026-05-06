import { useEffect, useRef, useState } from "react";
import { RingBuffer, computeRms } from "./audioAmplitude";

const RING_CAPACITY = 128;
const TARGET_FPS = 30;
const MIN_INTERVAL_MS = 1000 / TARGET_FPS;

/**
 * Returns a rolling window of normalized [0,1] RMS amplitude samples for the
 * given `MediaStream`. Push frequency is throttled to ~30 fps; the underlying
 * RAF loop runs every animation frame regardless.
 *
 * When `active === false`, the loop continues to read audio (so the
 * AudioContext stays warm) but does NOT push into the ring — this freezes
 * the visible buffer on pause without tearing down the context.
 */
export function useMicAmplitude(
  stream: MediaStream | null,
  active: boolean,
): { samples: Float32Array } {
  const [samples, setSamples] = useState<Float32Array>(() => new Float32Array(RING_CAPACITY));
  const ringRef = useRef<RingBuffer | null>(null);
  const activeRef = useRef(active);
  activeRef.current = active;

  useEffect(() => {
    if (!stream) {
      // Reset the ring so a future stream starts fresh.
      ringRef.current = null;
      setSamples(new Float32Array(RING_CAPACITY));
      return;
    }

    let ctx: AudioContext | null = null;
    let analyser: AnalyserNode | null = null;
    let rafId = 0;
    let buf: Float32Array | null = null;
    let lastPushAt = 0;
    let cancelled = false;

    try {
      ctx = new AudioContext();
      analyser = ctx.createAnalyser();
      analyser.fftSize = 1024;
      const src = ctx.createMediaStreamSource(stream);
      src.connect(analyser);
      buf = new Float32Array(analyser.fftSize);
      ringRef.current = new RingBuffer(RING_CAPACITY);
    } catch (e) {
      // Hook degrades to empty buffer; recording proceeds.
      console.warn("useMicAmplitude: AudioContext init failed", e);
      return;
    }

    const tick = () => {
      if (cancelled || !analyser || !buf || !ringRef.current) return;
      analyser.getFloatTimeDomainData(buf as Float32Array<ArrayBuffer>);
      const now = performance.now();
      if (activeRef.current && now - lastPushAt >= MIN_INTERVAL_MS) {
        ringRef.current.push(computeRms(buf));
        setSamples(ringRef.current.snapshot());
        lastPushAt = now;
      }
      rafId = requestAnimationFrame(tick);
    };
    rafId = requestAnimationFrame(tick);

    return () => {
      cancelled = true;
      cancelAnimationFrame(rafId);
      ctx?.close().catch(() => { /* ignore */ });
    };
  }, [stream]);

  return { samples };
}
