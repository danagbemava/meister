import { useEffect, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { RingBuffer } from "./audioAmplitude";

const RING_CAPACITY = 128;

interface AmplitudePayload {
  sessionId: string;
  source: "system" | "mic";
  rms: number;
}

/**
 * Returns a rolling window of normalized [0,1] amplitude samples driven by
 * `call-capture-amplitude` Tauri events filtered to the given `sessionId`.
 * When `sessionId === null`, returns a stable empty buffer.
 */
export function useTauriAmplitude(
  sessionId: string | null,
  source: AmplitudePayload["source"] = "system",
): { samples: Float32Array } {
  const [samples, setSamples] = useState<Float32Array>(() => new Float32Array(RING_CAPACITY));
  const ringRef = useRef<RingBuffer | null>(null);

  useEffect(() => {
    if (!sessionId) {
      ringRef.current = null;
      setSamples(new Float32Array(RING_CAPACITY));
      return;
    }
    ringRef.current = new RingBuffer(RING_CAPACITY);
    let unlisten: UnlistenFn | undefined;
    let cancelled = false;

    listen<AmplitudePayload>("call-capture-amplitude", (event) => {
      if (cancelled) return;
      if (event.payload.sessionId !== sessionId || event.payload.source !== source) return;
      ringRef.current?.push(event.payload.rms);
      if (ringRef.current) setSamples(ringRef.current.snapshot());
    })
      .then((fn) => {
        if (cancelled) {
          fn();
          return;
        }
        unlisten = fn;
      })
      .catch((e) => {
        console.warn("useTauriAmplitude: listen failed", e);
      });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [sessionId, source]);

  return { samples };
}
